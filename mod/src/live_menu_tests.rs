//! The live Java menu bridge (`live_menu.rs`) driven through Sunrise's own hook dispatch: a
//! block's `use_without_item` (the real `BlockPart`), `menu_clicked` (`mod_api::dispatch_menu_clicked`
//! over the installed `MenuPart`), `player_tick_post` and `menu_closed` (`dispatch_*` over the
//! installed mod), on a test host that keeps a player, their inventory, their container counter
//! and one blockless owned mod menu the way Sunrise's are described: owned slots written through
//! `ItemPlace::MenuSlot`, the cursor through `ItemPlace::PlayerCursor`, the inventory through
//! `PlayerRef::set_inventory_slot`, all journalled, and a window close that puts the cursor back
//! into the inventory before `menu_closed` is heard.
//!
//! The menus are the jar's own: a javac-built two-input combiner (`demo.BenchMenu`, the same
//! sources as `crates/horizon-native-java/tests/fixtures/live_menu`, whose Java 25 oracle pins
//! its click semantics), and the real Runes 1.3.2 altar (`RUNES_JAR`).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::__private::game::Fields;
use mod_api::testing::{FakeHost, FakeJournal};
use mod_api::{
    Backend, BackendMut, BlockClick, BlockHit, BlockKey, BlockState, EntityRef, Hand, Hooks, Host,
    HostSide, Identifier, InteractionResult, InventoryView, ItemKey, ItemPlace, ItemResource,
    ItemStack, ItemStackView, LevelDimension, LevelRef, LivingRef, LogicalSide, MenuClosedEvent,
    MenuDataRefusal, MenuKey, MenuRef, MenuSlotBacking, ModEntry, ModMenuOpen, OpenMenuError,
    PlayerDataRefusal, PlayerRef, PlayerTickPostEvent, Registrar, RegistrationContext, ServerHost,
    StackWriteError, StatType, Text, TransactionContext, TransactionScopeError,
    TransactionUnavailable, WorldEpoch,
};
use mod_api::{EntityTickListenedEvent, EntityTickPostEvent, EntityTickPreEvent, Players};
use zip::write::SimpleFileOptions;

use crate::block_use::JavaBlockUsePart;
use crate::live_menu::JavaMenuPart;
use crate::runtime::{NativeJar, StdHost};
use crate::{HorizonJava, LoadedJar, State};

const UUID: u128 = 0x00c0_ffee_0000_0000_0000_0000_0000_0042;
const ENTITY_ID: u32 = 9;
const POS: [i32; 3] = [12, 65, -7];
/// The menu's own slots come first, then the player's 27 main slots, then the hotbar.
fn hotbar(container_slots: i32, slot: i32) -> i32 {
    container_slots + 27 + slot
}

// ---------------------------------------------------------------------------
// The test host
// ---------------------------------------------------------------------------

/// The one mod menu the player has open: its window id, the generation its handles carry, its
/// type, its owned slots and its cursor.
#[derive(Clone, Debug)]
struct OpenMenu {
    window: i32,
    generation: u64,
    menu: String,
    owned: Vec<Option<ItemStack>>,
    cursor: Option<ItemStack>,
    /// The data slot values the window opened with.
    data: Vec<i32>,
}

/// Everything a transaction may change, so an abort can put it back.
#[derive(Clone, Debug, Default)]
struct World {
    /// `Inventory` by `PlayerRef::with_inventory` number: 0-35, 36-39 armour, 40 offhand, 41, 42.
    inventory: Vec<Option<ItemStack>>,
    counter: i32,
    generation: u64,
    menu: Option<OpenMenu>,
    stats: Vec<(StatType, String, i32)>,
    recipes: Vec<String>,
    /// Stacks the player threw (`drop_stack`).
    drops: Vec<ItemStack>,
    /// Stacks spawned in the level (`LevelRef::spawn_item`), and where.
    spawned: Vec<(ItemStack, [f64; 3])>,
}

/// Why `drop_stack` hands a toss back (see `MenuTestHost::toss_refusal`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TossRefusal {
    /// A mod cancelled it (`item_toss`).
    Cancel,
    /// The player died after the call read them alive.
    Die,
    /// The player left (a stale handle) after the call read them.
    Vanish,
}

struct MenuTestHost {
    fake: FakeHost,
    world: World,
    journal: FakeJournal<World>,
    /// Declared mod menu types, their owned slot count and their data slot count
    /// (`data/<ns>/sunrise/menu/*.json`).
    menu_types: Vec<(String, usize, usize)>,
    /// Whether the player is on the server's list (`false` once they left).
    present: bool,
    health: f32,
    /// How `drop_stack` refuses the player's tosses, if it does: the stack comes back.
    toss_refusal: Option<TossRefusal>,
    /// The block at every position, as `/setblock` spells it.
    block: String,
    tick_count: i32,
    eye: [f64; 3],
    /// `menu_closed` notices waiting for the game to deliver them, as Sunrise queues them.
    closes: RefCell<VecDeque<(i32, Vec<Option<ItemStack>>)>>,
    /// Every `open_mod_menu` request the bridge made: (menu, title as JSON, pos).
    opens: Vec<(String, Option<[i32; 3]>)>,
}

impl MenuTestHost {
    fn new(items: &[&str], menu_types: &[(&str, usize, usize)]) -> Self {
        let mut fake = FakeHost::new();
        fake.registries
            .resolve_items(
                items
                    .iter()
                    .map(|id| Identifier::parse(id).unwrap())
                    .collect::<Vec<_>>(),
            )
            .expect("resolve test items");
        mod_api::__private::resolve_menus(menu_types.iter().map(|(id, _, _)| *id))
            .expect("resolve test menus");
        Self {
            fake,
            world: World {
                inventory: vec![None; 43],
                ..World::default()
            },
            journal: FakeJournal::default(),
            menu_types: menu_types
                .iter()
                .map(|(id, slots, data)| ((*id).to_owned(), *slots, *data))
                .collect(),
            present: true,
            health: 20.0,
            toss_refusal: None,
            block: String::new(),
            tick_count: 1200,
            eye: [12.5, 66.62, -5.5],
            closes: RefCell::default(),
            opens: Vec::new(),
        }
    }

    fn stack(&self, id: &str, count: u32) -> ItemStack {
        let key = mod_api::prelude::ItemsHost::item_key(self, &Identifier::parse(id).unwrap())
            .unwrap_or_else(|| panic!("unknown test item {id}"));
        ItemStack::new(key, count).unwrap()
    }

    fn text(&self, stack: &Option<ItemStack>) -> String {
        match stack {
            None => "empty".to_owned(),
            Some(stack) => format!("{} {}", stack.count(), stack.item().id(self).unwrap()),
        }
    }

    fn owned(&self) -> Vec<String> {
        let menu = self.world.menu.as_ref().expect("a menu is open");
        menu.owned.iter().map(|stack| self.text(stack)).collect()
    }

    fn cursor(&self) -> String {
        self.text(&self.world.menu.as_ref().expect("a menu is open").cursor)
    }

    /// How many of item `id` exist anywhere: the inventory, the open menu's slots and cursor,
    /// thrown drops and spawned items. A refused drop must leave it unchanged.
    fn total(&self, id: &str) -> u32 {
        let count = |stack: &Option<ItemStack>| match stack {
            Some(stack) if stack.item().id(self).is_some_and(|item| item == id) => stack.count(),
            _ => 0,
        };
        let menu = self.world.menu.iter().flat_map(|open| {
            open.owned
                .iter()
                .map(count)
                .chain(std::iter::once(count(&open.cursor)))
        });
        self.world.inventory.iter().map(count).sum::<u32>()
            + menu.sum::<u32>()
            + self
                .world
                .drops
                .iter()
                .map(|stack| count(&Some(stack.clone())))
                .sum::<u32>()
            + self
                .world
                .spawned
                .iter()
                .map(|(stack, _)| count(&Some(stack.clone())))
                .sum::<u32>()
    }

    /// The non-empty inventory slots, sorted.
    fn held(&self) -> Vec<String> {
        let mut held: Vec<String> = self
            .world
            .inventory
            .iter()
            .filter(|stack| stack.is_some())
            .map(|stack| self.text(stack))
            .collect();
        held.sort();
        held
    }

    fn player_ref(&self) -> PlayerRef {
        mod_api::__private::game::player_ref(EntityRef::new(WorldEpoch::new(1), ENTITY_ID), UUID)
    }

    fn menu_is(&self, menu: MenuRef) -> Option<&OpenMenu> {
        let fields = menu.fields();
        self.world
            .menu
            .as_ref()
            .filter(|open| open.window == fields.window && open.generation == fields.generation)
    }

    fn menu_mut(&mut self, menu: MenuRef) -> Option<&mut OpenMenu> {
        let fields = menu.fields();
        self.world
            .menu
            .as_mut()
            .filter(|open| open.window == fields.window && open.generation == fields.generation)
    }

    /// The slot a menu index shows: an owned slot, or an inventory slot.
    fn backing(&self, open: &OpenMenu, slot: usize) -> Option<MenuSlotBacking> {
        let owned = open.owned.len();
        if slot < owned {
            return Some(MenuSlotBacking::Owned(slot));
        }
        let player = slot - owned;
        match player {
            0..27 => Some(MenuSlotBacking::Player(player + 9)),
            27..36 => Some(MenuSlotBacking::Player(player - 27)),
            _ => None,
        }
    }

    /// `Inventory.placeItemBackInInventory` for the cursor at a close: the first stack of the
    /// same item with room, then the first empty main slot.
    fn place_back(&mut self, stack: ItemStack) {
        let same = (0..36).find(|&slot| {
            self.world.inventory[slot].as_ref().is_some_and(|held| {
                held.is_stackable_with(&stack) && held.count() + stack.count() <= 64
            })
        });
        match same {
            Some(slot) => {
                let held = self.world.inventory[slot].as_ref().unwrap();
                let merged = ItemResource::of(held)
                    .to_stack(held.count() + stack.count())
                    .unwrap();
                self.world.inventory[slot] = Some(merged);
            }
            None => {
                let free = (0..36)
                    .find(|&slot| self.world.inventory[slot].is_none())
                    .expect("room");
                self.world.inventory[free] = Some(stack);
            }
        }
    }

    /// The window closes as `ServerPlayer.doCloseContainer` does on Sunrise: the cursor goes
    /// back into the inventory, the menu is gone, and `menu_closed` (with the generic owned
    /// menu's final slots) is queued.
    fn close_window(&mut self) -> bool {
        let Some(open) = self.world.menu.take() else {
            return false;
        };
        if let Some(cursor) = open.cursor.clone() {
            // `MenuRemoval::drops_items`: a killed or disconnected player's cursor drops.
            if self.present && self.health > 0.0 {
                self.place_back(cursor);
            } else {
                let at = [self.eye[0], self.eye[1] - 0.3, self.eye[2]];
                self.world.spawned.push((cursor, at));
            }
        }
        self.closes
            .borrow_mut()
            .push_back((open.window, open.owned.clone()));
        true
    }

    /// Deliver the queued `menu_closed` notices through the installed mods.
    fn deliver_closes(&mut self) {
        loop {
            let next = self.closes.borrow_mut().pop_front();
            let Some((window, owned)) = next else {
                return;
            };
            mod_api::dispatch_menu_closed(self, UUID, window, None, &owned);
        }
    }

    fn click(&mut self, slot: i32, button: i32, mode: i32) -> bool {
        let open = self.world.menu.as_ref().expect("a menu is open");
        let (menu, window) = (open.menu.clone(), open.window);
        let mut handled = false;
        mod_api::dispatch_menu_clicked(
            self,
            mod_api::calls::MenuClickedCall {
                menu: &menu,
                uuid: UUID,
                window,
                slot,
                button,
                mode,
                handled: &mut handled,
            },
        );
        handled
    }

    /// One server tick of the player: the level's entity tick (`entity_tick_pre`, the body of
    /// `ServerPlayer.tick`, in which `between` runs as `gameMode.tick` would, `entity_tick_post`),
    /// then the connection's `doTick` (`player_tick_post`), then queued closes.
    fn tick_with(&mut self, between: impl FnOnce(&mut Self)) {
        let mut listened = false;
        mod_api::dispatch_entity_tick_listened(HostSide::Server(self), &mut listened);
        if listened {
            let mut canceled = Vec::new();
            mod_api::dispatch_entity_tick_pre(
                HostSide::Server(self),
                "minecraft:overworld",
                &[ENTITY_ID],
                &["minecraft:player"],
                &mut canceled,
            );
        }
        between(self);
        if listened {
            mod_api::dispatch_entity_tick_post(
                HostSide::Server(self),
                "minecraft:overworld",
                &[ENTITY_ID],
                &["minecraft:player"],
            );
        }
        let player = self.player_ref();
        mod_api::dispatch_player_tick_post(
            HostSide::Server(self),
            mod_api::calls::PlayerTickPostCall { player },
        );
        self.deliver_closes();
    }

    fn tick(&mut self) {
        self.tick_with(|_| {});
    }

    /// The player disconnects: Sunrise takes them off the list, drops their cursor
    /// (`MenuRemoval::Disconnected`), closes the window and tells `menu_closed` after.
    fn disconnect(&mut self) {
        self.present = false;
        self.close_window();
        self.deliver_closes();
    }

    fn place_stack(&self, place: &ItemPlace) -> Option<Option<ItemStack>> {
        match place {
            ItemPlace::MenuSlot { menu, slot } => {
                let open = self.menu_is(*menu)?;
                match self.backing(open, *slot)? {
                    MenuSlotBacking::Owned(index) => Some(open.owned[index].clone()),
                    _ => None,
                }
            }
            ItemPlace::PlayerCursor { menu } => Some(self.menu_is(*menu)?.cursor.clone()),
            ItemPlace::PlayerSlot { slot, .. } => {
                Some(self.world.inventory.get(inventory_index(*slot)?)?.clone())
            }
            _ => None,
        }
    }

    fn set_place(&mut self, place: &ItemPlace, stack: Option<ItemStack>) -> bool {
        match place {
            ItemPlace::MenuSlot { menu, slot } => {
                let slot = *slot;
                let Some(open) = self.menu_mut(*menu) else {
                    return false;
                };
                if slot >= open.owned.len() {
                    return false;
                }
                open.owned[slot] = stack;
                true
            }
            ItemPlace::PlayerCursor { menu } => match self.menu_mut(*menu) {
                Some(open) => {
                    open.cursor = stack;
                    true
                }
                None => false,
            },
            ItemPlace::PlayerSlot { slot, .. } => {
                match inventory_index(*slot).and_then(|index| self.world.inventory.get_mut(index)) {
                    Some(held) => {
                        *held = stack;
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        }
    }
}

impl Host for MenuTestHost {
    fn backend(&self) -> Option<mod_api::BackendRef<'_>> {
        Some(mod_api::__private::backend_ref(self))
    }
}
impl mod_api::prelude::BlockStatesHost for MenuTestHost {
    fn block_state_string(&self, _state: BlockState) -> Option<String> {
        Some(self.block.clone())
    }
}
impl mod_api::prelude::ItemsHost for MenuTestHost {
    fn item_key(&self, id: &Identifier) -> Option<ItemKey> {
        mod_api::prelude::ItemsHost::item_key(&self.fake, id)
    }
}
impl mod_api::prelude::KeysHost for MenuTestHost {
    fn menu_keys(&self) -> Vec<MenuKey> {
        mod_api::__private::menu_keys()
    }
}
impl mod_api::prelude::PlayersHost for MenuTestHost {
    fn player(&self, uuid: u128) -> Option<PlayerRef> {
        (uuid == UUID && self.present).then(|| self.player_ref())
    }
}

/// The server's player list: the one player while they are on it.
struct TestPlayers(Option<PlayerRef>);

impl Players for TestPlayers {
    fn server_players(&self) -> Vec<PlayerRef> {
        self.0.into_iter().collect()
    }
    fn acting_player(&self) -> Option<PlayerRef> {
        None
    }
}
impl mod_api::prelude::TextHost for MenuTestHost {
    fn text_from_json(&self, json: &str) -> Result<Option<Text>, mod_api::TextFormError> {
        Ok(Some(Text::literal(json)))
    }
}
impl mod_api::prelude::LivingHost for MenuTestHost {
    fn living(&self, entity: EntityRef) -> Option<LivingRef> {
        (entity.id() == ENTITY_ID).then(|| LivingRef::new(entity))
    }
}
mod_api::impl_host_areas!(
    MenuTestHost,
    except(block_states, items, keys, living, players, text)
);

impl ServerHost for MenuTestHost {
    fn backend_mut(&mut self) -> Option<mod_api::BackendMutRef<'_>> {
        Some(mod_api::__private::parts_installation::backend_mut_ref(
            self,
        ))
    }
    fn with_current_transaction_scope(
        &mut self,
        callback: &mut dyn for<'loan> FnMut(&'loan mut dyn ServerHost, &'loan TransactionContext),
    ) -> Result<(), TransactionUnavailable> {
        FakeJournal::with_current_scope(self, |host| &host.journal, callback)
    }
    fn with_transaction_scope(
        &mut self,
        callback: &mut dyn for<'loan> FnMut(
            &'loan mut dyn ServerHost,
            &'loan TransactionContext,
        ) -> bool,
    ) -> Result<bool, TransactionScopeError> {
        FakeJournal::with_scope(
            self,
            |host| &mut host.journal,
            |host| host.world.clone(),
            |host, world| host.world = world,
            callback,
        )
    }
    fn transaction_matches(&self, tx: Option<&TransactionContext>) -> bool {
        self.journal.matches(tx)
    }
}
impl mod_api::prelude::ServicesServerHost for MenuTestHost {
    fn players(&self) -> &dyn Players {
        // A list built for each ask, leaked once per distinct answer: the test host is short-lived.
        if self.present {
            static PRESENT: std::sync::OnceLock<TestPlayers> = std::sync::OnceLock::new();
            PRESENT.get_or_init(|| {
                TestPlayers(Some(mod_api::__private::game::player_ref(
                    EntityRef::new(WorldEpoch::new(1), ENTITY_ID),
                    UUID,
                )))
            })
        } else {
            &TestPlayers(None)
        }
    }
}
mod_api::impl_server_host_areas!(MenuTestHost, except(services));

impl mod_api::prelude::KeysBackend for MenuTestHost {
    fn item_key_id(&self, key: ItemKey) -> Option<&str> {
        mod_api::prelude::KeysBackend::item_key_id(&self.fake, key)
    }
    fn menu_key_id(&self, key: MenuKey) -> Option<mod_api::RegistryName> {
        mod_api::__private::menu_key_id(key)
    }
}
impl mod_api::prelude::PlayerSessionBackend for MenuTestHost {
    fn player_name(&self, _player: PlayerRef) -> Option<&str> {
        Some("Alex")
    }
}
impl mod_api::prelude::EntityStanceBackend for MenuTestHost {
    fn entity_is_shift_key_down(&self, _entity: EntityRef) -> Option<bool> {
        Some(false)
    }
}
impl mod_api::prelude::EntitySightBackend for MenuTestHost {
    fn entity_eye_position(&self, _entity: EntityRef) -> Option<[f64; 3]> {
        Some(self.eye)
    }
}
impl mod_api::prelude::EntityPositionBackend for MenuTestHost {
    fn entity_position(&self, _entity: EntityRef) -> Option<[f64; 3]> {
        Some([self.eye[0], self.eye[1] - 1.62, self.eye[2]])
    }
}
impl mod_api::prelude::EntityLifetimeBackend for MenuTestHost {
    fn entity_tick_count(&self, _entity: EntityRef) -> Option<i32> {
        Some(self.tick_count)
    }
}
impl mod_api::prelude::EntityIdentityBackend for MenuTestHost {
    fn entity_level(&self, _entity: EntityRef) -> Option<LevelRef> {
        Some(self.fake.level.level())
    }
}
impl mod_api::prelude::LevelBlocksBackend for MenuTestHost {
    fn level_block_state(&self, _level: LevelRef, _pos: [i32; 3]) -> Option<BlockState> {
        Some(test_block_state())
    }
}
impl mod_api::prelude::LivingBackend for MenuTestHost {
    fn living_health(&self, _living: LivingRef) -> Option<f32> {
        // A player who left has no living entity to read.
        self.present.then_some(self.health)
    }
    fn living_attribute_value(&self, _living: LivingRef, id: &str) -> Option<f64> {
        (id == "minecraft:block_interaction_range").then_some(4.5)
    }
}
impl mod_api::prelude::LivingClientBackend for MenuTestHost {}
impl mod_api::prelude::PlayerModesBackend for MenuTestHost {
    fn player_has_infinite_materials(&self, _player: PlayerRef) -> Option<bool> {
        Some(false)
    }
    fn player_is_spectator(&self, _player: PlayerRef) -> Option<bool> {
        Some(false)
    }
}
impl mod_api::prelude::PlayerInventoryBackend for MenuTestHost {
    fn player_selected_slot(&self, _player: PlayerRef) -> Option<usize> {
        Some(0)
    }
    fn player_inventory_slots(
        &self,
        _player: PlayerRef,
        read: &mut dyn for<'a> FnMut(InventoryView<'a>),
    ) -> Option<()> {
        let inventory = &self.world.inventory;
        let metadata = |slot: u32| {
            let stack = inventory.get(slot as usize)?.as_ref()?;
            Some((
                stack.item(),
                NonZeroU32::new(stack.count())?,
                stack.damage_component(),
            ))
        };
        let project =
            |slot: u32, _: (ItemKey, NonZeroU32, Option<u32>)| inventory[slot as usize].clone();
        read(InventoryView::new(0..43, &metadata, &project));
        Some(())
    }
}
impl mod_api::prelude::PlayerMenusBackend for MenuTestHost {
    fn player_container_menu(&self, player: PlayerRef) -> Option<MenuRef> {
        Some(match &self.world.menu {
            Some(open) => mod_api::__private::game::menu_ref(player, open.window, open.generation),
            None => mod_api::__private::game::menu_ref(player, 0, 0),
        })
    }
    fn player_container_counter(&self, _player: PlayerRef) -> Option<i32> {
        Some(self.world.counter)
    }
}
impl mod_api::prelude::MenusBackend for MenuTestHost {
    fn menu_container_id(&self, menu: MenuRef) -> Option<i32> {
        let fields = menu.fields();
        if fields.window == 0 {
            return Some(0);
        }
        self.menu_is(menu).map(|open| open.window)
    }
    fn menu_kind(&self, menu: MenuRef) -> Option<Option<String>> {
        Some(self.menu_is(menu).map(|open| open.menu.clone()))
    }
    fn menu_slot_count(&self, menu: MenuRef) -> Option<usize> {
        self.menu_is(menu).map(|open| open.owned.len() + 36)
    }
    fn menu_slot(&self, menu: MenuRef, slot: usize) -> Option<Option<ItemStack>> {
        let open = self.menu_is(menu)?;
        match self.backing(open, slot)? {
            MenuSlotBacking::Owned(index) => Some(open.owned[index].clone()),
            MenuSlotBacking::Player(index) => Some(self.world.inventory[index].clone()),
            _ => None,
        }
    }
    fn menu_carried(&self, menu: MenuRef) -> Option<Option<ItemStack>> {
        Some(self.menu_is(menu)?.cursor.clone())
    }
    fn menu_slot_backing(&self, menu: MenuRef, slot: usize) -> Option<MenuSlotBacking> {
        self.backing(self.menu_is(menu)?, slot)
    }
    fn menu_slot_holds(&self, menu: MenuRef, slot: usize, _stack: &ItemStack) -> Option<u32> {
        self.backing(self.menu_is(menu)?, slot).map(|_| 99)
    }
    fn menu_slot_gives(&self, menu: MenuRef, slot: usize) -> Option<bool> {
        self.backing(self.menu_is(menu)?, slot).map(|_| true)
    }
}
impl mod_api::prelude::MenusClientBackend for MenuTestHost {}
mod_api::impl_backend_areas!(
    MenuTestHost,
    except(
        entity_identity,
        entity_lifetime,
        entity_position,
        entity_sight,
        entity_stance,
        keys,
        level_blocks,
        living,
        menus,
        player_inventory,
        player_menus,
        player_modes,
        player_session
    )
);
impl Backend for MenuTestHost {}

impl mod_api::prelude::ItemsBackendMut for MenuTestHost {
    fn game_stack(
        &mut self,
        place: &ItemPlace,
        read: &mut dyn for<'a> FnMut(ItemStackView<'a>),
    ) -> Option<()> {
        let stack = self.place_stack(place)??;
        read(ItemStackView::from_stack(&stack));
        Some(())
    }
    fn game_insert(
        &mut self,
        place: &ItemPlace,
        resource: &ItemResource,
        amount: u32,
        tx: &TransactionContext,
    ) -> Result<u32, StackWriteError> {
        if !self.journal.matches(Some(tx)) {
            return Ok(0);
        }
        let Some(held) = self.place_stack(place) else {
            return Ok(0);
        };
        let stack = match held {
            None => resource.to_stack(amount),
            Some(held) if resource.matches(&held) => resource.to_stack(held.count() + amount),
            Some(_) => return Ok(0),
        };
        Ok(if self.set_place(place, stack) {
            amount
        } else {
            0
        })
    }
    fn game_extract(
        &mut self,
        place: &ItemPlace,
        resource: &ItemResource,
        amount: u32,
        tx: &TransactionContext,
    ) -> Result<u32, StackWriteError> {
        if !self.journal.matches(Some(tx)) {
            return Ok(0);
        }
        let Some(Some(held)) = self.place_stack(place) else {
            return Ok(0);
        };
        if !resource.matches(&held) {
            return Ok(0);
        }
        let taken = amount.min(held.count());
        let left = held.count() - taken;
        let stack = (left > 0).then(|| resource.to_stack(left)).flatten();
        Ok(if self.set_place(place, stack) {
            taken
        } else {
            0
        })
    }
}
/// `ItemPlace::PlayerSlot`'s number as an `Inventory` index: main 0..36, armour 100..=103 as
/// 36..40, the off hand -106 as 40.
fn inventory_index(slot: i32) -> Option<usize> {
    match slot {
        0..36 => Some(slot as usize),
        100..=103 => Some(slot as usize - 64),
        -106 => Some(40),
        _ => None,
    }
}

impl mod_api::prelude::PlayerInventoryBackendMut for MenuTestHost {
    fn player_drop_stack(
        &mut self,
        _player: PlayerRef,
        stack: ItemStack,
        _thrown_by_player: bool,
    ) -> Result<Option<ItemStack>, StackWriteError> {
        // A dead player throws nothing, and a refused toss isn't thrown: the stack comes back.
        match self.toss_refusal {
            Some(TossRefusal::Cancel) => return Ok(Some(stack)),
            Some(TossRefusal::Die) => self.health = 0.0,
            Some(TossRefusal::Vanish) => self.present = false,
            None => {}
        }
        if self.health <= 0.0 || !self.present {
            return Ok(Some(stack));
        }
        self.world.drops.push(stack);
        Ok(None)
    }
    fn player_set_inventory_slot(
        &mut self,
        _player: PlayerRef,
        slot: u32,
        stack: Option<ItemStack>,
    ) -> Result<(), PlayerDataRefusal> {
        let Some(held) = self.world.inventory.get_mut(slot as usize) else {
            return Err(PlayerDataRefusal::Stale);
        };
        *held = stack;
        Ok(())
    }
}
impl mod_api::prelude::PlayerMenusBackendMut for MenuTestHost {
    fn player_open_mod_menu(
        &mut self,
        _player: PlayerRef,
        request: ModMenuOpen,
    ) -> Result<Option<i32>, OpenMenuError> {
        let id = request
            .menu
            .id(&*self)
            .map(|id| id.to_string())
            .ok_or(OpenMenuError::NotModMenu)?;
        let (slots, data) = self
            .menu_types
            .iter()
            .find(|(menu, _, _)| *menu == id)
            .map(|(_, slots, data)| (*slots, *data))
            .ok_or(OpenMenuError::NotModMenu)?;
        if request.data.len() != data {
            return Err(OpenMenuError::DataCountMismatch);
        }
        // The old window closes as a close does, before the new one is built.
        self.close_window();
        self.world.counter = self.world.counter % 100 + 1;
        self.world.generation += 1;
        self.world.menu = Some(OpenMenu {
            window: self.world.counter,
            generation: self.world.generation,
            menu: id.clone(),
            owned: vec![None; slots],
            cursor: None,
            data: request.data.clone(),
        });
        self.opens.push((id, request.pos));
        Ok(Some(self.world.counter))
    }
    fn player_next_container_counter(&mut self, _player: PlayerRef) -> Option<i32> {
        self.world.counter = self.world.counter % 100 + 1;
        Some(self.world.counter)
    }
}
impl mod_api::prelude::MenusBackendMut for MenuTestHost {
    fn menu_close_container(&mut self, menu: MenuRef) -> bool {
        self.menu_is(menu).is_some() && self.close_window()
    }
    fn menu_set_data_slot(
        &mut self,
        _menu: MenuRef,
        _index: usize,
        _value: i32,
    ) -> Result<bool, MenuDataRefusal> {
        Err(MenuDataRefusal::InvalidIndex)
    }
}
impl mod_api::prelude::PlayerProgressBackendMut for MenuTestHost {
    fn player_award_stat(
        &mut self,
        _player: PlayerRef,
        stat_type: StatType,
        key: &str,
        amount: i32,
    ) -> bool {
        self.world.stats.push((stat_type, key.to_owned(), amount));
        true
    }
}
impl mod_api::prelude::PlayerRecipesBackendMut for MenuTestHost {
    fn player_award_recipes(&mut self, _player: PlayerRef, recipes: &[&str]) -> Option<i32> {
        self.world
            .recipes
            .extend(recipes.iter().map(|recipe| (*recipe).to_owned()));
        Some(recipes.len() as i32)
    }
}
impl mod_api::prelude::LevelEntitiesBackendMut for MenuTestHost {
    fn level_spawn_item(
        &mut self,
        _level: LevelRef,
        x: f64,
        y: f64,
        z: f64,
        stack: ItemStack,
        _pickup_delay: u32,
    ) -> Result<Option<EntityRef>, StackWriteError> {
        self.world.spawned.push((stack, [x, y, z]));
        Ok(Some(EntityRef::new(WorldEpoch::new(1), 1000)))
    }
}
mod_api::impl_backend_mut_areas!(
    MenuTestHost,
    except(
        items,
        level_entities,
        menus,
        player_inventory,
        player_menus,
        player_progress,
        player_recipes
    )
);
impl BackendMut for MenuTestHost {}

fn test_block_state() -> BlockState {
    let key: BlockKey = mod_api::__private::parts_installation::block_key(0);
    mod_api::__private::block_state_from_ordinal(key, 0)
}

fn right_click<'a>() -> BlockClick<'a> {
    mod_api::__private::built::BlockClick {
        side: LogicalSide::Server,
        dimension: LevelDimension::Overworld,
        pos: POS,
        hit: BlockHit::new([12.5, 66.0, -6.5], [0, 1, 0], false),
        hand: Hand::Main,
        stack: None,
        player: UUID,
        player_entity_id: ENTITY_ID as i32,
    }
    .into()
}

// ---------------------------------------------------------------------------
// The mod as Sunrise installs it
// ---------------------------------------------------------------------------

/// The bridge's hooks as the game installs them: `HorizonJava`'s `menu_closed` and
/// `player_tick_post`, and the `JavaMenuPart` its registration attaches to the menu type.
struct BridgeMod {
    java: Arc<HorizonJava>,
    menus: &'static [&'static str],
}

#[mod_api::__subscriptions]
impl Hooks for BridgeMod {
    fn register(&self, registrar: &mut dyn Registrar, _context: &RegistrationContext) {
        registrar.entity_tick_each("minecraft:player");
        for menu in self.menus {
            registrar.menu_part(menu, JavaMenuPart::new(Arc::clone(&self.java.state)));
        }
    }
    fn entity_tick_listened(&self, host: HostSide<'_>, event: &mut EntityTickListenedEvent) {
        Hooks::entity_tick_listened(&*self.java, host, event);
    }
    fn entity_tick_pre(&self, host: HostSide<'_>, event: &mut EntityTickPreEvent<'_>) {
        Hooks::entity_tick_pre(&*self.java, host, event);
    }
    fn entity_tick_post(&self, host: HostSide<'_>, event: &mut EntityTickPostEvent<'_>) {
        Hooks::entity_tick_post(&*self.java, host, event);
    }
    fn menu_closed(&self, host: &mut dyn ServerHost, event: &mut MenuClosedEvent) {
        Hooks::menu_closed(&*self.java, host, event);
    }
    fn player_tick_post(&self, host: HostSide<'_>, event: &mut PlayerTickPostEvent) {
        Hooks::player_tick_post(&*self.java, host, event);
    }
}

/// A jar run the way this mod runs one, held by a fresh `HorizonJava` whose player logged in.
fn java_with(jar: Arc<NativeJar>) -> Arc<HorizonJava> {
    let mut host = jar
        .try_seed_host(StdHost::new().with_game_version("26.3"))
        .expect("seed host");
    for entry in jar.discover_entrypoints("main") {
        jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
            .expect("run main");
    }
    // The player joined: their `ServerPlayer` is session 1, life 0.
    host.server_player_created(UUID, 1, 0);
    let java = Arc::new(HorizonJava::default());
    {
        let mut state: std::sync::MutexGuard<'_, State> = java.state.lock().unwrap();
        state.loaded = true;
        state.lives.insert(UUID, (1, 0));
        state.jars.push(LoadedJar {
            row: crate::JarRow::refused("fixture.jar", "fixture".to_owned(), String::new()),
            jar,
            host,
            listeners: Vec::new(),
            orderings: Vec::new(),
            commands: Vec::new(),
            native_commands: Vec::new(),
            commands_registered: false,
            stages_ran: 1,
            failures: Vec::new(),
            hooks: crate::runtime::GameHookRegistrations::default(),
            hook_state: crate::runtime::GameHookState::default(),
        });
    }
    java
}

fn with_bridge(java: &Arc<HorizonJava>, menus: &'static [&'static str], run: impl FnOnce()) {
    let mods: mod_api::Mods = vec![ModEntry::new(
        "horizon_java",
        Box::new(BridgeMod {
            java: Arc::clone(java),
            menus,
        }),
    )]
    .into();
    mod_api::testing::with_mods(mods.clone(), || {
        mod_api::__private::part_tests::install_parts_fixture(&mods).expect("install the parts");
        run();
    });
}

/// The jar's counted calls and hooks so far, by name.
fn counted(java: &HorizonJava) -> Vec<(String, usize)> {
    let state: std::sync::MutexGuard<'_, State> = java.state.lock().unwrap();
    state.jars[0].host.unknown_host_calls()
}

fn on_a_big_stack(name: &str, run: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name(name.into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run)
        .expect("spawn test thread")
        .join()
        .unwrap();
}

// ---------------------------------------------------------------------------
// The javac fixture
// ---------------------------------------------------------------------------

/// `demo.Bench` opens `demo.BenchMenu` (cobblestone + amethyst shard make 4 stone bricks;
/// taking them shrinks each input; the second input refuses dirt), as in the runtime's
/// `fixtures/live_menu`, whose oracle pins these sources' click semantics on Java 25.
/// `BenchMenu.onTake` also prints the player's `tickCount`, the fact Sunrise supplies.
/// `demo.NullBench`'s provider builds no menu (`createMenu` answers `null`).
pub(crate) const DEMO_SOURCES: &[(&str, &str)] = &[
    (
        "demo/Probe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public final class Probe implements ModInitializer {\n\
           public static Block BENCH;\n\
           public static Block CHAIN_BENCH;\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"bench\"), BenchMenu.TYPE);\n\
             BENCH = new Bench(BlockBehaviour.Properties.of());\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"bench\"), BENCH);\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"null_bench\"), new NullBench(BlockBehaviour.Properties.of()));\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"data\"), DataMenu.TYPE);\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"data_bench\"), new DataBench(BlockBehaviour.Properties.of()));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"odd_bench\"), new OddBench(BlockBehaviour.Properties.of()));\n\
             CHAIN_BENCH = new ChainBench(BlockBehaviour.Properties.of());\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"chain_bench\"), CHAIN_BENCH);\n\
           }\n\
         }",
    ),
    (
        "demo/Bench.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class Bench extends Block {\n\
           public Bench(Properties properties) { super(properties); }\n\
           @Override protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (level.isClientSide()) return InteractionResult.SUCCESS;\n\
             player.openMenu(new SimpleMenuProvider((id, inventory, p) -> new BenchMenu(id, inventory, ContainerLevelAccess.create(level, pos)), Component.translatable(\"container.demo.bench\")));\n\
             return InteractionResult.CONSUME;\n\
           }\n\
         }",
    ),
    (
        "demo/ChainBench.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class ChainBench extends Block {\n\
           public ChainBench(Properties properties) { super(properties); }\n\
           @Override protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (level.isClientSide()) return InteractionResult.SUCCESS;\n\
             player.openMenu(new SimpleMenuProvider((id, inventory, p) -> new ChainMenu(id, inventory, ContainerLevelAccess.create(level, pos)), Component.translatable(\"container.demo.bench\")));\n\
             return InteractionResult.CONSUME;\n\
           }\n\
         }",
    ),
    (
        "demo/ChainMenu.java",
        "package demo;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         /** The bench whose shift-click on the base opens a fresh bench and moves the base into it:\n\
          *  the new window opens holding what the click's Java put there. */\n\
         public class ChainMenu extends BenchMenu {\n\
           public ChainMenu(int id, Inventory inventory, ContainerLevelAccess access) { super(id, inventory, access); }\n\
           @Override protected boolean isValidBlock(BlockState state) { return state.is(Probe.CHAIN_BENCH); }\n\
           @Override public ItemStack quickMoveStack(Player player, int index) {\n\
             if (index != 0) return ItemStack.EMPTY;\n\
             ItemStack base = inputSlots.getItem(0);\n\
             inputSlots.setItem(0, ItemStack.EMPTY);\n\
             ContainerLevelAccess where = access;\n\
             ChainMenu[] next = new ChainMenu[1];\n\
             player.openMenu(new SimpleMenuProvider((id, inventory, p) -> next[0] = new ChainMenu(id, inventory, where), Component.translatable(\"container.demo.bench\")));\n\
             next[0].inputSlots.setItem(0, base);\n\
             return ItemStack.EMPTY;\n\
           }\n\
         }",
    ),
    (
        "demo/NullBench.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class NullBench extends Block {\n\
           public NullBench(Properties properties) { super(properties); }\n\
           @Override protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (level.isClientSide()) return InteractionResult.SUCCESS;\n\
             player.openMenu(new SimpleMenuProvider((id, inventory, p) -> null, Component.translatable(\"container.demo.none\")));\n\
             return InteractionResult.CONSUME;\n\
           }\n\
         }",
    ),
    (
        "demo/BenchMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.inventory.ItemCombinerMenu;\n\
         import net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.inventory.Slot;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.item.Items;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         public class BenchMenu extends ItemCombinerMenu {\n\
           public static final MenuType<BenchMenu> TYPE = Types.bench();\n\
           public BenchMenu(int id, Inventory inventory) { this(id, inventory, ContainerLevelAccess.NULL); }\n\
           public BenchMenu(int id, Inventory inventory, ContainerLevelAccess access) { super(TYPE, id, inventory, access, slots()); }\n\
           private static ItemCombinerMenuSlotDefinition slots() {\n\
             return ItemCombinerMenuSlotDefinition.create()\n\
                 .withSlot(0, 27, 47, stack -> true)\n\
                 .withSlot(1, 76, 47, stack -> !stack.is(Items.DIRT))\n\
                 .withResultSlot(2, 134, 47)\n\
                 .build();\n\
           }\n\
           @Override protected boolean isValidBlock(BlockState state) { return state.is(Probe.BENCH); }\n\
           @Override public void createResult() {\n\
             ItemStack base = inputSlots.getItem(0);\n\
             ItemStack addition = inputSlots.getItem(1);\n\
             if (base.is(Items.COBBLESTONE) && addition.is(Items.AMETHYST_SHARD)) {\n\
               resultSlots.setItem(0, new ItemStack(Items.STONE_BRICKS, 4));\n\
             } else {\n\
               resultSlots.setItem(0, ItemStack.EMPTY);\n\
             }\n\
           }\n\
           @Override protected void onTake(Player player, ItemStack stack) {\n\
             System.out.println(\"mod: onTake at tick \" + player.tickCount);\n\
             stack.onCraftedBy(player, stack.getCount());\n\
             shrink(0);\n\
             shrink(1);\n\
           }\n\
           private void shrink(int index) {\n\
             ItemStack stack = inputSlots.getItem(index);\n\
             if (!stack.isEmpty()) { stack.shrink(1); inputSlots.setItem(index, stack); }\n\
           }\n\
           @Override public boolean canTakeItemForPickAll(ItemStack stack, Slot slot) {\n\
             return slot.container != resultSlots && super.canTakeItemForPickAll(stack, slot);\n\
           }\n\
         }",
    ),
    (
        "demo/DataBench.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class DataBench extends Block {\n\
           public DataBench(Properties properties) { super(properties); }\n\
           @Override protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (level.isClientSide()) return InteractionResult.SUCCESS;\n\
             player.openMenu(new SimpleMenuProvider((id, inventory, p) -> new DataMenu(id, inventory, ContainerLevelAccess.create(level, pos)), Component.translatable(\"container.demo.data\")));\n\
             return InteractionResult.CONSUME;\n\
           }\n\
         }",
    ),
    (
        "demo/DataMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.inventory.DataSlot;\n\
         import net.minecraft.world.inventory.ItemCombinerMenu;\n\
         import net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         public class DataMenu extends ItemCombinerMenu {\n\
           public static final MenuType<DataMenu> TYPE = new MenuType<>(DataMenu::new, FeatureFlags.VANILLA_SET);\n\
           public DataMenu(int id, Inventory inventory) { this(id, inventory, ContainerLevelAccess.NULL); }\n\
           public DataMenu(int id, Inventory inventory, ContainerLevelAccess access) {\n\
             super(TYPE, id, inventory, access, slots());\n\
             addDataSlot(DataSlot.standalone()).set(7);\n\
           }\n\
           private static ItemCombinerMenuSlotDefinition slots() {\n\
             return ItemCombinerMenuSlotDefinition.create().withSlot(0, 27, 47, stack -> true).withSlot(1, 76, 47, stack -> true).withResultSlot(2, 134, 47).build();\n\
           }\n\
           @Override protected boolean isValidBlock(BlockState state) { return true; }\n\
           @Override public void createResult() {}\n\
           @Override protected void onTake(Player player, ItemStack stack) {}\n\
         }",
    ),
    (
        "demo/OddMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.inventory.ItemCombinerMenu;\n\
         import net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         /** A menu of the bench's type with one input: not the layout the type declares. */\n\
         public class OddMenu extends ItemCombinerMenu {\n\
           public OddMenu(int id, Inventory inventory, ContainerLevelAccess access) {\n\
             super(BenchMenu.TYPE, id, inventory, access, ItemCombinerMenuSlotDefinition.create().withSlot(0, 27, 47, stack -> true).withResultSlot(1, 134, 47).build());\n\
           }\n\
           @Override protected boolean isValidBlock(BlockState state) { return true; }\n\
           @Override public void createResult() {}\n\
           @Override protected void onTake(Player player, ItemStack stack) {}\n\
         }",
    ),
    (
        "demo/OddBench.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class OddBench extends Block {\n\
           public OddBench(Properties properties) { super(properties); }\n\
           @Override protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (level.isClientSide()) return InteractionResult.SUCCESS;\n\
             player.openMenu(new SimpleMenuProvider((id, inventory, p) -> new OddMenu(id, inventory, ContainerLevelAccess.create(level, pos)), Component.translatable(\"container.demo.odd\")));\n\
             return InteractionResult.CONSUME;\n\
           }\n\
         }",
    ),
    (
        "demo/Types.java",
        "package demo;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         final class Types {\n\
           static MenuType<BenchMenu> bench() { return new MenuType<>(BenchMenu::new, FeatureFlags.VANILLA_SET); }\n\
         }",
    ),
];

/// Build the fixture jar with `javac --release 21`, packing only `demo/` classes.
fn bench_jar() -> Vec<u8> {
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "horizon-java-mod-live-menu-{}-{unique}",
        std::process::id()
    ));
    let _cleanup = Cleanup(root.clone());
    let classes = root.join("classes");
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| PathBuf::from("javac"));
    let mut command = Command::new(javac);
    command
        .args(["--release", "21", "-nowarn", "-d"])
        .arg(&classes);
    for (path, source) in STUBS.iter().chain(DEMO_SOURCES) {
        let file = root.join("src").join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        command.arg(file);
    }
    let output = command.output().expect("run javac");
    assert!(
        output.status.success(),
        "javac failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        let options = SimpleFileOptions::default();
        zip.start_file("fabric.mod.json", options).unwrap();
        zip.write_all(
            br#"{"schemaVersion":1,"id":"demo","version":"1","entrypoints":{"main":["demo.Probe"]}}"#,
        )
        .unwrap();
        let mut files: Vec<PathBuf> = fs::read_dir(classes.join("demo"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        files.sort();
        for file in files {
            let name = format!("demo/{}", file.file_name().unwrap().to_string_lossy());
            zip.start_file(name, options).unwrap();
            zip.write_all(&fs::read(&file).unwrap()).unwrap();
        }
        zip.finish().unwrap();
    }
    bytes.into_inner()
}

/// The game classes the fixture compiles against: the runtime's `fixtures/live_menu` stubs,
/// with `Entity.tickCount` on `Player`.
pub(crate) const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api; public interface ModInitializer { void onInitialize(); }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources;\n\
         public final class Identifier {\n\
           public static Identifier fromNamespaceAndPath(String ns, String p) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core;\n\
         import net.minecraft.resources.Identifier;\n\
         public interface Registry<T> {\n\
           static <V, T extends V> T register(Registry<V> reg, Identifier id, T val) { return val; }\n\
         }",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.level.block.Block;\n\
         public final class BuiltInRegistries {\n\
           public static final Registry<Block> BLOCK = null;\n\
           public static final Registry<MenuType<?>> MENU = null;\n\
         }",
    ),
    (
        "net/minecraft/core/BlockPos.java",
        "package net.minecraft.core; public class BlockPos {}",
    ),
    (
        "net/minecraft/world/phys/BlockHitResult.java",
        "package net.minecraft.world.phys; public class BlockHitResult {}",
    ),
    (
        "net/minecraft/world/InteractionResult.java",
        "package net.minecraft.world;\n\
         public interface InteractionResult {\n\
           InteractionResult SUCCESS = null;\n\
           InteractionResult CONSUME = null;\n\
           InteractionResult PASS = null;\n\
         }",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat;\n\
         public interface Component { static MutableComponent translatable(String key) { return null; } }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat; public class MutableComponent implements Component {}",
    ),
    (
        "net/minecraft/world/flag/FeatureFlagSet.java",
        "package net.minecraft.world.flag; public class FeatureFlagSet {}",
    ),
    (
        "net/minecraft/world/flag/FeatureFlags.java",
        "package net.minecraft.world.flag;\n\
         public final class FeatureFlags { public static final FeatureFlagSet VANILLA_SET = null; }",
    ),
    (
        "net/minecraft/world/level/ItemLike.java",
        "package net.minecraft.world.level; public interface ItemLike {}",
    ),
    (
        "net/minecraft/world/item/Item.java",
        "package net.minecraft.world.item; public class Item implements net.minecraft.world.level.ItemLike {}",
    ),
    (
        "net/minecraft/world/item/Items.java",
        "package net.minecraft.world.item;\n\
         public final class Items {\n\
           public static final Item COBBLESTONE = null;\n\
           public static final Item AMETHYST_SHARD = null;\n\
           public static final Item STONE_BRICKS = null;\n\
           public static final Item DIRT = null;\n\
         }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.ItemLike;\n\
         public final class ItemStack {\n\
           public static final ItemStack EMPTY = null;\n\
           public ItemStack(ItemLike item, int count) {}\n\
           public boolean isEmpty() { return true; }\n\
           public int getCount() { return 0; }\n\
           public void shrink(int amount) {}\n\
           public boolean is(Item item) { return false; }\n\
           public void onCraftedBy(Player player, int amount) {}\n\
         }",
    ),
    (
        "net/minecraft/world/Container.java",
        "package net.minecraft.world;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public interface Container {\n\
           ItemStack getItem(int slot);\n\
           void setItem(int slot, ItemStack stack);\n\
         }",
    ),
    (
        "net/minecraft/world/entity/player/Player.java",
        "package net.minecraft.world.entity.player;\n\
         import java.util.OptionalInt;\n\
         import net.minecraft.world.MenuProvider;\n\
         public abstract class Player {\n\
           public int tickCount;\n\
           public final double getX() { return 0; }\n\
           public final double getY() { return 0; }\n\
           public final double getZ() { return 0; }\n\
           public OptionalInt openMenu(MenuProvider provider) { return OptionalInt.empty(); }\n\
         }",
    ),
    (
        "net/minecraft/world/entity/player/Inventory.java",
        "package net.minecraft.world.entity.player; public class Inventory {}",
    ),
    (
        "net/minecraft/world/level/Level.java",
        "package net.minecraft.world.level;\n\
         public abstract class Level { public boolean isClientSide() { return false; } }",
    ),
    (
        "net/minecraft/world/inventory/Slot.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         public class Slot { public final Container container = null; }",
    ),
    (
        "net/minecraft/world/inventory/ResultContainer.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class ResultContainer implements Container {\n\
           public ItemStack getItem(int slot) { return null; }\n\
           public void setItem(int slot, ItemStack stack) {}\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/AbstractContainerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public abstract class AbstractContainerMenu {\n\
           protected AbstractContainerMenu(MenuType<?> type, int id) {}\n\
           public void removed(Player player) {}\n\
           public ItemStack getCarried() { return null; }\n\
           protected DataSlot addDataSlot(DataSlot slot) { return slot; }\n\
           public boolean canTakeItemForPickAll(ItemStack stack, Slot slot) { return true; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/DataSlot.java",
        "package net.minecraft.world.inventory;\n\
         public abstract class DataSlot {\n\
           public static DataSlot standalone() { return null; }\n\
           public abstract int get();\n\
           public abstract void set(int value);\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ContainerLevelAccess.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.level.Level;\n\
         public interface ContainerLevelAccess {\n\
           ContainerLevelAccess NULL = null;\n\
           static ContainerLevelAccess create(Level level, BlockPos pos) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition.java",
        "package net.minecraft.world.inventory;\n\
         import java.util.function.Predicate;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class ItemCombinerMenuSlotDefinition {\n\
           public static Builder create() { return null; }\n\
           public static class Builder {\n\
             public Builder withSlot(int i, int x, int y, Predicate<ItemStack> p) { return this; }\n\
             public Builder withResultSlot(int i, int x, int y) { return this; }\n\
             public ItemCombinerMenuSlotDefinition build() { return null; }\n\
           }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ItemCombinerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         public abstract class ItemCombinerMenu extends AbstractContainerMenu {\n\
           protected final Container inputSlots = null;\n\
           protected final ResultContainer resultSlots = null;\n\
           protected final ContainerLevelAccess access = null;\n\
           public ItemCombinerMenu(MenuType<?> type, int id, Inventory inv, ContainerLevelAccess access, ItemCombinerMenuSlotDefinition def) {\n\
             super(type, id);\n\
           }\n\
           protected abstract void onTake(Player player, ItemStack stack);\n\
           public ItemStack quickMoveStack(Player player, int index) { return null; }\n\
           protected abstract boolean isValidBlock(BlockState state);\n\
           public abstract void createResult();\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/MenuType.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.flag.FeatureFlagSet;\n\
         public class MenuType<T extends AbstractContainerMenu> {\n\
           public interface MenuSupplier<T extends AbstractContainerMenu> { T create(int id, Inventory inv); }\n\
           public MenuType(MenuSupplier<T> supplier, FeatureFlagSet features) {}\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/MenuConstructor.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         public interface MenuConstructor {\n\
           AbstractContainerMenu createMenu(int id, Inventory inv, Player player);\n\
         }",
    ),
    (
        "net/minecraft/world/MenuProvider.java",
        "package net.minecraft.world;\n\
         import net.minecraft.world.inventory.MenuConstructor;\n\
         public interface MenuProvider extends MenuConstructor {}",
    ),
    (
        "net/minecraft/world/SimpleMenuProvider.java",
        "package net.minecraft.world;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuConstructor;\n\
         public final class SimpleMenuProvider implements MenuProvider {\n\
           public SimpleMenuProvider(MenuConstructor ctor, Component title) {}\n\
           public AbstractContainerMenu createMenu(int id, Inventory inv, Player p) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/state/BlockBehaviour.java",
        "package net.minecraft.world.level.block.state;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public abstract class BlockBehaviour {\n\
           public static class Properties { public static Properties of() { return new Properties(); } }\n\
           protected BlockBehaviour(Properties props) {}\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             return InteractionResult.PASS;\n\
           }\n\
           public static abstract class BlockStateBase {}\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/state/BlockState.java",
        "package net.minecraft.world.level.block.state;\n\
         public class BlockState extends BlockBehaviour.BlockStateBase {\n\
           public boolean is(Object block) { return false; }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/Block.java",
        "package net.minecraft.world.level.block;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Block extends BlockBehaviour { public Block(Properties props) { super(props); } }",
    ),
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

fn held_window(java: &HorizonJava) -> Option<i32> {
    let state: std::sync::MutexGuard<'_, State> = java.state.lock().unwrap();
    state.jars[0]
        .host
        .player_open_menu(UUID)
        .map(|open| open.container_id)
}

/// The bench fixture's host and bridge: the jar run, its player logged in, its menus installed.
const BENCH_MENUS: &[&str] = &["demo:bench", "demo:data"];
const BENCH_ITEMS: &[&str] = &[
    "minecraft:cobblestone",
    "minecraft:amethyst_shard",
    "minecraft:stone_bricks",
    "minecraft:dirt",
];
const BENCH_TYPES: &[(&str, usize, usize)] = &[("demo:bench", 3, 0), ("demo:data", 3, 1)];

fn bench_part(java: &HorizonJava, block: &str) -> JavaBlockUsePart {
    JavaBlockUsePart::new(Arc::clone(&java.state), block.to_owned())
}

/// Open the bench with 5 cobblestone and 3 amethyst shards in the hotbar: the open window.
fn open_bench(java: &HorizonJava, host: &mut MenuTestHost) -> i32 {
    host.world.inventory[0] = Some(host.stack("minecraft:cobblestone", 5));
    host.world.inventory[1] = Some(host.stack("minecraft:amethyst_shard", 3));
    host.block = "demo:bench".to_owned();
    assert_eq!(
        bench_part(java, "demo:bench").dispatch(host, test_block_state(), &right_click(), false),
        Some(InteractionResult::Consume)
    );
    host.world
        .menu
        .as_ref()
        .expect("Sunrise opened the menu")
        .window
}

const SLOT_TAKE: &str = "game-hook re-entry: sunrise#live_menu:menu_slot_take_not_dispatched";

/// The two-input combiner from end to end through the hooks: a provider whose `createMenu`
/// answers `null` advances both counters; the bench opens as the Sunrise window the player's
/// `containerCounter` names; inputs placed by click and shift-click compute the result; taking
/// it consumes one of each input and awards the crafted stat (the take is the one counted
/// thing: Sunrise's `menu_slot_take` can't be raised by a mod); a tick keeps the menu; the close
/// hands the inputs back. A vanilla menu opened in between (no hook a mod hears moves the
/// counter, as a crafting table's) doesn't make the next window drift. Everything else is exact:
/// the take's `player.tickCount` read included (Sunrise's `tick_count` answers it).
#[test]
fn javac_bench_menu_opens_clicks_takes_and_closes_through_the_sunrise_hooks() {
    on_a_big_stack("live-menu-bench", || {
        let jar = Arc::new(NativeJar::from_bytes(&bench_jar()).expect("load the fixture jar"));
        let java = java_with(jar);
        with_bridge(&java, BENCH_MENUS, || {
            let mut host = MenuTestHost::new(BENCH_ITEMS, BENCH_TYPES);
            // The player opened 41 windows before: only Sunrise knows that.
            host.world.counter = 41;

            // 1. `createMenu` answers `null`: `ServerPlayer.openMenu` advanced the counter all
            //    the same, and Sunrise's follows (`next_container_counter`).
            host.block = "demo:null_bench".to_owned();
            assert_eq!(
                bench_part(&java, "demo:null_bench").dispatch(
                    &mut host,
                    test_block_state(),
                    &right_click(),
                    false
                ),
                Some(InteractionResult::Consume)
            );
            assert_eq!(host.world.counter, 42);
            assert!(host.world.menu.is_none());

            // 2. The bench opens window 43 on both sides; `initMenu` ran on Sunrise's facts.
            let window = open_bench(&java, &mut host);
            let open = host.world.menu.clone().unwrap();
            assert_eq!((window, open.menu.as_str()), (43, "demo:bench"));
            assert_eq!(host.opens, [("demo:bench".to_owned(), Some(POS))]);
            assert_eq!(held_window(&java), Some(43));
            assert_eq!(counted(&java), []);

            // 3. Pick up the cobblestone, right-click one into the base, put the rest back.
            assert!(host.click(hotbar(3, 0), 0, 0));
            assert_eq!(host.cursor(), "5 minecraft:cobblestone");
            assert_eq!(host.world.inventory[0], None);
            assert!(host.click(0, 1, 0));
            assert_eq!(host.owned(), ["1 minecraft:cobblestone", "empty", "empty"]);
            assert_eq!(host.cursor(), "4 minecraft:cobblestone");
            assert!(host.click(hotbar(3, 0), 0, 0));
            assert_eq!(host.cursor(), "empty");
            assert_eq!(
                host.text(&host.world.inventory[0]),
                "4 minecraft:cobblestone"
            );

            // 4. Shift-click the shards into the addition: `createResult` fills the result.
            assert!(host.click(hotbar(3, 1), 0, 1));
            assert_eq!(
                host.owned(),
                [
                    "1 minecraft:cobblestone",
                    "3 minecraft:amethyst_shard",
                    "4 minecraft:stone_bricks"
                ]
            );
            assert_eq!(host.world.inventory[1], None);
            assert_eq!(counted(&java), []);

            // 5. Take the result: `onTake` shrinks both inputs; `onCraftedBy` awards the stat.
            assert!(host.click(2, 0, 0));
            assert_eq!(host.cursor(), "4 minecraft:stone_bricks");
            assert_eq!(
                host.owned(),
                ["empty", "2 minecraft:amethyst_shard", "empty"]
            );
            assert_eq!(
                host.world.stats,
                [(StatType::Crafted, "minecraft:stone_bricks".to_owned(), 4)]
            );
            assert_eq!(counted(&java), [(SLOT_TAKE.to_owned(), 1)]);

            // 6. A tick: the bench is still there and in reach, so the menu stays.
            host.tick();
            assert_eq!(host.world.menu.as_ref().map(|open| open.window), Some(43));
            assert_eq!(held_window(&java), Some(43));

            // 7. The player closes it: Sunrise puts the cursor back, then the Java close
            //    (`ItemCombinerMenu.removed` → `clearContainer`) hands the shards back.
            assert!(host.close_window());
            host.deliver_closes();
            assert_eq!(
                host.held(),
                [
                    "2 minecraft:amethyst_shard",
                    "4 minecraft:cobblestone",
                    "4 minecraft:stone_bricks"
                ]
            );
            assert_eq!(held_window(&java), None);
            assert_eq!(counted(&java), [(SLOT_TAKE.to_owned(), 1)]);

            // 8. Two vanilla windows open and close (a crafting table: no hook a mod hears
            //    carries their ids), then the bench again: its window is the counter's next.
            host.world.counter += 2;
            assert_eq!(
                bench_part(&java, "demo:bench").dispatch(
                    &mut host,
                    test_block_state(),
                    &right_click(),
                    false
                ),
                Some(InteractionResult::Consume)
            );
            assert_eq!(host.world.menu.as_ref().map(|open| open.window), Some(46));
            assert_eq!(held_window(&java), Some(46));

            // 9. Put a cobblestone in, walk out of reach with three on the cursor:
            //    `ServerPlayer.tick`'s `stillValid` fails, the Java close returns both, and the
            //    window closes with an empty cursor, so nothing comes back twice.
            let cobble = (0..36)
                .find(|&slot| host.text(&host.world.inventory[slot]) == "4 minecraft:cobblestone")
                .unwrap() as i32;
            let menu_slot = if cobble < 9 {
                hotbar(3, cobble)
            } else {
                3 + cobble - 9
            };
            assert!(host.click(menu_slot, 0, 0));
            assert!(host.click(0, 1, 0));
            assert_eq!(host.cursor(), "3 minecraft:cobblestone");
            host.eye = [100.5, 66.62, 100.5];
            host.tick();
            assert!(host.world.menu.is_none());
            assert_eq!(held_window(&java), None);
            assert_eq!(
                host.held(),
                [
                    "2 minecraft:amethyst_shard",
                    "4 minecraft:cobblestone",
                    "4 minecraft:stone_bricks"
                ]
            );
            assert_eq!(counted(&java), [(SLOT_TAKE.to_owned(), 1)]);
        });
    });
}

/// `ServerPlayer.tick`'s check runs at `entity_tick_pre`, before `gameMode.tick`; a block change
/// at the menu's position during the player's tick (a delayed break) is one that vanilla's
/// check would have seen there: counted. `doTick`'s check, after it, closes the menu.
#[test]
fn a_block_change_during_the_players_tick_is_counted_and_closes_at_do_tick() {
    on_a_big_stack("live-menu-tick-phase", || {
        let jar = Arc::new(NativeJar::from_bytes(&bench_jar()).expect("load the fixture jar"));
        let java = java_with(jar);
        with_bridge(&java, BENCH_MENUS, || {
            let mut host = MenuTestHost::new(BENCH_ITEMS, BENCH_TYPES);
            open_bench(&java, &mut host);
            assert!(host.click(hotbar(3, 0), 0, 0));
            assert!(host.click(0, 1, 0));
            assert!(host.click(hotbar(3, 0), 0, 0));
            // Another block replaces the bench while the player ticks.
            host.tick_with(|host| host.block = "demo:null_bench".to_owned());
            assert!(host.world.menu.is_none());
            assert_eq!(held_window(&java), None);
            assert_eq!(
                host.held(),
                ["3 minecraft:amethyst_shard", "5 minecraft:cobblestone"]
            );
            assert_eq!(
                counted(&java),
                [(
                    "game-hook re-entry: sunrise#live_menu:still_valid_before_game_mode_tick"
                        .to_owned(),
                    1
                )]
            );
        });
    });
}

/// A window a Java click opens shows what the click's Java put in its menu, as vanilla's
/// `initMenu` sends it (`sendAllDataToRemote`): a shift-click on the chain bench's base opens
/// a fresh bench and moves the base into it, so Sunrise's new window holds the base, and not a
/// single item is lost or made on the way.
#[test]
fn a_menu_a_click_opens_shows_what_java_put_in_it() {
    on_a_big_stack("live-menu-chain", || {
        let jar = Arc::new(NativeJar::from_bytes(&bench_jar()).expect("load the fixture jar"));
        let java = java_with(jar);
        with_bridge(&java, BENCH_MENUS, || {
            let mut host = MenuTestHost::new(BENCH_ITEMS, BENCH_TYPES);
            host.world.inventory[0] = Some(host.stack("minecraft:cobblestone", 5));
            host.block = "demo:chain_bench".to_owned();
            assert_eq!(
                bench_part(&java, "demo:chain_bench").dispatch(
                    &mut host,
                    test_block_state(),
                    &right_click(),
                    false
                ),
                Some(InteractionResult::Consume)
            );
            let first = host.world.menu.as_ref().expect("the bench opened").window;
            // Three cobblestone into the base (all picked up, three right-clicked in, the rest
            // put back).
            assert!(host.click(hotbar(3, 0), 0, 0));
            for _ in 0..3 {
                assert!(host.click(0, 1, 0));
            }
            assert!(host.click(hotbar(3, 0), 0, 0));
            assert_eq!(host.owned()[0], "3 minecraft:cobblestone");
            // Shift-click the base: a new bench opens with it.
            assert!(host.click(0, 0, 1));
            let second = host
                .world
                .menu
                .as_ref()
                .expect("the next bench opened")
                .window;
            assert_ne!(second, first);
            assert_eq!(held_window(&java), Some(second));
            assert_eq!(host.owned(), ["3 minecraft:cobblestone", "empty", "empty"]);
            assert_eq!(host.cursor(), "empty");
            assert_eq!(host.held(), ["2 minecraft:cobblestone"]);
            assert_eq!(host.total("minecraft:cobblestone"), 5);
            assert!(counted(&java).is_empty(), "{:?}", counted(&java));
        });
    });
}

/// A menu with a data slot opens with the values `sendInitialData` sent (`ModMenuOpen::data`;
/// Sunrise refuses another count with `DataCountMismatch`). A menu whose live layout is not its
/// type's declared one (a provider building a one-input menu of the bench's type) never opens.
#[test]
fn data_slots_open_with_their_values_and_a_mismatched_layout_never_opens() {
    on_a_big_stack("live-menu-data", || {
        let jar = Arc::new(NativeJar::from_bytes(&bench_jar()).expect("load the fixture jar"));
        let java = java_with(jar);
        with_bridge(&java, BENCH_MENUS, || {
            let mut host = MenuTestHost::new(BENCH_ITEMS, BENCH_TYPES);
            host.block = "demo:data_bench".to_owned();
            assert_eq!(
                bench_part(&java, "demo:data_bench").dispatch(
                    &mut host,
                    test_block_state(),
                    &right_click(),
                    false
                ),
                Some(InteractionResult::Consume)
            );
            let open = host.world.menu.clone().expect("the data menu opened");
            assert_eq!(
                (open.menu.as_str(), open.data.as_slice()),
                ("demo:data", &[7][..])
            );
            assert_eq!(counted(&java), []);
            assert!(host.close_window());
            host.deliver_closes();

            host.block = "demo:odd_bench".to_owned();
            assert_eq!(
                bench_part(&java, "demo:odd_bench").dispatch(
                    &mut host,
                    test_block_state(),
                    &right_click(),
                    false
                ),
                Some(InteractionResult::Consume)
            );
            assert!(host.world.menu.is_none());
            assert_eq!(held_window(&java), None);
            assert_eq!(host.opens.len(), 1, "only the data menu opened");
            assert_eq!(
                counted(&java),
                [(
                    "game-hook re-entry: sunrise#openModMenu:layout_unverified".to_owned(),
                    1
                )]
            );
        });
    });
}

/// A disconnect or a death while an input sits in the menu: vanilla's `dropOrPlaceInInventory`
/// drops it (`isRemoved` for `KILLED`, `hasDisconnected`), as Sunrise drops its cursor
/// (`MenuRemoval::drops_items`). Nothing is lost or doubled: the stacks in the world and the
/// inventory are the ones the player had. A dead player throws nothing (`drop_stack` refuses), so
/// the stack is spawned where `Player.drop` puts it; a player Sunrise no longer has, at their last
/// known position. A logout that reaches the mod first closes the menu the same way.
#[test]
fn a_disconnect_or_a_death_drops_what_the_menu_held() {
    on_a_big_stack("live-menu-removal", || {
        let jar = Arc::new(NativeJar::from_bytes(&bench_jar()).expect("load the fixture jar"));
        for how in ["disconnect", "death", "logout"] {
            let java = java_with(Arc::clone(&jar));
            with_bridge(&java, BENCH_MENUS, || {
                let mut host = MenuTestHost::new(BENCH_ITEMS, BENCH_TYPES);
                open_bench(&java, &mut host);
                // All five cobblestone picked up, one right-clicked into the base.
                assert!(host.click(hotbar(3, 0), 0, 0));
                assert!(host.click(0, 1, 0));
                // A tick, so the bridge has seen the player where they stand.
                host.tick();
                match how {
                    "disconnect" => host.disconnect(),
                    "death" => {
                        host.health = 0.0;
                        assert!(host.close_window());
                        host.deliver_closes();
                    }
                    _ => {
                        // The logout reaches the mod before any `menu_closed`.
                        host.present = false;
                        crate::live_menu::close_for_removal(
                            &java,
                            &mut host,
                            UUID,
                            crate::live_menu::Removal::Disconnected,
                            None,
                        );
                        assert_eq!(held_window(&java), None);
                        host.close_window();
                        host.deliver_closes();
                    }
                }
                assert_eq!(held_window(&java), None, "{how}");
                let mut spawned: Vec<String> = host
                    .world
                    .spawned
                    .iter()
                    .map(|(stack, at)| {
                        format!("{} at y {:.2}", host.text(&Some(stack.clone())), at[1])
                    })
                    .collect();
                spawned.sort();
                assert_eq!(
                    spawned,
                    [
                        "1 minecraft:cobblestone at y 66.32",
                        "4 minecraft:cobblestone at y 66.32"
                    ],
                    "{how}"
                );
                assert_eq!(host.held(), ["3 minecraft:amethyst_shard"], "{how}");
                assert!(host.world.drops.is_empty(), "{how}");
                // The input's drop had no player to throw it: spawned with its own motion, where
                // vanilla's throw velocity comes from the player's rotation and random. Counted.
                assert_eq!(
                    counted(&java),
                    [
                        ("game-hook re-entry: sunrise#live_menu:drop".to_owned(), 1),
                        (
                            "game-hook re-entry: sunrise#live_menu:drop_without_player_throw"
                                .to_owned(),
                            1
                        ),
                    ],
                    "{how}"
                );
            });
        }
    });
}

/// The End's return replaces the player (`PlayerList.respawn` removes the old one with
/// `CHANGED_DIMENSION` first): the Java menu closes as that old player, removed but for a
/// dimension change, so what it held goes back into the inventory the new player kept, and
/// nothing drops.
#[test]
fn the_ends_return_places_back_as_the_old_player() {
    on_a_big_stack("live-menu-end-return", || {
        let jar = Arc::new(NativeJar::from_bytes(&bench_jar()).expect("load the fixture jar"));
        let java = java_with(jar);
        with_bridge(&java, BENCH_MENUS, || {
            let mut host = MenuTestHost::new(BENCH_ITEMS, BENCH_TYPES);
            open_bench(&java, &mut host);
            // One cobblestone into the base, the rest back.
            assert!(host.click(hotbar(3, 0), 0, 0));
            assert!(host.click(0, 1, 0));
            assert!(host.click(hotbar(3, 0), 0, 0));
            host.tick();
            let level = host
                .player_ref()
                .entity()
                .level(&host)
                .expect("the player's level");
            crate::live_menu::close_for_removal(
                &java,
                &mut host,
                UUID,
                crate::live_menu::Removal::ChangedDimension,
                Some((level, [100.5, 70.0, 100.5])),
            );
            assert_eq!(held_window(&java), None);
            host.close_window();
            host.deliver_closes();
            assert_eq!(
                host.held(),
                ["3 minecraft:amethyst_shard", "5 minecraft:cobblestone"]
            );
            assert!(host.world.spawned.is_empty());
            assert!(host.world.drops.is_empty());
            assert!(counted(&java).is_empty(), "{:?}", counted(&java));
        });
    });
}

/// A throw Sunrise refuses (`drop_stack` handing the stack back) never loses items: a cancelled
/// toss (`item_toss`) puts it back as `Inventory.placeItemBackInInventory` would, joining the
/// stack it came from, and a player who died or left since the call read them has it spawned
/// where they stood. The total count of the item stays what it was.
#[test]
fn a_refused_toss_keeps_every_item() {
    on_a_big_stack("live-menu-toss", || {
        let jar = Arc::new(NativeJar::from_bytes(&bench_jar()).expect("load the fixture jar"));
        let refusals = [
            None,
            Some(TossRefusal::Cancel),
            Some(TossRefusal::Die),
            Some(TossRefusal::Vanish),
        ];
        for refusal in refusals {
            let java = java_with(Arc::clone(&jar));
            with_bridge(&java, BENCH_MENUS, || {
                let mut host = MenuTestHost::new(BENCH_ITEMS, BENCH_TYPES);
                open_bench(&java, &mut host);
                // A tick, so the bridge has seen the player where they stand.
                host.tick();
                let before = host.total("minecraft:cobblestone");
                assert_eq!(before, 5);
                // One cobblestone into the base, the rest back; then throw it (`THROW`, Q).
                assert!(host.click(hotbar(3, 0), 0, 0));
                assert!(host.click(0, 1, 0));
                assert!(host.click(hotbar(3, 0), 0, 0));
                host.toss_refusal = refusal;
                assert!(host.click(0, 0, 4));
                assert_eq!(host.owned()[0], "empty", "{refusal:?}");
                assert_eq!(
                    host.total("minecraft:cobblestone"),
                    before,
                    "{refusal:?}: no item lost or made"
                );
                let tags: Vec<String> = counted(&java).into_iter().map(|(tag, _)| tag).collect();
                let spawned: Vec<String> = host
                    .world
                    .spawned
                    .iter()
                    .map(|(stack, at)| {
                        format!("{} at y {:.2}", host.text(&Some(stack.clone())), at[1])
                    })
                    .collect();
                match refusal {
                    None => {
                        assert_eq!(host.world.drops.len(), 1);
                        assert_eq!(
                            host.text(&Some(host.world.drops[0].clone())),
                            "1 minecraft:cobblestone"
                        );
                        assert!(spawned.is_empty());
                        assert_eq!(
                            tags,
                            ["game-hook re-entry: sunrise#live_menu:drop", SLOT_TAKE]
                        );
                    }
                    Some(TossRefusal::Cancel) => {
                        // Back onto the four it came from, in the selected hotbar slot.
                        assert!(host.world.drops.is_empty());
                        assert!(spawned.is_empty());
                        assert_eq!(
                            host.text(&host.world.inventory[0]),
                            "5 minecraft:cobblestone"
                        );
                        assert_eq!(
                            tags,
                            [
                                "game-hook re-entry: sunrise#live_menu:drop",
                                "game-hook re-entry: sunrise#live_menu:drop_refused",
                                SLOT_TAKE,
                            ]
                        );
                    }
                    Some(_) => {
                        // No live player to take it back: where they stood, as a dead player's
                        // drop is.
                        assert!(host.world.drops.is_empty());
                        assert_eq!(spawned, ["1 minecraft:cobblestone at y 66.32"]);
                        assert_eq!(
                            host.text(&host.world.inventory[0]),
                            "4 minecraft:cobblestone"
                        );
                        assert_eq!(
                            tags,
                            [
                                "game-hook re-entry: sunrise#live_menu:drop",
                                "game-hook re-entry: sunrise#live_menu:drop_refused",
                                "game-hook re-entry: sunrise#live_menu:drop_refused_spawned",
                                SLOT_TAKE,
                            ]
                        );
                    }
                }
            });
        }
    });
}

/// The real Runes 1.3.2 altar (`RUNES_JAR`): right-clicking `runes:crafting_altar` through the
/// block's `use_without_item` opens `runes:crafting` as the Sunrise window the player's
/// `containerCounter` names; cobblestone and an ender pearl clicked in make 16 arcane stones;
/// taking them consumes one of each and unlocks the recipe; the close hands the rest back.
#[test]
fn real_runes_altar_crafts_arcane_stones_through_the_sunrise_hooks() {
    let Some(jar_path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes live-menu bridge test skipped");
        return;
    };
    on_a_big_stack("live-menu-runes", move || {
        let jar = Arc::new(NativeJar::from_path(Path::new(&jar_path)).expect("load RUNES_JAR"));
        let java = java_with(jar);
        with_bridge(&java, &["runes:crafting"], || {
            let mut host = MenuTestHost::new(
                &[
                    "minecraft:cobblestone",
                    "minecraft:ender_pearl",
                    "runes:arcane_stone",
                ],
                &[("runes:crafting", 3, 0)],
            );
            host.world.inventory[0] = Some(host.stack("minecraft:cobblestone", 5));
            host.world.inventory[1] = Some(host.stack("minecraft:ender_pearl", 3));
            host.world.counter = 7;
            host.block = "runes:crafting_altar[facing=north]".to_owned();

            let altar =
                JavaBlockUsePart::new(Arc::clone(&java.state), "runes:crafting_altar".to_owned());
            assert_eq!(
                altar.dispatch(&mut host, test_block_state(), &right_click(), false),
                Some(InteractionResult::Consume)
            );
            let open = host
                .world
                .menu
                .clone()
                .expect("Sunrise opened the altar's menu");
            assert_eq!((open.window, open.menu.as_str()), (8, "runes:crafting"));
            assert_eq!(held_window(&java), Some(8));
            let opened = counted(&java);
            eprintln!("opening the altar counted: {opened:?}");
            assert_eq!(opened, []);

            // Cobblestone into the base, one at a time; the pearls shift-clicked in.
            assert!(host.click(hotbar(3, 0), 0, 0));
            assert!(host.click(0, 1, 0));
            assert!(host.click(hotbar(3, 0), 0, 0));
            assert!(host.click(hotbar(3, 1), 0, 1));
            assert_eq!(
                host.owned(),
                [
                    "1 minecraft:cobblestone",
                    "3 minecraft:ender_pearl",
                    "16 runes:arcane_stone"
                ]
            );
            assert_eq!(counted(&java), []);
            // A tick (both of `ServerPlayer`'s checks) keeps the altar's menu.
            host.tick();
            assert_eq!(held_window(&java), Some(8));

            // Take the stones: Runes' `onTake` consumes one of each input.
            assert!(host.click(2, 0, 0));
            assert_eq!(host.cursor(), "16 runes:arcane_stone");
            assert_eq!(host.owned(), ["empty", "2 minecraft:ender_pearl", "empty"]);
            assert_eq!(
                host.world.stats,
                [(StatType::Crafted, "runes:arcane_stone".to_owned(), 16)]
            );
            assert_eq!(host.world.recipes, ["runes:arcane_rune_medium_altar"]);
            let taken = counted(&java);
            eprintln!("taking the arcane stones counted: {taken:?}");
            // Runes' `RuneCrafter` cooldown read the player's `tickCount` and position (Sunrise's
            // facts, so exact) and went on to its crafting sound: the level's `RandomSource` for
            // its pitch is unknown, counted. `RecipeCraftingHolder.awardUsedRecipes`'
            // `RECIPE_CRAFTED` trigger has no Sunrise call (vanilla triggers aren't lent), and
            // the take's `menu_slot_take` is the engine's to raise: both counted.
            assert_eq!(
                taken,
                [
                    (SLOT_TAKE.to_owned(), 1),
                    (
                        "game-hook re-entry: sunrise#live_menu:recipe_crafted_trigger".to_owned(),
                        1
                    ),
                    (
                        "net/minecraft/world/level/Level#getRandom()Lnet/minecraft/util/RandomSource;"
                            .to_owned(),
                        1
                    ),
                ]
            );

            // Close: the stones (Sunrise's cursor return) and the pearls (Runes' `removed`).
            assert!(host.close_window());
            host.deliver_closes();
            assert_eq!(
                host.held(),
                [
                    "16 runes:arcane_stone",
                    "2 minecraft:ender_pearl",
                    "4 minecraft:cobblestone"
                ]
            );
            assert_eq!(held_window(&java), None);
            assert_eq!(counted(&java), taken, "the close counted nothing more");
        });
    });
}
