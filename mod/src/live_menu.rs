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
//! Cancelling Sunrise's click also means Sunrise never tells other mods' `menu_slot_take` parts of
//! a take: that hook is the engine's to dispatch (its `BlockEntityScope` subject is built by the
//! game; a mod can't raise it). Each Java `Slot.onTake` is counted instead
//! (`sunrise#live_menu:menu_slot_take_not_dispatched`), so the gap shows.
//!
//! # The menu type
//!
//! The bake declares a jar's menu type from the menu its `MenuType` builds
//! ([`runtime::NativeJar::menu_type_declarations`]) only when the bytecode proves the layout can't
//! vary (`runtime/menu_structure.rs`): one owned slot per container slot, in Java's order, then
//! the player's 36 slots in vanilla's standard order, so a Java slot index is the Sunrise slot
//! index. It declares no `results`: a processing menu's close would make Sunrise return the
//! inputs itself, where the Java menu's `removed` decides that. Before a window opens, the live
//! Java menu's layout is compared with that declaration (built here the same way, once per jar
//! and type), and its initial data slot values (`sendInitialData`) are what the window opens
//! with: a mismatch, or data Decaf doesn't know, refuses the open (counted), never a mismatched
//! window.
//!
//! # The player
//!
//! Each call reads the player from Sunrise into [`runtime::MenuPlayerFacts`]: their 43 inventory
//! slots, selected hotbar slot, creative `instabuild`, position, eye position and block
//! interaction range, the block at the menu's `ContainerLevelAccess` position, their `tickCount`,
//! the menu's cursor, and their `containerCounter`, read fresh before every call that may open a
//! menu, so Java's window ids can't drift from Sunrise's. A fact Sunrise can't give stays
//! unknown, and the Java step that needs it counts.
//!
//! # Ticks
//!
//! 26.3 runs a player's menu twice a tick: `ServerPlayer.tick` (the level's entity tick)
//! `broadcastChanges` and checks `stillValid` (offsets 38-65, after `gameMode.tick`), and
//! `ServerPlayer.doTick` (the connection tick, after every level) checks `stillValid` again right
//! after `Player.tick` (offsets 18-38). The first runs at Sunrise's `entity_tick_pre` for the
//! player ([`entity_tick_pre`]; the player type is told alone, `entity_tick_each`), before
//! `gameMode.tick`: if that tick changes the block at the menu's position (a delayed break), the
//! check saw the block before the change, and `entity_tick_post` counts it. The second runs at
//! `player_tick_post` ([`player_tick`]), which Sunrise fires after the rest of `doTick` (its
//! food, statistics and score updates), none of which moves the player, changes a block or an
//! inventory slot: the facts the check reads are the ones vanilla's sees. A failed check closes
//! the Sunrise window after the Java close returned the cursor and inputs.
//!
//! # Closes
//!
//! A window that closes runs the Java close (`removed`, `clearContainer`) from `menu_closed`
//! ([`close_held`]) with the flags vanilla's `dropOrPlaceInInventory` reads, matching Sunrise's own
//! rule for its processing menus (`MenuRemoval::drops_items`, which the mod API doesn't carry):
//! a live player, or a dimension change, gets the inputs back; a dead player's close (Sunrise
//! closes a dead player's menu with `Killed`) is a removal for a reason other than a dimension
//! change, and one Sunrise tells after the player left (`menu_closed` follows the disconnect)
//! is a disconnect: both drop them, at the player's last known position when Sunrise has no
//! player to drop from. Sunrise already put the cursor back (or dropped it), so the call starts
//! from an empty one ([`runtime::MenuPlayerFacts::carried`]). A logout or respawn that reaches
//! this mod while a Java menu is still held closes it the same way first, so the player's new
//! `ServerPlayer` never loses what the old one's menu held.
//!
//! Nothing here names a mod.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use mod_api::{
    Flow, HostTransaction, ItemPlace, ItemResource, ItemStack, LevelRef, MenuClickedEvent,
    MenuPart, MenuRef, MenuSlotBacking, PlayerRef, ServerHost, StatType, TransactionContext,
};

use crate::block_use::{self, MenuApplied};
use crate::runtime::{
    self, BlockUsePlayerData, ContainerInput, ExecutionBudget, LiveMenuEffect, LiveMenuOutcome,
    MenuClick, MenuEffect, MenuLayout, MenuPlayerFacts, MenuSlotRole, PlayerContainer, StdHost,
    Value,
};
use crate::{JAR_BUDGET, LOG, LoadedJar, State, caught};

/// What the bridge keeps between calls.
#[derive(Default)]
pub(crate) struct Bridge {
    /// Each player's last read: where a close finds them once Sunrise no longer can.
    seen: BTreeMap<u128, Seen>,
    /// Each jar's menu types as it declares them, by jar id then menu type id.
    layouts: BTreeMap<String, BTreeMap<String, Result<MenuLayout, String>>>,
    /// What each player's `ServerPlayer.tick` check read at `entity_tick_pre`.
    tick_facts: BTreeMap<u128, TickFacts>,
}

impl Bridge {
    /// A new server: its players are new, and so are their positions.
    pub(crate) fn reset_server(&mut self) {
        self.seen.clear();
        self.tick_facts.clear();
    }
}

/// A player as the bridge last read them.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    level: LevelRef,
    dimension: String,
    position: Option<[f64; 3]>,
    eye: Option<[f64; 3]>,
    name: String,
    entity_id: i32,
}

/// Why a window closed, as vanilla's `dropOrPlaceInInventory` reads it (`isRemoved`, its
/// `RemovalReason`, `hasDisconnected`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Removal {
    /// The player closed it, walked away, opened another, or changed dimension
    /// (`CHANGED_DIMENSION` places back too): the inputs go back to the inventory.
    Live,
    /// The player died (`Player.remove(KILLED)`): the inputs drop.
    Killed,
    /// The player left (`hasDisconnected()`): the inputs drop.
    Disconnected,
}

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
                None,
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
    /// `doCloseContainer` after Sunrise closed the window (`handleContainerClose`, or the
    /// player's removal).
    Close(Removal),
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
    /// Where the player was, for a drop Sunrise has no player to make.
    pub(crate) seen: Option<Seen>,
    /// Whether the player can throw a drop themselves (`drop_stack`): alive and on the server.
    /// A dead or departed player's drop is spawned where they were instead (counted).
    pub(crate) throws_from_player: bool,
    /// The menus this call opened, checked against their types (`opening_check`).
    pub(crate) openings: Vec<block_use::Opening>,
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
    pub(crate) seen: Option<Seen>,
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
        // disconnected. A close decides those from why it closed (`Removal`).
        removed: false,
        removed_for_dimension_change: false,
        disconnected: false,
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
    let level = entity.level(host);
    let seen = level.map(|level| Seen {
        level,
        dimension: level.dimension_id().to_owned(),
        position: facts.position,
        eye: facts.eye_position,
        name: data.name.clone(),
        entity_id: entity.id() as i32,
    });
    PlayerRead {
        facts,
        data,
        dimension: level.map(|level| level.dimension_id().to_owned()),
        seen,
    }
}

/// A player whose `ServerPlayer` is gone (they left, or a respawn replaced the dead one), as
/// last seen: removed (and disconnected, for a disconnect), with no inventory to read (such a
/// close drops what it hands back).
fn departed_read(
    seen: &Seen,
    uuid: u128,
    incarnation: Option<(u32, u32)>,
    removal: Removal,
) -> PlayerRead {
    PlayerRead {
        facts: MenuPlayerFacts {
            eye_position: seen.eye,
            position: seen.position,
            removed: removal != Removal::Live,
            disconnected: removal == Removal::Disconnected,
            carried: Some(StdHost::empty_item_stack()),
            ..MenuPlayerFacts::default()
        },
        data: BlockUsePlayerData {
            uuid,
            name: seen.name.clone(),
            entity_id: Some(seen.entity_id),
            incarnation,
            container: PlayerContainer::Inventory,
            sneaking: false,
            spectator: false,
            main_hand: None,
            off_hand: None,
        },
        dimension: Some(seen.dimension.clone()),
        seen: Some(seen.clone()),
    }
}

/// The raw block state at the held menu's access position, as Sunrise spells it.
fn access_block_now(host: &dyn ServerHost, player: PlayerRef, pos: [i32; 3]) -> Option<String> {
    let level = player.entity().level(host)?;
    let state = level.block_state(host, pos)?;
    host.block_state_string(state)
}

/// Run `call` on the Java menu window `window` that a jar holds for the player, under the state
/// lock: what Sunrise should mirror. `None` when no jar holds that window.
pub(crate) fn prepare(
    state: &mut State,
    host: &dyn ServerHost,
    uuid: u128,
    window: i32,
    call: Call,
    departed: Option<Seen>,
) -> Option<Prepared> {
    let jar_index = state.jars.iter().position(|jar| {
        jar.host
            .player_open_menu(uuid)
            .is_some_and(|open| open.container_id == window)
    })?;
    let incarnation = state.lives.get(&uuid).copied();
    let access_pos = state.jars[jar_index]
        .host
        .player_open_menu(uuid)?
        .access_pos;
    let gone = departed.is_some() || host.player(uuid).is_none();
    let read = match (host.player(uuid), &call) {
        // The old `ServerPlayer` of a respawn, where it was (`event.original`).
        (_, Call::Close(removal)) if departed.is_some() => {
            departed_read(departed.as_ref()?, uuid, incarnation, *removal)
        }
        (Some(player), _) => {
            let jar = &mut state.jars[jar_index];
            // `ServerPlayer.containerCounter` is Sunrise's, read fresh: whatever opened since
            // (a crafting table, a merchant, another mod) moved it, and a Java `openMenu` this
            // call makes takes the id Sunrise's next open takes.
            if let Some(counter) = player.container_counter(host) {
                jar.host.server_player_container_opened(uuid, Some(counter));
            }
            let menu = player
                .container_menu(host)
                .filter(|menu| menu.container_id(host) == Some(window));
            let (container, cursor) = match call {
                // Sunrise already put the cursor back into the inventory (or dropped it).
                Call::Close(_) => (PlayerContainer::Inventory, Some(None)),
                _ => (PlayerContainer::Open(window), menu.map(Some)),
            };
            let mut read = read_player(
                host,
                &mut jar.host,
                incarnation,
                player,
                access_pos,
                container,
                cursor,
            );
            match call {
                // `Player.remove(KILLED)`: removed, for a reason other than a dimension change.
                Call::Close(Removal::Killed) => read.facts.removed = true,
                Call::Close(Removal::Disconnected) => {
                    read.facts.removed = true;
                    read.facts.disconnected = true;
                }
                _ => {}
            }
            if let Some(seen) = read.seen.clone() {
                state.live_menus.seen.insert(uuid, seen);
            }
            read
        }
        (None, Call::Close(removal)) => match state.live_menus.seen.get(&uuid) {
            Some(seen) => departed_read(seen, uuid, incarnation, *removal),
            None => {
                state.jars[jar_index]
                    .host
                    .note_unknown_game_hook("sunrise#live_menu:departed_unseen");
                return None;
            }
        },
        (None, _) => {
            state.jars[jar_index]
                .host
                .note_unknown_game_hook("sunrise#live_menu:player_unavailable");
            return None;
        }
    };
    let jar = &mut state.jars[jar_index];
    // The menu's own slots, the only ones Sunrise's `menu_slot_take` hears a take from.
    let owned_slots = jar
        .host
        .held_menu_layout(uuid)
        .map(|layout| layout.owned_slots())
        .ok();
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
        Call::Close(_) => jar.jar.close_player_menu_live(
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
    // A menu this call opened (a click's, or a `removed` that opens another) is checked before
    // its window opens, as a block use's is, with the initial data its `initMenu` sent.
    let opening_effects = outcome
        .menu_effects
        .iter()
        .any(|effect| matches!(effect, MenuEffect::Opened(_)))
        .then(|| outcome.effects.clone());
    let mut prepared = mirror(
        jar,
        host,
        jar_index,
        uuid,
        window,
        &read.facts,
        outcome,
        owned_slots,
    );
    prepared.seen = read.seen;
    prepared.throws_from_player = !gone
        && !read.facts.dead
        && !matches!(call, Call::Close(Removal::Killed | Removal::Disconnected));
    if let Some(effects) = opening_effects {
        prepared.openings = opening_check(state, jar_index, uuid, &effects)
            .into_iter()
            .collect();
    }
    Some(prepared)
}

/// Turn a call's outcome into what Sunrise shows and does: the state Java left, converted to
/// mod API stacks, and its effects.
#[allow(clippy::too_many_arguments)]
fn mirror(
    jar: &mut LoadedJar,
    host: &dyn ServerHost,
    jar_index: usize,
    uuid: u128,
    window: i32,
    facts: &MenuPlayerFacts,
    outcome: LiveMenuOutcome,
    owned_slots: Option<usize>,
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
            // `Slot.onTake` on one of the menu's own slots: where Sunrise would tell
            // `menu_slot_take` parts of the take (it never tells takes from the player's
            // inventory). The engine dispatches that hook (a mod can't), and this click is the
            // Java menu's, so no part hears it: counted per take, so the gap shows. A take from
            // a slot Decaf can't place is counted too.
            LiveMenuEffect::SlotTaken { slot } => {
                let own = match (slot, owned_slots) {
                    (Some(slot), Some(owned)) => usize::try_from(*slot).is_ok_and(|s| s < owned),
                    _ => true,
                };
                if own {
                    counted.push("sunrise#live_menu:menu_slot_take_not_dispatched");
                }
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
        seen: None,
        throws_from_player: true,
        openings: Vec::new(),
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
    let player = host.player(prepared.uuid);
    let menu = player
        .and_then(|player| player.container_menu(&*host))
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
            let written = player.is_some_and(|player| {
                player
                    .set_inventory_slot(host, *slot, stack.clone())
                    .is_ok()
            });
            if !written {
                counted.push("sunrise#live_menu:inventory_write_refused");
            }
        }
        for effect in &prepared.effects {
            match effect {
                SideEffect::Drop {
                    stack,
                    retain_ownership,
                } => match drop_in_world(
                    host,
                    player.filter(|_| prepared.throws_from_player),
                    prepared.seen.as_ref(),
                    stack,
                    *retain_ownership,
                ) {
                    Dropped::Thrown => {}
                    Dropped::Spawned => counted.push("sunrise#live_menu:drop_without_player_throw"),
                    Dropped::Refused => counted.push("sunrise#live_menu:drop_refused"),
                    Dropped::Nowhere => counted.push("sunrise#live_menu:drop_lost"),
                },
                SideEffect::AwardStat {
                    stat_type,
                    key,
                    amount,
                } => {
                    if !player
                        .is_some_and(|player| player.award_stat(host, *stat_type, key, *amount))
                    {
                        counted.push("sunrise#live_menu:award_stat_refused");
                    }
                }
                SideEffect::AwardRecipes(recipes) => {
                    let ids: Vec<&str> = recipes.iter().map(String::as_str).collect();
                    if !player.is_some_and(|player| player.award_recipes(host, &ids).is_some()) {
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
    applied.menu_applied = block_use::apply_menu_effects(
        host,
        prepared.uuid,
        &prepared.menu_effects,
        &prepared.openings,
    );
    applied
}

/// How a Java drop went out.
enum Dropped {
    /// The player threw it (`drop_stack`).
    Thrown,
    /// Sunrise refused the throw (a mod cancelled the toss, `item_toss`): it stays refused.
    Refused,
    /// The player is dead or gone, so nothing can throw it: spawned where they were.
    Spawned,
    /// Nowhere to put it.
    Nowhere,
}

/// `ServerPlayer.drop(stack, false, SERVER_ONLY)` for a Java drop. A live player throws it
/// through Sunrise's `drop_stack` (`player` given), and a refusal (a cancelled toss) is left
/// refused. A dead or departed player (`player` `None`) has no throw: vanilla's
/// `createItemStackToDrop` would give the item a velocity from the player's rotation and three
/// `nextFloat`s of the player's random, which no fact here carries, so it is spawned at the
/// player's last known `(x, eyeY - 0.3, z)` with the 40-tick pickup delay a player's throw has,
/// its own motion (counted by the caller), so nothing is lost.
fn drop_in_world(
    host: &mut dyn ServerHost,
    player: Option<PlayerRef>,
    seen: Option<&Seen>,
    stack: &ItemStack,
    retain_ownership: bool,
) -> Dropped {
    if let Some(player) = player {
        return match player.drop_stack(host, stack.clone(), retain_ownership) {
            Ok(None) => Dropped::Thrown,
            _ => Dropped::Refused,
        };
    }
    let Some((level, position, eye)) =
        seen.and_then(|seen| Some((seen.level, seen.position?, seen.eye?)))
    else {
        return Dropped::Nowhere;
    };
    match level.spawn_item(
        host,
        position[0],
        eye[1] - 0.3,
        position[2],
        stack.clone(),
        40,
    ) {
        Ok(Some(_)) => Dropped::Spawned,
        _ => Dropped::Nowhere,
    }
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

/// The window a jar holds a Java menu in for the player, if any.
fn held_window(state: &State, uuid: u128) -> Option<i32> {
    state.jars.iter().find_map(|jar| {
        jar.host
            .player_open_menu(uuid)
            .map(|open| open.container_id)
    })
}

/// Whether any jar holds a Java menu for any player: what `entity_tick_listened` answers, so the
/// game keeps no entity tick list while no menu needs one.
pub(crate) fn any_held(java: &crate::HorizonJava) -> bool {
    let state = java.lock();
    state.jars.iter().any(|jar| {
        state
            .lives
            .keys()
            .any(|uuid| jar.host.player_open_menu(*uuid).is_some())
    })
}

/// Run `call` on the player's held Java menu and carry it out: `false` when none is held.
fn run_held(java: &crate::HorizonJava, host: &mut dyn ServerHost, uuid: u128, call: Call) -> bool {
    let prepared = {
        let mut state = java.lock();
        let Some(window) = held_window(&state, uuid) else {
            return false;
        };
        prepare(&mut state, &*host, uuid, window, call, None)
    };
    if let Some(prepared) = prepared {
        let applied = apply(host, &prepared);
        record(&mut java.lock(), prepared, applied);
    }
    true
}

/// What a `stillValid` may read of the player and the menu's block, as Sunrise has it: compared
/// before and after the player's tick (`entity_tick_pre` / `entity_tick_post`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TickFacts {
    access_block: Option<String>,
    inventory: Option<Vec<Option<ItemStack>>>,
    selected: Option<usize>,
    position: Option<[f64; 3]>,
    eye: Option<[f64; 3]>,
    reach: Option<f64>,
    instabuild: Option<bool>,
    health: Option<f32>,
}

fn tick_facts(host: &dyn ServerHost, player: PlayerRef, access_pos: Option<[i32; 3]>) -> TickFacts {
    let entity = player.entity();
    let living = host.living(entity);
    TickFacts {
        access_block: access_pos.and_then(|pos| access_block_now(host, player, pos)),
        inventory: player
            .with_inventory(host, |view| {
                (0..INVENTORY_SLOTS)
                    .map(|slot| match view.get(slot) {
                        None => Some(None),
                        Some(stack) => stack.project().map(Some),
                    })
                    .collect::<Option<Vec<Option<ItemStack>>>>()
            })
            .flatten(),
        selected: player.selected_slot(host),
        position: entity.position(host),
        eye: entity.eye_position(host),
        reach: living.and_then(|living| living.attribute_value(host, BLOCK_INTERACTION_RANGE)),
        instabuild: player.has_infinite_materials(host),
        health: living.and_then(|living| living.health(host)),
    }
}

/// `entity_tick_pre` for players about to tick: `ServerPlayer.tick`'s `broadcastChanges` and
/// first `stillValid` (offsets 38-65), with the facts the check read kept for
/// [`entity_tick_post`].
pub(crate) fn entity_tick_pre(
    java: &crate::HorizonJava,
    host: &mut dyn ServerHost,
    entities: &[u32],
    kinds: &[&str],
) {
    for uuid in ticking_players(host, entities, kinds) {
        let access = {
            let state = java.lock();
            state
                .jars
                .iter()
                .find_map(|jar| jar.host.player_open_menu(uuid).map(|open| open.access_pos))
        };
        let (Some(access), Some(player)) = (access, host.player(uuid)) else {
            continue;
        };
        let before = tick_facts(&*host, player, access);
        java.lock().live_menus.tick_facts.insert(uuid, before);
        run_held(java, host, uuid, Call::Tick);
    }
}

/// `entity_tick_post` for players that ticked: vanilla checks `stillValid` after `gameMode.tick`,
/// which the check at `entity_tick_pre` ran before. If anything it may read changed during the
/// tick (the block at the menu's position, a delayed break; the inventory or a held item, which
/// a jar `stillValid` override may read; the player's position, reach, mode or health), the
/// check may have answered differently: counted.
pub(crate) fn entity_tick_post(
    java: &crate::HorizonJava,
    host: &mut dyn ServerHost,
    entities: &[u32],
    kinds: &[&str],
) {
    for uuid in ticking_players(host, entities, kinds) {
        let mut state = java.lock();
        let Some(before) = state.live_menus.tick_facts.remove(&uuid) else {
            continue;
        };
        let Some(jar_index) = state
            .jars
            .iter()
            .position(|jar| jar.host.player_open_menu(uuid).is_some())
        else {
            continue;
        };
        let access = state.jars[jar_index]
            .host
            .player_open_menu(uuid)
            .and_then(|open| open.access_pos);
        let Some(player) = host.player(uuid) else {
            continue;
        };
        if tick_facts(&*host, player, access) != before {
            state.jars[jar_index]
                .host
                .note_unknown_game_hook("sunrise#live_menu:still_valid_before_game_mode_tick");
        }
    }
}

/// The players among an entity tick's entities, by UUID.
fn ticking_players(host: &dyn ServerHost, entities: &[u32], kinds: &[&str]) -> Vec<u128> {
    let players = host.players().server_players();
    entities
        .iter()
        .zip(kinds)
        .filter(|(_, kind)| **kind == "minecraft:player")
        .filter_map(|(id, _)| {
            players
                .iter()
                .find(|player| player.entity().id() == *id)
                .map(|player| player.uuid())
        })
        .collect()
}

/// `ServerPlayer.doTick`'s second `stillValid` (offsets 18-38), from `player_tick_post`.
pub(crate) fn player_tick(java: &crate::HorizonJava, host: &mut dyn ServerHost, player: PlayerRef) {
    run_held(java, host, player.uuid(), Call::Validity);
}

/// Sunrise closed window `window` (the player closed it, walked away, died, or left): the jar
/// that holds its Java menu runs `doCloseContainer` with the flags vanilla would see. A player
/// Sunrise still has is a live close, or a killed one when they are dying (Sunrise closes a dead
/// player's menu with `MenuRemoval::Killed`); one it no longer has left (Sunrise tells the close
/// after the disconnect). `false` when no jar holds that window.
pub(crate) fn close_held(
    java: &crate::HorizonJava,
    host: &mut dyn ServerHost,
    uuid: u128,
    window: i32,
) -> bool {
    let removal = match host.player(uuid) {
        None => Removal::Disconnected,
        Some(player) => {
            let dying = host
                .living(player.entity())
                .and_then(|living| living.health(&*host))
                .is_some_and(|health| health <= 0.0);
            if dying {
                Removal::Killed
            } else {
                Removal::Live
            }
        }
    };
    let prepared = {
        let mut state = java.lock();
        if held_window(&state, uuid) != Some(window) {
            return false;
        }
        prepare(&mut state, &*host, uuid, window, Call::Close(removal), None)
    };
    if let Some(prepared) = prepared {
        let applied = apply(host, &prepared);
        record(&mut java.lock(), prepared, applied);
    }
    true
}

/// The player's `ServerPlayer` is going (a logout, a respawn's new player): a Java menu a jar
/// still holds for them is closed first, as vanilla's `Player.remove` does (`doCloseContainer`),
/// so what it holds goes back or drops instead of vanishing with the old object. `original` is
/// where the old object was (a respawn's `event.original`: its level and position), when
/// Sunrise's player handle already names the new one.
pub(crate) fn close_for_removal(
    java: &crate::HorizonJava,
    host: &mut dyn ServerHost,
    uuid: u128,
    removal: Removal,
    original: Option<(LevelRef, [f64; 3])>,
) {
    let window = held_window(&java.lock(), uuid);
    if let Some(window) = window {
        let departed = original.map(|(level, position)| {
            let state = java.lock();
            let last = state.live_menus.seen.get(&uuid);
            // The old player's eye height, as last seen (the original lends no eye position).
            let eye = last
                .and_then(|seen| Some((seen.eye?, seen.position?)))
                .map(|(eye, at)| [position[0], position[1] + (eye[1] - at[1]), position[2]]);
            Seen {
                level,
                dimension: level.dimension_id().to_owned(),
                position: Some(position),
                eye,
                name: last.map_or_else(|| "Player".to_owned(), |seen| seen.name.clone()),
                entity_id: last.map_or(0, |seen| seen.entity_id),
            }
        });
        let prepared = prepare(
            &mut java.lock(),
            &*host,
            uuid,
            window,
            Call::Close(removal),
            departed,
        );
        if let Some(prepared) = prepared {
            let applied = apply(host, &prepared);
            record(&mut java.lock(), prepared, applied);
        }
    }
}

/// Whether the Java menu a block use left held can open as its Sunrise menu type, under the
/// state lock: its live layout must be its type's declared one (built here as the bake builds
/// it, once per jar and type), and the window opens with its initial data slot values (what
/// `ServerPlayer$1.sendInitialData` sent after the contents). `None` when no menu is held.
pub(crate) fn opening_check(
    state: &mut State,
    jar_index: usize,
    uuid: u128,
    live_effects: &[LiveMenuEffect],
) -> Option<block_use::Opening> {
    let jar = &mut state.jars[jar_index];
    let open = jar.host.player_open_menu(uuid)?;
    let container_id = open.container_id;
    let checked = match open.menu_type_id.clone() {
        None => Err("its MenuType is not registered".to_owned()),
        Some(menu_type) => {
            let layouts = state
                .live_menus
                .layouts
                .entry(jar.row.id.clone())
                .or_insert_with(|| {
                    jar.jar
                        .menu_type_declarations(&mut jar.host.clone())
                        .into_iter()
                        .map(|declaration| (declaration.id, declaration.layout))
                        .collect()
                });
            match layouts.get(&menu_type) {
                None => Err(format!("`{menu_type}` is not a menu type of this jar")),
                Some(Err(reason)) => Err(format!("`{menu_type}` has no declaration: {reason}")),
                Some(Ok(declared)) => match jar.host.held_menu_layout(uuid) {
                    Err(reason) => Err(format!("the open `{menu_type}` menu's layout: {reason}")),
                    Ok(live) if live != *declared => Err(format!(
                        "the open `{menu_type}` menu's layout is not its type's declared one"
                    )),
                    Ok(live) => initial_data(live_effects, container_id, live.data_slots),
                },
            }
        }
    };
    Some(block_use::Opening {
        container_id,
        data: checked,
    })
}

/// The data slot values `sendInitialData` sent for window `container_id`: one `DataChanged` per
/// data slot, in order, right after the contents.
fn initial_data(
    effects: &[LiveMenuEffect],
    container_id: i32,
    count: usize,
) -> Result<Vec<i32>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let start = effects
        .iter()
        .position(|effect| {
            matches!(effect, LiveMenuEffect::InitialData { container_id: id, .. } if *id == container_id)
        })
        .ok_or_else(|| "its initial data slot values were not sent (no player facts)".to_owned())?;
    let values: Vec<i32> = effects[start + 1..]
        .iter()
        .map_while(|effect| match effect {
            LiveMenuEffect::DataChanged {
                container_id: id,
                value,
                ..
            } if *id == container_id => Some(*value),
            _ => None,
        })
        .collect();
    if values.len() == count {
        Ok(values)
    } else {
        Err("its initial data slot values are not all known".to_owned())
    }
}

/// What a window just opened for a held Java menu should show: the Java menu's slots and cursor
/// (`ServerPlayer$1.sendInitialData`), and its layout for the check of Sunrise's window, under
/// the state lock. `None` when the jar holds no menu for that window, or its contents aren't
/// Decaf's to read.
pub(crate) fn opening_contents(
    jar: &mut LoadedJar,
    host: &dyn ServerHost,
    jar_index: usize,
    uuid: u128,
    window: i32,
) -> Option<(Prepared, Vec<MenuSlotRole>)> {
    let open = jar.host.player_open_menu(uuid)?;
    if open.container_id != window {
        return None;
    }
    let layout = jar.host.held_menu_layout(uuid).ok()?;
    let (carried, slots) = jar.host.held_menu_contents(uuid)?;
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
    let owned = layout.owned_slots();
    Some((
        mirror(
            jar,
            host,
            jar_index,
            uuid,
            window,
            &facts,
            outcome,
            Some(owned),
        ),
        layout.slots,
    ))
}

/// Whether Sunrise's window shows each of the Java menu's slots where Java has it: owned slot
/// `i` at index `i`, then the player's slots by `Inventory` number.
pub(crate) fn window_matches(host: &dyn ServerHost, menu: MenuRef, roles: &[MenuSlotRole]) -> bool {
    menu.slot_count(host) == Some(roles.len())
        && roles.iter().enumerate().all(|(index, role)| {
            match (role, menu.slot_backing(host, index)) {
                (MenuSlotRole::Owned { .. }, Some(MenuSlotBacking::Owned(owned))) => owned == index,
                (MenuSlotRole::Player(slot), Some(MenuSlotBacking::Player(held))) => {
                    usize::try_from(*slot).is_ok_and(|slot| slot == held)
                }
                _ => false,
            }
        })
}
