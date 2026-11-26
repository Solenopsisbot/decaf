//! Live Java menus: Minecraft 26.3's `AbstractContainerMenu` / `ItemCombinerMenu` / `Slot` /
//! `SimpleContainer` / `ResultContainer` / `Inventory` bodies, run on the menu a server player
//! has open, so a mod's menu reacts to clicks, drags, shift-clicks and closes exactly as it does
//! on a real server.
//!
//! # What this is
//!
//! `block_use.rs` opens a mod's menu (`ServerPlayer.openMenu` builds the mod's
//! `AbstractContainerMenu` subclass and Decaf holds it as the player's `containerMenu`). Nothing
//! ran that menu afterwards. This module does: every vanilla method the menu inherits, and every
//! vanilla method those methods call on its slots, containers and the player's inventory, is
//! transcribed here from the `server-26.3.jar` bytecode, offset by offset (the comments name
//! them). A mod's own overrides are never modelled: each virtual call a vanilla body makes goes
//! through [`vcall`], which runs the receiver's jar override first, exactly where the JVM would.
//!
//! Nothing here names a mod. Runes' altar menu (an `ItemCombinerMenu` with two inputs, a
//! `createResult` that asks the `RecipeManager`, and an `onTake` that shrinks the inputs) is one
//! user; any combiner, crafting-bench-like or plain `AbstractContainerMenu` subclass is another.
//!
//! # Exact or counted
//!
//! A body answers only what Java 25 / Minecraft 26.3 would answer. Whenever a step needs a fact
//! Decaf does not have (an opaque `ItemStack`, a player inventory the embedder did not describe,
//! a `ContainerLevelAccess` a jar implements, a bundle's own click handling, the order of a
//! `HashSet` iteration Java leaves to identity hashes) the step stops with [`Flow::Counted`]: the
//! outermost modelled method on the Rust call chain is counted as an unknown host call and
//! returns the neutral placeholder, and no later step of that body runs on a guess. A jar call a
//! body makes that itself counted is treated the same way (the counted-call count grew across
//! it), so a placeholder never flows into a vanilla decision (`counted-placeholder-trap`).
//!
//! # The player
//!
//! The player's real inventory, cursor and abilities live in the embedder (Sunrise). The embedder
//! hands them in for each call as [`MenuPlayerFacts`]; Decaf materializes them into the
//! `Inventory` object the menu's player slots point at, runs the call, and hands the resulting
//! inventory back ([`LiveMenuOutcome::inventory`]). A stack the embedder passes back unchanged
//! keeps its Java identity (the same object), so a menu that holds a reference keeps it.
//!
//! # Synchronization
//!
//! Vanilla's `ServerPlayer.initMenu` gives each opened menu the player's container listener
//! (`ServerPlayer$2`) and synchronizer (`ServerPlayer$1`). Decaf models both. The listener's one
//! effect (`CriteriaTriggers.INVENTORY_CHANGED` for player-inventory slots) and every packet the
//! synchronizer would send are recorded, in order, as [`LiveMenuEffect`]s: the typed stream a
//! `mod/` bridge maps onto Sunrise's menus (owned / blockless processing menus, upstream
//! #4007/#4008). The synchronizer's remote slots only ever `force` (Decaf takes no client
//! `HashedStack` predictions), so its sends are the server-side changes, which is what an
//! embedder that owns its own network sync needs; the `stateId` it bumps is then no longer the
//! client's, and a jar read of it counts ([`menu_models::LiveState::state_id_known`]).

use std::collections::BTreeMap;

use crate::runtime::{
    Host, MethodRef, NativeError, NativeJar, Runtime, StdHost, Value,
    block_use::{self, ActiveBlockUse, BlockUsePlayerData, BlockUseSide, PlayerContainer},
    expect_ref, game_object, jar_object,
    jdk_call::Site,
    menu_models, null_checks, recipe_models,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// 26.3 `net.minecraft.world.inventory.ContainerInput` (the click type `ClickType` used to be),
/// in declaration order (`id()` is the ordinal).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContainerInput {
    Pickup,
    QuickMove,
    Swap,
    Clone,
    Throw,
    QuickCraft,
    PickupAll,
}

impl ContainerInput {
    /// The constant's Java name.
    pub fn java_name(self) -> &'static str {
        match self {
            Self::Pickup => "PICKUP",
            Self::QuickMove => "QUICK_MOVE",
            Self::Swap => "SWAP",
            Self::Clone => "CLONE",
            Self::Throw => "THROW",
            Self::QuickCraft => "QUICK_CRAFT",
            Self::PickupAll => "PICKUP_ALL",
        }
    }

    fn from_java_name(name: &str) -> Option<Self> {
        Some(match name {
            "PICKUP" => Self::Pickup,
            "QUICK_MOVE" => Self::QuickMove,
            "SWAP" => Self::Swap,
            "CLONE" => Self::Clone,
            "THROW" => Self::Throw,
            "QUICK_CRAFT" => Self::QuickCraft,
            "PICKUP_ALL" => Self::PickupAll,
            _ => return None,
        })
    }

    /// The `getstatic ContainerInput.<NAME>` value a jar sees.
    pub fn to_value(self) -> Value {
        Value::Ref(Some(format!(
            "{CONTAINER_INPUT_PREFIX}{}",
            self.java_name()
        )))
    }

    fn from_value(value: &Value) -> Option<Self> {
        Self::from_java_name(expect_ref(value)?.strip_prefix(CONTAINER_INPUT_PREFIX)?)
    }
}

/// 26.3 `net.minecraft.util.Prediction`, the last argument of `Player.drop` and
/// `Inventory.placeItemBackInInventory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prediction {
    Predicted,
    ServerOnly,
}

impl Prediction {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        match expect_ref(value)?.strip_prefix(PREDICTION_PREFIX)? {
            "PREDICTED" => Some(Self::Predicted),
            "SERVER_ONLY" => Some(Self::ServerOnly),
            _ => None,
        }
    }

    fn to_value(self) -> Value {
        let name = match self {
            Self::Predicted => "PREDICTED",
            Self::ServerOnly => "SERVER_ONLY",
        };
        Value::Ref(Some(format!("{PREDICTION_PREFIX}{name}")))
    }
}

/// What the embedder knows of a server player for one live-menu call. Every field the call
/// does not need may stay unknown; a step that needs an unknown one counts.
#[derive(Clone, Debug, Default)]
pub struct MenuPlayerFacts {
    /// The player's `Inventory`, by container slot: `items` 0-35, then `EQUIPMENT_SLOT_MAPPING`'s
    /// 36 FEET, 37 LEGS, 38 CHEST, 39 HEAD, 40 OFFHAND, 41 BODY, 42 SADDLE, as `ItemStack`
    /// values ([`StdHost::make_host_item_stack_with_components`], [`StdHost::empty_item_stack`]).
    /// 36 entries leave the equipment unknown. `None`: the inventory is unknown.
    pub inventory: Option<Vec<Value>>,
    /// `Inventory.selected` (the hotbar slot, 0-8).
    pub selected_slot: i32,
    /// `Abilities.instabuild` (`Player.hasInfiniteMaterials()`, creative mode).
    pub instabuild: Option<bool>,
    /// `Entity.getEyePosition()`, for `Player.isWithinBlockInteractionRange`.
    pub eye_position: Option<[f64; 3]>,
    /// `Entity.position()`: what the player's `getX()`, `getY()` and `getZ()` answer.
    pub position: Option<[f64; 3]>,
    /// `Player.blockInteractionRange()` (the `minecraft:block_interaction_range` attribute).
    pub block_interaction_range: Option<f64>,
    /// The block at the menu's `ContainerLevelAccess` position (`Level.getBlockState`), as a
    /// registered block id and its state properties.
    pub access_block: Option<(String, BTreeMap<String, String>)>,
    /// `Entity.isRemoved()` and whether its `getRemovalReason()` is `CHANGED_DIMENSION`.
    pub removed: bool,
    pub removed_for_dimension_change: bool,
    /// `ServerPlayer.hasDisconnected()`.
    pub disconnected: bool,
    /// `LivingEntity.isDeadOrDying()`.
    pub dead: bool,
    /// The player's `Entity.tickCount`. A jar's `getfield tickCount` on the live player (the
    /// field `Entity` declares, read through any class of the player's hierarchy) answers it;
    /// `None` leaves that read counted.
    pub tick_count: Option<i32>,
    /// The held menu's cursor as the embedder shows it (`AbstractContainerMenu.carried`), when
    /// the embedder moved it itself: e.g. a window close that already put the cursor stack back
    /// into the inventory before the menu's `removed` runs. The call then starts from this
    /// stack (an unchanged stack keeps its Java object). `None` keeps the menu's own.
    pub carried: Option<Value>,
}

/// What a live-menu call did, in bytecode order, for the embedder to carry out.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveMenuEffect {
    /// `ServerPlayer$1.sendInitialData`: `ClientboundContainerSetContentPacket(containerId,
    /// stateId, items, carried)` (the stack copies vanilla sends), then one
    /// [`LiveMenuEffect::DataChanged`] per data slot.
    InitialData {
        container_id: i32,
        /// The `stateId` the packet carries; `None` once the client's ids are no longer
        /// Decaf's to know (after a click whose predictions the embedder kept).
        state_id: Option<i32>,
        items: Vec<recipe_models::StackComponents>,
        carried: recipe_models::StackComponents,
    },
    /// `ServerPlayer$1.sendSlotChange`: `ClientboundContainerSetSlotPacket(containerId, stateId,
    /// slot, stack)`: the server's stack in menu slot `slot` changed.
    SlotChanged {
        container_id: i32,
        state_id: Option<i32>,
        slot: i32,
        stack: recipe_models::StackComponents,
    },
    /// `ServerPlayer$1.sendCarriedChange`: `ClientboundSetCursorItemPacket(stack)`.
    CarriedChanged {
        stack: recipe_models::StackComponents,
    },
    /// `ServerPlayer$1.sendDataChange` / `broadcastDataValue`:
    /// `ClientboundContainerSetDataPacket(containerId, id, value)`.
    DataChanged {
        container_id: i32,
        index: i32,
        value: i32,
    },
    /// `ServerPlayer$2.slotChanged` on a slot of the player's own inventory:
    /// `CriteriaTriggers.INVENTORY_CHANGED.trigger(player, inventory, stack)`.
    InventoryChangedCriterion {
        stack: recipe_models::StackComponents,
    },
    /// `Player.drop(stack, retainOwnership, prediction)` on the server player, for a non-empty
    /// stack: the embedder drops it as `ServerPlayer.drop` does (`LivingEntity.drop`'s swing and
    /// item entity, then, with `retain_ownership`, the `ITEM_DROPPED` and `DROP` stats).
    Drop {
        stack: recipe_models::StackComponents,
        retain_ownership: bool,
        prediction: Prediction,
    },
    /// `ServerPlayer.awardStat(StatType.get(item), amount)`: `stat_type` is the stat type's
    /// registry id (`minecraft:crafted`), `value` the stat's value's id (the item).
    AwardStat {
        stat_type: String,
        value: String,
        amount: i32,
    },
    /// `ServerPlayer.triggerRecipeCrafted(recipe, ingredients)`:
    /// `CriteriaTriggers.RECIPE_CRAFTED.trigger(player, recipe.id(), ingredients)`.
    RecipeCrafted {
        recipe: String,
        ingredients: Vec<recipe_models::StackComponents>,
    },
    /// `ServerPlayer.awardRecipes(Collections.singleton(recipe))`: the recipe book unlock.
    AwardRecipes { recipes: Vec<String> },
    /// `Inventory.placeItemBackInInventory` sent `ClientboundSetPlayerInventoryPacket(slot,
    /// stack.copy())`.
    InventorySlotSent {
        slot: i32,
        stack: recipe_models::StackComponents,
    },
    /// `ServerPlayer.closeContainer()` (from a tick whose `stillValid` failed) sent the client
    /// `ClientboundContainerClosePacket(containerId)`.
    Closed { container_id: i32 },
    /// `Slot.onTake(player, stack)` ran on menu slot `slot` (`Slot.index`; `None` for a jar
    /// `Slot` Decaf keeps no state for), once per call (a vanilla body's, a jar method's on a
    /// vanilla slot, or a jar override `vcall` dispatched to): the moment an embedder's own take
    /// notice (Sunrise's `menu_slot_take`) is heard. Nothing is sent for it.
    SlotTaken { slot: Option<i32> },
}

/// A click the client sent for the menu it shows: `ServerboundContainerClickPacket`.
#[derive(Clone, Debug)]
pub struct MenuClick {
    /// The window (`containerId`) the click is for.
    pub container_id: i32,
    /// `slotNum`: a menu slot index, `-999` outside the window, `-1` for none.
    pub slot: i32,
    /// `buttonNum`.
    pub button: i32,
    pub input: ContainerInput,
    /// Whether the packet's `stateId` equals the window's (`getStateId()`): vanilla answers a
    /// mismatch with `broadcastFullState`, a match with `broadcastChanges`. The embedder owns the
    /// window's state ids, so it compares them.
    pub state_id_matches: bool,
}

/// Result of a live-menu call ([`NativeJar::click_player_menu`],
/// [`NativeJar::tick_player_menu`], [`NativeJar::close_player_menu_live`]).
#[derive(Clone, Debug)]
pub struct LiveMenuOutcome {
    /// The window the call was for.
    pub container_id: i32,
    /// What the call did, in order.
    pub effects: Vec<LiveMenuEffect>,
    /// Opens and closes the call's Java made (a `removed` that opens a menu, a failed
    /// `stillValid`'s close).
    pub menu_effects: Vec<block_use::MenuEffect>,
    /// The player's inventory afterwards (the 36 or 43 entries the facts gave), when known.
    pub inventory: Option<Vec<Value>>,
    /// The menu's `getCarried()` afterwards, when the menu is still held.
    pub carried: Option<Value>,
    /// Every menu slot's `getItem()` afterwards, when the menu is still held and Decaf knows
    /// them without running jar code.
    pub slots: Option<Vec<Value>>,
    /// Whether vanilla's handler ran the click at all (`false`: the packet named another
    /// window, `stillValid` failed, or the slot index was invalid; vanilla drops it).
    pub handled: bool,
    /// Unmodelled host calls this call made.
    pub unknown_host_calls: BTreeMap<String, usize>,
    pub unknown_host_call_count: usize,
    /// Why the call didn't finish (an exception Java didn't catch, a budget). A click whose
    /// `doClick` threw fails with `clicked`'s `ReportedException`, and the line names the
    /// exception it wrapped (`...; caused by ...`).
    pub failure: Option<String>,
}

// ---------------------------------------------------------------------------
// Classes and descriptors
// ---------------------------------------------------------------------------

const CONTAINER_INPUT_PREFIX: &str = "enum/net/minecraft/world/inventory/ContainerInput/";
const PREDICTION_PREFIX: &str = "enum/net/minecraft/util/Prediction/";

const ACM: &str = menu_models::ABSTRACT_CONTAINER_MENU;
const ICM: &str = menu_models::ITEM_COMBINER_MENU;
const SLOT: &str = menu_models::SLOT;
const INVENTORY: &str = menu_models::INVENTORY;
const CONTAINER: &str = "net/minecraft/world/Container";
const SIMPLE_CONTAINER: &str = "net/minecraft/world/SimpleContainer";
const RESULT_CONTAINER: &str = "net/minecraft/world/inventory/ResultContainer";
const RECIPE_CRAFTING_HOLDER: &str = "net/minecraft/world/inventory/RecipeCraftingHolder";
/// `ItemCombinerMenu$1`: the combiner's `ResultContainer`, whose `setChanged` calls
/// `slotsChanged(this)`.
const ICM_RESULT_CONTAINER: &str = "net/minecraft/world/inventory/ItemCombinerMenu$1";
/// `ItemCombinerMenu$2`: an input slot, whose `mayPlace` tests the definition's predicate.
const ICM_INPUT_SLOT: &str = "net/minecraft/world/inventory/ItemCombinerMenu$2";
/// `ItemCombinerMenu$3`: the result slot (`mayPlace` false, `mayPickup`/`onTake` to the menu).
const ICM_RESULT_SLOT: &str = "net/minecraft/world/inventory/ItemCombinerMenu$3";
/// `ItemCombinerMenu$4`: the input `SimpleContainer`, whose `setChanged` also calls
/// `slotsChanged(this)`.
const ICM_INPUT_CONTAINER: &str = "net/minecraft/world/inventory/ItemCombinerMenu$4";
const DATA_SLOT: &str = "net/minecraft/world/inventory/DataSlot";
const DATA_SLOT_CONTAINER: &str = "net/minecraft/world/inventory/DataSlot$1";
const DATA_SLOT_STANDALONE: &str = "net/minecraft/world/inventory/DataSlot$3";
const CONTAINER_DATA: &str = "net/minecraft/world/inventory/ContainerData";
const ITEM_STACK: &str = recipe_models::ITEM_STACK;
const SERVER_PLAYER: &str = "net/minecraft/server/level/ServerPlayer";

const D_PLAYER_V: &str = "(Lnet/minecraft/world/entity/player/Player;)V";
const D_PLAYER_Z: &str = "(Lnet/minecraft/world/entity/player/Player;)Z";
const D_GET_ITEM: &str = "()Lnet/minecraft/world/item/ItemStack;";
const D_STACK_V: &str = "(Lnet/minecraft/world/item/ItemStack;)V";
const D_STACK_Z: &str = "(Lnet/minecraft/world/item/ItemStack;)Z";
const D_STACK_I: &str = "(Lnet/minecraft/world/item/ItemStack;)I";
const D_CONTAINER_V: &str = "(Lnet/minecraft/world/Container;)V";
const D_CLICKED: &str =
    "(IILnet/minecraft/world/inventory/ContainerInput;Lnet/minecraft/world/entity/player/Player;)V";
const D_PLAYER_STACK_V: &str =
    "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/item/ItemStack;)V";
const D_I_STACK: &str = "(I)Lnet/minecraft/world/item/ItemStack;";
const D_II_STACK: &str = "(II)Lnet/minecraft/world/item/ItemStack;";
const D_I_STACK_V: &str = "(ILnet/minecraft/world/item/ItemStack;)V";
const D_SLOT_Z: &str = "(Lnet/minecraft/world/inventory/Slot;)Z";

fn mref(owner: &str, name: &str, descriptor: &str) -> MethodRef {
    MethodRef {
        owner: owner.to_owned(),
        name: name.to_owned(),
        descriptor: descriptor.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Control flow
// ---------------------------------------------------------------------------

/// How a modelled body stops early.
pub(crate) enum Flow {
    /// A Java exception (or a runtime error) propagating out of the body, as the JVM does.
    Java(NativeError),
    /// A step needed something Decaf can't answer exactly: the outermost modelled method is
    /// counted and nothing more of it runs.
    Counted,
}

impl From<NativeError> for Flow {
    fn from(error: NativeError) -> Self {
        Self::Java(error)
    }
}

type R<T> = Result<T, Flow>;

fn counted() -> Flow {
    Flow::Counted
}

fn npe() -> Flow {
    Flow::Java(NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    })
}

fn throw(class: &str, message: String) -> Flow {
    Flow::Java(NativeError::UncaughtException {
        class: class.to_owned(),
        message: Some(message),
        frame: None,
    })
}

/// `ArrayList.get`'s / `Objects.checkIndex`'s exception for `index` outside `0..len`.
fn index_out_of_bounds(index: i32, len: usize) -> Flow {
    throw(
        "java/lang/IndexOutOfBoundsException",
        format!("Index {index} out of bounds for length {len}"),
    )
}

/// `Arrays.asList` (a `NonNullList.withSize`'s backing list) indexing an array out of range.
fn array_index_out_of_bounds(index: i32, len: usize) -> Flow {
    throw(
        "java/lang/ArrayIndexOutOfBoundsException",
        format!("Index {index} out of bounds for length {len}"),
    )
}

fn as_int(value: &Value) -> R<i32> {
    match value {
        Value::Int(v) => Ok(*v),
        _ => Err(counted()),
    }
}

fn as_bool(value: &Value) -> R<bool> {
    as_int(value).map(|v| v != 0)
}

fn bool_value(flag: bool) -> Value {
    Value::Int(i32::from(flag))
}

fn std_host<'a, H: Host>(rt: &'a mut Runtime<'_, H>) -> R<&'a mut StdHost> {
    rt.host.as_std_host_mut().ok_or_else(counted)
}

/// Run `f`, then fail with [`Flow::Counted`] if anything it ran counted.
fn exact<H: Host, T>(rt: &mut Runtime<'_, H>, f: impl FnOnce(&mut Runtime<'_, H>) -> R<T>) -> R<T> {
    let before = rt.host.counted_unknown_calls();
    let value = f(rt)?;
    if rt.host.counted_unknown_calls() != before {
        return Err(counted());
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Vanilla class resolution
// ---------------------------------------------------------------------------

/// The first class of `receiver`'s superclass chain that the runtime doesn't hold: the vanilla
/// class a jar subclass exits at, or the receiver's own class when Decaf built it.
fn vanilla_class_of<H: Host>(rt: &Runtime<'_, H>, receiver: &Value) -> Option<String> {
    let class = jar_object::object_class(receiver)?;
    if rt.class(class).is_some() {
        rt.vanilla_superclass(class)
    } else {
        Some(class.to_owned())
    }
}

/// The vanilla classes a body search walks from `class`, most derived first.
fn vanilla_chain(class: &str) -> &'static [&'static str] {
    match class {
        ICM_INPUT_SLOT => &[ICM_INPUT_SLOT, SLOT],
        ICM_RESULT_SLOT => &[ICM_RESULT_SLOT, SLOT],
        SLOT => &[SLOT],
        ICM_INPUT_CONTAINER => &[ICM_INPUT_CONTAINER, SIMPLE_CONTAINER, CONTAINER],
        SIMPLE_CONTAINER => &[SIMPLE_CONTAINER, CONTAINER],
        ICM_RESULT_CONTAINER => &[
            ICM_RESULT_CONTAINER,
            RESULT_CONTAINER,
            CONTAINER,
            RECIPE_CRAFTING_HOLDER,
        ],
        RESULT_CONTAINER => &[RESULT_CONTAINER, CONTAINER, RECIPE_CRAFTING_HOLDER],
        INVENTORY => &[INVENTORY, CONTAINER],
        CONTAINER => &[CONTAINER],
        RECIPE_CRAFTING_HOLDER => &[RECIPE_CRAFTING_HOLDER],
        ICM => &[ICM, ACM],
        ACM => &[ACM],
        DATA_SLOT_CONTAINER => &[DATA_SLOT_CONTAINER, DATA_SLOT],
        DATA_SLOT_STANDALONE => &[DATA_SLOT_STANDALONE, DATA_SLOT],
        DATA_SLOT => &[DATA_SLOT],
        _ => &[],
    }
}

/// A vanilla method body this module runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
    // AbstractContainerMenu
    AcmGetCarried,
    AcmSetCarried,
    AcmClicked,
    AcmMoveItemStackTo,
    AcmResetQuickCraft,
    AcmCanDragTo,
    AcmCanTakeItemForPickAll,
    AcmClickMenuButton,
    AcmIsValidSlotIndex,
    AcmGetSlot,
    AcmSlotsChanged,
    AcmRemoved,
    AcmClearContainer,
    AcmBroadcastChanges,
    AcmBroadcastFullState,
    AcmSendAllDataToRemote,
    AcmSuppressRemoteUpdates,
    AcmResumeRemoteUpdates,
    AcmGetStateId,
    AcmIncrementStateId,
    AcmSetRemoteSlot,
    AcmSetItem,
    AcmSetData,
    AcmAddDataSlot,
    AcmAddDataSlots,
    AcmFindSlot,
    AcmAddSlotListener,
    // ItemCombinerMenu
    IcmQuickMoveStack,
    IcmCanMoveIntoInputSlots,
    IcmMayPickup,
    IcmSlotsChanged,
    IcmRemoved,
    IcmStillValid,
    IcmGetResultSlot,
    // Slot (+ ItemCombinerMenu$2 / $3)
    SlotGetItem,
    SlotHasItem,
    SlotSet,
    SlotSetByPlayer1,
    SlotSetByPlayer2,
    SlotSetChanged,
    SlotGetMaxStackSize0,
    SlotGetMaxStackSize1,
    SlotRemove,
    SlotMayPickup,
    SlotMayPlace,
    SlotOnTake,
    SlotOnQuickCraft2,
    SlotNoOp,
    SlotTryRemove,
    SlotSafeTake,
    SlotSafeClone,
    SlotSafeInsert1,
    SlotSafeInsert2,
    SlotAllowModification,
    SlotGetContainerSlot,
    SlotTrue,
    SlotFalse,
    SlotGetNoItemIcon,
    InputSlotMayPlace,
    ResultSlotMayPlace,
    ResultSlotMayPickup,
    ResultSlotOnTake,
    // Containers
    ContainerMaxStackSize0,
    ContainerMaxStackSize1,
    ContainerTrue,
    ContainerNoOp,
    SimpleGetItem,
    SimpleSetItem,
    SimpleRemoveItem,
    SimpleRemoveItemNoUpdate,
    SimpleSetChanged,
    SimpleGetContainerSize,
    SimpleIsEmpty,
    SimpleClearContent,
    InputContainerSetChanged,
    ResultGetItem,
    ResultSetItem,
    ResultTakeItem,
    ResultSetChanged,
    ResultGetContainerSize,
    ResultIsEmpty,
    ResultClearContent,
    ResultSetRecipeUsed,
    ResultGetRecipeUsed,
    ResultContainerSetChanged,
    HolderAwardUsedRecipes,
    InvGetItem,
    InvSetItem,
    InvRemoveItem,
    InvRemoveItemNoUpdate,
    InvGetContainerSize,
    InvSetChanged,
    InvAdd1,
    InvAdd2,
    InvPlaceBack2,
    InvPlaceBack3,
    InvGetSlotWithRemainingSpace,
    InvGetFreeSlot,
    InvGetSelectedSlot,
    InvGetSelectedItem,
    // DataSlot
    DataSlotCheckAndClear,
    DataSlotContainerGet,
    DataSlotContainerSet,
    DataSlotStandaloneGet,
    DataSlotStandaloneSet,
}

/// The body `class` itself declares for `name descriptor`, if this module models it.
fn declared(class: &str, name: &str, descriptor: &str) -> Option<Body> {
    use Body::*;
    Some(match (class, name, descriptor) {
        (ACM, "getCarried", D_GET_ITEM) => AcmGetCarried,
        (ACM, "setCarried", D_STACK_V) => AcmSetCarried,
        (ACM, "clicked", D_CLICKED) => AcmClicked,
        (ACM, "moveItemStackTo", "(Lnet/minecraft/world/item/ItemStack;IIZ)Z") => {
            AcmMoveItemStackTo
        }
        (ACM, "resetQuickCraft", "()V") => AcmResetQuickCraft,
        (ACM, "canDragTo", D_SLOT_Z) => AcmCanDragTo,
        (
            ACM,
            "canTakeItemForPickAll",
            "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/inventory/Slot;)Z",
        ) => AcmCanTakeItemForPickAll,
        (ACM, "clickMenuButton", "(Lnet/minecraft/world/entity/player/Player;I)Z") => {
            AcmClickMenuButton
        }
        (ACM, "isValidSlotIndex", "(I)Z") => AcmIsValidSlotIndex,
        (ACM, "getSlot", "(I)Lnet/minecraft/world/inventory/Slot;") => AcmGetSlot,
        (ACM, "slotsChanged", D_CONTAINER_V) => AcmSlotsChanged,
        (ACM, "removed", D_PLAYER_V) => AcmRemoved,
        (
            ACM,
            "clearContainer",
            "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/Container;)V",
        ) => AcmClearContainer,
        (ACM, "broadcastChanges", "()V") => AcmBroadcastChanges,
        (ACM, "broadcastFullState", "()V") => AcmBroadcastFullState,
        (ACM, "sendAllDataToRemote", "()V") => AcmSendAllDataToRemote,
        (ACM, "suppressRemoteUpdates", "()V") => AcmSuppressRemoteUpdates,
        (ACM, "resumeRemoteUpdates", "()V") => AcmResumeRemoteUpdates,
        (ACM, "getStateId", "()I") => AcmGetStateId,
        (ACM, "incrementStateId", "()I") => AcmIncrementStateId,
        (ACM, "setRemoteSlot", D_I_STACK_V) => AcmSetRemoteSlot,
        (ACM, "setItem", "(IILnet/minecraft/world/item/ItemStack;)V") => AcmSetItem,
        (ACM, "setData", "(II)V") => AcmSetData,
        (
            ACM,
            "addDataSlot",
            "(Lnet/minecraft/world/inventory/DataSlot;)Lnet/minecraft/world/inventory/DataSlot;",
        ) => AcmAddDataSlot,
        (ACM, "addDataSlots", "(Lnet/minecraft/world/inventory/ContainerData;)V") => {
            AcmAddDataSlots
        }
        (ACM, "findSlot", "(Lnet/minecraft/world/Container;I)Ljava/util/OptionalInt;") => {
            AcmFindSlot
        }
        (ACM, "addSlotListener", "(Lnet/minecraft/world/inventory/ContainerListener;)V") => {
            AcmAddSlotListener
        }
        (
            ICM,
            "quickMoveStack",
            "(Lnet/minecraft/world/entity/player/Player;I)Lnet/minecraft/world/item/ItemStack;",
        ) => IcmQuickMoveStack,
        (ICM, "canMoveIntoInputSlots", D_STACK_Z) => IcmCanMoveIntoInputSlots,
        (ICM, "mayPickup", "(Lnet/minecraft/world/entity/player/Player;Z)Z") => IcmMayPickup,
        (ICM, "slotsChanged", D_CONTAINER_V) => IcmSlotsChanged,
        (ICM, "removed", D_PLAYER_V) => IcmRemoved,
        (ICM, "stillValid", D_PLAYER_Z) => IcmStillValid,
        (ICM, "getResultSlot", "()I") => IcmGetResultSlot,
        (SLOT, "getItem", D_GET_ITEM) => SlotGetItem,
        (SLOT, "hasItem", "()Z") => SlotHasItem,
        (SLOT, "set", D_STACK_V) => SlotSet,
        (SLOT, "setByPlayer", D_STACK_V) => SlotSetByPlayer1,
        (
            SLOT,
            "setByPlayer",
            "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemStack;)V",
        ) => SlotSetByPlayer2,
        (SLOT, "setChanged", "()V") => SlotSetChanged,
        (SLOT, "getMaxStackSize", "()I") => SlotGetMaxStackSize0,
        (SLOT, "getMaxStackSize", D_STACK_I) => SlotGetMaxStackSize1,
        (SLOT, "remove", D_I_STACK) => SlotRemove,
        (SLOT, "mayPickup", D_PLAYER_Z) => SlotMayPickup,
        (SLOT, "mayPlace", D_STACK_Z) => SlotMayPlace,
        (SLOT, "onTake", D_PLAYER_STACK_V) => SlotOnTake,
        (
            SLOT,
            "onQuickCraft",
            "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemStack;)V",
        ) => SlotOnQuickCraft2,
        (SLOT, "onQuickCraft", "(Lnet/minecraft/world/item/ItemStack;I)V")
        | (SLOT, "onSwapCraft", "(I)V")
        | (SLOT, "checkTakeAchievements", D_STACK_V) => SlotNoOp,
        (
            SLOT,
            "tryRemove",
            "(IILnet/minecraft/world/entity/player/Player;)Ljava/util/Optional;",
        ) => SlotTryRemove,
        (
            SLOT,
            "safeTake",
            "(IILnet/minecraft/world/entity/player/Player;)Lnet/minecraft/world/item/ItemStack;",
        ) => SlotSafeTake,
        (
            SLOT,
            "safeClone",
            "(Lnet/minecraft/world/entity/player/Player;)Lnet/minecraft/world/item/ItemStack;",
        ) => SlotSafeClone,
        (
            SLOT,
            "safeInsert",
            "(Lnet/minecraft/world/item/ItemStack;)Lnet/minecraft/world/item/ItemStack;",
        ) => SlotSafeInsert1,
        (
            SLOT,
            "safeInsert",
            "(Lnet/minecraft/world/item/ItemStack;I)Lnet/minecraft/world/item/ItemStack;",
        ) => SlotSafeInsert2,
        (SLOT, "allowModification", D_PLAYER_Z) => SlotAllowModification,
        (SLOT, "getContainerSlot", "()I") => SlotGetContainerSlot,
        (SLOT, "isActive" | "isHighlightable", "()Z") => SlotTrue,
        (SLOT, "isFake", "()Z") => SlotFalse,
        (SLOT, "getNoItemIcon", "()Lnet/minecraft/resources/Identifier;") => SlotGetNoItemIcon,
        (ICM_INPUT_SLOT, "mayPlace", D_STACK_Z) => InputSlotMayPlace,
        (ICM_RESULT_SLOT, "mayPlace", D_STACK_Z) => ResultSlotMayPlace,
        (ICM_RESULT_SLOT, "mayPickup", D_PLAYER_Z) => ResultSlotMayPickup,
        (ICM_RESULT_SLOT, "onTake", D_PLAYER_STACK_V) => ResultSlotOnTake,
        (CONTAINER, "getMaxStackSize", "()I") => ContainerMaxStackSize0,
        (CONTAINER, "getMaxStackSize", D_STACK_I) => ContainerMaxStackSize1,
        (CONTAINER, "canPlaceItem", "(ILnet/minecraft/world/item/ItemStack;)Z")
        | (
            CONTAINER,
            "canTakeItem",
            "(Lnet/minecraft/world/Container;ILnet/minecraft/world/item/ItemStack;)Z",
        ) => ContainerTrue,
        (CONTAINER, "startOpen" | "stopOpen", "(Lnet/minecraft/world/entity/ContainerUser;)V") => {
            ContainerNoOp
        }
        (SIMPLE_CONTAINER, "getItem", D_I_STACK) => SimpleGetItem,
        (SIMPLE_CONTAINER, "setItem", D_I_STACK_V) => SimpleSetItem,
        (SIMPLE_CONTAINER, "removeItem", D_II_STACK) => SimpleRemoveItem,
        (SIMPLE_CONTAINER, "removeItemNoUpdate", D_I_STACK) => SimpleRemoveItemNoUpdate,
        (SIMPLE_CONTAINER, "setChanged", "()V") => SimpleSetChanged,
        (SIMPLE_CONTAINER, "getContainerSize", "()I") => SimpleGetContainerSize,
        (SIMPLE_CONTAINER, "isEmpty", "()Z") => SimpleIsEmpty,
        (SIMPLE_CONTAINER | RESULT_CONTAINER, "stillValid", D_PLAYER_Z) => ContainerTrue,
        (SIMPLE_CONTAINER, "clearContent", "()V") => SimpleClearContent,
        (ICM_INPUT_CONTAINER, "setChanged", "()V") => InputContainerSetChanged,
        (RESULT_CONTAINER, "getItem", D_I_STACK) => ResultGetItem,
        (RESULT_CONTAINER, "setItem", D_I_STACK_V) => ResultSetItem,
        (RESULT_CONTAINER, "removeItem", D_II_STACK)
        | (RESULT_CONTAINER, "removeItemNoUpdate", D_I_STACK) => ResultTakeItem,
        (RESULT_CONTAINER, "setChanged", "()V") => ResultSetChanged,
        (RESULT_CONTAINER, "getContainerSize", "()I") => ResultGetContainerSize,
        (RESULT_CONTAINER, "isEmpty", "()Z") => ResultIsEmpty,
        (RESULT_CONTAINER, "clearContent", "()V") => ResultClearContent,
        (
            RESULT_CONTAINER,
            "setRecipeUsed",
            "(Lnet/minecraft/world/item/crafting/RecipeHolder;)V",
        ) => ResultSetRecipeUsed,
        (
            RESULT_CONTAINER,
            "getRecipeUsed",
            "()Lnet/minecraft/world/item/crafting/RecipeHolder;",
        ) => ResultGetRecipeUsed,
        (ICM_RESULT_CONTAINER, "setChanged", "()V") => ResultContainerSetChanged,
        (
            RECIPE_CRAFTING_HOLDER,
            "awardUsedRecipes",
            "(Lnet/minecraft/world/entity/player/Player;Ljava/util/List;)V",
        ) => HolderAwardUsedRecipes,
        (INVENTORY, "getItem", D_I_STACK) => InvGetItem,
        (INVENTORY, "setItem", D_I_STACK_V) => InvSetItem,
        (INVENTORY, "removeItem", D_II_STACK) => InvRemoveItem,
        (INVENTORY, "removeItemNoUpdate", D_I_STACK) => InvRemoveItemNoUpdate,
        (INVENTORY, "getContainerSize", "()I") => InvGetContainerSize,
        (INVENTORY, "setChanged", "()V") => InvSetChanged,
        (INVENTORY, "add", D_STACK_Z) => InvAdd1,
        (INVENTORY, "add", "(ILnet/minecraft/world/item/ItemStack;)Z") => InvAdd2,
        (
            INVENTORY,
            "placeItemBackInInventory",
            "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/util/Prediction;)V",
        ) => InvPlaceBack2,
        (
            INVENTORY,
            "placeItemBackInInventory",
            "(Lnet/minecraft/world/item/ItemStack;ZLnet/minecraft/util/Prediction;)V",
        ) => InvPlaceBack3,
        (INVENTORY, "getSlotWithRemainingSpace", D_STACK_I) => InvGetSlotWithRemainingSpace,
        (INVENTORY, "getFreeSlot", "()I") => InvGetFreeSlot,
        (INVENTORY, "getSelectedSlot", "()I") => InvGetSelectedSlot,
        (INVENTORY, "getSelectedItem", D_GET_ITEM) => InvGetSelectedItem,
        (DATA_SLOT, "checkAndClearUpdateFlag", "()Z") => DataSlotCheckAndClear,
        (DATA_SLOT_CONTAINER, "get", "()I") => DataSlotContainerGet,
        (DATA_SLOT_CONTAINER, "set", "(I)V") => DataSlotContainerSet,
        (DATA_SLOT_STANDALONE, "get", "()I") => DataSlotStandaloneGet,
        (DATA_SLOT_STANDALONE, "set", "(I)V") => DataSlotStandaloneSet,
        _ => return None,
    })
}

/// JVM method selection over the vanilla chain from `class`: the first declaring class's body.
fn resolve(class: &str, name: &str, descriptor: &str) -> Option<(Body, &'static str)> {
    vanilla_chain(class)
        .iter()
        .find_map(|owner| declared(owner, name, descriptor).map(|body| (body, *owner)))
}

/// Whether Decaf holds the state `body` reads for `receiver` (otherwise the call is left to
/// the older models and counting it falls to them).
fn body_applies(host: &StdHost, body: Body, receiver: &Value) -> bool {
    use Body::*;
    let Some(key) = expect_ref(receiver) else {
        return false;
    };
    let state = host.menu_states.get(key);
    match body {
        AcmGetCarried
        | AcmSetCarried
        | AcmClicked
        | AcmMoveItemStackTo
        | AcmResetQuickCraft
        | AcmCanDragTo
        | AcmCanTakeItemForPickAll
        | AcmClickMenuButton
        | AcmIsValidSlotIndex
        | AcmGetSlot
        | AcmSlotsChanged
        | AcmRemoved
        | AcmClearContainer
        | AcmBroadcastChanges
        | AcmBroadcastFullState
        | AcmSendAllDataToRemote
        | AcmSuppressRemoteUpdates
        | AcmResumeRemoteUpdates
        | AcmGetStateId
        | AcmIncrementStateId
        | AcmSetRemoteSlot
        | AcmSetItem
        | AcmFindSlot
        | AcmAddSlotListener => {
            matches!(state, Some(menu_models::Object::Menu(menu)) if !menu_has_client_slots(host, menu))
        }
        // The data slots are the menu's own bookkeeping, the same on either side: a client
        // screen's menu (`client/inventory` slots) keeps them too, so the screen bridge can write
        // the server's values (`setData`) and a jar's `DataSlot.get` reads them.
        AcmSetData | AcmAddDataSlot | AcmAddDataSlots => {
            matches!(state, Some(menu_models::Object::Menu(_)))
        }
        IcmQuickMoveStack
        | IcmCanMoveIntoInputSlots
        | IcmMayPickup
        | IcmSlotsChanged
        | IcmRemoved
        | IcmStillValid
        | IcmGetResultSlot => matches!(
            state,
            Some(menu_models::Object::Menu(menu))
                if menu.combiner.is_some() && !menu_has_client_slots(host, menu)
        ),
        SlotGetItem
        | SlotHasItem
        | SlotSet
        | SlotSetByPlayer1
        | SlotSetByPlayer2
        | SlotSetChanged
        | SlotGetMaxStackSize0
        | SlotGetMaxStackSize1
        | SlotRemove
        | SlotMayPickup
        | SlotMayPlace
        | SlotOnTake
        | SlotOnQuickCraft2
        | SlotNoOp
        | SlotTryRemove
        | SlotSafeTake
        | SlotSafeClone
        | SlotSafeInsert1
        | SlotSafeInsert2
        | SlotAllowModification
        | SlotGetContainerSlot
        | SlotTrue
        | SlotFalse
        | SlotGetNoItemIcon
        | InputSlotMayPlace
        | ResultSlotMayPlace
        | ResultSlotMayPickup
        | ResultSlotOnTake => {
            matches!(state, Some(menu_models::Object::Slot(_))) && !slot_is_client(host, receiver)
        }
        ContainerMaxStackSize0 | ContainerMaxStackSize1 | ContainerTrue | ContainerNoOp => {
            matches!(
                state,
                Some(menu_models::Object::Container { .. } | menu_models::Object::Inventory { .. })
            )
        }
        SimpleGetItem
        | SimpleSetItem
        | SimpleRemoveItem
        | SimpleRemoveItemNoUpdate
        | SimpleSetChanged
        | SimpleGetContainerSize
        | SimpleIsEmpty
        | SimpleClearContent
        | InputContainerSetChanged => matches!(
            state,
            Some(menu_models::Object::Container { result: false, .. })
        ),
        ResultGetItem
        | ResultSetItem
        | ResultTakeItem
        | ResultSetChanged
        | ResultGetContainerSize
        | ResultIsEmpty
        | ResultClearContent
        | ResultSetRecipeUsed
        | ResultGetRecipeUsed
        | ResultContainerSetChanged
        | HolderAwardUsedRecipes => matches!(
            state,
            Some(menu_models::Object::Container { result: true, .. })
        ),
        InvGetItem
        | InvSetItem
        | InvRemoveItem
        | InvRemoveItemNoUpdate
        | InvGetContainerSize
        | InvSetChanged
        | InvAdd1
        | InvAdd2
        | InvPlaceBack2
        | InvPlaceBack3
        | InvGetSlotWithRemainingSpace
        | InvGetFreeSlot
        | InvGetSelectedSlot
        | InvGetSelectedItem => matches!(state, Some(menu_models::Object::Inventory { .. })),
        // `prevValue` lives on any `DataSlot`, a jar subclass's included (`menu_models`
        // files it under the object's key on first use).
        DataSlotCheckAndClear => jar_object::object_key(receiver).is_some(),
        DataSlotContainerGet
        | DataSlotContainerSet
        | DataSlotStandaloneGet
        | DataSlotStandaloneSet => matches!(state, Some(menu_models::Object::DataSlot { .. })),
    }
}

/// A menu whose slots read the client's inventory (`client/inventory`): a client-side screen's
/// menu, which the older client models drive.
fn menu_has_client_slots(host: &StdHost, menu: &menu_models::MenuState) -> bool {
    let Some(menu_models::Object::SlotList { slots }) = host.menu_states.get(&menu.slots) else {
        return false;
    };
    slots.iter().any(|slot| match host.menu_states.get(slot) {
        Some(menu_models::Object::Slot(slot)) => {
            game_object::handle_key(slot.container()) == Some("client/inventory")
        }
        _ => false,
    })
}

fn slot_is_client(host: &StdHost, slot: &Value) -> bool {
    menu_models::slot_container_and_index(&host.menu_states, slot).is_some_and(|(container, _)| {
        game_object::handle_key(&container) == Some("client/inventory")
    })
}

/// Entry from the interpreter's call chain (`Runtime::invoke_screen_or_menu_method`), after any
/// jar override had its turn: run the vanilla body `r` selects on `receiver`, if this module
/// models it and Decaf holds the receiver's state.
#[inline(never)]
pub(crate) fn invoke<H: Host>(
    rt: &mut Runtime<'_, H>,
    effective_owner: &str,
    r: &MethodRef,
    op: u8,
    receiver: &Value,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let start = if op == 0xb7 {
        effective_owner.to_owned()
    } else {
        match vanilla_class_of(rt, receiver) {
            Some(class) => class,
            None => return Ok(None),
        }
    };
    let Some((body, owner)) = resolve(&start, &r.name, &r.descriptor) else {
        return Ok(None);
    };
    let applies = rt
        .host
        .as_std_host()
        .is_some_and(|host| body_applies(host, body, receiver));
    if !applies {
        return Ok(None);
    }
    match run(rt, body, receiver, args) {
        Ok(value) => Ok(Some(value)),
        Err(Flow::Java(error)) => Err(error),
        Err(Flow::Counted) => rt
            .host
            .unknown_operand_call(&mref(owner, &r.name, &r.descriptor))
            .map(Some),
    }
}

/// `receiver.name(args)` as the `invokevirtual`/`invokeinterface` a vanilla body makes: the
/// receiver's jar override, else this module's body for its vanilla class, else the generic
/// dispatch chain. Counted when a jar body or the generic chain counted anything.
fn vcall<H: Host>(
    rt: &mut Runtime<'_, H>,
    site: Site,
    owner: &str,
    name: &str,
    descriptor: &str,
    receiver: &Value,
    args: &[Value],
) -> R<Value> {
    let r = mref(owner, name, descriptor);
    if *receiver == Value::Ref(None) {
        return Err(Flow::Java(null_checks::invoke_on_null(&r)));
    }
    let op = if site == Site::Interface { 0xb9 } else { 0xb6 };
    let before = rt.host.counted_unknown_calls();
    if let Some(body_owner) = rt.jvm_selected_jar_body(&r, op, Some(receiver))? {
        // A jar `Slot.onTake` override is a take, told once: its own direct `super.onTake`,
        // which runs in the override's frame (one deeper than this call), isn't told again.
        let take = owner == SLOT && name == "onTake" && descriptor == D_PLAYER_STACK_V;
        if take {
            note_take(rt, receiver)?;
            let frame = rt.depth + 1;
            std_host(rt)?
                .block_use
                .taking
                .push((receiver.clone(), frame));
        }
        let value = rt.invoke(&body_owner, name, descriptor, Some(receiver.clone()), args);
        if take && let Some(host) = rt.host.as_std_host_mut() {
            host.block_use.taking.pop();
        }
        let value = value?;
        if rt.host.counted_unknown_calls() != before {
            return Err(counted());
        }
        return Ok(value);
    }
    if let Some(class) = vanilla_class_of(rt, receiver)
        && let Some((body, _)) = resolve(&class, name, descriptor)
        && rt
            .host
            .as_std_host()
            .is_some_and(|host| body_applies(host, body, receiver))
    {
        return run(rt, body, receiver, args);
    }
    let value = rt.call_virtual(site, owner, name, descriptor, receiver, args)?;
    if rt.host.counted_unknown_calls() != before {
        return Err(counted());
    }
    Ok(value)
}

/// `invokespecial` of vanilla body `body` (a `super.x()` inside vanilla, or a private method).
fn special<H: Host>(
    rt: &mut Runtime<'_, H>,
    body: Body,
    receiver: &Value,
    args: &[Value],
) -> R<Value> {
    if !rt
        .host
        .as_std_host()
        .is_some_and(|host| body_applies(host, body, receiver))
    {
        return Err(counted());
    }
    run(rt, body, receiver, args)
}

fn arg(args: &[Value], index: usize) -> R<&Value> {
    args.get(index).ok_or_else(counted)
}

#[inline(never)]
fn run<H: Host>(rt: &mut Runtime<'_, H>, body: Body, this: &Value, args: &[Value]) -> R<Value> {
    use Body::*;
    match body {
        AcmGetCarried => menu_live(rt, this, |live| live.carried.clone()),
        AcmSetCarried => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let stack = arg(args, 0)?.clone();
                menu_live_mut(rt, this, |live| live.carried = stack)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmClicked => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let (slot, button) = (as_int(arg(args, 0)?)?, as_int(arg(args, 1)?)?);
                let input = ContainerInput::from_value(arg(args, 2)?).ok_or_else(counted)?;
                acm_clicked(rt, this, slot, button, input, arg(args, 3)?)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmMoveItemStackTo => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let stack = arg(args, 0)?.clone();
                let (start, end) = (as_int(arg(args, 1)?)?, as_int(arg(args, 2)?)?);
                let reverse = as_bool(arg(args, 3)?)?;
                acm_move_item_stack_to(rt, this, &stack, start, end, reverse).map(bool_value)
            }
            arm(rt, this, args)
        }
        AcmResetQuickCraft => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `resetQuickCraft` offsets 0-14.
                menu_live_mut(rt, this, |live| {
                    live.quickcraft_status = 0;
                    live.quickcraft_slots.clear();
                })?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmCanDragTo | AcmCanTakeItemForPickAll => Ok(bool_value(true)),
        AcmClickMenuButton => Ok(bool_value(false)),
        AcmIsValidSlotIndex => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `isValidSlotIndex` offsets 0-28.
                let index = as_int(arg(args, 0)?)?;
                let size = menu_slots(rt, this)?.len();
                Ok(bool_value(
                    index == -1 || index == -999 || (index as i64) < size as i64,
                ))
            }
            arm(rt, this, args)
        }
        AcmGetSlot => menu_slot(rt, this, as_int(arg(args, 0)?)?),
        AcmSlotsChanged => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `AbstractContainerMenu.slotsChanged` offsets 0-4: `broadcastChanges()`.
                vcall(rt, Site::Virtual, ACM, "broadcastChanges", "()V", this, &[])?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmRemoved => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                acm_removed(rt, this, arg(args, 0)?)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmClearContainer => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, _this: &Value, args: &[Value]) -> R<Value> {
                acm_clear_container(rt, arg(args, 0)?, arg(args, 1)?)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmBroadcastChanges => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                acm_broadcast_changes(rt, this)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmBroadcastFullState => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                acm_broadcast_full_state(rt, this)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmSendAllDataToRemote => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                acm_send_all_data_to_remote(rt, this)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmSuppressRemoteUpdates => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                menu_live_mut(rt, this, |live| live.suppress_remote_updates = true)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmResumeRemoteUpdates => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                menu_live_mut(rt, this, |live| live.suppress_remote_updates = false)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmGetStateId => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                let (known, id) = menu_live(rt, this, |live| (live.state_id_known, live.state_id))?;
                if !known {
                    return Err(counted());
                }
                Ok(Value::Int(id))
            }
            arm(rt, this, args)
        }
        AcmIncrementStateId => acm_increment_state_id(rt, this).map(Value::Int),
        AcmSetRemoteSlot => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `setRemoteSlot` offsets 0-17: `remoteSlots.get(i).force(stack)`.
                let index = as_int(arg(args, 0)?)?;
                let stack = arg(args, 1)?.clone();
                remote_force(rt, this, RemoteTarget::Slot(index), &stack)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmSetItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `setItem(int, int, ItemStack)` offsets 0-14: `getSlot(slot).set(stack)`
                // (both virtual), then `stateId = stateId`.
                let index = as_int(arg(args, 0)?)?;
                let state_id = as_int(arg(args, 1)?)?;
                let stack = arg(args, 2)?.clone();
                let slot = vcall(
                    rt,
                    Site::Virtual,
                    ACM,
                    "getSlot",
                    "(I)Lnet/minecraft/world/inventory/Slot;",
                    this,
                    &[Value::Int(index)],
                )?;
                vcall(rt, Site::Virtual, SLOT, "set", D_STACK_V, &slot, &[stack])?;
                menu_live_mut(rt, this, |live| {
                    live.state_id = state_id;
                    live.state_id_known = true;
                })?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmSetData => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `setData` offsets 0-17: `dataSlots.get(id).set(value)`.
                let index = as_int(arg(args, 0)?)?;
                let value = as_int(arg(args, 1)?)?;
                let slots = menu_live(rt, this, |live| live.data_slots.clone())?;
                let Some(entry) = usize::try_from(index).ok().and_then(|i| slots.get(i)) else {
                    return Err(index_out_of_bounds(index, slots.len()));
                };
                vcall(
                    rt,
                    Site::Virtual,
                    DATA_SLOT,
                    "set",
                    "(I)V",
                    &entry.slot,
                    &[Value::Int(value)],
                )?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmAddDataSlot => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let slot = arg(args, 0)?.clone();
                acm_add_data_slot(rt, this, &slot)?;
                Ok(slot)
            }
            arm(rt, this, args)
        }
        AcmAddDataSlots => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                acm_add_data_slots(rt, this, arg(args, 0)?)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        AcmFindSlot => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `findSlot(Container, int)` offsets 0-58: first slot whose `container` is
                // the same object and whose virtual `getContainerSlot()` is `index`.
                let container = arg(args, 0)?.clone();
                let index = as_int(arg(args, 1)?)?;
                for (position, slot) in menu_slots(rt, this)?.into_iter().enumerate() {
                    if slot_container(rt, &slot)? == container
                        && as_int(&vcall(
                            rt,
                            Site::Virtual,
                            SLOT,
                            "getContainerSlot",
                            "()I",
                            &slot,
                            &[],
                        )?)? == index
                    {
                        return Ok(crate::runtime::host_optional::optional_int(position as i32));
                    }
                }
                Ok(crate::runtime::host_optional::empty_of(
                    crate::runtime::host_ref::HostKind::OptionalInt,
                ))
            }
            arm(rt, this, args)
        }
        AcmAddSlotListener => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                acm_add_slot_listener(rt, this, arg(args, 0)?)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        IcmQuickMoveStack => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                icm_quick_move_stack(rt, this, arg(args, 0)?, as_int(arg(args, 1)?)?)
            }
            arm(rt, this, args)
        }
        IcmCanMoveIntoInputSlots | IcmMayPickup => Ok(bool_value(true)),
        IcmSlotsChanged => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `ItemCombinerMenu.slotsChanged` offsets 0-17: `super.slotsChanged(c)`, then
                // `createResult()` (virtual, the mod's) when `c == inputSlots`.
                let container = arg(args, 0)?.clone();
                special(
                    rt,
                    Body::AcmSlotsChanged,
                    this,
                    std::slice::from_ref(&container),
                )?;
                let input = combiner(rt, this)?.input;
                if expect_ref(&container) == Some(&input) {
                    vcall(rt, Site::Virtual, ICM, "createResult", "()V", this, &[])?;
                }
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        IcmRemoved => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                icm_removed(rt, this, arg(args, 0)?)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        IcmStillValid => icm_still_valid(rt, this, arg(args, 0)?).map(bool_value),
        IcmGetResultSlot => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // `resultSlotIndex` (`<init>` offsets 45-51: `definition.getResultSlotIndex()`).
                Ok(Value::Int(combiner(rt, this)?.definition.result.index))
            }
            arm(rt, this, args)
        }
        SlotGetItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `Slot.getItem` offsets 0-13: `container.getItem(slot)` (interface).
                let (container, index) = slot_state(rt, this)?;
                container_get_item(rt, &container, index)
            }
            arm(rt, this, args)
        }
        SlotHasItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `hasItem` offsets 0-15: `!getItem().isEmpty()` (virtual getItem).
                let item = slot_get_item(rt, this)?;
                Ok(bool_value(!stack_is_empty(rt, &item)?))
            }
            arm(rt, this, args)
        }
        SlotSet => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `Slot.set` offsets 0-18: `container.setItem(slot, stack)`, `setChanged()`.
                let (container, index) = slot_state(rt, this)?;
                let stack = arg(args, 0)?.clone();
                vcall(
                    rt,
                    Site::Interface,
                    CONTAINER,
                    "setItem",
                    D_I_STACK_V,
                    &container,
                    &[Value::Int(index), stack],
                )?;
                vcall(rt, Site::Virtual, SLOT, "setChanged", "()V", this, &[])?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        SlotSetByPlayer1 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `setByPlayer(ItemStack)` offsets 0-9: `setByPlayer(stack, getItem())`.
                let stack = arg(args, 0)?.clone();
                let old = slot_get_item(rt, this)?;
                slot_set_by_player(rt, this, &stack, &old)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        SlotSetByPlayer2 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `setByPlayer(ItemStack, ItemStack)` offsets 0-5: `set(stack)`.
                let stack = arg(args, 0)?.clone();
                vcall(rt, Site::Virtual, SLOT, "set", D_STACK_V, this, &[stack])?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        SlotSetChanged => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `Slot.setChanged` offsets 0-9: `container.setChanged()`.
                let (container, _) = slot_state(rt, this)?;
                vcall(
                    rt,
                    Site::Interface,
                    CONTAINER,
                    "setChanged",
                    "()V",
                    &container,
                    &[],
                )?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        SlotGetMaxStackSize0 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `Slot.getMaxStackSize()` offsets 0-9: `container.getMaxStackSize()`.
                let (container, _) = slot_state(rt, this)?;
                vcall(
                    rt,
                    Site::Interface,
                    CONTAINER,
                    "getMaxStackSize",
                    "()I",
                    &container,
                    &[],
                )
            }
            arm(rt, this, args)
        }
        SlotGetMaxStackSize1 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `Slot.getMaxStackSize(ItemStack)` offsets 0-11:
                // `Math.min(getMaxStackSize(), stack.getMaxStackSize())`.
                let stack = arg(args, 0)?.clone();
                let own = as_int(&vcall(
                    rt,
                    Site::Virtual,
                    SLOT,
                    "getMaxStackSize",
                    "()I",
                    this,
                    &[],
                )?)?;
                let item = stack_max_stack_size(rt, &stack)?;
                Ok(Value::Int(own.min(item)))
            }
            arm(rt, this, args)
        }
        SlotRemove => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `Slot.remove(int)` offsets 0-14: `container.removeItem(slot, amount)`.
                let (container, index) = slot_state(rt, this)?;
                let amount = arg(args, 0)?.clone();
                vcall(
                    rt,
                    Site::Interface,
                    CONTAINER,
                    "removeItem",
                    D_II_STACK,
                    &container,
                    &[Value::Int(index), amount],
                )
            }
            arm(rt, this, args)
        }
        SlotMayPickup | SlotTrue => Ok(bool_value(true)),
        SlotFalse | ResultSlotMayPlace => Ok(bool_value(false)),
        SlotMayPlace => {
            #[inline(never)]
            fn arm(_rt: &mut Runtime<'_, impl Host>, _this: &Value, _args: &[Value]) -> R<Value> {
                // A slot `menu_models` built with its own predicate (a vanilla subclass Decaf
                // constructs directly) tests it; a plain `Slot` answers `true` (offsets 0-1).
                Ok(bool_value(true))
            }
            arm(rt, this, args)
        }
        SlotOnTake => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                note_take(rt, this)?;
                // 26.3 `Slot.onTake` offsets 0-4: `setChanged()`.
                vcall(rt, Site::Virtual, SLOT, "setChanged", "()V", this, &[])?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        SlotOnQuickCraft2 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `onQuickCraft(ItemStack, ItemStack)` offsets 0-20:
                // `int n = original.getCount() - picked.getCount(); if (n > 0) onQuickCraft(original, n)`.
                let picked = arg(args, 0)?.clone();
                let original = arg(args, 1)?.clone();
                let amount = stack_count(rt, &original)?.wrapping_sub(stack_count(rt, &picked)?);
                if amount > 0 {
                    vcall(
                        rt,
                        Site::Virtual,
                        SLOT,
                        "onQuickCraft",
                        "(Lnet/minecraft/world/item/ItemStack;I)V",
                        this,
                        &[original, Value::Int(amount)],
                    )?;
                }
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        SlotNoOp | ContainerNoOp => Ok(Value::Void),
        SlotTryRemove => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let (count, limit) = (as_int(arg(args, 0)?)?, as_int(arg(args, 1)?)?);
                slot_try_remove(rt, this, count, limit, arg(args, 2)?)
            }
            arm(rt, this, args)
        }
        SlotSafeTake => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let (count, limit) = (as_int(arg(args, 0)?)?, as_int(arg(args, 1)?)?);
                slot_safe_take(rt, this, count, limit, arg(args, 2)?)
            }
            arm(rt, this, args)
        }
        SlotSafeClone => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `safeClone` offsets 0-13: `getItem().copyWithCount(getItem's max stack)`.
                let item = slot_get_item(rt, this)?;
                let max = stack_max_stack_size(rt, &item)?;
                stack_copy_with_count(rt, &item, max)
            }
            arm(rt, this, args)
        }
        SlotSafeInsert1 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `safeInsert(ItemStack)` offsets 0-9: `safeInsert(stack, stack.getCount())`.
                let stack = arg(args, 0)?.clone();
                let count = stack_count(rt, &stack)?;
                slot_safe_insert(rt, this, &stack, count)
            }
            arm(rt, this, args)
        }
        SlotSafeInsert2 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let stack = arg(args, 0)?.clone();
                slot_safe_insert(rt, this, &stack, as_int(arg(args, 1)?)?)
            }
            arm(rt, this, args)
        }
        SlotAllowModification => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `allowModification` offsets 0-24: `mayPickup(p) && mayPlace(getItem())`.
                let player = arg(args, 0)?.clone();
                if !slot_may_pickup(rt, this, &player)? {
                    return Ok(bool_value(false));
                }
                let item = slot_get_item(rt, this)?;
                slot_may_place(rt, this, &item).map(bool_value)
            }
            arm(rt, this, args)
        }
        SlotGetContainerSlot => Ok(Value::Int(slot_state(rt, this)?.1)),
        SlotGetNoItemIcon => Ok(Value::Ref(None)),
        InputSlotMayPlace => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `ItemCombinerMenu$2.mayPlace` offsets 0-13:
                // `val$slot.mayPlace().test(stack)` (the mod's own predicate).
                let stack = arg(args, 0)?.clone();
                let predicate = slot_predicate(rt, this)?;
                let answer = vcall(
                    rt,
                    Site::Interface,
                    "java/util/function/Predicate",
                    "test",
                    "(Ljava/lang/Object;)Z",
                    &predicate,
                    &[stack],
                )?;
                Ok(bool_value(as_bool(&answer)?))
            }
            arm(rt, this, args)
        }
        ResultSlotMayPickup => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `ItemCombinerMenu$3.mayPickup` offsets 0-12:
                // `this$0.mayPickup(player, hasItem())`.
                let player = arg(args, 0)?.clone();
                let has_item = as_bool(&vcall(
                    rt,
                    Site::Virtual,
                    SLOT,
                    "hasItem",
                    "()Z",
                    this,
                    &[],
                )?)?;
                let menu = slot_menu(rt, this)?;
                let answer = vcall(
                    rt,
                    Site::Virtual,
                    ICM,
                    "mayPickup",
                    "(Lnet/minecraft/world/entity/player/Player;Z)Z",
                    &menu,
                    &[player, bool_value(has_item)],
                )?;
                Ok(bool_value(as_bool(&answer)?))
            }
            arm(rt, this, args)
        }
        ResultSlotOnTake => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                note_take(rt, this)?;
                // 26.3 `ItemCombinerMenu$3.onTake` offsets 0-9: `this$0.onTake(player, stack)`.
                let menu = slot_menu(rt, this)?;
                vcall(
                    rt,
                    Site::Virtual,
                    ICM,
                    "onTake",
                    D_PLAYER_STACK_V,
                    &menu,
                    &[arg(args, 0)?.clone(), arg(args, 1)?.clone()],
                )?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        ContainerMaxStackSize0 => Ok(Value::Int(menu_models::DEFAULT_CONTAINER_MAX_STACK_SIZE)),
        ContainerMaxStackSize1 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `Container.getMaxStackSize(ItemStack)` offsets 0-13:
                // `Math.min(getMaxStackSize(), stack.getMaxStackSize())`.
                let stack = arg(args, 0)?.clone();
                let own = as_int(&vcall(
                    rt,
                    Site::Interface,
                    CONTAINER,
                    "getMaxStackSize",
                    "()I",
                    this,
                    &[],
                )?)?;
                let item = stack_max_stack_size(rt, &stack)?;
                Ok(Value::Int(own.min(item)))
            }
            arm(rt, this, args)
        }
        ContainerTrue => Ok(bool_value(true)),
        SimpleGetItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `SimpleContainer.getItem` offsets 0-30: out-of-range -> `EMPTY`.
                let index = as_int(arg(args, 0)?)?;
                let items = container_items(rt, this)?;
                Ok(usize::try_from(index)
                    .ok()
                    .and_then(|i| items.get(i).cloned())
                    .unwrap_or_else(menu_models::empty_stack))
            }
            arm(rt, this, args)
        }
        SimpleSetItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                simple_set_item(rt, this, as_int(arg(args, 0)?)?, arg(args, 1)?)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        SimpleRemoveItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `SimpleContainer.removeItem` offsets 0-22: `ContainerHelper.removeItem(items,
                // slot, count)`, then `setChanged()` if something came out.
                let (index, count) = (as_int(arg(args, 0)?)?, as_int(arg(args, 1)?)?);
                let items = container_items(rt, this)?;
                let taken = helper_remove_item(rt, &items, index, count)?;
                if !stack_is_empty(rt, &taken)? {
                    vcall(
                        rt,
                        Site::Virtual,
                        SIMPLE_CONTAINER,
                        "setChanged",
                        "()V",
                        this,
                        &[],
                    )?;
                }
                Ok(taken)
            }
            arm(rt, this, args)
        }
        SimpleRemoveItemNoUpdate => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `SimpleContainer.removeItemNoUpdate` offsets 0-36.
                let index = as_int(arg(args, 0)?)?;
                let items = container_items(rt, this)?;
                let Some(current) = usize::try_from(index)
                    .ok()
                    .and_then(|i| items.get(i))
                    .cloned()
                else {
                    return Err(array_index_out_of_bounds(index, items.len()));
                };
                if stack_is_empty(rt, &current)? {
                    return Ok(menu_models::empty_stack());
                }
                set_container_item(rt, this, index as usize, menu_models::empty_stack())?;
                Ok(current)
            }
            arm(rt, this, args)
        }
        SimpleSetChanged | ResultSetChanged | InvSetChanged => Ok(Value::Void),
        SimpleGetContainerSize => Ok(Value::Int(container_size(rt, this)?)),
        SimpleIsEmpty | ResultIsEmpty => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // `isEmpty` offsets 0-40 (both): every stack `isEmpty()`.
                for item in container_items(rt, this)? {
                    if !stack_is_empty(rt, &item)? {
                        return Ok(bool_value(false));
                    }
                }
                Ok(bool_value(true))
            }
            arm(rt, this, args)
        }
        SimpleClearContent => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `SimpleContainer.clearContent` offsets 0-11: `items.clear()` (back to
                // `EMPTY`), `setChanged()`.
                let len = container_items(rt, this)?.len();
                for index in 0..len {
                    set_container_item(rt, this, index, menu_models::empty_stack())?;
                }
                vcall(
                    rt,
                    Site::Virtual,
                    SIMPLE_CONTAINER,
                    "setChanged",
                    "()V",
                    this,
                    &[],
                )?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        InputContainerSetChanged => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `ItemCombinerMenu$4.setChanged` offsets 0-12: `super.setChanged()`
                // (`SimpleContainer`'s, empty), then `this$0.slotsChanged(this)`.
                let menu = container_owner(rt, this)?;
                vcall(
                    rt,
                    Site::Virtual,
                    ICM,
                    "slotsChanged",
                    D_CONTAINER_V,
                    &menu,
                    std::slice::from_ref(this),
                )?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        ResultGetItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `ResultContainer.getItem` offsets 0-11: `itemStacks.get(0)`, whatever `slot`.
                Ok(container_items(rt, this)?
                    .first()
                    .cloned()
                    .unwrap_or_else(menu_models::empty_stack))
            }
            arm(rt, this, args)
        }
        ResultSetItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                // 26.3 `ResultContainer.setItem` offsets 0-10: `itemStacks.set(0, stack)` (a
                // `NonNullList`: `null` throws).
                let stack = arg(args, 1)?.clone();
                if stack == Value::Ref(None) {
                    return Err(non_null_list_npe());
                }
                set_container_item(rt, this, 0, stack)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        ResultTakeItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `ResultContainer.removeItem` / `removeItemNoUpdate` offsets 0-8:
                // `ContainerHelper.takeItem(itemStacks, 0)`: the whole stack, `EMPTY` left.
                let current = container_items(rt, this)?
                    .first()
                    .cloned()
                    .unwrap_or_else(menu_models::empty_stack);
                set_container_item(rt, this, 0, menu_models::empty_stack())?;
                Ok(current)
            }
            arm(rt, this, args)
        }
        ResultGetContainerSize => Ok(Value::Int(1)),
        ResultClearContent => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                set_container_item(rt, this, 0, menu_models::empty_stack())?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        ResultSetRecipeUsed => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let recipe = arg(args, 0)?.clone();
                container_recipe_used(rt, this, Some(recipe))?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        ResultGetRecipeUsed => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                Ok(container_recipe_used(rt, this, None)?.unwrap_or(Value::Ref(None)))
            }
            arm(rt, this, args)
        }
        ResultContainerSetChanged => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `ItemCombinerMenu$1.setChanged` offsets 0-8: `this$0.slotsChanged(this)`.
                let menu = container_owner(rt, this)?;
                vcall(
                    rt,
                    Site::Virtual,
                    ICM,
                    "slotsChanged",
                    D_CONTAINER_V,
                    &menu,
                    std::slice::from_ref(this),
                )?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        HolderAwardUsedRecipes => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                holder_award_used_recipes(rt, this, arg(args, 0)?, arg(args, 1)?)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        InvGetItem => inventory_get_item(rt, this, as_int(arg(args, 0)?)?),
        InvSetItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                inventory_set_item(rt, this, as_int(arg(args, 0)?)?, arg(args, 1)?.clone())?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        InvRemoveItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                inventory_remove_item(rt, this, as_int(arg(args, 0)?)?, as_int(arg(args, 1)?)?)
            }
            arm(rt, this, args)
        }
        InvRemoveItemNoUpdate => inventory_remove_item_no_update(rt, this, as_int(arg(args, 0)?)?),
        InvGetContainerSize => {
            #[inline(never)]
            fn arm(_rt: &mut Runtime<'_, impl Host>, _this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `Inventory.getContainerSize`: `items.size() + EQUIPMENT_SLOT_MAPPING.size()`.
                Ok(Value::Int(
                    (menu_models::INVENTORY_ITEMS + menu_models::INVENTORY_EQUIPMENT) as i32,
                ))
            }
            arm(rt, this, args)
        }
        InvAdd1 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let stack = arg(args, 0)?.clone();
                inventory_add(rt, this, -1, &stack).map(bool_value)
            }
            arm(rt, this, args)
        }
        InvAdd2 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let slot = as_int(arg(args, 0)?)?;
                let stack = arg(args, 1)?.clone();
                inventory_add(rt, this, slot, &stack).map(bool_value)
            }
            arm(rt, this, args)
        }
        InvPlaceBack2 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let prediction = Prediction::from_value(arg(args, 1)?).ok_or_else(counted)?;
                inventory_place_back(rt, this, arg(args, 0)?, true, prediction)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        InvPlaceBack3 => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                let send = as_bool(arg(args, 1)?)?;
                let prediction = Prediction::from_value(arg(args, 2)?).ok_or_else(counted)?;
                inventory_place_back(rt, this, arg(args, 0)?, send, prediction)?;
                Ok(Value::Void)
            }
            arm(rt, this, args)
        }
        InvGetSlotWithRemainingSpace => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, args: &[Value]) -> R<Value> {
                inventory_slot_with_remaining_space(rt, this, arg(args, 0)?).map(Value::Int)
            }
            arm(rt, this, args)
        }
        InvGetFreeSlot => inventory_free_slot(rt, this).map(Value::Int),
        InvGetSelectedSlot => Ok(Value::Int(inventory_contents(rt, this)?.selected)),
        InvGetSelectedItem => {
            #[inline(never)]
            fn arm(rt: &mut Runtime<'_, impl Host>, this: &Value, _args: &[Value]) -> R<Value> {
                // 26.3 `getSelectedItem`: `items.get(selected)`.
                let contents = inventory_contents(rt, this)?;
                let selected = contents.selected;
                usize::try_from(selected)
                    .ok()
                    .and_then(|i| contents.items.get(i).cloned())
                    .ok_or_else(|| index_out_of_bounds(selected, contents.items.len()))
            }
            arm(rt, this, args)
        }
        DataSlotCheckAndClear => data_slot_check_and_clear(rt, this).map(bool_value),
        DataSlotContainerGet
        | DataSlotContainerSet
        | DataSlotStandaloneGet
        | DataSlotStandaloneSet => data_slot_model(rt, body, this, args),
    }
}

/// `NonNullList.set(i, null)`: offsets 0-4 `Objects.requireNonNull(element)` (no message).
fn non_null_list_npe() -> Flow {
    npe()
}

// ---------------------------------------------------------------------------
// Menu, slot and container state
// ---------------------------------------------------------------------------

fn key_of(value: &Value) -> R<String> {
    expect_ref(value).cloned().ok_or_else(counted)
}

fn menu_state(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<menu_models::MenuState> {
    let key = key_of(menu)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Menu(state)) => Ok(state.clone()),
        _ => Err(counted()),
    }
}

fn menu_live<T>(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    f: impl FnOnce(&menu_models::LiveState) -> T,
) -> R<T> {
    let key = key_of(menu)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Menu(state)) => Ok(f(&state.live)),
        _ => Err(counted()),
    }
}

fn menu_live_mut(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    f: impl FnOnce(&mut menu_models::LiveState),
) -> R<()> {
    let key = key_of(menu)?;
    match std_host(rt)?.menu_states.get_mut(&key) {
        Some(menu_models::Object::Menu(state)) => {
            f(&mut state.live);
            Ok(())
        }
        _ => Err(counted()),
    }
}

/// `menu.slots`, as slot values in index order.
fn menu_slots(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<Vec<Value>> {
    let state = menu_state(rt, menu)?;
    match std_host(rt)?.menu_states.get(&state.slots) {
        Some(menu_models::Object::SlotList { slots }) => Ok(slots
            .iter()
            .map(|key| menu_models::marker_of(key))
            .collect()),
        _ => Err(counted()),
    }
}

/// `slots.get(index)` (an `ArrayList`: out of range throws).
fn menu_slot(rt: &mut Runtime<'_, impl Host>, menu: &Value, index: i32) -> R<Value> {
    let slots = menu_slots(rt, menu)?;
    usize::try_from(index)
        .ok()
        .and_then(|i| slots.get(i).cloned())
        .ok_or_else(|| index_out_of_bounds(index, slots.len()))
}

fn combiner(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<menu_models::CombinerState> {
    menu_state(rt, menu)?.combiner.ok_or_else(counted)
}

/// A slot's `container` and private `slot` (its container index).
fn slot_state(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<(Value, i32)> {
    if *slot == Value::Ref(None) {
        return Err(npe());
    }
    menu_models::slot_container_and_index(&std_host(rt)?.menu_states, slot).ok_or_else(counted)
}

fn slot_container(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<Value> {
    slot_state(rt, slot).map(|(container, _)| container)
}

/// `Slot.index` (the menu index `addSlot` gave it).
fn slot_index(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<i32> {
    let key = key_of(slot)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Slot(state)) => Ok(state.index()),
        _ => Err(counted()),
    }
}

/// `ItemCombinerMenu$2.val$slot.mayPlace()`: the definition's predicate.
fn slot_predicate(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<Value> {
    let key = key_of(slot)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Slot(state)) => state.may_place().cloned().ok_or_else(counted),
        _ => Err(counted()),
    }
}

/// `ItemCombinerMenu$3.this$0`: the menu that owns the slot's (result) container.
fn slot_menu(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<Value> {
    let container = slot_container(rt, slot)?;
    container_owner(rt, &container)
}

/// `ItemCombinerMenu$1.this$0` / `ItemCombinerMenu$4.this$0`.
fn container_owner(rt: &mut Runtime<'_, impl Host>, container: &Value) -> R<Value> {
    let key = key_of(container)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Container {
            owner: Some(owner), ..
        }) => Ok(menu_models::marker_of(owner)),
        _ => Err(counted()),
    }
}

fn container_items(rt: &mut Runtime<'_, impl Host>, container: &Value) -> R<Vec<Value>> {
    let key = key_of(container)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Container { items, .. }) => Ok(items.clone()),
        _ => Err(counted()),
    }
}

fn container_size(rt: &mut Runtime<'_, impl Host>, container: &Value) -> R<i32> {
    let key = key_of(container)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Container { size, .. }) => Ok(*size),
        _ => Err(counted()),
    }
}

fn set_container_item(
    rt: &mut Runtime<'_, impl Host>,
    container: &Value,
    index: usize,
    value: Value,
) -> R<()> {
    let key = key_of(container)?;
    match std_host(rt)?.menu_states.get_mut(&key) {
        Some(menu_models::Object::Container { items, .. }) => {
            let len = items.len();
            let slot = items
                .get_mut(index)
                .ok_or_else(|| array_index_out_of_bounds(index as i32, len))?;
            *slot = value;
            Ok(())
        }
        _ => Err(counted()),
    }
}

/// `ResultContainer.recipeUsed`: set it (`Some`) or read it (`None`).
fn container_recipe_used(
    rt: &mut Runtime<'_, impl Host>,
    container: &Value,
    set: Option<Value>,
) -> R<Option<Value>> {
    let key = key_of(container)?;
    match std_host(rt)?.menu_states.get_mut(&key) {
        Some(menu_models::Object::Container { recipe_used, .. }) => {
            if let Some(value) = set {
                *recipe_used = (value != Value::Ref(None)).then_some(value);
                Ok(None)
            } else {
                Ok(recipe_used.clone())
            }
        }
        _ => Err(counted()),
    }
}

fn inventory_contents(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
) -> R<menu_models::InventoryContents> {
    let key = key_of(inventory)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Inventory {
            contents: Some(contents),
            ..
        }) => Ok(contents.clone()),
        _ => Err(counted()),
    }
}

fn inventory_player(rt: &mut Runtime<'_, impl Host>, inventory: &Value) -> R<Value> {
    let key = key_of(inventory)?;
    match std_host(rt)?.menu_states.get(&key) {
        Some(menu_models::Object::Inventory { player, .. }) => Ok(player.clone()),
        _ => Err(counted()),
    }
}

fn with_inventory_mut<T>(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    f: impl FnOnce(&mut menu_models::InventoryContents) -> R<T>,
) -> R<T> {
    let key = key_of(inventory)?;
    match std_host(rt)?.menu_states.get_mut(&key) {
        Some(menu_models::Object::Inventory {
            contents: Some(contents),
            ..
        }) => f(contents),
        _ => Err(counted()),
    }
}

// ---------------------------------------------------------------------------
// ItemStack (final in 26.3: no jar override exists, so its model answers directly)
// ---------------------------------------------------------------------------

/// `stack.name(args)` on a server `ItemStack`, answered by `recipe_models`' live-stack model.
fn stack_call(
    rt: &mut Runtime<'_, impl Host>,
    name: &str,
    descriptor: &str,
    stack: &Value,
    args: &[Value],
) -> R<Value> {
    let r = mref(ITEM_STACK, name, descriptor);
    if *stack == Value::Ref(None) {
        return Err(Flow::Java(null_checks::invoke_on_null(&r)));
    }
    let host = std_host(rt)?;
    match recipe_models::instance_call(host, &r, stack, args) {
        Some(Ok(value)) => Ok(value),
        Some(Err(error)) => Err(Flow::Java(error)),
        None => Err(counted()),
    }
}

fn stack_is_empty(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<bool> {
    as_bool(&stack_call(rt, "isEmpty", "()Z", stack, &[])?)
}

fn stack_count(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<i32> {
    as_int(&stack_call(rt, "getCount", "()I", stack, &[])?)
}

fn stack_set_count(rt: &mut Runtime<'_, impl Host>, stack: &Value, count: i32) -> R<()> {
    stack_call(rt, "setCount", "(I)V", stack, &[Value::Int(count)]).map(drop)
}

fn stack_grow(rt: &mut Runtime<'_, impl Host>, stack: &Value, amount: i32) -> R<()> {
    stack_call(rt, "grow", "(I)V", stack, &[Value::Int(amount)]).map(drop)
}

fn stack_shrink(rt: &mut Runtime<'_, impl Host>, stack: &Value, amount: i32) -> R<()> {
    stack_call(rt, "shrink", "(I)V", stack, &[Value::Int(amount)]).map(drop)
}

fn stack_copy(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<Value> {
    stack_call(rt, "copy", D_GET_ITEM, stack, &[])
}

fn stack_copy_with_count(rt: &mut Runtime<'_, impl Host>, stack: &Value, count: i32) -> R<Value> {
    stack_call(rt, "copyWithCount", D_I_STACK, stack, &[Value::Int(count)])
}

fn stack_split(rt: &mut Runtime<'_, impl Host>, stack: &Value, amount: i32) -> R<Value> {
    stack_call(rt, "split", D_I_STACK, stack, &[Value::Int(amount)])
}

fn stack_max_stack_size(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<i32> {
    as_int(&stack_call(rt, "getMaxStackSize", "()I", stack, &[])?)
}

fn stack_is_stackable(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<bool> {
    as_bool(&stack_call(rt, "isStackable", "()Z", stack, &[])?)
}

fn stack_is_damaged(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<bool> {
    as_bool(&stack_call(rt, "isDamaged", "()Z", stack, &[])?)
}

fn stack_limit_size(rt: &mut Runtime<'_, impl Host>, stack: &Value, max: i32) -> R<()> {
    stack_call(rt, "limitSize", "(I)V", stack, &[Value::Int(max)]).map(drop)
}

fn stack_copy_and_clear(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<Value> {
    stack_call(rt, "copyAndClear", D_GET_ITEM, stack, &[])
}

/// `ItemStack.isSameItem(a, b)`: `a.is(b.getItem())`.
fn same_item(rt: &mut Runtime<'_, impl Host>, a: &Value, b: &Value) -> R<bool> {
    let item = stack_call(rt, "getItem", "()Lnet/minecraft/world/item/Item;", b, &[])?;
    as_bool(&stack_call(
        rt,
        "is",
        "(Lnet/minecraft/world/item/Item;)Z",
        a,
        &[item],
    )?)
}

/// `ItemStack.isSameItemSameComponents(a, b)`.
fn same_item_same_components(rt: &mut Runtime<'_, impl Host>, a: &Value, b: &Value) -> R<bool> {
    if *a == Value::Ref(None) || *b == Value::Ref(None) {
        return Err(npe());
    }
    let host = std_host(rt)?;
    match recipe_models::stacks_same_item_same_components(host, a, b) {
        Ok(Some(same)) => Ok(same),
        Ok(None) => Err(counted()),
        Err(error) => Err(Flow::Java(error)),
    }
}

/// `ItemStack.matches(a, b)` offsets 0-25: identity, then count, then
/// `isSameItemSameComponents`.
fn stacks_match(rt: &mut Runtime<'_, impl Host>, a: &Value, b: &Value) -> R<bool> {
    if a == b {
        return Ok(true);
    }
    if stack_count(rt, a)? != stack_count(rt, b)? {
        return Ok(false);
    }
    same_item_same_components(rt, a, b)
}

/// The stack's item, count and components, for an effect the embedder reads after the call.
fn components(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<recipe_models::StackComponents> {
    if *stack == Value::Ref(None) {
        return Err(npe());
    }
    recipe_models::extract_stack_components(std_host(rt)?, stack).ok_or_else(counted)
}

// ---------------------------------------------------------------------------
// The server player (a game object: no jar body can override its methods)
// ---------------------------------------------------------------------------

fn with_active<T>(
    rt: &mut Runtime<'_, impl Host>,
    f: impl FnOnce(&mut ActiveBlockUse) -> R<T>,
) -> R<T> {
    let active = std_host(rt)?
        .block_use
        .active_use
        .as_mut()
        .ok_or_else(counted)?;
    f(active)
}

/// `player instanceof ServerPlayer`, for the player value of the running call.
fn is_server_player(rt: &mut Runtime<'_, impl Host>, player: &Value) -> R<bool> {
    if *player == Value::Ref(None) {
        return Ok(false);
    }
    if game_object::handle_key(player) == Some("client/player") {
        return Ok(false);
    }
    with_active(rt, |active| {
        if *player == active.player {
            Ok(active.side == BlockUseSide::Server)
        } else {
            Err(counted())
        }
    })
}

/// The running call's server player, checked against `player`.
fn server_player(rt: &mut Runtime<'_, impl Host>, player: &Value) -> R<()> {
    if *player == Value::Ref(None) {
        return Err(npe());
    }
    if is_server_player(rt, player)? {
        Ok(())
    } else {
        Err(counted())
    }
}

/// `player.getInventory()`.
fn player_inventory(rt: &mut Runtime<'_, impl Host>, player: &Value) -> R<Value> {
    server_player(rt, player)?;
    with_active(rt, |active| Ok(active.player_inventory.clone()))
}

fn facts(rt: &mut Runtime<'_, impl Host>) -> R<MenuPlayerFacts> {
    with_active(rt, |active| active.facts.clone().ok_or_else(counted))
}

/// The classes a jar can name the server player's `tickCount` through: `Entity` declares the
/// field (`public int tickCount`, `server-26.3.jar`), and javac names the static type of the
/// receiver expression, so a Mixin on `Player` reads `getfield Player.tickCount:I`.
const TICK_COUNT_OWNERS: &[&str] = &[
    "net/minecraft/world/entity/Entity",
    "net/minecraft/world/entity/LivingEntity",
    "net/minecraft/world/entity/Avatar",
    "net/minecraft/world/entity/player/Player",
    SERVER_PLAYER,
];

/// A `getfield` on the server player of the running block use or live-menu call that the
/// embedder's facts answer: `Entity.tickCount` ([`MenuPlayerFacts::tick_count`]). `None`
/// leaves the read to the host's ordinary (counted) path.
#[inline(never)]
pub(crate) fn player_field(host: &StdHost, receiver: &Value, r: &MethodRef) -> Option<Value> {
    if r.name != "tickCount" || r.descriptor != "I" || !TICK_COUNT_OWNERS.contains(&&*r.owner) {
        return None;
    }
    let active = host.block_use.active_use.as_ref()?;
    if active.side != BlockUseSide::Server || *receiver != active.player {
        return None;
    }
    active.facts.as_ref()?.tick_count.map(Value::Int)
}

/// `Player.hasInfiniteMaterials()`: `abilities.instabuild`.
fn has_infinite_materials(rt: &mut Runtime<'_, impl Host>, player: &Value) -> R<bool> {
    server_player(rt, player)?;
    facts(rt)?.instabuild.ok_or_else(counted)
}

fn record(rt: &mut Runtime<'_, impl Host>, effect: LiveMenuEffect) -> R<()> {
    // Carried out after the call returns: one that may draw a lent level
    // stream puts every later lent draw out of order (`level_random.rs`).
    let quiet = draws_no_level_random(&effect);
    with_active(rt, |active| {
        active.live_effects.push(effect);
        Ok(())
    })?;
    std_host(rt)?.note_deferred_effect(quiet);
    Ok(())
}

/// The allow-list of recorded effects whose carrying out draws no level's
/// `Level.random`: packets, stats and notices (`level_random.rs`). Not on it:
/// - `InventoryChangedCriterion`, `RecipeCrafted` and `AwardRecipes`: a
///   criterion (a recipe unlock fires `RECIPE_UNLOCKED`) can complete an
///   advancement at once, and `AdvancementRewards.grant` rolls its loot with
///   `LootContext$Builder.create(Optional.empty())`, which falls back to
///   `ServerLevel.getRandom()`;
/// - `Drop`: in 26.3 a drop draws only entity randoms (`ItemEntity`'s own and
///   the player's, javap), but the embedder's drop path isn't verified to
///   leave the level stream alone.
///
/// A new effect must take a side here.
fn draws_no_level_random(effect: &LiveMenuEffect) -> bool {
    match effect {
        LiveMenuEffect::InitialData { .. }
        | LiveMenuEffect::SlotChanged { .. }
        | LiveMenuEffect::CarriedChanged { .. }
        | LiveMenuEffect::DataChanged { .. }
        | LiveMenuEffect::AwardStat { .. }
        | LiveMenuEffect::InventorySlotSent { .. }
        | LiveMenuEffect::Closed { .. }
        | LiveMenuEffect::SlotTaken { .. } => true,
        LiveMenuEffect::InventoryChangedCriterion { .. }
        | LiveMenuEffect::RecipeCrafted { .. }
        | LiveMenuEffect::AwardRecipes { .. }
        | LiveMenuEffect::Drop { .. } => false,
    }
}

/// `player.drop(stack, retainOwnership, prediction)` (`ServerPlayer.drop` over
/// `LivingEntity.drop` offsets 0-8: an empty stack drops nothing and returns `null`), recorded
/// for the embedder, whose result every call site here pops.
fn player_drop(
    rt: &mut Runtime<'_, impl Host>,
    player: &Value,
    stack: &Value,
    retain_ownership: bool,
    prediction: Prediction,
) -> R<()> {
    server_player(rt, player)?;
    if stack_is_empty(rt, stack)? {
        return Ok(());
    }
    let stack = components(rt, stack)?;
    record(
        rt,
        LiveMenuEffect::Drop {
            stack,
            retain_ownership,
            prediction,
        },
    )
}

// ---------------------------------------------------------------------------
// Slot calls vanilla bodies make (each virtual, so a jar slot's override runs)
// ---------------------------------------------------------------------------

fn slot_get_item(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<Value> {
    let item = vcall(rt, Site::Virtual, SLOT, "getItem", D_GET_ITEM, slot, &[])?;
    if item == Value::Ref(None) {
        // Every vanilla caller dereferences it next.
        return Err(npe());
    }
    Ok(item)
}

fn slot_has_item(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<bool> {
    as_bool(&vcall(
        rt,
        Site::Virtual,
        SLOT,
        "hasItem",
        "()Z",
        slot,
        &[],
    )?)
}

fn slot_may_place(rt: &mut Runtime<'_, impl Host>, slot: &Value, stack: &Value) -> R<bool> {
    as_bool(&vcall(
        rt,
        Site::Virtual,
        SLOT,
        "mayPlace",
        D_STACK_Z,
        slot,
        std::slice::from_ref(stack),
    )?)
}

fn slot_may_pickup(rt: &mut Runtime<'_, impl Host>, slot: &Value, player: &Value) -> R<bool> {
    as_bool(&vcall(
        rt,
        Site::Virtual,
        SLOT,
        "mayPickup",
        D_PLAYER_Z,
        slot,
        std::slice::from_ref(player),
    )?)
}

fn slot_max_stack_size_for(rt: &mut Runtime<'_, impl Host>, slot: &Value, stack: &Value) -> R<i32> {
    as_int(&vcall(
        rt,
        Site::Virtual,
        SLOT,
        "getMaxStackSize",
        D_STACK_I,
        slot,
        std::slice::from_ref(stack),
    )?)
}

fn slot_set_by_player1(rt: &mut Runtime<'_, impl Host>, slot: &Value, stack: &Value) -> R<()> {
    vcall(
        rt,
        Site::Virtual,
        SLOT,
        "setByPlayer",
        D_STACK_V,
        slot,
        std::slice::from_ref(stack),
    )
    .map(drop)
}

fn slot_set_by_player(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    stack: &Value,
    old: &Value,
) -> R<()> {
    vcall(
        rt,
        Site::Virtual,
        SLOT,
        "setByPlayer",
        "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemStack;)V",
        slot,
        &[stack.clone(), old.clone()],
    )
    .map(drop)
}

fn slot_set_changed(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<()> {
    vcall(rt, Site::Virtual, SLOT, "setChanged", "()V", slot, &[]).map(drop)
}

/// `Slot.onTake` began on `slot`: report the take ([`LiveMenuEffect::SlotTaken`]) unless it is
/// the direct `super.onTake` of a jar override already reported: the vanilla body running on
/// that slot in the override's own frame (`BlockUseHostState::taking`). Any other `onTake`
/// inside the override is a take of its own and is told: one on another slot, or the override
/// taking from its own slot again (a nested `onTake` runs in a deeper frame, so its
/// `super.onTake` is told). Every vanilla body's take, and a jar
/// method's `slot.onTake(...)` on a vanilla slot, reaches the vanilla arms; a jar override is
/// reported where `vcall` dispatches to it. (A jar override a jar method calls directly runs in
/// the interpreter and is not seen here; a menu with a jar `Slot` class gets no Sunrise
/// declaration, so the bridge never shows one.)
fn note_take(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<()> {
    let depth = rt.depth;
    let super_call = std_host(rt)?
        .block_use
        .taking
        .iter()
        .any(|(taking, frame)| taking == slot && *frame == depth);
    if super_call {
        return Ok(());
    }
    // No active call (a jar calling `onTake` outside one): nothing to tell.
    if std_host(rt)?.block_use.active_use.is_none() {
        return Ok(());
    }
    let index = slot_index(rt, slot).ok();
    record(rt, LiveMenuEffect::SlotTaken { slot: index })
}

fn slot_on_take(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    player: &Value,
    stack: &Value,
) -> R<()> {
    // The take is reported where `onTake` runs (`note_take`): in the vanilla bodies' arms, or
    // at `vcall`'s dispatch to a jar override.
    vcall(
        rt,
        Site::Virtual,
        SLOT,
        "onTake",
        D_PLAYER_STACK_V,
        slot,
        &[player.clone(), stack.clone()],
    )
    .map(drop)
}

fn slot_safe_insert_call(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    stack: &Value,
    increment: i32,
) -> R<Value> {
    vcall(
        rt,
        Site::Virtual,
        SLOT,
        "safeInsert",
        "(Lnet/minecraft/world/item/ItemStack;I)Lnet/minecraft/world/item/ItemStack;",
        slot,
        &[stack.clone(), Value::Int(increment)],
    )
}

fn slot_safe_take_call(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    count: i32,
    limit: i32,
    player: &Value,
) -> R<Value> {
    vcall(
        rt,
        Site::Virtual,
        SLOT,
        "safeTake",
        "(IILnet/minecraft/world/entity/player/Player;)Lnet/minecraft/world/item/ItemStack;",
        slot,
        &[Value::Int(count), Value::Int(limit), player.clone()],
    )
}

/// `slot.tryRemove(count, limit, player)`, unwrapped from its `Optional`.
fn slot_try_remove_call(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    count: i32,
    limit: i32,
    player: &Value,
) -> R<Option<Value>> {
    let optional = vcall(
        rt,
        Site::Virtual,
        SLOT,
        "tryRemove",
        "(IILnet/minecraft/world/entity/player/Player;)Ljava/util/Optional;",
        slot,
        &[Value::Int(count), Value::Int(limit), player.clone()],
    )?;
    if optional == Value::Ref(None) {
        return Err(npe());
    }
    if !crate::runtime::host_optional::is_optional(&optional) {
        return Err(counted());
    }
    Ok(crate::runtime::host_optional::optional_present(&optional))
}

fn container_get_item(rt: &mut Runtime<'_, impl Host>, container: &Value, index: i32) -> R<Value> {
    vcall(
        rt,
        Site::Interface,
        CONTAINER,
        "getItem",
        D_I_STACK,
        container,
        &[Value::Int(index)],
    )
}

// ---------------------------------------------------------------------------
// Slot bodies
// ---------------------------------------------------------------------------

/// 26.3 `Slot.tryRemove(int, int, Player)` offsets 0-84.
fn slot_try_remove(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    count: i32,
    limit: i32,
    player: &Value,
) -> R<Value> {
    Ok(
        match slot_try_remove_inner(rt, slot, count, limit, player)? {
            Some(stack) => crate::runtime::host_optional::optional_wrap(stack),
            None => crate::runtime::host_optional::empty_optional(),
        },
    )
}

fn slot_try_remove_inner(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    count: i32,
    limit: i32,
    player: &Value,
) -> R<Option<Value>> {
    // 0-11: `if (!mayPickup(player)) return Optional.empty()`.
    if !slot_may_pickup(rt, slot, player)? {
        return Ok(None);
    }
    // 12-34: `if (!allowModification(player) && limit < getItem().getCount()) return empty`.
    let allowed = as_bool(&vcall(
        rt,
        Site::Virtual,
        SLOT,
        "allowModification",
        D_PLAYER_Z,
        slot,
        std::slice::from_ref(player),
    )?)?;
    if !allowed {
        let item = slot_get_item(rt, slot)?;
        if limit < stack_count(rt, &item)? {
            return Ok(None);
        }
    }
    // 35-46: `count = Math.min(count, limit); taken = remove(count)`.
    let count = count.min(limit);
    let taken = vcall(
        rt,
        Site::Virtual,
        SLOT,
        "remove",
        D_I_STACK,
        slot,
        &[Value::Int(count)],
    )?;
    // 48-59: `if (taken.isEmpty()) return empty`.
    if stack_is_empty(rt, &taken)? {
        return Ok(None);
    }
    // 60-79: `if (getItem().isEmpty()) setByPlayer(EMPTY, taken)`.
    let item = slot_get_item(rt, slot)?;
    if stack_is_empty(rt, &item)? {
        slot_set_by_player(rt, slot, &menu_models::empty_stack(), &taken)?;
    }
    Ok(Some(taken))
}

/// 26.3 `Slot.safeTake(int, int, Player)` offsets 0-32: `tryRemove` (virtual), its stack's
/// `onTake` (virtual), `orElse(EMPTY)`.
fn slot_safe_take(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    count: i32,
    limit: i32,
    player: &Value,
) -> R<Value> {
    match slot_try_remove_call(rt, slot, count, limit, player)? {
        Some(taken) => {
            slot_on_take(rt, slot, player, &taken)?;
            Ok(taken)
        }
        None => Ok(menu_models::empty_stack()),
    }
}

/// 26.3 `Slot.safeInsert(ItemStack, int)` offsets 0-98.
fn slot_safe_insert(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    stack: &Value,
    increment: i32,
) -> R<Value> {
    // 0-16: `if (stack.isEmpty() || !mayPlace(stack)) return stack`.
    if stack_is_empty(rt, stack)? || !slot_may_place(rt, slot, stack)? {
        return Ok(stack.clone());
    }
    // 17-43.
    let current = slot_get_item(rt, slot)?;
    let wanted = increment.min(stack_count(rt, stack)?);
    let room = slot_max_stack_size_for(rt, slot, stack)?.wrapping_sub(stack_count(rt, &current)?);
    let amount = wanted.min(room);
    // 45-51.
    if amount <= 0 {
        return Ok(stack.clone());
    }
    if stack_is_empty(rt, &current)? {
        // 52-69: `setByPlayer(stack.split(amount))`.
        let split = stack_split(rt, stack, amount)?;
        slot_set_by_player1(rt, slot, &split)?;
    } else if same_item_same_components(rt, &current, stack)? {
        // 72-94.
        stack_shrink(rt, stack, amount)?;
        stack_grow(rt, &current, amount)?;
        slot_set_by_player1(rt, slot, &current)?;
    }
    Ok(stack.clone())
}

// ---------------------------------------------------------------------------
// AbstractContainerMenu.clicked / doClick
// ---------------------------------------------------------------------------

/// Whether a Java throwable class is an `Exception` (what `clicked`'s handler catches),
/// `None` when Decaf can't tell.
fn is_java_exception(rt: &Runtime<'_, impl Host>, class: &str) -> Option<bool> {
    use crate::runtime::host_ref::ClassHierarchy;
    let mut current = class.to_owned();
    for _ in 0..crate::runtime::MAX_SUPERCLASS_WALK {
        let Some(file) = rt.class(&current) else {
            break;
        };
        if file.super_name.is_empty() || file.super_name == current {
            return None;
        }
        current = file.super_name.clone();
    }
    if current == "java/lang/Exception" {
        return Some(true);
    }
    crate::runtime::host_ref::JdkHierarchy.is_instance(&current, "java/lang/Exception")
}

/// 26.3 `AbstractContainerMenu.clicked` offsets 0-129: `doClick`, and an `Exception` from it
/// becomes `ReportedException(CrashReport.forThrowable(e, "Container click"))`, whose
/// `getMessage()` is the report's title.
fn acm_clicked(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    slot: i32,
    button: i32,
    input: ContainerInput,
    player: &Value,
) -> R<()> {
    match acm_do_click(rt, menu, slot, button, input, player) {
        Err(Flow::Java(error @ NativeError::UncaughtException { .. })) => {
            let NativeError::UncaughtException { class, .. } = &error else {
                unreachable!("matched an uncaught exception")
            };
            let class = class.clone();
            match is_java_exception(rt, &class) {
                Some(true) => {
                    note_wrapped_click_cause(rt, &error);
                    Err(throw(REPORTED_EXCEPTION, CONTAINER_CLICK.to_owned()))
                }
                Some(false) => Err(Flow::Java(NativeError::UncaughtException {
                    class,
                    message: None,
                    frame: None,
                })),
                None => Err(counted()),
            }
        }
        other => other,
    }
}

/// `ReportedException` and the crash report title `clicked` gives it.
const REPORTED_EXCEPTION: &str = "net/minecraft/ReportedException";
const CONTAINER_CLICK: &str = "Container click";

/// Keep the exception `clicked` wrapped on the running call, for its failure line (vanilla's
/// `ReportedException.getCause()`): nothing Java reads. Outside a live call (no active use)
/// there is no failure line to name it in.
fn note_wrapped_click_cause(rt: &mut Runtime<'_, impl Host>, error: &NativeError) {
    if let Some(host) = rt.host.as_std_host_mut()
        && let Some(active) = host.block_use.active_use.as_mut()
    {
        active.wrapped_click_cause = Some(error.to_string());
    }
}

/// A live call's failure line: the error, and when it is `clicked`'s `ReportedException`, the
/// exception that handler wrapped (`caused by ...`), as a crash report's `Caused by:` names it.
fn failure_line(error: &NativeError, wrapped_click_cause: Option<&str>) -> String {
    match (error, wrapped_click_cause) {
        (
            NativeError::UncaughtException {
                class,
                message: Some(message),
                ..
            },
            Some(cause),
        ) if class == REPORTED_EXCEPTION && message == CONTAINER_CLICK => {
            format!("{error}; caused by {cause}")
        }
        _ => error.to_string(),
    }
}

/// `getCarried()` (virtual: a menu may override it).
fn get_carried(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<Value> {
    let carried = vcall(rt, Site::Virtual, ACM, "getCarried", D_GET_ITEM, menu, &[])?;
    if carried == Value::Ref(None) {
        return Err(npe());
    }
    Ok(carried)
}

fn set_carried(rt: &mut Runtime<'_, impl Host>, menu: &Value, stack: &Value) -> R<()> {
    vcall(
        rt,
        Site::Virtual,
        ACM,
        "setCarried",
        D_STACK_V,
        menu,
        std::slice::from_ref(stack),
    )
    .map(drop)
}

fn reset_quick_craft(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<()> {
    vcall(rt, Site::Virtual, ACM, "resetQuickCraft", "()V", menu, &[]).map(drop)
}

/// 26.3 `AbstractContainerMenu.isValidQuickcraftType(int, Player)` offsets 0-28.
fn is_valid_quickcraft_type(rt: &mut Runtime<'_, impl Host>, kind: i32, player: &Value) -> R<bool> {
    Ok(match kind {
        0 | 1 => true,
        2 => has_infinite_materials(rt, player)?,
        _ => false,
    })
}

/// 26.3 `AbstractContainerMenu.canItemQuickReplace(Slot, ItemStack, boolean)` offsets 0-66.
fn can_item_quick_replace(
    rt: &mut Runtime<'_, impl Host>,
    slot: &Value,
    stack: &Value,
    stack_size_matters: bool,
) -> R<bool> {
    let empty = *slot == Value::Ref(None) || !slot_has_item(rt, slot)?;
    if !empty {
        let item = slot_get_item(rt, slot)?;
        if same_item_same_components(rt, stack, &item)? {
            let item = slot_get_item(rt, slot)?;
            let extra = if stack_size_matters {
                0
            } else {
                stack_count(rt, stack)?
            };
            return Ok(
                stack_count(rt, &item)?.wrapping_add(extra) <= stack_max_stack_size(rt, stack)?
            );
        }
    }
    Ok(empty)
}

/// 26.3 `AbstractContainerMenu.getQuickCraftPlaceCount(int, int, ItemStack)` offsets 0-57.
fn quick_craft_place_count(
    rt: &mut Runtime<'_, impl Host>,
    slot_count: i32,
    kind: i32,
    stack: &Value,
) -> R<i32> {
    Ok(match kind {
        // `Mth.floor((float) stack.getCount() / (float) slotCount)`.
        0 => mth_floor(stack_count(rt, stack)? as f32 / slot_count as f32),
        1 => 1,
        2 => stack_max_stack_size(rt, stack)?,
        _ => stack_count(rt, stack)?,
    })
}

/// 26.3 `Mth.floor(float)`: `int i = (int) f; return f < i ? i - 1 : i;` (Java's `(int)`
/// saturates and maps NaN to 0, as Rust's `as` does).
fn mth_floor(value: f32) -> i32 {
    let truncated = value as i32;
    if value < truncated as f32 {
        truncated.wrapping_sub(1)
    } else {
        truncated
    }
}

/// 26.3 `AbstractContainerMenu.doClick(int, int, ContainerInput, Player)` offsets 0-1877.
fn acm_do_click(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    slot_id: i32,
    button: i32,
    input: ContainerInput,
    player: &Value,
) -> R<()> {
    // 0-5: `Inventory inventory = player.getInventory()`.
    let inventory = player_inventory(rt, player)?;
    if input == ContainerInput::QuickCraft {
        return quick_craft(rt, menu, slot_id, button, player);
    }
    // 547-558: any other input while a drag is under way cancels the drag.
    if menu_live(rt, menu, |live| live.quickcraft_status)? != 0 {
        return reset_quick_craft(rt, menu);
    }
    let container_button = button == 0 || button == 1;
    if matches!(input, ContainerInput::Pickup | ContainerInput::QuickMove) && container_button {
        let primary = button == 0;
        return pickup_or_quick_move(rt, menu, slot_id, input, primary, player);
    }
    match input {
        // 1074-1380.
        ContainerInput::Swap if (0..9).contains(&button) || button == 40 => {
            swap(rt, menu, slot_id, button, &inventory, player)
        }
        // 1383-1444: `player.hasInfiniteMaterials() && getCarried().isEmpty() && slotId >= 0`.
        ContainerInput::Clone => {
            if has_infinite_materials(rt, player)? {
                let carried = get_carried(rt, menu)?;
                if stack_is_empty(rt, &carried)? && slot_id >= 0 {
                    let slot = menu_slot(rt, menu, slot_id)?;
                    if slot_has_item(rt, &slot)? {
                        let cloned = vcall(
                            rt,
                            Site::Virtual,
                            SLOT,
                            "safeClone",
                            "(Lnet/minecraft/world/entity/player/Player;)Lnet/minecraft/world/item/ItemStack;",
                            &slot,
                            std::slice::from_ref(player),
                        )?;
                        set_carried(rt, menu, &cloned)?;
                    }
                }
            }
            Ok(())
        }
        ContainerInput::Throw => throw_click(rt, menu, slot_id, button, player),
        ContainerInput::PickupAll => pickup_all(rt, menu, slot_id, button, player),
        _ => Ok(()),
    }
}

/// `doClick` offsets 7-544: the three-phase drag.
fn quick_craft(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    slot_id: i32,
    button: i32,
    player: &Value,
) -> R<()> {
    // 14-25: `int previous = quickcraftStatus; quickcraftStatus = getQuickcraftHeader(button)`.
    let previous = menu_live(rt, menu, |live| live.quickcraft_status)?;
    let status = button & 3;
    menu_live_mut(rt, menu, |live| live.quickcraft_status = status)?;
    // 28-55: a header out of sequence resets.
    if !(previous == 1 && status == 2) && previous != status {
        return reset_quick_craft(rt, menu);
    }
    // 58-72.
    let carried = get_carried(rt, menu)?;
    if stack_is_empty(rt, &carried)? {
        return reset_quick_craft(rt, menu);
    }
    match status {
        // 75-123: start: `quickcraftType = getQuickcraftType(button)`.
        0 => {
            let kind = (button >> 2) & 3;
            menu_live_mut(rt, menu, |live| live.quickcraft_type = kind)?;
            if is_valid_quickcraft_type(rt, kind, player)? {
                menu_live_mut(rt, menu, |live| {
                    live.quickcraft_status = 1;
                    live.quickcraft_slots.clear();
                })
            } else {
                reset_quick_craft(rt, menu)
            }
        }
        // 126-220: add a slot.
        1 => {
            let slot = menu_slot(rt, menu, slot_id)?;
            let carried = get_carried(rt, menu)?;
            if can_item_quick_replace(rt, &slot, &carried, true)?
                && slot_may_place(rt, &slot, &carried)?
            {
                let (kind, size) = menu_live(rt, menu, |live| {
                    (live.quickcraft_type, live.quickcraft_slots.len())
                })?;
                if (kind == 2 || stack_count(rt, &carried)? as i64 > size as i64)
                    && as_bool(&vcall(
                        rt,
                        Site::Virtual,
                        ACM,
                        "canDragTo",
                        D_SLOT_Z,
                        menu,
                        std::slice::from_ref(&slot),
                    )?)?
                {
                    // `quickcraftSlots.add(slot)`: a `HashSet`, so identity dedups.
                    menu_live_mut(rt, menu, |live| {
                        if !live.quickcraft_slots.contains(&slot) {
                            live.quickcraft_slots.push(slot.clone());
                        }
                    })?;
                }
            }
            Ok(())
        }
        // 223-537: end.
        2 => quick_craft_end(rt, menu, player),
        // 540: any other header.
        _ => reset_quick_craft(rt, menu),
    }
}

/// `doClick` offsets 231-537: place the drag.
fn quick_craft_end(rt: &mut Runtime<'_, impl Host>, menu: &Value, player: &Value) -> R<()> {
    let (slots, kind) = menu_live(rt, menu, |live| {
        (live.quickcraft_slots.clone(), live.quickcraft_type)
    })?;
    if slots.is_empty() {
        // 533.
        return reset_quick_craft(rt, menu);
    }
    if slots.len() == 1 {
        // 256-297: one slot is an ordinary pickup click on it, with the drag type as button.
        let index = slot_index(rt, &slots[0])?;
        reset_quick_craft(rt, menu)?;
        let kind = menu_live(rt, menu, |live| live.quickcraft_type)?;
        return acm_do_click(rt, menu, index, kind, ContainerInput::Pickup, player);
    }
    // 298-319.
    let carried = get_carried(rt, menu)?;
    let source = stack_copy(rt, &carried)?;
    if stack_is_empty(rt, &source)? {
        return reset_quick_craft(rt, menu);
    }
    // 320-327.
    let carried = get_carried(rt, menu)?;
    let mut remaining = stack_count(rt, &carried)?;
    // 329-517: Java iterates `quickcraftSlots`, a `HashSet` of `Slot`s, in identity-hash order,
    // which nothing can reproduce. Each slot's placement depends only on that slot and the
    // unchanged carried stack, so the order is unobservable unless something besides the
    // slots' own writes happens in between: jar code running, a synchronizer send, or two
    // slots writing one container index. Run in insertion order and count if any did.
    let instructions_before = rt.instructions;
    let effects_before = with_active(rt, |active| Ok(active.live_effects.len()))?;
    let mut targets = Vec::new();
    for slot in &slots {
        let target = slot_state(rt, slot)?;
        if targets.contains(&target) {
            return Err(counted());
        }
        targets.push(target);
    }
    let size = slots.len() as i32;
    for slot in &slots {
        let carried = get_carried(rt, menu)?;
        if !can_item_quick_replace(rt, slot, &carried, true)?
            || !slot_may_place(rt, slot, &carried)?
        {
            continue;
        }
        if kind != 2 && stack_count(rt, &carried)? < size {
            continue;
        }
        if !as_bool(&vcall(
            rt,
            Site::Virtual,
            ACM,
            "canDragTo",
            D_SLOT_Z,
            menu,
            std::slice::from_ref(slot),
        )?)? {
            continue;
        }
        // 428-514.
        let existing = if slot_has_item(rt, slot)? {
            let item = slot_get_item(rt, slot)?;
            stack_count(rt, &item)?
        } else {
            0
        };
        let max =
            stack_max_stack_size(rt, &source)?.min(slot_max_stack_size_for(rt, slot, &source)?);
        let place = quick_craft_place_count(rt, size, kind, &source)?
            .wrapping_add(existing)
            .min(max);
        remaining = remaining.wrapping_sub(place.wrapping_sub(existing));
        let placed = stack_copy_with_count(rt, &source, place)?;
        slot_set_by_player1(rt, slot, &placed)?;
    }
    let effects_after = with_active(rt, |active| Ok(active.live_effects.len()))?;
    if rt.instructions != instructions_before || effects_after != effects_before {
        return Err(counted());
    }
    // 520-534.
    stack_set_count(rt, &source, remaining)?;
    set_carried(rt, menu, &source)?;
    reset_quick_craft(rt, menu)
}

/// `doClick` offsets 575-1071: `PICKUP` / `QUICK_MOVE` with button 0 or 1.
fn pickup_or_quick_move(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    slot_id: i32,
    input: ContainerInput,
    primary: bool,
    player: &Value,
) -> R<()> {
    if slot_id == -999 {
        // 588-655: outside the window drops the carried stack (all, or one).
        let carried = get_carried(rt, menu)?;
        if !stack_is_empty(rt, &carried)? {
            if primary {
                let carried = get_carried(rt, menu)?;
                player_drop(rt, player, &carried, true, Prediction::Predicted)?;
                set_carried(rt, menu, &menu_models::empty_stack())?;
            } else {
                let carried = get_carried(rt, menu)?;
                let one = stack_split(rt, &carried, 1)?;
                player_drop(rt, player, &one, true, Prediction::Predicted)?;
            }
        }
        return Ok(());
    }
    if input == ContainerInput::QuickMove {
        // 658-736.
        if slot_id < 0 {
            return Ok(());
        }
        let slot = menu_slot(rt, menu, slot_id)?;
        if !slot_may_pickup(rt, &slot, player)? {
            return Ok(());
        }
        let mut moved = quick_move_stack(rt, menu, player, slot_id)?;
        loop {
            if stack_is_empty(rt, &moved)? {
                break;
            }
            let item = slot_get_item(rt, &slot)?;
            if !same_item(rt, &item, &moved)? {
                break;
            }
            moved = quick_move_stack(rt, menu, player, slot_id)?;
        }
        return Ok(());
    }
    // 739-1066: `PICKUP`.
    if slot_id < 0 {
        return Ok(());
    }
    let slot = menu_slot(rt, menu, slot_id)?;
    let slot_item = slot_get_item(rt, &slot)?;
    let carried = get_carried(rt, menu)?;
    // 770-781: `player.updateTutorialInventoryAction(carried, slot.getItem(), action)`: the
    // second `getItem()` still runs; `ServerPlayer` keeps `Player`'s empty body.
    slot_get_item(rt, &slot)?;
    server_player(rt, player)?;
    if !try_item_click_behaviour_override(rt, menu, player, primary, &slot, &slot_item, &carried)? {
        if stack_is_empty(rt, &slot_item)? {
            // 801-849.
            if !stack_is_empty(rt, &carried)? {
                let amount = if primary {
                    stack_count(rt, &carried)?
                } else {
                    1
                };
                let rest = slot_safe_insert_call(rt, &slot, &carried, amount)?;
                set_carried(rt, menu, &rest)?;
            }
        } else if slot_may_pickup(rt, &slot, player)? {
            if stack_is_empty(rt, &carried)? {
                // 862-926: take all, or half rounded up.
                let count = stack_count(rt, &slot_item)?;
                let amount = if primary {
                    count
                } else {
                    count.wrapping_add(1) / 2
                };
                if let Some(taken) = slot_try_remove_call(rt, &slot, amount, i32::MAX, player)? {
                    // `lambda$doClick$0`: `setCarried(stack); slot.onTake(player, stack)`.
                    set_carried(rt, menu, &taken)?;
                    slot_on_take(rt, &slot, player, &taken)?;
                }
            } else if slot_may_place(rt, &slot, &carried)? {
                if same_item_same_components(rt, &slot_item, &carried)? {
                    // 949-981.
                    let amount = if primary {
                        stack_count(rt, &carried)?
                    } else {
                        1
                    };
                    let rest = slot_safe_insert_call(rt, &slot, &carried, amount)?;
                    set_carried(rt, menu, &rest)?;
                } else if stack_count(rt, &carried)?
                    <= slot_max_stack_size_for(rt, &slot, &carried)?
                {
                    // 984-1012: swap the carried stack with the slot's.
                    set_carried(rt, menu, &slot_item)?;
                    slot_set_by_player1(rt, &slot, &carried)?;
                }
            } else if same_item_same_components(rt, &slot_item, &carried)? {
                // 1015-1063: fill the carried stack from the slot.
                let count = stack_count(rt, &slot_item)?;
                let room =
                    stack_max_stack_size(rt, &carried)?.wrapping_sub(stack_count(rt, &carried)?);
                if let Some(taken) = slot_try_remove_call(rt, &slot, count, room, player)? {
                    // `lambda$doClick$1`: `carried.grow(stack.getCount()); slot.onTake(...)`.
                    let taken_count = stack_count(rt, &taken)?;
                    stack_grow(rt, &carried, taken_count)?;
                    slot_on_take(rt, &slot, player, &taken)?;
                }
            }
        }
    }
    // 1066-1068.
    slot_set_changed(rt, &slot)
}

/// `quickMoveStack(player, slotId)` (virtual: the mod's, or `ItemCombinerMenu`'s).
fn quick_move_stack(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    player: &Value,
    slot_id: i32,
) -> R<Value> {
    let moved = vcall(
        rt,
        Site::Virtual,
        ACM,
        "quickMoveStack",
        "(Lnet/minecraft/world/entity/player/Player;I)Lnet/minecraft/world/item/ItemStack;",
        menu,
        &[player.clone(), Value::Int(slot_id)],
    )?;
    if moved == Value::Ref(None) {
        return Err(npe());
    }
    Ok(moved)
}

/// Java 25's listing of the 26.3 items whose class overrides a stacking or crafting hook:
/// `bundle <id>` for `BundleItem` (`overrideStackedOnOther` / `overrideOtherStackedOnMe`) and
/// `map <id>` for `MapItem` (`onCraftedPostProcess`), captured by
/// `tests/fixtures/live_menu/oracle/CaptureItemHooks.java`. javap over the server jar shows
/// no other `Item` subclass declares any of those or `onCraftedBy`.
const ITEM_HOOKS_JAVA25: &str = crate::runtime::vendored_live_menu_item_hooks_java25_txt::VENDORED_DATA;

/// Whether the vanilla item `id` is listed under `kind` in [`ITEM_HOOKS_JAVA25`].
fn item_hook(kind: &str, id: &str) -> bool {
    ITEM_HOOKS_JAVA25
        .lines()
        .filter_map(|line| line.split_once(' '))
        .any(|(k, item)| k == kind && item == id)
}

/// Whether `stack`'s `Item.overrideStackedOnOther` / `overrideOtherStackedOnMe` is certainly
/// `Item`'s `return false` (offsets 0-1): an empty stack's `isItemEnabled` is `false` first; a
/// vanilla item that is not a bundle keeps `Item`'s body; an item a jar built (`new Item(..)`
/// or a jar subclass) keeps it unless a jar class on its chain overrides either method or its
/// vanilla class is `BundleItem`. A bundle, or a jar override (which would also need the
/// level's enabled features for `isItemEnabled`), counts.
fn click_override_is_false(rt: &mut Runtime<'_, impl Host>, stack: &Value) -> R<bool> {
    if stack_is_empty(rt, stack)? {
        return Ok(true);
    }
    let item = stack_call(
        rt,
        "getItem",
        "()Lnet/minecraft/world/item/Item;",
        stack,
        &[],
    )?;
    if jar_object::object_class(&item).is_some() {
        for (name, descriptor) in [
            (
                "overrideStackedOnOther",
                "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/inventory/Slot;Lnet/minecraft/world/inventory/ClickAction;Lnet/minecraft/world/entity/player/Player;)Z",
            ),
            (
                "overrideOtherStackedOnMe",
                "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/inventory/Slot;Lnet/minecraft/world/inventory/ClickAction;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/SlotAccess;)Z",
            ),
        ] {
            let r = mref("net/minecraft/world/item/Item", name, descriptor);
            if rt.jvm_selected_jar_body(&r, 0xb6, Some(&item))?.is_some() {
                return Ok(false);
            }
        }
        let class = vanilla_class_of(rt, &item).ok_or_else(counted)?;
        return Ok(class != "net/minecraft/world/item/BundleItem");
    }
    let host = std_host(rt)?;
    let Some(id) = recipe_models::resolve_item_id(host, &item) else {
        return Ok(false);
    };
    Ok(crate::runtime::vanilla_items::is_vanilla_item_id(&id) && !item_hook("bundle", &id))
}

/// 26.3 `AbstractContainerMenu.tryItemClickBehaviourOverride` offsets 0-64: the carried
/// stack's `overrideStackedOnOther(slot, action, player)`, then the slot stack's
/// `overrideOtherStackedOnMe(carried, slot, action, player, createCarriedSlotAccess())`, each
/// behind `isItemEnabled(level.enabledFeatures())`. `Item`'s own `false` is answered without
/// running anything; a jar item's override runs here, exactly where vanilla runs it, with the
/// cursor's `SlotAccess` over this menu (`item_parts.rs`). A vanilla bundle's override, a jar
/// item whose required features may be off, or an override that counted, counts.
#[allow(clippy::too_many_arguments)]
fn try_item_click_behaviour_override(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    player: &Value,
    primary: bool,
    slot: &Value,
    slot_item: &Value,
    carried: &Value,
) -> R<bool> {
    use crate::runtime::item_parts::{ItemHook, item_enabled_in_any_level, jar_override_owner};
    let action = Value::Ref(Some(format!(
        "enum/net/minecraft/world/inventory/ClickAction/{}",
        if primary { "PRIMARY" } else { "SECONDARY" }
    )));
    for (stack, hook) in [
        (carried, ItemHook::StackedOnOther),
        (slot_item, ItemHook::OtherStackedOnMe),
    ] {
        if click_override_is_false(rt, stack)? {
            continue;
        }
        // `isItemEnabled(level.enabledFeatures())`: an item outside the vanilla flag needs the
        // level's features, which the embedder doesn't give.
        if item_enabled_in_any_level(std_host(rt)?, stack) != Some(true) {
            return Err(counted());
        }
        let item = stack_call(
            rt,
            "getItem",
            "()Lnet/minecraft/world/item/Item;",
            stack,
            &[],
        )?;
        // A vanilla bundle's own override isn't run here.
        let Some(owner) = jar_override_owner(rt, &item, hook)? else {
            return Err(counted());
        };
        let (name, descriptor) = hook.method();
        let handled = match hook {
            ItemHook::StackedOnOther => exact(rt, |rt| {
                Ok(rt.invoke(
                    &owner,
                    name,
                    descriptor,
                    Some(item.clone()),
                    &[stack.clone(), slot.clone(), action.clone(), player.clone()],
                )?)
            })?,
            _ => {
                let access = crate::runtime::item_parts::menu_carried_access(menu);
                exact(rt, |rt| {
                    Ok(rt.invoke(
                        &owner,
                        name,
                        descriptor,
                        Some(item.clone()),
                        &[
                            stack.clone(),
                            carried.clone(),
                            slot.clone(),
                            action.clone(),
                            player.clone(),
                            access.handle.clone(),
                        ],
                    )?)
                })?
            }
        };
        if as_bool(&handled)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `doClick` offsets 1081-1380: `SWAP` with a hotbar slot or the offhand.
fn swap(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    slot_id: i32,
    button: i32,
    inventory: &Value,
    player: &Value,
) -> R<()> {
    let inventory_item = inventory_get_item_call(rt, inventory, button)?;
    let slot = menu_slot(rt, menu, slot_id)?;
    let slot_item = slot_get_item(rt, &slot)?;
    let inv_empty = stack_is_empty(rt, &inventory_item)?;
    let slot_empty = stack_is_empty(rt, &slot_item)?;
    if inv_empty && slot_empty {
        return Ok(());
    }
    if inv_empty {
        // 1152-1197.
        if slot_may_pickup(rt, &slot, player)? {
            inventory_set_item_call(rt, inventory, button, &slot_item)?;
            let count = stack_count(rt, &slot_item)?;
            vcall(
                rt,
                Site::Virtual,
                SLOT,
                "onSwapCraft",
                "(I)V",
                &slot,
                &[Value::Int(count)],
            )?;
            slot_set_by_player1(rt, &slot, &menu_models::empty_stack())?;
            slot_on_take(rt, &slot, player, &slot_item)?;
        }
        return Ok(());
    }
    if slot_empty {
        // 1208-1268.
        if slot_may_place(rt, &slot, &inventory_item)? {
            let max = slot_max_stack_size_for(rt, &slot, &inventory_item)?;
            if stack_count(rt, &inventory_item)? > max {
                let split = stack_split(rt, &inventory_item, max)?;
                slot_set_by_player1(rt, &slot, &split)?;
            } else {
                inventory_set_item_call(rt, inventory, button, &menu_models::empty_stack())?;
                slot_set_by_player1(rt, &slot, &inventory_item)?;
            }
        }
        return Ok(());
    }
    // 1271-1377.
    if slot_may_pickup(rt, &slot, player)? && slot_may_place(rt, &slot, &inventory_item)? {
        let max = slot_max_stack_size_for(rt, &slot, &inventory_item)?;
        if stack_count(rt, &inventory_item)? > max {
            let split = stack_split(rt, &inventory_item, max)?;
            slot_set_by_player1(rt, &slot, &split)?;
            slot_on_take(rt, &slot, player, &slot_item)?;
            if !inventory_add_call(rt, inventory, &slot_item)? {
                player_drop(rt, player, &slot_item, true, Prediction::Predicted)?;
            }
        } else {
            inventory_set_item_call(rt, inventory, button, &slot_item)?;
            slot_set_by_player1(rt, &slot, &inventory_item)?;
            slot_on_take(rt, &slot, player, &slot_item)?;
        }
    }
    Ok(())
}

/// `doClick` offsets 1454-1612: `THROW` (button 0: one item, 1: the whole stack, again while
/// the slot refills with the same item).
fn throw_click(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    slot_id: i32,
    button: i32,
    player: &Value,
) -> R<()> {
    let carried = get_carried(rt, menu)?;
    if !stack_is_empty(rt, &carried)? || slot_id < 0 {
        return Ok(());
    }
    let slot = menu_slot(rt, menu, slot_id)?;
    let amount = if button == 0 {
        1
    } else {
        let item = slot_get_item(rt, &slot)?;
        stack_count(rt, &item)?
    };
    // `Player.canDropItems()` is `true` (`ServerPlayer` keeps it), and
    // `handleCreativeModeItemDrop` is `Player`'s empty body.
    server_player(rt, player)?;
    let mut taken = slot_safe_take_call(rt, &slot, amount, i32::MAX, player)?;
    player_drop(rt, player, &taken, true, Prediction::Predicted)?;
    if button == 1 {
        loop {
            if stack_is_empty(rt, &taken)? {
                break;
            }
            let item = slot_get_item(rt, &slot)?;
            if !same_item(rt, &item, &taken)? {
                break;
            }
            taken = slot_safe_take_call(rt, &slot, amount, i32::MAX, player)?;
            player_drop(rt, player, &taken, true, Prediction::Predicted)?;
        }
    }
    Ok(())
}

/// `doClick` offsets 1622-1877: `PICKUP_ALL` (double-click): gather matching stacks into the
/// carried one, partial stacks first.
fn pickup_all(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    slot_id: i32,
    button: i32,
    player: &Value,
) -> R<()> {
    if slot_id < 0 {
        return Ok(());
    }
    let slot = menu_slot(rt, menu, slot_id)?;
    let carried = get_carried(rt, menu)?;
    if stack_is_empty(rt, &carried)? {
        return Ok(());
    }
    if slot_has_item(rt, &slot)? && slot_may_pickup(rt, &slot, player)? {
        return Ok(());
    }
    let size = menu_slots(rt, menu)?.len() as i32;
    let start = if button == 0 { 0 } else { size.wrapping_sub(1) };
    let step = if button == 0 { 1 } else { -1 };
    for pass in 0..2 {
        let mut index = start;
        while index >= 0 && index < size {
            if stack_count(rt, &carried)? >= stack_max_stack_size(rt, &carried)? {
                break;
            }
            let other = menu_slot(rt, menu, index)?;
            if slot_has_item(rt, &other)?
                && can_item_quick_replace(rt, &other, &carried, true)?
                && slot_may_pickup(rt, &other, player)?
                && as_bool(&vcall(
                    rt,
                    Site::Virtual,
                    ACM,
                    "canTakeItemForPickAll",
                    "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/inventory/Slot;)Z",
                    menu,
                    &[carried.clone(), other.clone()],
                )?)?
            {
                let item = slot_get_item(rt, &other)?;
                if pass != 0 || stack_count(rt, &item)? != stack_max_stack_size(rt, &item)? {
                    let count = stack_count(rt, &item)?;
                    let room = stack_max_stack_size(rt, &carried)?
                        .wrapping_sub(stack_count(rt, &carried)?);
                    let taken = slot_safe_take_call(rt, &other, count, room, player)?;
                    let taken_count = stack_count(rt, &taken)?;
                    stack_grow(rt, &carried, taken_count)?;
                }
            }
            index = index.wrapping_add(step);
        }
    }
    Ok(())
}

/// 26.3 `AbstractContainerMenu.moveItemStackTo(ItemStack, int, int, boolean)` offsets 0-330.
fn acm_move_item_stack_to(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    stack: &Value,
    start: i32,
    end: i32,
    reverse: bool,
) -> R<bool> {
    let mut moved = false;
    let mut index = if reverse { end.wrapping_sub(1) } else { start };
    let in_range = |index: i32| {
        if reverse { index >= start } else { index < end }
    };
    // 16-190: merge into matching stacks first, when the stack can stack.
    if stack_is_stackable(rt, stack)? {
        while !stack_is_empty(rt, stack)? && in_range(index) {
            let slot = menu_slot(rt, menu, index)?;
            let target = slot_get_item(rt, &slot)?;
            if !stack_is_empty(rt, &target)? && same_item_same_components(rt, stack, &target)? {
                let total = stack_count(rt, &target)?.wrapping_add(stack_count(rt, stack)?);
                let max = slot_max_stack_size_for(rt, &slot, &target)?;
                if total <= max {
                    stack_set_count(rt, stack, 0)?;
                    stack_set_count(rt, &target, total)?;
                    slot_set_changed(rt, &slot)?;
                    moved = true;
                } else if stack_count(rt, &target)? < max {
                    let room = max.wrapping_sub(stack_count(rt, &target)?);
                    stack_shrink(rt, stack, room)?;
                    stack_set_count(rt, &target, max)?;
                    slot_set_changed(rt, &slot)?;
                    moved = true;
                }
            }
            index = index.wrapping_add(if reverse { -1 } else { 1 });
        }
    }
    // 193-325: then the first empty slot that accepts it.
    if !stack_is_empty(rt, stack)? {
        index = if reverse { end.wrapping_sub(1) } else { start };
        while in_range(index) {
            let slot = menu_slot(rt, menu, index)?;
            let target = slot_get_item(rt, &slot)?;
            if stack_is_empty(rt, &target)? && slot_may_place(rt, &slot, stack)? {
                let max = slot_max_stack_size_for(rt, &slot, stack)?;
                let amount = stack_count(rt, stack)?.min(max);
                let split = stack_split(rt, stack, amount)?;
                slot_set_by_player1(rt, &slot, &split)?;
                slot_set_changed(rt, &slot)?;
                moved = true;
                break;
            }
            index = index.wrapping_add(if reverse { -1 } else { 1 });
        }
    }
    Ok(moved)
}

fn move_item_stack_to_call(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    stack: &Value,
    start: i32,
    end: i32,
    reverse: bool,
) -> R<bool> {
    as_bool(&vcall(
        rt,
        Site::Virtual,
        ACM,
        "moveItemStackTo",
        "(Lnet/minecraft/world/item/ItemStack;IIZ)Z",
        menu,
        &[
            stack.clone(),
            Value::Int(start),
            Value::Int(end),
            bool_value(reverse),
        ],
    )?)
}

/// `getResultSlot()` (virtual), as each of `ItemCombinerMenu`'s private range helpers calls it
/// (`getInventorySlotStart` = it + 1, `getInventorySlotEnd` = start + 27, `getUseRowStart` =
/// that end, `getUseRowEnd` = row start + 9).
fn result_slot_call(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<i32> {
    as_int(&vcall(
        rt,
        Site::Virtual,
        ICM,
        "getResultSlot",
        "()I",
        menu,
        &[],
    )?)
}

/// 26.3 `ItemCombinerMenu.quickMoveStack(Player, int)` offsets 0-291. Each range helper the
/// bytecode calls re-reads `getResultSlot()`, in the bytecode's order.
fn icm_quick_move_stack(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    player: &Value,
    index: i32,
) -> R<Value> {
    let empty = menu_models::empty_stack();
    // 0-27: `ItemStack copy = EMPTY; Slot slot = slots.get(index); if (slot != null &&
    // slot.hasItem())`.
    let slot = menu_slot(rt, menu, index)?;
    if !slot_has_item(rt, &slot)? {
        return Ok(empty);
    }
    // 30-42.
    let stack = slot_get_item(rt, &slot)?;
    let copy = stack_copy(rt, &stack)?;
    // 43-53: `int start = getInventorySlotStart(); int end = getUseRowEnd();`.
    let inventory_start = result_slot_call(rt, menu)?.wrapping_add(1);
    let use_row_end = result_slot_call(rt, menu)?.wrapping_add(37);
    if index == result_slot_call(rt, menu)? {
        // 63-89: the result goes to the player's inventory, last slot first.
        if !move_item_stack_to_call(rt, menu, &stack, inventory_start, use_row_end, true)? {
            return Ok(empty);
        }
        vcall(
            rt,
            Site::Virtual,
            SLOT,
            "onQuickCraft",
            "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemStack;)V",
            &slot,
            &[stack.clone(), copy.clone()],
        )?;
    } else if index >= 0 && index < result_slot_call(rt, menu)? {
        // 92-121: an input goes to the player's inventory.
        if !move_item_stack_to_call(rt, menu, &stack, inventory_start, use_row_end, false)? {
            return Ok(empty);
        }
    } else if as_bool(&vcall(
        rt,
        Site::Virtual,
        ICM,
        "canMoveIntoInputSlots",
        D_STACK_Z,
        menu,
        std::slice::from_ref(&stack),
    )?)? && index >= result_slot_call(rt, menu)?.wrapping_add(1)
        && index < result_slot_call(rt, menu)?.wrapping_add(37)
    {
        // 122-165: the player's inventory goes to the inputs.
        let end = result_slot_call(rt, menu)?;
        if !move_item_stack_to_call(rt, menu, &stack, 0, end, false)? {
            return Ok(empty);
        }
    } else if index >= result_slot_call(rt, menu)?.wrapping_add(1)
        && index < result_slot_call(rt, menu)?.wrapping_add(28)
    {
        // 166-203: main inventory to the hotbar.
        let row_start = result_slot_call(rt, menu)?.wrapping_add(28);
        let row_end = result_slot_call(rt, menu)?.wrapping_add(37);
        if !move_item_stack_to_call(rt, menu, &stack, row_start, row_end, false)? {
            return Ok(empty);
        }
    } else if index >= result_slot_call(rt, menu)?.wrapping_add(28)
        && index < result_slot_call(rt, menu)?.wrapping_add(37)
    {
        // 204-241: hotbar to the main inventory.
        let main_start = result_slot_call(rt, menu)?.wrapping_add(1);
        let main_end = result_slot_call(rt, menu)?.wrapping_add(28);
        if !move_item_stack_to_call(rt, menu, &stack, main_start, main_end, false)? {
            return Ok(empty);
        }
    }
    // 242-265.
    if stack_is_empty(rt, &stack)? {
        slot_set_by_player1(rt, &slot, &empty)?;
    } else {
        slot_set_changed(rt, &slot)?;
    }
    // 266-281.
    if stack_count(rt, &stack)? == stack_count(rt, &copy)? {
        return Ok(empty);
    }
    // 282-291.
    slot_on_take(rt, &slot, player, &stack)?;
    Ok(copy)
}

// ---------------------------------------------------------------------------
// removed / clearContainer / stillValid
// ---------------------------------------------------------------------------

/// 26.3 `AbstractContainerMenu.removed(Player)` offsets 0-32: a server player gets the carried
/// stack back (`dropOrPlaceInInventory`), and the cursor empties.
fn acm_removed(rt: &mut Runtime<'_, impl Host>, menu: &Value, player: &Value) -> R<()> {
    if !is_server_player(rt, player)? {
        return Ok(());
    }
    let carried = get_carried(rt, menu)?;
    if !stack_is_empty(rt, &carried)? {
        drop_or_place_in_inventory(rt, player, &carried)?;
        set_carried(rt, menu, &menu_models::empty_stack())?;
    }
    Ok(())
}

/// 26.3 `AbstractContainerMenu.dropOrPlaceInInventory(Player, ItemStack)` offsets 0-74: a
/// player that left (removed for a reason other than a dimension change, or disconnected) drops
/// it; any other gets it back in the inventory.
fn drop_or_place_in_inventory(
    rt: &mut Runtime<'_, impl Host>,
    player: &Value,
    stack: &Value,
) -> R<()> {
    if !is_server_player(rt, player)? {
        return Ok(());
    }
    // An empty stack is a no-op either way (`LivingEntity.drop` offsets 0-8 return `null`
    // before anything, and `placeItemBackInInventory`'s loop never starts), and the player
    // state reads before it change nothing, so it needs no facts.
    if stack_is_empty(rt, stack)? {
        return Ok(());
    }
    let facts = facts(rt)?;
    let removed = facts.removed && !facts.removed_for_dimension_change;
    if removed || facts.disconnected {
        player_drop(rt, player, stack, false, Prediction::ServerOnly)
    } else {
        let inventory = player_inventory(rt, player)?;
        vcall(
            rt,
            Site::Virtual,
            INVENTORY,
            "placeItemBackInInventory",
            "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/util/Prediction;)V",
            &inventory,
            &[stack.clone(), Prediction::ServerOnly.to_value()],
        )
        .map(drop)
    }
}

/// 26.3 `AbstractContainerMenu.clearContainer(Player, Container)` offsets 0-29.
fn acm_clear_container(
    rt: &mut Runtime<'_, impl Host>,
    player: &Value,
    container: &Value,
) -> R<()> {
    let mut index = 0;
    loop {
        let size = as_int(&vcall(
            rt,
            Site::Interface,
            CONTAINER,
            "getContainerSize",
            "()I",
            container,
            &[],
        )?)?;
        if index >= size {
            return Ok(());
        }
        let stack = vcall(
            rt,
            Site::Interface,
            CONTAINER,
            "removeItemNoUpdate",
            D_I_STACK,
            container,
            &[Value::Int(index)],
        )?;
        drop_or_place_in_inventory(rt, player, &stack)?;
        index += 1;
    }
}

/// The level and position of a combiner's `ContainerLevelAccess`, `Some(None)` for
/// `ContainerLevelAccess.NULL`, `None` for one Decaf doesn't model (a jar's own).
fn access_level_pos(
    rt: &mut Runtime<'_, impl Host>,
    access: &Value,
) -> R<Option<Option<(Value, [i32; 3])>>> {
    if *access == Value::Ref(None) {
        return Err(npe());
    }
    if expect_ref(access).is_some_and(|key| key == "container-level-access/null") {
        return Ok(Some(None));
    }
    let key = key_of(access)?;
    let host = std_host(rt)?;
    match host.menu_states.get(&key) {
        Some(menu_models::Object::ContainerLevelAccess { level, .. }) => {
            let level = level.clone();
            Ok(
                menu_models::container_level_access_pos(&host.menu_states, access)
                    .map(|pos| Some((level, pos))),
            )
        }
        _ => Ok(None),
    }
}

/// 26.3 `ItemCombinerMenu.removed(Player)` offsets 0-21: `super.removed(player)`, then
/// `access.execute((level, pos) -> clearContainer(player, inputSlots))`.
fn icm_removed(rt: &mut Runtime<'_, impl Host>, menu: &Value, player: &Value) -> R<()> {
    special(rt, Body::AcmRemoved, menu, std::slice::from_ref(player))?;
    let combiner = combiner(rt, menu)?;
    match access_level_pos(rt, &combiner.access)? {
        // `ContainerLevelAccess.NULL.execute` does nothing.
        Some(None) => Ok(()),
        // `ContainerLevelAccess$2.evaluate` applies the lambda to its level and position:
        // `lambda$removed$0` is `this.clearContainer(player, this.inputSlots)` (virtual).
        Some(Some(_)) => {
            let input = menu_models::marker_of(&combiner.input);
            vcall(
                rt,
                Site::Virtual,
                ACM,
                "clearContainer",
                "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/Container;)V",
                menu,
                &[player.clone(), input],
            )
            .map(drop)
        }
        None => Err(counted()),
    }
}

/// 26.3 `ItemCombinerMenu.stillValid(Player)` offsets 0-26: `access.evaluate((level, pos) ->
/// isValidBlock(level.getBlockState(pos)) ? player.isWithinBlockInteractionRange(pos, 4.0) :
/// false, true)`.
fn icm_still_valid(rt: &mut Runtime<'_, impl Host>, menu: &Value, player: &Value) -> R<bool> {
    let combiner = combiner(rt, menu)?;
    let (level, pos) = match access_level_pos(rt, &combiner.access)? {
        Some(None) => return Ok(true),
        Some(Some(found)) => found,
        None => return Err(counted()),
    };
    let pos_value = crate::runtime::client_frame::block_pos_value(pos);
    let state = vcall(
        rt,
        Site::Virtual,
        "net/minecraft/world/level/Level",
        "getBlockState",
        "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;",
        &level,
        std::slice::from_ref(&pos_value),
    )?;
    let valid = as_bool(&vcall(
        rt,
        Site::Virtual,
        ICM,
        "isValidBlock",
        "(Lnet/minecraft/world/level/block/state/BlockState;)Z",
        menu,
        &[state],
    )?)?;
    if !valid {
        return Ok(false);
    }
    within_block_interaction_range(rt, player, pos, 4.0)
}

/// 26.3 `Player.isWithinBlockInteractionRange(BlockPos, double)` offsets 0-37:
/// `new AABB(pos).distanceToSqr(getEyePosition()) < (blockInteractionRange() + extra)^2`.
fn within_block_interaction_range(
    rt: &mut Runtime<'_, impl Host>,
    player: &Value,
    pos: [i32; 3],
    extra: f64,
) -> R<bool> {
    server_player(rt, player)?;
    let facts = facts(rt)?;
    let (Some(eye), Some(range)) = (facts.eye_position, facts.block_interaction_range) else {
        return Err(counted());
    };
    let reach = range + extra;
    Ok(aabb_distance_to_sqr(pos, eye) < reach * reach)
}

/// 26.3 `AABB.distanceToSqr(Vec3)` for `new AABB(pos)` (the unit cube at `pos`):
/// `dx = max(max(minX - x, x - maxX), 0)` per axis, summed squares.
fn aabb_distance_to_sqr(pos: [i32; 3], point: [f64; 3]) -> f64 {
    let mut sum = 0.0;
    for axis in 0..3 {
        let min = f64::from(pos[axis]);
        let max = min + 1.0;
        let d = (min - point[axis]).max(point[axis] - max).max(0.0);
        sum += d * d;
    }
    sum
}

// ---------------------------------------------------------------------------
// broadcastChanges / sendAllDataToRemote and the server player's listener + synchronizer
// ---------------------------------------------------------------------------

/// Decaf's `ServerPlayer.containerListener` (`ServerPlayer$2`): the listener `initMenu` adds.
pub(crate) const SERVER_PLAYER_LISTENER: &str = "live-menu/server-player-container-listener";
/// Decaf's `ServerPlayer.containerSynchronizer` (`ServerPlayer$1`).
pub(crate) const SERVER_PLAYER_SYNCHRONIZER: &str = "live-menu/server-player-synchronizer";

fn server_player_listener() -> Value {
    Value::Ref(Some(SERVER_PLAYER_LISTENER.to_owned()))
}

fn server_player_synchronizer() -> Value {
    Value::Ref(Some(SERVER_PLAYER_SYNCHRONIZER.to_owned()))
}

/// The memoized `Suppliers.memoize(stack::copy)` `broadcastChanges` builds per slot.
struct Memo {
    stack: Value,
    copy: Option<Value>,
}

impl Memo {
    fn get(&mut self, rt: &mut Runtime<'_, impl Host>) -> R<Value> {
        if let Some(copy) = &self.copy {
            return Ok(copy.clone());
        }
        let copy = stack_copy(rt, &self.stack)?;
        self.copy = Some(copy.clone());
        Ok(copy)
    }
}

fn last_slot_value(last: &menu_models::LastSlot) -> R<Value> {
    match last {
        menu_models::LastSlot::Empty => Ok(menu_models::empty_stack()),
        menu_models::LastSlot::Stack(stack) => Ok(stack.clone()),
        _ => Err(counted()),
    }
}

/// 26.3 `triggerSlotListeners(int, ItemStack, Supplier)` offsets 0-91.
fn trigger_slot_listeners(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    index: usize,
    stack: &Value,
    memo: &mut Memo,
) -> R<()> {
    let last = menu_state(rt, menu)?
        .last_slots
        .get(index)
        .cloned()
        .unwrap_or(menu_models::LastSlot::Empty);
    let last = last_slot_value(&last)?;
    if stacks_match(rt, &last, stack)? {
        return Ok(());
    }
    let copy = memo.get(rt)?;
    let entry = if copy == menu_models::empty_stack() {
        menu_models::LastSlot::Empty
    } else {
        menu_models::LastSlot::Stack(copy.clone())
    };
    let key = key_of(menu)?;
    menu_models::menu_set_last_slot(
        &mut std_host(rt)?.menu_states,
        &Value::Ref(Some(key)),
        index,
        entry,
    );
    let listeners = menu_models::menu_listeners(&std_host(rt)?.menu_states, menu);
    for listener in listeners {
        if listener == server_player_listener() {
            server_player_slot_changed(rt, menu, index as i32, &copy)?;
        } else {
            vcall(
                rt,
                Site::Interface,
                "net/minecraft/world/inventory/ContainerListener",
                "slotChanged",
                "(Lnet/minecraft/world/inventory/AbstractContainerMenu;ILnet/minecraft/world/item/ItemStack;)V",
                &listener,
                &[menu.clone(), Value::Int(index as i32), copy.clone()],
            )?;
        }
    }
    Ok(())
}

/// 26.3 `ServerPlayer$2.slotChanged` offsets 0-49: for a slot of the player's own inventory
/// (not a `ResultSlot`), `CriteriaTriggers.INVENTORY_CHANGED.trigger(player, inventory, stack)`.
fn server_player_slot_changed(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    index: i32,
    stack: &Value,
) -> R<()> {
    let slot = vcall(
        rt,
        Site::Virtual,
        ACM,
        "getSlot",
        "(I)Lnet/minecraft/world/inventory/Slot;",
        menu,
        &[Value::Int(index)],
    )?;
    if slot == Value::Ref(None) {
        // `instanceof` of null is false; `slot.container` then throws.
        return Err(npe());
    }
    // `slot instanceof ResultSlot`: Decaf's own slots are `Slot` / `ItemCombinerMenu$2` /
    // `ItemCombinerMenu$3`, none a `ResultSlot`; a jar slot's chain must leave at `Slot`.
    match vanilla_class_of(rt, &slot).as_deref() {
        Some(SLOT | ICM_INPUT_SLOT | ICM_RESULT_SLOT) => {}
        _ => return Err(counted()),
    }
    let container = slot_container(rt, &slot)?;
    let player = with_active(rt, |active| Ok(active.player.clone()))?;
    let inventory = player_inventory(rt, &player)?;
    if container == inventory {
        let stack = components(rt, stack)?;
        record(rt, LiveMenuEffect::InventoryChangedCriterion { stack })?;
    }
    Ok(())
}

enum RemoteTarget {
    Slot(i32),
    Carried,
}

/// `RemoteSlot.matches(stack)`: `PLACEHOLDER` matches anything; a synchronized slot matches
/// its last forced copy (`ItemStack.matches`), and nothing before its first force.
fn remote_matches(
    rt: &mut Runtime<'_, impl Host>,
    remote: &menu_models::RemoteSlot,
    stack: &Value,
) -> R<bool> {
    match remote {
        menu_models::RemoteSlot::Placeholder => Ok(true),
        menu_models::RemoteSlot::Synchronized(None) => Ok(false),
        menu_models::RemoteSlot::Synchronized(Some(copy)) => stacks_match(rt, copy, stack),
    }
}

fn remote_slot(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    target: &RemoteTarget,
) -> R<menu_models::RemoteSlot> {
    menu_live(rt, menu, |live| match target {
        RemoteTarget::Slot(index) => usize::try_from(*index)
            .ok()
            .and_then(|i| live.remote_slots.get(i).cloned())
            .ok_or(*index),
        RemoteTarget::Carried => Ok(live.remote_carried.clone()),
    })?
    .map_err(|index| index_out_of_bounds(index, 0))
}

/// `RemoteSlot.force(stack)`: `PLACEHOLDER` ignores it; a synchronized slot keeps a copy.
fn remote_force(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    target: RemoteTarget,
    stack: &Value,
) -> R<()> {
    let remote = remote_slot(rt, menu, &target)?;
    if remote == menu_models::RemoteSlot::Placeholder {
        return Ok(());
    }
    let copy = stack_copy(rt, stack)?;
    menu_live_mut(rt, menu, |live| {
        let entry = menu_models::RemoteSlot::Synchronized(Some(copy));
        match target {
            RemoteTarget::Slot(index) => {
                if let Some(slot) = live.remote_slots.get_mut(index as usize) {
                    *slot = entry;
                }
            }
            RemoteTarget::Carried => live.remote_carried = entry,
        }
    })
}

/// `incrementStateId()` (virtual): `stateId = stateId + 1 & 32767`.
fn acm_increment_state_id(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<i32> {
    let (known, id) = menu_live(rt, menu, |live| (live.state_id_known, live.state_id))?;
    if !known {
        return Err(counted());
    }
    let next = (id + 1) & 32767;
    menu_live_mut(rt, menu, |live| live.state_id = next)?;
    Ok(next)
}

/// `menu.incrementStateId()` as `ServerPlayer$1` calls it (virtual): a jar override runs; else
/// the model bumps `stateId`, `None` when its value is no longer known (not counted: the
/// number only reaches the embedder's packet, which the embedder numbers itself).
fn increment_state_id_call(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<Option<i32>> {
    let r = mref(ACM, "incrementStateId", "()I");
    if rt.jvm_selected_jar_body(&r, 0xb6, Some(menu))?.is_some() {
        return Ok(Some(as_int(&vcall(
            rt,
            Site::Virtual,
            ACM,
            "incrementStateId",
            "()I",
            menu,
            &[],
        )?)?));
    }
    let (known, id) = menu_live(rt, menu, |live| (live.state_id_known, live.state_id))?;
    let next = (id + 1) & 32767;
    menu_live_mut(rt, menu, |live| live.state_id = next)?;
    Ok(known.then_some(next))
}

fn menu_container_id(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<i32> {
    Ok(menu_state(rt, menu)?.container_id)
}

fn synchronizer(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<Option<Value>> {
    let sync = menu_live(rt, menu, |live| live.synchronizer.clone())?;
    match sync {
        None => Ok(None),
        Some(sync) if sync == server_player_synchronizer() => Ok(Some(sync)),
        // A synchronizer a jar implements.
        Some(_) => Err(counted()),
    }
}

/// 26.3 `synchronizeSlotToRemote(int, ItemStack, Supplier)` offsets 0-67, with
/// `ServerPlayer$1.sendSlotChange` (offsets 0-27) as the synchronizer.
fn synchronize_slot_to_remote(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    index: usize,
    stack: &Value,
    memo: &mut Memo,
) -> R<()> {
    if menu_live(rt, menu, |live| live.suppress_remote_updates)? {
        return Ok(());
    }
    let remote = remote_slot(rt, menu, &RemoteTarget::Slot(index as i32))?;
    if remote_matches(rt, &remote, stack)? {
        return Ok(());
    }
    remote_force(rt, menu, RemoteTarget::Slot(index as i32), stack)?;
    if synchronizer(rt, menu)?.is_some() {
        let copy = memo.get(rt)?;
        let container_id = menu_container_id(rt, menu)?;
        let state_id = increment_state_id_call(rt, menu)?;
        let stack = components(rt, &copy)?;
        record(
            rt,
            LiveMenuEffect::SlotChanged {
                container_id,
                state_id,
                slot: index as i32,
                stack,
            },
        )?;
    }
    Ok(())
}

/// 26.3 `synchronizeCarriedToRemote()` offsets 0-57, with `ServerPlayer$1.sendCarriedChange`.
fn synchronize_carried_to_remote(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<()> {
    if menu_live(rt, menu, |live| live.suppress_remote_updates)? {
        return Ok(());
    }
    let carried = get_carried(rt, menu)?;
    let remote = remote_slot(rt, menu, &RemoteTarget::Carried)?;
    if remote_matches(rt, &remote, &carried)? {
        return Ok(());
    }
    remote_force(rt, menu, RemoteTarget::Carried, &carried)?;
    if synchronizer(rt, menu)?.is_some() {
        let copy = stack_copy(rt, &carried)?;
        let stack = components(rt, &copy)?;
        record(rt, LiveMenuEffect::CarriedChanged { stack })?;
    }
    Ok(())
}

/// 26.3 `synchronizeDataSlotToRemote(int, int)` offsets 0-55, with
/// `ServerPlayer$1.sendDataChange` -> `broadcastDataValue`.
fn synchronize_data_slot_to_remote(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    index: usize,
    value: i32,
) -> R<()> {
    if menu_live(rt, menu, |live| live.suppress_remote_updates)? {
        return Ok(());
    }
    let old = menu_live(rt, menu, |live| live.remote_data_slots.get(index).copied())?
        .ok_or_else(counted)?;
    if old == value {
        return Ok(());
    }
    menu_live_mut(rt, menu, |live| live.remote_data_slots[index] = value)?;
    if synchronizer(rt, menu)?.is_some() {
        let container_id = menu_container_id(rt, menu)?;
        record(
            rt,
            LiveMenuEffect::DataChanged {
                container_id,
                index: index as i32,
                value,
            },
        )?;
    }
    Ok(())
}

/// 26.3 `updateDataSlotListeners(int, int)` offsets 0-43: each listener's `dataChanged`
/// (`ServerPlayer$2`'s is empty).
fn update_data_slot_listeners(
    rt: &mut Runtime<'_, impl Host>,
    menu: &Value,
    index: usize,
    value: i32,
) -> R<()> {
    for listener in menu_models::menu_listeners(&std_host(rt)?.menu_states, menu) {
        if listener == server_player_listener() {
            continue;
        }
        vcall(
            rt,
            Site::Interface,
            "net/minecraft/world/inventory/ContainerListener",
            "dataChanged",
            "(Lnet/minecraft/world/inventory/AbstractContainerMenu;II)V",
            &listener,
            &[menu.clone(), Value::Int(index as i32), Value::Int(value)],
        )?;
    }
    Ok(())
}

fn data_slot_get(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<i32> {
    as_int(&vcall(
        rt,
        Site::Virtual,
        DATA_SLOT,
        "get",
        "()I",
        slot,
        &[],
    )?)
}

fn data_slot_check(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<bool> {
    as_bool(&vcall(
        rt,
        Site::Virtual,
        DATA_SLOT,
        "checkAndClearUpdateFlag",
        "()Z",
        slot,
        &[],
    )?)
}

/// 26.3 `AbstractContainerMenu.broadcastChanges()` offsets 0-126.
fn acm_broadcast_changes(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<()> {
    let slots = menu_slots(rt, menu)?;
    for (index, slot) in slots.iter().enumerate() {
        let stack = vcall(rt, Site::Virtual, SLOT, "getItem", D_GET_ITEM, slot, &[])?;
        // 28-33: `Objects.requireNonNull(stack)` for the memoized `stack::copy`.
        if stack == Value::Ref(None) {
            return Err(npe());
        }
        let mut memo = Memo {
            stack: stack.clone(),
            copy: None,
        };
        trigger_slot_listeners(rt, menu, index, &stack, &mut memo)?;
        synchronize_slot_to_remote(rt, menu, index, &stack, &mut memo)?;
    }
    synchronize_carried_to_remote(rt, menu)?;
    let data_slots = menu_live(rt, menu, |live| live.data_slots.clone())?;
    for (index, entry) in data_slots.iter().enumerate() {
        let value = data_slot_get(rt, &entry.slot)?;
        if data_slot_check(rt, &entry.slot)? {
            update_data_slot_listeners(rt, menu, index, value)?;
        }
        synchronize_data_slot_to_remote(rt, menu, index, value)?;
    }
    Ok(())
}

/// 26.3 `AbstractContainerMenu.broadcastFullState()` offsets 0-106.
fn acm_broadcast_full_state(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<()> {
    let slots = menu_slots(rt, menu)?;
    for (index, slot) in slots.iter().enumerate() {
        let stack = vcall(rt, Site::Virtual, SLOT, "getItem", D_GET_ITEM, slot, &[])?;
        if stack == Value::Ref(None) {
            return Err(npe());
        }
        let mut memo = Memo {
            stack: stack.clone(),
            copy: None,
        };
        trigger_slot_listeners(rt, menu, index, &stack, &mut memo)?;
    }
    let data_slots = menu_live(rt, menu, |live| live.data_slots.clone())?;
    for (index, entry) in data_slots.iter().enumerate() {
        if data_slot_check(rt, &entry.slot)? {
            let value = data_slot_get(rt, &entry.slot)?;
            update_data_slot_listeners(rt, menu, index, value)?;
        }
    }
    vcall(
        rt,
        Site::Virtual,
        ACM,
        "sendAllDataToRemote",
        "()V",
        menu,
        &[],
    )
    .map(drop)
}

/// 26.3 `AbstractContainerMenu.sendAllDataToRemote()` offsets 0-180, with
/// `ServerPlayer$1.sendInitialData` (offsets 0-56) as the synchronizer.
fn acm_send_all_data_to_remote(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<()> {
    let slots = menu_slots(rt, menu)?;
    let mut copies = Vec::with_capacity(slots.len());
    for (index, slot) in slots.iter().enumerate() {
        let stack = slot_get_item(rt, slot)?;
        copies.push(stack_copy(rt, &stack)?);
        remote_force(rt, menu, RemoteTarget::Slot(index as i32), &stack)?;
    }
    let carried = get_carried(rt, menu)?;
    remote_force(rt, menu, RemoteTarget::Carried, &carried)?;
    let data_slots = menu_live(rt, menu, |live| live.data_slots.clone())?;
    for (index, entry) in data_slots.iter().enumerate() {
        let value = data_slot_get(rt, &entry.slot)?;
        menu_live_mut(rt, menu, |live| {
            if let Some(remote) = live.remote_data_slots.get_mut(index) {
                *remote = value;
            }
        })?;
    }
    if synchronizer(rt, menu)?.is_some() {
        let carried_copy = stack_copy(rt, &carried)?;
        let data = menu_live(rt, menu, |live| live.remote_data_slots.clone())?;
        let container_id = menu_container_id(rt, menu)?;
        let state_id = increment_state_id_call(rt, menu)?;
        let mut items = Vec::with_capacity(copies.len());
        for copy in &copies {
            items.push(components(rt, copy)?);
        }
        let carried = components(rt, &carried_copy)?;
        record(
            rt,
            LiveMenuEffect::InitialData {
                container_id,
                state_id,
                items,
                carried,
            },
        )?;
        for (index, value) in data.into_iter().enumerate() {
            record(
                rt,
                LiveMenuEffect::DataChanged {
                    container_id,
                    index: index as i32,
                    value,
                },
            )?;
        }
    }
    Ok(())
}

/// 26.3 `AbstractContainerMenu.addSlotListener(ContainerListener)` offsets 0-29: unless
/// `containerListeners.contains(listener)`, add it and `broadcastChanges()` (virtual).
fn acm_add_slot_listener(rt: &mut Runtime<'_, impl Host>, menu: &Value, listener: &Value) -> R<()> {
    let existing = menu_models::menu_listeners(&std_host(rt)?.menu_states, menu);
    for other in &existing {
        // `ArrayList.contains` calls `listener.equals(other)`: identity for Decaf's listener
        // (`ServerPlayer$2` keeps `Object`'s) and for a jar listener without its own `equals`.
        if *listener == server_player_listener() {
            if other == listener {
                return Ok(());
            }
            continue;
        }
        let equal = vcall(
            rt,
            Site::Virtual,
            "java/lang/Object",
            "equals",
            "(Ljava/lang/Object;)Z",
            listener,
            std::slice::from_ref(other),
        )?;
        if as_bool(&equal)? {
            return Ok(());
        }
    }
    menu_models::menu_add_slot_listener(&mut std_host(rt)?.menu_states, menu, listener)
        .ok_or_else(counted)?;
    vcall(rt, Site::Virtual, ACM, "broadcastChanges", "()V", menu, &[]).map(drop)
}

/// `addSlotListener(containerListener)` with the server player's own listener, as `initMenu`
/// adds it.
fn add_server_player_listener(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<()> {
    acm_add_slot_listener(rt, menu, &server_player_listener())
}

/// 26.3 `AbstractContainerMenu.setSynchronizer(ContainerSynchronizer)` offsets 0-32 with the
/// server player's synchronizer: every remote slot becomes a fresh synchronized one, then
/// `sendAllDataToRemote()` (virtual).
fn set_server_player_synchronizer(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<()> {
    menu_live_mut(rt, menu, |live| {
        live.synchronizer = Some(server_player_synchronizer());
        live.remote_carried = menu_models::RemoteSlot::Synchronized(None);
        for remote in &mut live.remote_slots {
            *remote = menu_models::RemoteSlot::Synchronized(None);
        }
    })?;
    vcall(
        rt,
        Site::Virtual,
        ACM,
        "sendAllDataToRemote",
        "()V",
        menu,
        &[],
    )
    .map(drop)
}

/// `ServerPlayer.initMenu(menu)` offsets 0-16: `menu.addSlotListener(containerListener)`,
/// `menu.setSynchronizer(containerSynchronizer)`, each virtual. A menu that overrides either
/// runs its own body with Decaf's listener/synchronizer markers, which only Decaf's bodies
/// understand, so an override is counted.
fn init_menu(rt: &mut Runtime<'_, impl Host>, menu: &Value) -> R<()> {
    for (name, descriptor) in [
        (
            "addSlotListener",
            "(Lnet/minecraft/world/inventory/ContainerListener;)V",
        ),
        (
            "setSynchronizer",
            "(Lnet/minecraft/world/inventory/ContainerSynchronizer;)V",
        ),
    ] {
        if rt
            .jvm_selected_jar_body(&mref(ACM, name, descriptor), 0xb6, Some(menu))?
            .is_some()
        {
            return Err(counted());
        }
    }
    add_server_player_listener(rt, menu)?;
    set_server_player_synchronizer(rt, menu)
}

// ---------------------------------------------------------------------------
// Data slots
// ---------------------------------------------------------------------------

/// 26.3 `addDataSlot(DataSlot)` offsets 0-23.
fn acm_add_data_slot(rt: &mut Runtime<'_, impl Host>, menu: &Value, slot: &Value) -> R<()> {
    if *slot == Value::Ref(None) {
        // `List.add(null)` succeeds; the next `broadcastChanges` throws on it.
        return Err(counted());
    }
    let duplicate = menu_live(rt, menu, |live| {
        live.data_slots.iter().any(|entry| entry.slot == *slot)
    })?;
    if duplicate {
        // Both entries would share the one object's `prevValue`; Decaf keeps it per object
        // too, but a second menu holding it is not tracked: count rather than guess.
        return Err(counted());
    }
    menu_live_mut(rt, menu, |live| {
        live.data_slots
            .push(menu_models::DataSlotEntry { slot: slot.clone() });
        live.remote_data_slots.push(0);
    })
}

/// 26.3 `addDataSlots(ContainerData)` offsets 0-28: `addDataSlot(DataSlot.forContainer(data,
/// i))` (virtual) for each `i < data.getCount()` (interface).
fn acm_add_data_slots(rt: &mut Runtime<'_, impl Host>, menu: &Value, data: &Value) -> R<()> {
    let mut index = 0;
    loop {
        let count = as_int(&vcall(
            rt,
            Site::Interface,
            CONTAINER_DATA,
            "getCount",
            "()I",
            data,
            &[],
        )?)?;
        if index >= count {
            return Ok(());
        }
        let slot = new_data_slot(
            rt,
            menu_models::DataSlotKind::Container {
                data: data.clone(),
                index,
            },
        )?;
        vcall(
            rt,
            Site::Virtual,
            ACM,
            "addDataSlot",
            "(Lnet/minecraft/world/inventory/DataSlot;)Lnet/minecraft/world/inventory/DataSlot;",
            menu,
            &[slot],
        )?;
        index += 1;
    }
}

/// A `DataSlot$1` (`forContainer`) or `DataSlot$3` (`standalone`) object.
fn new_data_slot(rt: &mut Runtime<'_, impl Host>, kind: menu_models::DataSlotKind) -> R<Value> {
    let class = match kind {
        menu_models::DataSlotKind::Container { .. } => DATA_SLOT_CONTAINER,
        menu_models::DataSlotKind::Standalone { .. } => DATA_SLOT_STANDALONE,
    };
    let host = std_host(rt)?;
    Ok(menu_models::new_data_slot(
        &mut host.menu_states,
        &mut host.next_menu_object,
        class,
        kind,
    ))
}

/// 26.3 `DataSlot.checkAndClearUpdateFlag()` offsets 0-25 (`prevValue` starts at 0).
fn data_slot_check_and_clear(rt: &mut Runtime<'_, impl Host>, slot: &Value) -> R<bool> {
    let value = data_slot_get(rt, slot)?;
    let key = key_of(slot)?;
    let host = std_host(rt)?;
    let previous = menu_models::data_slot_prev_value(&mut host.menu_states, &key, value);
    Ok(previous != value)
}

/// `DataSlot$1.get/set` (offsets 0-13: `container.get(dataId)` / `container.set(dataId, v)`)
/// and `DataSlot$3.get/set` (its own `value` field).
fn data_slot_model(
    rt: &mut Runtime<'_, impl Host>,
    body: Body,
    slot: &Value,
    args: &[Value],
) -> R<Value> {
    let key = key_of(slot)?;
    let kind = menu_models::data_slot_kind(&std_host(rt)?.menu_states, &key).ok_or_else(counted)?;
    match (body, kind) {
        (Body::DataSlotContainerGet, menu_models::DataSlotKind::Container { data, index }) => {
            vcall(
                rt,
                Site::Interface,
                CONTAINER_DATA,
                "get",
                "(I)I",
                &data,
                &[Value::Int(index)],
            )
        }
        (Body::DataSlotContainerSet, menu_models::DataSlotKind::Container { data, index }) => {
            let value = arg(args, 0)?.clone();
            vcall(
                rt,
                Site::Interface,
                CONTAINER_DATA,
                "set",
                "(II)V",
                &data,
                &[Value::Int(index), value],
            )
        }
        (Body::DataSlotStandaloneGet, menu_models::DataSlotKind::Standalone { value }) => {
            Ok(Value::Int(value))
        }
        (Body::DataSlotStandaloneSet, menu_models::DataSlotKind::Standalone { .. }) => {
            let value = as_int(arg(args, 0)?)?;
            menu_models::set_standalone_data_slot(&mut std_host(rt)?.menu_states, &key, value);
            Ok(Value::Void)
        }
        _ => Err(counted()),
    }
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

/// 26.3 `SimpleContainer.setItem(int, ItemStack)` offsets 0-23: `items.set(slot, stack)` (a
/// `withSize` list: `null` throws, an index outside it throws), `stack.limitSize(
/// getMaxStackSize(stack))` (virtual), `setChanged()` (virtual).
fn simple_set_item(
    rt: &mut Runtime<'_, impl Host>,
    container: &Value,
    index: i32,
    stack: &Value,
) -> R<()> {
    if *stack == Value::Ref(None) {
        return Err(non_null_list_npe());
    }
    let len = container_items(rt, container)?.len();
    let Some(position) = usize::try_from(index).ok().filter(|i| *i < len) else {
        return Err(array_index_out_of_bounds(index, len));
    };
    set_container_item(rt, container, position, stack.clone())?;
    let max = as_int(&vcall(
        rt,
        Site::Interface,
        CONTAINER,
        "getMaxStackSize",
        D_STACK_I,
        container,
        std::slice::from_ref(stack),
    )?)?;
    stack_limit_size(rt, stack, max)?;
    vcall(
        rt,
        Site::Virtual,
        SIMPLE_CONTAINER,
        "setChanged",
        "()V",
        container,
        &[],
    )
    .map(drop)
}

/// 26.3 `ContainerHelper.removeItem(List, int, int)` offsets 0-52.
fn helper_remove_item(
    rt: &mut Runtime<'_, impl Host>,
    items: &[Value],
    index: i32,
    count: i32,
) -> R<Value> {
    let Some(stack) = usize::try_from(index).ok().and_then(|i| items.get(i)) else {
        return Ok(menu_models::empty_stack());
    };
    if stack_is_empty(rt, stack)? || count <= 0 {
        return Ok(menu_models::empty_stack());
    }
    stack_split(rt, stack, count)
}

/// 26.3 `RecipeCraftingHolder.awardUsedRecipes(Player, List)` offsets 0-45: the recipe the
/// result came from is reported to the player (`triggerRecipeCrafted`) and, unless special,
/// unlocked (`awardRecipes`), and forgotten.
fn holder_award_used_recipes(
    rt: &mut Runtime<'_, impl Host>,
    holder: &Value,
    player: &Value,
    ingredients: &Value,
) -> R<()> {
    let recipe = vcall(
        rt,
        Site::Interface,
        RECIPE_CRAFTING_HOLDER,
        "getRecipeUsed",
        "()Lnet/minecraft/world/item/crafting/RecipeHolder;",
        holder,
        &[],
    )?;
    if recipe == Value::Ref(None) {
        return Ok(());
    }
    if *player == Value::Ref(None) {
        return Err(npe());
    }
    // `Player.triggerRecipeCrafted` is empty; `ServerPlayer`'s triggers the criterion.
    let server = is_server_player(rt, player)?;
    let id = recipe_holder_id(rt, &recipe)?;
    if server {
        let items = list_values(rt, ingredients)?;
        let mut stacks = Vec::with_capacity(items.len());
        for item in &items {
            stacks.push(components(rt, item)?);
        }
        record(
            rt,
            LiveMenuEffect::RecipeCrafted {
                recipe: id.clone(),
                ingredients: stacks,
            },
        )?;
    }
    // 17-26: `recipe.value().isSpecial()` (the mod's recipe, virtual).
    let value = vcall(
        rt,
        Site::Virtual,
        "net/minecraft/world/item/crafting/RecipeHolder",
        "value",
        "()Lnet/minecraft/world/item/crafting/Recipe;",
        &recipe,
        &[],
    )?;
    let special = recipe_is_special(rt, &value)?;
    if !special {
        if server {
            record(rt, LiveMenuEffect::AwardRecipes { recipes: vec![id] })?;
        }
        vcall(
            rt,
            Site::Interface,
            RECIPE_CRAFTING_HOLDER,
            "setRecipeUsed",
            "(Lnet/minecraft/world/item/crafting/RecipeHolder;)V",
            holder,
            &[Value::Ref(None)],
        )?;
    }
    Ok(())
}

/// `recipe.isSpecial()` (invokeinterface): the recipe's own body, or, for a jar recipe whose
/// only game supertype is `Recipe` itself (its classes extend `Object` and every interface
/// it reaches outside the jar is `Recipe`), `Recipe`'s default `return false` (offsets 0-1).
fn recipe_is_special(rt: &mut Runtime<'_, impl Host>, recipe: &Value) -> R<bool> {
    const RECIPE: &str = "net/minecraft/world/item/crafting/Recipe";
    let r = mref(RECIPE, "isSpecial", "()Z");
    if rt.jvm_selected_jar_body(&r, 0xb9, Some(recipe))?.is_some()
        || jar_object::object_class(recipe).is_none()
    {
        return as_bool(&vcall(
            rt,
            Site::Interface,
            RECIPE,
            "isSpecial",
            "()Z",
            recipe,
            &[],
        )?);
    }
    let class = jar_object::object_class(recipe)
        .ok_or_else(counted)?
        .to_owned();
    if vanilla_class_of(rt, recipe).as_deref() != Some("java/lang/Object") {
        return as_bool(&vcall(
            rt,
            Site::Interface,
            RECIPE,
            "isSpecial",
            "()Z",
            recipe,
            &[],
        )?);
    }
    // Every interface the jar classes reach, walking jar interfaces' superinterfaces.
    let mut pending = Vec::new();
    let mut current = Some(class);
    while let Some(name) = current.take() {
        let Some(file) = rt.class(&name) else {
            break;
        };
        pending.extend(file.interface_names());
        if !file.super_name.is_empty() && file.super_name != name {
            current = Some(file.super_name.clone());
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    while let Some(interface) = pending.pop() {
        if !seen.insert(interface.clone()) {
            continue;
        }
        match rt.class(&interface) {
            Some(file) => pending.extend(file.interface_names()),
            None if interface == RECIPE => {}
            None => return Err(counted()),
        }
    }
    Ok(false)
}

/// A modelled `RecipeHolder`'s recipe id.
fn recipe_holder_id(rt: &mut Runtime<'_, impl Host>, recipe: &Value) -> R<String> {
    let host = std_host(rt)?;
    let Value::Host(handle) = recipe else {
        return Err(counted());
    };
    if handle.kind() != crate::runtime::host_ref::HostKind::RecipeHolder {
        return Err(counted());
    }
    let holder = host.recipes.holders.get(handle.key()).ok_or_else(counted)?;
    let location = crate::runtime::host_ids::key_location(&holder.id).ok_or_else(counted)?;
    crate::runtime::host_ids::identifier_key(&location)
        .map(str::to_owned)
        .ok_or_else(counted)
}

/// The elements of a `List` a jar built (`List.of`, an `ArrayList`), read without running jar
/// code.
fn list_values(rt: &mut Runtime<'_, impl Host>, list: &Value) -> R<Vec<Value>> {
    if *list == Value::Ref(None) {
        return Err(npe());
    }
    rt.collection_elements(list).map_err(|_| Flow::Counted)
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

fn inventory_get_item_call(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    index: i32,
) -> R<Value> {
    let item = vcall(
        rt,
        Site::Virtual,
        INVENTORY,
        "getItem",
        D_I_STACK,
        inventory,
        &[Value::Int(index)],
    )?;
    if item == Value::Ref(None) {
        return Err(npe());
    }
    Ok(item)
}

fn inventory_set_item_call(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    index: i32,
    stack: &Value,
) -> R<()> {
    vcall(
        rt,
        Site::Virtual,
        INVENTORY,
        "setItem",
        D_I_STACK_V,
        inventory,
        &[Value::Int(index), stack.clone()],
    )
    .map(drop)
}

fn inventory_add_call(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    stack: &Value,
) -> R<bool> {
    as_bool(&vcall(
        rt,
        Site::Virtual,
        INVENTORY,
        "add",
        D_STACK_Z,
        inventory,
        std::slice::from_ref(stack),
    )?)
}

/// 26.3 `Inventory.getItem(int)` offsets 0-52: `items` below 36, then
/// `EQUIPMENT_SLOT_MAPPING`'s equipment slots, `EMPTY` past them.
fn inventory_get_item(rt: &mut Runtime<'_, impl Host>, inventory: &Value, index: i32) -> R<Value> {
    let contents = inventory_contents(rt, inventory)?;
    let items = contents.items.len() as i32;
    if index < items {
        return usize::try_from(index)
            .ok()
            .and_then(|i| contents.items.get(i).cloned())
            .ok_or_else(|| array_index_out_of_bounds(index, contents.items.len()));
    }
    let equipment_index = (index - items) as usize;
    if equipment_index >= menu_models::INVENTORY_EQUIPMENT {
        return Ok(menu_models::empty_stack());
    }
    contents
        .equipment
        .as_ref()
        .and_then(|equipment| equipment.get(equipment_index).cloned())
        .ok_or_else(counted)
}

/// 26.3 `Inventory.setItem(int, ItemStack)` offsets 0-48.
fn inventory_set_item(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    index: i32,
    stack: Value,
) -> R<()> {
    with_inventory_mut(rt, inventory, |contents| {
        let items = contents.items.len() as i32;
        if index < items {
            if stack == Value::Ref(None) {
                return Err(non_null_list_npe());
            }
            let len = contents.items.len();
            let slot = usize::try_from(index)
                .ok()
                .and_then(|i| contents.items.get_mut(i))
                .ok_or_else(|| array_index_out_of_bounds(index, len))?;
            *slot = stack;
            return Ok(());
        }
        let equipment_index = (index - items) as usize;
        if equipment_index >= menu_models::INVENTORY_EQUIPMENT {
            return Ok(());
        }
        // `EntityEquipment.set(slot, stack)`.
        let equipment = contents.equipment.as_mut().ok_or_else(counted)?;
        if stack == Value::Ref(None) {
            return Err(counted());
        }
        equipment[equipment_index] = stack;
        Ok(())
    })
}

/// 26.3 `Inventory.removeItem(int, int)` offsets 0-66.
fn inventory_remove_item(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    index: i32,
    count: i32,
) -> R<Value> {
    let contents = inventory_contents(rt, inventory)?;
    if index < contents.items.len() as i32 {
        return helper_remove_item(rt, &contents.items, index, count);
    }
    let equipment_index = (index - contents.items.len() as i32) as usize;
    if equipment_index >= menu_models::INVENTORY_EQUIPMENT {
        return Ok(menu_models::empty_stack());
    }
    let stack = contents
        .equipment
        .as_ref()
        .and_then(|equipment| equipment.get(equipment_index).cloned())
        .ok_or_else(counted)?;
    if stack_is_empty(rt, &stack)? {
        return Ok(menu_models::empty_stack());
    }
    stack_split(rt, &stack, count)
}

/// 26.3 `Inventory.removeItemNoUpdate(int)` offsets 0-69.
fn inventory_remove_item_no_update(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    index: i32,
) -> R<Value> {
    let contents = inventory_contents(rt, inventory)?;
    if index < contents.items.len() as i32 {
        let current = usize::try_from(index)
            .ok()
            .and_then(|i| contents.items.get(i).cloned())
            .ok_or_else(|| array_index_out_of_bounds(index, contents.items.len()))?;
        inventory_set_item(rt, inventory, index, menu_models::empty_stack())?;
        return Ok(current);
    }
    let equipment_index = (index - contents.items.len() as i32) as usize;
    if equipment_index >= menu_models::INVENTORY_EQUIPMENT {
        return Ok(menu_models::empty_stack());
    }
    let current = contents
        .equipment
        .as_ref()
        .and_then(|equipment| equipment.get(equipment_index).cloned())
        .ok_or_else(counted)?;
    inventory_set_item(rt, inventory, index, menu_models::empty_stack())?;
    Ok(current)
}

/// 26.3 `Inventory.hasRemainingSpaceForItem(ItemStack, ItemStack)` offsets 0-39.
fn has_remaining_space_for_item(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    destination: &Value,
    stack: &Value,
) -> R<bool> {
    if stack_is_empty(rt, destination)? || !same_item_same_components(rt, destination, stack)? {
        return Ok(false);
    }
    if !stack_is_stackable(rt, destination)? {
        return Ok(false);
    }
    let max = as_int(&vcall(
        rt,
        Site::Virtual,
        INVENTORY,
        "getMaxStackSize",
        D_STACK_I,
        inventory,
        std::slice::from_ref(destination),
    )?)?;
    Ok(stack_count(rt, destination)? < max)
}

/// 26.3 `Inventory.getSlotWithRemainingSpace(ItemStack)` offsets 0-79: the selected slot, the
/// offhand (40), then `items` in order.
fn inventory_slot_with_remaining_space(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    stack: &Value,
) -> R<i32> {
    let selected = inventory_contents(rt, inventory)?.selected;
    let item = inventory_get_item_call(rt, inventory, selected)?;
    if has_remaining_space_for_item(rt, inventory, &item, stack)? {
        return Ok(selected);
    }
    let offhand = inventory_get_item_call(rt, inventory, 40)?;
    if has_remaining_space_for_item(rt, inventory, &offhand, stack)? {
        return Ok(40);
    }
    let items = inventory_contents(rt, inventory)?.items;
    for (index, item) in items.iter().enumerate() {
        if has_remaining_space_for_item(rt, inventory, item, stack)? {
            return Ok(index as i32);
        }
    }
    Ok(-1)
}

/// 26.3 `Inventory.getFreeSlot()` offsets 0-39.
fn inventory_free_slot(rt: &mut Runtime<'_, impl Host>, inventory: &Value) -> R<i32> {
    let items = inventory_contents(rt, inventory)?.items;
    for (index, item) in items.iter().enumerate() {
        if stack_is_empty(rt, item)? {
            return Ok(index as i32);
        }
    }
    Ok(-1)
}

/// 26.3 `Inventory.addResource(int, ItemStack)` offsets 0-82.
fn inventory_add_resource_at(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    slot: i32,
    stack: &Value,
) -> R<i32> {
    let mut count = stack_count(rt, stack)?;
    let mut target = inventory_get_item_call(rt, inventory, slot)?;
    if stack_is_empty(rt, &target)? {
        target = stack_copy_with_count(rt, stack, 0)?;
        inventory_set_item_call(rt, inventory, slot, &target)?;
    }
    let max = as_int(&vcall(
        rt,
        Site::Virtual,
        INVENTORY,
        "getMaxStackSize",
        D_STACK_I,
        inventory,
        std::slice::from_ref(&target),
    )?)?;
    let room = max.wrapping_sub(stack_count(rt, &target)?);
    let amount = count.min(room);
    if amount == 0 {
        return Ok(count);
    }
    count = count.wrapping_sub(amount);
    stack_grow(rt, &target, amount)?;
    // 75-78: `target.setPopTime(5)`: the client's pickup animation, which no server stack
    // keeps (`LiveStack` has no pop time, so a later `getPopTime` counts).
    Ok(count)
}

/// 26.3 `Inventory.addResource(ItemStack)` offsets 0-32.
fn inventory_add_resource(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    stack: &Value,
) -> R<i32> {
    let mut slot = inventory_slot_with_remaining_space(rt, inventory, stack)?;
    if slot == -1 {
        slot = inventory_free_slot(rt, inventory)?;
    }
    if slot == -1 {
        return stack_count(rt, stack);
    }
    inventory_add_resource_at(rt, inventory, slot, stack)
}

/// 26.3 `Inventory.add(int, ItemStack)` offsets 0-164 (the `Throwable` handler's
/// `ReportedException` is not modelled: an exception from inside counts).
fn inventory_add(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    slot: i32,
    stack: &Value,
) -> R<bool> {
    let result = inventory_add_inner(rt, inventory, slot, stack);
    match result {
        Err(Flow::Java(_)) => Err(counted()),
        other => other,
    }
}

fn inventory_add_inner(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    mut slot: i32,
    stack: &Value,
) -> R<bool> {
    if stack_is_empty(rt, stack)? {
        return Ok(false);
    }
    let player = inventory_player(rt, inventory)?;
    if !stack_is_damaged(rt, stack)? {
        // 16-62: `do { int start = stack.getCount(); stack.setCount(addResource(..)); }
        // while (!stack.isEmpty() && stack.getCount() < start)`: the loop's back edge (offset
        // 60) re-enters at 16, so `start` is the count before the latest attempt and the loop
        // ends once an attempt places nothing.
        let mut start;
        loop {
            start = stack_count(rt, stack)?;
            let left = if slot == -1 {
                inventory_add_resource(rt, inventory, stack)?
            } else {
                inventory_add_resource_at(rt, inventory, slot, stack)?
            };
            stack_set_count(rt, stack, left)?;
            if stack_is_empty(rt, stack)? || stack_count(rt, stack)? >= start {
                break;
            }
        }
        if stack_count(rt, stack)? == start && has_infinite_materials(rt, &player)? {
            stack_set_count(rt, stack, 0)?;
            return Ok(true);
        }
        return Ok(stack_count(rt, stack)? < start);
    }
    // 102-145: a damaged stack takes a free slot whole.
    if slot == -1 {
        slot = inventory_free_slot(rt, inventory)?;
    }
    if slot >= 0 {
        let moved = stack_copy_and_clear(rt, stack)?;
        with_inventory_mut(rt, inventory, |contents| {
            let len = contents.items.len();
            let entry = contents
                .items
                .get_mut(slot as usize)
                .ok_or_else(|| array_index_out_of_bounds(slot, len))?;
            *entry = moved;
            Ok(())
        })?;
        return Ok(true);
    }
    if has_infinite_materials(rt, &player)? {
        stack_set_count(rt, stack, 0)?;
        return Ok(true);
    }
    Ok(false)
}

/// 26.3 `Inventory.placeItemBackInInventory(ItemStack, boolean, Prediction)` offsets 0-119.
fn inventory_place_back(
    rt: &mut Runtime<'_, impl Host>,
    inventory: &Value,
    stack: &Value,
    send: bool,
    prediction: Prediction,
) -> R<()> {
    loop {
        if stack_is_empty(rt, stack)? {
            return Ok(());
        }
        let mut slot = inventory_slot_with_remaining_space(rt, inventory, stack)?;
        if slot == -1 {
            slot = inventory_free_slot(rt, inventory)?;
        }
        if slot == -1 {
            // 32-43: no room: `player.drop(stack, false, prediction)`.
            let player = inventory_player(rt, inventory)?;
            return player_drop(rt, &player, stack, false, prediction);
        }
        // 46-71: `add(slot, stack.split(stack.getMaxStackSize() - getItem(slot).getCount()))`.
        let present = inventory_get_item_call(rt, inventory, slot)?;
        let room = stack_max_stack_size(rt, stack)?.wrapping_sub(stack_count(rt, &present)?);
        let part = stack_split(rt, stack, room)?;
        let added = as_bool(&vcall(
            rt,
            Site::Virtual,
            INVENTORY,
            "add",
            "(ILnet/minecraft/world/item/ItemStack;)Z",
            inventory,
            &[Value::Int(slot), part],
        )?)?;
        // 74-113: a server player is sent the slot.
        if added && send {
            let player = inventory_player(rt, inventory)?;
            if is_server_player(rt, &player)? {
                let item = inventory_get_item_call(rt, inventory, slot)?;
                let copy = stack_copy(rt, &item)?;
                let stack = components(rt, &copy)?;
                record(rt, LiveMenuEffect::InventorySlotSent { slot, stack })?;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// initMenu, inventory facts
// ---------------------------------------------------------------------------

/// `ServerPlayer.initMenu(menu)` for a menu `openMenu` just built, when the embedder described
/// the player's inventory: `Ok(true)` when it ran (or counted itself), `Ok(false)` when Decaf
/// can't run it at all (the caller counts `initMenu`, as before the live models).
pub(crate) fn run_init_menu<H: Host>(
    rt: &mut Runtime<'_, H>,
    menu: &Value,
) -> Result<bool, NativeError> {
    let ready = rt.host.as_std_host().is_some_and(|host| {
        let Some(active) = host.block_use.active_use.as_ref() else {
            return false;
        };
        let inventory_known = expect_ref(&active.player_inventory)
            .and_then(|key| host.menu_states.get(key))
            .is_some_and(|object| {
                matches!(
                    object,
                    menu_models::Object::Inventory {
                        contents: Some(_),
                        ..
                    }
                )
            });
        let server_menu = expect_ref(menu)
            .and_then(|key| host.menu_states.get(key))
            .is_some_and(|object| {
                matches!(object, menu_models::Object::Menu(state) if !menu_has_client_slots(host, state))
            });
        active.facts.is_some() && inventory_known && server_menu
    });
    if !ready {
        return Ok(false);
    }
    match init_menu(rt, menu) {
        Ok(()) => Ok(true),
        Err(Flow::Java(error)) => Err(error),
        Err(Flow::Counted) => {
            rt.host.unknown_operand_call(&mref(
                SERVER_PLAYER,
                "initMenu",
                "(Lnet/minecraft/world/inventory/AbstractContainerMenu;)V",
            ))?;
            Ok(true)
        }
    }
}

/// Whether two stacks are the same as far as Java could tell: one object, or two modelled,
/// fully known stacks with equal item, count and components.
fn same_stack(host: &StdHost, old: &Value, new: &Value) -> bool {
    if old == new {
        return true;
    }
    let (Some(a), Some(b)) = (
        recipe_models::extract_stack_components(host, old),
        recipe_models::extract_stack_components(host, new),
    ) else {
        return false;
    };
    !a.opaque_components && !b.opaque_components && a == b && a.count > 0
}

/// Fill the `Inventory` object `inventory` from the embedder's facts. A slot whose stack is
/// unchanged keeps its Java object (a menu or mod that holds it keeps holding the same one);
/// a changed slot gets the embedder's value. Facts without an inventory leave it unknown.
pub(crate) fn fill_inventory(host: &mut StdHost, inventory: &Value, facts: &MenuPlayerFacts) {
    let Some(stacks) = facts.inventory.as_ref() else {
        return;
    };
    let items_len = menu_models::INVENTORY_ITEMS;
    let full_len = items_len + menu_models::INVENTORY_EQUIPMENT;
    if stacks.len() != items_len && stacks.len() != full_len {
        return;
    }
    let Some(key) = expect_ref(inventory).cloned() else {
        return;
    };
    let previous = match host.menu_states.get(&key) {
        Some(menu_models::Object::Inventory { contents, .. }) => contents.clone(),
        _ => return,
    };
    let keep = |host: &StdHost, old: Option<&Value>, new: &Value| -> Value {
        match old {
            Some(old) if same_stack(host, old, new) => old.clone(),
            _ => new.clone(),
        }
    };
    let items: Vec<Value> = (0..items_len)
        .map(|i| {
            keep(
                host,
                previous.as_ref().and_then(|c| c.items.get(i)),
                &stacks[i],
            )
        })
        .collect();
    let equipment = (stacks.len() == full_len).then(|| {
        (0..menu_models::INVENTORY_EQUIPMENT)
            .map(|i| {
                keep(
                    host,
                    previous
                        .as_ref()
                        .and_then(|c| c.equipment.as_ref())
                        .and_then(|e| e.get(i)),
                    &stacks[items_len + i],
                )
            })
            .collect()
    });
    if let Some(menu_models::Object::Inventory { contents, .. }) = host.menu_states.get_mut(&key) {
        *contents = Some(menu_models::InventoryContents {
            items,
            equipment,
            selected: facts.selected_slot,
        });
    }
}

/// The `Inventory` object's stacks, in the facts' layout (36 or 43 entries).
/// The stack `inventory` (an `Inventory` model) holds at `index` (`items` 0-35, then the
/// equipment 36-42), from Decaf's state.
pub(crate) fn inventory_item(host: &StdHost, inventory: &Value, index: i32) -> Option<Value> {
    let key = expect_ref(inventory)?;
    let Some(menu_models::Object::Inventory {
        contents: Some(contents),
        ..
    }) = host.menu_states.get(key)
    else {
        return None;
    };
    let index = usize::try_from(index).ok()?;
    match index.checked_sub(menu_models::INVENTORY_ITEMS) {
        None => contents.items.get(index).cloned(),
        Some(equipment) => contents.equipment.as_ref()?.get(equipment).cloned(),
    }
}

/// Record `effect` on the running call (`record` without the interpreter), for the item-part
/// player façade (`item_parts.rs`).
pub(crate) fn record_effect(host: &mut StdHost, effect: LiveMenuEffect) {
    let quiet = draws_no_level_random(&effect);
    if let Some(active) = host.block_use.active_use.as_mut() {
        active.live_effects.push(effect);
    }
    host.note_deferred_effect(quiet);
}

pub(crate) fn inventory_out(
    host: &StdHost,
    inventory: &Value,
    facts: &MenuPlayerFacts,
) -> Option<Vec<Value>> {
    let given = facts.inventory.as_ref()?.len();
    let key = expect_ref(inventory)?;
    let Some(menu_models::Object::Inventory {
        contents: Some(contents),
        ..
    }) = host.menu_states.get(key)
    else {
        return None;
    };
    let mut out = contents.items.clone();
    if given > out.len() {
        out.extend(contents.equipment.clone()?);
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Static methods and ItemStack calls that need the interpreter
// ---------------------------------------------------------------------------

/// Whether an `invokestatic owner.name descriptor` resolves to `vanilla`'s static: `owner` is
/// `vanilla`, or a jar class whose superclasses reach it without a jar class declaring the
/// method (javac names the calling subclass for an inherited static).
fn static_resolves_to<H: Host>(rt: &Runtime<'_, H>, r: &MethodRef, vanilla: &[&str]) -> bool {
    let mut current = r.owner.clone();
    for _ in 0..crate::runtime::MAX_SUPERCLASS_WALK {
        if vanilla.contains(&current.as_str()) {
            return true;
        }
        let Some(file) = rt.class(&current) else {
            return false;
        };
        if file.method(&r.name, &r.descriptor).is_some() {
            return false;
        }
        if file.super_name.is_empty() || file.super_name == current {
            return false;
        }
        current = file.super_name.clone();
    }
    false
}

/// Entry from `Runtime::invoke_game_helpers`: the vanilla statics of `AbstractContainerMenu`,
/// `DataSlot` and `ItemStack` this module models, and `ItemStack.onCraftedBy`, whose `Item`
/// callback needs the interpreter.
#[inline(never)]
pub(crate) fn invoke_helper<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let result = match receiver {
        None => static_helper(rt, r, args),
        Some(stack)
            if r.owner == ITEM_STACK
                && r.name == "onCraftedBy"
                && r.descriptor == "(Lnet/minecraft/world/entity/player/Player;I)V" =>
        {
            if !rt.host.as_std_host().is_some_and(|host| {
                recipe_models::read_live_stack_opt(host, stack).is_some()
                    || recipe_models::is_empty_stack_handle(stack)
            }) {
                return Ok(None);
            }
            let player = arg(args, 0).cloned();
            let amount = arg(args, 1).and_then(as_int);
            match (player, amount) {
                (Ok(player), Ok(amount)) => {
                    on_crafted_by(rt, stack, &player, amount).map(|()| Some(Value::Void))
                }
                _ => Err(counted()),
            }
        }
        Some(_) => return Ok(None),
    };
    match result {
        Ok(value) => Ok(value),
        Err(Flow::Java(error)) => Err(error),
        Err(Flow::Counted) => rt.host.unknown_operand_call(r).map(Some),
    }
}

fn static_helper<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> R<Option<Value>> {
    let menu_static = static_resolves_to(rt, r, &[ACM, ICM]);
    if menu_static {
        match (r.name.as_str(), r.descriptor.as_str()) {
            // `getQuickcraftType` / `getQuickcraftHeader` / `getQuickcraftMask` offsets 0-9.
            ("getQuickcraftType", "(I)I") => {
                return Ok(Some(Value::Int((as_int(arg(args, 0)?)? >> 2) & 3)));
            }
            ("getQuickcraftHeader", "(I)I") => {
                return Ok(Some(Value::Int(as_int(arg(args, 0)?)? & 3)));
            }
            ("getQuickcraftMask", "(II)I") => {
                let (header, kind) = (as_int(arg(args, 0)?)?, as_int(arg(args, 1)?)?);
                return Ok(Some(Value::Int((header & 3) | ((kind & 3) << 2))));
            }
            ("isValidQuickcraftType", "(ILnet/minecraft/world/entity/player/Player;)Z") => {
                let kind = as_int(arg(args, 0)?)?;
                let player = arg(args, 1)?.clone();
                return is_valid_quickcraft_type(rt, kind, &player).map(|v| Some(bool_value(v)));
            }
            (
                "canItemQuickReplace",
                "(Lnet/minecraft/world/inventory/Slot;Lnet/minecraft/world/item/ItemStack;Z)Z",
            ) => {
                let (slot, stack) = (arg(args, 0)?.clone(), arg(args, 1)?.clone());
                let matters = as_bool(arg(args, 2)?)?;
                return can_item_quick_replace(rt, &slot, &stack, matters)
                    .map(|v| Some(bool_value(v)));
            }
            ("getQuickCraftPlaceCount", "(IILnet/minecraft/world/item/ItemStack;)I") => {
                let (count, kind) = (as_int(arg(args, 0)?)?, as_int(arg(args, 1)?)?);
                let stack = arg(args, 2)?.clone();
                return quick_craft_place_count(rt, count, kind, &stack)
                    .map(|v| Some(Value::Int(v)));
            }
            (
                "stillValid",
                "(Lnet/minecraft/world/inventory/ContainerLevelAccess;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/level/block/Block;)Z",
            ) => {
                let (access, player, block) = (
                    arg(args, 0)?.clone(),
                    arg(args, 1)?.clone(),
                    arg(args, 2)?.clone(),
                );
                return static_still_valid(rt, &access, &player, &block)
                    .map(|v| Some(bool_value(v)));
            }
            _ => {}
        }
    }
    if static_resolves_to(rt, r, &[DATA_SLOT]) {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("standalone", "()Lnet/minecraft/world/inventory/DataSlot;") => {
                return new_data_slot(rt, menu_models::DataSlotKind::Standalone { value: 0 })
                    .map(Some);
            }
            (
                "forContainer",
                "(Lnet/minecraft/world/inventory/ContainerData;I)Lnet/minecraft/world/inventory/DataSlot;",
            ) => {
                let data = arg(args, 0)?.clone();
                let index = as_int(arg(args, 1)?)?;
                return new_data_slot(rt, menu_models::DataSlotKind::Container { data, index })
                    .map(Some);
            }
            _ => {}
        }
    }
    if r.owner == ITEM_STACK {
        let both = "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemStack;)Z";
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("isSameItem", d) if d == both => {
                let (a, b) = (arg(args, 0)?.clone(), arg(args, 1)?.clone());
                return same_item(rt, &a, &b).map(|v| Some(bool_value(v)));
            }
            ("matches", d) if d == both => {
                let (a, b) = (arg(args, 0)?.clone(), arg(args, 1)?.clone());
                return stacks_match(rt, &a, &b).map(|v| Some(bool_value(v)));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// 26.3 `AbstractContainerMenu.stillValid(ContainerLevelAccess, Player, Block)` offsets
/// 0-23 with `lambda$stillValid$0`: `level.getBlockState(pos).is(block) &&
/// player.isWithinBlockInteractionRange(pos, 4.0)`, `true` for `NULL`.
fn static_still_valid(
    rt: &mut Runtime<'_, impl Host>,
    access: &Value,
    player: &Value,
    block: &Value,
) -> R<bool> {
    let (level, pos) = match access_level_pos(rt, access)? {
        Some(None) => return Ok(true),
        Some(Some(found)) => found,
        None => return Err(counted()),
    };
    let pos_value = crate::runtime::client_frame::block_pos_value(pos);
    let state = vcall(
        rt,
        Site::Virtual,
        "net/minecraft/world/level/Level",
        "getBlockState",
        "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;",
        &level,
        std::slice::from_ref(&pos_value),
    )?;
    let is = as_bool(&vcall(
        rt,
        Site::Virtual,
        "net/minecraft/world/level/block/state/BlockState",
        "is",
        "(Ljava/lang/Object;)Z",
        &state,
        std::slice::from_ref(block),
    )?)?;
    if !is {
        return Ok(false);
    }
    within_block_interaction_range(rt, player, pos, 4.0)
}

/// 26.3 `ItemStack.onCraftedBy(Player, int)` offsets 0-24: `player.awardStat(
/// Stats.ITEM_CRAFTED.get(getItem()), amount)`, then `getItem().onCraftedBy(this, player)`
/// (virtual: a jar item's override, else `Item`'s `onCraftedPostProcess(stack, level)`).
fn on_crafted_by(
    rt: &mut Runtime<'_, impl Host>,
    stack: &Value,
    player: &Value,
    amount: i32,
) -> R<()> {
    if *player == Value::Ref(None) {
        return Err(npe());
    }
    let item = stack_call(
        rt,
        "getItem",
        "()Lnet/minecraft/world/item/Item;",
        stack,
        &[],
    )?;
    if is_server_player(rt, player)? {
        let id = recipe_models::resolve_item_id(std_host(rt)?, &item).ok_or_else(counted)?;
        record(
            rt,
            LiveMenuEffect::AwardStat {
                stat_type: "minecraft:crafted".to_owned(),
                value: id,
                amount,
            },
        )?;
    }
    let on_crafted = mref(
        "net/minecraft/world/item/Item",
        "onCraftedBy",
        "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/entity/player/Player;)V",
    );
    if let Some(owner) = rt.jvm_selected_jar_body(&on_crafted, 0xb6, Some(&item))? {
        exact(rt, |rt| {
            rt.invoke(
                &owner,
                &on_crafted.name,
                &on_crafted.descriptor,
                Some(item.clone()),
                &[stack.clone(), player.clone()],
            )
            .map_err(Flow::Java)
        })?;
        return Ok(());
    }
    // `Item.onCraftedBy` offsets 0-9: `onCraftedPostProcess(stack, player.level())`.
    let post = mref(
        "net/minecraft/world/item/Item",
        "onCraftedPostProcess",
        "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/level/Level;)V",
    );
    if jar_object::object_class(&item).is_some() {
        // A jar item: its own `onCraftedPostProcess`, or the vanilla class it extends.
        let class = vanilla_class_of(rt, &item).ok_or_else(counted)?;
        if let Some(owner) = rt.jvm_selected_jar_body(&post, 0xb6, Some(&item))? {
            let level = with_active(rt, |active| Ok(active.level.clone()))?;
            exact(rt, |rt| {
                rt.invoke(
                    &owner,
                    &post.name,
                    &post.descriptor,
                    Some(item.clone()),
                    &[stack.clone(), level],
                )
                .map_err(Flow::Java)
            })?;
            return Ok(());
        }
        return if matches!(
            class.as_str(),
            "net/minecraft/world/item/Item" | "net/minecraft/world/item/BlockItem"
        ) {
            Ok(())
        } else {
            Err(counted())
        };
    }
    let id = recipe_models::resolve_item_id(std_host(rt)?, &item).ok_or_else(counted)?;
    if crate::runtime::vanilla_items::is_vanilla_item_id(&id) && !item_hook("map", &id) {
        Ok(())
    } else {
        Err(counted())
    }
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Which live-menu call [`NativeJar::live_menu_call`] runs.
#[derive(Clone, Copy)]
enum LiveCall<'a> {
    Click(&'a MenuClick),
    /// `ServerPlayer.tick()` offsets 38-65: `broadcastChanges()`, then the validity check.
    Tick,
    /// `ServerPlayer.doTick()` offsets 18-38: the validity check alone.
    ValidityCheck,
    /// `ServerGamePacketListenerImpl.handleContainerClose`: `doCloseContainer()`.
    Close,
}

impl StdHost {
    /// The Java menu Decaf holds for `uuid` as `(getCarried(), every slot's getItem())`, read
    /// from Decaf's own state: what a bridge shows when the menu opens. `None` when no menu is
    /// held, or one of its slots is a jar's whose contents only its own code knows.
    pub fn held_menu_contents(&self, uuid: u128) -> Option<(Value, Vec<Value>)> {
        let menu = self.player_open_menu(uuid)?.menu.clone();
        match held_menu_contents(self, &menu) {
            (Some(carried), Some(slots)) => Some((carried, slots)),
            _ => None,
        }
    }

    /// What Decaf should know of `uuid`'s player for their next block uses (its inventory,
    /// abilities and reach): with it, the `initMenu` of a menu their use opens runs exactly
    /// instead of being counted. `None` forgets it.
    pub fn set_player_menu_facts(&mut self, uuid: u128, facts: Option<MenuPlayerFacts>) {
        match facts {
            Some(facts) => {
                self.block_use.facts.insert(uuid, facts);
            }
            None => {
                self.block_use.facts.remove(&uuid);
            }
        }
    }
}

impl NativeJar {
    /// A click the client sent for the Java menu `click.container_id` Decaf holds as the
    /// player's `containerMenu`: vanilla's `ServerGamePacketListenerImpl.handleContainerClick`
    /// (offsets 19-352) on it. `Ok(None)` when Decaf holds no such menu.
    ///
    /// Decaf takes no client predictions (`setRemoteSlotUnsafe` / `setRemoteCarried`): the
    /// embedder owns the network sync, and the outcome's effects report every server-side
    /// change since the last call instead.
    pub fn click_player_menu(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        facts: &MenuPlayerFacts,
        click: &MenuClick,
        budget: &crate::runtime::ExecutionBudget,
    ) -> Result<Option<LiveMenuOutcome>, NativeError> {
        self.live_menu_call(
            host,
            player,
            dimension,
            facts,
            click.container_id,
            LiveCall::Click(click),
            budget,
        )
    }

    /// One server tick of the player's held Java menu: `ServerPlayer.tick()`'s
    /// `containerMenu.broadcastChanges()` and, when `stillValid(player)` fails,
    /// `closeContainer()` (the close appears in the outcome's `menu_effects` and `effects`).
    pub fn tick_player_menu(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        facts: &MenuPlayerFacts,
        container_id: i32,
        budget: &crate::runtime::ExecutionBudget,
    ) -> Result<Option<LiveMenuOutcome>, NativeError> {
        self.live_menu_call(
            host,
            player,
            dimension,
            facts,
            container_id,
            LiveCall::Tick,
            budget,
        )
    }

    /// `ServerPlayer.doTick()`'s second validity check (offsets 18-38): `stillValid(player)`,
    /// and `closeContainer()` when it fails.
    pub fn check_player_menu_valid(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        facts: &MenuPlayerFacts,
        container_id: i32,
        budget: &crate::runtime::ExecutionBudget,
    ) -> Result<Option<LiveMenuOutcome>, NativeError> {
        self.live_menu_call(
            host,
            player,
            dimension,
            facts,
            container_id,
            LiveCall::ValidityCheck,
            budget,
        )
    }

    /// [`NativeJar::close_player_menu`] with the player's facts: the menu's `removed` hands
    /// its inputs back into the inventory the facts describe (or drops them), and the outcome
    /// carries that inventory.
    pub fn close_player_menu_live(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        facts: &MenuPlayerFacts,
        container_id: i32,
        budget: &crate::runtime::ExecutionBudget,
    ) -> Result<Option<LiveMenuOutcome>, NativeError> {
        self.live_menu_call(
            host,
            player,
            dimension,
            facts,
            container_id,
            LiveCall::Close,
            budget,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn live_menu_call(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        facts: &MenuPlayerFacts,
        container_id: i32,
        call: LiveCall<'_>,
        budget: &crate::runtime::ExecutionBudget,
    ) -> Result<Option<LiveMenuOutcome>, NativeError> {
        let Some((menu, inventory, access_pos)) = host
            .block_use
            .player(player.uuid, player.incarnation)
            .filter(|entry| !entry.close_sent)
            .and_then(|entry| {
                let open = entry.open.as_ref()?;
                (open.container_id == container_id)
                    .then(|| (open.menu.clone(), entry.inventory.clone(), open.access_pos))
            })
        else {
            return Ok(None);
        };
        let Some(inventory) = inventory else {
            return Ok(None);
        };
        let level = game_object::server_value(format!("game/level/{dimension}"));
        let player_value = block_use::server_player_value(player);
        fill_inventory(host, &inventory, facts);
        // The embedder's cursor, when it moved the stack itself (a close that already put it
        // back into the inventory): whether a non-empty Java cursor was replaced.
        let carried_replaced = facts
            .carried
            .as_ref()
            .is_some_and(|carried| replace_carried(host, &menu, carried));
        // The block at the menu's access position, for `stillValid`'s `getBlockState`.
        let block = access_pos.and_then(|pos| {
            let (id, properties) = facts.access_block.as_ref()?;
            let (_, state) = host.block_state_for_use(id, properties)?;
            Some((state, pos))
        });
        let unknowns_before: BTreeMap<String, usize> =
            host.unknown_host_calls().into_iter().collect();
        let mark = host.call_log_mark();
        let closing = matches!(call, LiveCall::Close);
        let previous_active = host.block_use.active_use.replace(ActiveBlockUse {
            side: BlockUseSide::Server,
            dimension: dimension.to_owned(),
            level: level.clone(),
            block: block.clone(),
            player: player_value.clone(),
            player_data: player.clone(),
            player_inventory: inventory.clone(),
            container: if closing {
                PlayerContainer::Inventory
            } else {
                PlayerContainer::Open(container_id)
            },
            menu_effects: Vec::new(),
            facts: Some(facts.clone()),
            live_effects: Vec::new(),
            wrapped_click_cause: None,
        });
        if closing {
            // The window is gone; the menu stays installed until `doCloseContainer` offset 19.
            host.mark_close_sent();
        }
        let mut roots = vec![level, player_value.clone(), inventory.clone(), menu.clone()];
        if let Some((state, _)) = &block {
            roots.push(state.clone());
        }
        let mut handled = false;
        let call_result = self.with_persistent_runtime_with_budget(host, &roots, budget, |rt| {
            // A close whose cursor the embedder already put back: vanilla's `removed` would put
            // it back itself (`AbstractContainerMenu.removed` offsets 0-38), which the embedder
            // did, so the menu's own `removed` is exact. A jar `removed` override could read the
            // cursor before `super.removed` and see it empty where Java's would not: counted.
            if closing
                && carried_replaced
                && rt
                    .jvm_selected_jar_body(&mref(ACM, "removed", D_PLAYER_V), 0xb6, Some(&menu))?
                    .is_some()
            {
                rt.host
                    .unknown_operand_call(&mref(SERVER_PLAYER, "doCloseContainer", "()V"))?;
            }
            handled = run_call(rt, &menu, &player_value, call)?;
            Ok(Value::Void)
        });
        let finished_active = host.block_use.active_use.take();
        host.block_use.active_use = previous_active;
        host.rewind_call_log(mark);
        let (unknown_host_calls, unknown_host_call_count) =
            block_use::unknown_calls_since(host, &unknowns_before);
        let (effects, menu_effects, wrapped_click_cause) = finished_active
            .map(|active| {
                (
                    active.live_effects,
                    active.menu_effects,
                    active.wrapped_click_cause,
                )
            })
            .unwrap_or_default();
        let still_held = host
            .block_use
            .player(player.uuid, player.incarnation)
            .and_then(|entry| entry.open.as_ref())
            .is_some_and(|open| open.menu == menu);
        let (carried, slots) = if still_held {
            held_menu_contents(host, &menu)
        } else {
            (None, None)
        };
        Ok(Some(LiveMenuOutcome {
            container_id,
            effects,
            menu_effects,
            inventory: inventory_out(host, &inventory, facts),
            carried,
            slots,
            handled,
            unknown_host_calls,
            unknown_host_call_count,
            failure: call_result
                .err()
                .map(|error| failure_line(&error, wrapped_click_cause.as_deref())),
        }))
    }
}

// ---------------------------------------------------------------------------
// Menu-type declarations (the layout an embedder's menu type needs)
// ---------------------------------------------------------------------------

/// The size `Container.getMaxStackSize()` answers (`server-26.3.jar`: `bipush 99`), which
/// `SimpleContainer`, `ResultContainer` and the combiner's anonymous containers inherit, and
/// which `Slot.getMaxStackSize()` returns for each of their slots.
const CONTAINER_MAX_STACK_SIZE: i64 = 99;

/// What one menu slot shows, as an embedder lays a menu type out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSlotRole {
    /// A slot of the menu's own vanilla container (`SimpleContainer`, `ResultContainer`, the
    /// combiner's), holding `Slot.getMaxStackSize()` (99); `input` for a combiner input slot
    /// (`ItemCombinerMenu$2`).
    Owned { input: bool },
    /// The player's `Inventory` slot `.0`.
    Player(i32),
}

/// A menu's slot layout and data slot count, for a menu type's declaration or a live menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuLayout {
    pub slots: Vec<MenuSlotRole>,
    pub data_slots: usize,
}

impl MenuLayout {
    /// The menu's own slots (the owned ones come first).
    pub fn owned_slots(&self) -> usize {
        self.slots
            .iter()
            .filter(|role| matches!(role, MenuSlotRole::Owned { .. }))
            .count()
    }

    /// As a Sunrise content menu declaration (`data/<ns>/sunrise/menu/<path>.json`): one owned
    /// slot per container slot, the standard player group when the menu has it, the combiner's
    /// input slots as `inputs`, and the data slots. No `results`: a declared result makes Sunrise
    /// return the inputs itself on close, where the Java menu's own `removed` decides that.
    pub fn declaration(&self) -> serde_json::Value {
        let owned = self.owned_slots();
        let inputs: Vec<usize> = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, role)| matches!(role, MenuSlotRole::Owned { input: true }))
            .map(|(index, _)| index)
            .collect();
        let mut file = serde_json::Map::new();
        file.insert("slots".into(), owned.into());
        if owned > 0 {
            file.insert(
                "owned_slots".into(),
                vec![CONTAINER_MAX_STACK_SIZE; owned].into(),
            );
        }
        if !inputs.is_empty() {
            file.insert("inputs".into(), inputs.into());
        }
        file.insert("player_inventory".into(), (self.slots.len() > owned).into());
        if self.data_slots > 0 {
            file.insert("data_slots".into(), self.data_slots.into());
        }
        serde_json::Value::Object(file)
    }
}

/// One `MenuType` a jar registered, as an embedder needs to declare it before any menu opens:
/// the layout every menu its `MenuSupplier` builds has.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuTypeDeclaration {
    /// The registry id (`minecraft:menu`).
    pub id: String,
    /// The layout ([`MenuLayout::declaration`] is its Sunrise file), or why it can't be declared.
    pub layout: Result<MenuLayout, String>,
}

/// Classify a built `menu`'s slots, whose player slots show `inventory`. Exact only for vanilla
/// slot and container classes (`Slot` and the combiner's own, `SimpleContainer`,
/// `ResultContainer`): a jar `Slot` or `Container` can answer its own stack size and rules. The
/// container slots come first, then the player's 36 in `addStandardInventorySlots` order (27
/// main, then the hotbar), the order Sunrise appends them in, so a Java slot index is the
/// embedder's slot index.
fn classify_layout(host: &StdHost, menu: &Value, inventory: &Value) -> Result<MenuLayout, String> {
    let Some(menu_models::Object::Menu(state)) =
        expect_ref(menu).and_then(|key| host.menu_states.get(key))
    else {
        return Err("its menu is not one Decaf models".to_owned());
    };
    let data_slots = state.live.data_slots.len();
    let slots = menu_models::menu_broadcast_slots(&host.menu_states, menu)
        .ok_or_else(|| "its slots are not ones Decaf models".to_owned())?;
    let mut roles = Vec::with_capacity(slots.len());
    let mut player = Vec::new();
    let mut storages = std::collections::BTreeSet::new();
    for slot in &slots {
        let slot_class = jar_object::object_class(&slot.slot).unwrap_or("?");
        if !matches!(slot_class, SLOT | ICM_INPUT_SLOT | ICM_RESULT_SLOT) {
            return Err(format!(
                "slot {} is a {slot_class}, whose own rules a declaration can't carry",
                slot.index
            ));
        }
        if slot.container == *inventory {
            player.push(slot.container_slot);
            roles.push(MenuSlotRole::Player(slot.container_slot));
            continue;
        }
        if !player.is_empty() {
            return Err(format!(
                "container slot {} comes after player inventory slots",
                slot.index
            ));
        }
        let container_class = jar_object::object_class(&slot.container).unwrap_or("?");
        if !matches!(
            container_class,
            SIMPLE_CONTAINER | RESULT_CONTAINER | ICM_INPUT_CONTAINER | ICM_RESULT_CONTAINER
        ) {
            return Err(format!(
                "slot {} shows a {container_class}, whose stack size a declaration can't carry",
                slot.index
            ));
        }
        if !storages.insert((format!("{:?}", slot.container), slot.container_slot)) {
            return Err(format!(
                "slot {} shows storage another slot already shows",
                slot.index
            ));
        }
        roles.push(MenuSlotRole::Owned {
            input: slot_class == ICM_INPUT_SLOT,
        });
    }
    let standard: Vec<i32> = (9..36).chain(0..9).collect();
    if !player.is_empty() && player != standard {
        return Err("its player slots are not vanilla's 27 main slots then the hotbar".to_owned());
    }
    Ok(MenuLayout {
        slots: roles,
        data_slots,
    })
}

impl StdHost {
    /// The layout of the Java menu Decaf holds for `uuid` (what [`MenuLayout::declaration`]
    /// declares for a menu type), read from Decaf's own state: an embedder compares it with the
    /// menu type it declared before showing the menu. `Err` when no menu is held or its layout
    /// isn't one a declaration carries.
    pub fn held_menu_layout(&self, uuid: u128) -> Result<MenuLayout, String> {
        let entry = self
            .block_use
            .players
            .get(&uuid)
            .ok_or_else(|| "no menu is held".to_owned())?;
        let open = entry
            .open
            .as_ref()
            .ok_or_else(|| "no menu is held".to_owned())?;
        let inventory = entry
            .inventory
            .as_ref()
            .ok_or_else(|| "the menu's inventory is unknown".to_owned())?;
        classify_layout(self, &open.menu, inventory)
    }
}

impl NativeJar {
    /// Each `MenuType` the jar registered under its own namespace, with the layout every menu its
    /// `MenuSupplier` builds has: declared only when `menu_structure` proves from the bytecode that
    /// the layout can't depend on the player, inventory, level or static state, and then read
    /// off one construction (`MenuType.create(0, inventory)`, as a server player's). Every other
    /// type gets the reason it can't be declared, never a guessed layout.
    pub fn menu_type_declarations(&self, host: &mut StdHost) -> Vec<MenuTypeDeclaration> {
        let types: Vec<(String, Value)> = host
            .registrations()
            .iter()
            .filter(|entry| entry.registry == "minecraft:menu")
            .filter(|entry| !entry.id.starts_with("minecraft:"))
            .map(|entry| (entry.id.clone(), entry.value.clone()))
            .collect();
        types
            .into_iter()
            .map(|(id, menu_type)| MenuTypeDeclaration {
                layout: self.menu_type_layout(host, menu_type),
                id,
            })
            .collect()
    }

    fn menu_type_layout(&self, host: &mut StdHost, menu_type: Value) -> Result<MenuLayout, String> {
        let before = host.unknown_host_call_count();
        let (menu, inventory) = self.build_menu_for_declaration(host, menu_type)?;
        if host.unknown_host_call_count() != before {
            return Err("building its menu leaned on an unmodelled call".to_owned());
        }
        if menu == Value::Ref(None) {
            return Err("its MenuSupplier returned null".to_owned());
        }
        classify_layout(host, &menu, &inventory)
    }

    /// `MenuType.create(0, inventory)` (26.3 offsets 0-11: the type's `MenuSupplier`) for a
    /// server player of no one in particular, in the overworld, once `menu_structure` found its
    /// layout fixed: the menu and the `Inventory` its player slots show. A server player's, since
    /// that is the menu the live-menu bridge runs and a constructor may read
    /// `inventory.player.level()` (Runes' does), which only the server side of a block use
    /// answers here.
    fn build_menu_for_declaration(
        &self,
        host: &mut StdHost,
        menu_type: Value,
    ) -> Result<(Value, Value), String> {
        let player_data = BlockUsePlayerData {
            uuid: 0,
            name: "Player".to_owned(),
            entity_id: None,
            incarnation: None,
            container: PlayerContainer::Inventory,
            sneaking: false,
            spectator: false,
            main_hand: None,
            off_hand: None,
        };
        let dimension = "minecraft:overworld";
        let level = game_object::server_value(format!("game/level/{dimension}"));
        let player = block_use::server_player_value(&player_data);
        let inventory = menu_models::allocate_inventory(
            &mut host.menu_states,
            &mut host.next_menu_object,
            player.clone(),
        );
        let previous_active = host.block_use.active_use.replace(ActiveBlockUse {
            side: BlockUseSide::Server,
            dimension: dimension.to_owned(),
            level: level.clone(),
            block: None,
            player: player.clone(),
            player_data,
            player_inventory: inventory.clone(),
            container: PlayerContainer::Inventory,
            menu_effects: Vec::new(),
            facts: None,
            live_effects: Vec::new(),
            wrapped_click_cause: None,
        });
        let roots = [menu_type.clone(), level, player, inventory.clone()];
        let mut refusal = None;
        let built = self.with_persistent_runtime(host, &roots, |runtime| {
            if let Err(reason) = crate::runtime::menu_structure::check(runtime, &menu_type) {
                refusal = Some(reason);
                return Ok(Value::Ref(None));
            }
            let create = mref(
                menu_models::MENU_TYPE,
                "create",
                "(ILnet/minecraft/world/entity/player/Inventory;)Lnet/minecraft/world/inventory/AbstractContainerMenu;",
            );
            runtime
                .invoke_menu_model(&create, &menu_type, &[Value::Int(0), inventory.clone()])?
                .ok_or_else(|| {
                    NativeError::Metadata("MenuType was not constructed with a MenuSupplier".into())
                })
        });
        host.block_use.active_use = previous_active;
        if let Some(reason) = refusal {
            return Err(reason);
        }
        let menu =
            built.map_err(|error| format!("its MenuSupplier did not build a menu: {error}"))?;
        Ok((menu, inventory))
    }
}

/// Start the held `menu` from the embedder's cursor `carried` ([`MenuPlayerFacts::carried`]): a
/// stack equal to the menu's own keeps the menu's Java object. Answers whether a non-empty
/// cursor was replaced by a different stack.
fn replace_carried(host: &mut StdHost, menu: &Value, carried: &Value) -> bool {
    let Some(key) = expect_ref(menu).cloned() else {
        return false;
    };
    let old = match host.menu_states.get(&key) {
        Some(menu_models::Object::Menu(state)) => state.live.carried.clone(),
        _ => return false,
    };
    if same_stack(host, &old, carried) {
        return false;
    }
    // An opaque or unmodelled old cursor counts as non-empty: it may have held something.
    let old_non_empty =
        recipe_models::extract_stack_components(host, &old).is_none_or(|stack| stack.count > 0);
    if let Some(menu_models::Object::Menu(state)) = host.menu_states.get_mut(&key) {
        state.live.carried = carried.clone();
    }
    old_non_empty
}

/// The held menu's carried stack and slot contents, read from Decaf's own state (a slot or
/// container a jar subclass implements has no such state: then `None`).
fn held_menu_contents(host: &StdHost, menu: &Value) -> (Option<Value>, Option<Vec<Value>>) {
    let Some(menu_models::Object::Menu(state)) =
        expect_ref(menu).and_then(|key| host.menu_states.get(key))
    else {
        return (None, None);
    };
    let carried = Some(state.live.carried.clone());
    let Some(slots) = menu_models::menu_broadcast_slots(&host.menu_states, menu) else {
        return (carried, None);
    };
    let mut out = Vec::with_capacity(slots.len());
    for slot in slots {
        // Only Decaf's own vanilla objects: a jar `Slot` or `Container` subclass may override
        // `getItem`, which this read (outside any Java call) must not run.
        let vanilla_slot = matches!(
            jar_object::object_class(&slot.slot),
            Some(SLOT | ICM_INPUT_SLOT | ICM_RESULT_SLOT)
        );
        let vanilla_container = matches!(
            jar_object::object_class(&slot.container),
            Some(
                SIMPLE_CONTAINER
                    | RESULT_CONTAINER
                    | ICM_INPUT_CONTAINER
                    | ICM_RESULT_CONTAINER
                    | INVENTORY
            )
        );
        if !vanilla_slot || !vanilla_container {
            return (carried, None);
        }
        let stack = match slot.known_current {
            Some(stack) => stack,
            None => {
                // A player-inventory slot: read the inventory object.
                let Some(menu_models::Object::Inventory {
                    contents: Some(contents),
                    ..
                }) = expect_ref(&slot.container).and_then(|key| host.menu_states.get(key))
                else {
                    return (carried, None);
                };
                let index = slot.container_slot as usize;
                let stack = if index < contents.items.len() {
                    contents.items.get(index).cloned()
                } else {
                    contents
                        .equipment
                        .as_ref()
                        .and_then(|e| e.get(index - contents.items.len()).cloned())
                };
                match stack {
                    Some(stack) => stack,
                    None => return (carried, None),
                }
            }
        };
        out.push(stack);
    }
    (carried, Some(out))
}

/// One live-menu call on the held `menu`; `Ok(true)` when vanilla's handler acted on it.
fn run_call<H: Host>(
    rt: &mut Runtime<'_, H>,
    menu: &Value,
    player: &Value,
    call: LiveCall<'_>,
) -> Result<bool, NativeError> {
    let outer = match call {
        LiveCall::Click(_) => mref(
            "net/minecraft/server/network/ServerGamePacketListenerImpl",
            "handleContainerClick",
            "(Lnet/minecraft/network/protocol/game/ServerboundContainerClickPacket;)V",
        ),
        LiveCall::Tick => mref(SERVER_PLAYER, "tick", "()V"),
        LiveCall::ValidityCheck => mref(SERVER_PLAYER, "doTick", "()V"),
        LiveCall::Close => {
            block_use::run_do_close_container(rt, player)?;
            return Ok(true);
        }
    };
    let result = match call {
        LiveCall::Click(click) => handle_click(rt, menu, player, click),
        LiveCall::Tick => vcall(rt, Site::Virtual, ACM, "broadcastChanges", "()V", menu, &[])
            .and_then(|_| validity_check(rt, menu, player))
            .map(|()| true),
        LiveCall::ValidityCheck => validity_check(rt, menu, player).map(|()| true),
        LiveCall::Close => unreachable!(),
    };
    match result {
        Ok(handled) => Ok(handled),
        Err(Flow::Java(error)) => Err(error),
        Err(Flow::Counted) => {
            rt.host.unknown_operand_call(&outer)?;
            Ok(true)
        }
    }
}

/// `if (!containerMenu.stillValid(this)) { closeContainer(); containerMenu = inventoryMenu; }`.
fn validity_check<H: Host>(rt: &mut Runtime<'_, H>, menu: &Value, player: &Value) -> R<()> {
    let valid = as_bool(&vcall(
        rt,
        Site::Virtual,
        ACM,
        "stillValid",
        D_PLAYER_Z,
        menu,
        std::slice::from_ref(player),
    )?)?;
    if valid {
        return Ok(());
    }
    let container_id = menu_container_id(rt, menu)?;
    record(rt, LiveMenuEffect::Closed { container_id })?;
    block_use::run_close_container(rt, player)?;
    Ok(())
}

/// 26.3 `ServerGamePacketListenerImpl.handleContainerClick` offsets 19-352 on the held menu.
fn handle_click<H: Host>(
    rt: &mut Runtime<'_, H>,
    menu: &Value,
    player: &Value,
    click: &MenuClick,
) -> R<bool> {
    // 19-36: a packet for another window is dropped (the caller found this one by its id).
    // 37-67: a spectator, or a dead player, is sent the whole menu instead.
    let (spectator, dead) = with_active(rt, |active| {
        Ok((
            active.player_data.spectator,
            active.facts.as_ref().map(|facts| facts.dead),
        ))
    })?;
    let dead = dead.ok_or_else(counted)?;
    if spectator || dead {
        vcall(
            rt,
            Site::Virtual,
            ACM,
            "sendAllDataToRemote",
            "()V",
            menu,
            &[],
        )?;
        return Ok(false);
    }
    // 68-107.
    if !as_bool(&vcall(
        rt,
        Site::Virtual,
        ACM,
        "stillValid",
        D_PLAYER_Z,
        menu,
        std::slice::from_ref(player),
    )?)? {
        return Ok(false);
    }
    // 108-178.
    if !as_bool(&vcall(
        rt,
        Site::Virtual,
        ACM,
        "isValidSlotIndex",
        "(I)Z",
        menu,
        &[Value::Int(click.slot)],
    )?)? {
        return Ok(false);
    }
    // 179-201: `packet.stateId() != menu.getStateId()` (the embedder compared them: it owns
    // the window's state ids). A menu overriding `getStateId` would be asked here.
    if rt
        .jvm_selected_jar_body(&mref(ACM, "getStateId", "()I"), 0xb6, Some(menu))?
        .is_some()
    {
        return Err(counted());
    }
    let mismatch = !click.state_id_matches;
    // 202-232.
    vcall(
        rt,
        Site::Virtual,
        ACM,
        "suppressRemoteUpdates",
        "()V",
        menu,
        &[],
    )?;
    vcall(
        rt,
        Site::Virtual,
        ACM,
        "clicked",
        D_CLICKED,
        menu,
        &[
            Value::Int(click.slot),
            Value::Int(click.button),
            click.input.to_value(),
            player.clone(),
        ],
    )?;
    // 235-312: the packet's predicted slots and cursor would be `receive`d into the remote
    // slots here; the embedder keeps the client side, so the remote view stays the server's.
    // The client's `stateId` is then no longer Decaf's to know.
    menu_live_mut(rt, menu, |live| live.state_id_known = false)?;
    // 315-349.
    vcall(
        rt,
        Site::Virtual,
        ACM,
        "resumeRemoteUpdates",
        "()V",
        menu,
        &[],
    )?;
    let broadcast = if mismatch {
        "broadcastFullState"
    } else {
        "broadcastChanges"
    };
    vcall(rt, Site::Virtual, ACM, broadcast, "()V", menu, &[])?;
    Ok(true)
}
