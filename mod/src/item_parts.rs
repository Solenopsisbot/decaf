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

/// Sunrise's `menu::PLAYER` container index (`crates/sunrise-client/src/menu.rs`): every slot
/// over the player's own `Inventory` (`0..36` main/hotbar, `36..40` armour, `40` offhand) carries
/// `StackedOnOther.container == 0`; non-player containers carry `1` (`MAIN`), `2` (`GRID`), or
/// `3` (`RESULT`).
const PLAYER_CONTAINER: usize = 0;

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

/// The player-inventory index (`0..36`) the clicked menu slot shows, when it is a plain vanilla
/// `Slot` over `Inventory.items`.
///
/// `StackedOnOther` already identifies the clicked slot's container and index:
/// `click.container == PLAYER_CONTAINER` (`0`) is the player's own `Inventory`, and
/// `click.container_slot` is its `Inventory` index (`0..36` hotbar/main, `36..40` armour, `40`
/// offhand). In MC 26.3 every vanilla menu adds `Inventory.items` (`0..36`) through
/// `AbstractContainerMenu.addStandardInventorySlots` as plain `new Slot(inventory, index, x, y)`
/// (`mayPlace` true, `mayPickup` true, `getMaxStackSize` = `Container.getMaxStackSize()` = 99).
///
/// When `player.container_menu` is available and its `slot_backing(click.slot)` agrees
/// (`Some(MenuSlotBacking::Player(index))` with `index == click.container_slot`), this also
/// checks `menu.kind` (declining non-vanilla menus) and Sunrise's `slot_gives` / `slot_holds`
/// (so any `menu_slot_rules` listener still applies). When `container_menu` is `None` (upstream
/// Sunrise #6694 on crafting tables/workstations) or its `slot_backing` disagrees (Sunrise
/// `3d34bbe63d` falling back to `InventoryMenu` window 0 while a crafting table or workstation is
/// open), the event's player-inventory slot (`0..36`) is authoritative — guarded downstream by
/// requiring `inventory[index] == click.clicked`.
///
/// Non-inventory slots (`click.container != PLAYER_CONTAINER`: station inputs, crafting grid,
/// result slots, chest/furnace slots) and armour/offhand (`36..41`) return `None`, so the bridge
/// declines them, counts `clicked slot rules`, and leaves Sunrise's vanilla click standing.
fn plain_inventory_slot(
    host: &dyn ServerHost,
    player: PlayerRef,
    click: &StackedOnOther,
) -> Option<usize> {
    if click.container != PLAYER_CONTAINER || click.container_slot >= PLAIN_INVENTORY_SLOTS {
        return None;
    }
    let index = click.container_slot;
    if let Some(menu) = player.container_menu(host)
        && menu.slot_backing(host, click.slot) == Some(MenuSlotBacking::Player(index))
    {
        match menu.kind(host) {
            Some(None) => {}
            Some(Some(kind)) if kind.starts_with("minecraft:") => {}
            _ => return None,
        }
        if !menu.slot_gives(host, click.slot).unwrap_or(false) {
            return None;
        }
        for stack in [click.carried.as_ref(), click.clicked.as_ref()]
            .into_iter()
            .flatten()
        {
            let item_max = host.max_stack_size(stack.item())?;
            if menu.slot_holds(host, click.slot, stack)
                != Some(item_max.min(INVENTORY_MAX_STACK_SIZE))
            {
                return None;
            }
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
        struct PendingClickSounds {
            jar_index: usize,
            jar_id: String,
            sounds: Vec<runtime::SoundRecord>,
            sound_streams: std::collections::BTreeSet<String>,
        }
        let (flow, pending_sounds) = {
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
            let mut pending_sounds = None;
            let flow = (|| -> Flow {
                let jar = &mut guard.jars[jar_index];
                let jar_id = jar.row.id.clone();
                let label = format!("{jar_id} item {} {hook:?}", self.item_id);
                let Some(player) = click.player.and_then(|uuid| host.player(uuid)) else {
                    jar.host
                        .note_unknown_game_hook("item_stacked_on_other:no player");
                    return Flow::Continue;
                };
                let Some(inventory_slot) = plain_inventory_slot(&*host, player, click) else {
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
                // contents) crosses with it decoded, not opaque. Capture the exact stack read from
                // `inventory_slot` to cross-check against `click.clicked` before running Java and
                // to compare against what Java leaves afterwards.
                let mut slot_before: Option<Option<ItemStack>> = None;
                read.facts.inventory = player
                    .with_inventory(&*host, |view| {
                        (0..crate::live_menu::INVENTORY_SLOTS)
                            .map(|slot| {
                                let projected = match view.get(slot) {
                                    None => Some(None),
                                    Some(stack) => stack.project().map(Some),
                                }?;
                                if slot as usize == inventory_slot {
                                    slot_before = Some(projected.clone());
                                }
                                match projected {
                                    None => Some(StdHost::empty_item_stack()),
                                    Some(stack) => java_stack(
                                        &*host,
                                        &native,
                                        &mut jar.host,
                                        None,
                                        &stack,
                                        &jar_types,
                                    ),
                                }
                            })
                            .collect::<Option<Vec<Value>>>()
                    })
                    .flatten();
                let Some(slot_before) = slot_before else {
                    jar.host
                        .note_unknown_game_hook("item_stacked_on_other:stacks unreadable");
                    return Flow::Continue;
                };
                if slot_before != click.clicked {
                    log::warn!(
                        "{LOG}: {jar_id}: {label}: inventory slot {inventory_slot} stack disagrees with clicked event stack; refusing click"
                    );
                    jar.host
                        .note_unknown_game_hook("item_stacked_on_other:clicked slot mismatch");
                    return refuse_click(result);
                }
                let carried = match &click.carried {
                    None => Some(StdHost::empty_item_stack()),
                    Some(stack) => {
                        java_stack(&*host, &native, &mut jar.host, None, stack, &jar_types)
                    }
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
                let sound_streams = jar.host.unknown_sound_streams().clone();
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
                if outcome.takes > 1 {
                    jar.host
                        .note_unknown_game_hook("item_stacked_on_other:several takes");
                    return Flow::Continue;
                }
                // Never write back a slot or the cursor when Java didn't change it: compare with
                // what was read from the slot/cursor, and send Keep.
                let clicked = if outcome.takes == 0 && outcome.slot_unchanged {
                    ClickedStackResult::Keep
                } else {
                    let Some(slot_after) =
                        game_stack(&*host, &native, &mut jar.host, &outcome.slot)
                    else {
                        jar.host
                            .note_unknown_game_hook("item_stacked_on_other:unconvertible stack");
                        return Flow::Continue;
                    };
                    match (outcome.takes, &slot_before) {
                        (0, before) if *before == slot_after => ClickedStackResult::Keep,
                        (0, _) => ClickedStackResult::Set(slot_after),
                        // `Slot.safeTake` on a plain inventory slot splits its stack: what is left
                        // is the same item and components, fewer of them.
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
                    }
                };
                let carried = if outcome.carried_unchanged {
                    StackResult::Keep
                } else {
                    let Some(carried_after) =
                        game_stack(&*host, &native, &mut jar.host, &outcome.carried)
                    else {
                        jar.host
                            .note_unknown_game_hook("item_stacked_on_other:unconvertible stack");
                        return Flow::Continue;
                    };
                    if carried_after == click.carried {
                        StackResult::Keep
                    } else {
                        StackResult::Set(carried_after)
                    }
                };
                if !sounds.is_empty() {
                    pending_sounds = Some(PendingClickSounds {
                        jar_index,
                        jar_id,
                        sounds,
                        sound_streams,
                    });
                }
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
            (flow, pending_sounds)
        };
        if let Some(pending) = pending_sounds {
            let mut streams = pending.sound_streams;
            let streams_before = streams.len();
            let mut sound_counted = Vec::new();
            crate::sound::apply_server_sounds(host, &pending.sounds, &mut streams, |reason| {
                sound_counted.push(reason.to_owned());
            });
            if !sound_counted.is_empty() || streams.len() != streams_before {
                let mut guard = self.lock();
                if let Some(jar) = guard
                    .jars
                    .get_mut(pending.jar_index)
                    .filter(|jar| jar.row.id == pending.jar_id)
                {
                    for reason in &sound_counted {
                        jar.host.note_unknown_game_hook(reason);
                    }
                    crate::sound::share_unknown_sound_streams([&mut jar.host], &streams);
                }
            }
        }
        flow
    }

    /// `use_item` on the server: `player` uses `stack`, held in `hand`.
    ///
    /// Java runs under the state lock and collects its effects (held stack update, drops, stats,
    /// sounds). The lock is dropped before any mutating host call runs: `player.drop_stack` spawns
    /// the item entity synchronously on the same thread and dispatches `Hooks::entity_join_level`,
    /// which takes the same state lock.
    pub(crate) fn server_use(
        &self,
        host: &mut dyn ServerHost,
        player_uuid: u128,
        hand: Hand,
        stack: &ItemStack,
    ) -> Option<InteractionResult> {
        enum PreparedUseEffect {
            Drop {
                stack: ItemStack,
                thrower: bool,
                stats_when_dropped: Vec<(String, String, i32)>,
            },
            AwardStat {
                stat_type: String,
                value: String,
                amount: i32,
            },
        }
        struct PreparedUse {
            jar_index: usize,
            jar_id: String,
            player: PlayerRef,
            result: InteractionResult,
            set_hand: Option<Option<ItemStack>>,
            effects: Vec<PreparedUseEffect>,
            sounds: Vec<runtime::SoundRecord>,
            sound_streams: std::collections::BTreeSet<String>,
        }
        enum UsePlan {
            Immediate(Option<InteractionResult>),
            Apply(PreparedUse),
        }

        let plan = {
            let mut guard = self.lock();
            let (jar_index, item, owner) = jar_item(&guard, &self.item_id, ItemHook::Use)?;
            let incarnation = guard.lives.get(&player_uuid).copied();
            // Every stack this builds (the inventory, the carried stack, the held one, drops) is
            // turned into a game stack before the handler returns: none outlives the scope, so
            // what the call made is collectable once it closes (a long-lived server ran up one
            // permanent row per stack per click before). While it is open its rows are roots.
            let scope = guard.jars[jar_index].host.begin_transient_rows();
            let plan = (|| -> UsePlan {
                let jar = &mut guard.jars[jar_index];
                let jar_id = jar.row.id.clone();
                let label = format!("{jar_id} item {} use", self.item_id);
                let Some(player) = host.player(player_uuid) else {
                    jar.host.note_unknown_game_hook("use_item:no player");
                    return UsePlan::Immediate(None);
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
                    return UsePlan::Immediate(None);
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
                let Some(dimension) = read.dimension.clone() else {
                    return UsePlan::Immediate(None);
                };
                let level = player.entity().level(&*host);
                let sound_streams = jar.host.unknown_sound_streams().clone();
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
                // A use whose Java failed is refused (`FAIL`: nothing happens, the click ends),
                // never handed to Sunrise's vanilla `Item.use` as if the item had no override.
                const FAILED: &str = "use_item:failed";
                let no_counts = std::collections::BTreeMap::new();
                let outcome = match outcome {
                    Ok(Ok(outcome)) => outcome,
                    Ok(Err(error)) => {
                        note_failed(jar, &label, FAILED, &error.to_string(), &no_counts);
                        return UsePlan::Immediate(Some(InteractionResult::Fail));
                    }
                    Err(panic) => {
                        note_failed(
                            jar,
                            &label,
                            FAILED,
                            &format!("panicked: {panic}"),
                            &no_counts,
                        );
                        return UsePlan::Immediate(Some(InteractionResult::Fail));
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
                    return UsePlan::Immediate(Some(InteractionResult::Fail));
                }
                // Rule (b) of the level-stream lend, as the altar's take settled it (#665): a use
                // whose only counted calls are lent level-stream draws, the first of them after an
                // effect that may draw the stream (a bundle's `use` drops its contents, then draws
                // its drop-contents sound's pitch), stops at that draw. What Java did before it
                // still applies (the held stack, the drops and their stats, in order), the draw
                // counts, and nothing after it is carried out: no sound, no later stat, no result
                // (`FAIL` ends the click: the use did act, so the other hand isn't tried). Only
                // when the held stack is the one it was at the draw; otherwise, as for any other
                // counted call, a counted call's placeholder may have steered it: vanilla's answer
                // stands and nothing Java recorded is carried out.
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
                        _ => return UsePlan::Immediate(None),
                    }
                };
                let effects = match &stop {
                    Some(stop) => &outcome.effects[..stop.effects.min(outcome.effects.len())],
                    None => &outcome.effects[..],
                };
                let sounds = match &stop {
                    Some(stop) => sounds[..stop.sounds.min(sounds.len())].to_vec(),
                    None => sounds,
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
                    return UsePlan::Immediate(None);
                };
                // The held stack Java changed in place (`ItemStack.set`, `shrink`).
                let set_hand = outcome
                    .held
                    .as_ref()
                    .and_then(|value| game_stack(&*host, &native, &mut jar.host, value))
                    .filter(|held_after| held_after.as_ref() != Some(stack));
                let mut prepared_effects = Vec::with_capacity(effects.len());
                for effect in effects {
                    match effect {
                        LiveMenuEffect::Drop {
                            stack: components,
                            retain_ownership,
                            prediction,
                        } => {
                            // `ServerPlayer.drop` as the runtime spells it out
                            // (`player_drop_plan`). `drop_stack`'s `thrown_by_player` is
                            // `createItemStackToDrop`'s `retainOwnership` (Sunrise's
                            // `drop_stack_from_eyes` names the player as thrower); it awards no
                            // stats, so the plan's are awarded here once the item entity is in the
                            // world, in `ServerPlayer.drop`'s order.
                            let plan = crate::runtime::player_drop_plan(
                                components,
                                *retain_ownership,
                                *prediction,
                            );
                            // The SDK's drop takes no `Prediction`: whether the swing reaches the
                            // dropping player's own client (`swing(.., updateSelf)`) isn't in its
                            // contract, so that part counts.
                            jar.host.note_unknown_game_hook("use_item:drop prediction");
                            let Some(stack) = crate::client::api_stack_from_components(
                                &*host,
                                components.clone(),
                            ) else {
                                jar.host
                                    .note_unknown_game_hook("use_item:drop unconvertible");
                                continue;
                            };
                            prepared_effects.push(PreparedUseEffect::Drop {
                                stack,
                                thrower: plan.thrower,
                                stats_when_dropped: plan.stats_when_dropped,
                            });
                        }
                        LiveMenuEffect::AwardStat {
                            stat_type,
                            value,
                            amount,
                        } => {
                            prepared_effects.push(PreparedUseEffect::AwardStat {
                                stat_type: stat_type.clone(),
                                value: value.clone(),
                                amount: *amount,
                            });
                        }
                        _ => jar.host.note_unknown_game_hook("use_item:side effect"),
                    }
                }
                UsePlan::Apply(PreparedUse {
                    jar_index,
                    jar_id,
                    player,
                    result,
                    set_hand,
                    effects: prepared_effects,
                    sounds,
                    sound_streams,
                })
            })();
            guard.jars[jar_index].host.end_transient_rows(scope);
            plan
        };

        let prepared = match plan {
            UsePlan::Immediate(answer) => return answer,
            UsePlan::Apply(prepared) => prepared,
        };

        // Apply every mutating host effect without holding the state lock so synchronous hooks
        // such as `entity_join_level` (fired inside `drop_stack`) can lock state without
        // deadlocking.
        let mut post_counted: Vec<&'static str> = Vec::new();
        if let Some(held_after) = prepared.set_hand
            && !host.living(prepared.player.entity()).is_some_and(|living| {
                living
                    .set_hand_item(host, hand, held_after)
                    .unwrap_or(false)
            })
        {
            post_counted.push("use_item:held stack write refused");
        }
        for effect in prepared.effects {
            match effect {
                PreparedUseEffect::Drop {
                    stack,
                    thrower,
                    stats_when_dropped,
                } => {
                    // `Ok(None)`: the item entity is in the world; a stack handed back was refused
                    // (vanilla's `Player.drop` answered `null`: no stats).
                    if !matches!(prepared.player.drop_stack(host, stack, thrower), Ok(None)) {
                        post_counted.push("use_item:drop refused");
                        continue;
                    }
                    for (stat_type, value, amount) in &stats_when_dropped {
                        let awarded =
                            crate::live_menu::stat_type_of(stat_type).is_some_and(|stat_type| {
                                prepared.player.award_stat(host, stat_type, value, *amount)
                            });
                        if !awarded {
                            post_counted.push("use_item:drop stat refused");
                        }
                    }
                }
                PreparedUseEffect::AwardStat {
                    stat_type,
                    value,
                    amount,
                } => {
                    let awarded =
                        crate::live_menu::stat_type_of(&stat_type).is_some_and(|stat_type| {
                            prepared.player.award_stat(host, stat_type, &value, amount)
                        });
                    if !awarded {
                        post_counted.push("use_item:award stat refused");
                    }
                }
            }
        }
        let mut streams = prepared.sound_streams;
        let streams_before = streams.len();
        let mut sound_counted = Vec::new();
        crate::sound::apply_server_sounds(host, &prepared.sounds, &mut streams, |reason| {
            sound_counted.push(reason.to_owned());
        });
        if !post_counted.is_empty() || !sound_counted.is_empty() || streams.len() != streams_before
        {
            let mut guard = self.lock();
            if let Some(jar) = guard
                .jars
                .get_mut(prepared.jar_index)
                .filter(|jar| jar.row.id == prepared.jar_id)
            {
                for tag in post_counted {
                    jar.host.note_unknown_game_hook(tag);
                }
                for reason in &sound_counted {
                    jar.host.note_unknown_game_hook(reason);
                }
                crate::sound::share_unknown_sound_streams([&mut jar.host], &streams);
            }
        }
        Some(prepared.result)
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
