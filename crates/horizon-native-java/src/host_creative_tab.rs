//! Fabric's `FabricCreativeModeTab.builder()`, vanilla's `CreativeModeTab` / `CreativeModeTab.Output`
//! / `ItemStack` / `FeatureFlags` / `FeatureFlagSet`, and Fabric's `FabricCreativeModeTabOutput`
//! collector (#453, #610).
//!
//! Bytecode references checked with `javap -p -c` against:
//! * `fabric-creative-tab-api-v1` (5.0.21/5.0.22): `FabricCreativeModeTab`,
//!   `FabricCreativeModeTabBuilderImpl`, `FabricCreativeModeTabOutput`, `CreativeModeTabMixin`.
//! * `server-26.3.jar`: `CreativeModeTab`, `CreativeModeTab$Builder`, `CreativeModeTab$Output`,
//!   `CreativeModeTab$TabVisibility`, `Item`, `Item$Properties`, `BlockItem`, `ItemStack`,
//!   `FeatureFlags`, `FeatureFlagSet`, `FeatureElement`.
//!
//! Every answer matches Java 25 / Minecraft 26.3 / Fabric Creative Tab API; anything whose state
//! is not known (an item with unknown `requiredFeatures`, an unregistered item's `Item.toString`,
//! a `Predicate` placement overload, `getDisplayStacks`, etc.) returns `None` so the host counts
//! the call rather than guessing.

use crate::{
    MethodRef, NativeError, Value,
    host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested},
};
use std::collections::BTreeMap;

const FABRIC: &str = "net/fabricmc/fabric/api/creativetab/v1/FabricCreativeModeTab";
const BUILDER: &str = "net/minecraft/world/item/CreativeModeTab$Builder";
const TAB: &str = "net/minecraft/world/item/CreativeModeTab";
const OUTPUT_FABRIC: &str = "net/fabricmc/fabric/api/creativetab/v1/FabricCreativeModeTabOutput";
const OUTPUT_VANILLA: &str = "net/minecraft/world/item/CreativeModeTab$Output";
const TAB_VISIBILITY_PREFIX: &str = "enum/net/minecraft/world/item/CreativeModeTab$TabVisibility/";
const RETURNS_BUILDER: &str = ")Lnet/minecraft/world/item/CreativeModeTab$Builder;";
const ITEM_STACK: &str = "net/minecraft/world/item/ItemStack";
const FEATURE_FLAGS: &str = "net/minecraft/world/flag/FeatureFlags";
const FEATURE_FLAG: &str = "net/minecraft/world/flag/FeatureFlag";
const FEATURE_FLAG_SET: &str = "net/minecraft/world/flag/FeatureFlagSet";

/// Vanilla 26.3 `FeatureFlags` bit masks (`server-26.3.jar` `FeatureFlags.<clinit>` offsets 0..77,
/// `FeatureFlagRegistry$Builder.create` assigns bits sequentially starting at `1L << 0`).
pub(crate) const FEATURE_VANILLA: u64 = 1 << 0;
pub(crate) const FEATURE_TRADE_REBALANCE: u64 = 1 << 1;
pub(crate) const FEATURE_REDSTONE_EXPERIMENTS: u64 = 1 << 2;
pub(crate) const FEATURE_MINECART_IMPROVEMENTS: u64 = 1 << 3;
/// `FeatureFlags.VANILLA_SET` and `FeatureFlags.DEFAULT_FLAGS` (`FeatureFlags.<clinit>` 61..77).
pub(crate) const FEATURE_VANILLA_SET: u64 = FEATURE_VANILLA;

/// Vanilla 26.3 `CreativeModeTab$TabVisibility` (`server-26.3.jar` `<clinit>` offsets 0..45).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabVisibility {
    ParentAndSearchTabs,
    ParentTabOnly,
    SearchTabOnly,
}

impl TabVisibility {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        let name = crate::expect_ref(value)?.strip_prefix(TAB_VISIBILITY_PREFIX)?;
        Some(match name {
            "PARENT_AND_SEARCH_TABS" => Self::ParentAndSearchTabs,
            "PARENT_TAB_ONLY" => Self::ParentTabOnly,
            "SEARCH_TAB_ONLY" => Self::SearchTabOnly,
            _ => return None,
        })
    }

    pub(crate) fn ordinal(self) -> u8 {
        match self {
            Self::ParentAndSearchTabs => 0,
            Self::ParentTabOnly => 1,
            Self::SearchTabOnly => 2,
        }
    }

    pub(crate) fn from_ordinal(ordinal: u8) -> Option<Self> {
        Some(match ordinal {
            0 => Self::ParentAndSearchTabs,
            1 => Self::ParentTabOnly,
            2 => Self::SearchTabOnly,
            _ => return None,
        })
    }
}

/// Stored fields of a jar-allocated `ItemStack` (`new ItemStack(ItemLike[, int])`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ItemStackData {
    pub item: Value,
    pub count: i32,
}

/// One `CreativeModeTab.Builder`: whether `title` was called, the title it holds (`None` when
/// that was a null, which the host doesn't answer), whether `alignedRight()` was called, and
/// every argument a setter stored.
#[derive(Debug, Default, Clone)]
struct Builder {
    titled: bool,
    title: Option<Value>,
    aligned_right: bool,
    held: Vec<Value>,
}

/// One built `CreativeModeTab`.
#[derive(Debug, Default, Clone)]
struct Tab {
    title: Option<Value>,
    aligned_right: bool,
}

/// One active `FabricCreativeModeTabOutput` collector (`fabric-creative-tab-api-v1` 5.0.21).
#[derive(Debug, Clone)]
struct OutputCollector {
    display_stacks: Vec<Value>,
    search_tab_stacks: Vec<Value>,
    enabled_features: u64,
    has_permissions: bool,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct State {
    next: u64,
    builders: BTreeMap<String, Builder>,
    /// Tab key (`t#<n>`) → its display name (`None`: counted) and right-alignment flag.
    tabs: BTreeMap<String, Tab>,
    /// Active output collectors (`out/<tab>/<n>`) while a tab is being built.
    outputs: BTreeMap<String, OutputCollector>,
}

impl State {
    /// Every Java value held by builders, built tabs, and active output collectors: heap roots.
    pub(crate) fn values(&self) -> impl Iterator<Item = &Value> {
        self.builders
            .values()
            .flat_map(|builder| builder.held.iter())
            .chain(self.tabs.values().filter_map(|tab| tab.title.as_ref()))
            .chain(self.outputs.values().flat_map(|output| {
                output
                    .display_stacks
                    .iter()
                    .chain(output.search_tab_stacks.iter())
            }))
    }
}

fn thrown(class: &str, message: impl Into<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.to_owned(),
        message: Some(message.into()),
        frame: None,
    }
}

fn npe(message: impl Into<String>) -> NativeError {
    thrown("java/lang/NullPointerException", message)
}

fn iae(message: impl Into<String>) -> NativeError {
    thrown("java/lang/IllegalArgumentException", message)
}

fn unknown_text(_: &HostRef, _: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    Ok(None)
}

fn unknown_hash(_: &HostRef, _: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    Ok(None)
}

fn identity_equals(
    a: &HostRef,
    b: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match b {
        Value::Host(b) => a.same_object(b),
        _ => Some(false),
    })
}

fn stays_in_jar(_: &HostRef, _: &dyn Fn(&Value) -> bool) -> bool {
    false
}

/// `FabricCreativeModeTabOutput` (`implements CreativeModeTab$Output`).
fn output_instance_of(_: &HostRef, wanted: &str, _: &dyn ClassHierarchy) -> Option<bool> {
    Some(matches!(
        wanted,
        OUTPUT_FABRIC | OUTPUT_VANILLA | "java/lang/Object"
    ))
}

pub(crate) const OUTPUT_METHODS: Methods = Methods {
    to_string: unknown_text,
    hash_code: unknown_hash,
    equals: identity_equals,
    instance_of: output_instance_of,
    crosses_jars: stays_in_jar,
    compare_to: None,
};

/// `ClassRule::FromKey` for `HostKind::ItemStack`.
pub(crate) fn item_stack_class(_: &str) -> &str {
    ITEM_STACK
}

/// `net.minecraft.world.item.ItemStack` (`server-26.3.jar`):
/// `public final class ItemStack implements DataComponentHolder, ItemInstance`.
fn item_stack_instance_of(
    _: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if matches!(
        wanted,
        ITEM_STACK
            | "net/minecraft/core/component/DataComponentHolder"
            | "net/minecraft/core/component/DataComponentGetter"
            | "net/minecraft/world/item/ItemInstance"
            | "java/lang/Object"
    ) || hierarchy.is_instance(ITEM_STACK, wanted) == Some(true)
    {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false)
        || crate::host_ref::exact_class_rules_out(ITEM_STACK, wanted))
    .then_some(false)
}

/// `ItemStack.toString()` (`server-26.3.jar` offsets 0..16):
/// `this.getCount() + " " + String.valueOf(this.getItem())`.
fn item_stack_to_string(
    handle: &HostRef,
    nested: &mut dyn Nested,
) -> Result<Option<String>, NativeError> {
    let Some(data) = host_item_stack_data_ref(handle) else {
        return Ok(None);
    };
    if is_empty_stack_data(&data.item, data.count) {
        return Ok(Some("0 minecraft:air".to_owned()));
    }
    let Some(item_text) = nested.text(&data.item)? else {
        return Ok(None);
    };
    Ok(Some(format!("{} {item_text}", data.count)))
}

/// `ItemStack` inherits `Object.equals` and `Object.hashCode` (no overrides in 26.3).
pub(crate) const ITEM_STACK_METHODS: Methods = Methods {
    to_string: item_stack_to_string,
    hash_code: unknown_hash,
    equals: identity_equals,
    instance_of: item_stack_instance_of,
    crosses_jars: stays_in_jar,
    compare_to: None,
};

fn crosses_jars_always(_: &HostRef, _: &dyn Fn(&Value) -> bool) -> bool {
    true
}

fn feature_flag_instance_of(
    _: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if matches!(wanted, FEATURE_FLAG | "java/lang/Object")
        || hierarchy.is_instance(FEATURE_FLAG, wanted) == Some(true)
    {
        return Some(true);
    }
    (wanted.starts_with('[')
        || hierarchy.is_interface(wanted) == Some(false)
        || crate::host_ref::exact_class_rules_out(FEATURE_FLAG, wanted)
        || wanted == FEATURE_FLAG_SET)
        .then_some(false)
}

pub(crate) const FEATURE_FLAG_METHODS: Methods = Methods {
    to_string: unknown_text,
    hash_code: unknown_hash,
    equals: identity_equals,
    instance_of: feature_flag_instance_of,
    crosses_jars: crosses_jars_always,
    compare_to: None,
};

fn feature_flag_set_instance_of(
    _: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if matches!(wanted, FEATURE_FLAG_SET | "java/lang/Object")
        || hierarchy.is_instance(FEATURE_FLAG_SET, wanted) == Some(true)
    {
        return Some(true);
    }
    (wanted.starts_with('[')
        || hierarchy.is_interface(wanted) == Some(false)
        || crate::host_ref::exact_class_rules_out(FEATURE_FLAG_SET, wanted)
        || wanted == FEATURE_FLAG)
        .then_some(false)
}

/// `FeatureFlagSet.equals(Object)` (`server-26.3.jar` offsets 0..47):
/// equal iff both are `FeatureFlagSet` with the same universe and `mask`.
/// Within vanilla's single universe (`FeatureFlags.REGISTRY`), `universe == null` iff `mask == 0`,
/// so two vanilla `FeatureFlagSet`s are equal iff their masks match.
fn feature_flag_set_equals(
    a: &HostRef,
    b: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    let Some(mask_a) = parse_flag_set_mask(a.key()) else {
        return Ok(None);
    };
    Ok(match b {
        Value::Host(other) if other.kind() == HostKind::FeatureFlagSet => {
            parse_flag_set_mask(other.key()).map(|mask_b| mask_a == mask_b)
        }
        _ => Some(false),
    })
}

/// `FeatureFlagSet.hashCode()` (`server-26.3.jar` offsets 0..8):
/// `(int) it.unimi.dsi.fastutil.HashCommon.mix(this.mask)`.
fn feature_flag_set_hash(handle: &HostRef, _: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    let Some(mask) = parse_flag_set_mask(handle.key()) else {
        return Ok(None);
    };
    let h = mask.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64);
    let h = h ^ (h >> 32);
    Ok(Some((h ^ (h >> 16)) as i32))
}

pub(crate) const FEATURE_FLAG_SET_METHODS: Methods = Methods {
    to_string: unknown_text,
    hash_code: feature_flag_set_hash,
    equals: feature_flag_set_equals,
    instance_of: feature_flag_set_instance_of,
    crosses_jars: crosses_jars_always,
    compare_to: None,
};

fn parse_flag_set_mask(key: &str) -> Option<u64> {
    key.rsplit_once(':')?.1.parse().ok()
}

pub(crate) fn feature_flag_mask(value: &Value) -> Option<u64> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::FeatureFlag {
        return None;
    }
    Some(match handle.key() {
        "VANILLA" => FEATURE_VANILLA,
        "TRADE_REBALANCE" => FEATURE_TRADE_REBALANCE,
        "REDSTONE_EXPERIMENTS" => FEATURE_REDSTONE_EXPERIMENTS,
        "MINECART_IMPROVEMENTS" => FEATURE_MINECART_IMPROVEMENTS,
        _ => return None,
    })
}

pub(crate) fn feature_flag_set_mask(value: &Value) -> Option<u64> {
    let Value::Host(handle) = value else {
        return None;
    };
    (handle.kind() == HostKind::FeatureFlagSet)
        .then(|| parse_flag_set_mask(handle.key()))
        .flatten()
}

/// Canonical `FeatureFlagSet.EMPTY` (`server-26.3.jar` `FeatureFlagSet.<clinit>` 0..12).
pub(crate) fn empty_feature_flag_set() -> Value {
    Value::Host(HostRef::new(HostKind::FeatureFlagSet, "empty:0"))
}

/// Canonical `FeatureFlags.VANILLA_SET` / `FeatureFlags.DEFAULT_FLAGS`.
pub(crate) fn vanilla_feature_flag_set() -> Value {
    Value::Host(HostRef::new(
        HostKind::FeatureFlagSet,
        format!("vanilla:{FEATURE_VANILLA_SET}"),
    ))
}

/// Allocate a `FeatureFlagSet` for `mask` (`EMPTY` when `mask == 0`, else a fresh instance matching
/// `new FeatureFlagSet(universe, mask)` in `FeatureFlagSet.of` / `join` / `subtract`).
pub(crate) fn new_feature_flag_set(state: &mut State, mask: u64) -> Value {
    if mask == 0 {
        return empty_feature_flag_set();
    }
    state.next += 1;
    Value::Host(HostRef::new(
        HostKind::FeatureFlagSet,
        format!("n:{}:{mask}", state.next),
    ))
}

/// `FeatureFlags.*` static constants (`server-26.3.jar` `FeatureFlags.<clinit>` offsets 0..77).
pub(crate) fn feature_flag_static_field(r: &MethodRef) -> Option<Value> {
    if r.owner != FEATURE_FLAGS {
        return None;
    }
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "VANILLA" | "TRADE_REBALANCE" | "REDSTONE_EXPERIMENTS" | "MINECART_IMPROVEMENTS",
            "Lnet/minecraft/world/flag/FeatureFlag;",
        ) => Some(Value::Host(HostRef::new(HostKind::FeatureFlag, &r.name))),
        ("VANILLA_SET" | "DEFAULT_FLAGS", "Lnet/minecraft/world/flag/FeatureFlagSet;") => {
            Some(vanilla_feature_flag_set())
        }
        _ => None,
    }
}

/// `FeatureFlags.isExperimental(FeatureFlagSet)` (`server-26.3.jar` offsets 0..15):
/// `return !features.isSubsetOf(VANILLA_SET);`
/// Throws `NullPointerException` with HotSpot's exact message when `features` is null.
pub(crate) fn feature_flags_static_call(
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != FEATURE_FLAGS {
        return None;
    }
    match (r.name.as_str(), r.descriptor.as_str(), args) {
        ("isExperimental", "(Lnet/minecraft/world/flag/FeatureFlagSet;)Z", [features]) => {
            if matches!(features, Value::Ref(None)) {
                return Some(Err(npe(
                    "Cannot invoke \"net.minecraft.world.flag.FeatureFlagSet.isSubsetOf(net.minecraft.world.flag.FeatureFlagSet)\" because \"features\" is null",
                )));
            }
            let mask = feature_flag_set_mask(features)?;
            let is_subset = (mask & !FEATURE_VANILLA_SET) == 0;
            Some(Ok(Value::Int(i32::from(!is_subset))))
        }
        _ => None,
    }
}

/// `FeatureFlagSet.of()` and `FeatureFlagSet.of(FeatureFlag)` (`server-26.3.jar` offsets 0..15).
/// The varargs overload `of(FeatureFlag, FeatureFlag...)` is handled in `library_dispatch.rs`
/// where the interpreter's array elements are available.
pub(crate) fn feature_flag_set_static_call(
    state: &mut State,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != FEATURE_FLAG_SET || r.name != "of" {
        return None;
    }
    match (r.descriptor.as_str(), args) {
        ("()Lnet/minecraft/world/flag/FeatureFlagSet;", []) => Some(Ok(empty_feature_flag_set())),
        (
            "(Lnet/minecraft/world/flag/FeatureFlag;)Lnet/minecraft/world/flag/FeatureFlagSet;",
            [flag],
        ) => {
            if matches!(flag, Value::Ref(None)) {
                // `FeatureFlagSet.of(FeatureFlag)` offset 5: `getfield FeatureFlag.universe`.
                return Some(Err(npe(
                    "Cannot read field \"universe\" because \"flag\" is null",
                )));
            }
            let mask = feature_flag_mask(flag)?;
            Some(Ok(new_feature_flag_set(state, mask)))
        }
        _ => None,
    }
}

/// `FeatureFlagSet` instance methods (`server-26.3.jar`).
pub(crate) fn feature_flag_set_instance_call(
    state: &mut State,
    r: &MethodRef,
    recv: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != FEATURE_FLAG_SET {
        return None;
    }
    let mask = feature_flag_set_mask(recv)?;
    match (r.name.as_str(), r.descriptor.as_str(), args) {
        // `FeatureFlagSet.isEmpty()` offsets 0..7.
        ("isEmpty", "()Z", []) => Some(Ok(Value::Int(i32::from(mask == 0)))),
        // `FeatureFlagSet.contains(FeatureFlag)` offsets 0..32.
        ("contains", "(Lnet/minecraft/world/flag/FeatureFlag;)Z", [flag]) => {
            if matches!(flag, Value::Ref(None)) {
                return Some(Err(npe(
                    "Cannot read field \"universe\" because \"flag\" is null",
                )));
            }
            let bit = feature_flag_mask(flag)?;
            Some(Ok(Value::Int(i32::from((mask & bit) != 0))))
        }
        // `FeatureFlagSet.isSubsetOf(FeatureFlagSet)` offsets 0..45:
        // offset 4 returns `true` when `this.universe == null` (i.e. `mask == 0`) before touching `other`.
        ("isSubsetOf", "(Lnet/minecraft/world/flag/FeatureFlagSet;)Z", [other]) => {
            if mask == 0 {
                return Some(Ok(Value::Int(1)));
            }
            if matches!(other, Value::Ref(None)) {
                return Some(Err(npe(
                    "Cannot read field \"universe\" because \"set\" is null",
                )));
            }
            let other_mask = feature_flag_set_mask(other)?;
            Some(Ok(Value::Int(i32::from((mask & !other_mask) == 0))))
        }
        // `FeatureFlagSet.intersects(FeatureFlagSet)` offsets 0..46.
        ("intersects", "(Lnet/minecraft/world/flag/FeatureFlagSet;)Z", [other]) => {
            if mask == 0 {
                return Some(Ok(Value::Int(0)));
            }
            if matches!(other, Value::Ref(None)) {
                return Some(Err(npe(
                    "Cannot read field \"universe\" because \"set\" is null",
                )));
            }
            let other_mask = feature_flag_set_mask(other)?;
            Some(Ok(Value::Int(i32::from((mask & other_mask) != 0))))
        }
        // `FeatureFlagSet.join(FeatureFlagSet)` offsets 0..66.
        (
            "join",
            "(Lnet/minecraft/world/flag/FeatureFlagSet;)Lnet/minecraft/world/flag/FeatureFlagSet;",
            [other],
        ) => {
            if mask == 0 {
                return Some(Ok(other.clone()));
            }
            if matches!(other, Value::Ref(None)) {
                return Some(Err(npe(
                    "Cannot read field \"universe\" because \"other\" is null",
                )));
            }
            let other_mask = feature_flag_set_mask(other)?;
            if other_mask == 0 {
                return Some(Ok(recv.clone()));
            }
            Some(Ok(new_feature_flag_set(state, mask | other_mask)))
        }
        // `FeatureFlagSet.subtract(FeatureFlagSet)` offsets 0..90.
        (
            "subtract",
            "(Lnet/minecraft/world/flag/FeatureFlagSet;)Lnet/minecraft/world/flag/FeatureFlagSet;",
            [other],
        ) => {
            if mask == 0 {
                return Some(Ok(empty_feature_flag_set()));
            }
            if matches!(other, Value::Ref(None)) {
                return Some(Err(npe(
                    "Cannot read field \"universe\" because \"other\" is null",
                )));
            }
            let other_mask = feature_flag_set_mask(other)?;
            if other_mask == 0 {
                return Some(Ok(recv.clone()));
            }
            Some(Ok(new_feature_flag_set(state, mask & !other_mask)))
        }
        _ => None,
    }
}

/// Whether `item` is `Items.AIR` (`game/item/minecraft:air` or `client/item/minecraft:air`).
pub(crate) fn is_air_item(item: &Value) -> bool {
    matches!(
        crate::game_object::handle_key(item),
        Some("game/item/minecraft:air" | "client/item/minecraft:air")
    )
}

/// Canonical server-side `Items.AIR` value.
pub(crate) fn air_item_value() -> Value {
    crate::game_object::server_value("game/item/minecraft:air".to_owned())
}

/// `ItemStack.isEmpty()` (`server-26.3.jar` offsets 0..34):
/// `this == EMPTY || this.item.value() == Items.AIR || this.count <= 0`.
pub(crate) fn is_empty_stack_data(item: &Value, count: i32) -> bool {
    count <= 0 || is_air_item(item)
}

/// Allocate a standalone `ItemStack` handle for unit tests in `output_tests`.
#[cfg(test)]
fn new_host_item_stack(state: &mut State, item: Value, count: i32) -> Value {
    state.next += 1;
    Value::Host(HostRef::of_values(
        HostKind::ItemStack,
        &[Value::Long(state.next as i64), item, Value::Int(count)],
    ))
}

fn host_item_stack_data_ref(handle: &HostRef) -> Option<ItemStackData> {
    if handle.kind() != HostKind::ItemStack {
        return None;
    }
    match handle.values()?.as_slice() {
        [Value::Long(_), item, Value::Int(count)] => Some(ItemStackData {
            item: item.clone(),
            count: *count,
        }),
        _ => None,
    }
}

pub(crate) fn host_item_stack_data(value: &Value) -> Option<ItemStackData> {
    let Value::Host(handle) = value else {
        return None;
    };
    host_item_stack_data_ref(handle)
}

/// Whether `tab_id` (canonical `namespace:path`) is one of the 14 vanilla 26.3 tabs and whether
/// `CreativeModeTab.isAlignedRight()` is true on it (verified on Java 25 against `server-26.3.jar`).
pub(crate) fn vanilla_tab_aligned_right(tab_id: &str) -> Option<bool> {
    Some(match tab_id {
        "minecraft:hotbar" | "minecraft:search" | "minecraft:op_blocks" | "minecraft:inventory" => {
            true
        }
        "minecraft:building_blocks"
        | "minecraft:colored_blocks"
        | "minecraft:natural_blocks"
        | "minecraft:functional_blocks"
        | "minecraft:redstone_blocks"
        | "minecraft:tools_and_utilities"
        | "minecraft:combat"
        | "minecraft:food_and_drinks"
        | "minecraft:ingredients"
        | "minecraft:spawn_eggs" => false,
        _ => return None,
    })
}

/// A canonical `CreativeModeTab` handle for a vanilla (or host-provided) tab id.
pub(crate) fn vanilla_tab_handle(tab_id: &str) -> Value {
    Value::Host(HostRef::new(
        HostKind::CreativeModeTab,
        format!("tab:{tab_id}"),
    ))
}

/// `CreativeModeTab.isAlignedRight()` for a host `CreativeModeTab` handle.
pub(crate) fn tab_aligned_right(state: &State, tab: &Value) -> Option<bool> {
    let Value::Host(handle) = tab else {
        return None;
    };
    if handle.kind() != HostKind::CreativeModeTab {
        return None;
    }
    if let Some(built) = state.tabs.get(handle.key()) {
        return Some(built.aligned_right);
    }
    handle
        .key()
        .strip_prefix("tab:")
        .and_then(vanilla_tab_aligned_right)
}

/// Open a new `FabricCreativeModeTabOutput` collector with default vanilla features and no OP perms.
pub(crate) fn begin_output(state: &mut State, tab: &str) -> Value {
    begin_output_with_initial(
        state,
        tab,
        FEATURE_VANILLA_SET,
        false,
        Vec::new(),
        Vec::new(),
    )
}

/// Open a new `FabricCreativeModeTabOutput` collector seeded with the tab's existing display and
/// search entries (`CreativeModeTabMixin.getStacks` offsets 64..102).
pub(crate) fn begin_output_with_initial(
    state: &mut State,
    tab: &str,
    enabled_features: u64,
    has_permissions: bool,
    display_stacks: Vec<Value>,
    search_tab_stacks: Vec<Value>,
) -> Value {
    state.next += 1;
    let key = format!("out/{tab}/{}", state.next);
    state.outputs.insert(
        key.clone(),
        OutputCollector {
            display_stacks,
            search_tab_stacks,
            enabled_features,
            has_permissions,
        },
    );
    Value::Host(HostRef::new(HostKind::CreativeModeTabOutput, key))
}

/// Read `(enabled_features, has_permissions)` for an active `FabricCreativeModeTabOutput`.
pub(crate) fn output_context(state: &State, output: &Value) -> Option<(u64, bool)> {
    let Value::Host(handle) = output else {
        return None;
    };
    if handle.kind() != HostKind::CreativeModeTabOutput {
        return None;
    }
    let collector = state.outputs.get(handle.key())?;
    Some((collector.enabled_features, collector.has_permissions))
}

/// Append an already-validated, enabled `ItemStack` to an active `FabricCreativeModeTabOutput`
/// (`FabricCreativeModeTabOutput.accept(ItemStack, TabVisibility)` offsets 12..98).
pub(crate) fn accept_verified_stack(
    state: &mut State,
    output: &Value,
    stack: Value,
    visibility: TabVisibility,
) -> bool {
    let Value::Host(handle) = output else {
        return false;
    };
    if handle.kind() != HostKind::CreativeModeTabOutput {
        return false;
    }
    let Some(collector) = state.outputs.get_mut(handle.key()) else {
        return false;
    };
    match visibility {
        // Offsets 48..70: PARENT_AND_SEARCH_TABS appends to both displayStacks and searchTabStacks.
        TabVisibility::ParentAndSearchTabs => {
            collector.display_stacks.push(stack.clone());
            collector.search_tab_stacks.push(stack);
        }
        // Offsets 73..84: PARENT_TAB_ONLY appends to displayStacks only.
        TabVisibility::ParentTabOnly => {
            collector.display_stacks.push(stack);
        }
        // Offsets 87..98: SEARCH_TAB_ONLY appends to searchTabStacks only.
        TabVisibility::SearchTabOnly => {
            collector.search_tab_stacks.push(stack);
        }
    }
    true
}

/// Prepend an already-validated, enabled `ItemStack` at index 0 of an active
/// `FabricCreativeModeTabOutput` (`FabricCreativeModeTabOutput.prepend` offsets 12..98).
pub(crate) fn prepend_verified_stack(
    state: &mut State,
    output: &Value,
    stack: Value,
    visibility: TabVisibility,
) -> bool {
    let Value::Host(handle) = output else {
        return false;
    };
    if handle.kind() != HostKind::CreativeModeTabOutput {
        return false;
    }
    let Some(collector) = state.outputs.get_mut(handle.key()) else {
        return false;
    };
    match visibility {
        TabVisibility::ParentAndSearchTabs => {
            collector.display_stacks.insert(0, stack.clone());
            collector.search_tab_stacks.insert(0, stack);
        }
        TabVisibility::ParentTabOnly => {
            collector.display_stacks.insert(0, stack);
        }
        TabVisibility::SearchTabOnly => {
            collector.search_tab_stacks.insert(0, stack);
        }
    }
    true
}

fn find_relative_index(
    list: &[Value],
    before: bool,
    matches: &mut impl FnMut(&Value) -> Result<Option<bool>, NativeError>,
) -> Result<Option<usize>, NativeError> {
    if before {
        for (index, entry) in list.iter().enumerate() {
            match matches(entry)? {
                Some(true) => return Ok(Some(index)),
                Some(false) => {}
                None => return Ok(None),
            }
        }
    } else {
        for (index, entry) in list.iter().enumerate().rev() {
            match matches(entry)? {
                Some(true) => return Ok(Some(index + 1)),
                Some(false) => {}
                None => return Ok(None),
            }
        }
    }
    Ok(Some(list.len()))
}

/// Insert already-validated, enabled `ItemStack`s before the first matching anchor or after the
/// last matching anchor (appending to the end when absent), matching `FabricCreativeModeTabOutput`
/// `insertBefore` / `insertAfter` (5.0.21 offsets 655..801).
///
/// Computes target indices before mutating either list so an unknown comparison returns `Ok(None)`
/// without partial state changes.
pub(crate) fn insert_relative_verified(
    state: &mut State,
    output: &Value,
    stacks: Vec<Value>,
    visibility: TabVisibility,
    before: bool,
    mut matches: impl FnMut(&Value) -> Result<Option<bool>, NativeError>,
) -> Result<Option<bool>, NativeError> {
    let Value::Host(handle) = output else {
        return Ok(Some(false));
    };
    if handle.kind() != HostKind::CreativeModeTabOutput {
        return Ok(Some(false));
    }
    let Some(collector) = state.outputs.get(handle.key()) else {
        return Ok(Some(false));
    };
    let display_idx = match visibility {
        TabVisibility::ParentAndSearchTabs | TabVisibility::ParentTabOnly => {
            let Some(idx) = find_relative_index(&collector.display_stacks, before, &mut matches)?
            else {
                return Ok(None);
            };
            Some(idx)
        }
        TabVisibility::SearchTabOnly => None,
    };
    let search_idx = match visibility {
        TabVisibility::ParentAndSearchTabs | TabVisibility::SearchTabOnly => {
            let Some(idx) =
                find_relative_index(&collector.search_tab_stacks, before, &mut matches)?
            else {
                return Ok(None);
            };
            Some(idx)
        }
        TabVisibility::ParentTabOnly => None,
    };
    let collector = state
        .outputs
        .get_mut(handle.key())
        .expect("verified active output collector");
    match (display_idx, search_idx) {
        (Some(d), Some(s)) => {
            collector.display_stacks.splice(d..d, stacks.clone());
            collector.search_tab_stacks.splice(s..s, stacks);
        }
        (Some(d), None) => {
            collector.display_stacks.splice(d..d, stacks);
        }
        (None, Some(s)) => {
            collector.search_tab_stacks.splice(s..s, stacks);
        }
        (None, None) => {}
    }
    Ok(Some(true))
}

/// Validate a stack according to `FabricCreativeModeTabOutput.checkStack(ItemStack)`
/// (`fabric-creative-tab-api-v1` 5.0.21 offsets 0..42) and append it by visibility.
///
/// Caller has already evaluated `FabricCreativeModeTabOutput.isEnabled(stack)` (offsets 0..5).
/// Returns `Ok(true)` when accepted or skipped, `Ok(false)` when `stack_text` is needed for the
/// count!=1 exception message but unknown (caller must count), or `Err(...)` for the exact
/// `IllegalArgumentException` / `NullPointerException`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn accept_checked_stack(
    state: &mut State,
    output: &Value,
    stack: Value,
    is_empty: bool,
    count: i32,
    is_enabled: bool,
    stack_text: Option<&str>,
    visibility: Option<TabVisibility>,
) -> Result<bool, NativeError> {
    // `FabricCreativeModeTabOutput.accept(ItemStack, TabVisibility)` offsets 0..5:
    // `if (!this.isEnabled(stack)) return;` runs BEFORE `checkStack` and BEFORE `visibility.ordinal()`.
    if !is_enabled {
        return Ok(true);
    }
    // `FabricCreativeModeTabOutput.checkStack(ItemStack)` offsets 0..16.
    if is_empty {
        return Err(iae("Cannot add empty stack"));
    }
    // `FabricCreativeModeTabOutput.checkStack(ItemStack)` offsets 17..41.
    if count != 1 {
        let Some(text) = stack_text else {
            return Ok(false);
        };
        return Err(iae(format!(
            "Stack size must be exactly 1 for stack: {text}"
        )));
    }
    // `FabricCreativeModeTabOutput.accept` offset 16: `visibility.ordinal()` throws NPE on null.
    let Some(visibility) = visibility else {
        return Err(npe(
            "Cannot invoke \"net.minecraft.world.item.CreativeModeTab$TabVisibility.ordinal()\" because \"visibility\" is null",
        ));
    };
    Ok(accept_verified_stack(state, output, stack, visibility))
}

/// Finish an active collector and return `(display_stacks, search_tab_stacks)`.
pub(crate) fn take_output_lists(
    state: &mut State,
    output: &Value,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let Value::Host(handle) = output else {
        return None;
    };
    (handle.kind() == HostKind::CreativeModeTabOutput).then(|| {
        state
            .outputs
            .remove(handle.key())
            .map(|c| (c.display_stacks, c.search_tab_stacks))
            .unwrap_or_default()
    })
}

/// Finish an active collector and return its `display_stacks`.
pub(crate) fn take_output(state: &mut State, output: &Value) -> Option<Vec<Value>> {
    take_output_lists(state, output).map(|(display, _)| display)
}

/// The argument a public setter stores, by name and parameter list; `None` for any other method.
fn setter(name: &str, parameters: &str) -> Option<bool> {
    Some(match (name, parameters) {
        ("title", "(Lnet/minecraft/network/chat/Component;")
        | ("icon", "(Ljava/util/function/Supplier;")
        | ("displayItems", "(Lnet/minecraft/world/item/CreativeModeTab$DisplayItemsGenerator;")
        | ("backgroundTexture", "(Lnet/minecraft/resources/Identifier;") => true,
        ("alignedRight" | "hideTitle" | "noScrollBar", "(") => false,
        _ => return None,
    })
}

pub(crate) fn static_call(
    state: &mut State,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == FABRIC
        && r.name == "builder"
        && r.descriptor == "()Lnet/minecraft/world/item/CreativeModeTab$Builder;"
    {
        state.next += 1;
        // `/`, not `#`: a handle of its own kind, numbered by this model; `<text>#<digits>`
        // spells a jar object's allocation key, whose number the collector reads as the heap's
        // (#669 review).
        let k = format!("b/{}", state.next);
        state.builders.insert(k.clone(), Builder::default());
        return Some(Ok(Value::Host(HostRef::new(
            HostKind::CreativeModeTabBuilder,
            k,
        ))));
    }
    if let Some(res) = feature_flags_static_call(r, args) {
        return Some(res);
    }
    feature_flag_set_static_call(state, r, args)
}

pub(crate) fn instance_call(
    state: &mut State,
    r: &MethodRef,
    recv: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let Value::Host(h) = recv else { return None };
    match h.kind() {
        HostKind::FeatureFlagSet => feature_flag_set_instance_call(state, r, recv, args),
        HostKind::CreativeModeTabBuilder => {
            if r.owner != BUILDER {
                return None;
            }
            if r.name == "build" && r.descriptor == "()Lnet/minecraft/world/item/CreativeModeTab;" {
                let builder = state.builders.get(h.key())?;
                if !builder.titled {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalStateException".into(),
                        message: Some("No display name set for CreativeModeTab".into()),
                        frame: None,
                    }));
                }
                let tab = Tab {
                    title: builder.title.clone(),
                    aligned_right: builder.aligned_right,
                };
                state.next += 1;
                // `/`: see the builder's key.
                let k = format!("t/{}", state.next);
                state.tabs.insert(k.clone(), tab);
                return Some(Ok(Value::Host(HostRef::new(HostKind::CreativeModeTab, k))));
            }
            let parameters = r.descriptor.strip_suffix(RETURNS_BUILDER)?;
            let stores = setter(&r.name, parameters)?;
            let builder = state.builders.get_mut(h.key())?;
            if stores {
                let [arg] = args else { return None };
                builder.held.push(arg.clone());
                if r.name == "title" {
                    builder.titled = true;
                    builder.title = (!matches!(arg, Value::Ref(None))).then(|| arg.clone());
                }
            } else if r.name == "alignedRight" {
                builder.aligned_right = true;
            }
            Some(Ok(recv.clone()))
        }
        HostKind::CreativeModeTab => {
            if r.owner != TAB {
                return None;
            }
            match (r.name.as_str(), r.descriptor.as_str()) {
                ("getDisplayName", "()Lnet/minecraft/network/chat/Component;") => {
                    Some(Ok(state.tabs.get(h.key())?.title.clone()?))
                }
                ("isAlignedRight", "()Z") => {
                    let aligned = tab_aligned_right(state, recv)?;
                    Some(Ok(Value::Int(i32::from(aligned))))
                }
                _ => None,
            }
        }
        HostKind::CreativeModeTabOutput => {
            let collector = state.outputs.get(h.key())?;
            let (enabled_features, has_permissions) =
                (collector.enabled_features, collector.has_permissions);
            // `FabricCreativeModeTabOutput.getEnabledFeatures()` (offsets 0..7) and
            // `shouldShowOpRestrictedItems()` (offsets 0..7).
            match (r.name.as_str(), r.descriptor.as_str()) {
                ("getEnabledFeatures", "()Lnet/minecraft/world/flag/FeatureFlagSet;") => {
                    let set = if enabled_features == FEATURE_VANILLA_SET {
                        vanilla_feature_flag_set()
                    } else {
                        new_feature_flag_set(state, enabled_features)
                    };
                    Some(Ok(set))
                }
                ("shouldShowOpRestrictedItems", "()Z") => {
                    Some(Ok(Value::Int(i32::from(has_permissions))))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod output_tests {
    use super::*;

    #[test]
    fn output_records_visibility_lists_and_roots_values() {
        let mut state = State::default();
        let output = begin_output(&mut state, "minecraft:combat");
        let first_item = crate::game_object::server_value("game/item/minecraft:diamond".into());
        let second_item = crate::game_object::server_value("game/item/minecraft:apple".into());
        let third_item = crate::game_object::server_value("game/item/minecraft:bread".into());
        let first = new_host_item_stack(&mut state, first_item, 1);
        let second = new_host_item_stack(&mut state, second_item, 1);
        let third = new_host_item_stack(&mut state, third_item, 1);

        assert!(
            accept_checked_stack(
                &mut state,
                &output,
                first.clone(),
                false,
                1,
                true,
                Some("1 minecraft:diamond"),
                Some(TabVisibility::ParentAndSearchTabs),
            )
            .unwrap()
        );
        assert!(
            accept_checked_stack(
                &mut state,
                &output,
                second.clone(),
                false,
                1,
                true,
                Some("1 minecraft:apple"),
                Some(TabVisibility::ParentTabOnly),
            )
            .unwrap()
        );
        assert!(
            accept_checked_stack(
                &mut state,
                &output,
                third.clone(),
                false,
                1,
                true,
                Some("1 minecraft:bread"),
                Some(TabVisibility::SearchTabOnly),
            )
            .unwrap()
        );
        // State::values() must root every accepted stack in both display and search lists.
        let rooted: Vec<&Value> = state.values().collect();
        assert!(rooted.contains(&&first));
        assert!(rooted.contains(&&second));
        assert!(rooted.contains(&&third));

        assert_eq!(
            take_output_lists(&mut state, &output),
            Some((vec![first.clone(), second], vec![first, third]))
        );
    }

    #[test]
    fn check_stack_rejects_empty_and_count_not_one_after_enabled_check() {
        let mut state = State::default();
        let output = begin_output(&mut state, "minecraft:combat");
        let item = crate::game_object::server_value("game/item/minecraft:diamond".into());
        let two = new_host_item_stack(&mut state, item.clone(), 2);

        // Disabled count-2 stack skips silently before checkStack / visibility NPE.
        assert!(
            accept_checked_stack(
                &mut state,
                &output,
                two.clone(),
                false,
                2,
                false,
                Some("2 minecraft:diamond"),
                None,
            )
            .unwrap()
        );
        // Enabled empty stack throws exact IllegalArgumentException.
        assert!(matches!(
            accept_checked_stack(
                &mut state,
                &output,
                crate::menu_models::empty_stack(),
                true,
                0,
                true,
                Some("0 minecraft:air"),
                Some(TabVisibility::ParentAndSearchTabs),
            ),
            Err(NativeError::UncaughtException {
                class,
                message: Some(msg),
                ..
            }) if class == "java/lang/IllegalArgumentException" && msg == "Cannot add empty stack"
        ));
        // Enabled count-2 stack throws exact IllegalArgumentException with stack text.
        assert!(matches!(
            accept_checked_stack(
                &mut state,
                &output,
                two,
                false,
                2,
                true,
                Some("2 minecraft:diamond"),
                Some(TabVisibility::ParentAndSearchTabs),
            ),
            Err(NativeError::UncaughtException {
                class,
                message: Some(msg),
                ..
            }) if class == "java/lang/IllegalArgumentException"
                && msg == "Stack size must be exactly 1 for stack: 2 minecraft:diamond"
        ));
        assert_eq!(
            take_output_lists(&mut state, &output),
            Some((vec![], vec![]))
        );
    }

    #[test]
    fn copper_block_registry_item_is_accepted_without_counting() {
        use crate::Host;
        let mut host = crate::StdHost::new();
        let output = host.begin_creative_tab_output("minecraft:building_blocks");
        let copper_block =
            crate::game_object::server_value("game/item/minecraft:copper_block".into());
        let accept = MethodRef {
            owner: OUTPUT_FABRIC.into(),
            name: "accept".into(),
            descriptor: "(Lnet/minecraft/world/level/ItemLike;)V".into(),
        };
        host.invoke_instance(&accept, &output, std::slice::from_ref(&copper_block))
            .unwrap();
        assert_eq!(host.unknown_host_call_count(), 0);
        let stacks = host.take_creative_tab_output(&output).unwrap();
        assert_eq!(stacks.len(), 1);
        assert_eq!(
            host.item_stack_item_id(&stacks[0]).as_deref(),
            Some("minecraft:copper_block")
        );
        assert_eq!(host.item_stack_count(&stacks[0]), Some(1));
        assert_eq!(host.item_stack_is_empty(&stacks[0]), Some(false));

        // Direct `StdHost` `ItemStack.isItemEnabled(FeatureFlagSet)` (`server-26.3.jar` 0..23):
        // empty stacks short-circuit to `true` before touching `flags` (even when `flags` is null).
        let is_item_enabled = MethodRef {
            owner: ITEM_STACK.into(),
            name: "isItemEnabled".into(),
            descriptor: "(Lnet/minecraft/world/flag/FeatureFlagSet;)Z".into(),
        };
        let empty_flags = empty_feature_flag_set();
        let vanilla_flags = vanilla_feature_flag_set();
        let empty_stack = crate::menu_models::empty_stack();
        assert_eq!(
            host.invoke_instance(
                &is_item_enabled,
                &empty_stack,
                std::slice::from_ref(&empty_flags)
            )
            .unwrap(),
            Value::Int(1)
        );
        assert_eq!(
            host.invoke_instance(&is_item_enabled, &empty_stack, &[Value::Ref(None)])
                .unwrap(),
            Value::Int(1)
        );
        assert_eq!(
            host.invoke_instance(
                &is_item_enabled,
                &stacks[0],
                std::slice::from_ref(&empty_flags)
            )
            .unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            host.invoke_instance(
                &is_item_enabled,
                &stacks[0],
                std::slice::from_ref(&vanilla_flags)
            )
            .unwrap(),
            Value::Int(1)
        );
        assert_eq!(host.unknown_host_call_count(), 0);
    }

    #[test]
    fn feature_flags_is_experimental_and_handle_metadata_match_mc263() {
        use crate::Host;

        let mut host = crate::StdHost::new();
        let m = |owner: &str, name: &str, descriptor: &str| MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        };
        let vanilla = host
            .get_static(&m(FEATURE_FLAGS, "VANILLA", &format!("L{FEATURE_FLAG};")))
            .unwrap();
        let trade = host
            .get_static(&m(
                FEATURE_FLAGS,
                "TRADE_REBALANCE",
                &format!("L{FEATURE_FLAG};"),
            ))
            .unwrap();
        let vanilla_set = host
            .get_static(&m(
                FEATURE_FLAGS,
                "VANILLA_SET",
                &format!("L{FEATURE_FLAG_SET};"),
            ))
            .unwrap();
        let empty = host
            .invoke(
                &m(FEATURE_FLAG_SET, "of", &format!("()L{FEATURE_FLAG_SET};")),
                &[],
            )
            .unwrap();
        let of_vanilla = host
            .invoke(
                &m(
                    FEATURE_FLAG_SET,
                    "of",
                    &format!("(L{FEATURE_FLAG};)L{FEATURE_FLAG_SET};"),
                ),
                std::slice::from_ref(&vanilla),
            )
            .unwrap();
        let of_trade = host
            .invoke(
                &m(
                    FEATURE_FLAG_SET,
                    "of",
                    &format!("(L{FEATURE_FLAG};)L{FEATURE_FLAG_SET};"),
                ),
                std::slice::from_ref(&trade),
            )
            .unwrap();
        let joined = host
            .invoke_instance(
                &m(
                    FEATURE_FLAG_SET,
                    "join",
                    &format!("(L{FEATURE_FLAG_SET};)L{FEATURE_FLAG_SET};"),
                ),
                &vanilla_set,
                std::slice::from_ref(&of_trade),
            )
            .unwrap();
        let is_exp = m(
            FEATURE_FLAGS,
            "isExperimental",
            &format!("(L{FEATURE_FLAG_SET};)Z"),
        );
        assert_eq!(
            host.invoke(&is_exp, std::slice::from_ref(&empty)).unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            host.invoke(&is_exp, std::slice::from_ref(&vanilla_set))
                .unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            host.invoke(&is_exp, std::slice::from_ref(&of_vanilla))
                .unwrap(),
            Value::Int(0)
        );
        assert_eq!(
            host.invoke(&is_exp, std::slice::from_ref(&of_trade))
                .unwrap(),
            Value::Int(1)
        );
        assert_eq!(
            host.invoke(&is_exp, std::slice::from_ref(&joined)).unwrap(),
            Value::Int(1)
        );
        assert!(matches!(
            host.invoke(&is_exp, &[Value::Ref(None)]),
            Err(NativeError::UncaughtException {
                class,
                message: Some(msg),
                ..
            }) if class == "java/lang/NullPointerException"
                && msg == "Cannot invoke \"net.minecraft.world.flag.FeatureFlagSet.isSubsetOf(net.minecraft.world.flag.FeatureFlagSet)\" because \"features\" is null"
        ));
        assert_eq!(host.unknown_host_call_count(), 0);

        // `crosses_jars` is true for both FeatureFlag and FeatureFlagSet, and `instance_of`
        // consults ClassHierarchy (honouring Mixin-added interfaces while ruling out final JDK types).
        struct WithMixin;
        impl ClassHierarchy for WithMixin {
            fn is_instance(&self, class: &str, wanted: &str) -> Option<bool> {
                if (class == FEATURE_FLAG || class == FEATURE_FLAG_SET)
                    && wanted == "demo/MixinAdded"
                {
                    return Some(true);
                }
                crate::host_ref::JdkHierarchy.is_instance(class, wanted)
            }
            fn is_interface(&self, class: &str) -> Option<bool> {
                crate::host_ref::JdkHierarchy.is_interface(class)
            }
        }
        let Value::Host(flag_h) = &vanilla else {
            panic!("expected HostRef for FeatureFlag");
        };
        let Value::Host(set_h) = &vanilla_set else {
            panic!("expected HostRef for FeatureFlagSet");
        };
        assert!(flag_h.crosses_jars(&|_| false));
        assert!(set_h.crosses_jars(&|_| false));
        assert_eq!(
            flag_h.instance_of("demo/MixinAdded", &WithMixin),
            Some(true)
        );
        assert_eq!(set_h.instance_of("demo/MixinAdded", &WithMixin), Some(true));
        assert_eq!(
            flag_h.instance_of("java/lang/String", &WithMixin),
            Some(false)
        );
        assert_eq!(
            set_h.instance_of("java/lang/String", &WithMixin),
            Some(false)
        );
        assert_eq!(
            flag_h.instance_of(FEATURE_FLAG_SET, &WithMixin),
            Some(false)
        );
        assert_eq!(set_h.instance_of(FEATURE_FLAG, &WithMixin), Some(false));
        assert_eq!(flag_h.instance_of("demo/UnknownIface", &WithMixin), None);
        assert_eq!(set_h.instance_of("demo/UnknownIface", &WithMixin), None);
    }
}
