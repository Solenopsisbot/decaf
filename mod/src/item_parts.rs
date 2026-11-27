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
//! counted on the jar, as every bridge here does.

use std::sync::{Arc, Mutex};

use mod_api::{
    ClickedStackResult, Flow, Hand, HostSide, InteractionResult, ItemBarEvent, ItemDestroyedEvent,
    ItemPart, ItemStack, ItemStackedOnOtherEvent, MenuSlotBacking, PlayerRef, ServerHost,
    StackResult, StackedOnOtherDirection, UseItemEvent,
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
pub(crate) fn java_stack(
    host: &dyn mod_api::Host,
    jar: &NativeJar,
    jar_host: &mut StdHost,
    item_override: Option<Value>,
    stack: &ItemStack,
    jar_types: &[String],
) -> Option<Value> {
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
    for id in named {
        let key = mod_api::Identifier::parse(&id)
            .ok()
            .and_then(|parsed| host.item_key(&parsed));
        if let Some(key) = key {
            jar.record_item_prototype(jar_host, &id, defaults(key))
                .ok()?;
        }
    }
    let defaults = defaults(stack.item());
    jar.make_item_stack(
        jar_host,
        item_override,
        &parts.id,
        parts.count,
        parts.patch,
        defaults,
        parts.opaque_components,
        parts.opaque_keys,
    )
    .ok()
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
            HostSide::Server(host) => self.server_click(host, event, direction),
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
            HostSide::Server(host) => self.server_use(host, event),
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
        self.server_destroyed(host, event);
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

    fn server_click(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ItemStackedOnOtherEvent<'_>,
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
        let incarnation = event
            .event
            .player
            .and_then(|uuid| guard.lives.get(&uuid).copied());
        let jar = &mut guard.jars[jar_index];
        let label = format!("{} item {} {hook:?}", jar.row.id, self.item_id);
        let Some(player) = event.event.player.and_then(|uuid| host.player(uuid)) else {
            jar.host
                .note_unknown_game_hook("item_stacked_on_other:no player");
            return Flow::Continue;
        };
        let Some(inventory_slot) = plain_inventory_slot(
            &*host,
            player,
            event.event.slot,
            [event.event.carried.as_ref(), event.event.clicked.as_ref()],
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
        let carried = match &event.event.carried {
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
        let click = ItemClick {
            direction,
            inventory_slot: inventory_slot as i32,
            carried,
            primary: event.event.primary,
        };
        let level = player.entity().level(&*host);
        jar.host.set_host_deferred_server_sound_seeds(true);
        let budget = ExecutionBudget::within(JAR_BUDGET);
        let outcome = run_with_level_random(host, level, &dimension, &mut jar.host, |jar_host| {
            caught(&label, || {
                native.item_click_override(
                    jar_host,
                    &read.data,
                    &dimension,
                    &read.facts,
                    &item,
                    &owner,
                    &click,
                    &budget,
                )
            })
        });
        jar.host.set_host_deferred_server_sound_seeds(false);
        jar.host.clear_entity_sound_facts();
        let sounds = jar.host.take_sound_records();
        let outcome = match outcome {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => {
                log::warn!("{LOG}: {label} failed: {error}");
                jar.host
                    .note_unknown_game_hook("item_stacked_on_other:failed");
                return Flow::Continue;
            }
            Err(panic) => {
                log::warn!("{LOG}: {label} panicked: {panic}");
                jar.host
                    .note_unknown_game_hook("item_stacked_on_other:failed");
                return Flow::Continue;
            }
        };
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
        let clicked = match (outcome.takes, &event.event.clicked) {
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
        let carried = if carried_after == event.event.carried {
            StackResult::Keep
        } else {
            StackResult::Set(carried_after)
        };
        let mut streams = std::collections::BTreeSet::new();
        crate::sound::apply_server_sounds(host, &sounds, &mut streams, |reason| {
            jar.host.note_unknown_game_hook(reason);
        });
        crate::sound::share_unknown_sound_streams([&mut jar.host], &streams);
        event.result.carried = carried;
        event.result.clicked = clicked;
        event.result.handled = outcome.handled;
        if outcome.handled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn server_use(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let mut guard = self.lock();
        let (jar_index, item, owner) = jar_item(&guard, &self.item_id, ItemHook::Use)?;
        let incarnation = guard.lives.get(&event.player).copied();
        let jar = &mut guard.jars[jar_index];
        let label = format!("{} item {} use", jar.row.id, self.item_id);
        let Some(player) = host.player(event.player) else {
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
            event.stack,
            &jar_types,
        ) else {
            jar.host
                .note_unknown_game_hook("use_item:held stack unreadable");
            return None;
        };
        let hand = match event.hand {
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
            hand: hand.to_owned(),
        };
        let outcome = run_with_level_random(host, level, &dimension, &mut jar.host, |jar_host| {
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
        let outcome = match outcome {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(error)) => {
                log::warn!("{LOG}: {label} failed: {error}");
                jar.host.note_unknown_game_hook("use_item:failed");
                return None;
            }
            Err(panic) => {
                log::warn!("{LOG}: {label} panicked: {panic}");
                jar.host.note_unknown_game_hook("use_item:failed");
                return None;
            }
        };
        let result = outcome
            .result
            .as_ref()
            .map(crate::block_use::map_interaction_result);
        if !outcome.report.exact() || result.is_none() {
            crate::level_random::note_call_counts(
                &mut jar.row,
                &label,
                &outcome.report.unknown_host_calls,
            );
            return None;
        }
        // The held stack Java changed in place (`ItemStack.set`, `shrink`).
        if let Some(held_after) = outcome
            .held
            .as_ref()
            .and_then(|value| game_stack(&*host, &native, &mut jar.host, value))
            && held_after.as_ref() != Some(event.stack)
            && !host.living(player.entity()).is_some_and(|living| {
                living
                    .set_hand_item(host, event.hand, held_after)
                    .unwrap_or(false)
            })
        {
            jar.host
                .note_unknown_game_hook("use_item:held stack write refused");
        }
        for effect in &outcome.effects {
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
                        let awarded =
                            crate::live_menu::stat_type_of(stat_type).is_some_and(|stat_type| {
                                player.award_stat(host, stat_type, value, *amount)
                            });
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
        crate::sound::apply_server_sounds(host, &sounds, &mut streams, |reason| {
            jar.host.note_unknown_game_hook(reason);
        });
        crate::sound::share_unknown_sound_streams([&mut jar.host], &streams);
        result
    }

    fn server_destroyed(&self, host: &mut dyn ServerHost, event: &mut ItemDestroyedEvent<'_>) {
        let mut guard = self.lock();
        let Some((jar_index, item, owner)) = jar_item(&guard, &self.item_id, ItemHook::Destroyed)
        else {
            return;
        };
        let jar = &mut guard.jars[jar_index];
        let label = format!("{} item {} onDestroyed", jar.row.id, self.item_id);
        let native = Arc::clone(&jar.jar);
        let jar_types = persistent_jar_types(&jar.host);
        let entity = event.item.entity();
        let (Some(level), Some(position)) = (entity.level(&*host), entity.position(&*host)) else {
            jar.host
                .note_unknown_game_hook("item_destroyed:entity unreadable");
            return;
        };
        let Some(stack) = java_stack(
            &*host,
            &native,
            &mut jar.host,
            Some(item.clone()),
            event.stack,
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
            native.item_destroyed(&mut jar.host, &item, &owner, &request, &budget)
        });
        let Ok(Ok(outcome)) = outcome else {
            jar.host.note_unknown_game_hook("item_destroyed:failed");
            return;
        };
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
        let mut drops = Vec::with_capacity(outcome.drops.len());
        for value in &outcome.drops {
            match game_stack(&*host, &native, &mut jar.host, value) {
                Some(Some(stack)) => drops.push(stack),
                Some(None) => {}
                None => {
                    jar.host
                        .note_unknown_game_hook("item_destroyed:unconvertible drop");
                    return;
                }
            }
        }
        *event.drops = drops;
    }
}
