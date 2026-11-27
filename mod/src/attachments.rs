//! NeoForge data attachments on players, bridged onto the game's attachment store
//! (`docs/attachments-design.md`, slice 1; the runtime side is `runtime::neoforge_attachments`).
//!
//! The runtime keeps each jar's Java attachment types and the live Java objects, and runs the
//! mod's own codec. The game's store is authoritative for whether a player has a value and for
//! the bytes it saves. Around each batch of Java calls in a hook that has a `ServerHost`:
//!
//! 1. [`open`] reads every online player's entry for each of the jar's registered types into a
//!    snapshot the runtime answers holder calls from: absent, one of this jar's values (the
//!    [`JavaAttachment`] marker this bridge stored, by token), or a saved compound not decoded yet;
//! 2. the listeners (or the command) run;
//! 3. [`close`] lets the runtime re-encode every live value (R1, departure D4), then applies the
//!    runtime's journal of `store`/`discard` calls to the game's store.
//!
//! The marker stored as the value is an `Arc<JavaAttachment>`: the Java object itself never leaves
//! the runtime. Values are stored with `sync: false` (D5) and the per-type compound the codec
//! wrote as `encoded`, which the game saves under `horizon:attachments.<ns>.<path>.value` (D1).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use mod_api::{
    AttachmentCodec, AttachmentError, AttachmentForm, AttachmentPolicy, AttachmentStore,
    AttachmentType, AttachmentWrite, Compound, Holder, OriginalPlayer, ServerHost, Tag,
};

use crate::runtime::neoforge_attachments::{StoreEntry, StoreWrite, StoreWriteKind};
use crate::runtime::{ExecutionBudget, NativeJar, Nbt, NbtCompound, StdHost};
use crate::{JAR_BUDGET, LOG};

/// The value this bridge stores in the game's store for one Java attachment value: which jar's
/// runtime holds the object, and the token it was stored with. A later batch whose snapshot finds
/// the same token knows the runtime's object is current; anything else (a respawn copy, a
/// restart's saved compound) is read from the store instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JavaAttachment {
    pub(crate) jar: String,
    pub(crate) token: u64,
}

/// A lifecycle step the runtime must take inside a batch, before the listeners run.
#[derive(Clone, Copy)]
pub(crate) enum Lifecycle<'a> {
    None,
    /// A player joined: saved values decode now (D3) and the initial self-sync is counted.
    Joined(u128),
    /// A player's data was copied to their new player; `true` for a death. The old player the
    /// game lends for the hook is read into the batch before the copy step (`getOriginal`).
    Cloned(u128, bool, &'a dyn OriginalEntries),
    /// A player respawned or changed dimension: NeoForge re-syncs the player to itself.
    Resynced(u128),
    /// A player left: the runtime's objects for them are dropped after the batch.
    Left(u128),
}

/// The game's `namespace`/`name` of a registered id.
fn split_id(id: &str) -> (&str, &str) {
    id.split_once(':').unwrap_or(("minecraft", id))
}

/// The game's compound as the runtime's tree. A string that is not valid UTF-16 (a Java string
/// with an unpaired surrogate) or an `End` tag cannot be represented exactly, so it is refused
/// rather than replaced.
pub(crate) fn to_runtime(compound: &Compound) -> Result<NbtCompound, String> {
    compound
        .iter()
        .map(|(key, tag)| {
            let key = String::from_utf16(&key.0).map_err(|_| "a key is not valid UTF-16")?;
            Ok((key, tag_to_runtime(tag, 0)?))
        })
        .collect()
}

pub(crate) fn tag_to_runtime(tag: &Tag, depth: usize) -> Result<Nbt, String> {
    if depth > crate::runtime::NBT_MAX_DEPTH {
        return Err("NBT nested too deep".into());
    }
    Ok(match tag {
        Tag::End => return Err("an End tag".into()),
        Tag::Byte(v) => Nbt::Byte(*v),
        Tag::Short(v) => Nbt::Short(*v),
        Tag::Int(v) => Nbt::Int(*v),
        Tag::Long(v) => Nbt::Long(*v),
        Tag::Float(bits) => Nbt::Float(*bits),
        Tag::Double(bits) => Nbt::Double(*bits),
        Tag::ByteArray(v) => Nbt::ByteArray(v.clone()),
        Tag::String(v) => {
            Nbt::String(String::from_utf16(&v.0).map_err(|_| "a string is not valid UTF-16")?)
        }
        Tag::List(elements) => Nbt::List(
            elements
                .iter()
                .map(|element| tag_to_runtime(element, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Tag::Compound(entries) => Nbt::Compound(
            entries
                .iter()
                .map(|(key, tag)| {
                    let key =
                        String::from_utf16(&key.0).map_err(|_| "a key is not valid UTF-16")?;
                    Ok((key, tag_to_runtime(tag, depth + 1)?))
                })
                .collect::<Result<_, String>>()?,
        ),
        Tag::IntArray(v) => Nbt::IntArray(v.clone()),
        Tag::LongArray(v) => Nbt::LongArray(v.clone()),
    })
}

/// The runtime's tree as the game's compound, tag for tag.
pub(crate) fn to_game(compound: &NbtCompound) -> Compound {
    compound
        .iter()
        .map(|(key, tag)| (key.as_str().into(), tag_to_game(tag)))
        .collect()
}

pub(crate) fn tag_to_game(tag: &Nbt) -> Tag {
    match tag {
        Nbt::Byte(v) => Tag::Byte(*v),
        Nbt::Short(v) => Tag::Short(*v),
        Nbt::Int(v) => Tag::Int(*v),
        Nbt::Long(v) => Tag::Long(*v),
        Nbt::Float(bits) => Tag::Float(*bits),
        Nbt::Double(bits) => Tag::Double(*bits),
        Nbt::ByteArray(v) => Tag::ByteArray(v.clone()),
        Nbt::String(v) => Tag::String(v.as_str().into()),
        Nbt::List(elements) => Tag::List(elements.iter().map(tag_to_game).collect()),
        Nbt::Compound(entries) => Tag::Compound(to_game(entries)),
        Nbt::IntArray(v) => Tag::IntArray(v.clone()),
        Nbt::LongArray(v) => Tag::LongArray(v.clone()),
    }
}

/// Each player's entries for `ids`, read from the game's store. A player whose store cannot be
/// read (`MissingHolder`, `HolderBusy`) is an `Err` naming the store's answer, so the runtime
/// refuses their holder calls by name.
pub(crate) fn snapshot(
    store: &mut dyn AttachmentStore,
    jar: &str,
    ids: &[String],
    players: &[u128],
) -> BTreeMap<u128, Result<BTreeMap<String, StoreEntry>, String>> {
    players
        .iter()
        .map(|&uuid| (uuid, holder_entries(store, jar, ids, Holder::Player(uuid))))
        .collect()
}

/// One holder's entries for `ids`, read from the game's store, or why the store could not answer.
fn holder_entries(
    store: &mut dyn AttachmentStore,
    jar: &str,
    ids: &[String],
    holder: Holder,
) -> Result<BTreeMap<String, StoreEntry>, String> {
    ids.iter()
        .map(|id| {
            let (namespace, name) = split_id(id);
            let entry = match store.decoded(holder, namespace, name) {
                Err(error) => return Err(format!("the game's store answered {error:?}")),
                Ok(Some(value)) => match value.downcast_ref::<JavaAttachment>() {
                    Some(ours) if ours.jar == jar => StoreEntry::Ours(ours.token),
                    _ => StoreEntry::Foreign,
                },
                Ok(None) => match store.saved(holder, namespace, name) {
                    Ok(Some(saved)) => match to_runtime(&saved) {
                        Ok(saved) => StoreEntry::Raw(saved),
                        Err(why) => StoreEntry::Unreadable(why),
                    },
                    Ok(None) => StoreEntry::Absent,
                    Err(error) => StoreEntry::Unreadable(format!(
                        "the saved entry could not be read: {error:?}"
                    )),
                },
            };
            Ok((id.clone(), entry))
        })
        .collect()
}

/// The old player `player_clone` lends (`PlayerEvent.Clone.getOriginal`), read by each jar's batch
/// as it opens. `Lifecycle` stays `Copy` by holding this as a shared reference; the loan itself is
/// a `RefCell` because the old player's reads take `&mut`.
pub(crate) trait OriginalEntries {
    /// The old player's entries for `ids`, as [`holder_entries`] reads a live player's.
    fn entries(&self, jar: &str, ids: &[String]) -> Result<BTreeMap<String, StoreEntry>, String>;
}

impl OriginalEntries for RefCell<&mut OriginalPlayer<'_>> {
    fn entries(&self, jar: &str, ids: &[String]) -> Result<BTreeMap<String, StoreEntry>, String> {
        original_entries(&mut self.borrow_mut(), jar, ids)
    }
}

/// The old player's entries for `ids`: the same `Ours`/`Foreign`/`Raw`/`Absent` answers
/// [`holder_entries`] gives for a live player, from the old player's own store.
///
/// Since d404b5f02f the game lends that store only through [`OriginalPlayer`]'s typed
/// `AttachmentType<T>` reads (`Holder::OriginalPlayer` on the server's store is gone), so each id
/// is read with two keys of this bridge's value type, neither of which writes anything:
///
/// 1. `with_decoded_attachment` borrows a decoded value. A [`JavaAttachment`] is ours or another
///    jar's, as before. An error is the store's answer for the whole holder, as before.
/// 2. Otherwise `existing_attachment` with a probe codec: a value decoded as another Rust type
///    answers `TypeMismatch` (another mod's value: `Foreign`); a saved compound reaches the
///    probe's `decode`, which copies it out and fails, so nothing is decoded or cached (`Raw`);
///    nothing at all is `Ok(None)` (`Absent`). Any other error is the saved read's
///    (`Unreadable`), as `store.saved` failing was before.
fn original_entries(
    original: &mut OriginalPlayer<'_>,
    jar: &str,
    ids: &[String],
) -> Result<BTreeMap<String, StoreEntry>, String> {
    ids.iter()
        .map(|id| {
            let (namespace, name) = split_id(id);
            let (namespace, name) = (interned(namespace), interned(name));
            let decoded = AttachmentType::<JavaAttachment> {
                namespace,
                name,
                default: unread_default,
                codec: None,
                copy_on_death: false,
                sync: false,
            };
            let ours = |value: &JavaAttachment| {
                if value.jar == jar {
                    StoreEntry::Ours(value.token)
                } else {
                    StoreEntry::Foreign
                }
            };
            match original.with_decoded_attachment(&decoded, |value| value.map(ours)) {
                Err(error) => return Err(format!("the game's store answered {error:?}")),
                Ok(Some(entry)) => return Ok((id.clone(), entry)),
                Ok(None) => {}
            }
            let probe = AttachmentType::<JavaAttachment> {
                namespace,
                name,
                default: unread_default,
                codec: Some(AttachmentCodec {
                    encode: probe_encode,
                    decode: probe_decode,
                }),
                copy_on_death: false,
                sync: false,
            };
            SAVED_PROBE.with(|slot| slot.borrow_mut().take());
            let answer = original.existing_attachment(&probe);
            let saved = SAVED_PROBE.with(|slot| slot.borrow_mut().take());
            let entry = match (saved, answer) {
                (Some(saved), _) => match to_runtime(&saved) {
                    Ok(saved) => StoreEntry::Raw(saved),
                    Err(why) => StoreEntry::Unreadable(why),
                },
                (None, Ok(None)) => StoreEntry::Absent,
                // A decoded `JavaAttachment` step 1 already answered; kept for completeness.
                (None, Ok(Some(value))) => ours(&value),
                (None, Err(AttachmentError::TypeMismatch)) => StoreEntry::Foreign,
                (None, Err(error)) => {
                    StoreEntry::Unreadable(format!("the saved entry could not be read: {error:?}"))
                }
            };
            Ok((id.clone(), entry))
        })
        .collect()
}

thread_local! {
    /// The saved compound [`probe_decode`] was handed, for [`original_entries`] on this thread.
    static SAVED_PROBE: RefCell<Option<Compound>> = const { RefCell::new(None) };
}

/// The probe key's decoder: copies the saved compound out and refuses, so the store keeps the
/// entry saved (`AttachmentType::get_existing` stores only a value that decoded).
fn probe_decode(saved: &Compound) -> Result<JavaAttachment, AttachmentError> {
    SAVED_PROBE.with(|slot| *slot.borrow_mut() = Some(saved.clone()));
    Err(AttachmentError::Decode)
}

/// The probe key's encoder. Only a store calls it, and the probe never stores.
fn probe_encode(_: &JavaAttachment) -> Result<Compound, AttachmentError> {
    Err(AttachmentError::Encode)
}

/// The reading keys' default. Neither read inserts a default, so nothing ever sees it.
fn unread_default() -> JavaAttachment {
    JavaAttachment {
        jar: String::new(),
        token: 0,
    }
}

/// `AttachmentType` names a key with `&'static str`; a jar's attachment ids are known only at
/// run time. Each distinct namespace and path is leaked once and reused, so the leak is bounded
/// by the attachment types the loaded jars registered.
fn interned(text: &str) -> &'static str {
    static NAMES: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut names = NAMES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(name) = names.get(text) {
        return name;
    }
    let name: &'static str = Box::leak(text.to_owned().into_boxed_str());
    names.insert(name);
    name
}

/// Applies the runtime's journal to the game's store, in order, and answers a line for each
/// write the store refused.
pub(crate) fn apply(
    store: &mut dyn AttachmentStore,
    jar: &str,
    writes: Vec<StoreWrite>,
) -> Vec<String> {
    let mut refused = Vec::new();
    for write in writes {
        let (player, id, result) = match write {
            StoreWrite::Store {
                player,
                id,
                token,
                encoded,
                copy_on_death,
                write,
            } => {
                let (namespace, name) = split_id(&id);
                let form = AttachmentForm {
                    encoded: encoded.as_ref().map(to_game),
                    // Never `copy_on_death` without bytes: the game's death copy would fail as a
                    // whole (`InvalidCopyPolicy`) and keep every entry.
                    policy: AttachmentPolicy {
                        copy_on_death: copy_on_death && encoded.is_some(),
                        sync: false,
                    },
                    write: match write {
                        StoreWriteKind::Decoded => AttachmentWrite::Decoded,
                        StoreWriteKind::Default => AttachmentWrite::Default,
                        StoreWriteKind::Set => AttachmentWrite::Set,
                    },
                };
                let value = Arc::new(JavaAttachment {
                    jar: jar.to_owned(),
                    token,
                });
                let result = store.store(Holder::Player(player), namespace, name, value, form);
                (player, id, result)
            }
            StoreWrite::Discard { player, id } => {
                let (namespace, name) = split_id(&id);
                let result = store.discard(Holder::Player(player), namespace, name, false);
                (player, id, result)
            }
        };
        if let Err(error) = result {
            refused.push(format!("{id} on player {player:032x}: {error:?}"));
        }
    }
    refused
}

/// Opens one jar's batch if it has types the bridge runs, and takes the lifecycle step. Answers
/// the jar's unknown-call count before the batch, for [`close`]; `None` when no batch opened.
pub(crate) fn open(
    server: &mut dyn ServerHost,
    id: &str,
    jar: &NativeJar,
    host: &mut StdHost,
    lifecycle: Lifecycle<'_>,
) -> Option<usize> {
    let ids = host.attachment_store_ids();
    if ids.is_empty() {
        return None;
    }
    let players: Vec<u128> = server
        .players()
        .server_players()
        .into_iter()
        .map(|player| player.uuid())
        .chain(match lifecycle {
            // The leaving player is already off the list; their store may still answer.
            Lifecycle::Left(uuid) => Some(uuid),
            _ => None,
        })
        .collect();
    let view = snapshot(server.attachments(), id, &ids, &players);
    let before = host.unknown_host_call_count();
    host.open_attachment_batch(view);
    let budget = ExecutionBudget::within(JAR_BUDGET);
    match lifecycle {
        Lifecycle::Joined(player) => jar.attachment_player_joined(host, &budget, player),
        Lifecycle::Cloned(player, was_death, original) => {
            host.open_original_attachments(player, original.entries(id, &ids));
            jar.attachment_player_cloned(host, &budget, player, was_death)
        }
        Lifecycle::Resynced(player) => host.count_attachment_initial_sync(player),
        Lifecycle::None | Lifecycle::Left(_) => {}
    }
    Some(before)
}

/// Closes a batch [`open`] opened: R1, then the journal onto the game's store, then the runtime's
/// NeoForge log lines. A player who left is forgotten either way.
pub(crate) fn close(
    server: &mut dyn ServerHost,
    id: &str,
    jar: &NativeJar,
    host: &mut StdHost,
    before: Option<usize>,
    lifecycle: Lifecycle<'_>,
) {
    if let Some(before) = before {
        let batch_unknown = host.unknown_host_call_count().saturating_sub(before);
        jar.attachment_batch_end(host, &ExecutionBudget::within(JAR_BUDGET), batch_unknown);
        let writes = host.close_attachment_batch();
        for refused in apply(server.attachments(), id, writes) {
            log::warn!("{LOG}: {id}: the game's attachment store refused {refused}");
        }
    }
    for line in host.take_attachment_log() {
        log::warn!("{LOG}: {id}: {line}");
    }
    if let Lifecycle::Left(player) = lifecycle {
        host.attachment_player_left(player);
    }
}

/// The bridge's counters for the report row.
pub(crate) fn stats_json(
    stats: &crate::runtime::neoforge_attachments::AttachmentStats,
) -> serde_json::Value {
    serde_json::json!({
        "types": stats.types,
        "refused_types": stats.refused_types,
        "live": stats.live,
        "defaults_created": stats.defaults_created,
        "decoded": stats.decoded,
        "decode_failures": stats.decode_failures,
        "encodes": stats.encodes,
        "re_encode_stores": stats.re_encode_stores,
        "encode_failures": stats.encode_failures,
        "encodes_withheld": stats.encodes_withheld,
        "tainted": stats.tainted,
        "sync_not_sent": stats.sync_not_sent,
        "refusals": stats.refusals,
        "batches": stats.batches,
    })
}

#[cfg(test)]
mod tests {
    //! The bridge against the game's own store (`mod_api::Attachments`, which answers for any
    //! holder): its `to_nbt` envelope, its death copy and its load are the game's code, not a
    //! fake. The Java side is the runtime's fixture mod, carryon-shaped, compiled with `javac`
    //! (`crates/horizon-native-java/tests/fixtures/attachments`), with expected bytes from the JVM
    //! oracle run against NeoForge 26.3.
    use super::*;
    use crate::runtime::neoforge_attachments::SYNC_NOT_SENT;
    use crate::runtime::neoforge_attachments::tests::{
        compound, fixture_jar_bytes, oracle, original_player, player,
    };
    use crate::runtime::{MethodRef, NativeError, Value};
    use mod_api::{AttachmentError, Attachments};
    use std::time::Duration;

    const P: u128 = 0x00ff_00ff_00ff_00ff_00ff_00ff_00ff_00ff;
    const PLAYER: &str = "Lnet/minecraft/world/entity/player/Player;";
    const DATA: i32 = 0;
    const KEPT: i32 = 1;
    const OPAQUE: i32 = 3;

    struct Run {
        jar: NativeJar,
        host: StdHost,
        store: Attachments,
    }

    impl Run {
        fn new() -> Self {
            let jar = NativeJar::from_bytes(fixture_jar_bytes()).unwrap();
            let mut host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
            let budget = ExecutionBudget::within(Duration::from_secs(20));
            jar.invoke_persistent(
                &MethodRef {
                    owner: "demo/Demo".into(),
                    name: "init".into(),
                    descriptor: "(Lnet/neoforged/bus/api/IEventBus;)V".into(),
                },
                None,
                &[Value::Ref(Some("net/neoforged/bus/api/IEventBus".into()))],
                &mut host,
                &budget,
            )
            .unwrap();
            assert!(jar.register_attachment_types(&mut host, &budget).is_empty());
            Self {
                jar,
                host,
                store: Attachments::default(),
            }
        }

        fn call(&mut self, name: &str, which: i32) -> Result<Value, NativeError> {
            let descriptor = match name {
                "has" => format!("({PLAYER}I)Z"),
                _ => format!("({PLAYER}I)Ljava/lang/Object;"),
            };
            self.jar.invoke_persistent(
                &MethodRef {
                    owner: "demo/Demo".into(),
                    name: name.into(),
                    descriptor,
                },
                None,
                &[player(P), Value::Int(which)],
                &mut self.host,
                &ExecutionBudget::within(Duration::from_secs(20)),
            )
        }

        fn select(&mut self, data: &Value, selected: i32) {
            self.jar
                .invoke_persistent(
                    &MethodRef {
                        owner: "demo/Demo".into(),
                        name: "select".into(),
                        descriptor: "(Ljava/lang/Object;I)V".into(),
                    },
                    None,
                    &[data.clone(), Value::Int(selected)],
                    &mut self.host,
                    &ExecutionBudget::within(Duration::from_secs(20)),
                )
                .unwrap();
        }

        /// One batch through the bridge's real snapshot and journal, over the game's store.
        fn batch<R>(&mut self, lifecycle: Lifecycle, body: impl FnOnce(&mut Self) -> R) -> R {
            let ids = self.host.attachment_store_ids();
            let view = snapshot(&mut self.store, "demo", &ids, &[P]);
            let before = self.host.unknown_host_call_count();
            self.host.open_attachment_batch(view);
            let budget = ExecutionBudget::within(Duration::from_secs(20));
            match lifecycle {
                Lifecycle::Joined(p) => {
                    self.jar
                        .attachment_player_joined(&mut self.host, &budget, p)
                }
                Lifecycle::Cloned(p, death, _) => {
                    self.jar
                        .attachment_player_cloned(&mut self.host, &budget, p, death)
                }
                _ => {}
            }
            let result = body(self);
            let unknown = self.host.unknown_host_call_count() - before;
            self.jar
                .attachment_batch_end(&mut self.host, &budget, unknown);
            let writes = self.host.close_attachment_batch();
            assert_eq!(apply(&mut self.store, "demo", writes), Vec::<String>::new());
            result
        }

        /// What the game saves under `horizon:attachments` for `demo:<name>`.
        fn saved(&self, name: &str) -> Option<Compound> {
            match self.store.to_nbt().get(&"demo".into()) {
                Some(Tag::Compound(names)) => match names.get(&name.into()) {
                    Some(Tag::Compound(envelope)) => Some(envelope.clone()),
                    _ => None,
                },
                _ => None,
            }
        }
    }

    fn envelope(value: Compound) -> Compound {
        Compound::from([("value".into(), Tag::Compound(value))])
    }

    /// The oracle's child for an encode case, as the game's compound.
    fn oracle_child(case: &str) -> Compound {
        let saved = compound(&oracle(case)["saved"]);
        match saved.get("carryon:carry_on_data") {
            Some(Nbt::Compound(child)) => to_game(child),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_carryon_shaped_value_is_saved_with_neoforges_bytes_inside_the_games_envelope() {
        let mut run = Run::new();
        run.batch(Lifecycle::None, |run| run.call("get", DATA).unwrap());
        // D1: `horizon:attachments.demo.data`, then `value`: the oracle's child, and no
        // `copy_on_death` or `sync` byte (no copyOnDeath; sync is never sent, D5).
        assert_eq!(
            run.saved("data"),
            Some(envelope(oracle_child("encode/empty")))
        );
        assert_eq!(
            run.host
                .unknown_host_calls()
                .into_iter()
                .find(|(key, _)| key == SYNC_NOT_SENT)
                .map(|(_, count)| count),
            Some(1)
        );
        assert!(run.store.take_sync_changes().is_empty());
        // An in-place change reaches the saved bytes at the end of its batch (R1).
        let value = run.batch(Lifecycle::None, |run| run.call("get", DATA).unwrap());
        run.batch(Lifecycle::None, |run| run.select(&value, 3));
        let expected = Compound::from([(
            "CarryOnData".into(),
            Tag::Compound(Compound::from([("selected".into(), Tag::Int(3))])),
        )]);
        assert_eq!(run.saved("data"), Some(envelope(expected)));
    }

    #[test]
    fn a_restart_reads_the_saved_bytes_back_at_join() {
        let mut run = Run::new();
        let value = run.batch(Lifecycle::None, |run| run.call("get", DATA).unwrap());
        run.batch(Lifecycle::None, |run| run.select(&value, 4));
        // The game's save and load: the map round-trips through NBT, the runtime starts empty.
        run.store = Attachments::from_nbt(run.store.to_nbt());
        run.host.reset_attachment_values();
        let reloaded = run.batch(Lifecycle::Joined(P), |run| run.call("get", DATA).unwrap());
        assert_ne!(reloaded, value);
        assert_eq!(run.host.attachment_stats().decoded, 1);
        assert_eq!(run.host.attachment_stats().defaults_created, 1);
    }

    #[test]
    fn the_games_death_copy_keeps_copy_on_death_types_and_drops_carryons() {
        let mut run = Run::new();
        run.batch(Lifecycle::None, |run| {
            let carried = run.call("get", DATA).unwrap();
            run.select(&carried, 2);
            let kept = run.call("get", KEPT).unwrap();
            run.select(&kept, 5);
        });
        assert_eq!(
            run.saved("kept")
                .and_then(|e| e.get(&"copy_on_death".into()).cloned()),
            Some(Tag::Byte(1))
        );
        // `guest_respawn`: `copy_on_death` then `replace_contents`, before `player_clone`.
        let copy = run.store.copy_on_death().unwrap();
        run.store.replace_contents(copy);
        let (kept, has_data) = run.batch(
            // The harness takes the copy step only; the old player's own reads have their tests.
            Lifecycle::Cloned(P, true, &NoOriginal),
            |run| {
                (
                    run.call("get", KEPT).unwrap(),
                    run.call("has", DATA).unwrap(),
                )
            },
        );
        assert_eq!(has_data, Value::Int(0));
        assert_ne!(kept, Value::Ref(None));
        assert_eq!(run.host.attachment_stats().decoded, 1);
    }

    #[test]
    fn an_unmodelled_serializer_is_refused_and_the_store_never_holds_it() {
        let mut run = Run::new();
        let answer = run.batch(Lifecycle::None, |run| run.call("get", OPAQUE).unwrap());
        assert_eq!(answer, Value::Ref(None));
        assert_eq!(run.saved("opaque"), None);
        assert!(run.store.to_nbt().is_empty());
    }

    #[test]
    fn a_player_the_store_cannot_reach_is_refused_by_name() {
        struct Gone;
        impl mod_api::AttachmentHost for Gone {
            fn validate_attachment(
                &self,
                _: &str,
                _: &str,
                _: bool,
            ) -> Result<(), AttachmentError> {
                Ok(())
            }
            fn attachments(
                &mut self,
                _: Holder,
                _: mod_api::AttachmentAccess,
            ) -> Result<&mut Attachments, AttachmentError> {
                Err(AttachmentError::MissingHolder)
            }
        }
        let view = snapshot(&mut Gone, "demo", &["demo:data".into()], &[P]);
        assert_eq!(
            view.get(&P),
            Some(&Err("the game's store answered MissingHolder".to_owned()))
        );
    }

    /// A `Lifecycle::Cloned` reader for the harness, which never asks it.
    struct NoOriginal;
    impl OriginalEntries for NoOriginal {
        fn entries(&self, _: &str, _: &[String]) -> Result<BTreeMap<String, StoreEntry>, String> {
            Ok(BTreeMap::new())
        }
    }

    /// The old player `player_clone` lends, as the game builds it: only its own attachment store
    /// matters to the bridge.
    struct Lent {
        old: Attachments,
    }
    impl mod_api::OriginalPlayerAccess for Lent {
        fn uuid(&self) -> u128 {
            P
        }
        fn position(&self) -> [f64; 3] {
            [0.0; 3]
        }
        fn dimension(&self) -> &str {
            "minecraft:overworld"
        }
        fn level(&self) -> Option<mod_api::LevelRef> {
            None
        }
        fn health(&self) -> f32 {
            0.0
        }
        fn experience(&self) -> mod_api::PlayerExperience {
            mod_api::__private::built::PlayerExperience {
                level: 0,
                progress: 0.0,
                total: 0,
            }
            .into()
        }
        fn effects(&self) -> &[mod_api::ActiveEffect] {
            &[]
        }
        fn with_inventory(&self, read: &mut dyn for<'a> FnMut(mod_api::InventoryView<'a>)) {
            read(mod_api::InventoryView::new(0..0, &|_| None, &|_, _| None));
        }
        fn persistent_data(&self) -> Option<&Compound> {
            None
        }
        fn attachments(&mut self) -> &mut dyn AttachmentStore {
            &mut self.old
        }
    }

    /// Every answer [`original_entries`] gives, from the old player's own store: this jar's value,
    /// another jar's, another mod's Rust type, a saved compound (which it must leave saved, not
    /// decode or cache), and nothing.
    #[test]
    fn the_original_players_entries_are_read_without_writing() {
        let key = |name: &'static str| AttachmentType::<JavaAttachment> {
            namespace: "demo",
            name,
            default: unread_default,
            codec: None,
            copy_on_death: false,
            sync: false,
        };
        let other_type = AttachmentType::<i32> {
            namespace: "demo",
            name: "other_type",
            default: || 0,
            codec: None,
            copy_on_death: false,
            sync: false,
        };
        let saved = Compound::from([("n".into(), Tag::Int(5))]);
        let mut old = Attachments::from_nbt(Compound::from([(
            "demo".into(),
            Tag::Compound(Compound::from([(
                "raw".into(),
                Tag::Compound(envelope(saved.clone())),
            )])),
        )]));
        let value = |jar: &str, token| JavaAttachment {
            jar: jar.into(),
            token,
        };
        key("ours")
            .set(&mut old, Holder::Level, value("demo", 9))
            .unwrap();
        key("theirs")
            .set(&mut old, Holder::Level, value("other", 4))
            .unwrap();
        other_type.set(&mut old, Holder::Level, 3).unwrap();
        let mut lent = Lent { old };
        let ids: Vec<String> = ["ours", "theirs", "other_type", "raw", "absent"]
            .iter()
            .map(|name| format!("demo:{name}"))
            .collect();
        // The loan ends with this block, as the game's does with the hook.
        let entries = {
            let mut original = mod_api::OriginalPlayer::new(&mut lent);
            let reader = RefCell::new(&mut original);
            OriginalEntries::entries(&reader, "demo", &ids).unwrap()
        };
        assert_eq!(
            entries,
            BTreeMap::from([
                ("demo:ours".to_owned(), StoreEntry::Ours(9)),
                ("demo:theirs".to_owned(), StoreEntry::Foreign),
                ("demo:other_type".to_owned(), StoreEntry::Foreign),
                (
                    "demo:raw".to_owned(),
                    StoreEntry::Raw(to_runtime(&saved).unwrap())
                ),
                ("demo:absent".to_owned(), StoreEntry::Absent),
            ])
        );
        // The saved entry is still only saved: nothing decoded it, cached it or inserted a default.
        assert!(
            AttachmentStore::decoded(&mut lent.old, Holder::Level, "demo", "raw")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            AttachmentStore::saved(&mut lent.old, Holder::Level, "demo", "raw").unwrap(),
            Some(saved)
        );
        assert!(!AttachmentStore::holds(&mut lent.old, Holder::Level, "demo", "absent").unwrap());
    }

    /// `getOriginal().getData(...)` on a death: the game lends the old player's own store
    /// (`OriginalPlayer`), which still holds carryon's non-`copyOnDeath` value as this jar's
    /// object, while the new player's store has only the death copy. The bridge reads the
    /// original's entries from that store, so the old player answers the very object it held and
    /// the new one has nothing.
    #[test]
    fn the_original_player_answers_its_own_pre_death_object() {
        let mut run = Run::new();
        let carried = run.batch(Lifecycle::None, |run| {
            let carried = run.call("get", DATA).unwrap();
            run.select(&carried, 2);
            carried
        });
        // `guest_respawn`: the old map is lent as it was, the new player gets the death copy.
        let old = std::mem::take(&mut run.store);
        let copy = old.copy_on_death().unwrap();
        let mut new = Attachments::default();
        new.replace_contents(copy);
        let mut lent = Lent { old };
        let mut original = mod_api::OriginalPlayer::new(&mut lent);
        let original = RefCell::new(&mut original);

        let ids = run.host.attachment_store_ids();
        let view = snapshot(&mut new, "demo", &ids, &[P]);
        run.host.open_attachment_batch(view);
        // What `open` hands the runtime for `Lifecycle::Cloned`.
        let entries = OriginalEntries::entries(&original, "demo", &ids);
        run.host.open_original_attachments(P, entries);
        let budget = ExecutionBudget::within(Duration::from_secs(20));
        run.jar
            .attachment_player_cloned(&mut run.host, &budget, P, true);
        let on = |run: &mut Run, holder: Value, name: &str| {
            let descriptor = match name {
                "has" => format!("({PLAYER}I)Z"),
                _ => format!("({PLAYER}I)Ljava/lang/Object;"),
            };
            run.jar
                .invoke_persistent(
                    &MethodRef {
                        owner: "demo/Demo".into(),
                        name: name.into(),
                        descriptor,
                    },
                    None,
                    &[holder, Value::Int(DATA)],
                    &mut run.host,
                    &budget,
                )
                .unwrap()
        };
        let from_original = on(&mut run, original_player(P), "get");
        assert_eq!(from_original, carried);
        assert_eq!(on(&mut run, player(P), "has"), Value::Int(0));
        run.host.close_attachment_batch();
    }

    #[test]
    fn every_tag_type_crosses_the_bridge_unchanged() {
        let tree = compound(&oracle("encode/every-type")["input"]);
        assert_eq!(to_runtime(&to_game(&tree)), Ok(tree));
        // A Java string holding a lone surrogate, which no Rust `String` can hold, is refused
        // rather than replaced.
        let Tag::String(mut lone) = Tag::String("x".into()) else {
            unreachable!()
        };
        lone.0 = vec![0xd800];
        let bad = Compound::from([("k".into(), Tag::String(lone))]);
        assert!(to_runtime(&bad).is_err());
    }
}
