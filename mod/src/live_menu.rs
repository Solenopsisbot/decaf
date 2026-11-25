//! The live Java menu bridge (#609 batch 3 item 4): a jar's menu, which the runtime runs with
//! Minecraft 26.3's own `AbstractContainerMenu` / `ItemCombinerMenu` bodies (`runtime/live_menu.rs`,
//! #652), shown and played on Sunrise as a blockless owned mod menu.
//!
//! # One authority
//!
//! The Java menu decides and Sunrise mirrors it. Every click Sunrise hears on a window this mod
//! opened for a Java menu reaches [`MenuPart::menu_clicked`], which hands the raw click (slot,
//! button, mode, as `ServerboundContainerClickPacket` carries them) to
//! [`runtime::NativeJar::click_player_menu`] and answers `Flow::Cancel`: Sunrise's own click
//! engine never moves a stack in these windows. What the Java click left (every menu slot, the
//! cursor, the player's inventory) is then written onto Sunrise, and its side effects (a stat,
//! recipe unlocks, drops, data slots) go out through Sunrise's matching calls. So vanilla's click
//! semantics, as #652 transcribed them, stay the one source of truth, including the mod's own
//! `createResult`, `onTake`, `quickMoveStack` and `mayPlace` overrides.
//!
//! The menu type is declared by the bake from the menu its `MenuType` builds
//! ([`runtime::NativeJar::menu_type_declarations`]): one owned slot per container slot, in Java's
//! order, then the player's 36 slots in vanilla's standard order, so a Java slot index is the
//! Sunrise slot index. It declares no `results`: a processing menu's close would make Sunrise
//! return the inputs itself, where the Java menu's `removed` decides that.
//!
//! # The player
//!
//! Each call reads the player from Sunrise into [`runtime::MenuPlayerFacts`]: their 43 inventory
//! slots, selected hotbar slot, creative `instabuild`, eye position and block interaction range,
//! the block at the menu's `ContainerLevelAccess` position, whether they are dying, their
//! `tickCount`, and the menu's cursor. A fact Sunrise can't give stays unknown, and the Java step
//! that needs it counts.
//!
//! # Ticks and closes
//!
//! `ServerPlayer.tick` runs `broadcastChanges` and `stillValid` each tick, and `doTick` checks
//! `stillValid` again: [`player_tick`] runs both ([`runtime::NativeJar::tick_player_menu`],
//! [`runtime::NativeJar::check_player_menu_valid`]) from `player_tick_post`, and a failed check
//! closes the Sunrise window after the Java close returned the cursor and inputs. A window the
//! player closes runs the Java close (`removed`, `clearContainer`) from `menu_closed`
//! ([`close_held`]): Sunrise already put the cursor back, so the call starts from an empty one
//! ([`runtime::MenuPlayerFacts::carried`]).
//!
//! Nothing here names a mod.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use mod_api::{
    Flow, HostTransaction, ItemPlace, ItemResource, ItemStack, MenuClickedEvent, MenuPart, MenuRef,
    MenuSlotBacking, PlayerRef, ServerHost, StatType, TransactionContext,
};

use crate::block_use::{self, MenuApplied};
use crate::runtime::{
    self, BlockUsePlayerData, ContainerInput, ExecutionBudget, LiveMenuEffect, LiveMenuOutcome,
    MenuClick, MenuEffect, MenuPlayerFacts, PlayerContainer, StdHost, Value,
};
use crate::{JAR_BUDGET, LOG, LoadedJar, State, caught};

/// `Inventory`'s slots as `MenuPlayerFacts` and `PlayerRef::with_inventory` number them: 0-35
/// the items, 36-39 feet to head, 40 the off hand, 41 body, 42 saddle.
const INVENTORY_SLOTS: u32 = 43;

/// `Attributes.BLOCK_INTERACTION_RANGE`, which `Player.blockInteractionRange()` reads.
const BLOCK_INTERACTION_RANGE: &str = "minecraft:block_interaction_range";

/// The Sunrise [`MenuPart`] attached to each menu type a jar registered: the click half of the
/// bridge.
pub(crate) struct JavaMenuPart {
    state: Arc<Mutex<State>>,
}

impl JavaMenuPart {
    pub(crate) fn new(state: Arc<Mutex<State>>) -> Self {
        Self { state }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[mod_api::sunrise_part]
impl MenuPart for JavaMenuPart {
    /// `ServerGamePacketListenerImpl.handleContainerClick` for a window showing a Java menu: the
    /// Java menu runs the click, Sunrise mirrors it, and Sunrise's own click is skipped.
    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent<'_>) -> Flow {
        let click = MenuClick {
            container_id: event.window,
            slot: event.slot,
            button: event.button,
            input: match container_input(event.mode) {
                Some(input) => input,
                None => {
                    // The packet's codec reads `ContainerInput` by id; no other id reaches Java.
                    let mut state = self.lock();
                    note_holder(
                        &mut state,
                        event.uuid,
                        event.window,
                        "sunrise#menu_clicked:mode",
                    );
                    return Flow::Cancel;
                }
            },
            // Sunrise owns the window's state ids and resends the menu after a cancelled
            // click: Decaf runs `broadcastChanges`, the matching-id branch.
            state_id_matches: true,
        };
        let prepared = {
            let mut state = self.lock();
            prepare(
                &mut state,
                &*host,
                event.uuid,
                event.window,
                Call::Click(click),
            )
        };
        let Some(prepared) = prepared else {
            // A window of a Java menu type no jar holds (its open was refused, or its Java menu
            // went away): Sunrise's own click would move mirrored stacks no menu stands behind.
            let mut state = self.lock();
            for jar in &mut state.jars {
                if jar
                    .host
                    .registrations()
                    .iter()
                    .any(|entry| entry.registry == "minecraft:menu" && entry.id == event.menu)
                {
                    jar.host
                        .note_unknown_game_hook("sunrise#menu_clicked:no_java_menu");
                }
            }
            return Flow::Cancel;
        };
        let applied = apply(host, &prepared);
        let mut state = self.lock();
        record(&mut state, prepared, applied);
        Flow::Cancel
    }
}

/// 26.3 `ContainerInput` by its `id()` (its ordinal), as the click packet carries it.
pub(crate) fn container_input(mode: i32) -> Option<ContainerInput> {
    Some(match mode {
        0 => ContainerInput::Pickup,
        1 => ContainerInput::QuickMove,
        2 => ContainerInput::Swap,
        3 => ContainerInput::Clone,
        4 => ContainerInput::Throw,
        5 => ContainerInput::QuickCraft,
        6 => ContainerInput::PickupAll,
        _ => return None,
    })
}

/// Count `hook_tag` on each jar that holds window `window` for the player.
fn note_holder(state: &mut State, uuid: u128, window: i32, hook_tag: &'static str) {
    for jar in &mut state.jars {
        if jar
            .host
            .player_open_menu(uuid)
            .is_some_and(|open| open.container_id == window)
        {
            jar.host.note_unknown_game_hook(hook_tag);
        }
    }
}

/// Which live-menu entry point a call runs.
pub(crate) enum Call {
    Click(MenuClick),
    /// `ServerPlayer.tick`'s `broadcastChanges` and `stillValid`.
    Tick,
    /// `ServerPlayer.doTick`'s second `stillValid`.
    Validity,
    /// `handleContainerClose` → `doCloseContainer`, after Sunrise closed the window.
    Close,
}

/// A stack the Java menu left somewhere, as Sunrise should show it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Desired {
    /// Write this (`None`: empty).
    Stack(Option<ItemStack>),
    /// Leave Sunrise's stack as it is: Java didn't change it.
    Keep,
    /// Java changed it to a stack the mod API can't carry (an opaque component): counted, left.
    Unconvertible,
}

/// One side effect of the Java call that Sunrise carries out with a call of its own.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SideEffect {
    /// `ServerPlayer.drop(stack, retainOwnership, prediction)`.
    Drop {
        stack: ItemStack,
        retain_ownership: bool,
    },
    /// `ServerPlayer.awardStat(type.get(value), amount)`.
    AwardStat {
        stat_type: StatType,
        key: String,
        amount: i32,
    },
    /// `ServerPlayer.awardRecipes`.
    AwardRecipes(Vec<String>),
    /// `DataSlot` `index` changed to `value` (`ClientboundContainerSetDataPacket`).
    Data { index: usize, value: i32 },
}

/// What one Java call asks of Sunrise: the state to mirror, then its side effects and the
/// opens and closes its Java made. Built under the state lock, carried out without it.
#[derive(Debug)]
pub(crate) struct Prepared {
    pub(crate) jar_index: usize,
    pub(crate) jar_id: String,
    pub(crate) uuid: u128,
    pub(crate) window: i32,
    /// Each menu slot, by Java (= Sunrise) index; empty when the menu is no longer held.
    pub(crate) slots: Vec<Desired>,
    /// The cursor; `Keep` leaves it.
    pub(crate) cursor: Desired,
    /// The player's inventory by `Inventory` number (43 entries), or nothing to write.
    pub(crate) inventory: Vec<(u32, Desired)>,
    pub(crate) effects: Vec<SideEffect>,
    pub(crate) menu_effects: Vec<MenuEffect>,
    /// Hook tags counted while building this (an effect Sunrise has no call for).
    pub(crate) counted: Vec<&'static str>,
    pub(crate) unknown_host_calls: BTreeMap<String, usize>,
    pub(crate) unknown_host_call_count: usize,
    pub(crate) failure: Option<String>,
}

/// What carrying out a [`Prepared`] on Sunrise did.
#[derive(Debug, Default)]
pub(crate) struct Applied {
    /// Hook tags to count: writes Sunrise refused, effects it couldn't carry.
    pub(crate) counted: Vec<&'static str>,
    pub(crate) menu_applied: Vec<MenuApplied>,
}

/// The player's facts as Sunrise gives them, with the Sunrise stacks they were read from (for
/// [`Desired::Keep`] decisions). `None` when Sunrise can't resolve the player.
pub(crate) struct PlayerRead {
    pub(crate) facts: MenuPlayerFacts,
    pub(crate) data: BlockUsePlayerData,
    pub(crate) dimension: Option<String>,
}

/// Read what a live-menu call needs of `player` from Sunrise, converting stacks with `jar_host`.
/// `menu` is the window whose cursor the call starts from (`None`: keep the Java cursor, or
/// `Some(None)` for an already emptied one: a close).
pub(crate) fn read_player(
    host: &dyn ServerHost,
    jar_host: &mut StdHost,
    incarnation: Option<(u32, u32)>,
    player: PlayerRef,
    access_pos: Option<[i32; 3]>,
    container: PlayerContainer,
    cursor: Option<Option<MenuRef>>,
) -> PlayerRead {
    let entity = player.entity();
    let living = host.living(entity);
    // A slot whose stack the mod API can't project, or a hotbar selection it can't read, leaves
    // the whole inventory unknown: the Java steps that read it count.
    let selected = player.selected_slot(host);
    let inventory = selected.and_then(|_| {
        player
            .with_inventory(host, |view| {
                (0..INVENTORY_SLOTS)
                    .map(|slot| match view.get(slot) {
                        None => Some(None),
                        Some(stack) => stack.project().map(Some),
                    })
                    .collect::<Option<Vec<Option<ItemStack>>>>()
            })
            .flatten()
    });
    let inventory = inventory.and_then(|stacks| {
        stacks
            .iter()
            .map(|stack| match stack {
                None => Some(StdHost::empty_item_stack()),
                Some(stack) => crate::client::runtime_stack_from_api(host, jar_host, None, stack),
            })
            .collect::<Option<Vec<Value>>>()
    });
    let access_block = access_pos.and_then(|pos| {
        let level = entity.level(host)?;
        let state = level.block_state(host, pos)?;
        let raw = host.block_state_string(state)?;
        let (id, properties) = block_use::parse_block_state_str(&raw);
        Some((id.to_owned(), properties))
    });
    let health = living.and_then(|living| living.health(host));
    let carried = match cursor {
        None => None,
        Some(None) => Some(StdHost::empty_item_stack()),
        Some(Some(menu)) => match menu.carried(host) {
            Some(None) => Some(StdHost::empty_item_stack()),
            Some(Some(stack)) => {
                crate::client::runtime_stack_from_api(host, jar_host, None, &stack)
            }
            None => None,
        },
    };
    let facts = MenuPlayerFacts {
        inventory,
        selected_slot: selected.map_or(0, |slot| slot as i32),
        instabuild: player.has_infinite_materials(host),
        eye_position: entity.eye_position(host),
        position: entity.position(host),
        block_interaction_range: living
            .and_then(|living| living.attribute_value(host, BLOCK_INTERACTION_RANGE)),
        access_block,
        // Sunrise resolved the player, so they are on the server's list: not removed, not
        // disconnected. A dying player may be closing at death or at respawn's removal, which
        // decide `dropOrPlaceInInventory` differently: unknown then.
        removed: false,
        removed_for_dimension_change: false,
        disconnected: false,
        departure_unknown: health.is_none_or(|health| health <= 0.0),
        dead: health.is_some_and(|health| health <= 0.0),
        tick_count: entity.tick_count(host),
        carried,
    };
    let data = BlockUsePlayerData {
        uuid: player.uuid(),
        name: player
            .name(host)
            .map(str::to_owned)
            .unwrap_or_else(|| "Player".to_owned()),
        entity_id: Some(entity.id() as i32),
        incarnation,
        container,
        sneaking: entity.is_shift_key_down(host).unwrap_or(false),
        spectator: player.is_spectator(host).unwrap_or(false),
        main_hand: None,
        off_hand: None,
    };
    PlayerRead {
        facts,
        data,
        dimension: entity
            .level(host)
            .map(|level| level.dimension_id().to_owned()),
    }
}

/// Run `call` on the Java menu window `window` that a jar holds for the player, under the state
/// lock: what Sunrise should mirror. `None` when no jar holds that window.
pub(crate) fn prepare(
    state: &mut State,
    host: &dyn ServerHost,
    uuid: u128,
    window: i32,
    call: Call,
) -> Option<Prepared> {
    let jar_index = state.jars.iter().position(|jar| {
        jar.host
            .player_open_menu(uuid)
            .is_some_and(|open| open.container_id == window)
    })?;
    let incarnation = state.lives.get(&uuid).copied();
    let jar = &mut state.jars[jar_index];
    let access_pos = jar.host.player_open_menu(uuid)?.access_pos;
    let player = host.player(uuid);
    let menu = player
        .and_then(|player| player.container_menu(host))
        .filter(|menu| menu.container_id(host) == Some(window));
    let Some(player) = player else {
        jar.host
            .note_unknown_game_hook("sunrise#live_menu:player_unavailable");
        return None;
    };
    let (container, cursor) = match call {
        // Sunrise already put the cursor back into the inventory.
        Call::Close => (PlayerContainer::Inventory, Some(None)),
        _ => (PlayerContainer::Open(window), menu.map(Some)),
    };
    let read = read_player(
        host,
        &mut jar.host,
        incarnation,
        player,
        access_pos,
        container,
        cursor,
    );
    let Some(dimension) = read.dimension.clone() else {
        jar.host
            .note_unknown_game_hook("sunrise#live_menu:unknown_level");
        return None;
    };
    let budget = ExecutionBudget::within(JAR_BUDGET);
    let label = format!("{} menu {window}", jar.row.id);
    let ran = caught(&label, || match &call {
        Call::Click(click) => jar.jar.click_player_menu(
            &mut jar.host,
            &read.data,
            &dimension,
            &read.facts,
            click,
            &budget,
        ),
        Call::Tick => jar.jar.tick_player_menu(
            &mut jar.host,
            &read.data,
            &dimension,
            &read.facts,
            window,
            &budget,
        ),
        Call::Validity => jar.jar.check_player_menu_valid(
            &mut jar.host,
            &read.data,
            &dimension,
            &read.facts,
            window,
            &budget,
        ),
        Call::Close => jar.jar.close_player_menu_live(
            &mut jar.host,
            &read.data,
            &dimension,
            &read.facts,
            window,
            &budget,
        ),
    });
    for line in jar.host.take_stdout().lines() {
        log::info!("{LOG}: {}: {line}", jar.row.id);
    }
    let outcome = match ran {
        Ok(Ok(Some(outcome))) => outcome,
        Ok(Ok(None)) => return None,
        Ok(Err(error)) => {
            jar.row.block_use.uses_failed += 1;
            log::warn!("{LOG}: {}: menu {window} call failed: {error}", jar.row.id);
            return None;
        }
        Err(panic) => {
            jar.row.block_use.uses_failed += 1;
            log::warn!(
                "{LOG}: {}: menu {window} call panicked: {panic}",
                jar.row.id
            );
            return None;
        }
    };
    Some(mirror(
        jar,
        host,
        jar_index,
        uuid,
        window,
        &read.facts,
        outcome,
    ))
}

/// Turn a call's outcome into what Sunrise shows and does: the state Java left, converted to
/// mod API stacks, and its effects.
fn mirror(
    jar: &mut LoadedJar,
    host: &dyn ServerHost,
    jar_index: usize,
    uuid: u128,
    window: i32,
    facts: &MenuPlayerFacts,
    outcome: LiveMenuOutcome,
) -> Prepared {
    let mut counted = Vec::new();
    let desired = |value: &Value, before: Option<&Value>, counted: &mut Vec<&'static str>| {
        if before == Some(value) {
            // The very Java object the call started from: unchanged.
            return Desired::Keep;
        }
        match jar.host.item_stack_is_empty(value) {
            Some(true) => Desired::Stack(None),
            _ => match crate::client::api_stack(host, &jar.host, value) {
                Some(stack) => Desired::Stack(Some(stack)),
                None => {
                    counted.push("sunrise#live_menu:unconvertible_stack");
                    Desired::Unconvertible
                }
            },
        }
    };
    let slots = outcome
        .slots
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|stack| desired(stack, None, &mut counted))
        .collect();
    let cursor = match &outcome.carried {
        Some(carried) => desired(carried, facts.carried.as_ref(), &mut counted),
        // The menu is no longer held: its `removed` cleared the cursor
        // (`AbstractContainerMenu.removed` offsets 23-38).
        None => Desired::Stack(None),
    };
    let inventory = match (&outcome.inventory, &facts.inventory) {
        (Some(after), Some(before)) => after
            .iter()
            .enumerate()
            .map(|(slot, stack)| (slot as u32, desired(stack, before.get(slot), &mut counted)))
            .filter(|(_, desired)| *desired != Desired::Keep)
            .collect(),
        _ => Vec::new(),
    };
    let mut effects = Vec::new();
    for effect in &outcome.effects {
        match effect {
            // The packets of vanilla's synchronizer and inventory send: the state above is
            // what they carried, and Sunrise syncs its own window.
            LiveMenuEffect::InitialData { .. }
            | LiveMenuEffect::SlotChanged { .. }
            | LiveMenuEffect::CarriedChanged { .. }
            | LiveMenuEffect::InventorySlotSent { .. }
            // `closeContainer`'s packet: the close is one of `menu_effects`.
            | LiveMenuEffect::Closed { .. } => {}
            // `CriteriaTriggers.INVENTORY_CHANGED` for a changed inventory slot: Sunrise fires
            // its own `inventory_changed` whenever the inventory changes, which the writes do.
            LiveMenuEffect::InventoryChangedCriterion { .. } => {}
            LiveMenuEffect::DataChanged { index, value, .. } => match usize::try_from(*index) {
                Ok(index) => effects.push(SideEffect::Data {
                    index,
                    value: *value,
                }),
                Err(_) => counted.push("sunrise#live_menu:data_slot"),
            },
            LiveMenuEffect::Drop {
                stack,
                retain_ownership,
                prediction: _,
            } => {
                // `drop_stack` throws as `Player.drop` does, but whether it swings for the
                // player's own client (`Prediction`) and awards `ITEM_DROPPED`/`DROP` for a
                // retained drop (`ServerPlayer.drop` offsets 9-62) isn't in its contract: the
                // stack goes out (it must not vanish) and the drop is counted.
                counted.push("sunrise#live_menu:drop");
                match api_from_components(host, stack) {
                    Some(stack) => effects.push(SideEffect::Drop {
                        stack,
                        retain_ownership: *retain_ownership,
                    }),
                    None => counted.push("sunrise#live_menu:unconvertible_stack"),
                }
            }
            LiveMenuEffect::AwardStat {
                stat_type,
                value,
                amount,
            } => match stat_type_of(stat_type) {
                Some(stat_type) => effects.push(SideEffect::AwardStat {
                    stat_type,
                    key: value.clone(),
                    amount: *amount,
                }),
                None => counted.push("sunrise#live_menu:stat_type"),
            },
            // `CriteriaTriggers.RECIPE_CRAFTED`: Sunrise lends mods their own triggers only
            // (`trigger_criterion` refuses `minecraft:`), so vanilla's can't be fired.
            LiveMenuEffect::RecipeCrafted { .. } => {
                counted.push("sunrise#live_menu:recipe_crafted_trigger")
            }
            LiveMenuEffect::AwardRecipes { recipes } => {
                effects.push(SideEffect::AwardRecipes(recipes.clone()))
            }
        }
    }
    Prepared {
        jar_index,
        jar_id: jar.row.id.clone(),
        uuid,
        window,
        slots,
        cursor,
        inventory,
        effects,
        menu_effects: outcome.menu_effects,
        counted,
        unknown_host_calls: outcome.unknown_host_calls,
        unknown_host_call_count: outcome.unknown_host_call_count,
        failure: outcome.failure,
    }
}

/// A stack an effect reported (`StackComponents`) as a mod API stack.
fn api_from_components(
    host: &dyn ServerHost,
    stack: &runtime::StackComponents,
) -> Option<ItemStack> {
    if stack.opaque_components || stack.count <= 0 {
        return None;
    }
    let mut scratch = StdHost::new();
    let value = scratch.make_host_item_stack_with_components(
        None,
        &stack.item_id,
        stack.count,
        stack.patch.clone(),
        false,
        Vec::new(),
    );
    crate::client::api_stack(host, &scratch, &value)
}

/// A vanilla `StatType` registry id as Sunrise's [`StatType`].
fn stat_type_of(id: &str) -> Option<StatType> {
    Some(match id {
        "minecraft:mined" => StatType::Mined,
        "minecraft:crafted" => StatType::Crafted,
        "minecraft:used" => StatType::Used,
        "minecraft:broken" => StatType::Broken,
        "minecraft:picked_up" => StatType::PickedUp,
        "minecraft:dropped" => StatType::Dropped,
        "minecraft:killed" => StatType::Killed,
        "minecraft:killed_by" => StatType::KilledBy,
        "minecraft:custom" => StatType::Custom,
        _ => return None,
    })
}

/// Carry a [`Prepared`] out on Sunrise without the state lock (the writes fire hooks this mod
/// listens to): the menu's owned slots and cursor, then the inventory, then the side effects, in
/// one transaction, then the opens and closes the Java call made.
pub(crate) fn apply(host: &mut dyn ServerHost, prepared: &Prepared) -> Applied {
    let mut applied = Applied::default();
    let Some(player) = host.player(prepared.uuid) else {
        applied.counted.push("sunrise#live_menu:player_unavailable");
        return applied;
    };
    let menu = player
        .container_menu(&*host)
        .filter(|menu| menu.container_id(&*host) == Some(prepared.window));
    let written = host.transaction(|host, tx| {
        let mut counted = Vec::new();
        if let Some(menu) = menu {
            for (slot, desired) in prepared.slots.iter().enumerate() {
                // Only the menu's own slots: the player's are written as the inventory below.
                if !matches!(
                    menu.slot_backing(&*host, slot),
                    Some(MenuSlotBacking::Owned(_))
                ) {
                    continue;
                }
                let place = ItemPlace::MenuSlot { menu, slot };
                write_place(host, tx, &place, desired, &mut counted);
            }
            write_place(
                host,
                tx,
                &ItemPlace::PlayerCursor { menu },
                &prepared.cursor,
                &mut counted,
            );
        }
        for (slot, desired) in &prepared.inventory {
            let Desired::Stack(stack) = desired else {
                continue;
            };
            if player
                .set_inventory_slot(host, *slot, stack.clone())
                .is_err()
            {
                counted.push("sunrise#live_menu:inventory_write_refused");
            }
        }
        for effect in &prepared.effects {
            match effect {
                SideEffect::Drop {
                    stack,
                    retain_ownership,
                } => match player.drop_stack(host, stack.clone(), *retain_ownership) {
                    Ok(None) => {}
                    // Refused (a cancelled toss, a player in another level): vanilla's drop
                    // had no refusal, so the stack goes back to the inventory and is counted.
                    _ => counted.push("sunrise#live_menu:drop_refused"),
                },
                SideEffect::AwardStat {
                    stat_type,
                    key,
                    amount,
                } => {
                    if !player.award_stat(host, *stat_type, key, *amount) {
                        counted.push("sunrise#live_menu:award_stat_refused");
                    }
                }
                SideEffect::AwardRecipes(recipes) => {
                    let ids: Vec<&str> = recipes.iter().map(String::as_str).collect();
                    if player.award_recipes(host, &ids).is_none() {
                        counted.push("sunrise#live_menu:award_recipes_refused");
                    }
                }
                SideEffect::Data { index, value } => match menu {
                    Some(menu) if menu.set_data_slot(host, *index, *value).is_ok() => {}
                    _ => counted.push("sunrise#live_menu:data_slot_refused"),
                },
            }
        }
        Ok::<_, ()>(counted)
    });
    match written {
        Ok(counted) => applied.counted.extend(counted),
        Err(_) => applied
            .counted
            .push("sunrise#live_menu:transaction_refused"),
    }
    applied.menu_applied =
        block_use::apply_menu_effects(host, prepared.uuid, &prepared.menu_effects);
    applied
}

/// Make `place` hold `desired`: take out what is there (unless it is already that stack), then
/// put the stack in. A write Sunrise takes only in part is counted.
fn write_place(
    host: &mut dyn ServerHost,
    tx: &TransactionContext,
    place: &ItemPlace,
    desired: &Desired,
    counted: &mut Vec<&'static str>,
) {
    let want = match desired {
        Desired::Stack(stack) => stack.as_ref(),
        Desired::Keep | Desired::Unconvertible => return,
    };
    let held = place.amount(host);
    let have = place.with_stack(host, |view| view.project()).flatten();
    if held > 0 && have.is_none() {
        counted.push("sunrise#live_menu:unreadable_slot");
        return;
    }
    if have.as_ref() == want {
        return;
    }
    if let Some(have) = &have {
        let taken = place.extract(host, tx, &ItemResource::of(have), have.count());
        if !matches!(taken, Ok(count) if count == have.count()) {
            counted.push("sunrise#live_menu:slot_write_refused");
            return;
        }
    }
    if let Some(want) = want {
        let put = place.insert(host, tx, &ItemResource::of(want), want.count());
        if !matches!(put, Ok(count) if count == want.count()) {
            counted.push("sunrise#live_menu:slot_write_refused");
        }
    }
}

/// Record a carried-out call on its jar: tallies, counted calls and hooks, and what Sunrise did
/// with the call's opens and closes.
pub(crate) fn record(state: &mut State, prepared: Prepared, applied: Applied) {
    let Some(jar) = state
        .jars
        .get_mut(prepared.jar_index)
        .filter(|jar| jar.row.id == prepared.jar_id)
    else {
        return;
    };
    jar.row.block_use.menu_calls += 1;
    jar.row.block_use.unknown_host_calls += prepared.unknown_host_call_count;
    if prepared.unknown_host_call_count > 0 {
        log::warn!(
            "{LOG}: {}: menu {} leaned on {} unmodelled host call(s): {:?}",
            jar.row.id,
            prepared.window,
            prepared.unknown_host_call_count,
            prepared.unknown_host_calls
        );
    }
    if let Some(failure) = &prepared.failure {
        // Vanilla's handler would throw too (`ReportedException("Container click")`); what the
        // Java menu did before it is mirrored all the same.
        jar.row.block_use.uses_failed += 1;
        log::warn!(
            "{LOG}: {}: menu {} call threw: {failure}",
            jar.row.id,
            prepared.window
        );
    }
    for tag in prepared.counted.into_iter().chain(applied.counted) {
        jar.host.note_unknown_game_hook(tag);
    }
    for applied in applied.menu_applied {
        block_use::record_menu_outcome(jar, prepared.uuid, applied);
    }
}

/// `ServerPlayer.tick` (`broadcastChanges`, then `stillValid` → `closeContainer`) and
/// `doTick`'s second `stillValid`, for the Java menu a jar holds for the player, once per player
/// tick (`player_tick_post`).
pub(crate) fn player_tick(java: &crate::HorizonJava, host: &mut dyn ServerHost, player: PlayerRef) {
    let uuid = player.uuid();
    for call in [Call::Tick, Call::Validity] {
        let prepared = {
            let mut state = java.lock();
            let Some(window) = state.jars.iter().find_map(|jar| {
                jar.host
                    .player_open_menu(uuid)
                    .map(|open| open.container_id)
            }) else {
                return;
            };
            prepare(&mut state, &*host, uuid, window, call)
        };
        let Some(prepared) = prepared else {
            return;
        };
        let applied = apply(host, &prepared);
        record(&mut java.lock(), prepared, applied);
    }
}

/// The player closed Sunrise window `window` (or something closed it for them): the jar that
/// holds its Java menu runs `doCloseContainer` with the player's facts, and Sunrise gets the
/// inventory and the side effects its `removed` made. `false` when no jar holds the window with
/// a player Sunrise can still read (the caller then runs the close without facts).
pub(crate) fn close_held(
    java: &crate::HorizonJava,
    host: &mut dyn ServerHost,
    uuid: u128,
    window: i32,
) -> bool {
    if host.player(uuid).is_none() {
        return false;
    }
    let prepared = {
        let mut state = java.lock();
        if !state.jars.iter().any(|jar| {
            jar.host
                .player_open_menu(uuid)
                .is_some_and(|open| open.container_id == window)
        }) {
            return false;
        }
        prepare(&mut state, &*host, uuid, window, Call::Close)
    };
    let Some(prepared) = prepared else {
        return true;
    };
    let applied = apply(host, &prepared);
    record(&mut java.lock(), prepared, applied);
    true
}

/// What a window just opened for a held Java menu should show: the Java menu's slots and cursor
/// (`ServerPlayer$1.sendInitialData`), under the state lock. `None` when the jar holds no menu
/// for that window, or its contents aren't Decaf's to read.
pub(crate) fn opening_contents(
    jar: &mut LoadedJar,
    host: &dyn ServerHost,
    jar_index: usize,
    uuid: u128,
    window: i32,
) -> Option<(Prepared, usize)> {
    let open = jar.host.player_open_menu(uuid)?;
    if open.container_id != window {
        return None;
    }
    let (carried, slots) = jar.host.held_menu_contents(uuid)?;
    let count = slots.len();
    let outcome = LiveMenuOutcome {
        container_id: window,
        effects: Vec::new(),
        menu_effects: Vec::new(),
        inventory: None,
        carried: Some(carried),
        slots: Some(slots),
        handled: true,
        unknown_host_calls: BTreeMap::new(),
        unknown_host_call_count: 0,
        failure: None,
    };
    let facts = MenuPlayerFacts::default();
    Some((
        mirror(jar, host, jar_index, uuid, window, &facts, outcome),
        count,
    ))
}
