//! Exact, generic vanilla menu/container and container-screen models used by
//! mod-owned menus and screens.
//!
//! Item-combiner slot geometry is data supplied by the mod's own
//! `ItemCombinerMenuSlotDefinition.Builder` bytecode, and non-combiner menus
//! populate slots through `AbstractContainerMenu.addSlot` /
//! `addStandardInventorySlots`. Nothing here knows a particular mod or menu
//! type.

use std::collections::BTreeMap;

use crate::runtime::{MethodRef, NativeError, Value, expect_ref};

pub(crate) const SCREEN: &str = "net/minecraft/client/gui/screens/Screen";
pub(crate) const ABSTRACT_CONTAINER_SCREEN: &str =
    "net/minecraft/client/gui/screens/inventory/AbstractContainerScreen";
pub(crate) const ITEM_COMBINER_SCREEN: &str =
    "net/minecraft/client/gui/screens/inventory/ItemCombinerScreen";
pub(crate) const ABSTRACT_CONTAINER_MENU: &str =
    "net/minecraft/world/inventory/AbstractContainerMenu";
pub(crate) const ITEM_COMBINER_MENU: &str = "net/minecraft/world/inventory/ItemCombinerMenu";
pub(crate) const MENU_TYPE: &str = "net/minecraft/world/inventory/MenuType";
pub(crate) const SLOT: &str = "net/minecraft/world/inventory/Slot";
pub(crate) const INVENTORY: &str = "net/minecraft/world/entity/player/Inventory";
pub(crate) const SIMPLE_MENU_PROVIDER: &str = "net/minecraft/world/SimpleMenuProvider";
pub(crate) const MENU_PROVIDER: &str = "net/minecraft/world/MenuProvider";
pub(crate) const MENU_CONSTRUCTOR: &str = "net/minecraft/world/inventory/MenuConstructor";
pub(crate) const CONTAINER_LEVEL_ACCESS: &str =
    "net/minecraft/world/inventory/ContainerLevelAccess";
pub(crate) const CONTAINER_LEVEL_ACCESS_2: &str =
    "net/minecraft/world/inventory/ContainerLevelAccess$2";

/// `ItemStack.EMPTY`'s key, as the client surface answers the static (`client_surface.rs`).
const EMPTY_STACK: &str = "client/stack/empty";

/// `ItemStack.EMPTY`: the one typed host value every side answers the static with, so a menu's
/// stacks and the client item model agree on it, and a jar `String` spelling the key never
/// passes for it.
pub(crate) fn empty_stack() -> Value {
    crate::runtime::game_object::client_value(EMPTY_STACK.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenSlotKind {
    /// A menu's own input or storage container slot.
    Container,
    /// A crafting/combiner result slot (`ResultContainer`).
    Result,
    /// A player inventory slot (`Inventory`, 27 main + 9 hotbar).
    PlayerInventory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenSlot {
    pub index: i32,
    pub x: i32,
    pub y: i32,
    pub kind: ScreenSlotKind,
}

/// Outcome of writing a field through `menu_models::set_field`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldSetResult {
    /// Model owns the field and updated its state; caller must not mirror into `Runtime.fields`.
    Written,
    /// Not a vanilla field owned by this model (e.g. a subclass shadowing field); caller writes `Runtime.fields`.
    NotOwned,
    /// Write targeted a vanilla `final` field owned by this model; caller must count it instead of diverging.
    Counted,
}

/// Snapshot of a slot's last broadcast value (`AbstractContainerMenu.lastSlots`,
/// updated via `ItemStack.copy()` in `triggerSlotListeners` offsets 22-39).
#[derive(Clone, Debug, PartialEq)]
pub enum LastSlot {
    /// `ItemStack.EMPTY`.
    Empty,
    /// Snapshot of a client inventory slot's `ClientStack` contents at broadcast time.
    ClientStack(crate::runtime::client_surface::ClientStack),
    /// Any other non-empty stack value whose full contents/components are not modelled.
    Other(Value),
    /// The `ItemStack.copy()` of a server stack `broadcastChanges` stored (`live_menu.rs`),
    /// whose contents the live-stack model knows.
    Stack(Value),
    /// A copy `broadcastChanges` stored on a screen frame's client menu, whose contents Decaf
    /// does not have (`screen_models.rs`): a later comparison with it is counted.
    Unknown,
}

#[derive(Clone, Debug)]
pub struct BroadcastSlot {
    pub index: usize,
    pub slot: Value,
    pub container_slot: i32,
    pub container: Value,
    pub last: LastSlot,
    pub known_current: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct CombinerSlotInit {
    pub index: i32,
    pub x: i32,
    pub y: i32,
    pub may_place: Value,
}

#[derive(Clone, Debug)]
pub struct CombinerInit {
    pub input: Value,
    pub result: Value,
    pub inputs: Vec<CombinerSlotInit>,
    pub result_slot: CombinerSlotInit,
}

#[derive(Clone, Debug)]
pub(crate) struct SlotDef {
    pub(crate) index: i32,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) may_place: Value,
    pub(crate) empty: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Definition {
    pub(crate) inputs: Vec<SlotDef>,
    pub(crate) result: SlotDef,
}

#[derive(Clone, Debug)]
pub(crate) struct Slot {
    index: i32,
    container_slot: i32,
    container: Value,
    x: i32,
    y: i32,
    may_place: Option<Value>,
    kind: ScreenSlotKind,
}

impl Slot {
    /// `Slot.container`.
    pub(crate) fn container(&self) -> &Value {
        &self.container
    }

    /// `Slot.index`: the menu index `addSlot` gave it.
    pub(crate) fn index(&self) -> i32 {
        self.index
    }

    /// The `ItemCombinerMenuSlotDefinition$SlotDefinition.mayPlace()` predicate of a combiner
    /// input slot Decaf built.
    pub(crate) fn may_place(&self) -> Option<&Value> {
        self.may_place.as_ref()
    }
}

/// What a `DataSlot` Decaf built reads and writes: `DataSlot.forContainer`'s `DataSlot$1`
/// (`containerData.get/set(dataId)`) or `DataSlot.standalone()`'s `DataSlot$3` (its own
/// `value`, 0 at first).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DataSlotKind {
    Container { data: Value, index: i32 },
    Standalone { value: i32 },
}

#[derive(Clone, Debug)]
pub(crate) struct CombinerState {
    pub input: String,
    pub result: String,
    pub(crate) definition: Definition,
    pub(crate) access: Value,
    pub(crate) player: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct MenuState {
    pub(crate) menu_type: Value,
    pub(crate) container_id: i32,
    pub slots: String,
    pub(crate) last_slots: Vec<LastSlot>,
    pub(crate) container_listeners: Vec<Value>,
    pub(crate) combiner: Option<CombinerState>,
    pub(crate) seq: u64,
    /// The rest of 26.3 `AbstractContainerMenu`'s own state, which the live-menu bodies
    /// (`live_menu.rs`) read and write: `carried`, the quick-craft (drag) fields, `stateId`, the
    /// data slots, the synchronizer and its remote slots.
    pub(crate) live: LiveState,
}

/// One entry of 26.3 `AbstractContainerMenu.remoteSlots` (or `remoteCarried`).
///
/// `RemoteSlot.PLACEHOLDER` until a synchronizer is set, then the synchronizer's
/// `RemoteSlot.Synchronized`. Decaf's synchronizer only ever `force`s a slot (it does not take
/// the client's `HashedStack` predictions, `live_menu.rs`), so a synchronized slot is the stack
/// copy its last `force` stored, `None` before any.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum RemoteSlot {
    #[default]
    Placeholder,
    Synchronized(Option<Value>),
}

/// A 26.3 `DataSlot` added through `addDataSlot` / `addDataSlots`: the object itself (a jar
/// `DataSlot` subclass, or Decaf's `DataSlot.forContainer` / `standalone` model).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DataSlotEntry {
    pub(crate) slot: Value,
}

/// 26.3 `AbstractContainerMenu` fields beyond `slots`/`lastSlots`/`containerListeners`
/// (`<init>` offsets 0-86 give the initial values below).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LiveState {
    /// `carried` (offset 29: `ItemStack.EMPTY`).
    pub(crate) carried: Value,
    /// `quickcraftType` (offset 59: `-1`).
    pub(crate) quickcraft_type: i32,
    /// `quickcraftStatus` (`0`).
    pub(crate) quickcraft_status: i32,
    /// `quickcraftSlots`, a `HashSet<Slot>` (offset 63), in insertion order. Java iterates it
    /// in identity-hash order, which no one can predict: `live_menu.rs` counts the drag end
    /// whenever that order could be observed.
    pub(crate) quickcraft_slots: Vec<Value>,
    /// `stateId` (`0`).
    pub(crate) state_id: i32,
    /// `stateId` stays exact only while Decaf's synchronizer is the only thing that moved it;
    /// a click whose client predictions Decaf did not take (`setRemoteSlotUnsafe`) makes it
    /// unknown, and a jar read of it then counts.
    pub(crate) state_id_known: bool,
    /// `suppressRemoteUpdates`.
    pub(crate) suppress_remote_updates: bool,
    /// `dataSlots`, and `remoteDataSlots` beside them (each added as `0`).
    pub(crate) data_slots: Vec<DataSlotEntry>,
    pub(crate) remote_data_slots: Vec<i32>,
    /// `synchronizer`: `None` until `setSynchronizer`. Decaf only models the server player's
    /// own (`ServerPlayer$1`), whose sends it records as effects.
    pub(crate) synchronizer: Option<Value>,
    /// `remoteSlots`, one per slot (`addSlot` offsets 31-60).
    pub(crate) remote_slots: Vec<RemoteSlot>,
    /// `remoteCarried` (offset 51: `RemoteSlot.PLACEHOLDER`).
    pub(crate) remote_carried: RemoteSlot,
}

impl Default for LiveState {
    fn default() -> Self {
        Self {
            carried: empty_stack(),
            quickcraft_type: -1,
            quickcraft_status: 0,
            quickcraft_slots: Vec::new(),
            state_id: 0,
            state_id_known: true,
            suppress_remote_updates: false,
            data_slots: Vec::new(),
            remote_data_slots: Vec::new(),
            synchronizer: None,
            remote_slots: Vec::new(),
            remote_carried: RemoteSlot::Placeholder,
        }
    }
}

/// The contents of a 26.3 `Inventory`: `items` (36 stacks), `selected`, and the
/// `EntityEquipment` stacks `EQUIPMENT_SLOT_MAPPING` reads for slots 36-42 (FEET, LEGS, CHEST,
/// HEAD, OFFHAND, BODY, SADDLE). `None` equipment: an `Inventory` built around an
/// `EntityEquipment` Decaf does not model (or `null`), whose slots 36-42 count.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InventoryContents {
    pub(crate) items: Vec<Value>,
    pub(crate) equipment: Option<Vec<Value>>,
    pub(crate) selected: i32,
}

/// `Inventory.INVENTORY_SIZE`.
pub(crate) const INVENTORY_ITEMS: usize = 36;
/// `Inventory.EQUIPMENT_SLOT_MAPPING.size()` (slots 36-42).
pub(crate) const INVENTORY_EQUIPMENT: usize = 7;

impl InventoryContents {
    /// `new Inventory(player, equipment)`: 36 `EMPTY` items, `selected` 0.
    pub(crate) fn new(equipment: Option<Vec<Value>>) -> Self {
        Self {
            items: vec![empty_stack(); INVENTORY_ITEMS],
            equipment,
            selected: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ScreenState {
    pub menu: Value,
    pub title: Value,
    pub minecraft: Value,
    pub font: Value,
    pub player_inventory_title: Value,
    pub width: i32,
    pub height: i32,
    pub initialized: bool,
    pub is_container: bool,
    pub image_width: i32,
    pub image_height: i32,
    pub title_label_x: i32,
    pub title_label_y: i32,
    pub inventory_label_x: i32,
    pub inventory_label_y: i32,
    pub left_pos: i32,
    pub top_pos: i32,
    /// `ItemCombinerScreen.menuResource` as the real `Identifier` value
    /// (`None` for non-`ItemCombinerScreen` screens).
    pub menu_resource: Option<Value>,
}

#[derive(Clone, Debug)]
pub enum MenuAction {
    SlotsChanged {
        menu: Value,
        container: Value,
    },
    CreateResult {
        menu: Value,
    },
    Predicate {
        predicate: Value,
        stack: Value,
    },
    MenuSupplier {
        supplier: Value,
        container_id: Value,
        inventory: Value,
    },
    MenuConstructor {
        constructor: Value,
        container_id: Value,
        inventory: Value,
        player: Value,
    },
    AccessEvaluate {
        function: Value,
        level: Value,
        pos: Value,
        default: Option<Value>,
    },
    AccessExecute {
        consumer: Value,
        level: Value,
        pos: Value,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum Object {
    Menu(MenuState),
    MenuType {
        constructor: Value,
        required_features: Value,
    },
    SimpleMenuProvider {
        menu_constructor: Value,
        title: Value,
    },
    ContainerLevelAccess {
        level: Value,
        pos: Value,
    },
    Screen(ScreenState),
    Inventory {
        player: Value,
        /// `None`: an inventory whose contents Decaf does not know (a block use whose embedder
        /// gave none), whose item reads count.
        contents: Option<InventoryContents>,
    },
    Builder {
        inputs: Vec<SlotDef>,
        result: Option<SlotDef>,
    },
    Definition(Definition),
    SlotDefinition(SlotDef),
    Slot(Slot),
    SlotList {
        slots: Vec<String>,
    },
    Container {
        size: i32,
        items: Vec<Value>,
        result: bool,
        owner: Option<String>,
        recipe_used: Option<Value>,
    },
    Predicate(bool),
    Pending(MenuAction),
    /// 26.3 `DataSlot` state: `prevValue` (`checkAndClearUpdateFlag`) for any `DataSlot`,
    /// a jar subclass's filed under its object key, and the body of the two Decaf builds.
    DataSlot {
        kind: Option<DataSlotKind>,
        prev_value: i32,
    },
}

pub(crate) type States = BTreeMap<String, Object>;

#[derive(Clone, Debug, PartialEq)]
pub struct ScreenLayout {
    pub image_width: i32,
    pub image_height: i32,
    pub title_label: [i32; 2],
    pub inventory_label: [i32; 2],
    pub left_pos: i32,
    pub top_pos: i32,
    pub menu_resource: Option<String>,
    pub slots: Vec<ScreenSlot>,
}

pub(crate) fn screen_layout(states: &States, screen: &Value) -> Option<ScreenLayout> {
    let key = expect_ref(screen)?;
    let Object::Screen(state) = states.get(key)? else {
        return None;
    };
    if !state.is_container {
        return None;
    }
    let slots = match expect_ref(&state.menu).and_then(|menu_key| states.get(menu_key)) {
        Some(Object::Menu(menu)) => states.get(&menu.slots).and_then(|object| match object {
            Object::SlotList { slots } => Some(
                slots
                    .iter()
                    .filter_map(|slot| match states.get(slot) {
                        Some(Object::Slot(slot)) => Some(ScreenSlot {
                            index: slot.index,
                            x: slot.x,
                            y: slot.y,
                            kind: slot.kind,
                        }),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })?,
        _ => Vec::new(),
    };
    Some(ScreenLayout {
        image_width: state.image_width,
        image_height: state.image_height,
        title_label: [state.title_label_x, state.title_label_y],
        inventory_label: [state.inventory_label_x, state.inventory_label_y],
        left_pos: state.left_pos,
        top_pos: state.top_pos,
        menu_resource: state
            .menu_resource
            .as_ref()
            .and_then(crate::runtime::host_ids::identifier_key)
            .map(str::to_owned),
        slots,
    })
}

pub(crate) fn screen_initialized(states: &States, screen: &Value) -> Option<bool> {
    let key = expect_ref(screen)?;
    match states.get(key)? {
        Object::Screen(state) => Some(state.initialized),
        _ => None,
    }
}

pub(crate) fn set_screen_initialized(states: &mut States, screen: &Value, initialized: bool) {
    let Some(key) = expect_ref(screen).cloned() else {
        return;
    };
    if let Some(Object::Screen(state)) = states.get_mut(&key) {
        state.initialized = initialized;
    }
}

pub(crate) fn screen_is_container(states: &States, screen: &Value) -> bool {
    let Some(key) = expect_ref(screen) else {
        return false;
    };
    matches!(
        states.get(key),
        Some(Object::Screen(state)) if state.is_container
    )
}

/// Find the most recently constructed menu whose `MenuType` equals `menu_type`.
/// `null` never acts as a wildcard.
pub(crate) fn menu_for_type(states: &States, menu_type: &Value) -> Option<Value> {
    if matches!(menu_type, Value::Ref(None)) {
        return None;
    }
    states
        .iter()
        .filter_map(|(key, object)| match object {
            Object::Menu(state) if &state.menu_type == menu_type => Some((state.seq, key)),
            _ => None,
        })
        .max_by_key(|(seq, _)| *seq)
        .map(|(_, key)| marker(key.clone()))
}

/// 26.3 `AbstractContainerMenu.addSlotListener` offsets 0-24: returns `Some(true)` if
/// the listener was added (caller must then run `broadcastChanges`), `Some(false)` if
/// already present, `None` if receiver is not a modelled menu.
pub(crate) fn menu_add_slot_listener(
    states: &mut States,
    menu: &Value,
    listener: &Value,
) -> Option<bool> {
    let key = expect_ref(menu)?.clone();
    let Object::Menu(state) = states.get_mut(&key)? else {
        return None;
    };
    if state.container_listeners.contains(listener) {
        return Some(false);
    }
    state.container_listeners.push(listener.clone());
    Some(true)
}

/// 26.3 `AbstractContainerMenu.removeSlotListener` offsets 0-11: removes the first
/// matching listener from `containerListeners`.
pub(crate) fn menu_remove_slot_listener(
    states: &mut States,
    menu: &Value,
    listener: &Value,
) -> Option<bool> {
    let key = expect_ref(menu)?.clone();
    let Object::Menu(state) = states.get_mut(&key)? else {
        return None;
    };
    if let Some(pos) = state.container_listeners.iter().position(|l| l == listener) {
        state.container_listeners.remove(pos);
        Some(true)
    } else {
        Some(false)
    }
}

pub(crate) fn menu_combiner_access(states: &States, menu: &Value) -> Option<Value> {
    let key = expect_ref(menu)?;
    match states.get(key) {
        Some(Object::Menu(state)) => state.combiner.as_ref().map(|c| c.access.clone()),
        _ => None,
    }
}

pub(crate) fn alloc_simple_menu_provider(next_id: &mut u64) -> Value {
    marker(next_key(next_id, SIMPLE_MENU_PROVIDER))
}

pub(crate) fn allocate_inventory(states: &mut States, next_id: &mut u64, player: Value) -> Value {
    let key = next_key(next_id, INVENTORY);
    states.insert(
        key.clone(),
        Object::Inventory {
            player,
            contents: None,
        },
    );
    marker(key)
}

/// A vanilla `Slot(container, index, 0, 0)` with no menu: the clicked slot an item part's
/// click override reads (`item_parts.rs`). `live_menu.rs` runs its 26.3 bodies.
pub(crate) fn allocate_plain_slot(
    states: &mut States,
    next_id: &mut u64,
    container: Value,
    index: i32,
) -> Value {
    let slot = marker(next_key(next_id, SLOT));
    register_slot(
        states,
        &slot,
        container,
        index,
        0,
        0,
        None,
        ScreenSlotKind::PlayerInventory,
    );
    slot
}

pub(crate) fn simple_menu_provider_constructor(states: &States, provider: &Value) -> Option<Value> {
    let key = expect_ref(provider)?;
    match states.get(key) {
        Some(Object::SimpleMenuProvider {
            menu_constructor, ..
        }) => Some(menu_constructor.clone()),
        _ => None,
    }
}

pub(crate) fn container_level_access_pos(states: &States, access: &Value) -> Option<[i32; 3]> {
    let key = expect_ref(access)?;
    match states.get(key) {
        Some(Object::ContainerLevelAccess { pos, .. }) => {
            let pos_key = crate::runtime::game_object::handle_key(pos)?;
            crate::runtime::server_values::block_pos(pos_key)
        }
        _ => None,
    }
}

pub(crate) fn slot_container_and_index(states: &States, slot: &Value) -> Option<(Value, i32)> {
    let key = expect_ref(slot)?;
    match states.get(key) {
        Some(Object::Slot(slot)) => Some((slot.container.clone(), slot.container_slot)),
        _ => None,
    }
}

pub(crate) fn register_slot(
    states: &mut States,
    slot: &Value,
    container: Value,
    container_slot: i32,
    x: i32,
    y: i32,
    may_place: Option<Value>,
    kind: ScreenSlotKind,
) {
    let Some(key) = expect_ref(slot).cloned() else {
        return;
    };
    states.insert(
        key,
        Object::Slot(Slot {
            index: 0,
            container_slot,
            container,
            x,
            y,
            may_place,
            kind,
        }),
    );
}

pub(crate) fn register_container_screen(
    states: &mut States,
    screen: &Value,
    menu: Value,
    player_inventory_title: Value,
    title: Value,
    image_width: i32,
    image_height: i32,
    menu_resource: Option<Value>,
) {
    let Some(key) = expect_ref(screen).cloned() else {
        return;
    };
    states.insert(
        key,
        Object::Screen(ScreenState {
            menu,
            title,
            minecraft: crate::runtime::game_object::client_value(crate::runtime::client_surface::MINECRAFT.into()),
            font: crate::runtime::game_object::client_value("client/font".into()),
            player_inventory_title,
            width: 0,
            height: 0,
            initialized: false,
            is_container: true,
            image_width,
            image_height,
            title_label_x: 8,
            title_label_y: 6,
            inventory_label_x: 8,
            inventory_label_y: image_height.wrapping_sub(94),
            left_pos: 0,
            top_pos: 0,
            menu_resource,
        }),
    );
}

/// Initialize the structural containers and `MenuState` for an `ItemCombinerMenu.<init>`
/// (offsets 0-51) so `Runtime` can run virtual `addSlot` (`createInputSlots` offset 55,
/// `createResultSlot` offset 34) and virtual `addStandardInventorySlots` (offset 72).
pub(crate) fn register_combiner_menu(
    states: &mut States,
    next_id: &mut u64,
    menu: &Value,
    menu_type: Value,
    container_id: i32,
    access: Value,
    player: Value,
    definition_val: &Value,
) -> Option<CombinerInit> {
    let key = expect_ref(menu)?.clone();
    let def_key = expect_ref(definition_val)?;
    let Object::Definition(definition) = states.get(def_key).cloned()? else {
        return None;
    };
    let slots = next_key(next_id, "net/minecraft/core/NonNullList");
    states.insert(slots.clone(), Object::SlotList { slots: Vec::new() });
    let result = next_key(next_id, "net/minecraft/world/inventory/ItemCombinerMenu$1");
    states.insert(
        result.clone(),
        Object::Container {
            size: 1,
            items: vec![empty_stack()],
            result: true,
            owner: Some(key.clone()),
            recipe_used: None,
        },
    );
    let input = next_key(next_id, "net/minecraft/world/inventory/ItemCombinerMenu$4");
    states.insert(
        input.clone(),
        Object::Container {
            size: definition.inputs.len() as i32,
            items: vec![empty_stack(); definition.inputs.len()],
            result: false,
            owner: Some(key.clone()),
            recipe_used: None,
        },
    );
    let init = CombinerInit {
        input: marker(input.clone()),
        result: marker(result.clone()),
        inputs: definition
            .inputs
            .iter()
            .map(|s| CombinerSlotInit {
                index: s.index,
                x: s.x,
                y: s.y,
                may_place: s.may_place.clone(),
            })
            .collect(),
        result_slot: CombinerSlotInit {
            index: definition.result.index,
            x: definition.result.x,
            y: definition.result.y,
            may_place: definition.result.may_place.clone(),
        },
    };
    let seq = next_menu_seq(states);
    states.insert(
        key,
        Object::Menu(MenuState {
            menu_type,
            container_id,
            slots,
            last_slots: Vec::new(),
            container_listeners: Vec::new(),
            combiner: Some(CombinerState {
                input,
                result,
                definition,
                access,
                player,
            }),
            seq,
            live: LiveState::default(),
        }),
    );
    Some(init)
}

pub(crate) fn menu_listeners(states: &States, menu: &Value) -> Vec<Value> {
    let Some(key) = expect_ref(menu) else {
        return Vec::new();
    };
    match states.get(key) {
        Some(Object::Menu(state)) => state.container_listeners.clone(),
        _ => Vec::new(),
    }
}

pub(crate) fn menu_broadcast_slots(states: &States, menu: &Value) -> Option<Vec<BroadcastSlot>> {
    let key = expect_ref(menu)?;
    let Object::Menu(state) = states.get(key)? else {
        return None;
    };
    let Object::SlotList { slots } = states.get(&state.slots)? else {
        return None;
    };
    let mut out = Vec::with_capacity(slots.len());
    for (index, slot_key) in slots.iter().enumerate() {
        let Object::Slot(slot) = states.get(slot_key)? else {
            return None;
        };
        let last = state
            .last_slots
            .get(index)
            .cloned()
            .unwrap_or(LastSlot::Empty);
        let known_current = expect_ref(&slot.container)
            .and_then(|container_key| states.get(container_key))
            .and_then(|object| match object {
                Object::Container { items, result, .. } => {
                    let pos = if *result {
                        0
                    } else {
                        slot.container_slot as usize
                    };
                    Some(items.get(pos).cloned().unwrap_or_else(empty_stack))
                }
                _ => None,
            });
        out.push(BroadcastSlot {
            index,
            slot: marker(slot_key.clone()),
            container_slot: slot.container_slot,
            container: slot.container.clone(),
            last,
            known_current,
        });
    }
    Some(out)
}

pub(crate) fn menu_set_last_slot(states: &mut States, menu: &Value, index: usize, slot: LastSlot) {
    let Some(key) = expect_ref(menu).cloned() else {
        return;
    };
    if let Some(Object::Menu(state)) = states.get_mut(&key) {
        if index >= state.last_slots.len() {
            state.last_slots.resize(index + 1, LastSlot::Empty);
        }
        state.last_slots[index] = slot;
    }
}

pub(crate) fn is_menu_class(owner: &str) -> bool {
    matches!(
        owner,
        ABSTRACT_CONTAINER_MENU
            | ITEM_COMBINER_MENU
            | MENU_TYPE
            | "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition"
            | "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder"
            | "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition"
            | "net/minecraft/world/SimpleContainer"
            | "net/minecraft/world/inventory/ResultContainer"
            | "net/minecraft/world/inventory/RecipeCraftingHolder"
            | "net/minecraft/world/Container"
            | SLOT
            | INVENTORY
            | SIMPLE_MENU_PROVIDER
            | MENU_PROVIDER
            | MENU_CONSTRUCTOR
            | CONTAINER_LEVEL_ACCESS
            | CONTAINER_LEVEL_ACCESS_2
            | "java/util/function/Predicate"
            | SCREEN
            | ABSTRACT_CONTAINER_SCREEN
            | ITEM_COMBINER_SCREEN
    )
}

fn marker(key: impl Into<String>) -> Value {
    let key = key.into();
    if crate::runtime::jar_object::key_class(&key).is_some()
        || crate::runtime::jar_object::menu_object_key(&key).is_some()
    {
        crate::runtime::jar_object::stored(&key)
    } else {
        Value::Ref(Some(key))
    }
}

/// Host-synthesized menu objects use a non-numeric tagged suffix `#menu<n>`
/// driven by a monotonic counter on `StdHost`, so they are collectable by
/// `heap::collect` (`heap.rs`) without ever colliding with `Runtime::new_object`
/// numeric allocation keys (`jar_object::parse_key` / `freed_objects`) or reusing
/// collected ids.
fn next_key(next_id: &mut u64, prefix: &str) -> String {
    let id = *next_id;
    *next_id = next_id.saturating_add(1);
    format!("{prefix}#menu{id}")
}

fn next_menu_seq(states: &States) -> u64 {
    states
        .values()
        .filter_map(|object| match object {
            Object::Menu(state) => Some(state.seq),
            _ => None,
        })
        .max()
        .map_or(0, |seq| seq.saturating_add(1))
}

/// The handle of a model object filed under `key` (`marker`).
pub(crate) fn marker_of(key: &str) -> Value {
    marker(key.to_owned())
}

/// A new `DataSlot$1` / `DataSlot$3` (`class`) object of `kind`.
pub(crate) fn new_data_slot(
    states: &mut States,
    next_id: &mut u64,
    class: &str,
    kind: DataSlotKind,
) -> Value {
    let key = next_key(next_id, class);
    states.insert(
        key.clone(),
        Object::DataSlot {
            kind: Some(kind),
            prev_value: 0,
        },
    );
    marker(key)
}

/// The body kind of a `DataSlot` Decaf built.
pub(crate) fn data_slot_kind(states: &States, key: &str) -> Option<DataSlotKind> {
    match states.get(key) {
        Some(Object::DataSlot { kind, .. }) => kind.clone(),
        _ => None,
    }
}

/// `DataSlot.checkAndClearUpdateFlag()`'s `prevValue` swap: store `value`, answer the old one
/// (0 for a `DataSlot` seen the first time, the field's default).
pub(crate) fn data_slot_prev_value(states: &mut States, key: &str, value: i32) -> i32 {
    match states.get_mut(key) {
        Some(Object::DataSlot { prev_value, .. }) => std::mem::replace(prev_value, value),
        _ => {
            states.insert(
                key.to_owned(),
                Object::DataSlot {
                    kind: None,
                    prev_value: value,
                },
            );
            0
        }
    }
}

/// `DataSlot$3.set(value)`.
pub(crate) fn set_standalone_data_slot(states: &mut States, key: &str, new_value: i32) {
    if let Some(Object::DataSlot {
        kind: Some(DataSlotKind::Standalone { value }),
        ..
    }) = states.get_mut(key)
    {
        *value = new_value;
    }
}

fn slot_kind_for_container(states: &States, container: &Value) -> ScreenSlotKind {
    if crate::runtime::game_object::handle_key(container) == Some("client/inventory") {
        return ScreenSlotKind::PlayerInventory;
    }
    if let Some(key) = expect_ref(container) {
        match states.get(key) {
            Some(Object::Container { result: true, .. }) => return ScreenSlotKind::Result,
            Some(Object::Inventory { .. }) => return ScreenSlotKind::PlayerInventory,
            _ => {}
        }
    }
    ScreenSlotKind::Container
}

fn screen_field_owner(name: &str, descriptor: &str) -> Option<&'static str> {
    matches!(
        (name, descriptor),
        ("title", "Lnet/minecraft/network/chat/Component;")
            | ("minecraft", "Lnet/minecraft/client/Minecraft;")
            | ("font", "Lnet/minecraft/client/gui/Font;")
            | ("width", "I")
            | ("height", "I")
    )
    .then_some(SCREEN)
}

fn container_screen_field_owner(name: &str, descriptor: &str) -> Option<&'static str> {
    if matches!(
        (name, descriptor),
        ("imageWidth", "I")
            | ("imageHeight", "I")
            | ("titleLabelX", "I")
            | ("titleLabelY", "I")
            | ("inventoryLabelX", "I")
            | ("inventoryLabelY", "I")
            | ("leftPos", "I")
            | ("topPos", "I")
            | (
                "menu",
                "Lnet/minecraft/world/inventory/AbstractContainerMenu;"
            )
            | (
                "playerInventoryTitle",
                "Lnet/minecraft/network/chat/Component;"
            )
    ) {
        return Some(ABSTRACT_CONTAINER_SCREEN);
    }
    screen_field_owner(name, descriptor)
}

/// Resolve fields javac names through a jar subclass to the exact 26.3 vanilla
/// declaring class (walking the vanilla superclass hierarchy).
pub(crate) fn field_owner(owner: &str, name: &str, descriptor: &str) -> Option<&'static str> {
    match owner {
        ITEM_COMBINER_SCREEN => {
            if (name, descriptor) == ("menuResource", "Lnet/minecraft/resources/Identifier;") {
                return Some(ITEM_COMBINER_SCREEN);
            }
            container_screen_field_owner(name, descriptor)
        }
        ABSTRACT_CONTAINER_SCREEN => container_screen_field_owner(name, descriptor),
        SCREEN => screen_field_owner(name, descriptor),
        ITEM_COMBINER_MENU => match (name, descriptor) {
            ("inputSlots", "Lnet/minecraft/world/Container;")
            | ("resultSlots", "Lnet/minecraft/world/inventory/ResultContainer;")
            | ("access", "Lnet/minecraft/world/inventory/ContainerLevelAccess;")
            | ("player", "Lnet/minecraft/world/entity/player/Player;")
            | ("resultSlotIndex", "I") => Some(ITEM_COMBINER_MENU),
            ("slots", "Lnet/minecraft/core/NonNullList;") | ("containerId", "I") => {
                Some(ABSTRACT_CONTAINER_MENU)
            }
            _ => None,
        },
        ABSTRACT_CONTAINER_MENU => match (name, descriptor) {
            ("slots", "Lnet/minecraft/core/NonNullList;") | ("containerId", "I") => {
                Some(ABSTRACT_CONTAINER_MENU)
            }
            _ => None,
        },
        SLOT => match (name, descriptor) {
            ("container", "Lnet/minecraft/world/Container;")
            | ("x", "I")
            | ("y", "I")
            | ("index", "I") => Some(SLOT),
            _ => None,
        },
        INVENTORY => (name == "player"
            && descriptor == "Lnet/minecraft/world/entity/player/Player;")
            .then_some(INVENTORY),
        _ => None,
    }
}

pub(crate) fn field(states: &States, receiver: &Value, r: &MethodRef) -> Option<Value> {
    let key = expect_ref(receiver)?;
    let object = states.get(key)?;
    match (
        object,
        r.owner.as_str(),
        r.name.as_str(),
        r.descriptor.as_str(),
    ) {
        (
            Object::Menu(state),
            ABSTRACT_CONTAINER_MENU,
            "slots",
            "Lnet/minecraft/core/NonNullList;",
        ) => Some(marker(state.slots.clone())),
        (Object::Menu(state), ABSTRACT_CONTAINER_MENU, "containerId", "I") => {
            Some(Value::Int(state.container_id))
        }
        (
            Object::Menu(state),
            ITEM_COMBINER_MENU,
            "inputSlots",
            "Lnet/minecraft/world/Container;",
        ) => state.combiner.as_ref().map(|c| marker(c.input.clone())),
        (
            Object::Menu(state),
            ITEM_COMBINER_MENU,
            "resultSlots",
            "Lnet/minecraft/world/inventory/ResultContainer;",
        ) => state.combiner.as_ref().map(|c| marker(c.result.clone())),
        (
            Object::Menu(state),
            ITEM_COMBINER_MENU,
            "access",
            "Lnet/minecraft/world/inventory/ContainerLevelAccess;",
        ) => state.combiner.as_ref().map(|c| c.access.clone()),
        (
            Object::Menu(state),
            ITEM_COMBINER_MENU,
            "player",
            "Lnet/minecraft/world/entity/player/Player;",
        ) => state.combiner.as_ref().map(|c| c.player.clone()),
        (Object::Menu(state), ITEM_COMBINER_MENU, "resultSlotIndex", "I") => state
            .combiner
            .as_ref()
            .map(|c| Value::Int(c.definition.inputs.len() as i32)),
        (
            Object::Inventory { player, .. },
            INVENTORY,
            "player",
            "Lnet/minecraft/world/entity/player/Player;",
        ) => Some(player.clone()),
        (Object::Slot(slot), SLOT, "container", "Lnet/minecraft/world/Container;") => {
            Some(slot.container.clone())
        }
        (Object::Slot(slot), SLOT, "x", "I") => Some(Value::Int(slot.x)),
        (Object::Slot(slot), SLOT, "y", "I") => Some(Value::Int(slot.y)),
        (Object::Slot(slot), SLOT, "index", "I") => Some(Value::Int(slot.index)),
        // 26.3 Screen fields
        (Object::Screen(state), SCREEN, "title", "Lnet/minecraft/network/chat/Component;") => {
            Some(state.title.clone())
        }
        (Object::Screen(state), SCREEN, "minecraft", "Lnet/minecraft/client/Minecraft;") => {
            Some(state.minecraft.clone())
        }
        (Object::Screen(state), SCREEN, "font", "Lnet/minecraft/client/gui/Font;") => {
            Some(state.font.clone())
        }
        (Object::Screen(state), SCREEN, "width", "I") => Some(Value::Int(state.width)),
        (Object::Screen(state), SCREEN, "height", "I") => Some(Value::Int(state.height)),
        // 26.3 AbstractContainerScreen fields
        (Object::Screen(state), ABSTRACT_CONTAINER_SCREEN, "imageWidth", "I")
            if state.is_container =>
        {
            Some(Value::Int(state.image_width))
        }
        (Object::Screen(state), ABSTRACT_CONTAINER_SCREEN, "imageHeight", "I")
            if state.is_container =>
        {
            Some(Value::Int(state.image_height))
        }
        (Object::Screen(state), ABSTRACT_CONTAINER_SCREEN, "titleLabelX", "I")
            if state.is_container =>
        {
            Some(Value::Int(state.title_label_x))
        }
        (Object::Screen(state), ABSTRACT_CONTAINER_SCREEN, "titleLabelY", "I")
            if state.is_container =>
        {
            Some(Value::Int(state.title_label_y))
        }
        (Object::Screen(state), ABSTRACT_CONTAINER_SCREEN, "inventoryLabelX", "I")
            if state.is_container =>
        {
            Some(Value::Int(state.inventory_label_x))
        }
        (Object::Screen(state), ABSTRACT_CONTAINER_SCREEN, "inventoryLabelY", "I")
            if state.is_container =>
        {
            Some(Value::Int(state.inventory_label_y))
        }
        (Object::Screen(state), ABSTRACT_CONTAINER_SCREEN, "leftPos", "I")
            if state.is_container =>
        {
            Some(Value::Int(state.left_pos))
        }
        (Object::Screen(state), ABSTRACT_CONTAINER_SCREEN, "topPos", "I") if state.is_container => {
            Some(Value::Int(state.top_pos))
        }
        (
            Object::Screen(state),
            ABSTRACT_CONTAINER_SCREEN,
            "menu",
            "Lnet/minecraft/world/inventory/AbstractContainerMenu;",
        ) if state.is_container => Some(state.menu.clone()),
        (
            Object::Screen(state),
            ABSTRACT_CONTAINER_SCREEN,
            "playerInventoryTitle",
            "Lnet/minecraft/network/chat/Component;",
        ) if state.is_container => Some(state.player_inventory_title.clone()),
        // 26.3 ItemCombinerScreen.menuResource (exact Identifier handle)
        (
            Object::Screen(state),
            ITEM_COMBINER_SCREEN,
            "menuResource",
            "Lnet/minecraft/resources/Identifier;",
        ) => state.menu_resource.clone(),
        _ => None,
    }
}

/// Write a modelled vanilla field when `r.owner` matches the exact vanilla
/// declaring class and descriptor.
/// - `FieldSetResult::Written`: mutable vanilla field updated in model state
///   (caller must not mirror a stale copy in `Runtime.fields`).
/// - `FieldSetResult::Counted`: write targeted a `final` vanilla field owned by
///   this model (`title`, `menu`, `slots`, etc.); caller must count it so
///   `putfield` can never diverge from `getfield`.
/// - `FieldSetResult::NotOwned`: subclass field (including a shadowing field
///   like `private int leftPos`); caller stores in `Runtime.fields`.
pub(crate) fn set_field(
    states: &mut States,
    receiver: &Value,
    r: &MethodRef,
    value: &Value,
) -> FieldSetResult {
    let Some(key) = expect_ref(receiver).cloned() else {
        return FieldSetResult::NotOwned;
    };
    match states.get_mut(&key) {
        Some(Object::Screen(state)) => {
            match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
                (SCREEN, "width", "I") => {
                    if let Value::Int(v) = value {
                        state.width = *v;
                    }
                    FieldSetResult::Written
                }
                (SCREEN, "height", "I") => {
                    if let Value::Int(v) = value {
                        state.height = *v;
                    }
                    FieldSetResult::Written
                }
                (ABSTRACT_CONTAINER_SCREEN, name, "I") if state.is_container => {
                    match name {
                        // 26.3 `AbstractContainerScreen.imageWidth` / `imageHeight` are `final`.
                        "imageWidth" | "imageHeight" => return FieldSetResult::Counted,
                        "titleLabelX" | "titleLabelY" | "inventoryLabelX" | "inventoryLabelY"
                        | "leftPos" | "topPos" => {}
                        _ => return FieldSetResult::NotOwned,
                    }
                    let Value::Int(v) = value else {
                        return FieldSetResult::Counted;
                    };
                    match name {
                        "titleLabelX" => state.title_label_x = *v,
                        "titleLabelY" => state.title_label_y = *v,
                        "inventoryLabelX" => state.inventory_label_x = *v,
                        "inventoryLabelY" => state.inventory_label_y = *v,
                        "leftPos" => state.left_pos = *v,
                        "topPos" => state.top_pos = *v,
                        _ => unreachable!(),
                    }
                    FieldSetResult::Written
                }
                // Final vanilla fields modelled in `field(...)`: guard against divergent writes.
                (SCREEN, "title", "Lnet/minecraft/network/chat/Component;")
                | (SCREEN, "minecraft", "Lnet/minecraft/client/Minecraft;")
                | (SCREEN, "font", "Lnet/minecraft/client/gui/Font;")
                | (
                    ABSTRACT_CONTAINER_SCREEN,
                    "menu",
                    "Lnet/minecraft/world/inventory/AbstractContainerMenu;",
                )
                | (
                    ABSTRACT_CONTAINER_SCREEN,
                    "playerInventoryTitle",
                    "Lnet/minecraft/network/chat/Component;",
                )
                | (ITEM_COMBINER_SCREEN, "menuResource", "Lnet/minecraft/resources/Identifier;") => {
                    FieldSetResult::Counted
                }
                _ => FieldSetResult::NotOwned,
            }
        }
        Some(Object::Slot(slot)) => {
            match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
                (SLOT, "index", "I") => {
                    if let Value::Int(v) = value {
                        slot.index = *v;
                    }
                    FieldSetResult::Written
                }
                (SLOT, "container", "Lnet/minecraft/world/Container;")
                | (SLOT, "x", "I")
                | (SLOT, "y", "I") => FieldSetResult::Counted,
                _ => FieldSetResult::NotOwned,
            }
        }
        Some(Object::Menu(_)) => match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (ABSTRACT_CONTAINER_MENU, "slots", "Lnet/minecraft/core/NonNullList;")
            | (ABSTRACT_CONTAINER_MENU, "containerId", "I")
            | (ITEM_COMBINER_MENU, "inputSlots", "Lnet/minecraft/world/Container;")
            | (
                ITEM_COMBINER_MENU,
                "resultSlots",
                "Lnet/minecraft/world/inventory/ResultContainer;",
            )
            | (
                ITEM_COMBINER_MENU,
                "access",
                "Lnet/minecraft/world/inventory/ContainerLevelAccess;",
            )
            | (ITEM_COMBINER_MENU, "player", "Lnet/minecraft/world/entity/player/Player;")
            | (ITEM_COMBINER_MENU, "resultSlotIndex", "I") => FieldSetResult::Counted,
            _ => FieldSetResult::NotOwned,
        },
        Some(Object::Inventory { .. })
            if r.owner == INVENTORY
                && r.name == "player"
                && r.descriptor == "Lnet/minecraft/world/entity/player/Player;" =>
        {
            FieldSetResult::Counted
        }
        _ => FieldSetResult::NotOwned,
    }
}

pub(crate) fn static_call(
    states: &mut States,
    next_id: &mut u64,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == CONTAINER_LEVEL_ACCESS
        && r.name == "create"
        && r.descriptor
            == "(Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/inventory/ContainerLevelAccess;"
    {
        let [level, pos] = args else {
            return None;
        };
        let key = next_key(next_id, CONTAINER_LEVEL_ACCESS_2);
        states.insert(
            key.clone(),
            Object::ContainerLevelAccess {
                level: level.clone(),
                pos: pos.clone(),
            },
        );
        return Some(Ok(marker(key)));
    }
    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition"
        && r.name == "create"
    {
        let key = next_key(
            next_id,
            "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
        );
        states.insert(
            key.clone(),
            Object::Builder {
                inputs: Vec::new(),
                result: Some(SlotDef {
                    index: 0,
                    x: 0,
                    y: 0,
                    may_place: marker("menu/predicate/empty-result"),
                    empty: true,
                }),
            },
        );
        states.insert(
            "menu/predicate/empty-result".into(),
            Object::Predicate(true),
        );
        return Some(Ok(marker(key)));
    }
    None
}

pub(crate) fn take_action(states: &mut States) -> Option<MenuAction> {
    let key = states
        .iter()
        .find_map(|(key, object)| matches!(object, Object::Pending(_)).then_some(key.clone()))?;
    match states.remove(&key) {
        Some(Object::Pending(action)) => Some(action),
        _ => None,
    }
}

pub(crate) fn keyed_text(states: &States, key: &str) -> Option<String> {
    states.get(key).map(|object| format!("{object:?}"))
}

/// Default vanilla `Container.getMaxStackSize()` in Minecraft 26.3 (`Container.DEFAULT_MAX_STACK_SIZE`).
pub(crate) const DEFAULT_CONTAINER_MAX_STACK_SIZE: i32 = 99;

/// How `SimpleContainer.setItem` should clamp the placed stack:
/// - `ContainerMax(n)` computes `min(n, stack.getMaxStackSize())` (vanilla `Container.getMaxStackSize(ItemStack)`).
/// - `Exact(limit)` uses the value returned by a mod subclass's overridden `getMaxStackSize(ItemStack)`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ContainerClamp {
    ContainerMax(i32),
    Exact(i32),
}

pub(crate) trait StackOps {
    fn is_empty(&self, value: &Value) -> Option<bool>;
    fn max_stack_size(&self, value: &Value) -> Option<i32>;
    fn clamp_for_container(&mut self, value: &Value, container_max: i32) -> Option<()>;
    fn clamp_exact(&mut self, value: &Value, limit: i32) -> Option<()>;
    fn take_pending_clamp(&mut self) -> Option<ContainerClamp> {
        None
    }
}

#[cfg(test)]
struct EmptyOnlyStacks;

#[cfg(test)]
impl StackOps for EmptyOnlyStacks {
    fn is_empty(&self, value: &Value) -> Option<bool> {
        (*value == empty_stack()).then_some(true)
    }

    fn max_stack_size(&self, value: &Value) -> Option<i32> {
        (*value == empty_stack()).then_some(1)
    }

    fn clamp_for_container(&mut self, value: &Value, _max: i32) -> Option<()> {
        (*value == empty_stack()).then_some(())
    }

    fn clamp_exact(&mut self, value: &Value, _limit: i32) -> Option<()> {
        (*value == empty_stack()).then_some(())
    }
}

fn inventory_display_name(
    states: &States,
    inventory: &Value,
    has_client_player: bool,
) -> Result<Option<Value>, NativeError> {
    if matches!(inventory, Value::Ref(None)) {
        return Err(crate::runtime::null_checks::invoke_on_null(&MethodRef {
            owner: INVENTORY.into(),
            name: "getDisplayName".into(),
            descriptor: "()Lnet/minecraft/network/chat/Component;".into(),
        }));
    }
    let known = if crate::runtime::game_object::handle_key(inventory) == Some("client/inventory") {
        has_client_player
    } else {
        expect_ref(inventory)
            .and_then(|k| states.get(k))
            .is_some_and(|o| matches!(o, Object::Inventory { .. }))
    };
    if !known {
        return Ok(None);
    }
    // 26.3 Inventory.<clinit> offsets 93-99: DEFAULT_NAME = Component.translatable("container.inventory");
    // Inventory.getName offsets 0-3 returns DEFAULT_NAME; Nameable.getDisplayName offsets 0-6 delegates to getName.
    Ok(Some(crate::runtime::host_component::inventory_default_name()))
}

fn init_container_screen(
    states: &mut States,
    key: String,
    menu: Value,
    inventory: &Value,
    has_client_player: bool,
    title: Value,
    image_width: i32,
    image_height: i32,
    menu_resource: Option<Value>,
) -> Option<Result<Value, NativeError>> {
    // 26.3 AbstractContainerScreen.<init>(AbstractContainerMenu, Inventory, Component, int, int)
    // offsets 0-87:
    //   0-2:   super(title) -> Screen.<init>(Component) offsets 0-14 sets minecraft, font, title
    //   19-21: this.menu = menu
    //   24-29: this.playerInventoryTitle = inventory.getDisplayName()
    //   32-35: this.imageWidth = imageWidth
    //   38-41: this.imageHeight = imageHeight
    //   49-52: this.titleLabelX = 8
    //   55-58: this.titleLabelY = 6
    //   61-64: this.inventoryLabelX = 8
    //   67-73: this.inventoryLabelY = imageHeight - 94
    let player_inventory_title = match inventory_display_name(states, inventory, has_client_player)
    {
        Ok(Some(title)) => title,
        Ok(None) => return None,
        Err(err) => return Some(Err(err)),
    };
    states.insert(
        key,
        Object::Screen(ScreenState {
            menu,
            title,
            minecraft: crate::runtime::game_object::client_value(crate::runtime::client_surface::MINECRAFT.into()),
            font: crate::runtime::game_object::client_value("client/font".into()),
            player_inventory_title,
            width: 0,
            height: 0,
            initialized: false,
            is_container: true,
            image_width,
            image_height,
            title_label_x: 8,
            title_label_y: 6,
            inventory_label_x: 8,
            inventory_label_y: image_height.wrapping_sub(94),
            left_pos: 0,
            top_pos: 0,
            menu_resource,
        }),
    );
    Some(Ok(Value::Void))
}

/// Add `slot_key` (already stored as `Object::Slot` in `states`) into `menu_key`
/// per 26.3 `AbstractContainerMenu.addSlot` offsets 0-62.
fn add_slot_to_menu(states: &mut States, menu_key: &str, slot_key: &str) -> Option<()> {
    let slots_list_key = match states.get(menu_key) {
        Some(Object::Menu(state)) => state.slots.clone(),
        _ => return None,
    };
    let index = match states.get(&slots_list_key) {
        Some(Object::SlotList { slots }) => slots.len() as i32,
        _ => return None,
    };
    match states.get_mut(slot_key) {
        Some(Object::Slot(slot)) => slot.index = index,
        _ => return None,
    }
    if let Some(Object::SlotList { slots }) = states.get_mut(&slots_list_key) {
        slots.push(slot_key.to_owned());
    }
    if let Some(Object::Menu(state)) = states.get_mut(menu_key) {
        state.last_slots.push(LastSlot::Empty);
        // Offsets 31-60: `remoteSlots.add(synchronizer != null ? synchronizer.createSlot()
        // : RemoteSlot.PLACEHOLDER)`.
        let remote = if state.live.synchronizer.is_some() {
            RemoteSlot::Synchronized(None)
        } else {
            RemoteSlot::Placeholder
        };
        state.live.remote_slots.push(remote);
    }
    Some(())
}

#[cfg(test)]
pub(crate) fn call(
    states: &mut States,
    next_id: &mut u64,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    call_with_stacks_and_client_player(
        states,
        next_id,
        r,
        receiver,
        args,
        &mut EmptyOnlyStacks,
        None,
    )
}

pub(crate) fn call_with_stacks_and_client_player(
    states: &mut States,
    next_id: &mut u64,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
    stacks: &mut dyn StackOps,
    client_player: Option<Value>,
) -> Option<Result<Value, NativeError>> {
    let key = expect_ref(receiver)?.to_owned();

    // 26.3 Screen constructors
    if r.owner == SCREEN && r.name == "<init>" {
        match r.descriptor.as_str() {
            // 26.3 Screen.<init>(Component) offsets 0-14: delegates to (Minecraft.getInstance(), Minecraft.getInstance().font, title)
            "(Lnet/minecraft/network/chat/Component;)V" => {
                let title = args.first()?.clone();
                states.insert(
                    key,
                    Object::Screen(ScreenState {
                        menu: Value::Ref(None),
                        title,
                        minecraft: crate::runtime::game_object::client_value(
                            crate::runtime::client_surface::MINECRAFT.into(),
                        ),
                        font: crate::runtime::game_object::client_value("client/font".into()),
                        player_inventory_title: Value::Ref(None),
                        width: 0,
                        height: 0,
                        initialized: false,
                        is_container: false,
                        image_width: 0,
                        image_height: 0,
                        title_label_x: 0,
                        title_label_y: 0,
                        inventory_label_x: 0,
                        inventory_label_y: 0,
                        left_pos: 0,
                        top_pos: 0,
                        menu_resource: None,
                    }),
                );
                return Some(Ok(Value::Void));
            }
            // 26.3 Screen.<init>(Minecraft, Font, Component) offsets 0-83 (57-69 store minecraft, font, title)
            "(Lnet/minecraft/client/Minecraft;Lnet/minecraft/client/gui/Font;Lnet/minecraft/network/chat/Component;)V" =>
            {
                let [minecraft, font, title] = args else {
                    return None;
                };
                states.insert(
                    key,
                    Object::Screen(ScreenState {
                        menu: Value::Ref(None),
                        title: title.clone(),
                        minecraft: minecraft.clone(),
                        font: font.clone(),
                        player_inventory_title: Value::Ref(None),
                        width: 0,
                        height: 0,
                        initialized: false,
                        is_container: false,
                        image_width: 0,
                        image_height: 0,
                        title_label_x: 0,
                        title_label_y: 0,
                        inventory_label_x: 0,
                        inventory_label_y: 0,
                        left_pos: 0,
                        top_pos: 0,
                        menu_resource: None,
                    }),
                );
                return Some(Ok(Value::Void));
            }
            _ => return None,
        }
    }

    // 26.3 AbstractContainerScreen constructors
    if r.owner == ABSTRACT_CONTAINER_SCREEN && r.name == "<init>" {
        match r.descriptor.as_str() {
            // 26.3 AbstractContainerScreen.<init>(AbstractContainerMenu, Inventory, Component)
            // offsets 0-13: delegates to 5-arg overload with 176, 166.
            "(Lnet/minecraft/world/inventory/AbstractContainerMenu;Lnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/network/chat/Component;)V" =>
            {
                let [menu, inventory, title] = args else {
                    return None;
                };
                return init_container_screen(
                    states,
                    key,
                    menu.clone(),
                    inventory,
                    client_player.is_some(),
                    title.clone(),
                    176,
                    166,
                    None,
                );
            }
            // 26.3 AbstractContainerScreen.<init>(AbstractContainerMenu, Inventory, Component, int, int)
            // offsets 0-87: sets imageWidth=args[3], imageHeight=args[4], inventoryLabelY=imageHeight-94.
            "(Lnet/minecraft/world/inventory/AbstractContainerMenu;Lnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/network/chat/Component;II)V" =>
            {
                let [
                    menu,
                    inventory,
                    title,
                    Value::Int(image_width),
                    Value::Int(image_height),
                ] = args
                else {
                    return None;
                };
                return init_container_screen(
                    states,
                    key,
                    menu.clone(),
                    inventory,
                    client_player.is_some(),
                    title.clone(),
                    *image_width,
                    *image_height,
                    None,
                );
            }
            _ => return None,
        }
    }

    // 26.3 ItemCombinerScreen constructor
    if r.owner == ITEM_COMBINER_SCREEN
        && r.name == "<init>"
        && r.descriptor
            == "(Lnet/minecraft/world/inventory/ItemCombinerMenu;Lnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/network/chat/Component;Lnet/minecraft/resources/Identifier;)V"
    {
        // 26.3 ItemCombinerScreen.<init> offsets 0-13:
        //   0-4:  super(menu, inventory, title) -> 3-arg AbstractContainerScreen.<init> (176, 166)
        //   7-10: this.menuResource = args[3] (kept as the real Identifier handle)
        let [menu, inventory, title, texture] = args else {
            return None;
        };
        return init_container_screen(
            states,
            key,
            menu.clone(),
            inventory,
            client_player.is_some(),
            title.clone(),
            176,
            166,
            Some(texture.clone()),
        );
    }

    // Simple Screen / AbstractContainerScreen read-only methods
    if let Some(Object::Screen(state)) = states.get(&key) {
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            // 26.3 Screen.getTitle offsets 0-4
            (
                SCREEN | ABSTRACT_CONTAINER_SCREEN | ITEM_COMBINER_SCREEN,
                "getTitle",
                "()Lnet/minecraft/network/chat/Component;",
            ) => {
                return Some(Ok(state.title.clone()));
            }
            // 26.3 AbstractContainerScreen.getMenu offsets 0-4
            (
                ABSTRACT_CONTAINER_SCREEN | ITEM_COMBINER_SCREEN,
                "getMenu",
                "()Lnet/minecraft/world/inventory/AbstractContainerMenu;",
            ) if state.is_container => {
                return Some(Ok(state.menu.clone()));
            }
            // 26.3 Screen.isInGameUi offsets 0-1 -> false; AbstractContainerScreen.isInGameUi offsets 0-1 -> true
            (SCREEN, "isInGameUi", "()Z") => {
                return Some(Ok(Value::Int(0)));
            }
            (ABSTRACT_CONTAINER_SCREEN | ITEM_COMBINER_SCREEN, "isInGameUi", "()Z")
                if state.is_container =>
            {
                return Some(Ok(Value::Int(1)));
            }
            _ => {}
        }
    }

    // 26.3 MenuType.<init> offsets 0-14, create offsets 0-11, requiredFeatures offsets 0-4
    if r.owner == MENU_TYPE {
        match (r.name.as_str(), r.descriptor.as_str()) {
            (
                "<init>",
                "(Lnet/minecraft/world/inventory/MenuType$MenuSupplier;Lnet/minecraft/world/flag/FeatureFlagSet;)V",
            ) => {
                let [constructor, required_features] = args else {
                    return None;
                };
                states.insert(
                    key,
                    Object::MenuType {
                        constructor: constructor.clone(),
                        required_features: required_features.clone(),
                    },
                );
                return Some(Ok(Value::Void));
            }
            (
                "create",
                "(ILnet/minecraft/world/entity/player/Inventory;)Lnet/minecraft/world/inventory/AbstractContainerMenu;",
            ) => {
                let Some(Object::MenuType { constructor, .. }) = states.get(&key).cloned() else {
                    return None;
                };
                let [container_id, inventory] = args else {
                    return None;
                };
                let pending = next_key(next_id, "menu/action");
                states.insert(
                    pending,
                    Object::Pending(MenuAction::MenuSupplier {
                        supplier: constructor,
                        container_id: container_id.clone(),
                        inventory: inventory.clone(),
                    }),
                );
                return Some(Ok(Value::Ref(None)));
            }
            ("requiredFeatures", "()Lnet/minecraft/world/flag/FeatureFlagSet;") => {
                let Some(Object::MenuType {
                    required_features, ..
                }) = states.get(&key)
                else {
                    return None;
                };
                return Some(Ok(required_features.clone()));
            }
            _ => return None,
        }
    }

    if r.owner == CONTAINER_LEVEL_ACCESS && key == "container-level-access/null" {
        if r.name == "execute" && r.descriptor == "(Ljava/util/function/BiConsumer;)V" {
            return Some(Ok(Value::Void));
        }
        if r.name == "evaluate"
            && r.descriptor == "(Ljava/util/function/BiFunction;)Ljava/util/Optional;"
        {
            return Some(Ok(crate::runtime::empty_optional()));
        }
        if r.name == "evaluate"
            && r.descriptor
                == "(Ljava/util/function/BiFunction;Ljava/lang/Object;)Ljava/lang/Object;"
        {
            // ContainerLevelAccess's default overload is `evaluate(f).orElse(default)`.
            // NULL's Optional is empty, so the default is the exact answer.
            return Some(Ok(args.get(1)?.clone()));
        }
        return None;
    }

    if matches!(
        r.owner.as_str(),
        CONTAINER_LEVEL_ACCESS | CONTAINER_LEVEL_ACCESS_2
    ) && let Some(Object::ContainerLevelAccess { level, pos }) = states.get(&key).cloned()
    {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("evaluate", "(Ljava/util/function/BiFunction;)Ljava/util/Optional;") => {
                let function = args.first()?.clone();
                if matches!(function, Value::Ref(None)) {
                    return Some(Err(crate::runtime::null_checks::invoke_on_null(&MethodRef {
                        owner: "java/util/function/BiFunction".into(),
                        name: "apply".into(),
                        descriptor: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"
                            .into(),
                    })));
                }
                let pending = next_key(next_id, "menu/action");
                states.insert(
                    pending,
                    Object::Pending(MenuAction::AccessEvaluate {
                        function,
                        level,
                        pos,
                        default: None,
                    }),
                );
                return Some(Ok(crate::runtime::empty_optional()));
            }
            (
                "evaluate",
                "(Ljava/util/function/BiFunction;Ljava/lang/Object;)Ljava/lang/Object;",
            ) => {
                let [function, default] = args else {
                    return None;
                };
                if matches!(function, Value::Ref(None)) {
                    return Some(Err(crate::runtime::null_checks::invoke_on_null(&MethodRef {
                        owner: "java/util/function/BiFunction".into(),
                        name: "apply".into(),
                        descriptor: "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"
                            .into(),
                    })));
                }
                let pending = next_key(next_id, "menu/action");
                states.insert(
                    pending,
                    Object::Pending(MenuAction::AccessEvaluate {
                        function: function.clone(),
                        level,
                        pos,
                        default: Some(default.clone()),
                    }),
                );
                return Some(Ok(default.clone()));
            }
            ("execute", "(Ljava/util/function/BiConsumer;)V") => {
                let consumer = args.first()?.clone();
                if matches!(consumer, Value::Ref(None)) {
                    return Some(Err(crate::runtime::null_checks::invoke_on_null(&MethodRef {
                        owner: "java/util/function/BiConsumer".into(),
                        name: "accept".into(),
                        descriptor: "(Ljava/lang/Object;Ljava/lang/Object;)V".into(),
                    })));
                }
                let pending = next_key(next_id, "menu/action");
                states.insert(
                    pending,
                    Object::Pending(MenuAction::AccessExecute {
                        consumer,
                        level,
                        pos,
                    }),
                );
                return Some(Ok(Value::Void));
            }
            _ => return None,
        }
    }

    // 26.3 SimpleMenuProvider.<init>, getDisplayName, createMenu
    if r.owner == SIMPLE_MENU_PROVIDER
        && r.name == "<init>"
        && r.descriptor
            == "(Lnet/minecraft/world/inventory/MenuConstructor;Lnet/minecraft/network/chat/Component;)V"
    {
        let [menu_constructor, title] = args else {
            return None;
        };
        states.insert(
            key,
            Object::SimpleMenuProvider {
                menu_constructor: menu_constructor.clone(),
                title: title.clone(),
            },
        );
        return Some(Ok(Value::Void));
    }
    if matches!(
        r.owner.as_str(),
        SIMPLE_MENU_PROVIDER | MENU_PROVIDER | MENU_CONSTRUCTOR
    ) && let Some(Object::SimpleMenuProvider {
        menu_constructor,
        title,
    }) = states.get(&key).cloned()
    {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("getDisplayName", "()Lnet/minecraft/network/chat/Component;") => {
                return Some(Ok(title));
            }
            (
                "createMenu",
                "(ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/entity/player/Player;)Lnet/minecraft/world/inventory/AbstractContainerMenu;",
            ) => {
                let [container_id, inventory, player] = args else {
                    return None;
                };
                if matches!(menu_constructor, Value::Ref(None)) {
                    return Some(Err(crate::runtime::null_checks::invoke_on_null(&MethodRef {
                        owner: MENU_CONSTRUCTOR.into(),
                        name: "createMenu".into(),
                        descriptor: "(ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/entity/player/Player;)Lnet/minecraft/world/inventory/AbstractContainerMenu;".into(),
                    })));
                }
                let pending = next_key(next_id, "menu/action");
                states.insert(
                    pending,
                    Object::Pending(MenuAction::MenuConstructor {
                        constructor: menu_constructor,
                        container_id: container_id.clone(),
                        inventory: inventory.clone(),
                        player: player.clone(),
                    }),
                );
                return Some(Ok(Value::Ref(None)));
            }
            _ => return None,
        }
    }

    // The builder and immutable definition are ordinary objects in vanilla.
    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition" {
        let Some(Object::Definition(definition)) = states.get(&key).cloned() else {
            return None;
        };
        return Some(match r.name.as_str() {
            "getNumOfInputSlots" => Ok(Value::Int(definition.inputs.len() as i32)),
            "getResultSlotIndex" => Ok(Value::Int(definition.result.index)),
            "getResultSlot" => {
                let out = next_key(
                    next_id,
                    "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition",
                );
                states.insert(out.clone(), Object::SlotDefinition(definition.result));
                Ok(marker(out))
            }
            "getSlot" => {
                let Some(Value::Int(index)) = args.first() else {
                    return None;
                };
                let slot = definition.inputs.get(*index as usize).cloned()?;
                let out = next_key(
                    next_id,
                    "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition",
                );
                states.insert(out.clone(), Object::SlotDefinition(slot));
                Ok(marker(out))
            }
            "getSlots" => Ok(marker(format!("menu/{key}/definition-slots"))),
            _ => return None,
        });
    }
    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition" {
        let Some(Object::SlotDefinition(slot)) = states.get(&key) else {
            return None;
        };
        return Some(match r.name.as_str() {
            "slotIndex" => Ok(Value::Int(slot.index)),
            "x" => Ok(Value::Int(slot.x)),
            "y" => Ok(Value::Int(slot.y)),
            "mayPlace" => Ok(slot.may_place.clone()),
            _ => return None,
        });
    }

    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder" {
        match r.name.as_str() {
            "<init>" => {
                states.insert(
                    "menu/predicate/empty-result".into(),
                    Object::Predicate(true),
                );
                states.insert(
                    key,
                    Object::Builder {
                        inputs: Vec::new(),
                        result: Some(SlotDef {
                            index: 0,
                            x: 0,
                            y: 0,
                            may_place: marker("menu/predicate/empty-result"),
                            empty: true,
                        }),
                    },
                );
                return Some(Ok(Value::Void));
            }
            "withSlot" => {
                let Some(Object::Builder { inputs, .. }) = states.get_mut(&key) else {
                    return None;
                };
                let [Value::Int(index), Value::Int(x), Value::Int(y), ..] = args else {
                    return None;
                };
                inputs.push(SlotDef {
                    index: *index,
                    x: *x,
                    y: *y,
                    may_place: args[3].clone(),
                    empty: false,
                });
                return Some(Ok(receiver.clone()));
            }
            "withResultSlot" => {
                let Some(Object::Builder { result, .. }) = states.get_mut(&key) else {
                    return None;
                };
                let [Value::Int(index), Value::Int(x), Value::Int(y), ..] = args else {
                    return None;
                };
                *result = Some(SlotDef {
                    index: *index,
                    x: *x,
                    y: *y,
                    may_place: marker("menu/predicate/result"),
                    empty: false,
                });
                states.insert("menu/predicate/result".into(), Object::Predicate(false));
                return Some(Ok(receiver.clone()));
            }
            "build" => {
                let Some(Object::Builder { inputs, result }) = states.get(&key).cloned() else {
                    return None;
                };
                if inputs
                    .iter()
                    .enumerate()
                    .any(|(i, slot)| slot.index != i as i32)
                {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some("Expected input slots to have continous indexes".into()),
                        frame: None,
                    }));
                }
                let result = result?;
                if result.index != inputs.len() as i32 {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some(
                            "Expected result slot index to follow last input slot".into(),
                        ),
                        frame: None,
                    }));
                }
                if inputs.is_empty() || result.empty {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some("Need to define both inputSlots and resultSlot".into()),
                        frame: None,
                    }));
                }
                let definition = Definition { inputs, result };
                let out = next_key(
                    next_id,
                    "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition",
                );
                states.insert(out.clone(), Object::Definition(definition));
                return Some(Ok(marker(out)));
            }
            _ => {}
        }
    }

    // 26.3 AbstractContainerMenu.<init>(MenuType, int) offsets 0-86
    if r.owner == ABSTRACT_CONTAINER_MENU
        && r.name == "<init>"
        && r.descriptor == "(Lnet/minecraft/world/inventory/MenuType;I)V"
    {
        let [menu_type, Value::Int(container_id)] = args else {
            return None;
        };
        let slots = next_key(next_id, "net/minecraft/core/NonNullList");
        states.insert(slots.clone(), Object::SlotList { slots: Vec::new() });
        let seq = next_menu_seq(states);
        states.insert(
            key,
            Object::Menu(MenuState {
                menu_type: menu_type.clone(),
                container_id: *container_id,
                slots,
                last_slots: Vec::new(),
                container_listeners: Vec::new(),
                combiner: None,
                seq,
                live: LiveState::default(),
            }),
        );
        return Some(Ok(Value::Void));
    }

    // 26.3 ItemCombinerMenu.<init>(MenuType, int, Inventory, ContainerLevelAccess, ItemCombinerMenuSlotDefinition)
    // offsets 0-75 (fallback when called directly in unit tests outside Runtime).
    if r.owner == ITEM_COMBINER_MENU
        && r.name == "<init>"
        && r.descriptor
            == "(Lnet/minecraft/world/inventory/MenuType;ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/inventory/ContainerLevelAccess;Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;)V"
    {
        let menu_type = args.first()?.clone();
        let container_id = match args.get(1)? {
            Value::Int(id) => *id,
            _ => return None,
        };
        let inventory = args.get(2)?.clone();
        // Offset 26: `this.player = inventory.player;` throws NPE if inventory is null.
        if matches!(inventory, Value::Ref(None)) {
            return Some(Err(crate::runtime::null_checks::read_field_on_null(&MethodRef {
                owner: INVENTORY.into(),
                name: "player".into(),
                descriptor: "Lnet/minecraft/world/entity/player/Player;".into(),
            })));
        }
        let player = if crate::runtime::game_object::handle_key(&inventory) == Some("client/inventory") {
            // Count `inventory.player` when the client frame has no player.
            client_player?
        } else {
            match expect_ref(&inventory).and_then(|inv| states.get(inv)) {
                Some(Object::Inventory { player, .. }) => player.clone(),
                _ => return None,
            }
        };
        let def_val = args.get(4)?;
        if matches!(def_val, Value::Ref(None)) {
            return Some(Err(crate::runtime::null_checks::invoke_on_null(&MethodRef {
                owner: "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition".into(),
                name: "getNumOfInputSlots".into(),
                descriptor: "()I".into(),
            })));
        }
        let def_key = expect_ref(def_val)?;
        let Some(Object::Definition(definition)) = states.get(def_key).cloned() else {
            return None;
        };
        // 26.3 ItemCombinerMenu.<init> offsets 0-75:
        //   0-3:   super(menuType, containerId)
        //   6-15:  resultSlots = new ItemCombinerMenu$1(this)
        //   18-29: access = args[3]; player = inventory.player
        //   32-42: inputSlots = createContainer(def.getNumOfInputSlots())
        //   45-51: resultSlotIndex = def.getResultSlotIndex()
        //   54-57: createInputSlots(def)
        //   60-63: createResultSlot(def)
        //   66-72: addStandardInventorySlots(inventory, 8, 84) (hotbar 58px lower at y=142)
        let slots = next_key(next_id, "net/minecraft/core/NonNullList");
        states.insert(slots.clone(), Object::SlotList { slots: Vec::new() });
        let result = next_key(next_id, "net/minecraft/world/inventory/ItemCombinerMenu$1");
        states.insert(
            result.clone(),
            Object::Container {
                size: 1,
                items: vec![empty_stack()],
                result: true,
                owner: Some(key.clone()),
                recipe_used: None,
            },
        );
        let input = next_key(next_id, "net/minecraft/world/inventory/ItemCombinerMenu$4");
        states.insert(
            input.clone(),
            Object::Container {
                size: definition.inputs.len() as i32,
                items: vec![empty_stack(); definition.inputs.len()],
                result: false,
                owner: Some(key.clone()),
                recipe_used: None,
            },
        );
        let mut layout = definition
            .inputs
            .iter()
            .map(|slot| Slot {
                index: slot.index,
                container_slot: slot.index,
                container: marker(input.clone()),
                x: slot.x,
                y: slot.y,
                may_place: Some(slot.may_place.clone()),
                kind: ScreenSlotKind::Container,
            })
            .collect::<Vec<_>>();
        layout.push(Slot {
            index: definition.inputs.len() as i32,
            container_slot: definition.result.index,
            container: marker(result.clone()),
            x: definition.result.x,
            y: definition.result.y,
            may_place: Some(definition.result.may_place.clone()),
            kind: ScreenSlotKind::Result,
        });
        for row in 0..3 {
            for column in 0..9 {
                layout.push(Slot {
                    index: layout.len() as i32,
                    container_slot: 9 * (row + 1) + column,
                    container: inventory.clone(),
                    x: 8 + 18 * column,
                    y: 84 + 18 * row,
                    may_place: None,
                    kind: ScreenSlotKind::PlayerInventory,
                });
            }
        }
        for column in 0..9 {
            layout.push(Slot {
                index: layout.len() as i32,
                container_slot: column,
                container: inventory.clone(),
                x: 8 + 18 * column,
                y: 142,
                may_place: None,
                kind: ScreenSlotKind::PlayerInventory,
            });
        }
        let slot_count = layout.len();
        let mut slot_keys = Vec::with_capacity(slot_count);
        for (position, slot) in layout.into_iter().enumerate() {
            let class = if position < definition.inputs.len() {
                "net/minecraft/world/inventory/ItemCombinerMenu$2"
            } else if position == definition.inputs.len() {
                "net/minecraft/world/inventory/ItemCombinerMenu$3"
            } else {
                SLOT
            };
            let slot_key = next_key(next_id, class);
            states.insert(slot_key.clone(), Object::Slot(slot));
            slot_keys.push(slot_key);
        }
        states.insert(slots.clone(), Object::SlotList { slots: slot_keys });
        let seq = next_menu_seq(states);
        states.insert(
            key.clone(),
            Object::Menu(MenuState {
                menu_type,
                container_id,
                slots,
                last_slots: vec![LastSlot::Empty; slot_count],
                container_listeners: Vec::new(),
                combiner: Some(CombinerState {
                    input,
                    result,
                    definition,
                    access: args.get(3)?.clone(),
                    player,
                }),
                seq,
                live: LiveState {
                    remote_slots: vec![RemoteSlot::Placeholder; slot_count],
                    ..LiveState::default()
                },
            }),
        );
        return Some(Ok(Value::Void));
    }
    if r.owner == ITEM_COMBINER_MENU && r.name == "createContainer" {
        let Some(Object::Menu(state)) = states.get(&key) else {
            return None;
        };
        let input_key = state.combiner.as_ref()?.input.clone();
        let size = args
            .first()
            .and_then(|v| {
                if let Value::Int(i) = v {
                    Some(*i)
                } else {
                    None
                }
            })
            .unwrap_or(0)
            .max(0);
        states.insert(
            input_key.clone(),
            Object::Container {
                size,
                items: vec![empty_stack(); size as usize],
                result: false,
                owner: Some(key.clone()),
                recipe_used: None,
            },
        );
        return Some(Ok(marker(input_key)));
    }
    if r.owner == INVENTORY
        && r.name == "<init>"
        && r.descriptor
            == "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/EntityEquipment;)V"
    {
        // 26.3 `Inventory.<init>(Player, EntityEquipment)` offsets 0-46: `items` is 36 `EMPTY`
        // stacks, `selected` 0. The equipment object is kept only when it is Decaf's own
        // `EntityEquipment` model, which this file does not have: slots 36-42 count.
        states.insert(
            key,
            Object::Inventory {
                player: args.first()?.clone(),
                contents: Some(InventoryContents::new(None)),
            },
        );
        return Some(Ok(Value::Void));
    }
    if r.owner == INVENTORY
        && matches!(r.name.as_str(), "getDisplayName" | "getName")
        && r.descriptor == "()Lnet/minecraft/network/chat/Component;"
        && matches!(states.get(&key), Some(Object::Inventory { .. }))
    {
        return Some(Ok(crate::runtime::host_component::inventory_default_name()));
    }
    if r.owner == SLOT
        && r.name == "getContainerSlot"
        && r.descriptor == "()I"
        && let Some(Object::Slot(slot)) = states.get(&key)
    {
        return Some(Ok(Value::Int(slot.container_slot)));
    }
    if let Some(Object::Slot(slot)) = states.get(&key)
        && (r.owner == SLOT
            || r.owner == "net/minecraft/world/inventory/ItemCombinerMenu$2"
            || r.owner == "net/minecraft/world/inventory/ItemCombinerMenu$3")
        && r.name == "mayPlace"
        && r.descriptor == "(Lnet/minecraft/world/item/ItemStack;)Z"
    {
        if let Some(predicate) = &slot.may_place {
            let pending = next_key(next_id, "menu/action");
            states.insert(
                pending,
                Object::Pending(MenuAction::Predicate {
                    predicate: predicate.clone(),
                    stack: args.first().cloned().unwrap_or(Value::Ref(None)),
                }),
            );
            return Some(Ok(Value::Int(0)));
        }
        return Some(Ok(Value::Int(1)));
    }
    if r.owner == SLOT
        && r.name == "<init>"
        && r.descriptor == "(Lnet/minecraft/world/Container;III)V"
    {
        let [container, Value::Int(index), Value::Int(x), Value::Int(y)] = args else {
            return None;
        };
        let kind = slot_kind_for_container(states, container);
        states.insert(
            key,
            Object::Slot(Slot {
                index: 0,
                container_slot: *index,
                container: container.clone(),
                x: *x,
                y: *y,
                may_place: None,
                kind,
            }),
        );
        return Some(Ok(Value::Void));
    }
    if r.owner == "net/minecraft/world/SimpleContainer" && r.name == "<init>" {
        let size = args
            .first()
            .and_then(|v| {
                if let Value::Int(i) = v {
                    Some(*i)
                } else {
                    None
                }
            })
            .unwrap_or(0)
            .max(0);
        states.insert(
            key,
            Object::Container {
                size,
                items: vec![empty_stack(); size as usize],
                result: false,
                owner: None,
                recipe_used: None,
            },
        );
        return Some(Ok(Value::Void));
    }
    if r.owner == "net/minecraft/world/inventory/ResultContainer" && r.name == "<init>" {
        states.insert(
            key,
            Object::Container {
                size: 1,
                items: vec![empty_stack()],
                result: true,
                owner: None,
                recipe_used: None,
            },
        );
        return Some(Ok(Value::Void));
    }

    // 26.3 AbstractContainerMenu.addSlot / addInventory*Slots / getType / getSlot
    // plus ItemCombinerMenu range helpers.
    if matches!(states.get(&key), Some(Object::Menu(_)))
        && matches!(
            r.owner.as_str(),
            ABSTRACT_CONTAINER_MENU | ITEM_COMBINER_MENU
        )
    {
        match (r.name.as_str(), r.descriptor.as_str()) {
            // 26.3 AbstractContainerMenu.addSlot(Slot) offsets 0-62
            (
                "addSlot",
                "(Lnet/minecraft/world/inventory/Slot;)Lnet/minecraft/world/inventory/Slot;",
            ) => {
                let slot_val = args.first()?;
                if matches!(slot_val, Value::Ref(None)) {
                    return Some(Err(crate::runtime::null_checks::assign_field_on_null(&MethodRef {
                        owner: SLOT.into(),
                        name: "index".into(),
                        descriptor: "I".into(),
                    })));
                }
                let slot_key = expect_ref(slot_val)?.clone();
                add_slot_to_menu(states, &key, &slot_key)?;
                return Some(Ok(slot_val.clone()));
            }
            // 26.3 AbstractContainerMenu.addInventoryHotbarSlots(Container, int, int) offsets 0-39
            ("addInventoryHotbarSlots", "(Lnet/minecraft/world/Container;II)V") => {
                let [container, Value::Int(x), Value::Int(y)] = args else {
                    return None;
                };
                let kind = slot_kind_for_container(states, container);
                for column in 0..9 {
                    let slot_key = next_key(next_id, SLOT);
                    states.insert(
                        slot_key.clone(),
                        Object::Slot(Slot {
                            index: 0,
                            container_slot: column,
                            container: container.clone(),
                            x: x.wrapping_add(column * 18),
                            y: *y,
                            may_place: None,
                            kind,
                        }),
                    );
                    add_slot_to_menu(states, &key, &slot_key)?;
                }
                return Some(Ok(Value::Void));
            }
            // 26.3 AbstractContainerMenu.addInventoryExtendedSlots(Container, int, int) offsets 0-68
            ("addInventoryExtendedSlots", "(Lnet/minecraft/world/Container;II)V") => {
                let [container, Value::Int(x), Value::Int(y)] = args else {
                    return None;
                };
                let kind = slot_kind_for_container(states, container);
                for row in 0..3 {
                    for column in 0..9 {
                        let slot_key = next_key(next_id, SLOT);
                        states.insert(
                            slot_key.clone(),
                            Object::Slot(Slot {
                                index: 0,
                                container_slot: column + (row + 1) * 9,
                                container: container.clone(),
                                x: x.wrapping_add(column * 18),
                                y: y.wrapping_add(row * 18),
                                may_place: None,
                                kind,
                            }),
                        );
                        add_slot_to_menu(states, &key, &slot_key)?;
                    }
                }
                return Some(Ok(Value::Void));
            }
            // 26.3 AbstractContainerMenu.addStandardInventorySlots(Container, int, int) offsets 0-24
            ("addStandardInventorySlots", "(Lnet/minecraft/world/Container;II)V") => {
                let [container, Value::Int(x), Value::Int(y)] = args else {
                    return None;
                };
                let kind = slot_kind_for_container(states, container);
                for row in 0..3 {
                    for column in 0..9 {
                        let slot_key = next_key(next_id, SLOT);
                        states.insert(
                            slot_key.clone(),
                            Object::Slot(Slot {
                                index: 0,
                                container_slot: column + (row + 1) * 9,
                                container: container.clone(),
                                x: x.wrapping_add(column * 18),
                                y: y.wrapping_add(row * 18),
                                may_place: None,
                                kind,
                            }),
                        );
                        add_slot_to_menu(states, &key, &slot_key)?;
                    }
                }
                for column in 0..9 {
                    let slot_key = next_key(next_id, SLOT);
                    states.insert(
                        slot_key.clone(),
                        Object::Slot(Slot {
                            index: 0,
                            container_slot: column,
                            container: container.clone(),
                            x: x.wrapping_add(column * 18),
                            y: y.wrapping_add(58),
                            may_place: None,
                            kind,
                        }),
                    );
                    add_slot_to_menu(states, &key, &slot_key)?;
                }
                return Some(Ok(Value::Void));
            }
            // 26.3 AbstractContainerMenu.getType offsets 0-21
            ("getType", "()Lnet/minecraft/world/inventory/MenuType;") => {
                let Some(Object::Menu(state)) = states.get(&key) else {
                    return None;
                };
                if matches!(state.menu_type, Value::Ref(None)) {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/UnsupportedOperationException".into(),
                        message: Some("Unable to construct this menu by type".into()),
                        frame: None,
                    }));
                }
                return Some(Ok(state.menu_type.clone()));
            }
            ("getSlot", "(I)Lnet/minecraft/world/inventory/Slot;") => {
                let Some(Object::Menu(state)) = states.get(&key) else {
                    return None;
                };
                let Some(Value::Int(index)) = args.first() else {
                    return None;
                };
                let Some(Object::SlotList { slots }) = states.get(&state.slots) else {
                    return None;
                };
                return Some(Ok(marker(slots.get(*index as usize)?.clone())));
            }
            _ => {}
        }
        if let Some(Object::Menu(state)) = states.get(&key)
            && let Some(combiner) = state.combiner.as_ref()
        {
            match (r.name.as_str(), r.descriptor.as_str()) {
                ("getResultSlot", "()I") => {
                    return Some(Ok(Value::Int(combiner.definition.inputs.len() as i32)));
                }
                // These private vanilla methods are also used by the host-side oracle test.
                ("getInventorySlotStart", "()I") => {
                    return Some(Ok(Value::Int(combiner.definition.inputs.len() as i32 + 1)));
                }
                ("getInventorySlotEnd" | "getUseRowStart", "()I") => {
                    return Some(Ok(Value::Int(combiner.definition.inputs.len() as i32 + 28)));
                }
                ("getUseRowEnd" | "getRowEnd", "()I") => {
                    return Some(Ok(Value::Int(combiner.definition.inputs.len() as i32 + 37)));
                }
                ("stillValid", "(Lnet/minecraft/world/entity/player/Player;)Z")
                    if expect_ref(&combiner.access)
                        .is_some_and(|key| key == "container-level-access/null") =>
                {
                    // ItemCombinerMenu passes Boolean.TRUE as the default to
                    // ContainerLevelAccess.evaluate(BiFunction, Object).
                    return Some(Ok(Value::Int(1)));
                }
                _ => {}
            }
        }
    }
    if let Some(Object::SlotList { slots }) = states.get(&key)
        && matches!(
            r.owner.as_str(),
            "java/util/List"
                | "java/util/Collection"
                | "java/util/AbstractList"
                | "java/util/AbstractCollection"
                | "net/minecraft/core/NonNullList"
        )
    {
        return Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("size", "()I") => Ok(Value::Int(slots.len() as i32)),
            ("get", "(I)Ljava/lang/Object;") => {
                let Some(Value::Int(index)) = args.first() else {
                    return None;
                };
                Ok(marker(slots.get(*index as usize)?.clone()))
            }
            _ => return None,
        });
    }

    // Containers are shared by SimpleContainer, ResultContainer and the
    // Container interface. The recipe manager remains deliberately counted.
    if let Some(Object::Container {
        size,
        items,
        result,
        owner,
        recipe_used,
    }) = states.get_mut(&key)
        && (r.owner == "net/minecraft/world/Container"
            || r.owner == "net/minecraft/world/SimpleContainer"
            || r.owner == "net/minecraft/world/inventory/ResultContainer"
            || r.owner == "net/minecraft/world/inventory/RecipeCraftingHolder")
    {
        let index = args
            .first()
            .and_then(|v| {
                if let Value::Int(i) = v {
                    Some(*i as usize)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let mut pending_action = None;
        let answer = match r.name.as_str() {
            "getContainerSize" => Ok(Value::Int(*size)),
            "getMaxStackSize" => match r.descriptor.as_str() {
                "()I" => Ok(Value::Int(DEFAULT_CONTAINER_MAX_STACK_SIZE)),
                "(Lnet/minecraft/world/item/ItemStack;)I" => {
                    let stack = args.first()?;
                    if matches!(stack, Value::Ref(None)) {
                        return None;
                    }
                    let smax = stacks.max_stack_size(stack)?;
                    Ok(Value::Int(DEFAULT_CONTAINER_MAX_STACK_SIZE.min(smax)))
                }
                _ => return None,
            },
            "isEmpty" => {
                // Only `ItemStack.EMPTY` and known live stacks have known emptiness.
                // Any unknown stack marker stays counted so we never guess.
                let mut all_empty = true;
                for v in items.iter() {
                    match stacks.is_empty(v) {
                        Some(true) => {}
                        Some(false) => {
                            all_empty = false;
                            break;
                        }
                        None => return None,
                    }
                }
                Ok(Value::Int(i32::from(all_empty)))
            }
            "getItem" => {
                let position = if *result { 0 } else { index };
                let Some(item) = items.get(position).cloned() else {
                    if !*result && position >= *size as usize {
                        return Some(Ok(empty_stack()));
                    }
                    return None;
                };
                Ok(item)
            }
            "setItem" => {
                let value = args.get(1)?;
                // A null stack: 26.3's containers store into a `NonNullList`, whose `set`
                // rejects null with a `NullPointerException` (and `SimpleContainer` calls
                // `limitSize` on it first). Counted rather than stored or answered.
                if matches!(value, Value::Ref(None)) {
                    return None;
                }
                // SimpleContainer clamps through `this.getMaxStackSize(stack)` (default 99
                // bounded by `stack.getMaxStackSize()`, or a mod container's virtual override)
                // and `ItemStack.limitSize(max)`. Refuse to mutate for an unknown stack so Java's
                // clamp cannot be silently skipped. ResultContainer has no clamp in 26.3.
                let pending_clamp = stacks.take_pending_clamp();
                if !*result {
                    match pending_clamp {
                        Some(ContainerClamp::Exact(limit)) => stacks.clamp_exact(value, limit),
                        Some(ContainerClamp::ContainerMax(max)) => {
                            stacks.clamp_for_container(value, max)
                        }
                        None => stacks.clamp_for_container(value, DEFAULT_CONTAINER_MAX_STACK_SIZE),
                    }?;
                }
                let item = items.get_mut(index)?;
                *item = value.clone();
                if !*result && let Some(menu) = owner.as_ref() {
                    pending_action = Some(MenuAction::SlotsChanged {
                        menu: marker(menu.clone()),
                        container: marker(key.clone()),
                    });
                }
                Ok(Value::Void)
            }
            "setChanged" => {
                if !*result && let Some(menu) = owner.as_ref() {
                    pending_action = Some(MenuAction::SlotsChanged {
                        menu: marker(menu.clone()),
                        container: marker(key.clone()),
                    });
                }
                Ok(Value::Void)
            }
            "clearContent" => {
                // `NonNullList.clear()` on a `withSize` list sets every element back to its
                // default (`ItemStack.EMPTY`); it does not shrink the list.
                for item in items.iter_mut() {
                    *item = empty_stack();
                }
                if !*result && let Some(menu) = owner.as_ref() {
                    pending_action = Some(MenuAction::SlotsChanged {
                        menu: marker(menu.clone()),
                        container: marker(key.clone()),
                    });
                }
                Ok(Value::Void)
            }
            "setRecipeUsed" => {
                *recipe_used = args.first().cloned();
                Ok(Value::Void)
            }
            "getRecipeUsed" => Ok(recipe_used.clone().unwrap_or(Value::Ref(None))),
            _ => {
                return None;
            }
        };
        if let Some(action) = pending_action {
            let pending = next_key(next_id, "menu/action");
            states.insert(pending, Object::Pending(action));
        }
        return Some(answer);
    }
    if let Some(Object::Predicate(answer)) = states.get(&key)
        && r.owner == "java/util/function/Predicate"
        && r.name == "test"
        && r.descriptor == "(Ljava/lang/Object;)Z"
    {
        return Some(Ok(Value::Int(i32::from(*answer))));
    }
    if let Some(Object::Menu(state)) = states.get(&key)
        && let Some(combiner) = state.combiner.as_ref()
        && r.owner == ITEM_COMBINER_MENU
        && r.name == "slotsChanged"
        && r.descriptor == "(Lnet/minecraft/world/Container;)V"
        && args
            .first()
            .and_then(expect_ref)
            .is_some_and(|input| input == &combiner.input)
    {
        let pending = next_key(next_id, "menu/action");
        states.insert(
            pending,
            Object::Pending(MenuAction::CreateResult {
                menu: marker(key.clone()),
            }),
        );
        return Some(Ok(Value::Void));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn method(owner: &str, name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    #[test]
    fn definition_drives_smithing_like_ranges() {
        let mut states = States::new();
        let mut next_id = 0;
        let builder = static_call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition",
                "create",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &[],
        )
        .unwrap()
        .unwrap();
        let receiver = builder.clone();
        for (index, x, y) in [(0, 8, 48), (1, 26, 48), (2, 44, 48)] {
            call(&mut states, &mut next_id, &method("net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder", "withSlot", "(IIILjava/util/function/Predicate;)Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;"), &receiver, &[Value::Int(index), Value::Int(x), Value::Int(y), Value::Ref(None)]).unwrap().unwrap();
        }
        call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "withResultSlot",
                "(III)Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &receiver,
            &[Value::Int(3), Value::Int(98), Value::Int(48)],
        )
        .unwrap()
        .unwrap();
        let definition = call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "build",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;",
            ),
            &receiver,
            &[],
        )
        .unwrap()
        .unwrap();
        let inventory = Value::Ref(Some("inventory/test".into()));
        call(
            &mut states,
            &mut next_id,
            &method(
                INVENTORY,
                "<init>",
                "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/EntityEquipment;)V",
            ),
            &inventory,
            &[Value::Ref(None), Value::Ref(None)],
        )
        .unwrap()
        .unwrap();
        let menu = Value::Ref(Some("menu/test".into()));
        // Null inventory throws NPE at ItemCombinerMenu.<init> offset 26 (`inventory.player`).
        let null_inv_err = call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenu",
                "<init>",
                "(Lnet/minecraft/world/inventory/MenuType;ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/inventory/ContainerLevelAccess;Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;)V",
            ),
            &menu,
            &[
                Value::Ref(None),
                Value::Int(0),
                Value::Ref(None),
                Value::Ref(Some("container-level-access/null".into())),
                definition.clone(),
            ],
        )
        .unwrap()
        .unwrap_err();
        assert!(matches!(
            null_inv_err,
            NativeError::UncaughtException { ref class, .. }
                if class == "java/lang/NullPointerException"
        ));
        // client/inventory without a client player is counted (returns None).
        assert!(
            call(
                &mut states,
                &mut next_id,
                &method(
                    "net/minecraft/world/inventory/ItemCombinerMenu",
                    "<init>",
                    "(Lnet/minecraft/world/inventory/MenuType;ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/inventory/ContainerLevelAccess;Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;)V",
                ),
                &menu,
                &[
                    Value::Ref(None),
                    Value::Int(0),
                    crate::runtime::game_object::client_value("client/inventory".into()),
                    Value::Ref(Some("container-level-access/null".into())),
                    definition.clone(),
                ],
            )
            .is_none()
        );
        call(&mut states, &mut next_id, &method("net/minecraft/world/inventory/ItemCombinerMenu", "<init>", "(Lnet/minecraft/world/inventory/MenuType;ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/inventory/ContainerLevelAccess;Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;)V"), &menu, &[Value::Ref(None), Value::Int(0), inventory, Value::Ref(Some("container-level-access/null".into())), definition]).unwrap().unwrap();
        for (name, expected) in [
            ("getResultSlot", 3),
            ("getInventorySlotStart", 4),
            ("getInventorySlotEnd", 31),
            ("getUseRowStart", 31),
            ("getUseRowEnd", 40),
            ("getRowEnd", 40),
        ] {
            assert_eq!(
                call(
                    &mut states,
                    &mut next_id,
                    &method(
                        "net/minecraft/world/inventory/AbstractContainerMenu",
                        name,
                        "()I"
                    ),
                    &menu,
                    &[]
                )
                .unwrap()
                .unwrap(),
                Value::Int(expected)
            );
        }
        assert_eq!(
            match states.get("menu/test") {
                Some(Object::Menu(state)) => match states.get(&state.slots) {
                    Some(Object::SlotList { slots }) => slots.len(),
                    _ => 0,
                },
                _ => 0,
            },
            40
        );

        let Object::Menu(state) = states.get("menu/test").unwrap() else {
            panic!("constructor did not make a menu");
        };
        let combiner = state.combiner.as_ref().unwrap();
        let Object::SlotList { slots } = states.get(&state.slots).unwrap() else {
            panic!("constructor did not make the slot list");
        };
        let mut observed = String::new();
        writeln!(observed, "inputCount={}", combiner.definition.inputs.len()).unwrap();
        for key in &slots[..combiner.definition.inputs.len()] {
            let Object::Slot(slot) = states.get(key).unwrap() else {
                panic!("input is not a slot");
            };
            writeln!(
                observed,
                "slot index={} x={} y={}",
                slot.index, slot.x, slot.y
            )
            .unwrap();
        }
        let Object::Slot(result) = states
            .get(&slots[combiner.definition.inputs.len()])
            .unwrap()
        else {
            panic!("result is not a slot");
        };
        writeln!(
            observed,
            "result index={} x={} y={}",
            result.index, result.x, result.y
        )
        .unwrap();
        for (name, _) in [
            ("resultSlot", 0),
            ("inventoryStart", 0),
            ("inventoryEnd", 0),
            ("useRowStart", 0),
            ("useRowEnd", 0),
        ] {
            let value = call(
                &mut states,
                &mut next_id,
                &method(
                    "net/minecraft/world/inventory/AbstractContainerMenu",
                    match name {
                        "resultSlot" => "getResultSlot",
                        "inventoryStart" => "getInventorySlotStart",
                        "inventoryEnd" => "getInventorySlotEnd",
                        "useRowStart" => "getUseRowStart",
                        _ => "getUseRowEnd",
                    },
                    "()I",
                ),
                &menu,
                &[],
            )
            .unwrap()
            .unwrap();
            let Value::Int(value) = value else {
                panic!("range is not an integer");
            };
            writeln!(observed, "{name}={value}").unwrap();
        }
        assert_eq!(observed, crate::runtime::vendored_menu_models_oracle_txt::VENDORED_DATA);
    }

    #[test]
    fn builder_reproduces_vanilla_validation_messages() {
        let mut states = States::new();
        let mut next_id = 0;
        let builder = static_call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition",
                "create",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &[],
        )
        .unwrap()
        .unwrap();
        let empty = call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "build",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;",
            ),
            &builder,
            &[],
        )
        .unwrap()
        .unwrap_err();
        assert!(
            matches!(empty, NativeError::UncaughtException { message: Some(message), .. } if message == "Need to define both inputSlots and resultSlot")
        );

        call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "withSlot",
                "(IIILjava/util/function/Predicate;)Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &builder,
            &[Value::Int(2), Value::Int(0), Value::Int(0), Value::Ref(None)],
        )
        .unwrap()
        .unwrap();
        call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "withResultSlot",
                "(III)Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &builder,
            &[Value::Int(1), Value::Int(0), Value::Int(0)],
        )
        .unwrap()
        .unwrap();
        let indexes = call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "build",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;",
            ),
            &builder,
            &[],
        )
        .unwrap()
        .unwrap_err();
        assert!(
            matches!(indexes, NativeError::UncaughtException { message: Some(message), .. } if message == "Expected input slots to have continous indexes")
        );
    }

    #[test]
    fn a_null_stack_is_counted_not_stored() {
        // 26.3's containers keep stacks in a `NonNullList`, whose `set` rejects null with a
        // `NullPointerException`; the model counts the call and leaves the container as it was.
        let mut states = States::new();
        let mut next_id = 0;
        for (owner, init, key) in [
            ("net/minecraft/world/SimpleContainer", "(I)V", "simple/null"),
            (
                "net/minecraft/world/inventory/ResultContainer",
                "()V",
                "result/null",
            ),
        ] {
            let container = Value::Ref(Some(key.into()));
            let args: &[Value] = if init == "(I)V" {
                &[Value::Int(1)]
            } else {
                &[]
            };
            call(
                &mut states,
                &mut next_id,
                &method(owner, "<init>", init),
                &container,
                args,
            )
            .unwrap()
            .unwrap();
            assert!(
                call(
                    &mut states,
                    &mut next_id,
                    &method(owner, "setItem", "(ILnet/minecraft/world/item/ItemStack;)V"),
                    &container,
                    &[Value::Int(0), Value::Ref(None)],
                )
                .is_none()
            );
            assert_eq!(
                call(
                    &mut states,
                    &mut next_id,
                    &method(owner, "isEmpty", "()Z"),
                    &container,
                    &[]
                )
                .unwrap()
                .unwrap(),
                Value::Int(1)
            );
        }
    }

    #[test]
    fn null_access_uses_the_evaluate_default_and_unknown_stacks_count() {
        let mut states = States::new();
        let mut next_id = 0;
        let access = Value::Ref(Some("container-level-access/null".into()));
        let fallback = Value::Int(1);
        assert_eq!(
            call(
                &mut states,
                &mut next_id,
                &method(
                    "net/minecraft/world/inventory/ContainerLevelAccess",
                    "evaluate",
                    "(Ljava/util/function/BiFunction;Ljava/lang/Object;)Ljava/lang/Object;",
                ),
                &access,
                &[Value::Ref(None), fallback.clone()],
            )
            .unwrap()
            .unwrap(),
            fallback
        );

        let simple = Value::Ref(Some("simple/test".into()));
        call(
            &mut states,
            &mut next_id,
            &method("net/minecraft/world/SimpleContainer", "<init>", "(I)V"),
            &simple,
            &[Value::Int(1)],
        )
        .unwrap()
        .unwrap();
        assert!(
            call(
                &mut states,
                &mut next_id,
                &method(
                    "net/minecraft/world/SimpleContainer",
                    "setItem",
                    "(ILnet/minecraft/world/item/ItemStack;)V",
                ),
                &simple,
                &[Value::Int(0), Value::Ref(Some("stack/unknown".into()))],
            )
            .is_none()
        );
        assert_eq!(
            call(
                &mut states,
                &mut next_id,
                &method("net/minecraft/world/SimpleContainer", "isEmpty", "()Z",),
                &simple,
                &[],
            )
            .unwrap()
            .unwrap(),
            Value::Int(1)
        );

        let result = Value::Ref(Some("result/test".into()));
        call(
            &mut states,
            &mut next_id,
            &method(
                "net/minecraft/world/inventory/ResultContainer",
                "<init>",
                "()V",
            ),
            &result,
            &[],
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            call(
                &mut states,
                &mut next_id,
                &method(
                    "net/minecraft/world/inventory/ResultContainer",
                    "setItem",
                    "(ILnet/minecraft/world/item/ItemStack;)V",
                ),
                &result,
                &[Value::Int(0), Value::Ref(Some("stack/unknown".into()))],
            )
            .unwrap()
            .unwrap(),
            Value::Void
        );
        assert!(
            call(
                &mut states,
                &mut next_id,
                &method(
                    "net/minecraft/world/inventory/ResultContainer",
                    "isEmpty",
                    "()Z",
                ),
                &result,
                &[],
            )
            .is_none()
        );
    }

    #[test]
    fn abstract_container_menu_add_slot_and_standard_inventory_slots() {
        let mut states = States::new();
        let mut next_id = 0;
        let menu_type = Value::Ref(Some("demo/MenuType#1".into()));
        let menu = Value::Ref(Some("demo/CustomMenu#2".into()));
        let box_container = Value::Ref(Some("net/minecraft/world/SimpleContainer#3".into()));
        let inv = Value::Ref(Some("net/minecraft/world/entity/player/Inventory#4".into()));
        let player = Value::Ref(Some("demo/Player#5".into()));
        states.insert(
            "net/minecraft/world/entity/player/Inventory#4".into(),
            Object::Inventory {
                player,
                contents: None,
            },
        );
        call(
            &mut states,
            &mut next_id,
            &method("net/minecraft/world/SimpleContainer", "<init>", "(I)V"),
            &box_container,
            &[Value::Int(2)],
        )
        .unwrap()
        .unwrap();
        call(
            &mut states,
            &mut next_id,
            &method(
                ABSTRACT_CONTAINER_MENU,
                "<init>",
                "(Lnet/minecraft/world/inventory/MenuType;I)V",
            ),
            &menu,
            &[menu_type.clone(), Value::Int(7)],
        )
        .unwrap()
        .unwrap();
        let slot0 = Value::Ref(Some("net/minecraft/world/inventory/Slot#0".into()));
        call(
            &mut states,
            &mut next_id,
            &method(SLOT, "<init>", "(Lnet/minecraft/world/Container;III)V"),
            &slot0,
            &[box_container, Value::Int(0), Value::Int(44), Value::Int(20)],
        )
        .unwrap()
        .unwrap();
        let added = call(
            &mut states,
            &mut next_id,
            &method(
                ABSTRACT_CONTAINER_MENU,
                "addSlot",
                "(Lnet/minecraft/world/inventory/Slot;)Lnet/minecraft/world/inventory/Slot;",
            ),
            &menu,
            std::slice::from_ref(&slot0),
        )
        .unwrap()
        .unwrap();
        assert_eq!(added, slot0);
        call(
            &mut states,
            &mut next_id,
            &method(
                ABSTRACT_CONTAINER_MENU,
                "addStandardInventorySlots",
                "(Lnet/minecraft/world/Container;II)V",
            ),
            &menu,
            &[inv.clone(), Value::Int(8), Value::Int(51)],
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            call(
                &mut states,
                &mut next_id,
                &method(
                    ABSTRACT_CONTAINER_MENU,
                    "getType",
                    "()Lnet/minecraft/world/inventory/MenuType;",
                ),
                &menu,
                &[],
            )
            .unwrap()
            .unwrap(),
            menu_type
        );
        // Opening a screen over this menu reads all 37 tagged slots (1 Container + 36 PlayerInventory).
        let screen = Value::Ref(Some("demo/CustomScreen#6".into()));
        call(
            &mut states,
            &mut next_id,
            &method(
                ABSTRACT_CONTAINER_SCREEN,
                "<init>",
                "(Lnet/minecraft/world/inventory/AbstractContainerMenu;Lnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/network/chat/Component;II)V",
            ),
            &screen,
            &[menu, inv.clone(), Value::Ref(Some("Title".into())), Value::Int(176), Value::Int(133)],
        )
        .unwrap()
        .unwrap();
        // Canonical Inventory.DEFAULT_NAME: playerInventoryTitle == inv.getDisplayName() == inv.getName().
        let screen_inv_title = field(
            &states,
            &screen,
            &method(
                ABSTRACT_CONTAINER_SCREEN,
                "playerInventoryTitle",
                "Lnet/minecraft/network/chat/Component;",
            ),
        )
        .unwrap();
        let inv_display_name = call(
            &mut states,
            &mut next_id,
            &method(
                INVENTORY,
                "getDisplayName",
                "()Lnet/minecraft/network/chat/Component;",
            ),
            &inv,
            &[],
        )
        .unwrap()
        .unwrap();
        assert_eq!(screen_inv_title, inv_display_name);
        // Guarded final vanilla field writes return FieldSetResult::Counted instead of diverging.
        assert_eq!(
            set_field(
                &mut states,
                &screen,
                &method(SCREEN, "title", "Lnet/minecraft/network/chat/Component;"),
                &Value::Ref(Some("Mutated".into())),
            ),
            FieldSetResult::Counted
        );
        assert_eq!(
            set_field(
                &mut states,
                &screen,
                &method(ABSTRACT_CONTAINER_SCREEN, "imageWidth", "I"),
                &Value::Int(999),
            ),
            FieldSetResult::Counted
        );
        assert_eq!(
            set_field(
                &mut states,
                &screen,
                &method(ABSTRACT_CONTAINER_SCREEN, "imageHeight", "I"),
                &Value::Int(999),
            ),
            FieldSetResult::Counted
        );
        let layout = screen_layout(&states, &screen).unwrap();
        assert_eq!(layout.image_width, 176);
        assert_eq!(layout.image_height, 133);
        assert_eq!(layout.inventory_label, [8, 39]);
        assert_eq!(layout.slots.len(), 37);
        assert_eq!(
            layout.slots[0],
            ScreenSlot {
                index: 0,
                x: 44,
                y: 20,
                kind: ScreenSlotKind::Container,
            }
        );
        assert_eq!(
            layout.slots[1],
            ScreenSlot {
                index: 1,
                x: 8,
                y: 51,
                kind: ScreenSlotKind::PlayerInventory,
            }
        );
        assert_eq!(
            layout.slots[36],
            ScreenSlot {
                index: 36,
                x: 8 + 8 * 18,
                y: 51 + 58,
                kind: ScreenSlotKind::PlayerInventory,
            }
        );
        // Null MenuType throws UnsupportedOperationException on getType() (26.3 offsets 0-21).
        let null_menu = Value::Ref(Some("demo/NullTypeMenu#7".into()));
        call(
            &mut states,
            &mut next_id,
            &method(
                ABSTRACT_CONTAINER_MENU,
                "<init>",
                "(Lnet/minecraft/world/inventory/MenuType;I)V",
            ),
            &null_menu,
            &[Value::Ref(None), Value::Int(0)],
        )
        .unwrap()
        .unwrap();
        let err = call(
            &mut states,
            &mut next_id,
            &method(
                ABSTRACT_CONTAINER_MENU,
                "getType",
                "()Lnet/minecraft/world/inventory/MenuType;",
            ),
            &null_menu,
            &[],
        )
        .unwrap()
        .unwrap_err();
        assert!(matches!(
            err,
            NativeError::UncaughtException { ref class, .. }
                if class == "java/lang/UnsupportedOperationException"
        ));
    }
}
