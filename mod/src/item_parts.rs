//! Generic Sunrise `ItemPart` bridge for a Java mod item's own overrides
//! (`docs/java-item-parts.md`, steps 3-6).
//!
//! One [`JavaItemPart`] is attached to every jar item whose class chain declares one of the
//! hooks the runtime runs ([`runtime::ItemHook`]): `item_stacked_on_other` runs
//! `overrideStackedOnOther` / `overrideOtherStackedOnMe`, `use_item` runs `use`, `item_bar` runs
//! `isBarVisible` / `getBarWidth` / `getBarColor`, `item_destroyed` runs `onDestroyed`, and
//! `tooltip_image` runs `getTooltipImage` (`client::item_models`, step 8). The
//! Java call gets the runtime's façades (`runtime/item_parts.rs`); its stacks cross with their
//! jar data components as the NBT the type's own codec writes.
//!
//! Nothing here names a mod. A call that counted (an unmodelled host call, a slot whose rules
//! this hook can't read, a client prediction) leaves Sunrise's vanilla answer standing and is
//! counted on the jar, as every bridge here does. A Java call that *failed* (it threw, ran out
//! of budget, or hit a call the runtime refused) is logged and counted too ([`note_failed`]),
//! but it has no answer to stand on: a click is refused (nothing moves) and a use answers
//! `FAIL`, never Sunrise's vanilla answer as if the override had returned `false` or `PASS`.

use std::sync::{Arc, Mutex};

use mod_api::{
    ClickedStackResult, Flow, Hand, HostSide, InteractionResult, ItemBarEvent, ItemDestroyedEvent,
    ItemPart, ItemStack, ItemStackedOnOtherEvent, MenuSlotBacking, PlayerRef, ServerHost,
    StackResult, StackedOnOther, StackedOnOtherDirection, StackedOnOtherResult, UseItemEvent,
};

use crate::level_random::run_with_level_random;
use crate::runtime::{
    ExecutionBudget, GAME_PROTOTYPE_COMPONENTS, ItemClick, ItemClickDirection, ItemHook, ItemUse,
    LiveMenuEffect, NativeJar, StdHost, Value,
};
use crate::{JAR_BUDGET, LOG, State, caught, runtime};

/// Player inventory indices a plain `Slot` shows in a vanilla menu: `Inventory.items` 0..36
/// (the armour 36..40 and off hand 40 slots of `InventoryMenu` are slot subclasses).
const PLAIN_INVENTORY_SLOTS: usize = 36;

/// `Container.getMaxStackSize()` (99), which `Slot.getMaxStackSize()` answers for a slot over
/// the player's `Inventory`.
const INVENTORY_MAX_STACK_SIZE: u32 = 99;

/// The Sunrise `ItemPart` of one jar item.
pub(crate) struct JavaItemPart {
    state: Arc<Mutex<State>>,
    client: Arc<Mutex<crate::client::State>>,
    item_id: String,
}

impl JavaItemPart {
    pub(crate) fn new(
        state: Arc<Mutex<State>>,
        client: Arc<Mutex<crate::client::State>>,
        item_id: String,
    ) -> Self {
        Self {
            state,
            client,
            item_id,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The jar that registered `item_id`, its `Item` object, and the class declaring `hook`'s
/// override (`None` when no jar class of the item's chain declares it).
fn jar_item(state: &State, item_id: &str, hook: ItemHook) -> Option<(usize, Value, String)> {
    state.jars.iter().enumerate().find_map(|(index, jar)| {
        let item = jar
            .host
            .registrations()
            .iter()
            .find(|entry| entry.registry == "minecraft:item" && entry.id == item_id)?
            .value
            .clone();
        let owner = jar
            .jar
            .item_overrides(&item, &jar.host)
            .owners
            .get(&hook)?
            .clone();
        Some((index, item, owner))
    })
}

/// The jar data component types whose values cross as NBT: the ones the jar built and
/// registered with a persistent codec (`StdHost::jar_component_types`).
pub(crate) fn persistent_jar_types(host: &StdHost) -> Vec<String> {
    host.jar_component_types()
        .into_iter()
        .filter(|kind| kind.persistent)
        .map(|kind| kind.id)
        .collect()
}

/// The jar-aware runtime stack for a game stack: jar data components decoded by their own codec,
/// the prototype values of those types and of [`GAME_PROTOTYPE_COMPONENTS`] read from the game.
/// A new stack each call. Every caller builds it inside a transient scope
/// (`StdHost::begin_transient_rows`) and is done with it when the scope closes.
pub(crate) fn java_stack(
    host: &dyn mod_api::Host,
    jar: &NativeJar,
    jar_host: &mut StdHost,
    item_override: Option<Value>,
    stack: &ItemStack,
    jar_types: &[String],
) -> Option<Value> {
    let request = java_stack_request(host, item_override, stack, jar_types)?;
    jar.build_item_stack(jar_host, request).ok()
}

/// Everything a Java stack for `stack` is built from (`runtime::ItemStackRequest`).
fn java_stack_request(
    host: &dyn mod_api::Host,
    item_override: Option<Value>,
    stack: &ItemStack,
    jar_types: &[String],
) -> Option<runtime::ItemStackRequest> {
    let parts = crate::client::stack_parts(host, stack, jar_types)?;
    let defaults = |item: mod_api::ItemKey| {
        jar_types
            .iter()
            .map(String::as_str)
            .chain(GAME_PROTOTYPE_COMPONENTS.iter().copied())
            .map(|id| {
                let nbt = host
                    .item_default_component(item, id)
                    .and_then(|tag| crate::attachments::tag_to_runtime(&tag, 0).ok());
                (id.to_owned(), nbt)
            })
            .collect::<Vec<_>>()
    };
    // The items a jar component's stored value names (a bundle's `ItemStackTemplate`s): the
    // type's codec reads their prototypes while it decodes (a bundle's occupancy asks each one's
    // max stack size and contents). Each string that is a registered item's id gets the game's
    // values; one that only looks like an id adds a fact nothing reads.
    let mut named = std::collections::BTreeSet::new();
    for (_, entry) in &parts.patch {
        if let Some(runtime::RuntimeComponentValue::Nbt(nbt)) = entry {
            item_ids_in(nbt, &mut named);
        }
    }
    let named_prototypes = named
        .into_iter()
        .filter_map(|id| {
            let key = mod_api::Identifier::parse(&id)
                .ok()
                .and_then(|parsed| host.item_key(&parsed))?;
            Some((id, defaults(key)))
        })
        .collect();
    Some(runtime::ItemStackRequest {
        item_override,
        item_id: parts.id,
        count: parts.count,
        patch: parts.patch,
        defaults: defaults(stack.item()),
        opaque_components: parts.opaque_components,
        opaque_keys: parts.opaque_keys,
        named_prototypes,
    })
}

/// Every string in `nbt` that could be an item id (`namespace:path`), at any depth.
fn item_ids_in(nbt: &runtime::Nbt, out: &mut std::collections::BTreeSet<String>) {
    match nbt {
        runtime::Nbt::String(text) if text.contains(':') => {
            out.insert(text.clone());
        }
        runtime::Nbt::List(items) => items.iter().for_each(|item| item_ids_in(item, out)),
        runtime::Nbt::Compound(fields) => fields.values().for_each(|field| item_ids_in(field, out)),
        _ => {}
    }
}

/// A runtime stack as a game stack, its jar components encoded: `Some(None)` for an empty one,
/// `None` when the mod API can't carry it.
fn game_stack(
    host: &dyn mod_api::Host,
    jar: &NativeJar,
    jar_host: &mut StdHost,
    value: &Value,
) -> Option<Option<ItemStack>> {
    let parts = jar.item_stack_components(jar_host, value).ok()??;
    if parts.count <= 0 || parts.item_id == "minecraft:air" {
        return Some(None);
    }
    crate::client::api_stack_from_components(host, parts).map(Some)
}

/// The player-inventory index the clicked menu slot shows, when it is a plain vanilla `Slot`
/// whose rules this hook can read: the player's open menu is their inventory menu or a vanilla
/// menu type, the slot is backed by `Inventory.items`, and Sunrise's own slot rules
/// (`MenuRef::slot_gives` / `slot_holds`, which a menu part may change) agree with a plain
/// slot's for the stacks the click holds. `None` otherwise: the click counts.
fn plain_inventory_slot(
    host: &dyn ServerHost,
    player: PlayerRef,
    slot: usize,
    stacks: [Option<&ItemStack>; 2],
) -> Option<usize> {
    let menu = player.container_menu(host)?;
    match menu.kind(host)? {
        None => {}
        Some(kind) if kind.starts_with("minecraft:") => {}
        Some(_) => return None,
    }
    let MenuSlotBacking::Player(index) = menu.slot_backing(host, slot)? else {
        return None;
    };
    if index >= PLAIN_INVENTORY_SLOTS || !menu.slot_gives(host, slot)? {
        return None;
    }
    for stack in stacks.into_iter().flatten() {
        let item_max = host.max_stack_size(stack.item())?;
        if menu.slot_holds(host, slot, stack)? != item_max.min(INVENTORY_MAX_STACK_SIZE) {
            return None;
        }
    }
    Some(index)
}

/// One Java call of this bridge that failed: an exception Java didn't catch, a budget, a
/// refused bytecode, a host call the runtime refused to answer (`ItemCallReport::failure`), or
/// the runtime's own error or panic around the call. Logged with what it said, kept in the
/// row's bridge failures with the calls it counted (`block_use.failures`, as a failed block use
/// is), and counted under `tag`, so it never reads as a clean run.
///
/// A failed call's runtime rolls back, and the host calls it counted on the way go with it, so
/// this is the only place the failure is counted: the caller never passes it on as an answer.
fn note_failed(
    jar: &mut crate::LoadedJar,
    label: &str,
    tag: &str,
    failure: &str,
    counted: &std::collections::BTreeMap<String, usize>,
) {
    log::warn!("{LOG}: {}: {label} failed: {failure}", jar.row.id);
    jar.row
        .block_use
        .note_failure(format!("{label}: {failure}"), counted);
    jar.host.note_unknown_game_hook(tag);
}

/// Refuse a click whose override failed: handled, nothing moves. Falling through to Sunrise's
/// vanilla click would answer as if the override had returned `false` (a plain swap), which
/// the override never said. The clicking client predicted vanilla; the server's answer corrects
/// it, as for every click this part answers.
fn refuse_click(result: &mut StackedOnOtherResult) -> Flow {
    result.carried = StackResult::Keep;
    result.clicked = ClickedStackResult::Keep;
    result.handled = true;
    Flow::Cancel
}

#[mod_api::sunrise_part]
impl ItemPart for JavaItemPart {
    fn item_stacked_on_other(
        &self,
        host: HostSide,
        event: &mut ItemStackedOnOtherEvent<'_>,
    ) -> Flow {
        let direction = match event.event.direction {
            StackedOnOtherDirection::CarriedOnOther => ItemClickDirection::CarriedOnOther,
            StackedOnOtherDirection::OtherStackedOnMe => ItemClickDirection::OtherStackedOnMe,
            _ => return Flow::Continue,
        };
        match host {
            HostSide::Server(host) => self.server_click(host, event.event, event.result, direction),
            // Client prediction: the clicked slot's rules are server reads
            // (`MenuRef::slot_holds`, `slot_gives`), so the client predicts vanilla and the
            // server's answer corrects it.
            HostSide::Client(_) => {
                self.count_client("item_stacked_on_other:client prediction");
                Flow::Continue
            }
        }
    }

    fn use_item(&self, host: HostSide, event: &mut UseItemEvent<'_>) -> Option<InteractionResult> {
        match host {
            HostSide::Server(host) => self.server_use(host, event.player, event.hand, event.stack),
            HostSide::Client(_) => {
                self.count_client("use_item:client prediction");
                None
            }
        }
    }

    fn item_bar(&self, host: &dyn mod_api::ClientHost, event: &mut ItemBarEvent<'_>) {
        let mut client = self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        client.java_item_bar(host, &self.item_id, event.stack, event.bar);
    }

    fn item_destroyed(&self, host: &mut dyn ServerHost, event: &mut ItemDestroyedEvent<'_>) {
        self.server_destroyed(host, event.item, event.stack, event.drops);
    }

    fn tooltip_image(
        &self,
        host: &dyn mod_api::ClientHost,
        event: &mut mod_api::TooltipImageEvent<'_>,
    ) -> Option<mod_api::TooltipComponent> {
        let mut client = self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        client.java_tooltip_image(&self.client, host, &self.item_id, event.stack)
    }
}

impl JavaItemPart {
    fn count_client(&self, what: &str) {
        let mut client = self
            .client
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        client.note_item_part_unanswered(&self.item_id, what);
    }

    /// `item_stacked_on_other` on the server: the click Sunrise describes in `click`, answered
    /// in `result`.
    pub(crate) fn server_click(
        &self,
        host: &mut dyn ServerHost,
        click: &StackedOnOther,
        result: &mut StackedOnOtherResult,
        direction: ItemClickDirection,
    ) -> Flow {
        let hook = match direction {
            ItemClickDirection::CarriedOnOther => ItemHook::StackedOnOther,
            ItemClickDirection::OtherStackedOnMe => ItemHook::OtherStackedOnMe,
        };
        let mut guard = self.lock();
        let Some((jar_index, item, owner)) = jar_item(&guard, &self.item_id, hook) else {
            // `Item`'s own `false`: nothing to run.
            return Flow::Continue;
        };
        let incarnation = click
            .player
            .and_then(|uuid| guard.lives.get(&uuid).copied());
        // Every stack this builds (the inventory, the carried stack, the held one, drops) is
        // turned into a game stack before the handler returns: none outlives the scope, so
        // what the call made is collectable once it closes (a long-lived server ran up one
        // permanent row per stack per click before). While it is open its rows are roots.
        let scope = guard.jars[jar_index].host.begin_transient_rows();
        let flow = (|| -> Flow {
            let jar = &mut guard.jars[jar_index];
            let label = format!("{} item {} {hook:?}", jar.row.id, self.item_id);
            let Some(player) = click.player.and_then(|uuid| host.player(uuid)) else {
                jar.host
                    .note_unknown_game_hook("item_stacked_on_other:no player");
                return Flow::Continue;
            };
            let Some(inventory_slot) = plain_inventory_slot(
                &*host,
                player,
                click.slot,
                [click.carried.as_ref(), click.clicked.as_ref()],
            ) else {
                jar.host
                    .note_unknown_game_hook("item_stacked_on_other:clicked slot rules");
                return Flow::Continue;
            };
            let native = Arc::clone(&jar.jar);
            let jar_types = persistent_jar_types(&jar.host);
            let container = crate::block_use::player_container(&*host, Some(player));
            let mut read = crate::live_menu::read_player(
                &*host,
                &mut jar.host,
                incarnation,
                player,
                None,
                container,
                None,
            );
            // The inventory again, jar-aware: a stack with a jar data component (a pouch's
            // contents) crosses with it decoded, not opaque.
            read.facts.inventory = player
                .with_inventory(&*host, |view| {
                    (0..crate::live_menu::INVENTORY_SLOTS)
                        .map(|slot| match view.get(slot) {
                            None => Some(StdHost::empty_item_stack()),
                            Some(stack) => java_stack(
                                &*host,
                                &native,
                                &mut jar.host,
                                None,
                                &stack.project()?,
                                &jar_types,
                            ),
                        })
                        .collect::<Option<Vec<Value>>>()
                })
                .flatten();
            let carried = match &click.carried {
                None => Some(StdHost::empty_item_stack()),
                Some(stack) => java_stack(&*host, &native, &mut jar.host, None, stack, &jar_types),
            };
            let (Some(carried), Some(_), Some(dimension)) = (
                carried,
                read.facts.inventory.as_ref(),
                read.dimension.clone(),
            ) else {
                jar.host
                    .note_unknown_game_hook("item_stacked_on_other:stacks unreadable");
                return Flow::Continue;
            };
            let item_click = ItemClick {
                direction,
                inventory_slot: inventory_slot as i32,
                carried,
                primary: click.primary,
            };
            let level = player.entity().level(&*host);
            jar.host.set_host_deferred_server_sound_seeds(true);
            let budget = ExecutionBudget::within(JAR_BUDGET);
            let outcome =
                run_with_level_random(host, level, &dimension, &mut jar.host, |jar_host| {
                    caught(&label, || {
                        native.item_click_override(
                            jar_host,
                            &read.data,
                            &dimension,
                            &read.facts,
                            &item,
                            &owner,
                            &item_click,
                            &budget,
                        )
                    })
                });
            jar.host.set_host_deferred_server_sound_seeds(false);
            jar.host.clear_entity_sound_facts();
            let sounds = jar.host.take_sound_records();
            const FAILED: &str = "item_stacked_on_other:failed";
            let no_counts = std::collections::BTreeMap::new();
            let outcome = match outcome {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(error)) => {
                    note_failed(jar, &label, FAILED, &error.to_string(), &no_counts);
                    return refuse_click(result);
                }
                Err(panic) => {
                    note_failed(
                        jar,
                        &label,
                        FAILED,
                        &format!("panicked: {panic}"),
                        &no_counts,
                    );
                    return refuse_click(result);
                }
            };
            if let Some(failure) = &outcome.report.failure {
                // Java didn't return: neither its `true` nor its `false` is known, and what it
                // changed before failing is no answer either.
                note_failed(
                    jar,
                    &label,
                    FAILED,
                    failure,
                    &outcome.report.unknown_host_calls,
                );
                return refuse_click(result);
            }
            if !outcome.report.exact() || !outcome.effects.is_empty() {
                // A counted call's placeholder may have steered it: vanilla's answer stands, and
                // the sounds it would have made don't play.
                crate::level_random::note_call_counts(
                    &mut jar.row,
                    &label,
                    &outcome.report.unknown_host_calls,
                );
                if !outcome.effects.is_empty() {
                    jar.host
                        .note_unknown_game_hook("item_stacked_on_other:side effects");
                }
                return Flow::Continue;
            }
            let carried_after = game_stack(&*host, &native, &mut jar.host, &outcome.carried);
            let slot_after = game_stack(&*host, &native, &mut jar.host, &outcome.slot);
            let (Some(carried_after), Some(slot_after)) = (carried_after, slot_after) else {
                jar.host
                    .note_unknown_game_hook("item_stacked_on_other:unconvertible stack");
                return Flow::Continue;
            };
            let clicked = match (outcome.takes, &click.clicked) {
                (0, before) if *before == slot_after => ClickedStackResult::Keep,
                (0, _) => ClickedStackResult::Set(slot_after),
                // `Slot.safeTake` on a plain inventory slot splits its stack: what is left is the
                // same item and components, fewer of them.
                (_, Some(before))
                    if slot_after.as_ref().is_none_or(|after| {
                        after.same_item(before)
                            && after.same_components(before)
                            && after.count() < before.count()
                    }) =>
                {
                    let left = slot_after.as_ref().map_or(0, ItemStack::count);
                    ClickedStackResult::Take(before.count() - left)
                }
                _ => {
                    jar.host
                        .note_unknown_game_hook("item_stacked_on_other:take shape");
                    return Flow::Continue;
                }
            };
            if outcome.takes > 1 {
                jar.host
                    .note_unknown_game_hook("item_stacked_on_other:several takes");
                return Flow::Continue;
            }
            let carried = if carried_after == click.carried {
                StackResult::Keep
            } else {
                StackResult::Set(carried_after)
            };
            let mut streams = std::collections::BTreeSet::new();
            crate::sound::apply_server_sounds(host, &sounds, &mut streams, |reason| {
                jar.host.note_unknown_game_hook(reason);
            });
            crate::sound::share_unknown_sound_streams([&mut jar.host], &streams);
            result.carried = carried;
            result.clicked = clicked;
            result.handled = outcome.handled;
            if outcome.handled {
                Flow::Cancel
            } else {
                Flow::Continue
            }
        })();
        guard.jars[jar_index].host.end_transient_rows(scope);
        flow
    }

    /// `use_item` on the server: `player` uses `stack`, held in `hand`.
    pub(crate) fn server_use(
        &self,
        host: &mut dyn ServerHost,
        player_uuid: u128,
        hand: Hand,
        stack: &ItemStack,
    ) -> Option<InteractionResult> {
        let mut guard = self.lock();
        let (jar_index, item, owner) = jar_item(&guard, &self.item_id, ItemHook::Use)?;
        let incarnation = guard.lives.get(&player_uuid).copied();
        // Every stack this builds (the inventory, the carried stack, the held one, drops) is
        // turned into a game stack before the handler returns: none outlives the scope, so
        // what the call made is collectable once it closes (a long-lived server ran up one
        // permanent row per stack per click before). While it is open its rows are roots.
        let scope = guard.jars[jar_index].host.begin_transient_rows();
        let answer = (|| -> Option<InteractionResult> {
            let jar = &mut guard.jars[jar_index];
            let label = format!("{} item {} use", jar.row.id, self.item_id);
            let Some(player) = host.player(player_uuid) else {
                jar.host.note_unknown_game_hook("use_item:no player");
                return None;
            };
            let native = Arc::clone(&jar.jar);
            let jar_types = persistent_jar_types(&jar.host);
            let container = crate::block_use::player_container(&*host, Some(player));
            let mut read = crate::live_menu::read_player(
                &*host,
                &mut jar.host,
                incarnation,
                player,
                None,
                container,
                None,
            );
            let Some(held) = java_stack(
                &*host,
                &native,
                &mut jar.host,
                Some(item.clone()),
                stack,
                &jar_types,
            ) else {
                jar.host
                    .note_unknown_game_hook("use_item:held stack unreadable");
                return None;
            };
            let hand_name = match hand {
                Hand::Main => {
                    read.data.main_hand = Some(held.clone());
                    "MAIN_HAND"
                }
                Hand::Off => {
                    read.data.off_hand = Some(held.clone());
                    "OFF_HAND"
                }
            };
            let dimension = read.dimension.clone()?;
            let level = player.entity().level(&*host);
            jar.host.set_host_deferred_server_sound_seeds(true);
            let budget = ExecutionBudget::within(JAR_BUDGET);
            let request = ItemUse {
                hand: hand_name.to_owned(),
            };
            let outcome =
                run_with_level_random(host, level, &dimension, &mut jar.host, |jar_host| {
                    caught(&label, || {
                        native.item_use(
                            jar_host,
                            &read.data,
                            &dimension,
                            &read.facts,
                            &item,
                            &owner,
                            &request,
                            &budget,
                        )
                    })
                });
            jar.host.set_host_deferred_server_sound_seeds(false);
            jar.host.clear_entity_sound_facts();
            let sounds = jar.host.take_sound_records();
            // A use whose Java failed is refused (`FAIL`: nothing happens, the click ends), never
            // handed to Sunrise's vanilla `Item.use` as if the item had no override.
            const FAILED: &str = "use_item:failed";
            let no_counts = std::collections::BTreeMap::new();
            let outcome = match outcome {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(error)) => {
                    note_failed(jar, &label, FAILED, &error.to_string(), &no_counts);
                    return Some(InteractionResult::Fail);
                }
                Err(panic) => {
                    note_failed(
                        jar,
                        &label,
                        FAILED,
                        &format!("panicked: {panic}"),
                        &no_counts,
                    );
                    return Some(InteractionResult::Fail);
                }
            };
            if let Some(failure) = &outcome.report.failure {
                note_failed(
                    jar,
                    &label,
                    FAILED,
                    failure,
                    &outcome.report.unknown_host_calls,
                );
                return Some(InteractionResult::Fail);
            }
            // Rule (b) of the level-stream lend, as the altar's take settled it (#665): a use whose
            // only counted calls are lent level-stream draws, the first of them after an effect
            // that may draw the stream (a bundle's `use` drops its contents, then draws its
            // drop-contents sound's pitch), stops at that draw. What Java did before it still
            // applies (the held stack, the drops and their stats, in order), the draw counts, and
            // nothing after it is carried out: no sound, no later stat, no result (`FAIL` ends the
            // click: the use did act, so the other hand isn't tried). Only when the held stack is
            // the one it was at the draw; otherwise, as for any other counted call, a counted
            // call's placeholder may have steered it: vanilla's answer stands and nothing Java
            // recorded is carried out.
            let stop = if outcome.report.exact() {
                None
            } else {
                crate::level_random::note_call_counts(
                    &mut jar.row,
                    &label,
                    &outcome.report.unknown_host_calls,
                );
                let only_draws = outcome
                    .report
                    .unknown_host_calls
                    .keys()
                    .all(|key| crate::runtime::is_counted_level_stream_draw(key));
                match &outcome.counted_draw_stop {
                    Some(stop) if only_draws && stop.first_counted && stop.held_unchanged => {
                        log::info!(
                            "{LOG}: {}: {label} stopped at its first counted level-stream draw: the {} effect(s) and {} sound(s) before it apply, the {} after it don't",
                            jar.row.id,
                            stop.effects,
                            stop.sounds,
                            outcome.effects.len().saturating_sub(stop.effects)
                                + sounds.len().saturating_sub(stop.sounds),
                        );
                        Some(stop.clone())
                    }
                    _ => return None,
                }
            };
            let effects = match &stop {
                Some(stop) => &outcome.effects[..stop.effects.min(outcome.effects.len())],
                None => &outcome.effects[..],
            };
            let sounds = match &stop {
                Some(stop) => &sounds[..stop.sounds.min(sounds.len())],
                None => &sounds[..],
            };
            let result = match &stop {
                Some(_) => Some(InteractionResult::Fail),
                None => outcome
                    .result
                    .as_ref()
                    .map(crate::block_use::map_interaction_result),
            };
            let Some(result) = result else {
                // `use` returned null or an `InteractionResult` the runtime doesn't model.
                jar.host
                    .note_unknown_game_hook("use_item:unmodelled interaction result");
                log::warn!(
                    "{LOG}: {}: {label} returned null or an unmodelled InteractionResult",
                    jar.row.id
                );
                return None;
            };
            // The held stack Java changed in place (`ItemStack.set`, `shrink`).
            if let Some(held_after) = outcome
                .held
                .as_ref()
                .and_then(|value| game_stack(&*host, &native, &mut jar.host, value))
                && held_after.as_ref() != Some(stack)
                && !host.living(player.entity()).is_some_and(|living| {
                    living
                        .set_hand_item(host, hand, held_after)
                        .unwrap_or(false)
                })
            {
                jar.host
                    .note_unknown_game_hook("use_item:held stack write refused");
            }
            for effect in effects {
                match effect {
                    LiveMenuEffect::Drop {
                        stack: components,
                        retain_ownership,
                        prediction,
                    } => {
                        // `ServerPlayer.drop` as the runtime spells it out (`player_drop_plan`).
                        // `drop_stack`'s `thrown_by_player` is `createItemStackToDrop`'s
                        // `retainOwnership` (Sunrise's `drop_stack_from_eyes` names the player as
                        // thrower); it awards no stats, so the plan's are awarded here once the item
                        // entity is in the world, in `ServerPlayer.drop`'s order.
                        let plan = crate::runtime::player_drop_plan(
                            components,
                            *retain_ownership,
                            *prediction,
                        );
                        // The SDK's drop takes no `Prediction`: whether the swing reaches the
                        // dropping player's own client (`swing(.., updateSelf)`) isn't in its
                        // contract, so that part counts.
                        jar.host.note_unknown_game_hook("use_item:drop prediction");
                        let stack =
                            crate::client::api_stack_from_components(&*host, components.clone());
                        let Some(stack) = stack else {
                            jar.host
                                .note_unknown_game_hook("use_item:drop unconvertible");
                            continue;
                        };
                        // `Ok(None)`: the item entity is in the world; a stack handed back was refused
                        // (vanilla's `Player.drop` answered `null`: no stats).
                        if !matches!(player.drop_stack(host, stack, plan.thrower), Ok(None)) {
                            jar.host.note_unknown_game_hook("use_item:drop refused");
                            continue;
                        }
                        for (stat_type, value, amount) in &plan.stats_when_dropped {
                            let awarded = crate::live_menu::stat_type_of(stat_type).is_some_and(
                                |stat_type| player.award_stat(host, stat_type, value, *amount),
                            );
                            if !awarded {
                                jar.host
                                    .note_unknown_game_hook("use_item:drop stat refused");
                            }
                        }
                    }
                    LiveMenuEffect::AwardStat {
                        stat_type,
                        value,
                        amount,
                    } => {
                        let awarded =
                            crate::live_menu::stat_type_of(stat_type).is_some_and(|stat_type| {
                                player.award_stat(host, stat_type, value, *amount)
                            });
                        if !awarded {
                            jar.host
                                .note_unknown_game_hook("use_item:award stat refused");
                        }
                    }
                    _ => jar.host.note_unknown_game_hook("use_item:side effect"),
                }
            }
            let mut streams = std::collections::BTreeSet::new();
            crate::sound::apply_server_sounds(host, sounds, &mut streams, |reason| {
                jar.host.note_unknown_game_hook(reason);
            });
            crate::sound::share_unknown_sound_streams([&mut jar.host], &streams);
            Some(result)
        })();
        guard.jars[jar_index].host.end_transient_rows(scope);
        answer
    }

    /// `item_destroyed` on the server: the item entity `item`, holding `stack`, is destroyed;
    /// `drops` is Sunrise's baseline of what it spills, which the item's own `onDestroyed`
    /// replaces.
    pub(crate) fn server_destroyed(
        &self,
        host: &mut dyn ServerHost,
        item: mod_api::ItemEntityRef,
        stack: &ItemStack,
        drops: &mut Vec<ItemStack>,
    ) {
        let mut guard = self.lock();
        let Some((jar_index, item_value, owner)) =
            jar_item(&guard, &self.item_id, ItemHook::Destroyed)
        else {
            return;
        };
        // Every stack this builds (the inventory, the carried stack, the held one, drops) is
        // turned into a game stack before the handler returns: none outlives the scope, so
        // what the call made is collectable once it closes (a long-lived server ran up one
        // permanent row per stack per click before). While it is open its rows are roots.
        let scope = guard.jars[jar_index].host.begin_transient_rows();
        (|| {
            let jar = &mut guard.jars[jar_index];
            let label = format!("{} item {} onDestroyed", jar.row.id, self.item_id);
            let native = Arc::clone(&jar.jar);
            let jar_types = persistent_jar_types(&jar.host);
            let entity = item.entity();
            let (Some(level), Some(position)) = (entity.level(&*host), entity.position(&*host))
            else {
                jar.host
                    .note_unknown_game_hook("item_destroyed:entity unreadable");
                return;
            };
            let Some(stack) = java_stack(
                &*host,
                &native,
                &mut jar.host,
                Some(item_value.clone()),
                stack,
                &jar_types,
            ) else {
                jar.host
                    .note_unknown_game_hook("item_destroyed:stack unreadable");
                return;
            };
            let request = crate::runtime::ItemDestroyed {
                dimension: level.dimension_id().to_owned(),
                stack,
                position,
            };
            let budget = ExecutionBudget::within(JAR_BUDGET);
            let outcome = caught(&label, || {
                native.item_destroyed(&mut jar.host, &item_value, &owner, &request, &budget)
            });
            // A failed `onDestroyed` leaves Sunrise's vanilla container baseline in place (the
            // stacks the item's vanilla container component holds), counted and logged: an item
            // entity that is gone can't be left undestroyed.
            const FAILED: &str = "item_destroyed:failed";
            let no_counts = std::collections::BTreeMap::new();
            let outcome = match outcome {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(error)) => {
                    note_failed(jar, &label, FAILED, &error.to_string(), &no_counts);
                    return;
                }
                Err(panic) => {
                    note_failed(
                        jar,
                        &label,
                        FAILED,
                        &format!("panicked: {panic}"),
                        &no_counts,
                    );
                    return;
                }
            };
            if let Some(failure) = &outcome.report.failure {
                note_failed(
                    jar,
                    &label,
                    FAILED,
                    failure,
                    &outcome.report.unknown_host_calls,
                );
                return;
            }
            if !outcome.report.exact() {
                crate::level_random::note_call_counts(
                    &mut jar.row,
                    &label,
                    &outcome.report.unknown_host_calls,
                );
                return;
            }
            // The drops Java spawned replace the vanilla container baseline Sunrise filled in: the
            // jar's `onDestroyed` is the item's, and it spawned exactly these.
            let mut spilled = Vec::with_capacity(outcome.drops.len());
            for value in &outcome.drops {
                match game_stack(&*host, &native, &mut jar.host, value) {
                    Some(Some(stack)) => spilled.push(stack),
                    Some(None) => {}
                    None => {
                        jar.host
                            .note_unknown_game_hook("item_destroyed:unconvertible drop");
                        return;
                    }
                }
            }
            *drops = spilled;
        })();
        guard.jars[jar_index].host.end_transient_rows(scope);
    }
}
