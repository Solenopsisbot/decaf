//! Item parts: a jar `Item`'s own overrides, run for an embedder's item hooks.
//!
//! An embedder (Sunrise's `ItemPart`s, `mod/src/item_parts.rs`) asks about one item at a time:
//! a click that stacks one item on another, a right-click use, the durability-style bar under a
//! stack, an item entity destroyed while it holds the stack. Vanilla answers those through
//! `Item.overrideStackedOnOther`, `overrideOtherStackedOnMe`, `use`, `isBarVisible` /
//! `getBarWidth` / `getBarColor` and `onDestroyed`. A jar item whose class chain declares one of
//! them has its own answer; this module finds which ([`NativeJar::item_overrides`]) and runs it
//! on façades over the embedder's state:
//!
//! * the **`ItemStack`** is the runtime's live stack model (`recipe_models::LiveStack`), with a
//!   jar's own data component types decoded from, and encoded back to, the NBT the game stores
//!   through the type's own persistent codec ([`NativeJar::decode_item_component`],
//!   [`NativeJar::item_stack_components`]), and a jar item's prototype values the ones its
//!   `Item.Properties.component` calls set (`jar_components.rs`, the objects themselves);
//! * the **`Slot`** is a vanilla `Slot` over the player's `Inventory` (`live_menu.rs` runs its
//!   26.3 bodies): the embedder says which inventory index the clicked slot shows and vouches that
//!   it is a plain `Slot` (no subclass, no rule a menu part changes);
//! * the **`SlotAccess`** of `overrideOtherStackedOnMe` is `AbstractContainerMenu$1` over the
//!   cursor;
//! * the **`Player`** is the server player of `block_use.rs`, with `playSound`, `drop` and
//!   `awardStat` answered here and recorded for the embedder to carry out.
//!
//! Nothing here names a mod. Every answer is what Java 25 / Minecraft 26.3 gives, or the call
//! counts (`unknown_host_call_count` of the outcome) and the embedder keeps vanilla's answer.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::runtime::block_use::{self, ActiveBlockUse, BlockUsePlayerData, BlockUseSide};
use crate::runtime::live_menu::{LiveMenuEffect, MenuPlayerFacts};
use crate::runtime::recipe_models::{self, RuntimeComponentValue, StackComponents};
use crate::runtime::{
    ExecutionBudget, Host, MethodRef, NativeError, NativeJar, Nbt, Runtime, StdHost, Value,
    game_object, jar_object, menu_models,
};

const ITEM: &str = "net/minecraft/world/item/Item";
const ITEM_STACK: &str = "net/minecraft/world/item/ItemStack";
const SLOT_ACCESS: &str = "net/minecraft/world/entity/SlotAccess";
const DATA_COMPONENT_TYPE: &str = "net/minecraft/core/component/DataComponentType";
const STATS: &str = "net/minecraft/stats/Stats";
const STAT_TYPE: &str = "net/minecraft/stats/StatType";
const BEES: &str = "net/minecraft/world/item/component/Bees";
const BEES_EMPTY: &str = "game/bees/EMPTY";
const CLICK_ACTION_PREFIX: &str = "enum/net/minecraft/world/inventory/ClickAction/";
const HAND_PREFIX: &str = "enum/net/minecraft/world/InteractionHand/";
const ACC_PRIVATE: u16 = 0x0002;

/// The vanilla data component types whose prototype value an embedder should hand
/// [`NativeJar::make_item_stack`] (as the game holds it, `None` when the item hasn't it) for a
/// jar item's stack: the ones 26.3's container code weighs a stack by
/// (`BundleContents.getWeight`: `BUNDLE_CONTENTS`, then `BEES`; `CONTAINER` for a shulker
/// box), which Decaf's own prototype model doesn't answer for a jar item, since its
/// `Item.Properties` may have set them.
pub const GAME_PROTOTYPE_COMPONENTS: &[&str] = &[
    "minecraft:bundle_contents",
    "minecraft:bees",
    "minecraft:container",
];

/// One of the `Item` methods an item part answers for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemHook {
    /// `overrideStackedOnOther(ItemStack, Slot, ClickAction, Player)`.
    StackedOnOther,
    /// `overrideOtherStackedOnMe(ItemStack, ItemStack, Slot, ClickAction, Player, SlotAccess)`.
    OtherStackedOnMe,
    /// `use(Level, Player, InteractionHand)`.
    Use,
    /// `isBarVisible(ItemStack)`.
    BarVisible,
    /// `getBarWidth(ItemStack)`.
    BarWidth,
    /// `getBarColor(ItemStack)`.
    BarColor,
    /// `onDestroyed(ItemEntity)`.
    Destroyed,
    /// `getTooltipImage(ItemStack)`.
    TooltipImage,
}

impl ItemHook {
    /// Every hook, in declaration order.
    pub const ALL: [ItemHook; 8] = [
        Self::StackedOnOther,
        Self::OtherStackedOnMe,
        Self::Use,
        Self::BarVisible,
        Self::BarWidth,
        Self::BarColor,
        Self::Destroyed,
        Self::TooltipImage,
    ];

    /// The `Item` method's name and descriptor (`server-26.3.jar`).
    pub fn method(self) -> (&'static str, &'static str) {
        match self {
            Self::StackedOnOther => (
                "overrideStackedOnOther",
                "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/inventory/Slot;Lnet/minecraft/world/inventory/ClickAction;Lnet/minecraft/world/entity/player/Player;)Z",
            ),
            Self::OtherStackedOnMe => (
                "overrideOtherStackedOnMe",
                "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/inventory/Slot;Lnet/minecraft/world/inventory/ClickAction;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/SlotAccess;)Z",
            ),
            Self::Use => (
                "use",
                "(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/InteractionResult;",
            ),
            Self::BarVisible => ("isBarVisible", "(Lnet/minecraft/world/item/ItemStack;)Z"),
            Self::BarWidth => ("getBarWidth", "(Lnet/minecraft/world/item/ItemStack;)I"),
            Self::BarColor => ("getBarColor", "(Lnet/minecraft/world/item/ItemStack;)I"),
            Self::Destroyed => (
                "onDestroyed",
                "(Lnet/minecraft/world/entity/item/ItemEntity;)V",
            ),
            Self::TooltipImage => (
                "getTooltipImage",
                "(Lnet/minecraft/world/item/ItemStack;)Ljava/util/Optional;",
            ),
        }
    }
}

/// Which hooks a jar item's class chain declares itself, each with the most derived jar class
/// that declares it (the body a virtual call selects).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemOverrides {
    pub owners: BTreeMap<ItemHook, String>,
    /// The first class of the chain the jar does not hold (`net/minecraft/world/item/Item`, or
    /// a vanilla subclass such as `BlockItem` whose own bodies the game runs), `None` when the
    /// chain ends inside the jar or at a class nobody holds.
    pub vanilla_base: Option<String>,
}

impl ItemOverrides {
    pub fn overrides(&self, hook: ItemHook) -> bool {
        self.owners.contains_key(&hook)
    }

    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }
}

/// Unmodelled host calls a call made, and why it stopped, as every outcome here reports them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemCallReport {
    pub unknown_host_calls: BTreeMap<String, usize>,
    pub unknown_host_call_count: usize,
    /// An exception Java didn't catch, a budget, a refused bytecode.
    pub failure: Option<String>,
}

impl ItemCallReport {
    /// Whether the answer is exact: no counted call and no failure.
    pub fn exact(&self) -> bool {
        self.unknown_host_call_count == 0 && self.failure.is_none()
    }
}

/// What `isBarVisible` / `getBarWidth` / `getBarColor` answered for the hooks the item
/// overrides (`None` for one it doesn't, or one not reached: width and colour are asked only
/// for a visible bar, as `GuiGraphicsExtractor.itemDecorations` asks them).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemBarOutcome {
    pub visible: Option<bool>,
    pub width: Option<i32>,
    pub color: Option<i32>,
    pub report: ItemCallReport,
}

/// Which `Item` override a pickup click runs (`AbstractContainerMenu.tryItemClickBehaviourOverride`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemClickDirection {
    /// The carried stack's `overrideStackedOnOther(carried, slot, action, player)`.
    CarriedOnOther,
    /// The clicked slot stack's `overrideOtherStackedOnMe(slotStack, carried, slot, action,
    /// player, carriedAccess)`.
    OtherStackedOnMe,
}

/// A pickup click on a plain player-inventory `Slot`, for [`NativeJar::item_click_override`].
#[derive(Clone, Debug)]
pub struct ItemClick {
    pub direction: ItemClickDirection,
    /// The player's inventory index the clicked slot shows (`Slot.getContainerSlot()`), whose
    /// stack the facts' inventory holds.
    pub inventory_slot: i32,
    /// The cursor stack (`ItemStack` value; `StdHost::empty_item_stack` when empty).
    pub carried: Value,
    /// `ClickAction.PRIMARY` (left button) or `SECONDARY`.
    pub primary: bool,
}

/// What a click override did.
#[derive(Clone, Debug)]
pub struct ItemClickOutcome {
    /// The override's `boolean`: `true` ends the click (vanilla's plain pickup doesn't run).
    pub handled: bool,
    /// The clicked slot's stack afterwards.
    pub slot: Value,
    /// The cursor afterwards (the same object, changed in place, unless the override set it).
    pub carried: Value,
    /// How many times `Slot.onTake` ran on the clicked slot (`Slot.safeTake`).
    pub takes: usize,
    /// The player's inventory afterwards.
    pub inventory: Option<Vec<Value>>,
    /// Drops, stats and the other effects recorded for the embedder, in order.
    pub effects: Vec<LiveMenuEffect>,
    pub report: ItemCallReport,
}

/// A right-click use, for [`NativeJar::item_use`].
#[derive(Clone, Debug)]
pub struct ItemUse {
    /// `"MAIN_HAND"` or `"OFF_HAND"`.
    pub hand: String,
}

/// What `Item.use` did.
#[derive(Clone, Debug)]
pub struct ItemUseOutcome {
    pub result: Option<block_use::InteractionResultValue>,
    /// The held stack afterwards (the one `getItemInHand` answered, changed in place).
    pub held: Option<Value>,
    pub inventory: Option<Vec<Value>>,
    pub effects: Vec<LiveMenuEffect>,
    pub report: ItemCallReport,
}

/// A dropped item entity destroyed while holding `stack`, for [`NativeJar::item_destroyed`].
#[derive(Clone, Debug)]
pub struct ItemDestroyed {
    pub dimension: String,
    pub stack: Value,
    /// `Entity.position()`.
    pub position: [f64; 3],
}

/// What `Item.onDestroyed` did: the stacks `ItemUtils.onContainerDestroyed` spawned, in order.
#[derive(Clone, Debug)]
pub struct ItemDestroyedOutcome {
    pub drops: Vec<Value>,
    pub report: ItemCallReport,
}

// ---------------------------------------------------------------------------
// The façades a running call answers (one frame per call, innermost last)
// ---------------------------------------------------------------------------

/// The cursor `AbstractContainerMenu$1` lends `overrideOtherStackedOnMe`: a live menu's own
/// (`getCarried` / `setCarried` on it), or a cell the item-part call owns.
#[derive(Clone, Debug)]
enum Carried {
    Menu(Value),
    Cell(Value),
}

#[derive(Clone, Debug, Default)]
struct Frame {
    /// The `SlotAccess` handle and what it reads and writes.
    access: Option<(Value, Carried)>,
    /// The destroyed `ItemEntity` handle, its stack, level and position.
    entity: Option<(Value, ItemDestroyed)>,
    /// Stacks `ItemUtils.onContainerDestroyed` spawned.
    drops: Vec<Value>,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Pops the frame when the call ends, on return or unwind.
struct FrameGuard;

impl Drop for FrameGuard {
    fn drop(&mut self) {
        FRAMES.with(|frames| frames.borrow_mut().pop());
    }
}

fn push_frame(frame: Frame) -> FrameGuard {
    FRAMES.with(|frames| frames.borrow_mut().push(frame));
    FrameGuard
}

fn with_frame<T>(f: impl FnOnce(&mut Frame) -> Option<T>) -> Option<T> {
    FRAMES.with(|frames| frames.borrow_mut().last_mut().and_then(f))
}

fn current_access(handle: &Value) -> Option<Carried> {
    with_frame(|frame| {
        frame
            .access
            .as_ref()
            .filter(|(own, _)| own == handle)
            .map(|(_, carried)| carried.clone())
    })
}

static NEXT_HANDLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn fresh_handle(family: &str) -> Value {
    let n = NEXT_HANDLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    game_object::server_value(format!("game/{family}/{n}"))
}

/// `AbstractContainerMenu.createCarriedSlotAccess()` over a live menu, for `live_menu.rs`'s
/// `tryItemClickBehaviourOverride`: the handle and the frame guard that keeps it answering.
pub(crate) struct MenuCarriedAccess {
    pub(crate) handle: Value,
    _guard: FrameGuard,
}

pub(crate) fn menu_carried_access(menu: &Value) -> MenuCarriedAccess {
    let handle = fresh_handle("carried-slot-access");
    let guard = push_frame(Frame {
        access: Some((handle.clone(), Carried::Menu(menu.clone()))),
        ..Frame::default()
    });
    MenuCarriedAccess {
        handle,
        _guard: guard,
    }
}

// ---------------------------------------------------------------------------
// Override detection and the jar component boundary
// ---------------------------------------------------------------------------

impl NativeJar {
    /// Which [`ItemHook`]s `item`'s own class chain declares (a non-static, non-private method
    /// of the same name and descriptor in a class this jar or a linked jar holds). An item a jar
    /// built as a plain `new Item(..)` declares none.
    pub fn item_overrides(&self, item: &Value, host: &StdHost) -> ItemOverrides {
        let mut out = ItemOverrides::default();
        let Some(start) = jar_object::value_marker_class(item) else {
            return out;
        };
        let mut current = start.to_owned();
        let linked = Host::linked_class_table(host).cloned().unwrap_or_default();
        for _ in 0..crate::runtime::MAX_SUPERCLASS_WALK {
            let class = self.classes.get(&current).or_else(|| {
                if Host::models_class(host, &current) {
                    None
                } else {
                    linked.get(&current)
                }
            });
            let Some(class) = class else {
                out.vanilla_base = Some(current);
                break;
            };
            for hook in ItemHook::ALL {
                if out.owners.contains_key(&hook) {
                    continue;
                }
                let (name, descriptor) = hook.method();
                if let Some(method) = class.method(name, descriptor)
                    && !method.is_static()
                    && (method.access & ACC_PRIVATE) == 0
                {
                    out.owners.insert(hook, current.clone());
                }
            }
            if class.super_name.is_empty() || class.super_name == current {
                break;
            }
            current = class.super_name.clone();
        }
        out
    }

    /// Decode `nbt`, a value of the jar data component type `component`, with the type's own
    /// persistent codec, as `NbtOps` would. `Ok(None)` (counted) when the type has no codec
    /// Decaf runs, the codec's NBT shape is unknown, or decoding failed.
    pub fn decode_item_component(
        &self,
        host: &mut StdHost,
        component: &str,
        nbt: &Nbt,
    ) -> Result<Option<Value>, NativeError> {
        let Some(type_value) = component_type_value(host, component) else {
            return Ok(None);
        };
        let mut decoded = None;
        self.with_persistent_runtime(host, std::slice::from_ref(&type_value), |rt| {
            decoded = decode_component(rt, component, &type_value, nbt)?;
            Ok(decoded.clone().unwrap_or(Value::Void))
        })?;
        Ok(decoded)
    }

    /// Encode `value` of the jar data component type `component` to the NBT its persistent
    /// codec writes with `NbtOps`. `Ok(None)` (counted) when Decaf can't.
    pub fn encode_item_component(
        &self,
        host: &mut StdHost,
        component: &str,
        value: &Value,
    ) -> Result<Option<Nbt>, NativeError> {
        let Some(type_value) = component_type_value(host, component) else {
            return Ok(None);
        };
        let mut encoded = None;
        self.with_persistent_runtime(host, &[type_value.clone(), value.clone()], |rt| {
            encoded = encode_component(rt, component, &type_value, value)?;
            Ok(Value::Void)
        })?;
        Ok(encoded)
    }

    /// A runtime `ItemStack` for a game stack: [`StdHost::make_host_item_stack_with_components`]
    /// with each [`RuntimeComponentValue::Nbt`] entry decoded by its jar type's codec (one that
    /// can't be decoded is opaque: a read of it counts). `defaults` are the item's prototype
    /// values of the jar component types (`Item.components()`, as the game holds them):
    /// they answer `get` of a component the stack's patch doesn't name.
    #[allow(clippy::too_many_arguments)]
    pub fn make_item_stack(
        &self,
        host: &mut StdHost,
        item_override: Option<Value>,
        item_id: &str,
        count: i32,
        patch: Vec<(String, Option<RuntimeComponentValue>)>,
        defaults: Vec<(String, Option<Nbt>)>,
        opaque_components: bool,
        mut opaque_keys: Vec<String>,
    ) -> Result<Value, NativeError> {
        let mut plain = Vec::with_capacity(patch.len());
        let mut decoded = Vec::new();
        for (id, entry) in patch {
            match entry {
                Some(RuntimeComponentValue::Nbt(nbt)) => {
                    match self.decode_item_component(host, &id, &nbt)? {
                        Some(value) => decoded.push((id, value)),
                        None => opaque_keys.push(id),
                    }
                }
                other => plain.push((id, other)),
            }
        }
        opaque_keys.extend(self.record_item_prototype(host, item_id, defaults)?);
        let stack = host.make_host_item_stack_with_components(
            item_override,
            item_id,
            count,
            plain,
            opaque_components,
            opaque_keys,
        );
        if let Value::Host(handle) = &stack
            && let Some(live) = host.recipes.stacks.get_mut(handle.key())
        {
            for (id, value) in decoded {
                live.patch.entries.insert(id, Some(value));
            }
        }
        Ok(stack)
    }

    /// Record the game's prototype values of `item_id` (`Item.components()`, as the game holds
    /// them: `None` when the item hasn't the component) for the components `defaults` names, as
    /// [`Self::make_item_stack`] does for its own item. An embedder also hands them for every item
    /// a stack's jar component contents name (a stored `ItemStackTemplate`'s item), which the
    /// type's codec reads while it decodes. Returns the components whose value stays unknown
    /// (a vanilla type's value, or a jar type's the runtime can't decode): a read of one counts.
    pub fn record_item_prototype(
        &self,
        host: &mut StdHost,
        item_id: &str,
        defaults: Vec<(String, Option<Nbt>)>,
    ) -> Result<Vec<String>, NativeError> {
        let mut opaque_keys = Vec::new();
        let mut prototype = BTreeMap::new();
        for (id, nbt) in defaults {
            // The item's own recorded `Item.Properties.component` value answers its prototype
            // (`recipe_models::prototype_component`): the game's copy isn't decoded over it.
            if crate::runtime::jar_components::recorded_item_default(host, item_id, &id).is_some() {
                continue;
            }
            let value = match nbt {
                None => Some(None),
                // A vanilla type's value has no codec Decaf runs here: the prototype stays
                // unknown and a read of it counts.
                Some(_) if component_type_value(host, &id).is_none() => None,
                Some(nbt) => self.decode_item_component(host, &id, &nbt)?.map(Some),
            };
            match value {
                Some(value) => {
                    prototype.insert(id, value);
                }
                // A default Decaf can't decode leaves the prototype unknown: reads count.
                None => opaque_keys.push(id),
            }
        }
        if !prototype.is_empty() {
            host.recipes.set_game_prototype(item_id, prototype);
        }
        Ok(opaque_keys)
    }

    /// [`StdHost::item_stack_components`] with each jar component value encoded to its NBT
    /// ([`RuntimeComponentValue::Nbt`]): what the game stores for the stack. A value Decaf
    /// can't encode stays [`RuntimeComponentValue::Unsupported`].
    pub fn item_stack_components(
        &self,
        host: &mut StdHost,
        stack: &Value,
    ) -> Result<Option<StackComponents>, NativeError> {
        let Some(mut out) = host.item_stack_components(stack) else {
            return Ok(None);
        };
        let Some(live) = recipe_models::read_live_stack_opt(host, stack) else {
            return Ok(Some(out));
        };
        for (id, entry) in &mut out.patch {
            if !matches!(entry, Some(RuntimeComponentValue::Unsupported)) {
                continue;
            }
            let Some(Some(value)) = live.patch.entries.get(id) else {
                continue;
            };
            if let Some(nbt) = self.encode_item_component(host, id, value)? {
                *entry = Some(RuntimeComponentValue::Nbt(nbt));
            }
        }
        Ok(Some(out))
    }
}

/// The registered `DataComponentType` object of `component`.
fn component_type_value(host: &StdHost, component: &str) -> Option<Value> {
    host.registrations()
        .iter()
        .rev()
        .find(|entry| entry.registry == "minecraft:data_component_type" && entry.id == component)
        .map(|entry| entry.value.clone())
}

/// The persistent codec a jar's `DataComponentType` was built with: the codec its builder's
/// `persistent` was given (`declarations_content.rs`'s `ContentBuilders::component_codec`).
fn component_codec<H: Host>(rt: &Runtime<'_, H>, type_value: &Value) -> Option<Value> {
    let marker = crate::runtime::expect_ref(type_value)?;
    rt.host
        .as_std_host()?
        .content_builders
        .component_codec(marker)
        .cloned()
}

/// The counted line for a jar component value Decaf can't move across.
fn component_unknown(component: &str, what: &str) -> MethodRef {
    MethodRef {
        owner: DATA_COMPONENT_TYPE.into(),
        name: format!("codec <{what} {component}>"),
        descriptor: "()Lcom/mojang/serialization/Codec;".into(),
    }
}

fn decode_component<H: Host>(
    rt: &mut Runtime<'_, H>,
    component: &str,
    type_value: &Value,
    nbt: &Nbt,
) -> Result<Option<Value>, NativeError> {
    let json = component_codec(rt, type_value).and_then(|codec| {
        let shape = crate::runtime::dfu_codec::codec_nbt_shape(&codec)?;
        Some((codec, crate::runtime::dfu_codec::codec_json_from_nbt(&shape, nbt)?))
    });
    let Some((codec, json)) = json else {
        rt.host
            .unknown_operand_call(&component_unknown(component, "decode"))?;
        return Ok(None);
    };
    match crate::runtime::dfu_codec::decode_value_json(rt, &codec, &json)? {
        crate::runtime::DfuCodecResult::Ok(value) => Ok(Some(value)),
        // `decode_value_json` counted an unmodelled codec itself; a DFU error on data the game
        // stored is a value vanilla's `NbtOps` would have refused with its own message.
        crate::runtime::DfuCodecResult::Error { .. } => {
            rt.host
                .unknown_operand_call(&component_unknown(component, "decode error"))?;
            Ok(None)
        }
        crate::runtime::DfuCodecResult::Unmodelled(_) => Ok(None),
    }
}

fn encode_component<H: Host>(
    rt: &mut Runtime<'_, H>,
    component: &str,
    type_value: &Value,
    value: &Value,
) -> Result<Option<Nbt>, NativeError> {
    let Some(codec) = component_codec(rt, type_value) else {
        rt.host
            .unknown_operand_call(&component_unknown(component, "encode"))?;
        return Ok(None);
    };
    // `codec.encodeStart(NbtOps.INSTANCE, value)` (`dfu_codec::encode_value_nbt`, #671).
    match crate::runtime::dfu_codec::encode_value_nbt(rt, &codec, value)? {
        crate::runtime::DfuCodecResult::Ok(nbt) => Ok(Some(nbt)),
        crate::runtime::DfuCodecResult::Error { .. } => {
            rt.host
                .unknown_operand_call(&component_unknown(component, "encode error"))?;
            Ok(None)
        }
        // Counted by the encode itself (`Codec#encodeStart`).
        crate::runtime::DfuCodecResult::Unmodelled(_) => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Running the hooks
// ---------------------------------------------------------------------------

/// The roots, active server player and inventory one item-part call runs with.
struct ServerCall {
    level: Value,
    player: Value,
    inventory: Value,
}

impl NativeJar {
    /// Install `player`'s server-side context (`block_use.rs`'s `ActiveBlockUse`) with an
    /// `Inventory` filled from `facts`, run `run`, and report its counted calls.
    #[allow(clippy::too_many_arguments)]
    fn run_server_item_call<T>(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        facts: &MenuPlayerFacts,
        extra_roots: &[Value],
        budget: &ExecutionBudget,
        run: impl FnOnce(&mut Runtime<'_, StdHost>, &ServerCall) -> Result<T, NativeError>,
    ) -> (Option<T>, ServerCall, Vec<LiveMenuEffect>, ItemCallReport) {
        let level = game_object::server_value(format!("game/level/{dimension}"));
        let player_value = block_use::server_player_value(player);
        let inventory = menu_models::allocate_inventory(
            &mut host.menu_states,
            &mut host.next_menu_object,
            player_value.clone(),
        );
        crate::runtime::live_menu::fill_inventory(host, &inventory, facts);
        // `Player.playSound` reads the player's position and level (`level_sound.rs`).
        if let Some(marker) = game_object::handle_key(&player_value) {
            host.level_sound.entity_facts.insert(
                marker.to_owned(),
                crate::runtime::level_sound::EntitySoundFacts {
                    level: Some(dimension.to_owned()),
                    position: facts.position,
                    silent: None,
                    sound_source: None,
                    entity_id: player.entity_id,
                    player_uuid: Some(player.uuid),
                    is_player: true,
                },
            );
        }
        let call = ServerCall {
            level: level.clone(),
            player: player_value.clone(),
            inventory: inventory.clone(),
        };
        let unknowns_before: BTreeMap<String, usize> =
            host.unknown_host_calls().into_iter().collect();
        let mark = host.call_log_mark();
        let previous = host.block_use.active_use.replace(ActiveBlockUse {
            side: BlockUseSide::Server,
            dimension: dimension.to_owned(),
            level: level.clone(),
            block: None,
            player: player_value.clone(),
            player_data: player.clone(),
            player_inventory: inventory.clone(),
            container: player.container,
            menu_effects: Vec::new(),
            facts: Some(facts.clone()),
            live_effects: Vec::new(),
            wrapped_click_cause: None,
        });
        let mut roots = vec![level, player_value, inventory];
        roots.extend_from_slice(extra_roots);
        let mut out = None;
        let result = self.with_persistent_runtime_with_budget(host, &roots, budget, |rt| {
            out = Some(run(rt, &call)?);
            Ok(Value::Void)
        });
        let finished = host.block_use.active_use.take();
        host.block_use.active_use = previous;
        host.rewind_call_log(mark);
        let (unknown_host_calls, unknown_host_call_count) =
            block_use::unknown_calls_since(host, &unknowns_before);
        let effects = finished.map(|a| a.live_effects).unwrap_or_default();
        let report = ItemCallReport {
            unknown_host_calls,
            unknown_host_call_count,
            failure: result.err().map(|error| error.to_string()),
        };
        (out, call, effects, report)
    }

    /// Run a jar item's own click override (`ItemClickDirection`) for a pickup click on a
    /// plain player-inventory `Slot`: `item` is the carried stack's item (`CarriedOnOther`) or
    /// the clicked slot stack's (`OtherStackedOnMe`), and `owner` the class
    /// [`NativeJar::item_overrides`] found declaring that override.
    ///
    /// The embedder vouches for the slot: a vanilla `Slot` (not a subclass) whose
    /// `getContainerSlot()` is `click.inventory_slot` of the player's `Inventory`, with no rule
    /// a menu part changes. `facts.inventory` must give the inventory (36 or 43 stacks).
    #[allow(clippy::too_many_arguments)]
    pub fn item_click_override(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        facts: &MenuPlayerFacts,
        item: &Value,
        owner: &str,
        click: &ItemClick,
        budget: &ExecutionBudget,
    ) -> Result<ItemClickOutcome, NativeError> {
        let Some(slot_stack) = facts
            .inventory
            .as_ref()
            .and_then(|stacks| stacks.get(usize::try_from(click.inventory_slot).ok()?))
            .cloned()
        else {
            return Err(NativeError::Metadata(
                "item click override: the facts give no stack for the clicked slot".into(),
            ));
        };
        let access = fresh_handle("carried-slot-access");
        let _guard = push_frame(Frame {
            access: Some((access.clone(), Carried::Cell(click.carried.clone()))),
            ..Frame::default()
        });
        let action = Value::Ref(Some(format!(
            "{CLICK_ACTION_PREFIX}{}",
            if click.primary {
                "PRIMARY"
            } else {
                "SECONDARY"
            }
        )));
        let roots = [
            item.clone(),
            click.carried.clone(),
            slot_stack,
            access.clone(),
        ];
        let (handled, call, effects, report) = self.run_server_item_call(
            host,
            player,
            dimension,
            facts,
            &roots,
            budget,
            |rt, call| {
                let host = rt.host.as_std_host_mut().ok_or_else(|| {
                    NativeError::Metadata("item click override needs a StdHost".into())
                })?;
                let slot = menu_models::allocate_plain_slot(
                    &mut host.menu_states,
                    &mut host.next_menu_object,
                    call.inventory.clone(),
                    click.inventory_slot,
                );
                let slot_item =
                    crate::runtime::live_menu::inventory_item(host, &call.inventory, click.inventory_slot)
                        .ok_or_else(|| {
                        NativeError::Metadata("item click override: no inventory stack".into())
                    })?;
                let (hook, args) = match click.direction {
                    ItemClickDirection::CarriedOnOther => (
                        ItemHook::StackedOnOther,
                        vec![
                            click.carried.clone(),
                            slot.clone(),
                            action.clone(),
                            call.player.clone(),
                        ],
                    ),
                    ItemClickDirection::OtherStackedOnMe => (
                        ItemHook::OtherStackedOnMe,
                        vec![
                            slot_item,
                            click.carried.clone(),
                            slot.clone(),
                            action.clone(),
                            call.player.clone(),
                            access.clone(),
                        ],
                    ),
                };
                let (name, descriptor) = hook.method();
                let value = rt.invoke(owner, name, descriptor, Some(item.clone()), &args)?;
                Ok(matches!(value, Value::Int(n) if n != 0))
            },
        );
        let carried = with_frame(|frame| match &frame.access {
            Some((_, Carried::Cell(stack))) => Some(stack.clone()),
            _ => None,
        })
        .unwrap_or_else(|| click.carried.clone());
        let slot = crate::runtime::live_menu::inventory_item(host, &call.inventory, click.inventory_slot)
            .unwrap_or_else(menu_models::empty_stack);
        let takes = effects
            .iter()
            .filter(|effect| matches!(effect, LiveMenuEffect::SlotTaken { .. }))
            .count();
        let inventory = crate::runtime::live_menu::inventory_out(host, &call.inventory, facts);
        Ok(ItemClickOutcome {
            handled: handled.unwrap_or(false),
            slot,
            carried,
            takes,
            inventory,
            effects: effects
                .into_iter()
                .filter(|effect| !matches!(effect, LiveMenuEffect::SlotTaken { .. }))
                .collect(),
            report,
        })
    }

    /// Run a jar item's own `use(level, player, hand)` on the server: the held stack is the
    /// one `player.main_hand` / `off_hand` gives for `hand`.
    #[allow(clippy::too_many_arguments)]
    pub fn item_use(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        facts: &MenuPlayerFacts,
        item: &Value,
        owner: &str,
        request: &ItemUse,
        budget: &ExecutionBudget,
    ) -> Result<ItemUseOutcome, NativeError> {
        let hand = Value::Ref(Some(format!("{HAND_PREFIX}{}", request.hand)));
        let held = match request.hand.as_str() {
            "MAIN_HAND" => player.main_hand.clone(),
            "OFF_HAND" => player.off_hand.clone(),
            _ => None,
        };
        let mut roots = vec![item.clone()];
        roots.extend(held.clone());
        let (raw, call, effects, report) = self.run_server_item_call(
            host,
            player,
            dimension,
            facts,
            &roots,
            budget,
            |rt, call| {
                let (name, descriptor) = ItemHook::Use.method();
                rt.invoke(
                    owner,
                    name,
                    descriptor,
                    Some(item.clone()),
                    &[call.level.clone(), call.player.clone(), hand.clone()],
                )
            },
        );
        let result = raw
            .as_ref()
            .and_then(|value| block_use::decode_interaction_result(host, value));
        let inventory = crate::runtime::live_menu::inventory_out(host, &call.inventory, facts);
        Ok(ItemUseOutcome {
            result,
            held,
            inventory,
            effects,
            report,
        })
    }

    /// Run the bar hooks `overrides` names on `stack` (`isBarVisible`, then for a visible bar
    /// `getBarWidth` and `getBarColor`), with no world: the item's own bodies over the stack.
    /// A hook the item doesn't override answers `None`: the embedder's vanilla value stands.
    /// `visible_default` is vanilla's `isBarVisible` for the stack, which decides whether the
    /// width and colour are asked when the item overrides only those.
    pub fn item_bar(
        &self,
        host: &mut StdHost,
        item: &Value,
        overrides: &ItemOverrides,
        stack: &Value,
        visible_default: bool,
        budget: &ExecutionBudget,
    ) -> Result<ItemBarOutcome, NativeError> {
        let unknowns_before: BTreeMap<String, usize> =
            host.unknown_host_calls().into_iter().collect();
        let mark = host.call_log_mark();
        let mut out = ItemBarOutcome::default();
        let roots = [item.clone(), stack.clone()];
        let result = self.with_persistent_runtime_with_budget(host, &roots, budget, |rt| {
            let ask = |rt: &mut Runtime<'_, StdHost>, hook: ItemHook| {
                let Some(owner) = overrides.owners.get(&hook) else {
                    return Ok(None);
                };
                let (name, descriptor) = hook.method();
                let value = rt.invoke(
                    owner,
                    name,
                    descriptor,
                    Some(item.clone()),
                    std::slice::from_ref(stack),
                )?;
                match value {
                    Value::Int(n) => Ok(Some(n)),
                    _ => Err(NativeError::Metadata(format!(
                        "{name} answered a non-int value"
                    ))),
                }
            };
            out.visible = ask(rt, ItemHook::BarVisible)?.map(|n| n != 0);
            if out.visible.unwrap_or(visible_default) {
                out.width = ask(rt, ItemHook::BarWidth)?;
                out.color = ask(rt, ItemHook::BarColor)?;
            }
            Ok(Value::Void)
        });
        host.rewind_call_log(mark);
        let (unknown_host_calls, unknown_host_call_count) =
            block_use::unknown_calls_since(host, &unknowns_before);
        out.report = ItemCallReport {
            unknown_host_calls,
            unknown_host_call_count,
            failure: result.err().map(|error| error.to_string()),
        };
        Ok(out)
    }

    /// Run a jar item's own `onDestroyed(itemEntity)` on the server for an item entity holding
    /// `request.stack`: the stacks `ItemUtils.onContainerDestroyed` spawns are the outcome's
    /// drops (`Level.addFreshEntity(new ItemEntity(level, x, y, z, stack))`).
    pub fn item_destroyed(
        &self,
        host: &mut StdHost,
        item: &Value,
        owner: &str,
        request: &ItemDestroyed,
        budget: &ExecutionBudget,
    ) -> Result<ItemDestroyedOutcome, NativeError> {
        let entity = fresh_handle("destroyed-item-entity");
        let _guard = push_frame(Frame {
            entity: Some((entity.clone(), request.clone())),
            ..Frame::default()
        });
        let unknowns_before: BTreeMap<String, usize> =
            host.unknown_host_calls().into_iter().collect();
        let mark = host.call_log_mark();
        let roots = [item.clone(), request.stack.clone(), entity.clone()];
        let result = self.with_persistent_runtime_with_budget(host, &roots, budget, |rt| {
            let (name, descriptor) = ItemHook::Destroyed.method();
            rt.invoke(
                owner,
                name,
                descriptor,
                Some(item.clone()),
                std::slice::from_ref(&entity),
            )
        });
        host.rewind_call_log(mark);
        let (unknown_host_calls, unknown_host_call_count) =
            block_use::unknown_calls_since(host, &unknowns_before);
        let drops = with_frame(|frame| Some(std::mem::take(&mut frame.drops))).unwrap_or_default();
        Ok(ItemDestroyedOutcome {
            drops,
            report: ItemCallReport {
                unknown_host_calls,
                unknown_host_call_count,
                failure: result.err().map(|error| error.to_string()),
            },
        })
    }
}

/// What 26.3 `ServerPlayer.drop(stack, retainOwnership, prediction)` does once Java has asked
/// for it ([`LiveMenuEffect::Drop`]), for an embedder that carries the drop out:
///
/// - `LivingEntity.drop` (offsets 0-70): `swing(MAIN_HAND, SwingAnimation.DEFAULT,
///   prediction != PREDICTED)`, whose last argument says whether the swing is also sent to the
///   dropping player's own client ([`PlayerDropPlan::swing_updates_self`]); then
///   `createItemStackToDrop(stack, false, retainOwnership)`, which names the player as the item
///   entity's thrower when `retainOwnership` ([`PlayerDropPlan::thrower`]), and adds it to the
///   level.
/// - `ServerPlayer.drop` offsets 9-62: when `retainOwnership` and the item entity that came of
///   it holds a non-empty stack, `awardStat(Stats.ITEM_DROPPED.get(item), stack.getCount())`
///   then `awardStat(Stats.DROP)` ([`PlayerDropPlan::stats_when_dropped`], awarded only if
///   the embedder's drop put an item entity in the world).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerDropPlan {
    pub swing_updates_self: bool,
    pub thrower: bool,
    /// `(stat type, value, amount)`, in award order.
    pub stats_when_dropped: Vec<(String, String, i32)>,
}

/// [`PlayerDropPlan`] of a recorded drop.
pub fn player_drop_plan(
    stack: &StackComponents,
    retain_ownership: bool,
    prediction: crate::runtime::live_menu::Prediction,
) -> PlayerDropPlan {
    let stats_when_dropped = if retain_ownership && stack.count > 0 {
        vec![
            (
                "minecraft:dropped".to_owned(),
                stack.item_id.clone(),
                stack.count,
            ),
            (
                "minecraft:custom".to_owned(),
                "minecraft:drop".to_owned(),
                1,
            ),
        ]
    } else {
        Vec::new()
    };
    PlayerDropPlan {
        swing_updates_self: prediction != crate::runtime::live_menu::Prediction::Predicted,
        thrower: retain_ownership,
        stats_when_dropped,
    }
}

// ---------------------------------------------------------------------------
// Interpreter entry: the façades' methods
// ---------------------------------------------------------------------------

/// `getstatic Stats.ITEM_*`: the item stat types (`Stats.<clinit>`, `makeRegistryStatType`
/// under `Identifier.withDefaultNamespace(name)`).
pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    // `Bees.EMPTY` (`<clinit>` offsets 50-60: `new Bees(List.of())`): the default a bundle-like
    // container's occupancy reads (`getOrDefault(BEES, Bees.EMPTY).bees().isEmpty()`).
    if r.owner == BEES
        && r.name == "EMPTY"
        && r.descriptor == "Lnet/minecraft/world/item/component/Bees;"
    {
        return Some(game_object::server_value(BEES_EMPTY.to_owned()));
    }
    if r.owner != STATS || r.descriptor != "Lnet/minecraft/stats/StatType;" {
        return None;
    }
    let id = match r.name.as_str() {
        "ITEM_CRAFTED" => "crafted",
        "ITEM_USED" => "used",
        "ITEM_BROKEN" => "broken",
        "ITEM_PICKED_UP" => "picked_up",
        "ITEM_DROPPED" => "dropped",
        _ => return None,
    };
    Some(game_object::server_value(format!(
        "game/stat-type/minecraft:{id}"
    )))
}

/// Entry from `Runtime::invoke_game_helpers`: the `SlotAccess`, `ItemEntity`, stat and
/// server-player calls of a running item-part call, and `ItemStack.set` of a component whose
/// prototype value only Java's `equals` can compare.
#[inline(never)]
pub(crate) fn invoke_helper<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(receiver) = receiver else {
        return static_helper(rt, r, args);
    };
    if r.owner == ITEM_STACK || r.owner == "net/minecraft/core/component/DataComponentHolder" {
        return stack_set(rt, r, receiver, args);
    }
    // 26.3 `Item.canFitInsideContainerItems()` is `iconst_1; ireturn`: a jar item whose class
    // chain leaves the jar at `Item` itself (a jar override already ran before this) answers
    // `true`. `BlockItem` and `BundleItem` have bodies of their own, left to their models.
    if r.name == "canFitInsideContainerItems" && r.descriptor == "()Z" {
        let plain_item = match jar_object::object_class(receiver) {
            Some(class) if rt.class(class).is_some() => {
                rt.vanilla_superclass(class).as_deref() == Some(ITEM)
            }
            // A jar's `new Item(properties)`: the host's own `Item` object.
            Some(ITEM) => true,
            _ => false,
        };
        if plain_item {
            return Ok(Some(Value::Int(1)));
        }
        // A vanilla item's handle (`ItemStack.getItem()` of a vanilla stack): 26.3's own answer,
        // `Item`'s `true` or `BlockItem`'s `!(getBlock() instanceof ShulkerBoxBlock)`, as Java 25
        // gives it for every registered item (`vanilla_item_data`). An id the game doesn't
        // register falls through, counted.
        let vanilla = game_object::handle_key(receiver)
            .and_then(|key| {
                key.strip_prefix("game/item/")
                    .or_else(|| key.strip_prefix("client/item/"))
            })
            .and_then(crate::runtime::vanilla_item_data::vanilla_can_fit_inside_container_items);
        if let Some(fits) = vanilla {
            return Ok(Some(Value::Int(i32::from(fits))));
        }
    }
    let Some(key) = game_object::handle_key(receiver) else {
        return Ok(None);
    };
    if key.starts_with("game/carried-slot-access/") {
        let Some(carried) = current_access(receiver) else {
            return Ok(None);
        };
        return slot_access_call(rt, r, carried, args);
    }
    if key == BEES_EMPTY {
        // `Bees.bees()` (the record accessor) of `EMPTY`: its `List.of()`.
        if r.name == "bees" && r.descriptor == "()Ljava/util/List;" {
            return rt
                .invoke("java/util/List", "of", "()Ljava/util/List;", None, &[])
                .map(Some);
        }
        return rt.host.unknown_operand_call(r).map(Some);
    }
    if key.starts_with("game/destroyed-item-entity/") {
        return item_entity_call(rt, r, receiver, args);
    }
    if let Some(stat_type) = key.strip_prefix("game/stat-type/") {
        if r.owner == STAT_TYPE
            && r.name == "get"
            && r.descriptor == "(Ljava/lang/Object;)Lnet/minecraft/stats/Stat;"
        {
            let Some(arg) = args.first() else {
                return Ok(None);
            };
            let Some(item) = rt
                .host
                .as_std_host()
                .and_then(|host| recipe_models::resolve_item_id(host, arg))
            else {
                return Ok(None);
            };
            return Ok(Some(game_object::server_value(format!(
                "game/stat/{stat_type}/{item}"
            ))));
        }
        return Ok(None);
    }
    if key.starts_with("game/player/") {
        return active_player_call(rt, r, receiver, args);
    }
    Ok(None)
}

fn static_helper<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    // 26.3 `ItemUtils.onContainerDestroyed(ItemEntity, Stream)` offsets 0-26 with its lambda:
    // on a server level, each stack becomes `new ItemEntity(level, x, y, z, stack)` added to
    // the level, which the embedder spawns.
    if r.owner == "net/minecraft/world/item/ItemUtils"
        && r.name == "onContainerDestroyed"
        && r.descriptor
            == "(Lnet/minecraft/world/entity/item/ItemEntity;Ljava/util/stream/Stream;)V"
    {
        let (Some(entity), Some(stream)) = (args.first(), args.get(1)) else {
            return Ok(None);
        };
        let ours = with_frame(|frame| {
            frame
                .entity
                .as_ref()
                .filter(|(own, _)| own == entity)
                .map(|_| ())
        });
        if ours.is_none() {
            return Ok(None);
        }
        let before = rt.host.counted_unknown_calls();
        let stacks = rt.call_virtual(
            crate::runtime::jdk_call::Site::Interface,
            "java/util/stream/Stream",
            "toList",
            "()Ljava/util/List;",
            stream,
            &[],
        )?;
        let size = rt.call_virtual(
            crate::runtime::jdk_call::Site::Interface,
            "java/util/List",
            "size",
            "()I",
            &stacks,
            &[],
        )?;
        let Value::Int(size) = size else {
            return rt.host.unknown_operand_call(r).map(Some);
        };
        let mut drops = Vec::new();
        for index in 0..size {
            drops.push(rt.call_virtual(
                crate::runtime::jdk_call::Site::Interface,
                "java/util/List",
                "get",
                "(I)Ljava/lang/Object;",
                &stacks,
                &[Value::Int(index)],
            )?);
        }
        if rt.host.counted_unknown_calls() != before {
            return rt.host.unknown_operand_call(r).map(Some);
        }
        with_frame(|frame| {
            frame.drops.extend(drops);
            Some(())
        });
        return Ok(Some(Value::Void));
    }
    Ok(None)
}

/// `AbstractContainerMenu$1`: `get()` is `getCarried()`, `set(stack)` is `setCarried(stack)`
/// then `true`.
fn slot_access_call<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    carried: Carried,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    match (r.name.as_str(), r.descriptor.as_str(), carried) {
        ("get", "()Lnet/minecraft/world/item/ItemStack;", Carried::Cell(stack)) => Ok(Some(stack)),
        ("set", "(Lnet/minecraft/world/item/ItemStack;)Z", Carried::Cell(_)) => {
            let stack = args.first().cloned().unwrap_or(Value::Ref(None));
            with_frame(|frame| {
                if let Some((_, Carried::Cell(cell))) = frame.access.as_mut() {
                    *cell = stack;
                }
                Some(())
            });
            Ok(Some(Value::Int(1)))
        }
        ("get", "()Lnet/minecraft/world/item/ItemStack;", Carried::Menu(menu)) => rt
            .call_virtual(
                crate::runtime::jdk_call::Site::Virtual,
                menu_models::ABSTRACT_CONTAINER_MENU,
                "getCarried",
                "()Lnet/minecraft/world/item/ItemStack;",
                &menu,
                &[],
            )
            .map(Some),
        ("set", "(Lnet/minecraft/world/item/ItemStack;)Z", Carried::Menu(menu)) => {
            rt.call_virtual(
                crate::runtime::jdk_call::Site::Virtual,
                menu_models::ABSTRACT_CONTAINER_MENU,
                "setCarried",
                "(Lnet/minecraft/world/item/ItemStack;)V",
                &menu,
                args,
            )?;
            Ok(Some(Value::Int(1)))
        }
        _ if r.owner == SLOT_ACCESS || r.owner == "java/lang/Object" => {
            rt.host.unknown_operand_call(r).map(Some)
        }
        _ => Ok(None),
    }
}

/// The destroyed `ItemEntity`: `getItem()` (its stack), `level()` (its server level), its
/// position, and `isClientSide()` of that level through the level handle.
fn item_entity_call<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: &Value,
    _args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(request) = with_frame(|frame| {
        frame
            .entity
            .as_ref()
            .filter(|(own, _)| own == receiver)
            .map(|(_, request)| request.clone())
    }) else {
        return Ok(None);
    };
    Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
        ("getItem", "()Lnet/minecraft/world/item/ItemStack;") => request.stack,
        ("level", "()Lnet/minecraft/world/level/Level;") => {
            game_object::server_value(format!("game/level/{}", request.dimension))
        }
        ("getX", "()D") => Value::Float(request.position[0]),
        ("getY", "()D") => Value::Float(request.position[1]),
        ("getZ", "()D") => Value::Float(request.position[2]),
        _ => return rt.host.unknown_operand_call(r).map(Some),
    }))
}

/// The running call's server player: `playSound` (`level_sound.rs`), `drop` and `awardStat`
/// (recorded as [`LiveMenuEffect`]s). Anything else is left to `block_use.rs`.
fn active_player_call<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(host) = rt.host.as_std_host_mut() else {
        return Ok(None);
    };
    let Some(active) = host.block_use.active_use.as_ref() else {
        return Ok(None);
    };
    if active.player != *receiver || active.side != BlockUseSide::Server {
        return Ok(None);
    }
    let marker = game_object::handle_key(receiver)
        .unwrap_or_default()
        .to_owned();
    match (r.name.as_str(), r.descriptor.as_str()) {
        // `Player.playSound(SoundEvent, float, float)` (no `isSilent` check) and
        // `Entity.playSound(SoundEvent)`: the level sound path, while the embedder defers server
        // sound seeds; counted otherwise (nothing would play it).
        ("playSound", _) | ("getSoundSource", _) | ("isSilent", _) => {
            if !host.level_sound.host_deferred_server_seeds {
                return host.unknown_host_call(r).map(Some);
            }
            match crate::runtime::level_sound::server_facade_sound_call(host, &marker, r, args) {
                Some(result) => result.map(Some),
                None => host.unknown_host_call(r).map(Some),
            }
        }
        // `Player.drop(ItemStack, boolean, Prediction)` on the server player
        // (`ServerPlayer.drop` over `LivingEntity.drop`: an empty stack drops nothing and
        // answers `null`). The `ItemEntity` it answers is a handle whose every call counts.
        (
            "drop",
            "(Lnet/minecraft/world/item/ItemStack;ZLnet/minecraft/util/Prediction;)Lnet/minecraft/world/entity/item/ItemEntity;",
        ) => {
            let (Some(stack), Some(Value::Int(retain)), Some(prediction)) =
                (args.first(), args.get(1), args.get(2))
            else {
                return host.unknown_host_call(r).map(Some);
            };
            let retain = *retain != 0;
            let Some(prediction) = crate::runtime::live_menu::Prediction::from_value(prediction) else {
                return host.unknown_host_call(r).map(Some);
            };
            let Some(mut components) = host.item_stack_components(stack) else {
                return host.unknown_host_call(r).map(Some);
            };
            if components.count <= 0 || components.item_id == "minecraft:air" {
                return Ok(Some(Value::Ref(None)));
            }
            // A jar component on the dropped stack crosses as its codec's NBT.
            encode_jar_entries(rt, stack, &mut components)?;
            let Some(host) = rt.host.as_std_host_mut() else {
                return Ok(None);
            };
            crate::runtime::live_menu::record_effect(
                host,
                LiveMenuEffect::Drop {
                    stack: components,
                    retain_ownership: retain,
                    prediction,
                },
            );
            Ok(Some(fresh_handle("dropped-item-entity")))
        }
        // `Player.awardStat(Stat)` is `awardStat(stat, 1)`; `ServerPlayer.awardStat(Stat, int)`
        // increments the stat (the embedder's), and the scoreboard criteria that follow it.
        ("awardStat", "(Lnet/minecraft/stats/Stat;)V")
        | ("awardStat", "(Lnet/minecraft/stats/Stat;I)V") => {
            let amount = match args.get(1) {
                Some(Value::Int(n)) => *n,
                Some(_) => return host.unknown_host_call(r).map(Some),
                None => 1,
            };
            let Some((stat_type, value)) = args
                .first()
                .and_then(game_object::handle_key)
                .and_then(|key| key.strip_prefix("game/stat/"))
                .and_then(|rest| {
                    // `<namespace:type>/<namespace:item>`: the type id has no `/`.
                    let (stat_type, value) = rest.split_once('/')?;
                    Some((stat_type.to_owned(), value.to_owned()))
                })
            else {
                return host.unknown_host_call(r).map(Some);
            };
            crate::runtime::live_menu::record_effect(
                host,
                LiveMenuEffect::AwardStat {
                    stat_type,
                    value,
                    amount,
                },
            );
            Ok(Some(Value::Void))
        }
        _ => Ok(None),
    }
}

/// Encode each jar component value of `stack` that `components` could only call
/// [`RuntimeComponentValue::Unsupported`] to its codec's NBT, in place.
fn encode_jar_entries<H: Host>(
    rt: &mut Runtime<'_, H>,
    stack: &Value,
    components: &mut StackComponents,
) -> Result<(), NativeError> {
    let Some(live) = rt
        .host
        .as_std_host()
        .and_then(|host| recipe_models::read_live_stack_opt(host, stack))
    else {
        return Ok(());
    };
    for (id, entry) in &mut components.patch {
        if !matches!(entry, Some(RuntimeComponentValue::Unsupported)) {
            continue;
        }
        let (Some(Some(value)), Some(type_value)) = (
            live.patch.entries.get(id),
            rt.host
                .as_std_host()
                .and_then(|host| component_type_value(host, id)),
        ) else {
            continue;
        };
        if let Some(nbt) = encode_component(rt, id, &type_value, value)? {
            *entry = Some(RuntimeComponentValue::Nbt(nbt));
        }
    }
    Ok(())
}

/// `ItemStack.set(type, value)` of a component whose new or prototype value is a jar object:
/// 26.3 `PatchedDataComponentMap.set` compares it with the prototype by `Objects.equals`, which
/// only the interpreter can run on a jar record. Other calls are left to `recipe_models.rs`.
fn stack_set<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    if r.name != "set"
        || r.descriptor
            != "(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Ljava/lang/Object;"
    {
        return Ok(None);
    }
    let (Some(type_arg), Some(new_value)) = (args.first(), args.get(1)) else {
        return Ok(None);
    };
    let Some(host) = rt.host.as_std_host() else {
        return Ok(None);
    };
    let Some(live) = recipe_models::read_live_stack_opt(host, receiver) else {
        return Ok(None);
    };
    let Some(comp_id) = recipe_models::resolve_data_component_type_id(host, type_arg) else {
        return Ok(None);
    };
    if live.is_key_opaque(&comp_id) {
        return Ok(None);
    }
    let Some(item_id) = live.resolved_item_id(host) else {
        return Ok(None);
    };
    let Some(proto) = recipe_models::prototype_component(host, &item_id, &comp_id) else {
        return Ok(None);
    };
    let is_jar = |value: &Value| jar_object::object_key(value).is_some();
    if !is_jar(new_value) && !proto.as_ref().is_some_and(is_jar) {
        return Ok(None);
    }
    let Value::Host(handle) = receiver else {
        return Ok(None);
    };
    let stack_key = handle.key().to_owned();
    // 26.3 `PatchedDataComponentMap.set` / `setTyped`: a value equal to the prototype's
    // removes the patch entry, any other value (null: a removal) is patched in.
    let new_opt = (!matches!(new_value, Value::Ref(None))).then(|| new_value.clone());
    let matches_proto = match (&new_opt, &proto) {
        (None, None) => true,
        (None, Some(_)) | (Some(_), None) => false,
        (Some(a), Some(b)) => {
            let before = rt.host.counted_unknown_calls();
            let equal = rt.objects_equals(a, b)?;
            if rt.host.counted_unknown_calls() != before {
                return rt.host.unknown_operand_call(r).map(Some);
            }
            equal
        }
    };
    let Some(host) = rt.host.as_std_host_mut() else {
        return Ok(None);
    };
    let Some(slot) = host.recipes.stacks.get_mut(&stack_key) else {
        return Ok(None);
    };
    let old = if matches_proto {
        slot.patch.entries.remove(&comp_id)
    } else {
        slot.patch.entries.insert(comp_id, new_opt)
    };
    Ok(Some(match old {
        Some(Some(previous)) => previous,
        Some(None) | None => proto.unwrap_or(Value::Ref(None)),
    }))
}

/// `ItemStack.isItemEnabled(level.enabledFeatures())` for a pickup click's stack: an empty
/// stack is enabled; an item whose `requiredFeatures` is within `FeatureFlags.VANILLA_SET` is,
/// since every world enables the vanilla flag (the `vanilla` pack is required). `None` when
/// the item's features are unknown or need another flag the level may not enable.
pub(crate) fn item_enabled_in_any_level(host: &StdHost, stack: &Value) -> Option<bool> {
    let (item, _, empty) = host.read_item_stack_impl(stack)?;
    if empty {
        return Some(true);
    }
    let mask = host.item_required_features_mask_impl(&item)?;
    ((mask & !crate::runtime::host_creative_tab::FEATURE_VANILLA_SET) == 0).then_some(true)
}

/// The item [`ItemHook`] `stack`'s item would run for `hook`, when a jar class declares it.
pub(crate) fn jar_override_owner<H: Host>(
    rt: &mut Runtime<'_, H>,
    item: &Value,
    hook: ItemHook,
) -> Result<Option<String>, NativeError> {
    let (name, descriptor) = hook.method();
    let r = MethodRef {
        owner: ITEM.into(),
        name: name.into(),
        descriptor: descriptor.into(),
    };
    rt.jvm_selected_jar_body(&r, 0xb6, Some(item))
}
