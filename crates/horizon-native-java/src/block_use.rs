//! Generic Minecraft 26.3 block-use (`BlockBehaviour.useItemOn`, `useWithoutItem`,
//! `getMenuProvider`), sealed `InteractionResult` hierarchy, `BlockHitResult`, and
//! `ServerPlayer.openMenu(MenuProvider)` bridge.
//!
//! Nothing here knows any specific mod. Every answer matches Java 25 / Minecraft 26.3
//! bytecode (`server-26.3.jar`) or is counted as an unknown host call.

use std::collections::BTreeMap;

use crate::{
    ExecutionBudget, Host, MethodRef, NativeError, NativeJar, Runtime, StdHost, Value,
    component_value, expect_ref, facade_text, game_object, host_ids, host_optional,
    host_ref::{ClassHierarchy, HostKind, HostRef, Methods, exact_class_rules_out},
    jar_object, jdk_call, menu_models, null_checks, server_values, uuid_value,
    vanilla_block_use_methods,
};

pub(crate) const INTERACTION_RESULT: &str = "net/minecraft/world/InteractionResult";
pub(crate) const INTERACTION_RESULT_SUCCESS: &str = "net/minecraft/world/InteractionResult$Success";
pub(crate) const INTERACTION_RESULT_FAIL: &str = "net/minecraft/world/InteractionResult$Fail";
pub(crate) const INTERACTION_RESULT_PASS: &str = "net/minecraft/world/InteractionResult$Pass";
pub(crate) const INTERACTION_RESULT_TRY_EMPTY: &str =
    "net/minecraft/world/InteractionResult$TryEmptyHandInteraction";
pub(crate) const INTERACTION_RESULT_SWING_SOURCE: &str =
    "net/minecraft/world/InteractionResult$SwingSource";
pub(crate) const INTERACTION_RESULT_ITEM_CONTEXT: &str =
    "net/minecraft/world/InteractionResult$ItemContext";

pub(crate) const BLOCK_HIT_RESULT: &str = "net/minecraft/world/phys/BlockHitResult";
const HIT_RESULT_TYPE_PREFIX: &str = "enum/net/minecraft/world/phys/HitResult$Type/";
const DIRECTION_ENUM_PREFIX: &str = "enum/net/minecraft/core/Direction/";
const HAND_ENUM_PREFIX: &str = "enum/net/minecraft/world/InteractionHand/";
const SWING_SOURCE_ENUM_PREFIX: &str = "enum/net/minecraft/world/InteractionResult$SwingSource/";

pub(crate) const BLOCK_BEHAVIOUR: &str = "net/minecraft/world/level/block/state/BlockBehaviour";
pub(crate) const BLOCK_STATE_BASE: &str =
    "net/minecraft/world/level/block/state/BlockBehaviour$BlockStateBase";
pub(crate) const BLOCK_STATE: &str = "net/minecraft/world/level/block/state/BlockState";

pub(crate) const USE_ITEM_ON_DESC: &str = "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;";
pub(crate) const USE_WITHOUT_ITEM_DESC: &str = "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;";
pub(crate) const GET_MENU_PROVIDER_DESC: &str = "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/MenuProvider;";

const BLOCK_STATE_USE_ITEM_ON_DESC: &str = "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;";
const BLOCK_STATE_USE_WITHOUT_ITEM_DESC: &str = "(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;";
const BLOCK_STATE_GET_MENU_PROVIDER_DESC: &str = "(Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/MenuProvider;";
const PLAYER_OPEN_MENU_DESC: &str = "(Lnet/minecraft/world/MenuProvider;)Ljava/util/OptionalInt;";

const STATIC_SUCCESS: &str = "static/SUCCESS";
const STATIC_SUCCESS_SERVER: &str = "static/SUCCESS_SERVER";
const STATIC_CONSUME: &str = "static/CONSUME";
const STATIC_FAIL: &str = "static/FAIL";
const STATIC_PASS: &str = "static/PASS";
const STATIC_TRY_EMPTY: &str = "static/TRY_WITH_EMPTY_HAND";

const STATIC_ITEM_CONTEXT_NONE: &str = "static/NONE";
const STATIC_ITEM_CONTEXT_DEFAULT: &str = "static/DEFAULT";

/// Logical side on which a block-use call runs (`Level.isClientSide()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockUseSide {
    Server,
    Client,
}

/// Exact data of a Minecraft 26.3 `BlockHitResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockHitData {
    pub location: [f64; 3],
    pub direction: String,
    pub pos: [i32; 3],
    pub inside: bool,
    pub miss: bool,
    pub world_border_hit: bool,
}

impl BlockHitData {
    /// A normal non-miss block hit at `pos` on face `direction` (`UP`, `NORTH`, ...).
    pub fn block(
        pos: [i32; 3],
        direction: impl Into<String>,
        location: [f64; 3],
        inside: bool,
    ) -> Self {
        Self {
            location,
            direction: direction.into(),
            pos,
            inside,
            miss: false,
            world_border_hit: false,
        }
    }
}

/// What the embedder knows of a server player's `containerMenu` when a block use starts.
///
/// `ServerPlayer.openMenu` (26.3, offsets 8-20) first closes the open container unless it is
/// the player's `inventoryMenu`, so Decaf must know which one is open before Java builds the
/// new menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerContainer {
    /// `containerMenu == inventoryMenu`: no container is open.
    Inventory,
    /// A container whose `containerId` is this is open.
    Open(i32),
    /// The embedder can't tell (a menu it has no handle for).
    Unknown,
}

/// Player snapshot handed into a block-use invocation.
#[derive(Clone, Debug)]
pub struct BlockUsePlayerData {
    pub uuid: u128,
    pub name: String,
    pub entity_id: Option<i32>,
    /// Which `ServerPlayer` object this is, as (session, life): a login and each
    /// `PlayerList.respawn` build a new one. `None` when the embedder can't tell; Decaf then
    /// keeps no per-object state for the player and counts what needs it
    /// (`containerCounter`).
    pub incarnation: Option<(u32, u32)>,
    /// The player's open container as the block use starts (server side only).
    pub container: PlayerContainer,
    pub sneaking: bool,
    pub spectator: bool,
    /// Optional held stacks (`ItemStack` host values). When `None`, `getMainHandItem` /
    /// `getOffhandItem` / `getItemInHand` remain counted rather than invented.
    pub main_hand: Option<Value>,
    pub off_hand: Option<Value>,
}

impl Default for BlockUsePlayerData {
    fn default() -> Self {
        Self {
            uuid: 1,
            name: "Player".to_owned(),
            entity_id: Some(1),
            incarnation: Some((1, 0)),
            container: PlayerContainer::Inventory,
            sneaking: false,
            spectator: false,
            main_hand: None,
            off_hand: None,
        }
    }
}

/// Full vanilla context for a single `Block.useWithoutItem` or `Block.useItemOn` dispatch.
#[derive(Clone, Debug)]
pub struct BlockUseContext {
    pub side: BlockUseSide,
    pub dimension: String,
    pub block_id: String,
    pub state_properties: BTreeMap<String, String>,
    pub pos: [i32; 3],
    pub hit: BlockHitData,
    pub player: BlockUsePlayerData,
    /// `InteractionHand` enum constant name: `"MAIN_HAND"` or `"OFF_HAND"`.
    pub hand: String,
    /// Held `ItemStack` for `useItemOn` (`None` uses `ItemStack.EMPTY`).
    pub stack: Option<Value>,
}

/// Minecraft 26.3 `InteractionResult$SwingSource`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwingSource {
    None,
    Predicted,
    ServerOnly,
}

impl SwingSource {
    fn enum_name(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Predicted => "PREDICTED",
            Self::ServerOnly => "SERVER_ONLY",
        }
    }

    fn from_value(value: &Value) -> Option<Self> {
        let name = expect_ref(value)?.strip_prefix(SWING_SOURCE_ENUM_PREFIX)?;
        match name {
            "NONE" => Some(Self::None),
            "PREDICTED" => Some(Self::Predicted),
            "SERVER_ONLY" => Some(Self::ServerOnly),
            _ => None,
        }
    }

    fn to_value(self) -> Value {
        Value::Ref(Some(format!(
            "{SWING_SOURCE_ENUM_PREFIX}{}",
            self.enum_name()
        )))
    }
}

/// Minecraft 26.3 `InteractionResult$ItemContext`.
#[derive(Clone, Debug, PartialEq)]
pub struct InteractionItemContext {
    pub was_item_interaction: bool,
    pub held_item_transformed_to: Option<Value>,
}

/// Decoded Minecraft 26.3 `InteractionResult` sealed value.
#[derive(Clone, Debug, PartialEq)]
pub enum InteractionResultValue {
    Success {
        swing_source: SwingSource,
        item_context: InteractionItemContext,
    },
    Fail,
    Pass,
    TryEmptyHandInteraction,
}

impl InteractionResultValue {
    /// Vanilla `InteractionResult.consumesAction()`.
    pub fn consumes_action(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Vanilla `InteractionResult$Success.shouldSwing()` (`false` for non-`Success`).
    pub fn should_swing(&self) -> bool {
        matches!(
            self,
            Self::Success {
                swing_source: SwingSource::Predicted | SwingSource::ServerOnly,
                ..
            }
        )
    }

    /// Vanilla `SwingSource` when `Success`, `None` otherwise.
    pub fn swing_source(&self) -> Option<SwingSource> {
        match self {
            Self::Success { swing_source, .. } => Some(*swing_source),
            _ => None,
        }
    }
}

/// Typed effect recorded when server-side Java calls `player.openMenu(MenuProvider)`.
#[derive(Clone, Debug)]
pub struct OpenMenuEffect {
    /// Per-player server container sync id (`1..=100`, matching `ServerPlayer.nextContainerCounter`).
    pub container_id: i32,
    /// Java `MenuType` object returned by `menu.getType()`.
    pub menu_type: Value,
    /// Resolved registry id (e.g. `"example:bench"` or `"minecraft:crafting"`), when known.
    pub menu_type_id: Option<String>,
    /// Java `Component` returned by `provider.getDisplayName()`.
    pub title: Value,
    /// Exact vanilla JSON of `title`, when representable.
    pub title_json: Option<serde_json::Value>,
    /// Block position carried by `ContainerLevelAccess.create(level, pos)`, if any.
    pub access_pos: Option<[i32; 3]>,
    /// Java `MenuConstructor` lambda captured by `SimpleMenuProvider`, kept for the live-menu
    /// bridge.
    pub constructor: Option<Value>,
    /// Java `MenuProvider` passed to `player.openMenu(...)`.
    pub provider: Value,
    /// Live `AbstractContainerMenu` returned by `provider.createMenu(...)`.
    pub menu: Value,
}

impl OpenMenuEffect {
    /// Every Java value the effect holds: what keeping it alive across collections means.
    fn values(&self) -> impl Iterator<Item = &Value> {
        [&self.menu_type, &self.title, &self.provider, &self.menu]
            .into_iter()
            .chain(self.constructor.as_ref())
    }
}

/// What a block use did to the player's open container, in bytecode order.
#[derive(Clone, Debug)]
pub enum MenuEffect {
    /// `ServerPlayer.closeContainer()` sent the client `ClientboundContainerClosePacket` for
    /// the Java menu Decaf held as the player's `containerMenu`, before its `doCloseContainer`
    /// ran: the embedder closes the window it showed for it. When that menu's `removed` threw,
    /// the menu stays installed with no window ([`StdHost::player_open_menu`] still has it).
    Closed { container_id: i32 },
    /// `ServerPlayer.openMenu` opened a Java menu. The last one a use opens is the player's
    /// `containerMenu` afterwards, and Decaf holds it ([`StdHost::player_open_menu`]).
    Opened(OpenMenuEffect),
    /// A menu a nested `openMenu` installed (one a closing menu's `removed`, or a menu's
    /// constructor, opened) was overwritten by the outer call's `containerMenu = ...`
    /// assignment: vanilla's server forgets it with no `removed` and no close packet, while the
    /// client still shows its window.
    Orphaned { container_id: i32 },
    /// `ServerPlayer.openMenu` advanced `containerCounter` to `container_id`
    /// (`nextContainerCounter`, offsets 23-24) and the provider's `createMenu` answered `null`
    /// (offsets 43-73): no window opens, but the next menu takes the id after this one. The
    /// embedder advances its own counter the same way.
    CounterAdvanced { container_id: i32 },
}

/// Result of running a block's `useItemOn` or `useWithoutItem` hook.
#[derive(Clone, Debug)]
pub struct BlockUseOutcome {
    /// Decoded `InteractionResult`, or `None` if Java returned `null` or a non-`InteractionResult` value.
    pub result: Option<InteractionResultValue>,
    /// Raw Java return value.
    pub raw_result: Value,
    /// Menus closed and opened through `ServerPlayer.openMenu(MenuProvider)`, in order.
    pub menu_effects: Vec<MenuEffect>,
    /// What the opened menus' `initMenu` (and anything else of `live_menu.rs`) recorded: the
    /// initial contents and listener effects, when the embedder gave the player's facts
    /// ([`StdHost::set_player_menu_facts`]).
    pub live_effects: Vec<crate::live_menu::LiveMenuEffect>,
    /// The player's inventory after the use, when the facts gave it.
    pub inventory: Option<Vec<Value>>,
    /// Unmodelled host calls recorded during this block-use invocation.
    pub unknown_host_calls: BTreeMap<String, usize>,
    /// Total count of unmodelled host calls during this block-use invocation.
    pub unknown_host_call_count: usize,
    /// Why the hook didn't return (an exception it didn't catch, a budget it ran out of).
    /// `menu_effects` still lists what it did before that: a close packet vanilla sent stays
    /// sent.
    pub failure: Option<String>,
}

impl BlockUseOutcome {
    /// The menus this use opened, in order.
    pub fn open_menus(&self) -> impl Iterator<Item = &OpenMenuEffect> {
        self.menu_effects.iter().filter_map(|effect| match effect {
            MenuEffect::Opened(open) => Some(open),
            MenuEffect::Closed { .. }
            | MenuEffect::Orphaned { .. }
            | MenuEffect::CounterAdvanced { .. } => None,
        })
    }
}

/// Result of [`NativeJar::close_player_menu`].
#[derive(Clone, Debug)]
pub struct MenuCloseOutcome {
    /// The `containerId` of the Java menu that closed.
    pub container_id: i32,
    /// What its `removed` did to the player's menus, in order: a `removed` may open another.
    pub menu_effects: Vec<MenuEffect>,
    /// What its `removed` did in the order it did it, as a block use's
    /// [`BlockUseOutcome::live_effects`]: a menu it opened sent its initial data
    /// (`LiveMenuEffect::InitialData`) here.
    pub live_effects: Vec<crate::live_menu::LiveMenuEffect>,
    /// Unmodelled host calls its close made.
    pub unknown_host_calls: BTreeMap<String, usize>,
    pub unknown_host_call_count: usize,
    /// Why the close didn't finish (its `removed` threw): the menu is then still installed.
    pub failure: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct SuccessData {
    pub(crate) swing_source: Value,
    pub(crate) item_context: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct ItemContextData {
    pub(crate) was_item_interaction: bool,
    pub(crate) held_item_transformed_to: Value,
}

/// The server player a Java call is running for: a block use, or the close of the menu Decaf
/// holds for them ([`NativeJar::close_player_menu`]).
#[derive(Clone, Debug)]
pub(crate) struct ActiveBlockUse {
    pub(crate) side: BlockUseSide,
    pub(crate) dimension: String,
    pub(crate) level: Value,
    /// The clicked block's state and position; `None` for a close, which has no block.
    pub(crate) block: Option<(Value, [i32; 3])>,
    pub(crate) player: Value,
    pub(crate) player_data: BlockUsePlayerData,
    pub(crate) player_inventory: Value,
    /// The player's `containerMenu` as it stands at this point of the call: the embedder's
    /// snapshot at the start, then what this call's closes and opens made it.
    pub(crate) container: PlayerContainer,
    pub(crate) menu_effects: Vec<MenuEffect>,
    /// What the embedder told Decaf of the player for this call (`live_menu.rs`).
    pub(crate) facts: Option<crate::live_menu::MenuPlayerFacts>,
    /// The live-menu effects this call recorded, in order.
    pub(crate) live_effects: Vec<crate::live_menu::LiveMenuEffect>,
}

/// One `ServerPlayer` object's menu state, as Decaf models it (#629 review P1 1 and 2).
///
/// Vanilla keeps both on the `ServerPlayer`: `containerCounter` is a plain `int` field that only
/// `nextContainerCounter` writes (`server-26.3.jar`: the constructor leaves it 0 and
/// `restoreFrom` does not copy it), and `containerMenu` is the one open menu. A login and each
/// `PlayerList.respawn` build a new object, so both start over there.
#[derive(Clone, Debug)]
pub(crate) struct PlayerMenus {
    /// The object, as (session, life) ([`BlockUsePlayerData::incarnation`]).
    pub(crate) incarnation: (u32, u32),
    /// `containerCounter`, or `None` once Decaf can no longer know it: Decaf was not told the
    /// object was created, a container opened that the embedder could not read the id of, or
    /// the embedder refused a menu Java opened (Java advanced the counter, the embedder's
    /// server did not).
    pub(crate) counter: Option<i32>,
    /// The Java menu Decaf opened that is still the object's `containerMenu`. Its Java values
    /// are retained on the host while it is held, and released when it is dropped.
    pub(crate) open: Option<OpenMenuEffect>,
    /// The `Inventory` object `open`'s player slots read (the one its open built), retained
    /// with it: the live-menu calls refill it from the embedder's facts.
    pub(crate) inventory: Option<Value>,
    /// `closeContainer` sent the client `open`'s close packet, so no window shows it, but its
    /// `doCloseContainer` hasn't reached `containerMenu = inventoryMenu` (it is running, or its
    /// `removed` threw): vanilla still has it installed, and its next `openMenu` closes it
    /// again.
    pub(crate) close_sent: bool,
}

/// Host state for block-use, `InteractionResult`, `BlockHitResult`, and `openMenu`.
#[derive(Clone, Debug, Default)]
pub(crate) struct BlockUseHostState {
    pub(crate) interaction_results: BTreeMap<String, SuccessData>,
    pub(crate) interaction_item_contexts: BTreeMap<String, ItemContextData>,
    pub(crate) block_hits: BTreeMap<String, BlockHitData>,
    pub(crate) active_use: Option<ActiveBlockUse>,
    /// Each player's current `ServerPlayer` object's menu state, by UUID. At most one entry
    /// (and so at most one held menu) per player: a newer object's entry replaces an older's.
    pub(crate) players: BTreeMap<u128, PlayerMenus>,
    /// The slots whose jar `Slot.onTake` override `vcall` dispatched and is running, with the
    /// interpreter depth of the override's frame, so its direct `super.onTake` (run in that
    /// frame) isn't told as a second take, while a nested take is (`live_menu::note_take`).
    pub(crate) taking: Vec<(Value, usize)>,
    pub(crate) next_id: u64,
    /// What the embedder told Decaf of each player for their next block use
    /// ([`StdHost::set_player_menu_facts`]).
    pub(crate) facts: BTreeMap<u128, crate::live_menu::MenuPlayerFacts>,
}

impl BlockUseHostState {
    fn next_serial(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// The entry of the `ServerPlayer` object `incarnation` of `uuid`, if Decaf has one.
    pub(crate) fn player(
        &self,
        uuid: u128,
        incarnation: Option<(u32, u32)>,
    ) -> Option<&PlayerMenus> {
        let incarnation = incarnation?;
        self.players
            .get(&uuid)
            .filter(|entry| entry.incarnation == incarnation)
    }

    fn player_mut(
        &mut self,
        uuid: u128,
        incarnation: Option<(u32, u32)>,
    ) -> Option<&mut PlayerMenus> {
        let incarnation = incarnation?;
        self.players
            .get_mut(&uuid)
            .filter(|entry| entry.incarnation == incarnation)
    }

    pub(crate) fn alloc_success(&mut self, swing_source: Value, item_context: Value) -> Value {
        let id = self.next_serial();
        let key = format!("new/Success#ir{id}");
        self.interaction_results.insert(
            key.clone(),
            SuccessData {
                swing_source,
                item_context,
            },
        );
        Value::Host(HostRef::new(HostKind::InteractionResult, key))
    }

    pub(crate) fn alloc_empty_result(&mut self, variant: &str) -> Value {
        let id = self.next_serial();
        let key = format!("new/{variant}#ir{id}");
        Value::Host(HostRef::new(HostKind::InteractionResult, key))
    }

    pub(crate) fn alloc_item_context(
        &mut self,
        was_item_interaction: bool,
        held_item_transformed_to: Value,
    ) -> Value {
        let id = self.next_serial();
        let key = format!("new/ItemContext#ir{id}");
        self.interaction_item_contexts.insert(
            key.clone(),
            ItemContextData {
                was_item_interaction,
                held_item_transformed_to,
            },
        );
        Value::Host(HostRef::new(HostKind::InteractionItemContext, key))
    }

    pub(crate) fn alloc_block_hit(&mut self, data: BlockHitData) -> Value {
        let id = self.next_serial();
        let key = format!("game/block-hit/hit#hit{id}");
        self.block_hits.insert(key.clone(), data);
        Value::Host(HostRef::new(HostKind::GameTransient, key))
    }

    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        fn push_val<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
            out.extend(crate::heap::value_texts(v));
        }
        // A held menu (`PlayerMenus::open`) is kept through `StdHost::retain`, so only the
        // running call's own values are roots here.
        for (slot, _) in &self.taking {
            push_val(slot, out);
        }
        if let Some(active) = &self.active_use {
            push_val(&active.level, out);
            if let Some((state, _)) = &active.block {
                push_val(state, out);
            }
            push_val(&active.player, out);
            push_val(&active.player_inventory, out);
            if let Some(stack) = &active.player_data.main_hand {
                push_val(stack, out);
            }
            if let Some(stack) = &active.player_data.off_hand {
                push_val(stack, out);
            }
            for effect in &active.menu_effects {
                if let MenuEffect::Opened(effect) = effect {
                    for value in effect.values() {
                        push_val(value, out);
                    }
                }
            }
        }
    }

    pub(crate) fn keyed_text(&self, marker: &str) -> Option<String> {
        if let Some(entry) = self.interaction_results.get(marker) {
            return Some(format!("{entry:?}"));
        }
        if let Some(entry) = self.interaction_item_contexts.get(marker) {
            return Some(format!("{entry:?}"));
        }
        None
    }

    pub(crate) fn keyed_table_keys(&self) -> impl Iterator<Item = &str> {
        self.interaction_results
            .keys()
            .chain(self.interaction_item_contexts.keys())
            .chain(self.block_hits.keys())
            .map(String::as_str)
    }

    pub(crate) fn prune_keyed_tables(&mut self, keep: impl Fn(&str) -> bool) {
        self.interaction_results.retain(|k, _| keep(k));
        self.interaction_item_contexts.retain(|k, _| keep(k));
        self.block_hits.retain(|k, _| keep(k));
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.interaction_results.len()
            + self.interaction_item_contexts.len()
            + self.block_hits.len()
    }
}

// ---------------------------------------------------------------------------
// Vanilla MC 26.3 InteractionResult static handles & HostRef kind methods
// ---------------------------------------------------------------------------

pub(crate) fn ir_success() -> Value {
    Value::Host(HostRef::new(HostKind::InteractionResult, STATIC_SUCCESS))
}

pub(crate) fn ir_success_server() -> Value {
    Value::Host(HostRef::new(
        HostKind::InteractionResult,
        STATIC_SUCCESS_SERVER,
    ))
}

pub(crate) fn ir_consume() -> Value {
    Value::Host(HostRef::new(HostKind::InteractionResult, STATIC_CONSUME))
}

pub(crate) fn ir_fail() -> Value {
    Value::Host(HostRef::new(HostKind::InteractionResult, STATIC_FAIL))
}

pub(crate) fn ir_pass() -> Value {
    Value::Host(HostRef::new(HostKind::InteractionResult, STATIC_PASS))
}

pub(crate) fn ir_try_with_empty_hand() -> Value {
    Value::Host(HostRef::new(HostKind::InteractionResult, STATIC_TRY_EMPTY))
}

pub(crate) fn item_context_none() -> Value {
    Value::Host(HostRef::new(
        HostKind::InteractionItemContext,
        STATIC_ITEM_CONTEXT_NONE,
    ))
}

pub(crate) fn item_context_default() -> Value {
    Value::Host(HostRef::new(
        HostKind::InteractionItemContext,
        STATIC_ITEM_CONTEXT_DEFAULT,
    ))
}

pub(crate) fn interaction_result_class(key: &str) -> &'static str {
    match key {
        STATIC_SUCCESS | STATIC_SUCCESS_SERVER | STATIC_CONSUME => INTERACTION_RESULT_SUCCESS,
        STATIC_FAIL => INTERACTION_RESULT_FAIL,
        STATIC_PASS => INTERACTION_RESULT_PASS,
        STATIC_TRY_EMPTY => INTERACTION_RESULT_TRY_EMPTY,
        _ if key.starts_with("new/Success#") => INTERACTION_RESULT_SUCCESS,
        _ if key.starts_with("new/Fail#") => INTERACTION_RESULT_FAIL,
        _ if key.starts_with("new/Pass#") => INTERACTION_RESULT_PASS,
        _ if key.starts_with("new/TryEmptyHandInteraction#") => INTERACTION_RESULT_TRY_EMPTY,
        _ => INTERACTION_RESULT,
    }
}

fn static_success_parts(key: &str) -> Option<(Value, Value)> {
    match key {
        STATIC_SUCCESS => Some((SwingSource::Predicted.to_value(), item_context_default())),
        STATIC_SUCCESS_SERVER => Some((SwingSource::ServerOnly.to_value(), item_context_default())),
        STATIC_CONSUME => Some((SwingSource::None.to_value(), item_context_default())),
        _ => None,
    }
}

fn static_item_context_parts(key: &str) -> Option<(bool, Value)> {
    match key {
        STATIC_ITEM_CONTEXT_NONE => Some((false, Value::Ref(None))),
        STATIC_ITEM_CONTEXT_DEFAULT => Some((true, Value::Ref(None))),
        _ => None,
    }
}

pub(crate) fn success_parts_in_host(host: &StdHost, key: &str) -> Option<(Value, Value)> {
    static_success_parts(key).or_else(|| {
        host.block_use
            .interaction_results
            .get(key)
            .map(|d| (d.swing_source.clone(), d.item_context.clone()))
    })
}

pub(crate) fn item_context_parts_in_host(host: &StdHost, key: &str) -> Option<(bool, Value)> {
    static_item_context_parts(key).or_else(|| {
        host.block_use
            .interaction_item_contexts
            .get(key)
            .map(|d| (d.was_item_interaction, d.held_item_transformed_to.clone()))
    })
}

/// Map a `HostKind::InteractionResult` (or legacy text marker) to its Fabric event name,
/// used by `events.rs` `Combine::Interaction`.
pub(crate) fn events_interaction_name(host: &StdHost, value: &Value) -> Option<String> {
    if let Value::Host(handle) = value
        && handle.kind() == HostKind::InteractionResult
    {
        return Some(match handle.key() {
            STATIC_SUCCESS => "SUCCESS".to_owned(),
            STATIC_SUCCESS_SERVER => "SUCCESS_SERVER".to_owned(),
            STATIC_CONSUME => "CONSUME".to_owned(),
            STATIC_FAIL => "FAIL".to_owned(),
            STATIC_PASS => "PASS".to_owned(),
            STATIC_TRY_EMPTY => "TRY_WITH_EMPTY_HAND".to_owned(),
            key => match interaction_result_class(key) {
                INTERACTION_RESULT_FAIL => "FAIL".to_owned(),
                INTERACTION_RESULT_PASS => "PASS".to_owned(),
                INTERACTION_RESULT_TRY_EMPTY => "TRY_WITH_EMPTY_HAND".to_owned(),
                INTERACTION_RESULT_SUCCESS => match success_parts_in_host(host, key)
                    .and_then(|(s, _)| SwingSource::from_value(&s))
                {
                    Some(SwingSource::Predicted) => "SUCCESS".to_owned(),
                    Some(SwingSource::ServerOnly) => "SUCCESS_SERVER".to_owned(),
                    Some(SwingSource::None) => "CONSUME".to_owned(),
                    None => "SUCCESS".to_owned(),
                },
                _ => return None,
            },
        });
    }
    expect_ref(value)
        .and_then(|text| text.strip_prefix("interaction-result/"))
        .map(str::to_owned)
}

/// Decode a Java `InteractionResult` handle into a typed [`InteractionResultValue`].
pub(crate) fn decode_interaction_result(
    host: &StdHost,
    value: &Value,
) -> Option<InteractionResultValue> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::InteractionResult {
        return None;
    }
    match interaction_result_class(handle.key()) {
        INTERACTION_RESULT_FAIL => Some(InteractionResultValue::Fail),
        INTERACTION_RESULT_PASS => Some(InteractionResultValue::Pass),
        INTERACTION_RESULT_TRY_EMPTY => Some(InteractionResultValue::TryEmptyHandInteraction),
        INTERACTION_RESULT_SUCCESS => {
            let (swing_val, ctx_val) = success_parts_in_host(host, handle.key())?;
            let swing_source = SwingSource::from_value(&swing_val)?;
            let item_context = match &ctx_val {
                Value::Host(ctx) if ctx.kind() == HostKind::InteractionItemContext => {
                    let (was_item, transformed) = item_context_parts_in_host(host, ctx.key())?;
                    InteractionItemContext {
                        was_item_interaction: was_item,
                        held_item_transformed_to: if matches!(transformed, Value::Ref(None)) {
                            None
                        } else {
                            Some(transformed)
                        },
                    }
                }
                _ => return None,
            };
            Some(InteractionResultValue::Success {
                swing_source,
                item_context,
            })
        }
        _ => None,
    }
}

fn interaction_result_instance_of(
    handle: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    let exact = interaction_result_class(handle.key());
    if wanted == exact
        || wanted == INTERACTION_RESULT
        || wanted == "java/lang/Record"
        || wanted == "java/lang/Object"
    {
        return Some(true);
    }
    if matches!(
        wanted,
        INTERACTION_RESULT_SUCCESS
            | INTERACTION_RESULT_FAIL
            | INTERACTION_RESULT_PASS
            | INTERACTION_RESULT_TRY_EMPTY
            | INTERACTION_RESULT_ITEM_CONTEXT
            | INTERACTION_RESULT_SWING_SOURCE
            | "java/lang/String"
    ) || wanted.starts_with('[')
        || exact_class_rules_out(exact, wanted)
    {
        return Some(false);
    }
    hierarchy.is_instance(exact, wanted)
}

fn interaction_item_context_instance_of(
    _handle: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if wanted == INTERACTION_RESULT_ITEM_CONTEXT
        || wanted == "java/lang/Record"
        || wanted == "java/lang/Object"
    {
        return Some(true);
    }
    if matches!(
        wanted,
        INTERACTION_RESULT
            | INTERACTION_RESULT_SUCCESS
            | INTERACTION_RESULT_FAIL
            | INTERACTION_RESULT_PASS
            | INTERACTION_RESULT_TRY_EMPTY
            | INTERACTION_RESULT_SWING_SOURCE
            | "java/lang/String"
    ) || wanted.starts_with('[')
        || exact_class_rules_out(INTERACTION_RESULT_ITEM_CONTEXT, wanted)
    {
        return Some(false);
    }
    hierarchy.is_instance(INTERACTION_RESULT_ITEM_CONTEXT, wanted)
}

pub(crate) const INTERACTION_RESULT_METHODS: Methods = Methods {
    to_string: |handle, nested| {
        let key = handle.key();
        match interaction_result_class(key) {
            INTERACTION_RESULT_FAIL => Ok(Some("Fail[]".to_owned())),
            INTERACTION_RESULT_PASS => Ok(Some("Pass[]".to_owned())),
            INTERACTION_RESULT_TRY_EMPTY => Ok(Some("TryEmptyHandInteraction[]".to_owned())),
            INTERACTION_RESULT_SUCCESS => {
                let Some((swing, ctx)) =
                    static_success_parts(key).or_else(|| nested.interaction_result_success(key))
                else {
                    return Ok(None);
                };
                let swing_str = match &swing {
                    Value::Ref(None) => "null".to_owned(),
                    Value::Ref(Some(text)) => match text.strip_prefix(SWING_SOURCE_ENUM_PREFIX) {
                        Some(name) => name.to_owned(),
                        None => match nested.text(&swing)? {
                            Some(t) => t,
                            None => return Ok(None),
                        },
                    },
                    _ => match nested.text(&swing)? {
                        Some(t) => t,
                        None => return Ok(None),
                    },
                };
                let ctx_str = match &ctx {
                    Value::Ref(None) => "null".to_owned(),
                    _ => match nested.text(&ctx)? {
                        Some(t) => t,
                        None => return Ok(None),
                    },
                };
                Ok(Some(format!(
                    "Success[swingSource={swing_str}, itemContext={ctx_str}]"
                )))
            }
            _ => Ok(None),
        }
    },
    hash_code: |handle, nested| {
        let key = handle.key();
        match interaction_result_class(key) {
            INTERACTION_RESULT_FAIL | INTERACTION_RESULT_PASS | INTERACTION_RESULT_TRY_EMPTY => {
                Ok(Some(0))
            }
            INTERACTION_RESULT_SUCCESS => {
                let Some((swing, ctx)) =
                    static_success_parts(key).or_else(|| nested.interaction_result_success(key))
                else {
                    return Ok(None);
                };
                // Enum.hashCode() is Object.hashCode() (identity hash), so a non-null
                // SwingSource has no reproducible hashCode and must remain counted (None).
                let swing_hash = match &swing {
                    Value::Ref(None) => 0,
                    _ => match nested.hash(&swing)? {
                        Some(h) => h,
                        None => return Ok(None),
                    },
                };
                let ctx_hash = match &ctx {
                    Value::Ref(None) => 0,
                    _ => match nested.hash(&ctx)? {
                        Some(h) => h,
                        None => return Ok(None),
                    },
                };
                Ok(Some(swing_hash.wrapping_mul(31).wrapping_add(ctx_hash)))
            }
            _ => Ok(None),
        }
    },
    equals: |handle, other, nested| {
        let Value::Host(other_handle) = other else {
            return Ok(Some(false));
        };
        if other_handle.kind() != HostKind::InteractionResult {
            return Ok(Some(false));
        }
        if handle == other_handle {
            return Ok(Some(true));
        }
        let class_a = interaction_result_class(handle.key());
        let class_b = interaction_result_class(other_handle.key());
        if class_a != class_b {
            return Ok(Some(false));
        }
        if class_a != INTERACTION_RESULT_SUCCESS {
            return Ok(Some(true));
        }
        let Some((s1, c1)) = static_success_parts(handle.key())
            .or_else(|| nested.interaction_result_success(handle.key()))
        else {
            return Ok(None);
        };
        let Some((s2, c2)) = static_success_parts(other_handle.key())
            .or_else(|| nested.interaction_result_success(other_handle.key()))
        else {
            return Ok(None);
        };
        // Record `equals` compares components last-to-first (`itemContext`, then `swingSource`)
        // or short-circuits cleanly; both are pure (`ItemContext` / enum / null).
        let eq_ctx = match (&c1, &c2) {
            (Value::Ref(None), Value::Ref(None)) => Some(true),
            (Value::Ref(None), _) | (_, Value::Ref(None)) => Some(false),
            _ => nested.equals(&c1, &c2)?,
        };
        let Some(eq_ctx) = eq_ctx else {
            return Ok(None);
        };
        if !eq_ctx {
            return Ok(Some(false));
        }
        let eq_swing = match (&s1, &s2) {
            (Value::Ref(None), Value::Ref(None)) => Some(true),
            (Value::Ref(None), _) | (_, Value::Ref(None)) => Some(false),
            _ => Some(s1 == s2),
        };
        Ok(eq_swing)
    },
    instance_of: interaction_result_instance_of,
    crosses_jars: |_, _| true,
    compare_to: None,
};

pub(crate) const INTERACTION_ITEM_CONTEXT_METHODS: Methods = Methods {
    to_string: |handle, nested| {
        let key = handle.key();
        let Some((was_item, stack)) =
            static_item_context_parts(key).or_else(|| nested.interaction_item_context(key))
        else {
            return Ok(None);
        };
        let stack_str = match &stack {
            Value::Ref(None) => "null".to_owned(),
            _ => match nested.text(&stack)? {
                Some(t) => t,
                None => return Ok(None),
            },
        };
        Ok(Some(format!(
            "ItemContext[wasItemInteraction={was_item}, heldItemTransformedTo={stack_str}]"
        )))
    },
    hash_code: |handle, nested| {
        let key = handle.key();
        let Some((was_item, stack)) =
            static_item_context_parts(key).or_else(|| nested.interaction_item_context(key))
        else {
            return Ok(None);
        };
        let b_hash = if was_item { 1231_i32 } else { 1237_i32 };
        let stack_hash = match &stack {
            Value::Ref(None) => 0,
            _ => match nested.hash(&stack)? {
                Some(h) => h,
                None => return Ok(None),
            },
        };
        Ok(Some(b_hash.wrapping_mul(31).wrapping_add(stack_hash)))
    },
    equals: |handle, other, nested| {
        let Value::Host(other_handle) = other else {
            return Ok(Some(false));
        };
        if other_handle.kind() != HostKind::InteractionItemContext {
            return Ok(Some(false));
        }
        if handle == other_handle {
            return Ok(Some(true));
        }
        let Some((b1, s1)) = static_item_context_parts(handle.key())
            .or_else(|| nested.interaction_item_context(handle.key()))
        else {
            return Ok(None);
        };
        let Some((b2, s2)) = static_item_context_parts(other_handle.key())
            .or_else(|| nested.interaction_item_context(other_handle.key()))
        else {
            return Ok(None);
        };
        if b1 != b2 {
            return Ok(Some(false));
        }
        match (&s1, &s2) {
            (Value::Ref(None), Value::Ref(None)) => Ok(Some(true)),
            (Value::Ref(None), _) | (_, Value::Ref(None)) => Ok(Some(false)),
            _ => nested.equals(&s1, &s2),
        }
    },
    instance_of: interaction_item_context_instance_of,
    crosses_jars: |_, _| true,
    compare_to: None,
};

// ---------------------------------------------------------------------------
// Vanilla MC 26.3 MenuType static constants (verified via javap on server-26.3.jar)
// ---------------------------------------------------------------------------

const VANILLA_MENU_TYPES: &[(&str, &str)] = &[
    ("GENERIC_9x1", "minecraft:generic_9x1"),
    ("GENERIC_9x2", "minecraft:generic_9x2"),
    ("GENERIC_9x3", "minecraft:generic_9x3"),
    ("GENERIC_9x4", "minecraft:generic_9x4"),
    ("GENERIC_9x5", "minecraft:generic_9x5"),
    ("GENERIC_9x6", "minecraft:generic_9x6"),
    ("GENERIC_3x3", "minecraft:generic_3x3"),
    ("CRAFTER_3x3", "minecraft:crafter_3x3"),
    ("ANVIL", "minecraft:anvil"),
    ("BEACON", "minecraft:beacon"),
    ("BLAST_FURNACE", "minecraft:blast_furnace"),
    ("BREWING_STAND", "minecraft:brewing_stand"),
    ("CRAFTING", "minecraft:crafting"),
    ("ENCHANTMENT", "minecraft:enchantment"),
    ("FURNACE", "minecraft:furnace"),
    ("GRINDSTONE", "minecraft:grindstone"),
    ("HOPPER", "minecraft:hopper"),
    ("LECTERN", "minecraft:lectern"),
    ("LOOM", "minecraft:loom"),
    ("MERCHANT", "minecraft:merchant"),
    ("SHULKER_BOX", "minecraft:shulker_box"),
    ("SMITHING", "minecraft:smithing"),
    ("SMOKER", "minecraft:smoker"),
    ("CARTOGRAPHY_TABLE", "minecraft:cartography_table"),
    ("STONECUTTER", "minecraft:stonecutter"),
];

/// Resolve a `MenuType` value to its registry id (`namespace:path`).
pub(crate) fn resolve_menu_type_id(host: &StdHost, menu_type: &Value) -> Option<String> {
    if let Some(key) = game_object::handle_key(menu_type)
        && let Some(id) = key.strip_prefix("game/menu-type/")
    {
        return Some(id.to_owned());
    }
    host.registrations()
        .iter()
        .find(|entry| entry.registry == "minecraft:menu" && &entry.value == menu_type)
        .map(|entry| entry.id.clone())
}

// ---------------------------------------------------------------------------
// Inherited vanilla bodies of the block-use methods (#629 review P1 3)
// ---------------------------------------------------------------------------

/// `BlockBehaviour`'s own body of a block-use method, with its bit in
/// [`vanilla_block_use_methods`]. 26.3 `javap -c BlockBehaviour`: `useItemOn` is
/// `getstatic InteractionResult.TRY_WITH_EMPTY_HAND; areturn`, `useWithoutItem`
/// `getstatic InteractionResult.PASS; areturn`, `getMenuProvider` `aconst_null; areturn`.
fn block_behaviour_body(name: &str, descriptor: &str) -> Option<(u8, Value)> {
    match (name, descriptor) {
        ("useItemOn", USE_ITEM_ON_DESC) => Some((
            vanilla_block_use_methods::USE_ITEM_ON,
            ir_try_with_empty_hand(),
        )),
        ("useWithoutItem", USE_WITHOUT_ITEM_DESC) => {
            Some((vanilla_block_use_methods::USE_WITHOUT_ITEM, ir_pass()))
        }
        ("getMenuProvider", GET_MENU_PROVIDER_DESC) => Some((
            vanilla_block_use_methods::GET_MENU_PROVIDER,
            Value::Ref(None),
        )),
        _ => None,
    }
}

/// Run the vanilla body an object of vanilla block class `vanilla_class` (the first class of
/// its superclass chain outside the jar) has for block-use method `name`: the body of the
/// nearest class on that chain that declares it. Decaf answers it when that class is
/// `BlockBehaviour`; any other declaring class (`AbstractFurnaceBlock.useWithoutItem` opens a
/// furnace menu, `CraftingTableBlock.getMenuProvider` builds a crafting one) is counted as
/// that class's method, and so is a class 26.3 has no block of.
fn run_inherited_block_method<H: Host>(
    host: &mut H,
    vanilla_class: &str,
    name: &str,
    descriptor: &str,
) -> Result<Value, NativeError> {
    let declaring = block_behaviour_body(name, descriptor).and_then(|(bit, body)| {
        let class = vanilla_block_use_methods::declaring_class(vanilla_class, bit)?;
        Some((class, body))
    });
    match declaring {
        Some((BLOCK_BEHAVIOUR, body)) => Ok(body),
        _ => host.unknown_operand_call(&MethodRef {
            owner: declaring
                .map_or(vanilla_class, |(class, _)| class)
                .to_owned(),
            name: name.to_owned(),
            descriptor: descriptor.to_owned(),
        }),
    }
}

// ---------------------------------------------------------------------------
// StdHost hooks: get_static, random_new, static_call, instance_call
// ---------------------------------------------------------------------------

pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    if r.owner == INTERACTION_RESULT
        && r.descriptor
            .starts_with("Lnet/minecraft/world/InteractionResult")
    {
        return match r.name.as_str() {
            "SUCCESS" => Some(ir_success()),
            "SUCCESS_SERVER" => Some(ir_success_server()),
            "CONSUME" => Some(ir_consume()),
            "FAIL" => Some(ir_fail()),
            "PASS" => Some(ir_pass()),
            "TRY_WITH_EMPTY_HAND" => Some(ir_try_with_empty_hand()),
            _ => None,
        };
    }
    if r.owner == INTERACTION_RESULT_ITEM_CONTEXT
        && r.descriptor == "Lnet/minecraft/world/InteractionResult$ItemContext;"
    {
        return match r.name.as_str() {
            "NONE" => Some(item_context_none()),
            "DEFAULT" => Some(item_context_default()),
            _ => None,
        };
    }
    if r.owner == menu_models::MENU_TYPE
        && r.descriptor == "Lnet/minecraft/world/inventory/MenuType;"
    {
        return VANILLA_MENU_TYPES
            .iter()
            .find(|(field, _)| *field == r.name)
            .map(|(_, id)| game_object::server_value(format!("game/menu-type/{id}")));
    }
    None
}

pub(crate) fn random_new(host: &mut StdHost, class: &str) -> Option<Value> {
    match class {
        INTERACTION_RESULT_SUCCESS => Some(
            host.block_use
                .alloc_success(Value::Ref(None), Value::Ref(None)),
        ),
        INTERACTION_RESULT_FAIL => Some(host.block_use.alloc_empty_result("Fail")),
        INTERACTION_RESULT_PASS => Some(host.block_use.alloc_empty_result("Pass")),
        INTERACTION_RESULT_TRY_EMPTY => {
            Some(host.block_use.alloc_empty_result("TryEmptyHandInteraction"))
        }
        INTERACTION_RESULT_ITEM_CONTEXT => {
            Some(host.block_use.alloc_item_context(false, Value::Ref(None)))
        }
        BLOCK_HIT_RESULT => Some(host.block_use.alloc_block_hit(BlockHitData {
            location: [0.0, 0.0, 0.0],
            direction: "UP".to_owned(),
            pos: [0, 0, 0],
            inside: false,
            miss: false,
            world_border_hit: false,
        })),
        menu_models::SIMPLE_MENU_PROVIDER => Some(menu_models::alloc_simple_menu_provider(
            &mut host.next_menu_object,
        )),
        _ => None,
    }
}

fn parse_vec3(value: &Value) -> Option<[f64; 3]> {
    let key = game_object::handle_key(value)?.strip_prefix(crate::client_values::VEC3)?;
    let mut parts = key.split(',');
    let x = parts.next()?.parse::<f64>().ok()?;
    let y = parts.next()?.parse::<f64>().ok()?;
    let z = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some([x, y, z])
}

fn vec3_value([x, y, z]: [f64; 3]) -> Value {
    game_object::client_value(format!(
        "{}{},{},{}",
        crate::client_values::VEC3,
        crate::client_values::coord_text(x),
        crate::client_values::coord_text(y),
        crate::client_values::coord_text(z),
    ))
}

fn parse_direction(value: &Value) -> Option<String> {
    expect_ref(value)?
        .strip_prefix(DIRECTION_ENUM_PREFIX)
        .map(str::to_owned)
}

fn parse_block_pos(value: &Value) -> Option<[i32; 3]> {
    let key = game_object::handle_key(value)?;
    server_values::block_pos(key)
}

fn block_pos_value([x, y, z]: [i32; 3]) -> Value {
    game_object::server_value(format!("block-pos/{x},{y},{z}"))
}

fn uuid_text(uuid: u128) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (uuid >> 96) as u32,
        (uuid >> 80) as u16,
        (uuid >> 64) as u16,
        (uuid >> 48) as u16,
        uuid & 0xffff_ffff_ffff
    )
}

pub(crate) fn static_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == BLOCK_HIT_RESULT
        && r.name == "miss"
        && r.descriptor
            == "(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/core/Direction;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/phys/BlockHitResult;"
    {
        let [loc, dir, pos] = args else {
            return None;
        };
        let location = parse_vec3(loc)?;
        let direction = parse_direction(dir)?;
        let pos = parse_block_pos(pos)?;
        return Some(Ok(host.block_use.alloc_block_hit(BlockHitData {
            location,
            direction,
            pos,
            inside: false,
            miss: true,
            world_border_hit: false,
        })));
    }
    None
}

pub(crate) fn instance_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    // 1. InteractionResult & its sealed records
    if let Value::Host(handle) = receiver
        && handle.kind() == HostKind::InteractionResult
    {
        return interaction_result_call(host, r, handle, args);
    }

    // 2. InteractionResult$ItemContext
    if let Value::Host(handle) = receiver
        && handle.kind() == HostKind::InteractionItemContext
    {
        return item_context_call(host, r, handle, args);
    }

    // 3. BlockHitResult / HitResult
    if !host.block_use.block_hits.is_empty()
        && let Some(key) = game_object::handle_key(receiver)
        && host.block_use.block_hits.contains_key(key)
    {
        return block_hit_call(host, r, key.to_owned(), args);
    }

    // An inherited `useItemOn`/`useWithoutItem`/`getMenuProvider` on a jar block is answered
    // in `invoke_runtime_helper`, which knows the receiver's superclass chain.

    // 4. Active block-use Level & Player objects
    if let Some(active) = host.block_use.active_use.as_ref() {
        let is_level = receiver == &active.level;
        let is_player = !is_level && receiver == &active.player;
        if is_level || is_player {
            let active = active.clone();
            return Some(if is_level {
                active_level_call(host, &active, r, args)
            } else {
                active_player_call(host, &active, r, args)
            });
        }
    }

    None
}

fn interaction_result_call(
    host: &mut StdHost,
    r: &MethodRef,
    handle: &HostRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let key = handle.key();
    let exact = interaction_result_class(key);
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "<init>",
            "(Lnet/minecraft/world/InteractionResult$SwingSource;Lnet/minecraft/world/InteractionResult$ItemContext;)V",
        ) if exact == INTERACTION_RESULT_SUCCESS && key.starts_with("new/Success#") => {
            let [swing_source, item_context] = args else {
                return None;
            };
            host.block_use.interaction_results.insert(
                key.to_owned(),
                SuccessData {
                    swing_source: swing_source.clone(),
                    item_context: item_context.clone(),
                },
            );
            Some(Ok(Value::Void))
        }
        ("<init>", "()V")
            if matches!(
                exact,
                INTERACTION_RESULT_FAIL | INTERACTION_RESULT_PASS | INTERACTION_RESULT_TRY_EMPTY
            ) && key.starts_with("new/") =>
        {
            Some(Ok(Value::Void))
        }
        ("consumesAction", "()Z") => Some(Ok(Value::Int(i32::from(
            exact == INTERACTION_RESULT_SUCCESS,
        )))),
        ("shouldSwing", "()Z") if exact == INTERACTION_RESULT_SUCCESS => {
            let (swing, _) = success_parts_in_host(host, key)?;
            let is_none = swing == SwingSource::None.to_value();
            Some(Ok(Value::Int(i32::from(!is_none))))
        }
        ("swingSource", "()Lnet/minecraft/world/InteractionResult$SwingSource;")
            if exact == INTERACTION_RESULT_SUCCESS =>
        {
            let (swing, _) = success_parts_in_host(host, key)?;
            Some(Ok(swing))
        }
        ("itemContext", "()Lnet/minecraft/world/InteractionResult$ItemContext;")
            if exact == INTERACTION_RESULT_SUCCESS =>
        {
            let (_, ctx) = success_parts_in_host(host, key)?;
            Some(Ok(ctx))
        }
        ("wasItemInteraction", "()Z") if exact == INTERACTION_RESULT_SUCCESS => {
            let (_, ctx) = success_parts_in_host(host, key)?;
            if ctx == Value::Ref(None) {
                return Some(Err(null_checks::invoke_on_null(&MethodRef {
                    owner: INTERACTION_RESULT_ITEM_CONTEXT.into(),
                    name: "wasItemInteraction".into(),
                    descriptor: "()Z".into(),
                })));
            }
            let Value::Host(ctx_handle) = ctx else {
                return None;
            };
            let (was_item, _) = item_context_parts_in_host(host, ctx_handle.key())?;
            Some(Ok(Value::Int(i32::from(was_item))))
        }
        ("heldItemTransformedTo", "()Lnet/minecraft/world/item/ItemStack;")
            if exact == INTERACTION_RESULT_SUCCESS =>
        {
            let (_, ctx) = success_parts_in_host(host, key)?;
            if ctx == Value::Ref(None) {
                return Some(Err(null_checks::invoke_on_null(&MethodRef {
                    owner: INTERACTION_RESULT_ITEM_CONTEXT.into(),
                    name: "heldItemTransformedTo".into(),
                    descriptor: "()Lnet/minecraft/world/item/ItemStack;".into(),
                })));
            }
            let Value::Host(ctx_handle) = ctx else {
                return None;
            };
            let (_, stack) = item_context_parts_in_host(host, ctx_handle.key())?;
            Some(Ok(stack))
        }
        (
            "heldItemTransformedTo",
            "(Lnet/minecraft/world/item/ItemStack;)Lnet/minecraft/world/InteractionResult$Success;",
        ) if exact == INTERACTION_RESULT_SUCCESS => {
            let [stack] = args else {
                return None;
            };
            let (swing, _) = success_parts_in_host(host, key)?;
            let new_ctx = host.block_use.alloc_item_context(true, stack.clone());
            Some(Ok(host.block_use.alloc_success(swing, new_ctx)))
        }
        ("withoutItem", "()Lnet/minecraft/world/InteractionResult$Success;")
            if exact == INTERACTION_RESULT_SUCCESS =>
        {
            let (swing, _) = success_parts_in_host(host, key)?;
            Some(Ok(host.block_use.alloc_success(swing, item_context_none())))
        }
        _ => None,
    }
}

fn item_context_call(
    host: &mut StdHost,
    r: &MethodRef,
    handle: &HostRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let key = handle.key();
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("<init>", "(ZLnet/minecraft/world/item/ItemStack;)V")
            if key.starts_with("new/ItemContext#") =>
        {
            let [Value::Int(was_item), stack] = args else {
                return None;
            };
            host.block_use.interaction_item_contexts.insert(
                key.to_owned(),
                ItemContextData {
                    was_item_interaction: *was_item != 0,
                    held_item_transformed_to: stack.clone(),
                },
            );
            Some(Ok(Value::Void))
        }
        ("wasItemInteraction", "()Z") => {
            let (was_item, _) = item_context_parts_in_host(host, key)?;
            Some(Ok(Value::Int(i32::from(was_item))))
        }
        ("heldItemTransformedTo", "()Lnet/minecraft/world/item/ItemStack;") => {
            let (_, stack) = item_context_parts_in_host(host, key)?;
            Some(Ok(stack))
        }
        _ => None,
    }
}

fn block_hit_call(
    host: &mut StdHost,
    r: &MethodRef,
    key: String,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "<init>",
            "(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/core/Direction;Lnet/minecraft/core/BlockPos;Z)V",
        ) => {
            let [loc, dir, pos, Value::Int(inside)] = args else {
                return None;
            };
            let data = BlockHitData {
                location: parse_vec3(loc)?,
                direction: parse_direction(dir)?,
                pos: parse_block_pos(pos)?,
                inside: *inside != 0,
                miss: false,
                world_border_hit: false,
            };
            host.block_use.block_hits.insert(key, data);
            Some(Ok(Value::Void))
        }
        (
            "<init>",
            "(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/core/Direction;Lnet/minecraft/core/BlockPos;ZZ)V",
        ) => {
            let [
                loc,
                dir,
                pos,
                Value::Int(inside),
                Value::Int(world_border_hit),
            ] = args
            else {
                return None;
            };
            let data = BlockHitData {
                location: parse_vec3(loc)?,
                direction: parse_direction(dir)?,
                pos: parse_block_pos(pos)?,
                inside: *inside != 0,
                miss: false,
                world_border_hit: *world_border_hit != 0,
            };
            host.block_use.block_hits.insert(key, data);
            Some(Ok(Value::Void))
        }
        ("getBlockPos", "()Lnet/minecraft/core/BlockPos;") => {
            let data = host.block_use.block_hits.get(&key)?;
            Some(Ok(block_pos_value(data.pos)))
        }
        ("getDirection", "()Lnet/minecraft/core/Direction;") => {
            let data = host.block_use.block_hits.get(&key)?;
            Some(Ok(Value::Ref(Some(format!(
                "{DIRECTION_ENUM_PREFIX}{}",
                data.direction
            )))))
        }
        ("getLocation", "()Lnet/minecraft/world/phys/Vec3;") => {
            let data = host.block_use.block_hits.get(&key)?;
            Some(Ok(vec3_value(data.location)))
        }
        ("getType", "()Lnet/minecraft/world/phys/HitResult$Type;") => {
            let data = host.block_use.block_hits.get(&key)?;
            let kind = if data.miss { "MISS" } else { "BLOCK" };
            Some(Ok(Value::Ref(Some(format!(
                "{HIT_RESULT_TYPE_PREFIX}{kind}"
            )))))
        }
        ("isInside", "()Z") => {
            let data = host.block_use.block_hits.get(&key)?;
            Some(Ok(Value::Int(i32::from(data.inside))))
        }
        ("isWorldBorderHit", "()Z") => {
            let data = host.block_use.block_hits.get(&key)?;
            Some(Ok(Value::Int(i32::from(data.world_border_hit))))
        }
        (
            "withDirection",
            "(Lnet/minecraft/core/Direction;)Lnet/minecraft/world/phys/BlockHitResult;",
        ) => {
            let dir = parse_direction(args.first()?)?;
            let mut next = host.block_use.block_hits.get(&key)?.clone();
            next.direction = dir;
            Some(Ok(host.block_use.alloc_block_hit(next)))
        }
        (
            "withPosition",
            "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/phys/BlockHitResult;",
        ) => {
            let pos = parse_block_pos(args.first()?)?;
            let mut next = host.block_use.block_hits.get(&key)?.clone();
            next.pos = pos;
            Some(Ok(host.block_use.alloc_block_hit(next)))
        }
        ("hitBorder", "()Lnet/minecraft/world/phys/BlockHitResult;") => {
            let mut next = host.block_use.block_hits.get(&key)?.clone();
            next.world_border_hit = true;
            Some(Ok(host.block_use.alloc_block_hit(next)))
        }
        _ => None,
    }
}

fn active_level_call(
    host: &mut StdHost,
    active: &ActiveBlockUse,
    r: &MethodRef,
    args: &[Value],
) -> Result<Value, NativeError> {
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("isClientSide", "()Z") => Ok(Value::Int(i32::from(active.side == BlockUseSide::Client))),
        ("dimension", "()Lnet/minecraft/resources/ResourceKey;") => Ok(
            host_ids::resource_key_named("minecraft:dimension", &active.dimension),
        ),
        (
            "getBlockState",
            "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;",
        ) => {
            if let Some(pos) = args.first().and_then(parse_block_pos)
                && let Some((state, clicked)) = &active.block
                && pos == *clicked
            {
                return Ok(state.clone());
            }
            host.unknown_host_call(r)
        }
        ("getServer", "()Lnet/minecraft/server/MinecraftServer;") => {
            if active.side == BlockUseSide::Client {
                Ok(Value::Ref(None))
            } else {
                Ok(game_object::server_value("game/server".into()))
            }
        }
        // `ServerLevel.recipeAccess()`: the server's one `RecipeManager` (`recipe_models.rs`).
        (
            "recipeAccess",
            "()Lnet/minecraft/world/item/crafting/RecipeManager;"
            | "()Lnet/minecraft/world/item/crafting/RecipeAccess;",
        ) if active.side == BlockUseSide::Server
            && matches!(
                r.owner.as_str(),
                "net/minecraft/server/level/ServerLevel" | "net/minecraft/world/level/Level"
            ) =>
        {
            Ok(game_object::server_value(
                crate::recipe_models::RECIPE_MANAGER_KEY.into(),
            ))
        }
        _ => host.unknown_host_call(r),
    }
}

fn active_player_call(
    host: &mut StdHost,
    active: &ActiveBlockUse,
    r: &MethodRef,
    args: &[Value],
) -> Result<Value, NativeError> {
    let p = &active.player_data;
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("level", "()Lnet/minecraft/world/level/Level;") => Ok(active.level.clone()),
        ("serverLevel", "()Lnet/minecraft/server/level/ServerLevel;")
            if active.side == BlockUseSide::Server =>
        {
            Ok(active.level.clone())
        }
        ("getInventory", "()Lnet/minecraft/world/entity/player/Inventory;") => {
            Ok(active.player_inventory.clone())
        }
        ("getUUID", "()Ljava/util/UUID;") => uuid_value(&uuid_text(p.uuid)),
        ("getStringUUID", "()Ljava/lang/String;") => Ok(Value::Ref(Some(uuid_text(p.uuid)))),
        ("getName" | "getDisplayName", "()Lnet/minecraft/network/chat/Component;") => {
            Ok(component_value("literal", &p.name))
        }
        ("getScoreboardName" | "getPlainTextName", "()Ljava/lang/String;") => {
            Ok(Value::Ref(Some(p.name.clone())))
        }
        ("getId", "()I") => match p.entity_id {
            Some(id) => Ok(Value::Int(id)),
            None => host.unknown_host_call(r),
        },
        ("isShiftKeyDown" | "isCrouching" | "isSecondaryUseActive", "()Z") => {
            Ok(Value::Int(i32::from(p.sneaking)))
        }
        ("isSpectator", "()Z") => Ok(Value::Int(i32::from(p.spectator))),
        // `Entity.getX/getY/getZ()` (final: `position.x` / `.y` / `.z`), from the embedder's
        // facts for a live player; unknown without them.
        ("getX" | "getY" | "getZ", "()D") => {
            let axis = match r.name.as_str() {
                "getX" => 0,
                "getY" => 1,
                _ => 2,
            };
            match active.facts.as_ref().and_then(|facts| facts.position) {
                Some(position) => Ok(Value::Float(position[axis])),
                None => host.unknown_host_call(r),
            }
        }
        ("getMainHandItem", "()Lnet/minecraft/world/item/ItemStack;") => match &p.main_hand {
            Some(stack) => Ok(stack.clone()),
            None => host.unknown_host_call(r),
        },
        ("getOffhandItem", "()Lnet/minecraft/world/item/ItemStack;") => match &p.off_hand {
            Some(stack) => Ok(stack.clone()),
            None => host.unknown_host_call(r),
        },
        (
            "getItemInHand",
            "(Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/item/ItemStack;",
        ) => {
            let hand = args
                .first()
                .and_then(expect_ref)
                .and_then(|s| s.strip_prefix(HAND_ENUM_PREFIX));
            let stack = match hand {
                Some("MAIN_HAND") => p.main_hand.as_ref(),
                Some("OFF_HAND") => p.off_hand.as_ref(),
                _ => None,
            };
            match stack {
                Some(stack) => Ok(stack.clone()),
                None => host.unknown_host_call(r),
            }
        }
        _ => host.unknown_host_call(r),
    }
}

// ---------------------------------------------------------------------------
// Out-of-line Runtime helpers (`#[inline(never)]` to preserve invoke_frame stack budget)
// ---------------------------------------------------------------------------

/// Handle `BlockStateBase.{getMenuProvider, useWithoutItem, useItemOn}` and
/// `Player.openMenu(MenuProvider)` from `Runtime::invoke_game_helpers`.
#[inline(never)]
pub(crate) fn invoke_runtime_helper<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(receiver) = receiver else {
        return Ok(None);
    };

    if matches!(r.owner.as_str(), BLOCK_STATE | BLOCK_STATE_BASE) {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("getMenuProvider", BLOCK_STATE_GET_MENU_PROVIDER_DESC) => {
                return invoke_block_state_get_menu_provider(runtime, receiver, args);
            }
            ("useWithoutItem", BLOCK_STATE_USE_WITHOUT_ITEM_DESC) => {
                return invoke_block_state_use_without_item(runtime, receiver, args);
            }
            ("useItemOn", BLOCK_STATE_USE_ITEM_ON_DESC) => {
                return invoke_block_state_use_item_on(runtime, receiver, args);
            }
            _ => {}
        }
    }

    if r.name == "openMenu"
        && r.descriptor == PLAYER_OPEN_MENU_DESC
        && matches!(
            r.owner.as_str(),
            "net/minecraft/world/entity/player/Player"
                | "net/minecraft/server/level/ServerPlayer"
                | "net/minecraft/client/player/LocalPlayer"
                | "net/minecraft/client/player/AbstractClientPlayer"
                | "net/minecraft/client/player/RemotePlayer"
        )
    {
        return invoke_player_open_menu(runtime, receiver, args);
    }

    // A block-use method on a jar block that reaches the model chain with no jar class
    // declaring it on the symbolic owner's chain (`member_owner` stops at the first class the
    // runtime doesn't hold): `super.useWithoutItem(..)` (`invokespecial` of a superclass), or
    // an `invokevirtual` the receiver's jar classes don't override (`jvm_selected_jar_body`
    // ran first). The body is then the one the receiver's first vanilla class inherits, which
    // the symbolic owner's first vanilla class is or extends. A jar declaration keeps the
    // interpreter's own resolution.
    if block_behaviour_body(&r.name, &r.descriptor).is_some()
        && jar_object::object_class(receiver).is_some()
        && let Some((resolved, false)) = runtime.member_owner(&r.owner, &r.name, &r.descriptor)
        && let Some(vanilla_class) = block_vanilla_superclass(runtime, receiver)
        && vanilla_block_use_methods::extends(&vanilla_class, &resolved)
    {
        return run_inherited_block_method(runtime.host, &vanilla_class, &r.name, &r.descriptor)
            .map(Some);
    }

    Ok(None)
}

/// The first class of `block`'s superclass chain that the runtime doesn't hold: the vanilla
/// (or library) class it leaves the jar at. `None` for a value that is no jar object, or a
/// chain that ends inside the jar.
#[inline(never)]
fn block_vanilla_superclass<H: Host>(runtime: &Runtime<'_, H>, block: &Value) -> Option<String> {
    let mut current = jar_object::object_class(block)?.to_owned();
    for _ in 0..64 {
        let Some(file) = runtime.class(&current) else {
            return Some(current);
        };
        if file.super_name.is_empty() {
            return None;
        }
        current = file.super_name.clone();
    }
    None
}

/// `block.<name>(args)` as `invokevirtual BlockBehaviour.<name>` selects it: the jar's
/// override when it has one, else the vanilla body its chain inherits
/// ([`run_inherited_block_method`]); a block whose class Decaf can't place is counted.
#[inline(never)]
fn dispatch_block_virtual<H: Host>(
    runtime: &mut Runtime<'_, H>,
    block: &Value,
    name: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Value, NativeError> {
    let virtual_ref = MethodRef {
        owner: BLOCK_BEHAVIOUR.to_owned(),
        name: name.to_owned(),
        descriptor: descriptor.to_owned(),
    };
    if let Some(jar_owner) = runtime.jvm_selected_jar_body(&virtual_ref, 0xb6, Some(block))? {
        return runtime.invoke(&jar_owner, name, descriptor, Some(block.clone()), args);
    }
    match block_vanilla_superclass(runtime, block) {
        Some(vanilla_class) => {
            run_inherited_block_method(runtime.host, &vanilla_class, name, descriptor)
        }
        None => runtime.host.unknown_operand_call(&virtual_ref),
    }
}

#[inline(never)]
fn invoke_block_state_get_menu_provider<H: Host>(
    runtime: &mut Runtime<'_, H>,
    state: &Value,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(block) = runtime.host.block_state_block(state) else {
        return Ok(None);
    };
    let [level, pos] = args else {
        return Ok(None);
    };
    dispatch_block_virtual(
        runtime,
        &block,
        "getMenuProvider",
        GET_MENU_PROVIDER_DESC,
        &[state.clone(), level.clone(), pos.clone()],
    )
    .map(Some)
}

#[inline(never)]
fn invoke_block_state_use_without_item<H: Host>(
    runtime: &mut Runtime<'_, H>,
    state: &Value,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(block) = runtime.host.block_state_block(state) else {
        return Ok(None);
    };
    let [level, player, hit] = args else {
        return Ok(None);
    };
    let pos = runtime.call_virtual(
        jdk_call::Site::Virtual,
        BLOCK_HIT_RESULT,
        "getBlockPos",
        "()Lnet/minecraft/core/BlockPos;",
        hit,
        &[],
    )?;
    dispatch_block_virtual(
        runtime,
        &block,
        "useWithoutItem",
        USE_WITHOUT_ITEM_DESC,
        &[
            state.clone(),
            level.clone(),
            pos,
            player.clone(),
            hit.clone(),
        ],
    )
    .map(Some)
}

#[inline(never)]
fn invoke_block_state_use_item_on<H: Host>(
    runtime: &mut Runtime<'_, H>,
    state: &Value,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(block) = runtime.host.block_state_block(state) else {
        return Ok(None);
    };
    let [stack, level, player, hand, hit] = args else {
        return Ok(None);
    };
    let pos = runtime.call_virtual(
        jdk_call::Site::Virtual,
        BLOCK_HIT_RESULT,
        "getBlockPos",
        "()Lnet/minecraft/core/BlockPos;",
        hit,
        &[],
    )?;
    dispatch_block_virtual(
        runtime,
        &block,
        "useItemOn",
        USE_ITEM_ON_DESC,
        &[
            stack.clone(),
            state.clone(),
            level.clone(),
            pos,
            player.clone(),
            hand.clone(),
            hit.clone(),
        ],
    )
    .map(Some)
}

const SERVER_PLAYER: &str = "net/minecraft/server/level/ServerPlayer";
const PLAYER_DESC_ARG: &str = "(Lnet/minecraft/world/entity/player/Player;)V";

fn server_player_method(name: &str, descriptor: &str) -> MethodRef {
    MethodRef {
        owner: SERVER_PLAYER.to_owned(),
        name: name.to_owned(),
        descriptor: descriptor.to_owned(),
    }
}

/// Exact MC 26.3 `Player.openMenu(MenuProvider)` vs `ServerPlayer.openMenu(MenuProvider)`.
///
/// `Player.openMenu` (the client's `LocalPlayer` inherits it) returns `OptionalInt.empty()` and
/// records nothing. `ServerPlayer.openMenu` runs, by its `server-26.3.jar` offsets:
/// - 0-7: `provider == null` -> `OptionalInt.empty()`;
/// - 8-20: `if (containerMenu != inventoryMenu) closeContainer()` ([`open_menu_close_current`]),
///   so the old menu's `removed` runs before the new menu exists (#629 review P1 4);
/// - 23-24: `nextContainerCounter()` ([`open_menu_next_container_counter`]);
/// - 27-42: `menu = provider.createMenu(containerCounter, getInventory(), this)`;
/// - 43-73: `menu == null` -> a spectator is sent `container.spectatorCantOpen` (counted), and
///   `OptionalInt.empty()`;
/// - 74-99: `ClientboundOpenScreenPacket(menu.containerId, menu.getType(),
///   provider.getDisplayName())`, recorded as [`MenuEffect::Opened`];
/// - 102-104: `initMenu(menu)` (counted, see below);
/// - 107-109: `containerMenu = menu`: Decaf holds the menu for the player ([`PlayerMenus::open`]);
/// - 112-119: `OptionalInt.of(containerCounter)`.
#[inline(never)]
fn invoke_player_open_menu<H: Host>(
    runtime: &mut Runtime<'_, H>,
    receiver: &Value,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(provider) = args.first() else {
        return Ok(None);
    };
    // Client side (`LocalPlayer` inherits `Player.openMenu` -> `OptionalInt.empty()`).
    if game_object::handle_key(receiver) == Some("client/player") {
        return Ok(Some(host_optional::empty_of(HostKind::OptionalInt)));
    }
    let Some(std_host) = runtime.host.as_std_host_mut() else {
        return Ok(None);
    };
    let Some((inventory, spectator)) = std_host.block_use.active_use.as_ref().and_then(|active| {
        (active.side == BlockUseSide::Server && receiver == &active.player).then(|| {
            (
                active.player_inventory.clone(),
                active.player_data.spectator,
            )
        })
    }) else {
        return Ok(None);
    };
    if *provider == Value::Ref(None) {
        return Ok(Some(host_optional::empty_of(HostKind::OptionalInt)));
    }

    open_menu_close_current(runtime, receiver)?;
    let container_id = open_menu_next_container_counter(runtime)?;

    let menu = runtime.call_virtual(
        jdk_call::Site::Interface,
        menu_models::MENU_PROVIDER,
        "createMenu",
        "(ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/entity/player/Player;)Lnet/minecraft/world/inventory/AbstractContainerMenu;",
        provider,
        &[Value::Int(container_id), inventory, receiver.clone()],
    )?;
    if menu == Value::Ref(None) {
        // The counter moved all the same: the embedder's must too, or the next window's id
        // drifts from the one Java hands the next `createMenu`.
        if let Some(active) = runtime
            .host
            .as_std_host_mut()
            .and_then(|host| host.block_use.active_use.as_mut())
        {
            active
                .menu_effects
                .push(MenuEffect::CounterAdvanced { container_id });
        }
        if spectator {
            // Offsets 54-67: `sendOverlayMessage(Component.translatable(
            // "container.spectatorCantOpen").withStyle(RED))`, a message Decaf can't send.
            runtime.host.unknown_operand_call(&server_player_method(
                "sendOverlayMessage",
                "(Lnet/minecraft/network/chat/Component;)V",
            ))?;
        }
        return Ok(Some(host_optional::empty_of(HostKind::OptionalInt)));
    }

    let menu_type = runtime.call_virtual(
        jdk_call::Site::Virtual,
        menu_models::ABSTRACT_CONTAINER_MENU,
        "getType",
        "()Lnet/minecraft/world/inventory/MenuType;",
        &menu,
        &[],
    )?;
    let title = runtime.call_virtual(
        jdk_call::Site::Interface,
        menu_models::MENU_PROVIDER,
        "getDisplayName",
        "()Lnet/minecraft/network/chat/Component;",
        provider,
        &[],
    )?;

    // Offsets 102-104: `initMenu(menu)` adds the player's slot listener (`addSlotListener`,
    // which runs `broadcastChanges`) and synchronizer (`setSynchronizer`, which runs
    // `sendAllDataToRemote`). Both read the menu's slots, containers and data slots through
    // methods a mod's menu can override, and send what they read to the client. Nothing runs
    // them for a Java menu yet (the live-menu bridge will), so the call is counted.
    // When the embedder described the player's inventory (`MenuPlayerFacts`), Decaf models the
    // player's listener and synchronizer (`live_menu.rs`) and runs both exactly; otherwise the
    // reads of the player's slots would be guesses, so `initMenu` stays counted.
    if !crate::live_menu::run_init_menu(runtime, &menu)? {
        runtime.host.unknown_operand_call(&server_player_method(
            "initMenu",
            "(Lnet/minecraft/world/inventory/AbstractContainerMenu;)V",
        ))?;
    }

    // Resolve access_pos: first from ItemCombinerMenu.access (exact, honoring NULL),
    // otherwise from any ContainerLevelAccess instance field stored on a custom menu subclass.
    let mut access_pos = None;
    let mut has_combiner_access = false;
    if let Some(std_host) = runtime.host.as_std_host_mut()
        && let Some(access) = menu_models::menu_combiner_access(&std_host.menu_states, &menu)
    {
        has_combiner_access = true;
        access_pos = menu_models::container_level_access_pos(&std_host.menu_states, &access);
    }
    if !has_combiner_access
        && let Some(menu_key) = jar_object::object_key(&menu)
        && let Some(std_host) = runtime.host.as_std_host_mut()
    {
        let prefix = format!("{menu_key}|");
        for (field_key, field_val) in &runtime.fields {
            if field_key.starts_with(&prefix)
                && let Some(pos) =
                    menu_models::container_level_access_pos(&std_host.menu_states, field_val)
            {
                access_pos = Some(pos);
                break;
            }
        }
    }

    if let Some(std_host) = runtime.host.as_std_host_mut() {
        let menu_type_id = resolve_menu_type_id(std_host, &menu_type);
        let title_json = facade_text::json_now(&title, Some(&std_host.component_states));
        let constructor =
            menu_models::simple_menu_provider_constructor(&std_host.menu_states, provider);
        let effect = OpenMenuEffect {
            container_id,
            menu_type,
            menu_type_id,
            title,
            title_json,
            access_pos,
            constructor,
            provider: provider.clone(),
            menu,
        };
        // Offsets 107-109: `containerMenu = menu`.
        std_host.hold_opened_menu(&effect);
        if let Some(active) = std_host.block_use.active_use.as_mut() {
            active.container = PlayerContainer::Open(container_id);
            active.menu_effects.push(MenuEffect::Opened(effect));
        }
    }

    Ok(Some(host_optional::optional_int(container_id)))
}

/// The player's `containerMenu` at this point of the running call, as Decaf models it.
#[derive(Clone, Debug, PartialEq)]
enum InstalledMenu {
    /// `inventoryMenu`: nothing open.
    Inventory,
    /// The Java menu Decaf holds: the window the embedder shows, or one whose close packet went
    /// out while its `doCloseContainer` never finished ([`PlayerMenus::close_sent`]).
    Held { container_id: i32, menu: Value },
    /// A Java menu Decaf held whose window the embedder closed without Decaf running its close.
    Stale,
    /// A container Decaf doesn't hold (the embedder's own, another jar's), or one the embedder
    /// can't name.
    Foreign,
}

fn installed_menu(std_host: &StdHost) -> InstalledMenu {
    let Some(active) = std_host.block_use.active_use.as_ref() else {
        return InstalledMenu::Foreign;
    };
    let entry = std_host
        .block_use
        .player(active.player_data.uuid, active.player_data.incarnation);
    let held = entry.and_then(|entry| {
        let open = entry.open.as_ref()?;
        Some((open.container_id, open.menu.clone(), entry.close_sent))
    });
    match (held, active.container) {
        (Some((container_id, menu, _)), PlayerContainer::Open(open)) if open == container_id => {
            InstalledMenu::Held { container_id, menu }
        }
        (Some((container_id, menu, true)), PlayerContainer::Inventory) => {
            InstalledMenu::Held { container_id, menu }
        }
        (Some(_), PlayerContainer::Inventory) => InstalledMenu::Stale,
        (None, PlayerContainer::Inventory) => InstalledMenu::Inventory,
        (_, PlayerContainer::Open(_) | PlayerContainer::Unknown) => InstalledMenu::Foreign,
    }
}

fn installed_menu_of<H: Host>(runtime: &mut Runtime<'_, H>) -> InstalledMenu {
    runtime
        .host
        .as_std_host_mut()
        .map_or(InstalledMenu::Foreign, |std_host| installed_menu(std_host))
}

/// `ServerPlayer.openMenu` offsets 8-20: `if (containerMenu != inventoryMenu) closeContainer()`.
///
/// Decaf knows the player's `containerMenu` from the embedder's snapshot
/// ([`BlockUsePlayerData::container`], updated by this call's own closes and opens) and the
/// Java menu it holds for them ([`installed_menu`]):
/// - the inventory menu: nothing to close;
/// - the held menu: `closeContainer()` runs here, before the new menu is built
///   ([`run_close_container`]);
/// - a held menu whose window the embedder closed without Decaf running its
///   `doCloseContainer`: that missed close is counted;
/// - any other menu (one the embedder or another jar owns), or a menu the embedder can't name:
///   Decaf can't run its close, so `closeContainer` is counted.
#[inline(never)]
fn open_menu_close_current<H: Host>(
    runtime: &mut Runtime<'_, H>,
    player: &Value,
) -> Result<(), NativeError> {
    let counted = match installed_menu_of(runtime) {
        InstalledMenu::Inventory => return Ok(()),
        InstalledMenu::Held { .. } => return run_close_container(runtime, player),
        InstalledMenu::Stale => server_player_method("doCloseContainer", "()V"),
        InstalledMenu::Foreign => server_player_method("closeContainer", "()V"),
    };
    // What Decaf held is no longer the player's `containerMenu` either way.
    if let Some(std_host) = runtime.host.as_std_host_mut() {
        std_host.drop_installed_menu();
        if let Some(active) = std_host.block_use.active_use.as_mut() {
            active.container = PlayerContainer::Inventory;
        }
    }
    runtime.host.unknown_operand_call(&counted).map(|_| ())
}

/// `ServerPlayer.closeContainer()` (26.3, offsets 0-25) on the held menu: offsets 0-18 send
/// the client `ClientboundContainerClosePacket(containerMenu.containerId)`
/// ([`MenuEffect::Closed`]), so its window is gone from here on, then offset 22 runs
/// `doCloseContainer` ([`run_do_close_container`]) with the menu still installed.
#[inline(never)]
pub(crate) fn run_close_container<H: Host>(
    runtime: &mut Runtime<'_, H>,
    player: &Value,
) -> Result<(), NativeError> {
    let Some(std_host) = runtime.host.as_std_host_mut() else {
        return Ok(());
    };
    let InstalledMenu::Held { container_id, .. } = installed_menu(std_host) else {
        return Ok(());
    };
    std_host.mark_close_sent();
    if let Some(active) = std_host.block_use.active_use.as_mut() {
        active.container = PlayerContainer::Inventory;
        active
            .menu_effects
            .push(MenuEffect::Closed { container_id });
    }
    run_do_close_container(runtime, player)
}

/// `ServerPlayer.doCloseContainer()` (26.3, offsets 0-27), each step on the `containerMenu`
/// installed when it runs, as the bytecode reads the field afresh each time:
/// - 0-5: `containerMenu.removed(this)`, dispatched to the menu's own override. The menu stays
///   installed while it runs, so a `removed` that opens a menu closes this one again first
///   (re-entrantly) and installs its own. If it throws, nothing below runs: the menu stays
///   installed (the client's window is already gone, [`PlayerMenus::close_sent`]) and the
///   exception propagates to the caller, as in vanilla;
/// - 8-16: `inventoryMenu.transferState(containerMenu)`, which copies that menu's remote slot
///   state by `Slot.getContainerSlot()`. That is unobservable unless a jar slot overrides
///   `getContainerSlot`, so it is counted only then (or when Decaf doesn't know the slots);
/// - 19-24: `containerMenu = inventoryMenu`. Decaf drops whatever is installed then; a menu
///   other than the one closing (a nested open's) is discarded without any close
///   ([`MenuEffect::Orphaned`]).
#[inline(never)]
pub(crate) fn run_do_close_container<H: Host>(
    runtime: &mut Runtime<'_, H>,
    player: &Value,
) -> Result<(), NativeError> {
    let InstalledMenu::Held { menu: closing, .. } = installed_menu_of(runtime) else {
        return Ok(());
    };
    runtime.call_virtual(
        jdk_call::Site::Virtual,
        menu_models::ABSTRACT_CONTAINER_MENU,
        "removed",
        PLAYER_DESC_ARG,
        &closing,
        std::slice::from_ref(player),
    )?;
    match installed_menu_of(runtime) {
        InstalledMenu::Held { menu, .. } => transfer_state(runtime, &menu)?,
        // `inventoryMenu.transferState(inventoryMenu)`: its own state, nothing a mod reads.
        InstalledMenu::Inventory => {}
        InstalledMenu::Stale | InstalledMenu::Foreign => {
            runtime
                .host
                .unknown_operand_call(&transfer_state_method())?;
        }
    }
    if let Some(std_host) = runtime.host.as_std_host_mut() {
        if let InstalledMenu::Held { container_id, menu } = installed_menu(std_host)
            && menu != closing
            && let Some(active) = std_host.block_use.active_use.as_mut()
        {
            active
                .menu_effects
                .push(MenuEffect::Orphaned { container_id });
        }
        std_host.drop_installed_menu();
        if let Some(active) = std_host.block_use.active_use.as_mut() {
            active.container = PlayerContainer::Inventory;
        }
    }
    Ok(())
}

fn transfer_state_method() -> MethodRef {
    MethodRef {
        owner: "net/minecraft/world/inventory/InventoryMenu".to_owned(),
        name: "transferState".to_owned(),
        descriptor: "(Lnet/minecraft/world/inventory/AbstractContainerMenu;)V".to_owned(),
    }
}

/// `inventoryMenu.transferState(menu)` (`AbstractContainerMenu.transferState`): reads each of
/// `menu`'s slots' `getContainerSlot()`, which only a jar slot's override makes observable.
#[inline(never)]
fn transfer_state<H: Host>(runtime: &mut Runtime<'_, H>, menu: &Value) -> Result<(), NativeError> {
    let slots = runtime
        .host
        .as_std_host_mut()
        .and_then(|std_host| menu_models::menu_broadcast_slots(&std_host.menu_states, menu));
    let get_container_slot = MethodRef {
        owner: "net/minecraft/world/inventory/Slot".to_owned(),
        name: "getContainerSlot".to_owned(),
        descriptor: "()I".to_owned(),
    };
    let mut observable = slots.is_none();
    for slot in slots.unwrap_or_default() {
        if runtime
            .jvm_selected_jar_body(&get_container_slot, 0xb6, Some(&slot.slot))?
            .is_some()
        {
            observable = true;
            break;
        }
    }
    if observable {
        runtime
            .host
            .unknown_operand_call(&transfer_state_method())?;
    }
    Ok(())
}

/// `ServerPlayer.nextContainerCounter()` (26.3, offsets 0-13:
/// `containerCounter = containerCounter % 100 + 1`) on the object Decaf keeps state for
/// ([`PlayerMenus::counter`]). When Decaf doesn't know the counter (no object it was told was
/// created, or the counter's value since lost), the call is counted and the menu is built
/// with id 1 as a placeholder.
#[inline(never)]
fn open_menu_next_container_counter<H: Host>(
    runtime: &mut Runtime<'_, H>,
) -> Result<i32, NativeError> {
    let next = runtime.host.as_std_host_mut().and_then(|std_host| {
        let active = std_host.block_use.active_use.as_ref()?;
        let (uuid, incarnation) = (active.player_data.uuid, active.player_data.incarnation);
        let counter = std_host
            .block_use
            .player_mut(uuid, incarnation)?
            .counter
            .as_mut()?;
        *counter = *counter % 100 + 1;
        Some(*counter)
    });
    match next {
        Some(id) => Ok(id),
        None => {
            runtime
                .host
                .unknown_operand_call(&server_player_method("nextContainerCounter", "()V"))?;
            Ok(1)
        }
    }
}

// ---------------------------------------------------------------------------
// Public entrypoints on StdHost and NativeJar
// ---------------------------------------------------------------------------

impl StdHost {
    /// Look up the Java `Block` object registered under `minecraft:block` for `block_id`.
    pub fn registered_block(&self, block_id: &str) -> Option<Value> {
        self.registrations()
            .iter()
            .find(|entry| entry.registry == "minecraft:block" && entry.id == block_id)
            .map(|entry| entry.value.clone())
    }

    /// A new `ServerPlayer` object for `uuid`: a login (`session`) or `PlayerList.respawn`'s
    /// copy (`life`). Its `containerCounter` starts at 0 (the field's default: `restoreFrom`
    /// does not copy it, `server-26.3.jar`) and its `containerMenu` is its `inventoryMenu`, so
    /// whatever Decaf held for the player's previous object is dropped.
    pub fn server_player_created(&mut self, uuid: u128, session: u32, life: u32) {
        self.forget_player_menus(uuid);
        self.block_use.players.insert(
            uuid,
            PlayerMenus {
                incarnation: (session, life),
                counter: Some(0),
                open: None,
                inventory: None,
                close_sent: false,
            },
        );
    }

    /// The player left the server: their object is gone, and so is what Decaf held for it.
    pub fn server_player_removed(&mut self, uuid: u128) {
        self.forget_player_menus(uuid);
    }

    /// A container opened for the player's current object other than through this jar's
    /// `ServerPlayer.openMenu` (vanilla, the embedder, another jar), or the embedder showed
    /// one Decaf opened under its own id. `container_id` is its `containerId`, which every
    /// opener takes from `containerCounter` (`openMenu` offsets 23-37, `openHorseInventory`),
    /// so that is the counter now. `None` when the embedder can't read the id: Decaf then no
    /// longer knows the counter, and counts `nextContainerCounter` until the next known open
    /// or the player's next object.
    pub fn server_player_container_opened(&mut self, uuid: u128, container_id: Option<i32>) {
        if let Some(entry) = self.block_use.players.get_mut(&uuid) {
            entry.counter = container_id;
        }
    }

    /// The Java menu Decaf holds as the player's `containerMenu`: the last one their current
    /// object opened that nothing closed since, with the `MenuConstructor` and live menu the
    /// live-menu bridge drives. At most one per player.
    pub fn player_open_menu(&self, uuid: u128) -> Option<&OpenMenuEffect> {
        self.block_use.players.get(&uuid)?.open.as_ref()
    }

    /// The embedder could not show the menu `container_id` Decaf opened for the player (a
    /// menu type it doesn't know, a refusal, a different window id): Decaf drops it, since
    /// nothing shows it, and stops trusting the player's `containerCounter`, which Java
    /// advanced and the embedder's server did not. Answers whether that menu was held.
    pub fn refuse_open_menu(&mut self, uuid: u128, container_id: i32) -> bool {
        let Some(entry) = self.block_use.players.get_mut(&uuid) else {
            return false;
        };
        entry.counter = None;
        let held = entry.open.take_if(|open| open.container_id == container_id);
        let dropped = held.is_some();
        let inventory = if dropped {
            entry.close_sent = false;
            entry.inventory.take()
        } else {
            None
        };
        if let Some(record) = held {
            self.release_menu_values(&record);
        }
        if let Some(inventory) = inventory {
            self.release(&inventory);
        }
        dropped
    }

    /// Drop every player's menu state: a new server, whose players are new objects.
    pub fn reset_player_menus(&mut self) {
        let players = std::mem::take(&mut self.block_use.players);
        for entry in players.into_values() {
            if let Some(record) = entry.open {
                self.release_menu_values(&record);
            }
            if let Some(inventory) = entry.inventory {
                self.release(&inventory);
            }
        }
    }

    fn forget_player_menus(&mut self, uuid: u128) {
        if let Some(entry) = self.block_use.players.remove(&uuid) {
            if let Some(record) = entry.open {
                self.release_menu_values(&record);
            }
            if let Some(inventory) = entry.inventory {
                self.release(&inventory);
            }
        }
    }

    /// `containerMenu = menu` (`ServerPlayer.openMenu` offsets 107-109) for the player the
    /// running call is for: Decaf holds the menu on their object's entry and retains its Java
    /// values, so a later block use can close it and the live-menu bridge can drive it. A
    /// player whose object Decaf can't name keeps nothing (a later close is then counted).
    pub(crate) fn hold_opened_menu(&mut self, effect: &OpenMenuEffect) {
        // A menu still installed here was installed by an open nested in this one (its
        // constructor's): the assignment discards it without any close.
        if let InstalledMenu::Held { container_id, .. } = installed_menu(self)
            && let Some(active) = self.block_use.active_use.as_mut()
        {
            active
                .menu_effects
                .push(MenuEffect::Orphaned { container_id });
        }
        let Some(active) = self.block_use.active_use.as_ref() else {
            return;
        };
        let uuid = active.player_data.uuid;
        let inventory = active.player_inventory.clone();
        let Some(incarnation) = active.player_data.incarnation else {
            return;
        };
        let stale = self
            .block_use
            .players
            .get(&uuid)
            .is_some_and(|entry| entry.incarnation != incarnation);
        if stale {
            // An older object's entry: that object is gone.
            self.forget_player_menus(uuid);
        }
        let entry = self.block_use.players.entry(uuid).or_insert(PlayerMenus {
            incarnation,
            // Decaf was not told this object was created, so it can't know the counter.
            counter: None,
            open: None,
            inventory: None,
            close_sent: false,
        });
        let replaced = entry.open.replace(effect.clone());
        let replaced_inventory = entry.inventory.replace(inventory.clone());
        entry.close_sent = false;
        for value in effect.values() {
            self.retain(value);
        }
        self.retain(&inventory);
        if let Some(record) = replaced {
            self.release_menu_values(&record);
        }
        if let Some(old) = replaced_inventory {
            self.release(&old);
        }
    }

    /// The running call's player's `containerMenu` stops being the menu Decaf holds
    /// (`containerMenu = inventoryMenu`): drop it and release its values.
    pub(crate) fn drop_installed_menu(&mut self) {
        let Some(active) = self.block_use.active_use.as_ref() else {
            return;
        };
        let (uuid, incarnation) = (active.player_data.uuid, active.player_data.incarnation);
        let Some(entry) = self.block_use.player_mut(uuid, incarnation) else {
            return;
        };
        entry.close_sent = false;
        let inventory = entry.inventory.take();
        if let Some(record) = entry.open.take() {
            self.release_menu_values(&record);
        }
        if let Some(inventory) = inventory {
            self.release(&inventory);
        }
    }

    /// `closeContainer` sent the client the held menu's close packet: its window is gone,
    /// while the menu stays installed until `doCloseContainer` reaches offset 19.
    pub(crate) fn mark_close_sent(&mut self) {
        let Some(active) = self.block_use.active_use.as_ref() else {
            return;
        };
        let (uuid, incarnation) = (active.player_data.uuid, active.player_data.incarnation);
        if let Some(entry) = self.block_use.player_mut(uuid, incarnation) {
            entry.close_sent = true;
        }
    }

    /// Drop the retain holds [`Self::hold_opened_menu`] took on a menu's Java values.
    pub(crate) fn release_menu_values(&mut self, effect: &OpenMenuEffect) {
        for value in effect.values() {
            self.release(value);
        }
    }

    /// Build the canonical `BlockState` value for a registered mod block and property overrides.
    pub(crate) fn block_state_for_use(
        &mut self,
        block_id: &str,
        overrides: &BTreeMap<String, String>,
    ) -> Option<(Value, Value)> {
        let block = self.registered_block(block_id)?;
        let block_key = jar_object::object_key(&block)?.to_owned();
        let mut values = BTreeMap::new();
        if let Some(props) = self.block_state_properties.get(&block_key) {
            for (name, info) in props {
                if let Some(first) = info.values.first() {
                    values.insert(name.clone(), first.clone());
                }
            }
        }
        if let Some(defaults) = self.block_default_states.get(&block_key) {
            for (name, value) in defaults {
                values.insert(name.clone(), value.clone());
            }
        }
        for (name, value) in overrides {
            values.insert(name.clone(), value.clone());
        }
        let state = self.canonical_block_state(&block_key, values);
        Some((block, state))
    }
}

#[derive(Clone, Copy)]
enum BlockUseKind {
    WithoutItem,
    ItemOn,
}

/// The `ServerPlayer` handle a player's data names, keyed as the server's game objects key a
/// player (`GameObject::marker`): by UUID, object (session and life) and entity id.
pub(crate) fn server_player_value(player: &BlockUsePlayerData) -> Value {
    let uuid = player.uuid;
    let key = match (player.incarnation, player.entity_id) {
        (Some((session, life)), Some(id)) => {
            format!("game/player/{uuid:032x}/{session}.{life}/{id}")
        }
        (Some((session, life)), None) => format!("game/player/{uuid:032x}/{session}.{life}"),
        (None, _) => format!("game/player/{uuid:032x}"),
    };
    game_object::server_value(key)
}

/// How many unmodelled host calls `host` made since `before`, by method.
pub(crate) fn unknown_calls_since(
    host: &StdHost,
    before: &BTreeMap<String, usize>,
) -> (BTreeMap<String, usize>, usize) {
    let mut calls = BTreeMap::new();
    let mut total = 0usize;
    for (name, count) in host.unknown_host_calls() {
        let prev = before.get(&name).copied().unwrap_or(0);
        if count > prev {
            let delta = count - prev;
            calls.insert(name, delta);
            total += delta;
        }
    }
    (calls, total)
}

impl NativeJar {
    /// Run a registered Java block's `Block.useWithoutItem` override with exact MC 26.3 arguments.
    pub fn block_use_without_item(
        &self,
        host: &mut StdHost,
        context: &BlockUseContext,
    ) -> Result<BlockUseOutcome, NativeError> {
        self.block_use_inner(
            host,
            context,
            BlockUseKind::WithoutItem,
            &ExecutionBudget::UNBOUNDED,
        )
    }

    /// [`NativeJar::block_use_without_item`] bounded by `budget`.
    pub fn block_use_without_item_with_budget(
        &self,
        host: &mut StdHost,
        context: &BlockUseContext,
        budget: &ExecutionBudget,
    ) -> Result<BlockUseOutcome, NativeError> {
        self.block_use_inner(host, context, BlockUseKind::WithoutItem, budget)
    }

    /// Run a registered Java block's `Block.useItemOn` override with exact MC 26.3 arguments.
    pub fn block_use_item_on(
        &self,
        host: &mut StdHost,
        context: &BlockUseContext,
    ) -> Result<BlockUseOutcome, NativeError> {
        self.block_use_inner(
            host,
            context,
            BlockUseKind::ItemOn,
            &ExecutionBudget::UNBOUNDED,
        )
    }

    /// [`NativeJar::block_use_item_on`] bounded by `budget`.
    pub fn block_use_item_on_with_budget(
        &self,
        host: &mut StdHost,
        context: &BlockUseContext,
        budget: &ExecutionBudget,
    ) -> Result<BlockUseOutcome, NativeError> {
        self.block_use_inner(host, context, BlockUseKind::ItemOn, budget)
    }

    /// The embedder closed the window of the Java menu `container_id` that Decaf holds as the
    /// player's `containerMenu` (the player closed it, walked away or left, or something else
    /// replaced it): vanilla's `ServerGamePacketListenerImpl.handleContainerClose` runs
    /// `ServerPlayer.doCloseContainer`, so the menu's `removed(player)` runs here with the menu
    /// still installed, and Decaf drops it at `containerMenu = inventoryMenu` (offset 19). What
    /// a `removed` opens meanwhile is in the outcome's effects; a `removed` that throws leaves
    /// the menu installed with no window (the outcome's `failure`), and the player's next
    /// `openMenu` closes it again.
    ///
    /// `Ok(None)` when Decaf holds no such menu for the player's current object, or when the
    /// window closed because Java's own `closeContainer` sent its close packet: vanilla's
    /// client doesn't answer that with a close of its own.
    pub fn close_player_menu(
        &self,
        host: &mut StdHost,
        player: &BlockUsePlayerData,
        dimension: &str,
        container_id: i32,
        budget: &ExecutionBudget,
    ) -> Result<Option<MenuCloseOutcome>, NativeError> {
        let Some(menu) = host
            .block_use
            .player(player.uuid, player.incarnation)
            .filter(|entry| !entry.close_sent)
            .and_then(|entry| entry.open.as_ref())
            .filter(|open| open.container_id == container_id)
            .map(|open| open.menu.clone())
        else {
            return Ok(None);
        };
        let level = game_object::server_value(format!("game/level/{dimension}"));
        let player_value = server_player_value(player);
        let player_inventory = menu_models::allocate_inventory(
            &mut host.menu_states,
            &mut host.next_menu_object,
            player_value.clone(),
        );
        let unknowns_before: BTreeMap<String, usize> =
            host.unknown_host_calls().into_iter().collect();
        let mark = host.call_log_mark();
        // The window is gone; the menu stays installed until offset 19.
        let previous_active = host.block_use.active_use.replace(ActiveBlockUse {
            side: BlockUseSide::Server,
            dimension: dimension.to_owned(),
            level: level.clone(),
            block: None,
            player: player_value.clone(),
            player_data: player.clone(),
            player_inventory: player_inventory.clone(),
            container: PlayerContainer::Inventory,
            menu_effects: Vec::new(),
            facts: None,
            live_effects: Vec::new(),
        });
        host.mark_close_sent();
        let roots = vec![level, player_value.clone(), player_inventory, menu];
        let call_result =
            self.with_persistent_runtime_with_budget(host, &roots, budget, |runtime| {
                run_do_close_container(runtime, &player_value).map(|()| Value::Void)
            });
        let finished_active = host.block_use.active_use.take();
        host.block_use.active_use = previous_active;
        host.rewind_call_log(mark);
        let (unknown_host_calls, unknown_host_call_count) =
            unknown_calls_since(host, &unknowns_before);
        let (menu_effects, live_effects) = finished_active
            .map(|active| (active.menu_effects, active.live_effects))
            .unwrap_or_default();
        Ok(Some(MenuCloseOutcome {
            container_id,
            menu_effects,
            live_effects,
            unknown_host_calls,
            unknown_host_call_count,
            failure: call_result.err().map(|error| error.to_string()),
        }))
    }

    fn block_use_inner(
        &self,
        host: &mut StdHost,
        context: &BlockUseContext,
        kind: BlockUseKind,
        budget: &ExecutionBudget,
    ) -> Result<BlockUseOutcome, NativeError> {
        let Some((block, state)) =
            host.block_state_for_use(&context.block_id, &context.state_properties)
        else {
            return Err(NativeError::Metadata(format!(
                "block `{}` is not registered in this jar",
                context.block_id
            )));
        };

        let level = match context.side {
            BlockUseSide::Server => {
                game_object::server_value(format!("game/level/{}", context.dimension))
            }
            BlockUseSide::Client => game_object::client_value("client/level".into()),
        };
        let player = match context.side {
            BlockUseSide::Server => server_player_value(&context.player),
            BlockUseSide::Client => game_object::client_value("client/player".into()),
        };
        let player_inventory = menu_models::allocate_inventory(
            &mut host.menu_states,
            &mut host.next_menu_object,
            player.clone(),
        );
        // What the embedder said of the player's inventory and abilities (server side only:
        // the client's `LocalPlayer` inventory is the client frame's).
        let facts = (context.side == BlockUseSide::Server)
            .then(|| host.block_use.facts.get(&context.player.uuid).cloned())
            .flatten();
        if let Some(facts) = &facts {
            crate::live_menu::fill_inventory(host, &player_inventory, facts);
        }
        let pos = block_pos_value(context.pos);
        let hit = host.block_use.alloc_block_hit(context.hit.clone());
        let hand = Value::Ref(Some(format!("{HAND_ENUM_PREFIX}{}", context.hand)));
        let stack = context
            .stack
            .clone()
            .unwrap_or_else(menu_models::empty_stack);

        let unknowns_before: BTreeMap<String, usize> =
            host.unknown_host_calls().into_iter().collect();
        let mark = host.call_log_mark();
        let previous_active = host.block_use.active_use.replace(ActiveBlockUse {
            side: context.side,
            dimension: context.dimension.clone(),
            level: level.clone(),
            block: Some((state.clone(), context.pos)),
            player: player.clone(),
            player_data: context.player.clone(),
            player_inventory: player_inventory.clone(),
            container: context.player.container,
            menu_effects: Vec::new(),
            facts: facts.clone(),
            live_effects: Vec::new(),
        });

        let roots = vec![
            block.clone(),
            state.clone(),
            level.clone(),
            pos.clone(),
            player.clone(),
            player_inventory.clone(),
            hit.clone(),
            stack.clone(),
        ];

        let call_result =
            self.with_persistent_runtime_with_budget(host, &roots, budget, |runtime| match kind {
                BlockUseKind::WithoutItem => dispatch_block_virtual(
                    runtime,
                    &block,
                    "useWithoutItem",
                    USE_WITHOUT_ITEM_DESC,
                    &[
                        state.clone(),
                        level.clone(),
                        pos.clone(),
                        player.clone(),
                        hit.clone(),
                    ],
                ),
                BlockUseKind::ItemOn => dispatch_block_virtual(
                    runtime,
                    &block,
                    "useItemOn",
                    USE_ITEM_ON_DESC,
                    &[
                        stack.clone(),
                        state.clone(),
                        level.clone(),
                        pos.clone(),
                        player.clone(),
                        hand.clone(),
                        hit.clone(),
                    ],
                ),
            });

        let finished_active = host.block_use.active_use.take();
        host.block_use.active_use = previous_active;
        host.rewind_call_log(mark);

        // A hook that threw (or ran out of budget) still did what it did before that: a close
        // packet vanilla sent stays sent, so its effects are kept with the failure.
        let (menu_effects, live_effects) = finished_active
            .map(|active| (active.menu_effects, active.live_effects))
            .unwrap_or_default();
        let inventory = facts
            .as_ref()
            .and_then(|facts| crate::live_menu::inventory_out(host, &player_inventory, facts));
        let (raw_result, failure) = match call_result {
            Ok(value) => (value, None),
            Err(error) => (Value::Void, Some(error.to_string())),
        };
        let result = if failure.is_none() {
            decode_interaction_result(host, &raw_result)
        } else {
            None
        };
        let (unknown_host_calls, unknown_host_call_count) =
            unknown_calls_since(host, &unknowns_before);

        Ok(BlockUseOutcome {
            result,
            raw_result,
            menu_effects,
            live_effects,
            inventory,
            unknown_host_calls,
            unknown_host_call_count,
            failure,
        })
    }
}
