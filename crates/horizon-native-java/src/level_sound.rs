//! Minecraft 26.3 `Level.playSound`, `playSeededSound`, `playLocalSound`,
//! `playPlayerSound`, and `Entity.playSound` models.
//!
//! Every overload mirrors the real 26.3 `server-26.3.jar` and client bytecode:
//!
//! - `Level` owns two distinct `RandomSource` fields:
//!   - `random` (`Level.getRandom()`): advanced by `ClientLevel.playLocalSound`
//!     and `ClientLevel.playPlayerSound` (when `minecraft.player != null`), and
//!     NOT advanced by `Level.playSound`.
//!   - `soundSeedGenerator` (`RandomSource.createThreadSafe()`, same 48-bit LCG
//!     as `LegacyRandomSource`): advanced by every non-seeded `Level.playSound`
//!     overload before calling `playSeededSound`, on both `ServerLevel` and
//!     `ClientLevel`.
//! - `playSeededSound` uses the caller-supplied `long seed` and draws neither
//!   random stream.
//! - `BlockPos` overloads convert `(x, y, z)` to `(x + 0.5, y + 0.5, z + 0.5)`
//!   and throw `NullPointerException` on a `null` position before drawing any
//!   random.
//! - `ServerLevel.playSeededSound` maps `except` via
//!   `except instanceof Player p ? p : null` and broadcasts using
//!   `SoundEvent.getRange(volume)`. `ServerLevel.playLocalSound` and
//!   `ServerLevel.playPlayerSound` are no-ops (after reading `BlockPos` for the
//!   `BlockPos` overload) and advance no random stream.
//! - `ClientLevel.playSeededSound` is a no-op unless
//!   `except == minecraft.player` (`if_acmpne`), though calling via
//!   `Level.playSound` still draws `soundSeedGenerator.nextLong()` first.
//!   `ClientLevel.playLocalSound` with `distanceDelay == true` computes delay
//!   ticks as `d2 > 100.0 ? (int)((Math.sqrt(d2) / 40.0) * 20.0) : 0` from the
//!   main camera position.
//! - `Entity.playSound(SoundEvent)` checks `!isSilent()` before calling
//!   `playSound(SoundEvent, 1.0f, 1.0f)` (even on `Player` / `LocalPlayer`,
//!   which inherit the 1-arg overload). `Player.playSound(SoundEvent, float,
//!   float)` and `LocalPlayer.playSound(SoundEvent, float, float)` do not check
//!   `isSilent()`.
//!
//! Whenever a required fact (sound event/holder, sound source, `BlockPos`
//! coordinates, target entity id, entity position/silent/sound-source, camera
//! position for `distanceDelay`, or the needed random stream) is absent or
//! unmodelled, the call is counted as unknown instead of guessing.
//!
//! # Where the seed is drawn
//!
//! Every draw happens exactly where the bytecode draws. Each unseeded
//! `Level.playSound` overload draws `soundSeedGenerator.nextLong()` before
//! `playSeededSound` reads `except`, the target, the sound or the source;
//! `ClientLevel.playLocalSound` and `playPlayerSound` draw `random.nextLong()`
//! before the sound instance reads anything. A call that then counts or
//! throws has still consumed its seed, so the next sound gets the next one.
//!
//! # Stream positions
//!
//! Each draw advances a level's stream, named by a stream key
//! ([`server_level_sound_stream`], [`CLIENT_LEVEL_SOUND_STREAM`],
//! [`level_random_stream`]). A stream whose position may differ from Java's is
//! *unknown* ([`StdHost::mark_sound_stream_unknown`]); every later draw from it
//! counts until a host seeds it again. A stream becomes unknown when:
//!
//! - Java draws where Decaf cannot (no seeded copy and no live host to defer
//!   the draw to);
//! - a call may or may not reach its draw (a `BlockPos` Decaf cannot read, or
//!   an entity whose `isSilent()`, level or `getSoundSource()` is unknown);
//! - a seed deferred to a live host ([`SoundSeed::HostDeferred`]) is never
//!   drawn by the host: on the server only where the host bridge cannot reach
//!   a level to draw on (see below); on the client whenever the effect is
//!   refused, because a client host draws only for a sound it plays. A live
//!   host may back every level with one stream (as Sunrise does), so while
//!   seeds are deferred a mark covers every stream on its side
//!   ([`host_sound_streams`]).
//!
//! # Server seeds a live host draws
//!
//! While server seeds are deferred, every server draw is logged, in call
//! order, as a [`SoundRecord`]: [`SoundRecord::Play`] for a call that recorded
//! a sound, and [`SoundRecord::SeedDrawn`] for a call that drew and then
//! played nothing (Decaf counted it after the draw, or it threw). The host
//! bridge replays the log in that order, drawing each seed from the live
//! host's stream (Sunrise's `LevelRef::next_sound_seed`, which is exactly the
//! stream its own `playSound` uses) and playing each sound with that seed
//! (`play_seeded_sound`). So the host's stream moves once per Java draw, in
//! Java's order, whether or not the sound then plays: a sound the bridge
//! refuses has consumed its seed, as in vanilla.
//!
//! Nothing draws from the host's stream between a Java call and the replay:
//! the Java code runs to the end before the bridge delivers, and the game does
//! not run in between. So drawing at the replay gives every seed the value a
//! draw at the call would have, and Java cannot read the value either way
//! (`Level.soundSeedGenerator` is private, with no getter, in 26.3; only
//! `playSeededSound` sees the seed, and Decaf models that method).

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Host, MethodRef, NativeError, StdHost, Value, client_surface, events, game_hooks, game_object,
    host_ref::HostKind, random_source, sound_event,
};

/// Minecraft 26.3 `net.minecraft.sounds.SoundSource`, in declaration (`ordinal`) order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum SoundSource {
    Master = 0,
    Music = 1,
    Records = 2,
    Weather = 3,
    Blocks = 4,
    Hostile = 5,
    Neutral = 6,
    Players = 7,
    Ambient = 8,
    Voice = 9,
    Ui = 10,
}

impl SoundSource {
    /// Every vanilla `SoundSource` in `SoundSource.values()` order.
    pub const ALL: [SoundSource; 11] = [
        Self::Master,
        Self::Music,
        Self::Records,
        Self::Weather,
        Self::Blocks,
        Self::Hostile,
        Self::Neutral,
        Self::Players,
        Self::Ambient,
        Self::Voice,
        Self::Ui,
    ];

    pub const fn ordinal(self) -> usize {
        self as usize
    }

    pub const fn from_ordinal(ordinal: usize) -> Option<Self> {
        if ordinal < Self::ALL.len() {
            Some(Self::ALL[ordinal])
        } else {
            None
        }
    }

    /// The Java enum constant name (`SoundSource.name()`).
    pub const fn java_name(self) -> &'static str {
        match self {
            Self::Master => "MASTER",
            Self::Music => "MUSIC",
            Self::Records => "RECORDS",
            Self::Weather => "WEATHER",
            Self::Blocks => "BLOCKS",
            Self::Hostile => "HOSTILE",
            Self::Neutral => "NEUTRAL",
            Self::Players => "PLAYERS",
            Self::Ambient => "AMBIENT",
            Self::Voice => "VOICE",
            Self::Ui => "UI",
        }
    }

    /// The vanilla wire/category name (`SoundSource.getName()`).
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Music => "music",
            Self::Records => "record",
            Self::Weather => "weather",
            Self::Blocks => "block",
            Self::Hostile => "hostile",
            Self::Neutral => "neutral",
            Self::Players => "player",
            Self::Ambient => "ambient",
            Self::Voice => "voice",
            Self::Ui => "ui",
        }
    }

    pub fn from_java_name(name: &str) -> Option<Self> {
        match name {
            "MASTER" => Some(Self::Master),
            "MUSIC" => Some(Self::Music),
            "RECORDS" => Some(Self::Records),
            "WEATHER" => Some(Self::Weather),
            "BLOCKS" => Some(Self::Blocks),
            "HOSTILE" => Some(Self::Hostile),
            "NEUTRAL" => Some(Self::Neutral),
            "PLAYERS" => Some(Self::Players),
            "AMBIENT" => Some(Self::Ambient),
            "VOICE" => Some(Self::Voice),
            "UI" => Some(Self::Ui),
            _ => None,
        }
    }

    /// Parse a runtime `SoundSource` enum constant marker.
    pub fn from_value(value: &Value) -> Option<Self> {
        let Value::Ref(Some(text)) = value else {
            return None;
        };
        let variant = text.strip_prefix("enum/net/minecraft/sounds/SoundSource/")?;
        Self::from_java_name(variant)
    }

    pub fn to_value(self) -> Value {
        Value::Ref(Some(format!(
            "enum/net/minecraft/sounds/SoundSource/{}",
            self.java_name()
        )))
    }
}

/// How the sound's `Holder<SoundEvent>` was obtained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoundHolderKind {
    /// Bound `Holder$Reference` (passed directly or returned by
    /// `BuiltInRegistries.SOUND_EVENT.wrapAsHolder` for a registered `SoundEvent`).
    Reference { registry_id: String },
    /// Direct `Holder$Direct` (passed via `Holder.direct` or returned by
    /// `wrapAsHolder` for an unregistered `SoundEvent`).
    Direct,
}

/// Whether the recorded sound effect came from a `ServerLevel` or `ClientLevel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundSide {
    Server,
    Client,
}

/// Where a recorded sound plays.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundTarget {
    /// Positional sound at `[x, y, z]` (`BlockPos` is centred with `+0.5`).
    Position([f64; 3]),
    /// Entity-bound sound (`ClientboundSoundEntityPacket` on server,
    /// `EntityBoundSoundInstance` on client).
    Entity {
        entity_id: i32,
        position: Option<[f64; 3]>,
    },
}

/// How the sound's 64-bit seed was produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundSeed {
    /// Drawn from `Level.soundSeedGenerator.nextLong()`.
    LevelSeedGenerator(i64),
    /// Drawn from `ClientLevel.random.nextLong()`.
    LevelRandom(i64),
    /// Explicit `long seed` argument to `Level.playSeededSound`.
    Explicit(i64),
    /// Delegated to the live host engine, which draws it from its own
    /// sound-seed stream when the recorded log is replayed: on a server with
    /// `LevelRef::next_sound_seed` (then `play_seeded_sound`), and where the
    /// host offers no separate draw (`HitHost::play_sound_*`,
    /// `LevelWriteHost::play_sound`, `ClientHost::play_local_sound`) by the
    /// play itself.
    HostDeferred,
}

/// One entry of a recorded sound log, in call order.
///
/// A server call whose seed was deferred to a live host leaves one entry per
/// draw, so the host bridge can draw the host's stream exactly where Java drew
/// (see the module docs). Client calls only ever record [`Self::Play`].
#[derive(Clone, Debug, PartialEq)]
pub enum SoundRecord {
    /// A sound to play.
    Play(LevelSoundEffect),
    /// A server `Level.playSound` drew `dimension`'s `soundSeedGenerator`
    /// (deferred to the live host: [`SoundSeed::HostDeferred`]) and then
    /// played nothing: Decaf counted the call after the draw, or it threw. The
    /// host still draws the seed, as vanilla's stream moved.
    SeedDrawn { dimension: String },
}

impl SoundRecord {
    /// The sound this entry plays, if any.
    pub fn effect(&self) -> Option<&LevelSoundEffect> {
        match self {
            Self::Play(effect) => Some(effect),
            Self::SeedDrawn { .. } => None,
        }
    }

    /// The sound this entry plays, if any.
    pub fn into_effect(self) -> Option<LevelSoundEffect> {
        match self {
            Self::Play(effect) => Some(effect),
            Self::SeedDrawn { .. } => None,
        }
    }
}

/// One exact level sound effect recorded from a Java `Level` or `Entity` call.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelSoundEffect {
    pub side: SoundSide,
    pub dimension: Option<String>,
    /// Resolved `SoundEvent.location()`.
    pub sound_id: String,
    /// `SoundEvent.fixedRange()`: `None` means variable range
    /// (`volume > 1.0f ? 16.0f * volume : 16.0f`).
    pub fixed_range: Option<f32>,
    /// `Holder<SoundEvent>` kind for `playSound` / `playSeededSound`, or `None`
    /// for `ClientLevel.playLocalSound` / `playPlayerSound` which take
    /// `SoundEvent` directly.
    pub holder: Option<SoundHolderKind>,
    pub target: SoundTarget,
    pub source: SoundSource,
    pub volume: f32,
    pub pitch: f32,
    /// Excluded player UUID for `ServerLevel.playSeededSound`
    /// (`except instanceof Player p ? p : null`). Always `None` on client.
    pub except_player: Option<u128>,
    pub seed: SoundSeed,
    /// `distanceDelay` argument to `ClientLevel.playLocalSound`.
    pub distance_delay: bool,
    /// Exact `SoundManager.playDelayed` tick delay on client (`0` for
    /// immediate `SoundManager.play` and all server sounds).
    pub delay_ticks: u32,
}

impl LevelSoundEffect {
    /// Exact MC 26.3 `SoundEvent.getRange(volume)`:
    /// `fixedRange.orElse(volume > 1.0f ? 16.0f * volume : 16.0f)`.
    pub fn range(&self) -> f32 {
        self.fixed_range.unwrap_or(if self.volume > 1.0 {
            16.0 * self.volume
        } else {
            16.0
        })
    }

    /// Exact 64-bit seed when drawn locally or passed explicitly (`None` when
    /// deferred to a live host).
    pub fn exact_seed(&self) -> Option<i64> {
        match self.seed {
            SoundSeed::LevelSeedGenerator(seed)
            | SoundSeed::LevelRandom(seed)
            | SoundSeed::Explicit(seed) => Some(seed),
            SoundSeed::HostDeferred => None,
        }
    }

    /// The stream key this effect's seed was drawn from (or deferred to a live
    /// host for); `None` for an explicit `playSeededSound` seed, which draws
    /// nothing.
    ///
    /// A server effect's is its level's `soundSeedGenerator`. A client effect
    /// with a [`Self::holder`] came through `Level.playSound` /
    /// `playSeededSound`, which draw the client level's `soundSeedGenerator`;
    /// one without came through `playLocalSound`, `playPlayerSound` or
    /// `LocalPlayer.playSound`, which draw `Level.random`.
    pub fn seed_stream(&self) -> Option<String> {
        if matches!(self.seed, SoundSeed::Explicit(_)) {
            return None;
        }
        Some(match self.side {
            SoundSide::Server => self.dimension.as_deref().map_or_else(
                || ANY_SERVER_LEVEL_SOUND_STREAM.to_owned(),
                server_level_sound_stream,
            ),
            SoundSide::Client if self.holder.is_some() => CLIENT_LEVEL_SOUND_STREAM.to_owned(),
            SoundSide::Client => level_random_stream(client_surface::LEVEL),
        })
    }
}

const SERVER_LEVEL_PREFIX: &str = "game/level/";
const LEVEL_RANDOM_SUFFIX: &str = "#random";

/// Stream key of a server level's `soundSeedGenerator`: the level's marker,
/// `game/level/<dimension>`.
pub fn server_level_sound_stream(dimension: &str) -> String {
    format!("{SERVER_LEVEL_PREFIX}{dimension}")
}

/// Stream key standing for the `soundSeedGenerator` of every server level.
/// Marked unknown when Java drew on some server level Decaf cannot name (an
/// entity whose level is unknown); a dimension id never contains `*`.
pub const ANY_SERVER_LEVEL_SOUND_STREAM: &str = "game/level/*";

/// Stream key of the client level's `soundSeedGenerator`.
pub const CLIENT_LEVEL_SOUND_STREAM: &str = client_surface::LEVEL;

/// Stream key of a level's `Level.random`, which `ClientLevel.playLocalSound`
/// and `playPlayerSound` draw from.
pub fn level_random_stream(level_marker: &str) -> String {
    format!("{level_marker}{LEVEL_RANDOM_SUFFIX}")
}

/// A server level's `soundSeedGenerator` key (or [`ANY_SERVER_LEVEL_SOUND_STREAM`]).
fn is_server_seed_generator(stream: &str) -> bool {
    stream.starts_with(SERVER_LEVEL_PREFIX) && !stream.ends_with(LEVEL_RANDOM_SUFFIX)
}

/// Whether `stream` is known, given the set of unknown stream keys. The one
/// place [`ANY_SERVER_LEVEL_SOUND_STREAM`] is read: it makes every server
/// level's `soundSeedGenerator` unknown.
pub fn sound_stream_known_in(unknown: &BTreeSet<String>, stream: &str) -> bool {
    if unknown.contains(stream) {
        return false;
    }
    !(is_server_seed_generator(stream) && unknown.contains(ANY_SERVER_LEVEL_SOUND_STREAM))
}

/// Every stream key a live host's one stream for `stream` may stand for.
///
/// Java keeps one `soundSeedGenerator` per level (and a `Level.random`), but
/// a live host need not. Sunrise (at the pinned commit) draws every server
/// sound's seed from one server-wide stream (`LanHub::next_sound_seed`), and
/// every client mod sound's from its audio engine's one random
/// (`Audio::play_mod_sound`). So once a seed deferred to the host goes
/// undelivered on one stream, every stream on that side is out of step with
/// the host: a server stream widens to [`ANY_SERVER_LEVEL_SOUND_STREAM`], a
/// client one to both client streams.
pub fn host_sound_streams(stream: &str) -> Vec<String> {
    if is_server_seed_generator(stream) {
        let mut streams = vec![stream.to_owned()];
        if stream != ANY_SERVER_LEVEL_SOUND_STREAM {
            streams.push(ANY_SERVER_LEVEL_SOUND_STREAM.to_owned());
        }
        streams
    } else if stream.starts_with(client_surface::LEVEL) {
        vec![
            CLIENT_LEVEL_SOUND_STREAM.to_owned(),
            level_random_stream(client_surface::LEVEL),
        ]
    } else {
        vec![stream.to_owned()]
    }
}

/// Optional sound-relevant facts for an entity or player marker.
/// Every absent field stays unknown and causes any call needing it to count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntitySoundFacts {
    pub level: Option<String>,
    pub position: Option<[f64; 3]>,
    pub silent: Option<bool>,
    pub sound_source: Option<SoundSource>,
    pub entity_id: Option<i32>,
    pub player_uuid: Option<u128>,
    pub is_player: bool,
}

/// Per-host level sound state: seeded level randoms, standalone entity/client
/// facts, host-deferred seed flags, and recorded standalone sounds.
#[derive(Clone, Debug, Default)]
pub(crate) struct LevelSoundState {
    pub(crate) level_random_handles: BTreeMap<String, Value>,
    pub(crate) level_sound_seeds: BTreeMap<String, random_source::State>,
    pub(crate) host_deferred_server_seeds: bool,
    pub(crate) host_deferred_client_seeds: bool,
    /// Stream keys whose position may differ from Java's (see the module
    /// docs): every draw from one counts.
    pub(crate) unknown_sound_streams: BTreeSet<String>,
    pub(crate) entity_facts: BTreeMap<String, EntitySoundFacts>,
    pub(crate) standalone_client_camera: Option<[f64; 3]>,
    pub(crate) standalone_client_player: Option<EntitySoundFacts>,
    /// Sounds (and server seed draws) recorded outside a frame, in call order.
    pub(crate) played_sounds: Vec<SoundRecord>,
}

/// Exact vanilla MC 26.3 `Entity.getSoundSource()` for entity types whose sound
/// source does not depend on instance state.
///
/// Verified against every `net/minecraft/world/entity/**/*.class` in
/// `server-26.3.jar`:
/// - `minecraft:rabbit` depends on `Rabbit.getVariant() == Variant.EVIL`, so
///   it returns `None` here (counted unless an explicit `sound_source` is given).
/// - Modded or unknown entity ids return `None`.
pub fn vanilla_entity_sound_source(type_id: &str) -> Option<SoundSource> {
    if !crate::vanilla_entity_types::ENTITY_TYPES.contains(&type_id) {
        return None;
    }
    match type_id {
        "minecraft:rabbit" => None,
        "minecraft:player" => Some(SoundSource::Players),
        "minecraft:experience_orb" | "minecraft:item" => Some(SoundSource::Ambient),
        "minecraft:lightning_bolt" => Some(SoundSource::Weather),
        "minecraft:ender_dragon"
        | "minecraft:wither"
        | "minecraft:blaze"
        | "minecraft:creeper"
        | "minecraft:elder_guardian"
        | "minecraft:enderman"
        | "minecraft:endermite"
        | "minecraft:ghast"
        | "minecraft:giant"
        | "minecraft:guardian"
        | "minecraft:phantom"
        | "minecraft:ravager"
        | "minecraft:shulker"
        | "minecraft:silverfish"
        | "minecraft:vex"
        | "minecraft:witch"
        | "minecraft:zoglin"
        | "minecraft:breeze"
        | "minecraft:creaking"
        | "minecraft:magma_cube"
        | "minecraft:slime"
        | "minecraft:hoglin"
        | "minecraft:evoker"
        | "minecraft:illusioner"
        | "minecraft:pillager"
        | "minecraft:vindicator"
        | "minecraft:piglin"
        | "minecraft:piglin_brute"
        | "minecraft:bogged"
        | "minecraft:parched"
        | "minecraft:skeleton"
        | "minecraft:stray"
        | "minecraft:wither_skeleton"
        | "minecraft:cave_spider"
        | "minecraft:spider"
        | "minecraft:warden"
        | "minecraft:drowned"
        | "minecraft:husk"
        | "minecraft:zombie"
        | "minecraft:zombie_villager"
        | "minecraft:zombified_piglin"
        | "minecraft:shulker_bullet" => Some(SoundSource::Hostile),
        _ => Some(SoundSource::Neutral),
    }
}

/// Outcome of a sound-surface call before converting `Unmodelled` into either
/// a lenient counted host answer or a strict façade refusal.
pub(crate) enum SoundCallOutcome {
    Handled(Value),
    Exception(NativeError),
    Unmodelled,
}

#[derive(Clone, Debug)]
struct ResolvedSound {
    sound_id: String,
    fixed_range: Option<f32>,
    holder: Option<SoundHolderKind>,
}

enum BlockPosArg {
    Coords([i32; 3]),
    Null,
    Unmodelled,
}

enum ExceptArg {
    None,
    Player(u128),
    NonPlayer,
    Unmodelled,
}

enum ClientExceptMatch {
    MatchesLocalPlayer,
    DoesNotMatch,
    Unmodelled,
}

enum TargetEntityArg {
    Entity {
        entity_id: i32,
        position: Option<[f64; 3]>,
    },
    Unmodelled,
}

fn npe(message: impl Into<String>) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn block_pos_arg(value: &Value) -> BlockPosArg {
    if *value == Value::Ref(None) {
        return BlockPosArg::Null;
    }
    let Value::Host(handle) = value else {
        return BlockPosArg::Unmodelled;
    };
    if handle.kind() != HostKind::GameValue {
        return BlockPosArg::Unmodelled;
    }
    match crate::server_values::block_pos(handle.key()) {
        Some(coords) => BlockPosArg::Coords(coords),
        None => BlockPosArg::Unmodelled,
    }
}

fn double_arg(args: &[Value], index: usize) -> Option<f64> {
    match args.get(index)? {
        Value::Float(v) => Some(*v),
        Value::Int(v) => Some(f64::from(*v)),
        Value::Long(v) => Some(*v as f64),
        _ => None,
    }
}

fn float_arg(args: &[Value], index: usize) -> Option<f32> {
    match args.get(index)? {
        Value::Float(v) => Some(*v as f32),
        Value::Int(v) => Some(*v as f32),
        Value::Long(v) => Some(*v as f32),
        _ => None,
    }
}

fn long_arg(args: &[Value], index: usize) -> Option<i64> {
    match args.get(index)? {
        Value::Long(v) => Some(*v),
        Value::Int(v) => Some(i64::from(*v)),
        _ => None,
    }
}

fn bool_arg(args: &[Value], index: usize) -> Option<bool> {
    match args.get(index)? {
        Value::Int(v) => Some(*v != 0),
        _ => None,
    }
}

fn resolve_direct_sound_event(value: &Value) -> Option<(String, Option<f32>)> {
    sound_event::from_value(value)
}

fn wrap_sound_event_as_holder<H: Host + ?Sized>(host: &H, value: &Value) -> Option<ResolvedSound> {
    let (sound_id, fixed_range) = resolve_direct_sound_event(value)?;
    let holder = match host.registered_sound_id(value) {
        Some(registry_id) => SoundHolderKind::Reference { registry_id },
        None => SoundHolderKind::Direct,
    };
    Some(ResolvedSound {
        sound_id,
        fixed_range,
        holder: Some(holder),
    })
}

fn resolve_sound_holder<H: Host + ?Sized>(
    host: &mut H,
    value: &Value,
) -> Result<Option<ResolvedSound>, NativeError> {
    let Some(unwrapped) = host.unwrap_sound_holder(value) else {
        return Ok(None);
    };
    let (inner, registry_id) = unwrapped?;
    let Some((sound_id, fixed_range)) = resolve_direct_sound_event(&inner) else {
        return Ok(None);
    };
    let holder = match registry_id {
        Some(registry_id) => SoundHolderKind::Reference { registry_id },
        None => SoundHolderKind::Direct,
    };
    Ok(Some(ResolvedSound {
        sound_id,
        fixed_range,
        holder: Some(holder),
    }))
}

/// Exact MC 26.3 `ClientLevel.playSound(DDD..., boolean distanceDelay, long seed)`
/// delay formula: `d2 > 100.0 ? (int)((Math.sqrt(d2) / 40.0) * 20.0) : 0`.
fn client_delay_ticks(camera: [f64; 3], pos: [f64; 3]) -> u32 {
    let dx = camera[0] - pos[0];
    let dy = camera[1] - pos[1];
    let dz = camera[2] - pos[2];
    let d2 = dx * dx + dy * dy + dz * dz;
    if d2 > 100.0 {
        let seconds = d2.sqrt() / 40.0;
        let ticks = (seconds * 20.0) as i32;
        ticks.max(0) as u32
    } else {
        0
    }
}

fn is_level_sound_method_name(name: &str) -> bool {
    matches!(
        name,
        "playSound" | "playSeededSound" | "playLocalSound" | "playPlayerSound" | "getRandom"
    )
}

fn is_entity_sound_method_name(name: &str) -> bool {
    matches!(name, "playSound" | "getSoundSource" | "isSilent")
}

/// Parsed overload of `Level` / `LevelAccessor` / `ServerLevel` / `ClientLevel`.
enum LevelSoundOp<'a> {
    /// `playSound(Entity except, BlockPos pos, SoundEvent sound, SoundSource source[, float vol, float pitch])`
    BlockPos {
        except: &'a Value,
        pos: &'a Value,
        sound: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
    },
    /// `playSound(Entity except, double x, y, z, SoundEvent sound, SoundSource source[, float vol, float pitch])`
    PosEvent {
        except: &'a Value,
        pos: [f64; 3],
        sound: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
    },
    /// `playSound(Entity except, double x, y, z, Holder<SoundEvent> sound, SoundSource source, float vol, float pitch)`
    PosHolder {
        except: &'a Value,
        pos: [f64; 3],
        holder: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
    },
    /// `playSound(Entity except, Entity target, SoundEvent sound, SoundSource source, float vol, float pitch)`
    EntityEvent {
        except: &'a Value,
        target: &'a Value,
        sound: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
    },
    /// `playSound(Entity except, Entity target, Holder<SoundEvent> sound, SoundSource source, float vol, float pitch)`
    EntityHolder {
        except: &'a Value,
        target: &'a Value,
        holder: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
    },
    /// `playSeededSound(Entity except, double x, y, z, Holder<SoundEvent> sound, SoundSource source, float vol, float pitch, long seed)`
    SeededPosHolder {
        except: &'a Value,
        pos: [f64; 3],
        holder: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
        seed: i64,
    },
    /// `playSeededSound(Entity except, double x, y, z, SoundEvent sound, SoundSource source, float vol, float pitch, long seed)`
    SeededPosEvent {
        except: &'a Value,
        pos: [f64; 3],
        sound: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
        seed: i64,
    },
    /// `playSeededSound(Entity except, Entity target, Holder<SoundEvent> sound, SoundSource source, float vol, float pitch, long seed)`
    SeededEntityHolder {
        except: &'a Value,
        target: &'a Value,
        holder: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
        seed: i64,
    },
    /// `playLocalSound(BlockPos pos, SoundEvent sound, SoundSource source, float vol, float pitch, boolean distanceDelay)`
    LocalBlockPos {
        pos: &'a Value,
        sound: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
        distance_delay: bool,
    },
    /// `playLocalSound(double x, y, z, SoundEvent sound, SoundSource source, float vol, float pitch, boolean distanceDelay)`
    LocalPos {
        pos: [f64; 3],
        sound: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
        distance_delay: bool,
    },
    /// `playLocalSound(Entity target, SoundEvent sound, SoundSource source, float vol, float pitch)`
    LocalEntity {
        target: &'a Value,
        sound: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
    },
    /// `playPlayerSound(SoundEvent sound, SoundSource source, float vol, float pitch)`
    PlayerSound {
        sound: &'a Value,
        source: &'a Value,
        volume: f32,
        pitch: f32,
    },
}

fn parse_level_op<'a>(r: &MethodRef, args: &'a [Value]) -> Option<LevelSoundOp<'a>> {
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "playSound",
            "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/BlockPos;Lnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;)V",
        ) => Some(LevelSoundOp::BlockPos {
            except: args.first()?,
            pos: args.get(1)?,
            sound: args.get(2)?,
            source: args.get(3)?,
            volume: 1.0,
            pitch: 1.0,
        }),
        (
            "playSound",
            "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/BlockPos;Lnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;FF)V",
        ) => Some(LevelSoundOp::BlockPos {
            except: args.first()?,
            pos: args.get(1)?,
            sound: args.get(2)?,
            source: args.get(3)?,
            volume: float_arg(args, 4)?,
            pitch: float_arg(args, 5)?,
        }),
        (
            "playSound",
            "(Lnet/minecraft/world/entity/Entity;DDDLnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;)V",
        ) => Some(LevelSoundOp::PosEvent {
            except: args.first()?,
            pos: [
                double_arg(args, 1)?,
                double_arg(args, 2)?,
                double_arg(args, 3)?,
            ],
            sound: args.get(4)?,
            source: args.get(5)?,
            volume: 1.0,
            pitch: 1.0,
        }),
        (
            "playSound",
            "(Lnet/minecraft/world/entity/Entity;DDDLnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;FF)V",
        ) => Some(LevelSoundOp::PosEvent {
            except: args.first()?,
            pos: [
                double_arg(args, 1)?,
                double_arg(args, 2)?,
                double_arg(args, 3)?,
            ],
            sound: args.get(4)?,
            source: args.get(5)?,
            volume: float_arg(args, 6)?,
            pitch: float_arg(args, 7)?,
        }),
        (
            "playSound",
            "(Lnet/minecraft/world/entity/Entity;DDDLnet/minecraft/core/Holder;Lnet/minecraft/sounds/SoundSource;FF)V",
        ) => Some(LevelSoundOp::PosHolder {
            except: args.first()?,
            pos: [
                double_arg(args, 1)?,
                double_arg(args, 2)?,
                double_arg(args, 3)?,
            ],
            holder: args.get(4)?,
            source: args.get(5)?,
            volume: float_arg(args, 6)?,
            pitch: float_arg(args, 7)?,
        }),
        (
            "playSound",
            "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;FF)V",
        ) => Some(LevelSoundOp::EntityEvent {
            except: args.first()?,
            target: args.get(1)?,
            sound: args.get(2)?,
            source: args.get(3)?,
            volume: float_arg(args, 4)?,
            pitch: float_arg(args, 5)?,
        }),
        (
            "playSound",
            "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/Holder;Lnet/minecraft/sounds/SoundSource;FF)V",
        ) => Some(LevelSoundOp::EntityHolder {
            except: args.first()?,
            target: args.get(1)?,
            holder: args.get(2)?,
            source: args.get(3)?,
            volume: float_arg(args, 4)?,
            pitch: float_arg(args, 5)?,
        }),
        (
            "playSeededSound",
            "(Lnet/minecraft/world/entity/Entity;DDDLnet/minecraft/core/Holder;Lnet/minecraft/sounds/SoundSource;FFJ)V",
        ) => Some(LevelSoundOp::SeededPosHolder {
            except: args.first()?,
            pos: [
                double_arg(args, 1)?,
                double_arg(args, 2)?,
                double_arg(args, 3)?,
            ],
            holder: args.get(4)?,
            source: args.get(5)?,
            volume: float_arg(args, 6)?,
            pitch: float_arg(args, 7)?,
            seed: long_arg(args, 8)?,
        }),
        (
            "playSeededSound",
            "(Lnet/minecraft/world/entity/Entity;DDDLnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;FFJ)V",
        ) => Some(LevelSoundOp::SeededPosEvent {
            except: args.first()?,
            pos: [
                double_arg(args, 1)?,
                double_arg(args, 2)?,
                double_arg(args, 3)?,
            ],
            sound: args.get(4)?,
            source: args.get(5)?,
            volume: float_arg(args, 6)?,
            pitch: float_arg(args, 7)?,
            seed: long_arg(args, 8)?,
        }),
        (
            "playSeededSound",
            "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/Holder;Lnet/minecraft/sounds/SoundSource;FFJ)V",
        ) => Some(LevelSoundOp::SeededEntityHolder {
            except: args.first()?,
            target: args.get(1)?,
            holder: args.get(2)?,
            source: args.get(3)?,
            volume: float_arg(args, 4)?,
            pitch: float_arg(args, 5)?,
            seed: long_arg(args, 6)?,
        }),
        (
            "playLocalSound",
            "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;FFZ)V",
        ) => Some(LevelSoundOp::LocalBlockPos {
            pos: args.first()?,
            sound: args.get(1)?,
            source: args.get(2)?,
            volume: float_arg(args, 3)?,
            pitch: float_arg(args, 4)?,
            distance_delay: bool_arg(args, 5)?,
        }),
        (
            "playLocalSound",
            "(DDDLnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;FFZ)V",
        ) => Some(LevelSoundOp::LocalPos {
            pos: [
                double_arg(args, 0)?,
                double_arg(args, 1)?,
                double_arg(args, 2)?,
            ],
            sound: args.get(3)?,
            source: args.get(4)?,
            volume: float_arg(args, 5)?,
            pitch: float_arg(args, 6)?,
            distance_delay: bool_arg(args, 7)?,
        }),
        (
            "playLocalSound",
            "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;FF)V",
        ) => Some(LevelSoundOp::LocalEntity {
            target: args.first()?,
            sound: args.get(1)?,
            source: args.get(2)?,
            volume: float_arg(args, 3)?,
            pitch: float_arg(args, 4)?,
        }),
        (
            "playPlayerSound",
            "(Lnet/minecraft/sounds/SoundEvent;Lnet/minecraft/sounds/SoundSource;FF)V",
        ) => Some(LevelSoundOp::PlayerSound {
            sound: args.first()?,
            source: args.get(1)?,
            volume: float_arg(args, 2)?,
            pitch: float_arg(args, 3)?,
        }),
        _ => None,
    }
}

/// Resolve `except` for `ServerLevel.playSeededSound` (`except instanceof Player p ? p : null`).
fn server_except_arg(
    value: &Value,
    context: Option<&game_hooks::GameHookContext>,
    entity_facts: &BTreeMap<String, EntitySoundFacts>,
) -> ExceptArg {
    if *value == Value::Ref(None) {
        return ExceptArg::None;
    }
    let Value::Host(handle) = value else {
        return ExceptArg::Unmodelled;
    };
    if handle.kind() != HostKind::GameEntity {
        return ExceptArg::Unmodelled;
    }
    let key = handle.key();
    if let Some(ctx) = context
        && let Some(obj) = ctx.object(key)
    {
        return match obj.kind {
            game_hooks::GameObjectKind::Player => match obj.uuid {
                Some(uuid) => ExceptArg::Player(uuid),
                None => ExceptArg::Unmodelled,
            },
            game_hooks::GameObjectKind::Living
            | game_hooks::GameObjectKind::Entity
            | game_hooks::GameObjectKind::ItemEntity => ExceptArg::NonPlayer,
            _ => ExceptArg::Unmodelled,
        };
    }
    if let Some(uuid) = game_object::player_uuid(key) {
        return ExceptArg::Player(uuid);
    }
    if key.starts_with("game/entity/") {
        if let Some(facts) = entity_facts.get(key)
            && facts.is_player
        {
            return facts
                .player_uuid
                .map_or(ExceptArg::Unmodelled, ExceptArg::Player);
        }
        return ExceptArg::NonPlayer;
    }
    if let Some(facts) = entity_facts.get(key) {
        if facts.is_player {
            return facts
                .player_uuid
                .map_or(ExceptArg::Unmodelled, ExceptArg::Player);
        }
        return ExceptArg::NonPlayer;
    }
    ExceptArg::Unmodelled
}

/// Resolve `target` for entity-bound `ServerLevel` sounds.
fn server_target_entity(
    value: &Value,
    firing: Option<&events::EventFiring>,
    hook_context: Option<&game_hooks::GameHookContext>,
    host_facts: &BTreeMap<String, EntitySoundFacts>,
) -> TargetEntityArg {
    let Value::Host(handle) = value else {
        return TargetEntityArg::Unmodelled;
    };
    if handle.kind() != HostKind::GameEntity {
        return TargetEntityArg::Unmodelled;
    }
    let key = handle.key();
    if let Some(ctx) = hook_context
        && let Some(obj) = ctx.object(key)
    {
        return match obj.entity_id {
            Some(entity_id) => TargetEntityArg::Entity {
                entity_id,
                position: obj.position,
            },
            None => TargetEntityArg::Unmodelled,
        };
    }
    let extra = firing
        .and_then(|f| f.context.entity_sound_facts.get(key))
        .or_else(|| host_facts.get(key));
    if let Some(entity_id) = game_object::entity_id_of(key) {
        let position = extra.and_then(|f| f.position);
        return TargetEntityArg::Entity {
            entity_id,
            position,
        };
    }
    if let Some(uuid) = game_object::player_uuid(key)
        && let Some(firing) = firing
        && let Some(player) = game_object::facade_player(&firing.facade, uuid)
        && let Some(entity_id) = player.entity_id
    {
        return TargetEntityArg::Entity {
            entity_id,
            position: extra.and_then(|f| f.position),
        };
    }
    if let Some(facts) = extra
        && let Some(entity_id) = facts.entity_id
    {
        return TargetEntityArg::Entity {
            entity_id,
            position: facts.position,
        };
    }
    TargetEntityArg::Unmodelled
}

/// Check `ClientLevel.playSeededSound`'s `except == this.minecraft.player` (`if_acmpne`).
fn client_except_matches(except: &Value, has_local_player: bool) -> ClientExceptMatch {
    if !has_local_player {
        return if *except == Value::Ref(None) {
            ClientExceptMatch::MatchesLocalPlayer
        } else {
            ClientExceptMatch::DoesNotMatch
        };
    }
    if *except == Value::Ref(None) {
        return ClientExceptMatch::DoesNotMatch;
    }
    match except {
        Value::Host(handle)
            if handle.kind() == HostKind::GameEntity && handle.key() == client_surface::PLAYER =>
        {
            ClientExceptMatch::MatchesLocalPlayer
        }
        Value::Host(handle) if handle.kind() == HostKind::GameEntity => {
            ClientExceptMatch::DoesNotMatch
        }
        _ => ClientExceptMatch::Unmodelled,
    }
}

fn client_target_entity(
    value: &Value,
    frame: Option<&client_surface::ClientSnapshot>,
    host: &StdHost,
) -> TargetEntityArg {
    let Value::Host(handle) = value else {
        return TargetEntityArg::Unmodelled;
    };
    if handle.kind() != HostKind::GameEntity {
        return TargetEntityArg::Unmodelled;
    }
    let key = handle.key();
    if key == client_surface::PLAYER {
        if let Some(player) = frame.and_then(|f| f.player.as_ref()) {
            return TargetEntityArg::Entity {
                entity_id: player.entity_id,
                position: Some(player.pos),
            };
        }
        if let Some(facts) = host.level_sound.standalone_client_player.as_ref()
            && let Some(entity_id) = facts.entity_id
        {
            return TargetEntityArg::Entity {
                entity_id,
                position: facts.position,
            };
        }
        return TargetEntityArg::Unmodelled;
    }
    if let Some(entity_id) = game_object::entity_id_of(key) {
        let position = host
            .level_sound
            .entity_facts
            .get(key)
            .and_then(|f| f.position);
        return TargetEntityArg::Entity {
            entity_id,
            position,
        };
    }
    if let Some(facts) = host.level_sound.entity_facts.get(key)
        && let Some(entity_id) = facts.entity_id
    {
        return TargetEntityArg::Entity {
            entity_id,
            position: facts.position,
        };
    }
    TargetEntityArg::Unmodelled
}

/// The sound argument of a `Level` sound overload.
///
/// A `SoundEvent` is wrapped with `BuiltInRegistries.SOUND_EVENT.wrapAsHolder`
/// (`MappedRegistry.wrapAsHolder`: an `IdentityHashMap.get`, then
/// `Holder.direct` when unregistered), which reads nothing of the event and
/// cannot throw. A `Holder<SoundEvent>` is unwrapped by `Holder.value()` in the
/// seeded half, which throws for an unbound stand-alone reference.
#[derive(Clone, Copy)]
enum SoundArg<'a> {
    Event(&'a Value),
    Holder(&'a Value),
}

/// Where the seeded half of a `Level` sound call plays.
#[derive(Clone, Copy)]
enum PlayAt<'a> {
    Position([f64; 3]),
    /// Bound to an entity (`ClientboundSoundEntityPacket` / `EntityBoundSoundInstance`).
    Entity(&'a Value),
}

/// What `playSeededSound` reads, once an overload has parsed (and, for an
/// unseeded overload, once it has drawn its seed).
#[derive(Clone, Copy)]
struct SeededArgs<'a> {
    except: &'a Value,
    at: PlayAt<'a>,
    sound: SoundArg<'a>,
    source: &'a Value,
    volume: f32,
    pitch: f32,
}

/// The `SoundEvent`-taking calls that draw `Level.random` (`playLocalSound`,
/// `playPlayerSound`, `LocalPlayer.playSound`): the event itself, its source,
/// volume and pitch.
#[derive(Clone, Copy)]
struct LocalSoundArgs<'a> {
    sound: &'a Value,
    source: &'a Value,
    volume: f32,
    pitch: f32,
}

/// The facts a `ServerLevel` call resolves its arguments against.
struct ServerLevelCall<'a> {
    level_marker: &'a str,
    dimension: &'a str,
    firing: Option<&'a events::EventFiring>,
    hook_context: Option<&'a game_hooks::GameHookContext>,
    host_facts: &'a BTreeMap<String, EntitySoundFacts>,
}

/// The facts a `ClientLevel` call resolves its arguments against.
struct ClientLevelCall<'a> {
    frame: Option<&'a client_surface::ClientSnapshot>,
    dimension: Option<String>,
    has_local_player: bool,
    camera_pos: Option<[f64; 3]>,
}

fn resolve_sound_arg<H: Host + ?Sized>(
    host: &mut H,
    sound: SoundArg<'_>,
) -> Result<ResolvedSound, SoundCallOutcome> {
    match sound {
        SoundArg::Event(value) => {
            wrap_sound_event_as_holder(host, value).ok_or(SoundCallOutcome::Unmodelled)
        }
        SoundArg::Holder(value) => match resolve_sound_holder(host, value) {
            Ok(Some(resolved)) => Ok(resolved),
            Ok(None) => Err(SoundCallOutcome::Unmodelled),
            Err(err) => Err(SoundCallOutcome::Exception(err)),
        },
    }
}

/// `BlockPos` overloads pass `(double) pos.getX() + 0.5` (and Y, Z) on.
fn centred(coords: [i32; 3]) -> [f64; 3] {
    [
        f64::from(coords[0]) + 0.5,
        f64::from(coords[1]) + 0.5,
        f64::from(coords[2]) + 0.5,
    ]
}

fn null_block_pos() -> NativeError {
    npe(r#"Cannot invoke "net.minecraft.core.BlockPos.getX()" because "pos" is null"#)
}

/// A client seed was drawn for a call that then recorded no effect: Decaf
/// counted it, it threw, or the call skipped another player's sound.
///
/// Java consumed that seed all the same. A seed Decaf drew itself (or an
/// explicit one) leaves Decaf's copy of the stream exactly where Java's is.
/// A seed deferred to a live client host ([`SoundSeed::HostDeferred`]) does
/// not: `play_local_sound` draws only for a sound it plays, and the client
/// host has no draw of its own, so its stream is now one draw behind Java's
/// and every later draw from `stream` must count.
///
/// A server seed needs none of this: [`server_seed_not_played`] logs the draw
/// for the host bridge to replay.
fn seed_not_delivered<H: Host + ?Sized>(host: &mut H, stream: &str, seed: SoundSeed) {
    if seed == SoundSeed::HostDeferred {
        host.mark_sound_stream_unknown(stream);
    }
}

/// A server seed was drawn from `dimension`'s `soundSeedGenerator` for a call
/// that then recorded no sound (Decaf counted it after the draw, or it threw).
///
/// A seed Decaf drew itself, or an explicit one, needs nothing: Decaf's copy
/// of the stream is where Java's is. A seed deferred to the live host is
/// logged as [`SoundRecord::SeedDrawn`], in call order, so the bridge draws
/// the host's stream at this point too and it stays in step with Java's.
fn server_seed_not_played(dimension: &str, seed: SoundSeed, emit: &mut dyn FnMut(SoundRecord)) {
    if seed == SoundSeed::HostDeferred {
        emit(SoundRecord::SeedDrawn {
            dimension: dimension.to_owned(),
        });
    }
}

/// Core `ServerLevel` sound/random dispatcher shared by standalone `StdHost`,
/// `events::EventFiring`, and `game_hooks::GameHookHost`.
#[allow(clippy::too_many_arguments)]
fn execute_server_level_call<H: Host + ?Sized>(
    host: &mut H,
    level_marker: &str,
    dimension: &str,
    r: &MethodRef,
    args: &[Value],
    firing: Option<&events::EventFiring>,
    hook_context: Option<&game_hooks::GameHookContext>,
    host_facts: &BTreeMap<String, EntitySoundFacts>,
    emit: &mut dyn FnMut(SoundRecord),
) -> SoundCallOutcome {
    if r.name == "getRandom" && r.descriptor == "()Lnet/minecraft/util/RandomSource;" {
        return match host.level_random_handle(level_marker) {
            Some(handle) => SoundCallOutcome::Handled(handle),
            None => SoundCallOutcome::Unmodelled,
        };
    }
    let Some(op) = parse_level_op(r, args) else {
        return SoundCallOutcome::Unmodelled;
    };
    let call = ServerLevelCall {
        level_marker,
        dimension,
        firing,
        hook_context,
        host_facts,
    };
    match op {
        // `ServerLevel` inherits `Level.playLocalSound` / `playPlayerSound` as no-ops.
        // Only the `BlockPos` overload reads `pos.getX/Y/Z()` before delegating.
        LevelSoundOp::LocalBlockPos { pos, .. } => match block_pos_arg(pos) {
            BlockPosArg::Coords(_) => SoundCallOutcome::Handled(Value::Void),
            BlockPosArg::Null => SoundCallOutcome::Exception(null_block_pos()),
            BlockPosArg::Unmodelled => SoundCallOutcome::Unmodelled,
        },
        LevelSoundOp::LocalPos { .. }
        | LevelSoundOp::LocalEntity { .. }
        | LevelSoundOp::PlayerSound { .. } => SoundCallOutcome::Handled(Value::Void),

        LevelSoundOp::BlockPos {
            except,
            pos,
            sound,
            source,
            volume,
            pitch,
        } => {
            // `Level.playSound(Entity, BlockPos, ...)` reads `pos.getX/Y/Z()` (a
            // null `pos` throws here, before any draw), then calls the `double`
            // overload, which draws.
            let coords = match block_pos_arg(pos) {
                BlockPosArg::Coords(coords) => coords,
                BlockPosArg::Null => return SoundCallOutcome::Exception(null_block_pos()),
                BlockPosArg::Unmodelled => {
                    // A `BlockPos` Decaf cannot read may or may not throw before
                    // the draw: the stream's position is no longer known.
                    host.mark_sound_stream_unknown(level_marker);
                    return SoundCallOutcome::Unmodelled;
                }
            };
            let args = SeededArgs {
                except,
                at: PlayAt::Position(centred(coords)),
                sound: SoundArg::Event(sound),
                source,
                volume,
                pitch,
            };
            server_play(host, &call, args, None, emit)
        }
        LevelSoundOp::PosEvent {
            except,
            pos,
            sound,
            source,
            volume,
            pitch,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Position(pos),
                sound: SoundArg::Event(sound),
                source,
                volume,
                pitch,
            };
            server_play(host, &call, args, None, emit)
        }
        LevelSoundOp::PosHolder {
            except,
            pos,
            holder,
            source,
            volume,
            pitch,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Position(pos),
                sound: SoundArg::Holder(holder),
                source,
                volume,
                pitch,
            };
            server_play(host, &call, args, None, emit)
        }
        LevelSoundOp::EntityEvent {
            except,
            target,
            sound,
            source,
            volume,
            pitch,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Entity(target),
                sound: SoundArg::Event(sound),
                source,
                volume,
                pitch,
            };
            server_play(host, &call, args, None, emit)
        }
        LevelSoundOp::EntityHolder {
            except,
            target,
            holder,
            source,
            volume,
            pitch,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Entity(target),
                sound: SoundArg::Holder(holder),
                source,
                volume,
                pitch,
            };
            server_play(host, &call, args, None, emit)
        }
        LevelSoundOp::SeededPosHolder {
            except,
            pos,
            holder,
            source,
            volume,
            pitch,
            seed,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Position(pos),
                sound: SoundArg::Holder(holder),
                source,
                volume,
                pitch,
            };
            server_play(host, &call, args, Some(seed), emit)
        }
        LevelSoundOp::SeededPosEvent {
            except,
            pos,
            sound,
            source,
            volume,
            pitch,
            seed,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Position(pos),
                sound: SoundArg::Event(sound),
                source,
                volume,
                pitch,
            };
            server_play(host, &call, args, Some(seed), emit)
        }
        LevelSoundOp::SeededEntityHolder {
            except,
            target,
            holder,
            source,
            volume,
            pitch,
            seed,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Entity(target),
                sound: SoundArg::Holder(holder),
                source,
                volume,
                pitch,
            };
            server_play(host, &call, args, Some(seed), emit)
        }
    }
}

/// `Level.playSound` (`explicit_seed == None`) or `playSeededSound` on a
/// `ServerLevel`.
///
/// Every unseeded overload draws `this.soundSeedGenerator.nextLong()` before
/// `playSeededSound` reads a single argument: `Level.playSound(Entity, DDD,
/// SoundEvent | Holder, SoundSource, FF)` at offsets 16-19 and
/// `playSound(Entity, Entity, Holder, ...)` at 11-14, straight after loading
/// its arguments; `playSound(Entity, Entity, SoundEvent, ...)` at 19-22, after
/// only `wrapAsHolder`, which cannot throw. So the draw comes first here too,
/// and a sound, holder, source, `except` or target Decaf cannot resolve, or one
/// that throws, has still advanced the stream.
fn server_play<H: Host + ?Sized>(
    host: &mut H,
    call: &ServerLevelCall<'_>,
    args: SeededArgs<'_>,
    explicit_seed: Option<i64>,
    emit: &mut dyn FnMut(SoundRecord),
) -> SoundCallOutcome {
    let seed = match explicit_seed {
        Some(seed) => SoundSeed::Explicit(seed),
        None => match host.draw_level_sound_seed(call.level_marker, true) {
            Some(seed) => seed,
            None => return SoundCallOutcome::Unmodelled,
        },
    };
    match server_seeded_effect(host, call, args, seed) {
        Ok(effect) => {
            emit(SoundRecord::Play(effect));
            SoundCallOutcome::Handled(Value::Void)
        }
        Err(outcome) => {
            server_seed_not_played(call.dimension, seed, emit);
            outcome
        }
    }
}

/// `ServerLevel.playSeededSound`'s reads, in bytecode order: `except
/// instanceof Player` (never throws), the target's `getX/Y/Z()` (entity
/// overload), then `sound.value()` and `SoundEvent.getRange(volume)`. `source`
/// is only stored in the packet; Decaf needs it to record the effect, so it is
/// resolved last.
fn server_seeded_effect<H: Host + ?Sized>(
    host: &mut H,
    call: &ServerLevelCall<'_>,
    args: SeededArgs<'_>,
    seed: SoundSeed,
) -> Result<LevelSoundEffect, SoundCallOutcome> {
    let except_player = match server_except_arg(args.except, call.hook_context, call.host_facts) {
        ExceptArg::None | ExceptArg::NonPlayer => None,
        ExceptArg::Player(uuid) => Some(uuid),
        ExceptArg::Unmodelled => return Err(SoundCallOutcome::Unmodelled),
    };
    let target = match args.at {
        PlayAt::Position(pos) => SoundTarget::Position(pos),
        PlayAt::Entity(target) => {
            match server_target_entity(target, call.firing, call.hook_context, call.host_facts) {
                TargetEntityArg::Entity {
                    entity_id,
                    position,
                } => SoundTarget::Entity {
                    entity_id,
                    position,
                },
                TargetEntityArg::Unmodelled => return Err(SoundCallOutcome::Unmodelled),
            }
        }
    };
    let resolved = resolve_sound_arg(host, args.sound)?;
    let source = SoundSource::from_value(args.source).ok_or(SoundCallOutcome::Unmodelled)?;
    Ok(LevelSoundEffect {
        side: SoundSide::Server,
        dimension: Some(call.dimension.to_owned()),
        sound_id: resolved.sound_id,
        fixed_range: resolved.fixed_range,
        holder: resolved.holder,
        target,
        source,
        volume: args.volume,
        pitch: args.pitch,
        except_player,
        seed,
        distance_delay: false,
        delay_ticks: 0,
    })
}

/// Core `ClientLevel` sound/random dispatcher for both `ClientSnapshot` frames
/// and standalone `client/level` handles.
fn execute_client_level_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
    frame: Option<&client_surface::ClientSnapshot>,
    emit: &mut dyn FnMut(&mut StdHost, LevelSoundEffect),
) -> SoundCallOutcome {
    let level_marker = client_surface::LEVEL;
    if r.name == "getRandom" && r.descriptor == "()Lnet/minecraft/util/RandomSource;" {
        return match host.level_random_handle(level_marker) {
            Some(handle) => SoundCallOutcome::Handled(handle),
            None => SoundCallOutcome::Unmodelled,
        };
    }
    let Some(op) = parse_level_op(r, args) else {
        return SoundCallOutcome::Unmodelled;
    };
    let call = ClientLevelCall {
        frame,
        dimension: frame
            .and_then(|f| f.level.as_ref())
            .map(|l| l.dimension.clone())
            .filter(|d| !d.is_empty()),
        has_local_player: frame.map_or_else(
            || host.level_sound.standalone_client_player.is_some(),
            |f| f.player.is_some(),
        ),
        camera_pos: frame
            .and_then(|f| f.camera.as_ref().map(|c| c.pos))
            .or(host.level_sound.standalone_client_camera),
    };

    match op {
        LevelSoundOp::BlockPos {
            except,
            pos,
            sound,
            source,
            volume,
            pitch,
        } => {
            let coords = match block_pos_arg(pos) {
                BlockPosArg::Coords(coords) => coords,
                BlockPosArg::Null => return SoundCallOutcome::Exception(null_block_pos()),
                BlockPosArg::Unmodelled => {
                    // May or may not throw before `soundSeedGenerator` draws.
                    host.mark_sound_stream_unknown(CLIENT_LEVEL_SOUND_STREAM);
                    return SoundCallOutcome::Unmodelled;
                }
            };
            let args = SeededArgs {
                except,
                at: PlayAt::Position(centred(coords)),
                sound: SoundArg::Event(sound),
                source,
                volume,
                pitch,
            };
            client_play(host, &call, args, None, emit)
        }
        LevelSoundOp::PosEvent {
            except,
            pos,
            sound,
            source,
            volume,
            pitch,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Position(pos),
                sound: SoundArg::Event(sound),
                source,
                volume,
                pitch,
            };
            client_play(host, &call, args, None, emit)
        }
        LevelSoundOp::PosHolder {
            except,
            pos,
            holder,
            source,
            volume,
            pitch,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Position(pos),
                sound: SoundArg::Holder(holder),
                source,
                volume,
                pitch,
            };
            client_play(host, &call, args, None, emit)
        }
        LevelSoundOp::EntityEvent {
            except,
            target,
            sound,
            source,
            volume,
            pitch,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Entity(target),
                sound: SoundArg::Event(sound),
                source,
                volume,
                pitch,
            };
            client_play(host, &call, args, None, emit)
        }
        LevelSoundOp::EntityHolder {
            except,
            target,
            holder,
            source,
            volume,
            pitch,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Entity(target),
                sound: SoundArg::Holder(holder),
                source,
                volume,
                pitch,
            };
            client_play(host, &call, args, None, emit)
        }
        LevelSoundOp::SeededPosHolder {
            except,
            pos,
            holder,
            source,
            volume,
            pitch,
            seed,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Position(pos),
                sound: SoundArg::Holder(holder),
                source,
                volume,
                pitch,
            };
            client_play(host, &call, args, Some(seed), emit)
        }
        LevelSoundOp::SeededPosEvent {
            except,
            pos,
            sound,
            source,
            volume,
            pitch,
            seed,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Position(pos),
                sound: SoundArg::Event(sound),
                source,
                volume,
                pitch,
            };
            client_play(host, &call, args, Some(seed), emit)
        }
        LevelSoundOp::SeededEntityHolder {
            except,
            target,
            holder,
            source,
            volume,
            pitch,
            seed,
        } => {
            let args = SeededArgs {
                except,
                at: PlayAt::Entity(target),
                sound: SoundArg::Holder(holder),
                source,
                volume,
                pitch,
            };
            client_play(host, &call, args, Some(seed), emit)
        }

        LevelSoundOp::LocalBlockPos {
            pos,
            sound,
            source,
            volume,
            pitch,
            distance_delay,
        } => {
            // `Level.playLocalSound(BlockPos, ...)` reads `pos.getX/Y/Z()` before
            // `ClientLevel.playLocalSound(DDD, ...)` draws `this.random`.
            let coords = match block_pos_arg(pos) {
                BlockPosArg::Coords(coords) => coords,
                BlockPosArg::Null => return SoundCallOutcome::Exception(null_block_pos()),
                BlockPosArg::Unmodelled => {
                    host.mark_sound_stream_unknown(&level_random_stream(level_marker));
                    return SoundCallOutcome::Unmodelled;
                }
            };
            let args = LocalSoundArgs {
                sound,
                source,
                volume,
                pitch,
            };
            client_play_local_pos(host, &call, centred(coords), args, distance_delay, emit)
        }

        LevelSoundOp::LocalPos {
            pos,
            sound,
            source,
            volume,
            pitch,
            distance_delay,
        } => {
            let args = LocalSoundArgs {
                sound,
                source,
                volume,
                pitch,
            };
            client_play_local_pos(host, &call, pos, args, distance_delay, emit)
        }

        LevelSoundOp::LocalEntity {
            target,
            sound,
            source,
            volume,
            pitch,
        } => {
            // `ClientLevel.playLocalSound(Entity, ...)` evaluates
            // `this.random.nextLong()` (offset 22) as the last argument of
            // `new EntityBoundSoundInstance(...)`, before the constructor reads
            // the sound or the entity.
            let Some(seed) = host.draw_client_local_seed() else {
                return SoundCallOutcome::Unmodelled;
            };
            let args = LocalSoundArgs {
                sound,
                source,
                volume,
                pitch,
            };
            let effect = client_local_entity_effect(host, &call, target, args, seed);
            finish_client_local(host, seed, effect, emit)
        }

        LevelSoundOp::PlayerSound {
            sound,
            source,
            volume,
            pitch,
        } => {
            // `ClientLevel.playPlayerSound`: checks `if (this.minecraft.player != null)`
            // first. When `player == null`, returns without drawing `this.random` or
            // reading the sound. Otherwise it draws (offset 37) before
            // `EntityBoundSoundInstance`'s constructor reads anything.
            if !call.has_local_player {
                return SoundCallOutcome::Handled(Value::Void);
            }
            let Some(seed) = host.draw_client_local_seed() else {
                return SoundCallOutcome::Unmodelled;
            };
            let args = LocalSoundArgs {
                sound,
                source,
                volume,
                pitch,
            };
            let effect = client_player_sound_effect(host, &call, args, seed);
            finish_client_local(host, seed, effect, emit)
        }
    }
}

/// `Level.playSound` (`explicit_seed == None`) or `playSeededSound` on a
/// `ClientLevel`.
///
/// An unseeded overload draws `this.soundSeedGenerator.nextLong()` first, as
/// on a server (see [`server_play`]); `ClientLevel.playSeededSound` then
/// returns at once unless `except == minecraft.player` (`if_acmpne`), so the
/// draw happens whoever `except` is.
fn client_play(
    host: &mut StdHost,
    call: &ClientLevelCall<'_>,
    args: SeededArgs<'_>,
    explicit_seed: Option<i64>,
    emit: &mut dyn FnMut(&mut StdHost, LevelSoundEffect),
) -> SoundCallOutcome {
    let seed = match explicit_seed {
        Some(seed) => SoundSeed::Explicit(seed),
        None => match host.draw_level_sound_seed(client_surface::LEVEL, false) {
            Some(seed) => seed,
            None => return SoundCallOutcome::Unmodelled,
        },
    };
    match client_seeded_effect(host, call, args, seed) {
        Ok(Some(effect)) => {
            emit(host, effect);
            SoundCallOutcome::Handled(Value::Void)
        }
        Ok(None) => {
            seed_not_delivered(host, CLIENT_LEVEL_SOUND_STREAM, seed);
            SoundCallOutcome::Handled(Value::Void)
        }
        Err(outcome) => {
            seed_not_delivered(host, CLIENT_LEVEL_SOUND_STREAM, seed);
            outcome
        }
    }
}

/// `ClientLevel.playSeededSound`'s reads, in bytecode order. `Ok(None)` is
/// the `except != minecraft.player` early return: nothing plays.
///
/// Both overloads unwrap `sound.value()` before anything reads the target:
/// the position one hands it to the private `playSound`, the entity one to
/// `EntityBoundSoundInstance`'s constructor, which then reads the entity.
fn client_seeded_effect(
    host: &mut StdHost,
    call: &ClientLevelCall<'_>,
    args: SeededArgs<'_>,
    seed: SoundSeed,
) -> Result<Option<LevelSoundEffect>, SoundCallOutcome> {
    match client_except_matches(args.except, call.has_local_player) {
        ClientExceptMatch::Unmodelled => return Err(SoundCallOutcome::Unmodelled),
        ClientExceptMatch::DoesNotMatch => return Ok(None),
        ClientExceptMatch::MatchesLocalPlayer => {}
    }
    let resolved = resolve_sound_arg(host, args.sound)?;
    let target = match args.at {
        PlayAt::Position(pos) => SoundTarget::Position(pos),
        PlayAt::Entity(target) => match client_target_entity(target, call.frame, host) {
            TargetEntityArg::Entity {
                entity_id,
                position,
            } => SoundTarget::Entity {
                entity_id,
                position,
            },
            TargetEntityArg::Unmodelled => return Err(SoundCallOutcome::Unmodelled),
        },
    };
    let source = SoundSource::from_value(args.source).ok_or(SoundCallOutcome::Unmodelled)?;
    Ok(Some(LevelSoundEffect {
        side: SoundSide::Client,
        dimension: call.dimension.clone(),
        sound_id: resolved.sound_id,
        fixed_range: resolved.fixed_range,
        holder: resolved.holder,
        target,
        source,
        volume: args.volume,
        pitch: args.pitch,
        except_player: None,
        seed,
        distance_delay: false,
        delay_ticks: 0,
    }))
}

/// Emit a `Level.random`-seeded client effect, or note that its drawn seed
/// was not delivered.
fn finish_client_local(
    host: &mut StdHost,
    seed: SoundSeed,
    effect: Result<LevelSoundEffect, SoundCallOutcome>,
    emit: &mut dyn FnMut(&mut StdHost, LevelSoundEffect),
) -> SoundCallOutcome {
    match effect {
        Ok(effect) => {
            emit(host, effect);
            SoundCallOutcome::Handled(Value::Void)
        }
        Err(outcome) => {
            seed_not_delivered(host, &level_random_stream(client_surface::LEVEL), seed);
            outcome
        }
    }
}

/// `ClientLevel.playLocalSound(DDD, ...)`: draws `this.random.nextLong()`
/// (offset 19) before the private `playSound` reads the camera and builds the
/// `SimpleSoundInstance` from the sound.
fn client_play_local_pos(
    host: &mut StdHost,
    call: &ClientLevelCall<'_>,
    pos: [f64; 3],
    args: LocalSoundArgs<'_>,
    distance_delay: bool,
    emit: &mut dyn FnMut(&mut StdHost, LevelSoundEffect),
) -> SoundCallOutcome {
    let Some(seed) = host.draw_client_local_seed() else {
        return SoundCallOutcome::Unmodelled;
    };
    let effect = client_local_pos_effect(call, pos, args, distance_delay, seed);
    finish_client_local(host, seed, effect, emit)
}

fn client_local_pos_effect(
    call: &ClientLevelCall<'_>,
    pos: [f64; 3],
    args: LocalSoundArgs<'_>,
    distance_delay: bool,
    seed: SoundSeed,
) -> Result<LevelSoundEffect, SoundCallOutcome> {
    let delay_ticks = if distance_delay {
        let camera = call.camera_pos.ok_or(SoundCallOutcome::Unmodelled)?;
        client_delay_ticks(camera, pos)
    } else {
        0
    };
    let (sound_id, fixed_range) =
        resolve_direct_sound_event(args.sound).ok_or(SoundCallOutcome::Unmodelled)?;
    let source = SoundSource::from_value(args.source).ok_or(SoundCallOutcome::Unmodelled)?;
    Ok(LevelSoundEffect {
        side: SoundSide::Client,
        dimension: call.dimension.clone(),
        sound_id,
        fixed_range,
        holder: None,
        target: SoundTarget::Position(pos),
        source,
        volume: args.volume,
        pitch: args.pitch,
        except_player: None,
        seed,
        distance_delay,
        delay_ticks,
    })
}

/// `EntityBoundSoundInstance(sound, source, volume, pitch, entity, seed)` for
/// `ClientLevel.playLocalSound(Entity, ...)`: reads the sound, then the entity.
fn client_local_entity_effect(
    host: &StdHost,
    call: &ClientLevelCall<'_>,
    target: &Value,
    args: LocalSoundArgs<'_>,
    seed: SoundSeed,
) -> Result<LevelSoundEffect, SoundCallOutcome> {
    let (sound_id, fixed_range) =
        resolve_direct_sound_event(args.sound).ok_or(SoundCallOutcome::Unmodelled)?;
    let TargetEntityArg::Entity {
        entity_id,
        position,
    } = client_target_entity(target, call.frame, host)
    else {
        return Err(SoundCallOutcome::Unmodelled);
    };
    let source = SoundSource::from_value(args.source).ok_or(SoundCallOutcome::Unmodelled)?;
    Ok(LevelSoundEffect {
        side: SoundSide::Client,
        dimension: call.dimension.clone(),
        sound_id,
        fixed_range,
        holder: None,
        target: SoundTarget::Entity {
            entity_id,
            position,
        },
        source,
        volume: args.volume,
        pitch: args.pitch,
        except_player: None,
        seed,
        distance_delay: false,
        delay_ticks: 0,
    })
}

/// `ClientLevel.playPlayerSound` past its `player != null` check: an
/// `EntityBoundSoundInstance` on the local player.
fn client_player_sound_effect(
    host: &StdHost,
    call: &ClientLevelCall<'_>,
    args: LocalSoundArgs<'_>,
    seed: SoundSeed,
) -> Result<LevelSoundEffect, SoundCallOutcome> {
    let (sound_id, fixed_range) =
        resolve_direct_sound_event(args.sound).ok_or(SoundCallOutcome::Unmodelled)?;
    let (entity_id, position) = if let Some(player) = call.frame.and_then(|f| f.player.as_ref()) {
        (player.entity_id, Some(player.pos))
    } else if let Some(facts) = host.level_sound.standalone_client_player.as_ref() {
        let entity_id = facts.entity_id.ok_or(SoundCallOutcome::Unmodelled)?;
        (entity_id, facts.position)
    } else {
        return Err(SoundCallOutcome::Unmodelled);
    };
    let source = SoundSource::from_value(args.source).ok_or(SoundCallOutcome::Unmodelled)?;
    Ok(LevelSoundEffect {
        side: SoundSide::Client,
        dimension: call.dimension.clone(),
        sound_id,
        fixed_range,
        holder: None,
        target: SoundTarget::Entity {
            entity_id,
            position,
        },
        source,
        volume: args.volume,
        pitch: args.pitch,
        except_player: None,
        seed,
        distance_delay: false,
        delay_ticks: 0,
    })
}

/// The `soundSeedGenerator` an entity's `Level.playSound` draws from: its own
/// level's, or (when Decaf does not know the entity's level) any server level's.
fn entity_level_stream(dimension: Option<&str>) -> String {
    dimension.map_or_else(
        || ANY_SERVER_LEVEL_SOUND_STREAM.to_owned(),
        server_level_sound_stream,
    )
}

/// Dispatch `Entity.playSound`, `Player.playSound`, `Entity.getSoundSource`, and
/// `Entity.isSilent` for a server-side entity or player.
#[allow(clippy::too_many_arguments)]
fn execute_server_entity_call<H: Host + ?Sized>(
    host: &mut H,
    r: &MethodRef,
    args: &[Value],
    is_player: bool,
    player_uuid: Option<u128>,
    dimension: Option<&str>,
    position: Option<[f64; 3]>,
    silent: Option<bool>,
    sound_source: Option<SoundSource>,
    emit: &mut dyn FnMut(SoundRecord),
) -> SoundCallOutcome {
    let call = EntitySoundCall {
        is_player,
        player_uuid,
        dimension,
        position,
        sound_source,
    };
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("getSoundSource", "()Lnet/minecraft/sounds/SoundSource;") => {
            let source = if is_player {
                Some(SoundSource::Players)
            } else {
                sound_source
            };
            match source {
                Some(source) => SoundCallOutcome::Handled(source.to_value()),
                None => SoundCallOutcome::Unmodelled,
            }
        }
        ("isSilent", "()Z") => match silent {
            Some(silent) => SoundCallOutcome::Handled(Value::Int(i32::from(silent))),
            None => SoundCallOutcome::Unmodelled,
        },
        // `Entity.playSound(SoundEvent)`: checks `!isSilent()` BEFORE calling
        // virtual `this.playSound(sound, 1.0f, 1.0f)` (even on `Player`!).
        ("playSound", "(Lnet/minecraft/sounds/SoundEvent;)V") => {
            let Some(is_silent) = silent else {
                // Whether the call reaches `Level.playSound`'s draw hangs on `isSilent()`.
                host.mark_sound_stream_unknown(&entity_level_stream(dimension));
                return SoundCallOutcome::Unmodelled;
            };
            if is_silent {
                return SoundCallOutcome::Handled(Value::Void);
            }
            let Some(sound) = args.first() else {
                return SoundCallOutcome::Unmodelled;
            };
            server_entity_emit_play_sound(host, &call, sound, 1.0, 1.0, emit)
        }
        // `Player.playSound(SoundEvent, float, float)` does NOT check `isSilent()`;
        // `Entity.playSound(SoundEvent, float, float)` DOES check `!isSilent()`.
        ("playSound", "(Lnet/minecraft/sounds/SoundEvent;FF)V") => {
            if !is_player {
                let Some(is_silent) = silent else {
                    host.mark_sound_stream_unknown(&entity_level_stream(dimension));
                    return SoundCallOutcome::Unmodelled;
                };
                if is_silent {
                    return SoundCallOutcome::Handled(Value::Void);
                }
            }
            let (Some(sound), Some(volume), Some(pitch)) =
                (args.first(), float_arg(args, 1), float_arg(args, 2))
            else {
                return SoundCallOutcome::Unmodelled;
            };
            server_entity_emit_play_sound(host, &call, sound, volume, pitch, emit)
        }
        _ => SoundCallOutcome::Unmodelled,
    }
}

/// The receiver facts `Entity.playSound` / `Player.playSound` hand to `Level.playSound`.
struct EntitySoundCall<'a> {
    is_player: bool,
    player_uuid: Option<u128>,
    dimension: Option<&'a str>,
    position: Option<[f64; 3]>,
    sound_source: Option<SoundSource>,
}

/// `Entity.playSound(SoundEvent, FF)` past its `isSilent()` check, or
/// `Player.playSound(SoundEvent, FF)`.
///
/// Both read `level()`, `getX/Y/Z()` (final: they never throw) and
/// `getSoundSource()` (virtual), then call `Level.playSound(except, x, y, z,
/// sound, source, volume, pitch)`, which draws the level's
/// `soundSeedGenerator` before anything else (see [`server_play`]). `except`
/// is `null` for an entity and the player itself for a player.
fn server_entity_emit_play_sound<H: Host + ?Sized>(
    host: &mut H,
    call: &EntitySoundCall<'_>,
    sound: &Value,
    volume: f32,
    pitch: f32,
    emit: &mut dyn FnMut(SoundRecord),
) -> SoundCallOutcome {
    let Some(dimension) = call.dimension else {
        // The draw happens on whichever level `level()` is: Decaf cannot name it.
        host.mark_sound_stream_unknown(ANY_SERVER_LEVEL_SOUND_STREAM);
        return SoundCallOutcome::Unmodelled;
    };
    let level_marker = server_level_sound_stream(dimension);
    let source = if call.is_player {
        SoundSource::Players
    } else {
        let Some(source) = call.sound_source else {
            // An unknown `getSoundSource()` may be a mod's override that throws
            // before the draw.
            host.mark_sound_stream_unknown(&level_marker);
            return SoundCallOutcome::Unmodelled;
        };
        source
    };
    let Some(seed) = host.draw_level_sound_seed(&level_marker, true) else {
        return SoundCallOutcome::Unmodelled;
    };
    match server_entity_effect(host, call, dimension, source, sound, volume, pitch, seed) {
        Ok(effect) => {
            emit(SoundRecord::Play(effect));
            SoundCallOutcome::Handled(Value::Void)
        }
        Err(outcome) => {
            server_seed_not_played(dimension, seed, emit);
            outcome
        }
    }
}

/// What `ServerLevel.playSeededSound` reads after an entity's draw: `except
/// instanceof Player` (the player itself, or `null`), then the sound.
#[allow(clippy::too_many_arguments)]
fn server_entity_effect<H: Host + ?Sized>(
    host: &H,
    call: &EntitySoundCall<'_>,
    dimension: &str,
    source: SoundSource,
    sound: &Value,
    volume: f32,
    pitch: f32,
    seed: SoundSeed,
) -> Result<LevelSoundEffect, SoundCallOutcome> {
    let pos = call.position.ok_or(SoundCallOutcome::Unmodelled)?;
    let except_player = if call.is_player {
        Some(call.player_uuid.ok_or(SoundCallOutcome::Unmodelled)?)
    } else {
        None
    };
    let resolved = wrap_sound_event_as_holder(host, sound).ok_or(SoundCallOutcome::Unmodelled)?;
    Ok(LevelSoundEffect {
        side: SoundSide::Server,
        dimension: Some(dimension.to_owned()),
        sound_id: resolved.sound_id,
        fixed_range: resolved.fixed_range,
        holder: resolved.holder,
        target: SoundTarget::Position(pos),
        source,
        volume,
        pitch,
        except_player,
        seed,
        distance_delay: false,
        delay_ticks: 0,
    })
}

/// Dispatch `LocalPlayer.playSound`, `getSoundSource`, and `isSilent`.
fn execute_local_player_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
    frame: Option<&client_surface::ClientSnapshot>,
    emit: &mut dyn FnMut(&mut StdHost, LevelSoundEffect),
) -> SoundCallOutcome {
    let facts = match frame {
        Some(frame) => frame.player.as_ref().map(|player| {
            (
                frame.level.is_some(),
                frame
                    .level
                    .as_ref()
                    .map(|l| l.dimension.clone())
                    .filter(|d| !d.is_empty()),
                Some(player.pos),
                player.silent,
            )
        }),
        None => host
            .level_sound
            .standalone_client_player
            .as_ref()
            .map(|facts| {
                (
                    facts.level.is_some(),
                    facts.level.clone(),
                    facts.position,
                    facts.silent,
                )
            }),
    };
    let Some((has_level, dimension, pos, silent)) = facts else {
        if r.name == "playSound" {
            // A local player Decaf has no facts for may or may not reach
            // `playLocalSound`'s `Level.random` draw.
            host.mark_sound_stream_unknown(&level_random_stream(client_surface::LEVEL));
        }
        return SoundCallOutcome::Unmodelled;
    };
    let player = LocalPlayerSound {
        has_level,
        dimension,
        pos,
    };
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("getSoundSource", "()Lnet/minecraft/sounds/SoundSource;") => {
            SoundCallOutcome::Handled(SoundSource::Players.to_value())
        }
        ("isSilent", "()Z") => match silent {
            Some(silent) => SoundCallOutcome::Handled(Value::Int(i32::from(silent))),
            None => SoundCallOutcome::Unmodelled,
        },
        // `Entity.playSound(SoundEvent)` checks `!isSilent()`, then calls
        // virtual `LocalPlayer.playSound(sound, 1.0f, 1.0f)`.
        ("playSound", "(Lnet/minecraft/sounds/SoundEvent;)V") => {
            let Some(is_silent) = silent else {
                host.mark_sound_stream_unknown(&level_random_stream(client_surface::LEVEL));
                return SoundCallOutcome::Unmodelled;
            };
            if is_silent {
                return SoundCallOutcome::Handled(Value::Void);
            }
            let Some(sound) = args.first() else {
                return SoundCallOutcome::Unmodelled;
            };
            local_player_emit_play_sound(host, player, sound, 1.0, 1.0, emit)
        }
        // `LocalPlayer.playSound(SoundEvent, float, float)` does NOT check `isSilent()`,
        // and calls `level.playLocalSound(getX(), getY(), getZ(), sound, getSoundSource(), vol, pitch, false)`.
        ("playSound", "(Lnet/minecraft/sounds/SoundEvent;FF)V") => {
            let (Some(sound), Some(volume), Some(pitch)) =
                (args.first(), float_arg(args, 1), float_arg(args, 2))
            else {
                return SoundCallOutcome::Unmodelled;
            };
            local_player_emit_play_sound(host, player, sound, volume, pitch, emit)
        }
        _ => SoundCallOutcome::Unmodelled,
    }
}

/// The local player facts `LocalPlayer.playSound` reads before its draw.
struct LocalPlayerSound {
    has_level: bool,
    dimension: Option<String>,
    pos: Option<[f64; 3]>,
}

/// `LocalPlayer.playSound(SoundEvent, FF)`: `level().playLocalSound(getX(),
/// getY(), getZ(), sound, getSoundSource(), volume, pitch, false)`. The
/// position reads are final and never throw; `playLocalSound` then draws
/// `Level.random` before anything reads the sound.
fn local_player_emit_play_sound(
    host: &mut StdHost,
    player: LocalPlayerSound,
    sound: &Value,
    volume: f32,
    pitch: f32,
    emit: &mut dyn FnMut(&mut StdHost, LevelSoundEffect),
) -> SoundCallOutcome {
    if !player.has_level {
        // A client level Decaf does not model: the draw may or may not happen.
        host.mark_sound_stream_unknown(&level_random_stream(client_surface::LEVEL));
        return SoundCallOutcome::Unmodelled;
    }
    let Some(seed) = host.draw_client_local_seed() else {
        return SoundCallOutcome::Unmodelled;
    };
    let effect = local_player_effect(player, sound, volume, pitch, seed);
    finish_client_local(host, seed, effect, emit)
}

fn local_player_effect(
    player: LocalPlayerSound,
    sound: &Value,
    volume: f32,
    pitch: f32,
    seed: SoundSeed,
) -> Result<LevelSoundEffect, SoundCallOutcome> {
    let pos = player.pos.ok_or(SoundCallOutcome::Unmodelled)?;
    let (sound_id, fixed_range) =
        resolve_direct_sound_event(sound).ok_or(SoundCallOutcome::Unmodelled)?;
    Ok(LevelSoundEffect {
        side: SoundSide::Client,
        dimension: player.dimension,
        sound_id,
        fixed_range,
        holder: None,
        target: SoundTarget::Position(pos),
        source: SoundSource::Players,
        volume,
        pitch,
        except_player: None,
        seed,
        distance_delay: false,
        delay_ticks: 0,
    })
}

/// Entry point for `client_surface.rs` inside an active `ClientSnapshot` frame.
pub(crate) fn client_frame_call(
    host: &mut StdHost,
    object: &str,
    frame: &client_surface::ClientSnapshot,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let outcome = match object {
        client_surface::LEVEL if is_level_sound_method_name(&r.name) => {
            frame.level.as_ref()?;
            let mut recorded = Vec::new();
            let outcome =
                execute_client_level_call(host, r, args, Some(frame), &mut |_, effect| {
                    recorded.push(effect);
                });
            host.client.sounds.extend(recorded);
            outcome
        }
        client_surface::PLAYER if is_entity_sound_method_name(&r.name) => {
            frame.player.as_ref()?;
            let mut recorded = Vec::new();
            let outcome =
                execute_local_player_call(host, r, args, Some(frame), &mut |_, effect| {
                    recorded.push(effect);
                });
            host.client.sounds.extend(recorded);
            outcome
        }
        _ => return None,
    };
    Some(match outcome {
        SoundCallOutcome::Handled(value) => Ok(value),
        SoundCallOutcome::Exception(err) => Err(err),
        SoundCallOutcome::Unmodelled => Err(host.refuse_client_sound_call(r)),
    })
}

/// Entry point for `events.rs` (both inside an active `EventFiring` and for
/// standalone `game/level/<dim>`, `game/player/...`, `game/entity/...` handles).
pub(crate) fn server_facade_sound_call(
    host: &mut StdHost,
    marker: &str,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let in_firing = host.game_event.is_some();
    if let Some(dimension) = marker.strip_prefix("game/level/") {
        if !is_level_sound_method_name(&r.name) {
            return None;
        }
        let firing_snapshot = host.game_event.as_deref().cloned();
        let host_facts = host.level_sound.entity_facts.clone();
        let mut recorded = Vec::new();
        let outcome = execute_server_level_call(
            host,
            marker,
            dimension,
            r,
            args,
            firing_snapshot.as_ref(),
            None,
            &host_facts,
            &mut |effect| recorded.push(effect),
        );
        if let Some(firing) = host.game_event.as_deref_mut() {
            firing.sounds.extend(recorded);
        } else {
            host.level_sound.played_sounds.extend(recorded);
        }
        return Some(finish_server_outcome(host, in_firing, r, outcome));
    }

    if !is_entity_sound_method_name(&r.name) {
        return None;
    }

    if let Some(uuid) = game_object::player_uuid(marker) {
        let firing_snapshot = host.game_event.as_deref().cloned();
        let extra = firing_snapshot
            .as_ref()
            .and_then(|f| f.context.entity_sound_facts.get(marker))
            .or_else(|| host.level_sound.entity_facts.get(marker))
            .cloned();
        let facade_level = firing_snapshot
            .as_ref()
            .and_then(|f| game_object::facade_player(&f.facade, uuid))
            .and_then(|p| p.level.clone());
        let dimension = extra
            .as_ref()
            .and_then(|e| e.level.clone())
            .or(facade_level);
        let position = extra.as_ref().and_then(|e| e.position);
        let silent = extra.as_ref().and_then(|e| e.silent);
        let mut recorded = Vec::new();
        let outcome = execute_server_entity_call(
            host,
            r,
            args,
            true,
            Some(uuid),
            dimension.as_deref(),
            position,
            silent,
            Some(SoundSource::Players),
            &mut |effect| recorded.push(effect),
        );
        if let Some(firing) = host.game_event.as_deref_mut() {
            firing.sounds.extend(recorded);
        } else {
            host.level_sound.played_sounds.extend(recorded);
        }
        return Some(finish_server_outcome(host, in_firing, r, outcome));
    }

    if let Some(id_str) = marker.strip_prefix("game/entity/")
        && let Ok(entity_id) = id_str.parse::<i32>()
    {
        let firing_snapshot = host.game_event.as_deref().cloned();
        let extra = firing_snapshot
            .as_ref()
            .and_then(|f| f.context.entity_sound_facts.get(marker))
            .or_else(|| host.level_sound.entity_facts.get(marker))
            .cloned();
        let facade_entity = firing_snapshot
            .as_ref()
            .and_then(|f| f.context.entity.as_ref())
            .filter(|e| e.id == entity_id);
        let dimension = extra
            .as_ref()
            .and_then(|e| e.level.clone())
            .or_else(|| facade_entity.and_then(|e| e.level.clone()));
        let position = extra.as_ref().and_then(|e| e.position);
        let silent = extra.as_ref().and_then(|e| e.silent);
        let sound_source = extra
            .as_ref()
            .and_then(|e| e.sound_source)
            .or_else(|| facade_entity.and_then(|e| vanilla_entity_sound_source(&e.kind)));
        let mut recorded = Vec::new();
        let outcome = execute_server_entity_call(
            host,
            r,
            args,
            false,
            None,
            dimension.as_deref(),
            position,
            silent,
            sound_source,
            &mut |effect| recorded.push(effect),
        );
        if let Some(firing) = host.game_event.as_deref_mut() {
            firing.sounds.extend(recorded);
        } else {
            host.level_sound.played_sounds.extend(recorded);
        }
        return Some(finish_server_outcome(host, in_firing, r, outcome));
    }

    None
}

fn finish_server_outcome(
    host: &mut StdHost,
    in_firing: bool,
    r: &MethodRef,
    outcome: SoundCallOutcome,
) -> Result<Value, NativeError> {
    match outcome {
        SoundCallOutcome::Handled(value) => Ok(value),
        SoundCallOutcome::Exception(err) => Err(err),
        SoundCallOutcome::Unmodelled => {
            if in_firing {
                let _ = host.unknown_host_call(r);
                Err(NativeError::UnknownHostCall(r.clone()))
            } else {
                host.unknown_host_call(r)
            }
        }
    }
}

/// Entry point for standalone `client/level` and `client/player` handles outside
/// an active `ClientSnapshot` frame (e.g. direct `NativeJar::invoke_method` fixtures).
pub(crate) fn standalone_client_sound_call(
    host: &mut StdHost,
    receiver: &Value,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if host.client.frame.is_some() {
        return None;
    }
    let Value::Host(handle) = receiver else {
        return None;
    };
    let key = handle.key();
    let outcome = match (handle.kind(), key) {
        (HostKind::GameLevel, client_surface::LEVEL)
            if r.name == "isClientSide" && r.descriptor == "()Z" =>
        {
            SoundCallOutcome::Handled(Value::Int(1))
        }
        (HostKind::GameLevel, client_surface::LEVEL) if is_level_sound_method_name(&r.name) => {
            execute_client_level_call(host, r, args, None, &mut |host, effect| {
                host.level_sound
                    .played_sounds
                    .push(SoundRecord::Play(effect));
            })
        }
        (HostKind::GameEntity, client_surface::PLAYER)
            if matches!(r.name.as_str(), "getX" | "getY" | "getZ") && r.descriptor == "()D" =>
        {
            let axis = match r.name.as_str() {
                "getX" => 0,
                "getY" => 1,
                _ => 2,
            };
            match host
                .level_sound
                .standalone_client_player
                .as_ref()
                .and_then(|facts| facts.position)
            {
                Some(pos) => SoundCallOutcome::Handled(Value::Float(pos[axis])),
                None => SoundCallOutcome::Unmodelled,
            }
        }
        (HostKind::GameEntity, client_surface::PLAYER) if is_entity_sound_method_name(&r.name) => {
            execute_local_player_call(host, r, args, None, &mut |host, effect| {
                host.level_sound
                    .played_sounds
                    .push(SoundRecord::Play(effect));
            })
        }
        _ => return None,
    };
    Some(match outcome {
        SoundCallOutcome::Handled(value) => Ok(value),
        SoundCallOutcome::Exception(err) => Err(err),
        SoundCallOutcome::Unmodelled => host.unknown_host_call(r),
    })
}

/// Entry point for `game_hooks.rs::GameHookHost::answer`.
pub(crate) fn game_hook_sound_call<H: Host>(
    inner: &mut H,
    context: &mut game_hooks::GameHookContext,
    marker: &str,
    object: &game_hooks::GameObject,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let empty_facts = BTreeMap::new();
    match object.kind {
        game_hooks::GameObjectKind::Level if is_level_sound_method_name(&r.name) => {
            let dimension = object.dimension.as_deref()?;
            let mut recorded = Vec::new();
            let outcome = execute_server_level_call(
                inner,
                marker,
                dimension,
                r,
                args,
                None,
                Some(context),
                &empty_facts,
                &mut |effect| recorded.push(effect),
            );
            for record in recorded {
                context.push_effect(game_hooks::GameHookEffect::from_sound_record(record));
            }
            Some(finish_game_hook_outcome(inner, marker, r, outcome))
        }
        game_hooks::GameObjectKind::Player
        | game_hooks::GameObjectKind::Living
        | game_hooks::GameObjectKind::Entity
        | game_hooks::GameObjectKind::ItemEntity
            if is_entity_sound_method_name(&r.name) =>
        {
            let is_player = object.kind == game_hooks::GameObjectKind::Player;
            let sound_source = if is_player {
                Some(SoundSource::Players)
            } else if object.kind == game_hooks::GameObjectKind::ItemEntity {
                Some(SoundSource::Ambient)
            } else {
                object.sound_source.or_else(|| {
                    object
                        .type_id
                        .as_deref()
                        .and_then(vanilla_entity_sound_source)
                })
            };
            let mut recorded = Vec::new();
            let outcome = execute_server_entity_call(
                inner,
                r,
                args,
                is_player,
                object.uuid,
                object.dimension.as_deref(),
                object.position,
                object.silent,
                sound_source,
                &mut |effect| recorded.push(effect),
            );
            for record in recorded {
                context.push_effect(game_hooks::GameHookEffect::from_sound_record(record));
            }
            Some(finish_game_hook_outcome(inner, marker, r, outcome))
        }
        _ => None,
    }
}

fn finish_game_hook_outcome<H: Host>(
    inner: &mut H,
    marker: &str,
    r: &MethodRef,
    outcome: SoundCallOutcome,
) -> Result<Value, NativeError> {
    match outcome {
        SoundCallOutcome::Handled(value) => Ok(value),
        SoundCallOutcome::Exception(err) => Err(err),
        SoundCallOutcome::Unmodelled => {
            let _ = inner.unknown_operand_call(r);
            Err(NativeError::Metadata(format!(
                "game hook object {marker} does not model {}#{}{}",
                r.owner, r.name, r.descriptor
            )))
        }
    }
}

impl StdHost {
    /// Handle for a server level of `dimension` (`game/level/<dimension>`).
    pub fn server_level_handle(dimension: &str) -> Value {
        game_object::server_value(format!("game/level/{dimension}"))
    }

    /// Handle for the client level (`client/level`).
    pub fn client_level_handle() -> Value {
        game_object::client_value(client_surface::LEVEL.to_owned())
    }

    /// Handle for the local client player (`client/player`).
    pub fn client_player_handle() -> Value {
        game_object::client_value(client_surface::PLAYER.to_owned())
    }

    /// Seed `Level.random` (`Level.getRandom()`, and the seed stream used by
    /// `ClientLevel.playLocalSound` / `playPlayerSound`) for `level_marker`.
    ///
    /// Seeding makes the stream known again (see the module docs).
    pub fn seed_level_random(&mut self, level_marker: &str, seed: i64) {
        // Out of random source numbers (counted): the stream stays as it was.
        let Some(handle) = self.make_random_handle(random_source::legacy_state(seed)) else {
            return;
        };
        self.level_sound
            .level_random_handles
            .insert(level_marker.to_owned(), handle);
        self.level_sound
            .unknown_sound_streams
            .remove(&level_random_stream(level_marker));
    }

    /// Seed `Level.soundSeedGenerator` (used by non-seeded `Level.playSound`
    /// overloads on both `ServerLevel` and `ClientLevel`) for `level_marker`.
    ///
    /// Seeding makes the stream known again, except while
    /// [`ANY_SERVER_LEVEL_SOUND_STREAM`] is unknown: that stands for every
    /// server level until [`Self::forget_unknown_server_sound_streams`].
    pub fn seed_level_sound_seeds(&mut self, level_marker: &str, seed: i64) {
        self.level_sound
            .level_sound_seeds
            .insert(level_marker.to_owned(), random_source::legacy_state(seed));
        self.level_sound.unknown_sound_streams.remove(level_marker);
    }

    /// Seed both `random` and `soundSeedGenerator` for a server level dimension.
    pub fn seed_server_level_randoms(
        &mut self,
        dimension: &str,
        random_seed: i64,
        sound_seed: i64,
    ) {
        let marker = format!("game/level/{dimension}");
        self.seed_level_random(&marker, random_seed);
        self.seed_level_sound_seeds(&marker, sound_seed);
    }

    /// Seed `soundSeedGenerator` only for a server level dimension.
    pub fn seed_server_level_sound_seeds(&mut self, dimension: &str, sound_seed: i64) {
        self.seed_level_sound_seeds(&format!("game/level/{dimension}"), sound_seed);
    }

    /// Seed `random` only for a server level dimension.
    pub fn seed_server_level_random(&mut self, dimension: &str, random_seed: i64) {
        self.seed_level_random(&format!("game/level/{dimension}"), random_seed);
    }

    /// Seed both `random` and `soundSeedGenerator` for `ClientLevel`.
    pub fn seed_client_level_randoms(&mut self, random_seed: i64, sound_seed: i64) {
        self.seed_level_random(client_surface::LEVEL, random_seed);
        self.seed_level_sound_seeds(client_surface::LEVEL, sound_seed);
    }

    /// Seed `random` only for `ClientLevel`.
    pub fn seed_client_level_random(&mut self, random_seed: i64) {
        self.seed_level_random(client_surface::LEVEL, random_seed);
    }

    /// Seed `soundSeedGenerator` only for `ClientLevel`.
    pub fn seed_client_level_sound_seeds(&mut self, sound_seed: i64) {
        self.seed_level_sound_seeds(client_surface::LEVEL, sound_seed);
    }

    /// Allow server-side `Level.playSound` to defer its `soundSeedGenerator`
    /// draw to a live host (such as Sunrise's `LevelRef::play_sound`, which
    /// draws from the server's real `sound_seeds` stream). Off by default so
    /// unseeded standalone runs count unreachable random draws.
    pub fn set_host_deferred_server_sound_seeds(&mut self, enabled: bool) {
        self.level_sound.host_deferred_server_seeds = enabled;
    }

    /// Allow client-side sound calls to defer their seed draw to a live client
    /// host (`ClientHost::play_local_sound`). Off by default.
    pub fn set_host_deferred_client_sound_seeds(&mut self, enabled: bool) {
        self.level_sound.host_deferred_client_seeds = enabled;
    }

    /// Mark a level sound stream ([`server_level_sound_stream`],
    /// [`ANY_SERVER_LEVEL_SOUND_STREAM`], [`CLIENT_LEVEL_SOUND_STREAM`],
    /// [`level_random_stream`]) unknown: Java drew, or may have drawn, from it
    /// where neither Decaf nor the live host did. Every later draw from it
    /// counts until it is seeded again.
    ///
    /// A seeded copy of the stream stops answering too: a `soundSeedGenerator`
    /// copy is dropped, and a seeded `Level.random` handle (which the jar may
    /// hold through `getRandom()`) forgets its state, so its draws count.
    ///
    /// While seeds are deferred to a live host, the mark widens to every
    /// stream the host's one stream may stand for ([`host_sound_streams`]).
    pub fn mark_sound_stream_unknown(&mut self, stream: &str) {
        let deferred = if is_server_seed_generator(stream) {
            self.level_sound.host_deferred_server_seeds
        } else if stream.starts_with(client_surface::LEVEL) {
            self.level_sound.host_deferred_client_seeds
        } else {
            false
        };
        if deferred {
            for stream in host_sound_streams(stream) {
                self.mark_one_sound_stream_unknown(&stream);
            }
        } else {
            self.mark_one_sound_stream_unknown(stream);
        }
    }

    fn mark_one_sound_stream_unknown(&mut self, stream: &str) {
        if let Some(level_marker) = stream.strip_suffix(LEVEL_RANDOM_SUFFIX) {
            if let Some(handle) = self
                .level_sound
                .level_random_handles
                .get(level_marker)
                .cloned()
            {
                self.forget_random_state(&handle);
            }
        } else if stream == ANY_SERVER_LEVEL_SOUND_STREAM {
            self.level_sound
                .level_sound_seeds
                .retain(|marker, _| !marker.starts_with(SERVER_LEVEL_PREFIX));
        } else {
            self.level_sound.level_sound_seeds.remove(stream);
        }
        self.level_sound
            .unknown_sound_streams
            .insert(stream.to_owned());
    }

    /// Whether a draw from `stream` can still be answered exactly.
    pub fn sound_stream_known(&self, stream: &str) -> bool {
        sound_stream_known_in(&self.level_sound.unknown_sound_streams, stream)
    }

    /// The stream keys marked unknown on this host.
    pub fn unknown_sound_streams(&self) -> &BTreeSet<String> {
        &self.level_sound.unknown_sound_streams
    }

    /// A new server opens new levels, each with a fresh `soundSeedGenerator`
    /// (and the live host's streams start fresh with them): forget which
    /// server streams were unknown. A seeded copy dropped when its stream
    /// went unknown stays dropped.
    pub fn forget_unknown_server_sound_streams(&mut self) {
        self.level_sound
            .unknown_sound_streams
            .retain(|stream| !stream.starts_with(SERVER_LEVEL_PREFIX));
    }

    /// A new client level has a fresh `random` and `soundSeedGenerator`:
    /// forget which client streams were unknown.
    pub fn forget_unknown_client_sound_streams(&mut self) {
        self.level_sound
            .unknown_sound_streams
            .retain(|stream| !stream.starts_with(client_surface::LEVEL));
    }

    /// Record optional sound facts for an entity or player marker (used by
    /// standalone calls and host bridges).
    pub fn set_entity_sound_facts(&mut self, marker: impl Into<String>, facts: EntitySoundFacts) {
        self.level_sound.entity_facts.insert(marker.into(), facts);
    }

    /// Clear all standalone entity sound facts.
    pub fn clear_entity_sound_facts(&mut self) {
        self.level_sound.entity_facts.clear();
    }

    /// Configure standalone `ClientLevel` camera position (outside a `ClientSnapshot` frame).
    pub fn set_standalone_client_camera(&mut self, pos: Option<[f64; 3]>) {
        self.level_sound.standalone_client_camera = pos;
    }

    /// Configure standalone `ClientLevel` local player facts (outside a `ClientSnapshot` frame).
    pub fn set_standalone_client_player(&mut self, player: Option<EntitySoundFacts>) {
        self.level_sound.standalone_client_player = player;
    }

    /// Standalone sounds recorded outside an `EventFiring`, `GameHookContext`,
    /// or `ClientSnapshot` frame, in call order (without the server seed
    /// draws that played nothing: [`Self::sound_records`] has those too).
    pub fn played_sounds(&self) -> Vec<LevelSoundEffect> {
        self.level_sound
            .played_sounds
            .iter()
            .filter_map(SoundRecord::effect)
            .cloned()
            .collect()
    }

    /// The standalone sound log: every recorded sound and every server seed
    /// draw deferred to the live host, in call order.
    pub fn sound_records(&self) -> &[SoundRecord] {
        &self.level_sound.played_sounds
    }

    /// Drain the standalone sound log ([`Self::sound_records`]). A server
    /// bridge drains this one, so it can replay every deferred draw.
    pub fn take_sound_records(&mut self) -> Vec<SoundRecord> {
        std::mem::take(&mut self.level_sound.played_sounds)
    }

    /// Drain standalone recorded sounds, dropping any server seed draw that
    /// played nothing. For client drains (a client log holds only sounds) and
    /// tests; a server bridge uses [`Self::take_sound_records`].
    pub fn take_played_sounds(&mut self) -> Vec<LevelSoundEffect> {
        self.take_sound_records()
            .into_iter()
            .filter_map(SoundRecord::into_effect)
            .collect()
    }

    /// Draw the client level's `Level.random.nextLong()` where Java does
    /// (`ClientLevel.playLocalSound`, `playPlayerSound`). `None` counts the
    /// call: the stream was already unknown, or Decaf cannot make this draw,
    /// which makes it unknown (Java drew all the same).
    pub(crate) fn draw_client_local_seed(&mut self) -> Option<SoundSeed> {
        let stream = level_random_stream(client_surface::LEVEL);
        let seed = if !self.sound_stream_known(&stream) {
            None
        } else if let Some(handle) = self
            .level_sound
            .level_random_handles
            .get(client_surface::LEVEL)
            .cloned()
        {
            // A seeded handle is the stream: no deferring past it.
            self.draw_random_handle_long(&handle)
                .map(SoundSeed::LevelRandom)
        } else {
            self.level_sound
                .host_deferred_client_seeds
                .then_some(SoundSeed::HostDeferred)
        };
        if seed.is_none() {
            self.mark_sound_stream_unknown(&stream);
        }
        seed
    }

    /// Draw `level_marker`'s `soundSeedGenerator.nextLong()` where Java does,
    /// from a seeded copy or by deferring to a live host. `None` counts the
    /// call, and the stream is (or stays) unknown: Java drew all the same.
    pub(crate) fn draw_level_sound_seed_impl(
        &mut self,
        level_marker: &str,
        server_side: bool,
    ) -> Option<SoundSeed> {
        let seed = if !self.sound_stream_known(level_marker) {
            None
        } else if let Some(state) = self.level_sound.level_sound_seeds.get_mut(level_marker) {
            Some(SoundSeed::LevelSeedGenerator(
                random_source::draw_next_long(state),
            ))
        } else {
            let deferred = if server_side {
                self.level_sound.host_deferred_server_seeds
            } else {
                self.level_sound.host_deferred_client_seeds
            };
            deferred.then_some(SoundSeed::HostDeferred)
        };
        if seed.is_none() {
            self.mark_sound_stream_unknown(level_marker);
        }
        seed
    }

    pub(crate) fn level_random_handle_impl(&self, level_marker: &str) -> Option<Value> {
        self.level_sound
            .level_random_handles
            .get(level_marker)
            .cloned()
    }

    pub(crate) fn registered_sound_id_impl(&self, sound_value: &Value) -> Option<String> {
        self.registrations
            .iter()
            .rev()
            .find(|entry| entry.registry == "minecraft:sound_event" && entry.value == *sound_value)
            .map(|entry| entry.id.clone())
    }

    pub(crate) fn refuse_client_sound_call(&mut self, r: &MethodRef) -> NativeError {
        let key = format!("{}#{}{}", r.owner, r.name, r.descriptor);
        *self.unknown_host_calls.entry(key).or_default() += 1;
        NativeError::UnknownHostCall(r.clone())
    }
}

/// Fast check for whether `r` takes a `BlockPos` argument that may be a
/// jar-allocated `BlockPos` / `MutableBlockPos` / `Vec3i` object.
#[inline]
pub(crate) fn needs_block_pos_resolution(r: &MethodRef, args: &[Value]) -> bool {
    let pos_index = if r.name == "playSound"
        && r.descriptor
            .starts_with("(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/BlockPos;")
    {
        1
    } else if r.name == "playLocalSound"
        && r.descriptor.starts_with("(Lnet/minecraft/core/BlockPos;")
    {
        0
    } else {
        return false;
    };
    args.get(pos_index)
        .is_some_and(crate::jar_object::is_object)
}

/// Resolve a jar-constructed `BlockPos` / `MutableBlockPos` / `Vec3i` argument
/// in `Level.playSound` / `Level.playLocalSound` into a `HostKind::GameValue`
/// `block-pos/x,y,z` handle before forwarding to the host.
///
/// Kept `#[inline(never)]` so `Runtime::invoke_frame` does not grow its stack.
#[inline(never)]
pub(super) fn resolve_block_pos_arg<H: Host>(
    runtime: &crate::Runtime<'_, H>,
    r: &MethodRef,
    args: &mut [Value],
) {
    let pos_index = if r.name == "playSound"
        && r.descriptor
            .starts_with("(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/BlockPos;")
    {
        1
    } else if r.name == "playLocalSound"
        && r.descriptor.starts_with("(Lnet/minecraft/core/BlockPos;")
    {
        0
    } else {
        return;
    };
    let Some(slot) = args.get_mut(pos_index) else {
        return;
    };
    if !crate::jar_object::is_object(slot) {
        return;
    }
    if let Some(Some(key)) = runtime.jar_server_value_key(slot)
        && key.starts_with(crate::server_values::BLOCK_POS)
        && let Some(handle) = game_object::value(&key)
    {
        *slot = handle;
    }
}
