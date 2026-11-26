//! General vanilla 26.3 recipe, ingredient, live item stack, and component-patch models.
//!
//! Nothing here knows any particular mod, recipe class, or slot layout. Mod recipes decode
//! their own `data/<ns>/recipe/*.json` resources through their own `RecipeSerializer` / DFU
//! `MapCodec`, and match/assemble inputs by executing their own `Recipe.matches` and
//! `Recipe.assemble` bytecode.

use std::collections::{BTreeMap, BTreeSet};

use crate::runtime::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested, exact_class_rules_out};
use crate::runtime::{
    Boxed, Host, MethodRef, NativeError, Runtime, StdHost, Value, expect_int, expect_ref,
    vanilla_item_data,
};

pub(crate) const ITEM_STACK: &str = "net/minecraft/world/item/ItemStack";
pub(crate) const ITEM_INSTANCE: &str = "net/minecraft/world/item/ItemInstance";
pub(crate) const DATA_COMPONENT_HOLDER: &str = "net/minecraft/core/component/DataComponentHolder";
pub(crate) const DATA_COMPONENT_GETTER: &str = "net/minecraft/core/component/DataComponentGetter";
pub(crate) const DATA_COMPONENT_PATCH: &str = "net/minecraft/core/component/DataComponentPatch";
pub(crate) const DATA_COMPONENT_PATCH_BUILDER: &str =
    "net/minecraft/core/component/DataComponentPatch$Builder";
pub(crate) const DATA_COMPONENTS: &str = "net/minecraft/core/component/DataComponents";
pub(crate) const INGREDIENT: &str = "net/minecraft/world/item/crafting/Ingredient";
pub(crate) const RECIPE_COMMON_INFO: &str = "net/minecraft/world/item/crafting/Recipe$CommonInfo";
pub(crate) const RECIPE_HOLDER: &str = "net/minecraft/world/item/crafting/RecipeHolder";
pub(crate) const RECIPE_MANAGER: &str = "net/minecraft/world/item/crafting/RecipeManager";
pub(crate) const RECIPE_ACCESS: &str = "net/minecraft/world/item/crafting/RecipeAccess";
pub(crate) const TRANSMUTE_RECIPE: &str = "net/minecraft/world/item/crafting/TransmuteRecipe";
pub(crate) const CUSTOM_DATA: &str = "net/minecraft/world/item/component/CustomData";
pub(crate) const UNIT: &str = "net/minecraft/util/Unit";
pub(crate) const EMPTY_PATCH_KEY: &str = "empty";
pub(crate) const EMPTY_CUSTOM_DATA_KEY: &str = "empty";
pub(crate) const UNIT_INSTANCE_KEY: &str = "INSTANCE";
pub(crate) const TOOLTIP_DISPLAY_CANONICAL_DEFAULT: &str = "canonical-default";
pub(crate) const RECIPE_MANAGER_KEY: &str = "game/recipe-manager";

/// State carried by a modelled `net.minecraft.world.item.component.TooltipDisplay` handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TooltipDisplayData {
    pub(crate) hide_tooltip: bool,
    pub(crate) hidden_components: Vec<String>,
    /// True when `hidden_components` is complete (so an absent component id is proven shown).
    /// False when Sunrise's tooltip display carried unrepresentable entries or `hide_tooltip`
    /// masked `hidden_components` without raw NBT.
    pub(crate) hidden_known: bool,
}

/// Typed representation of a modelled `ItemStack` data component crossing the host boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeComponentValue {
    Int(i32),
    Unit,
    CustomData(crate::runtime::NbtCompound),
    ComponentJson(String),
    TooltipDisplay {
        hide_tooltip: bool,
        hidden_components: Vec<String>,
        hidden_known: bool,
    },
    /// The stack holds a component value this boundary cannot represent accurately; callers must
    /// count and skip rather than dropping the component.
    Unsupported,
    /// A jar data component type's value as the NBT its own persistent codec writes
    /// (`item_parts.rs`): only [`crate::runtime::NativeJar::make_item_stack`] decodes it, which runs the
    /// codec; a host-only conversion keeps it opaque.
    Nbt(crate::runtime::Nbt),
}

/// Extracted `ItemStack` data for converting a Java stack across the game boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct StackComponents {
    pub item_id: String,
    pub count: i32,
    pub patch: Vec<(String, Option<RuntimeComponentValue>)>,
    pub opaque_components: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ComponentPatch {
    pub(crate) entries: BTreeMap<String, Option<Value>>,
}

impl ComponentPatch {
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn size(&self) -> i32 {
        self.entries.len() as i32
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LiveStack {
    /// Exact `Item` object when constructed from an `Item`/`ItemLike` value (preserving jar
    /// subclass identity, `toString()`, `isEnabled()`, and `requiredFeatures`), or `None` when
    /// constructed purely from an item id (JSON/template/Holder).
    pub(crate) item_value: Option<Value>,
    /// Canonical `namespace:path` item id when known (`None` for unregistered custom items).
    pub(crate) item_id: Option<String>,
    pub(crate) count: i32,
    pub(crate) patch: ComponentPatch,
    /// True when the stack came from the host game with custom/damage components that this
    /// model does not materialize: `ItemStack.isSameItemSameComponents` and `getComponentsPatch`
    /// count instead of falsely treating the patch as complete.
    pub(crate) opaque_components: bool,
    /// Specific component ids that came from the host game with unmodelled values (or removals).
    /// Reading/writing one of these ids counts as unknown, while modelled sibling components on
    /// the same stack still answer accurately.
    pub(crate) opaque_keys: BTreeSet<String>,
}

impl LiveStack {
    pub(crate) fn from_item_id(item_id: String, count: i32, patch: ComponentPatch) -> Self {
        Self {
            item_value: None,
            item_id: Some(item_id),
            count,
            patch,
            opaque_components: false,
            opaque_keys: BTreeSet::new(),
        }
    }

    pub(crate) fn from_host_item_id(
        host: &StdHost,
        item_id: &str,
        count: i32,
        opaque_components: bool,
    ) -> Self {
        Self::from_host_stack(
            host,
            None,
            item_id,
            count,
            ComponentPatch::default(),
            opaque_components,
            BTreeSet::new(),
        )
    }

    pub(crate) fn from_host_stack(
        host: &StdHost,
        item_override: Option<Value>,
        item_id: &str,
        count: i32,
        patch: ComponentPatch,
        opaque_components: bool,
        opaque_keys: BTreeSet<String>,
    ) -> Self {
        let item_value = item_override.unwrap_or_else(|| item_value_for_id(host, item_id));
        Self {
            item_value: Some(item_value),
            item_id: Some(item_id.to_owned()),
            count,
            patch,
            opaque_components: opaque_components || !opaque_keys.is_empty(),
            opaque_keys,
        }
    }

    pub(crate) fn from_item_value(host: &StdHost, item: Value, count: i32) -> Self {
        let item_id = resolve_item_id(host, &item);
        Self {
            item_value: Some(item),
            item_id,
            count,
            patch: ComponentPatch::default(),
            opaque_components: false,
            opaque_keys: BTreeSet::new(),
        }
    }

    pub(crate) fn has_opaque_components(&self) -> bool {
        self.opaque_components || !self.opaque_keys.is_empty()
    }

    pub(crate) fn is_key_opaque(&self, comp_id: &str) -> bool {
        self.opaque_keys.contains(comp_id)
            || (self.opaque_components && self.opaque_keys.is_empty())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.count <= 0
            || self.item_id.as_deref() == Some("minecraft:air")
            || self
                .item_value
                .as_ref()
                .is_some_and(crate::runtime::host_creative_tab::is_air_item)
    }

    pub(crate) fn resolved_item_id(&self, host: &StdHost) -> Option<String> {
        if self.is_empty() {
            return Some("minecraft:air".to_owned());
        }
        self.item_id.clone().or_else(|| {
            self.item_value
                .as_ref()
                .and_then(|value| resolve_item_id(host, value))
        })
    }

    pub(crate) fn effective_item_value(&self, host: &StdHost) -> Value {
        if self.is_empty() {
            return crate::runtime::host_creative_tab::air_item_value();
        }
        if let Some(value) = &self.item_value {
            return value.clone();
        }
        if let Some(id) = &self.item_id {
            return item_value_for_id(host, id);
        }
        crate::runtime::host_creative_tab::air_item_value()
    }

    pub(crate) fn effective_count(&self) -> i32 {
        if self.is_empty() { 0 } else { self.count }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TemplateData {
    pub(crate) item_id: String,
    pub(crate) count: i32,
    pub(crate) patch: ComponentPatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IngredientSpec {
    Tag(String),
    Items(Vec<String>),
}

#[derive(Clone, Debug)]
pub(crate) struct HolderData {
    pub(crate) id: Value,
    pub(crate) value: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedRecipe {
    pub(crate) id: String,
    pub(crate) source_pack: String,
    pub(crate) recipe_type: Value,
    pub(crate) recipe: Value,
    pub(crate) holder: Value,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LoadedRecipeCache {
    pub(crate) fingerprint: (usize, usize, u64, usize),
    pub(crate) recipes: Vec<LoadedRecipe>,
    pub(crate) errors: Vec<(String, String)>,
    pub(crate) unmodelled_serializers: BTreeSet<String>,
    pub(crate) skipped_recipe_ids: BTreeSet<String>,
}

/// Persistent recipe, live-stack, template, ingredient, and component-patch state on [`StdHost`].
#[derive(Clone, Debug, Default)]
pub(crate) struct RecipeState {
    next_id: u64,
    pub(crate) stacks: BTreeMap<String, LiveStack>,
    pub(crate) templates: BTreeMap<String, TemplateData>,
    pub(crate) patches: BTreeMap<String, ComponentPatch>,
    pub(crate) patch_builders: BTreeMap<String, ComponentPatch>,
    pub(crate) ingredients: BTreeMap<String, IngredientSpec>,
    pub(crate) holders: BTreeMap<String, HolderData>,
    pub(crate) common_infos: BTreeMap<String, bool>,
    pub(crate) custom_data: BTreeMap<String, crate::runtime::NbtCompound>,
    pub(crate) tooltip_displays: BTreeMap<String, TooltipDisplayData>,
    pub(crate) extra_item_tags: BTreeMap<String, Vec<String>>,
    pub(crate) loaded: Option<LoadedRecipeCache>,
    /// Prototype values of jar data component types, by item id, as the game holds them
    /// (`Item.components()`): what the item-part bridge read from the game
    /// (`NativeJar::make_item_stack`). They answer before the runtime's own prototype model.
    pub(crate) game_prototypes: BTreeMap<String, BTreeMap<String, Option<Value>>>,
}

impl RecipeState {
    /// Record the game's prototype values of jar component types for `item_id`
    /// ([`RecipeState::game_prototypes`]).
    pub(crate) fn set_game_prototype(
        &mut self,
        item_id: &str,
        values: BTreeMap<String, Option<Value>>,
    ) {
        self.game_prototypes
            .entry(item_id.to_owned())
            .or_default()
            .extend(values);
    }

    fn next_key(&mut self, prefix: &str) -> String {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        format!("{prefix}/{id}")
    }

    pub(crate) fn alloc_stack(&mut self, stack: LiveStack) -> Value {
        let key = self.next_key("recipe-stack");
        self.stacks.insert(key.clone(), stack);
        Value::Host(HostRef::new(HostKind::ItemStack, key))
    }

    pub(crate) fn alloc_template(&mut self, data: TemplateData) -> Value {
        let key = self.next_key("recipe-template");
        self.templates.insert(key.clone(), data);
        Value::Host(HostRef::new(HostKind::ItemStackTemplate, key))
    }

    pub(crate) fn alloc_patch(&mut self, patch: ComponentPatch) -> Value {
        if patch.is_empty() {
            return empty_patch();
        }
        let key = self.next_key("recipe-patch");
        self.patches.insert(key.clone(), patch);
        Value::Host(HostRef::new(HostKind::DataComponentPatch, key))
    }

    pub(crate) fn alloc_patch_builder(&mut self) -> Value {
        let key = self.next_key("recipe-patch-builder");
        self.patch_builders
            .insert(key.clone(), ComponentPatch::default());
        Value::Host(HostRef::new(HostKind::DataComponentPatchBuilder, key))
    }

    pub(crate) fn alloc_ingredient(&mut self, spec: IngredientSpec) -> Value {
        let key = self.next_key("recipe-ingredient");
        self.ingredients.insert(key.clone(), spec);
        Value::Host(HostRef::new(HostKind::Ingredient, key))
    }

    pub(crate) fn alloc_common_info(&mut self, show_notification: bool) -> Value {
        let key = self.next_key("recipe-common-info");
        self.common_infos.insert(key.clone(), show_notification);
        Value::Host(HostRef::new(HostKind::RecipeCommonInfo, key))
    }

    pub(crate) fn alloc_holder(&mut self, id: Value, value: Value) -> Value {
        let key = self.next_key("recipe-holder");
        self.holders.insert(key.clone(), HolderData { id, value });
        Value::Host(HostRef::new(HostKind::RecipeHolder, key))
    }

    pub(crate) fn alloc_custom_data(&mut self, tag: crate::runtime::NbtCompound) -> Value {
        if tag.is_empty() {
            return empty_custom_data();
        }
        let key = self.next_key("custom-data");
        self.custom_data.insert(key.clone(), tag);
        Value::Host(HostRef::new(HostKind::CustomData, key))
    }

    pub(crate) fn custom_data_of(&self, value: &Value) -> Option<&crate::runtime::NbtCompound> {
        let Value::Host(handle) = value else {
            return None;
        };
        if handle.kind() != HostKind::CustomData {
            return None;
        }
        if handle.key() == EMPTY_CUSTOM_DATA_KEY {
            static EMPTY: std::sync::OnceLock<crate::runtime::NbtCompound> = std::sync::OnceLock::new();
            return Some(EMPTY.get_or_init(crate::runtime::NbtCompound::new));
        }
        self.custom_data.get(handle.key())
    }

    pub(crate) fn alloc_tooltip_display(&mut self, mut data: TooltipDisplayData) -> Value {
        // `TooltipDisplay.hiddenComponents` is a `ReferenceLinkedOpenHashSet`: deduplicate while
        // preserving first-occurrence insertion order.
        let mut deduped = Vec::with_capacity(data.hidden_components.len());
        for id in data.hidden_components {
            if !deduped.iter().any(|existing| existing == &id) {
                deduped.push(id);
            }
        }
        data.hidden_components = deduped;
        let key = self.next_key("td");
        self.tooltip_displays.insert(key.clone(), data);
        Value::Host(HostRef::new(HostKind::TooltipDisplay, key))
    }

    pub(crate) fn tooltip_display_of(&self, value: &Value) -> Option<TooltipDisplayData> {
        let Value::Host(handle) = value else {
            return None;
        };
        if handle.kind() != HostKind::TooltipDisplay {
            return None;
        }
        if handle.key() == TOOLTIP_DISPLAY_CANONICAL_DEFAULT {
            return Some(TooltipDisplayData {
                hide_tooltip: false,
                hidden_components: Vec::new(),
                hidden_known: true,
            });
        }
        self.tooltip_displays.get(handle.key()).cloned()
    }

    pub(crate) fn patch_of(&self, value: &Value) -> Option<&ComponentPatch> {
        let Value::Host(handle) = value else {
            return None;
        };
        if handle.kind() != HostKind::DataComponentPatch {
            return None;
        }
        if handle.key() == EMPTY_PATCH_KEY {
            static EMPTY: std::sync::OnceLock<ComponentPatch> = std::sync::OnceLock::new();
            return Some(EMPTY.get_or_init(ComponentPatch::default));
        }
        self.patches.get(handle.key())
    }

    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        fn push_val<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
            out.extend(crate::runtime::heap::value_texts(value));
        }
        for stack in self.stacks.values() {
            if let Some(item) = &stack.item_value {
                push_val(item, out);
            }
            for val in stack.patch.entries.values().flatten() {
                push_val(val, out);
            }
        }
        for tpl in self.templates.values() {
            for val in tpl.patch.entries.values().flatten() {
                push_val(val, out);
            }
        }
        for values in self.game_prototypes.values() {
            for val in values.values().flatten() {
                push_val(val, out);
            }
        }
        for patch in self.patches.values().chain(self.patch_builders.values()) {
            for val in patch.entries.values().flatten() {
                push_val(val, out);
            }
        }
        for holder in self.holders.values() {
            push_val(&holder.id, out);
            push_val(&holder.value, out);
        }
        if let Some(cache) = &self.loaded {
            for recipe in &cache.recipes {
                push_val(&recipe.recipe_type, out);
                push_val(&recipe.recipe, out);
                push_val(&recipe.holder, out);
            }
        }
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.stacks.len()
            + self.templates.len()
            + self.patches.len()
            + self.patch_builders.len()
            + self.ingredients.len()
            + self.holders.len()
            + self.common_infos.len()
            + self.custom_data.len()
            + self.tooltip_displays.len()
    }
}

pub(crate) fn empty_patch() -> Value {
    Value::Host(HostRef::new(HostKind::DataComponentPatch, EMPTY_PATCH_KEY))
}

pub(crate) fn empty_custom_data() -> Value {
    Value::Host(HostRef::new(HostKind::CustomData, EMPTY_CUSTOM_DATA_KEY))
}

pub(crate) fn unit_instance() -> Value {
    Value::Host(HostRef::new(HostKind::Unit, UNIT_INSTANCE_KEY))
}

pub(crate) fn canonical_default_tooltip_display() -> Value {
    Value::Host(HostRef::new(
        HostKind::TooltipDisplay,
        TOOLTIP_DISPLAY_CANONICAL_DEFAULT,
    ))
}

pub(crate) fn is_empty_stack_handle(value: &Value) -> bool {
    crate::runtime::game_object::handle_key(value) == Some("client/stack/empty")
}

pub(crate) fn is_recipe_object_kind(kind: HostKind) -> bool {
    matches!(
        kind,
        HostKind::ItemStackTemplate
            | HostKind::DataComponentPatch
            | HostKind::DataComponentPatchBuilder
            | HostKind::Ingredient
            | HostKind::RecipeCommonInfo
            | HostKind::RecipeHolder
            | HostKind::CustomData
            | HostKind::Unit
            | HostKind::TooltipDisplay
    )
}

fn fixed_instance_of(
    exact: &'static str,
    supertypes: &'static [&'static str],
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if wanted == "java/lang/Object" || wanted == exact || supertypes.contains(&wanted) {
        return Some(true);
    }
    if hierarchy.is_instance(exact, wanted) == Some(true) {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false) || exact_class_rules_out(exact, wanted))
        .then_some(false)
}

fn identity_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) => handle.same_object(other),
        _ => Some(false),
    })
}

pub(crate) const PATCH_METHODS: Methods = Methods {
    to_string: |handle, _| Ok((handle.key() == EMPTY_PATCH_KEY).then(|| "{}".to_owned())),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| {
        if handle.key() == EMPTY_PATCH_KEY {
            return Ok(match other {
                Value::Host(other)
                    if other.kind() == HostKind::DataComponentPatch
                        && other.key() == EMPTY_PATCH_KEY =>
                {
                    Some(true)
                }
                Value::Host(other) if other.kind() == HostKind::DataComponentPatch => None,
                _ => Some(false),
            });
        }
        Ok(None)
    },
    instance_of: |_, wanted, hierarchy| {
        fixed_instance_of(DATA_COMPONENT_PATCH, &[], wanted, hierarchy)
    },
    crosses_jars: |_, _| false,
    compare_to: None,
};

pub(crate) const PATCH_BUILDER_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: identity_equals,
    instance_of: |_, wanted, hierarchy| {
        fixed_instance_of(DATA_COMPONENT_PATCH_BUILDER, &[], wanted, hierarchy)
    },
    crosses_jars: |_, _| false,
    compare_to: None,
};

pub(crate) const INGREDIENT_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |_, _, _| Ok(None),
    instance_of: |_, wanted, hierarchy| {
        fixed_instance_of(
            INGREDIENT,
            &["java/util/function/Predicate"],
            wanted,
            hierarchy,
        )
    },
    crosses_jars: |_, _| false,
    compare_to: None,
};

pub(crate) const COMMON_INFO_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |_, _, _| Ok(None),
    instance_of: |_, wanted, hierarchy| {
        fixed_instance_of(RECIPE_COMMON_INFO, &["java/lang/Record"], wanted, hierarchy)
    },
    crosses_jars: |_, _| false,
    compare_to: None,
};

pub(crate) const HOLDER_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |_, _, _| Ok(None),
    instance_of: |_, wanted, hierarchy| {
        fixed_instance_of(RECIPE_HOLDER, &["java/lang/Record"], wanted, hierarchy)
    },
    crosses_jars: |_, _| false,
    compare_to: None,
};

pub(crate) const CUSTOM_DATA_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| {
        if handle.key() == EMPTY_CUSTOM_DATA_KEY {
            return Ok(match other {
                Value::Host(other)
                    if other.kind() == HostKind::CustomData
                        && other.key() == EMPTY_CUSTOM_DATA_KEY =>
                {
                    Some(true)
                }
                Value::Host(other) if other.kind() == HostKind::CustomData => None,
                _ => Some(false),
            });
        }
        Ok(None)
    },
    instance_of: |_, wanted, hierarchy| fixed_instance_of(CUSTOM_DATA, &[], wanted, hierarchy),
    crosses_jars: |_, _| false,
    compare_to: None,
};

pub(crate) const UNIT_METHODS: Methods = Methods {
    to_string: |_, _| Ok(Some("INSTANCE".to_owned())),
    hash_code: |_, _| Ok(None),
    equals: identity_equals,
    instance_of: |_, wanted, hierarchy| {
        fixed_instance_of(
            UNIT,
            &[
                "java/lang/Enum",
                "java/lang/Comparable",
                "java/io/Serializable",
            ],
            wanted,
            hierarchy,
        )
    },
    crosses_jars: |_, _| true,
    compare_to: None,
};

pub(crate) fn normalize_id(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let (ns, path) = match raw.split_once(':') {
        Some((ns, path)) => (ns, path),
        None => ("minecraft", raw),
    };
    if ns.is_empty()
        || path.is_empty()
        || !ns.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        })
        || !path.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.' | b'/')
        })
    {
        return None;
    }
    Some(format!("{ns}:{path}"))
}

/// Convert a JSON number to a truncated 64-bit integer matching DFU `JsonOps` + Gson
/// `LazilyParsedNumber` semantics (`1.2` -> `1`, `-1.9` -> `-1`, `1e2` -> `100`, wrapping on
/// narrowing casts to `i32`/`i16`/`i8`). Returns `None` for non-numbers.
pub(crate) fn json_number_to_i64(val: &serde_json::Value) -> Option<i64> {
    let num = val.as_number()?;
    if let Some(i) = num.as_i64() {
        return Some(i);
    }
    if let Some(u) = num.as_u64() {
        return Some(u as i64);
    }
    let f = num.as_f64()?;
    f.is_finite().then_some(f.trunc() as i64)
}

pub(crate) fn is_known_item_id(host: &StdHost, id: &str) -> bool {
    vanilla_item_data::is_vanilla_item(id)
        || host
            .registrations()
            .iter()
            .any(|entry| entry.registry == "minecraft:item" && entry.id == id)
}

pub(crate) fn item_value_for_id(host: &StdHost, id: &str) -> Value {
    if let Some(entry) = host
        .registrations()
        .iter()
        .rev()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == id)
    {
        return entry.value.clone();
    }
    crate::runtime::game_object::server_value(format!("game/item/{id}"))
}

pub(crate) fn item_holder_for_id(host: &StdHost, id: &str) -> Value {
    crate::runtime::host_registry_holder::vanilla("minecraft:item", id, item_value_for_id(host, id))
}

pub(crate) fn resolve_item_id(host: &StdHost, value: &Value) -> Option<String> {
    let Value::Host(handle) = value else {
        return None;
    };
    match handle.kind() {
        HostKind::GameRegistryObject | HostKind::GameValue => {
            if let Some(id) = handle
                .key()
                .strip_prefix("game/item/")
                .or_else(|| handle.key().strip_prefix("client/item/"))
            {
                return Some(id.to_owned());
            }
            if let Some(block_id) = handle
                .key()
                .strip_prefix("game/block/")
                .or_else(|| handle.key().strip_prefix("client/block/"))
            {
                return crate::runtime::vanilla_block_items::item_id(block_id).map(str::to_owned);
            }
            None
        }
        HostKind::RegistryHolder => {
            let held = handle.values()?;
            let reg = crate::runtime::host_ids::registry_name(held.first()?)?;
            let id = crate::runtime::host_ids::identifier_key(held.get(1)?)?.to_owned();
            if reg == "minecraft:item" {
                Some(id)
            } else if reg == "minecraft:block" {
                crate::runtime::vanilla_block_items::item_id(&id).map(str::to_owned)
            } else {
                None
            }
        }
        HostKind::DirectHolder => {
            let inner = handle.values()?.first()?.clone();
            resolve_item_id(host, &inner)
        }
        HostKind::JarObject => {
            let key = handle.key();
            if let Some(field) =
                key.strip_prefix("net/minecraft/world/item/Item#net/minecraft/world/item/Items.")
            {
                return crate::runtime::vanilla_items::field_id(field);
            }
            if let Some(entry) = host.registrations().iter().rev().find(|entry| {
                entry.registry == "minecraft:item"
                    && crate::runtime::jar_object::object_key(&entry.value) == Some(key)
            }) {
                return Some(entry.id.clone());
            }
            if let Some(block_item) = host.block_item_for_block_key(key) {
                return resolve_item_id(host, block_item);
            }
            None
        }
        _ => None,
    }
}

pub(crate) fn tag_key_parts(value: &Value) -> Option<(String, String)> {
    if !crate::runtime::host_ids::is_tag_key(value) {
        return None;
    }
    let reg = crate::runtime::host_ids::registry_name(&crate::runtime::host_ids::key_registry(value)?)?;
    let loc = crate::runtime::host_ids::identifier_key(&crate::runtime::host_ids::key_location(value)?)?.to_owned();
    Some((reg, loc))
}

pub(crate) fn resolve_data_component_type_id(host: &StdHost, value: &Value) -> Option<String> {
    if let Some(key) = crate::runtime::game_object::handle_key(value)
        && let Some(id) = key.strip_prefix("game/data-component-type/")
    {
        return Some(id.to_owned());
    }
    if let Some(entry) =
        host.registrations().iter().rev().find(|entry| {
            entry.registry == "minecraft:data_component_type" && entry.value == *value
        })
    {
        return Some(entry.id.clone());
    }
    None
}

fn base_max_stack_size(host: &StdHost, item_id: &str) -> Option<i32> {
    if let Some(size) = vanilla_item_data::vanilla_max_stack_size(item_id) {
        return Some(size);
    }
    let entry = host
        .registrations()
        .iter()
        .rev()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == item_id)?;
    let marker = expect_ref(&entry.value)?;
    if let Some(props) = host.item_properties_for_marker(marker) {
        if let Some(raw) = props.get("stacksTo") {
            return raw.parse::<i32>().ok();
        }
        if props.contains_key("durability") {
            return Some(1);
        }
        return Some(64);
    }
    // Constructed via `new Item(...)` or `new BlockItem(...)` with no custom properties override.
    host.is_constructed_item_marker(marker).then_some(64)
}

fn base_max_damage(host: &StdHost, item_id: &str) -> Option<Option<i32>> {
    if let Some(dmg) = vanilla_item_data::vanilla_max_damage(item_id) {
        return Some(dmg);
    }
    let entry = host
        .registrations()
        .iter()
        .rev()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == item_id)?;
    let marker = expect_ref(&entry.value)?;
    if let Some(props) = host.item_properties_for_marker(marker) {
        if let Some(raw) = props.get("durability") {
            return raw.parse::<i32>().ok().map(Some);
        }
        return Some(None);
    }
    host.is_constructed_item_marker(marker).then_some(None)
}

/// Exact prototype value of `comp_id` on `item_id`, or `None` when the prototype value is not
/// modelled (callers must count the operation rather than guessing `null`).
pub(crate) fn prototype_component(
    host: &StdHost,
    item_id: &str,
    comp_id: &str,
) -> Option<Option<Value>> {
    if item_id == "minecraft:air" || !is_known_item_id(host, item_id) {
        return None;
    }
    if let Some(value) = host
        .recipes
        .game_prototypes
        .get(item_id)
        .and_then(|values| values.get(comp_id))
    {
        return Some(value.clone());
    }
    match comp_id {
        "minecraft:max_stack_size" => {
            base_max_stack_size(host, item_id).map(|n| Some(Value::Boxed(Boxed::Integer(n))))
        }
        "minecraft:repair_cost" => Some(Some(Value::Boxed(Boxed::Integer(0)))),
        "minecraft:max_damage" => {
            base_max_damage(host, item_id).map(|opt| opt.map(|d| Value::Boxed(Boxed::Integer(d))))
        }
        "minecraft:damage" => {
            base_max_damage(host, item_id).map(|opt| opt.map(|_| Value::Boxed(Boxed::Integer(0))))
        }
        // `DataComponents.COMMON_ITEM_COMPONENTS` sets `TOOLTIP_DISPLAY` to `TooltipDisplay.DEFAULT`
        // across vanilla 26.3 items.
        "minecraft:tooltip_display" => Some(Some(canonical_default_tooltip_display())),
        // Verified empty across all 1,658 vanilla 26.3 item prototypes.
        "minecraft:custom_name"
        | "minecraft:custom_data"
        | "minecraft:custom_model_data"
        | "minecraft:unbreakable"
        | "minecraft:profile" => Some(None),
        _ => {
            // Mod-registered component types default to absent on unconfigured prototypes.
            let is_mod_component = host
                .registrations()
                .iter()
                .any(|e| e.registry == "minecraft:data_component_type" && e.id == comp_id);
            is_mod_component.then_some(None)
        }
    }
}

/// Compare two modelled component values (`Objects.equals(a, b)`) without running arbitrary Java
/// `.equals`. Returns `None` if equality cannot be proven cleanly.
pub(crate) fn host_component_values_equal(host: &StdHost, a: &Value, b: &Value) -> Option<bool> {
    match (a, b) {
        (Value::Ref(None), Value::Ref(None)) => Some(true),
        (Value::Ref(None), _) | (_, Value::Ref(None)) => Some(false),
        (
            Value::Boxed(Boxed::Integer(x)) | Value::Int(x),
            Value::Boxed(Boxed::Integer(y)) | Value::Int(y),
        ) => Some(*x == *y),
        (Value::Boxed(Boxed::Integer(_)) | Value::Int(_), other)
        | (other, Value::Boxed(Boxed::Integer(_)) | Value::Int(_)) => match other {
            Value::Boxed(_) | Value::Long(_) | Value::Float(_) | Value::Ref(Some(_)) => Some(false),
            Value::Host(h) if h.kind() != HostKind::JarObject => Some(false),
            _ => None,
        },
        (Value::Host(ha), Value::Host(hb)) => match (ha.kind(), hb.kind()) {
            (HostKind::Unit, HostKind::Unit) => Some(true),
            (HostKind::Unit, k) | (k, HostKind::Unit) if k != HostKind::JarObject => Some(false),
            (HostKind::CustomData, HostKind::CustomData) => {
                let da = host.recipes.custom_data_of(a)?;
                let db = host.recipes.custom_data_of(b)?;
                Some(da == db)
            }
            (HostKind::CustomData, k) | (k, HostKind::CustomData) if k != HostKind::JarObject => {
                Some(false)
            }
            (HostKind::TooltipDisplay, HostKind::TooltipDisplay) => {
                let da = host.recipes.tooltip_display_of(a)?;
                let db = host.recipes.tooltip_display_of(b)?;
                if da.hide_tooltip != db.hide_tooltip {
                    return Some(false);
                }
                if !da.hidden_known || !db.hidden_known {
                    return None;
                }
                // `TooltipDisplay.hiddenComponents` is a `SequencedSet` (a `LinkedHashSet`):
                // record equality uses `AbstractSet.equals`, which ignores order and duplicates.
                let set_a: BTreeSet<&str> =
                    da.hidden_components.iter().map(String::as_str).collect();
                let set_b: BTreeSet<&str> =
                    db.hidden_components.iter().map(String::as_str).collect();
                Some(set_a == set_b)
            }
            (HostKind::TooltipDisplay, k) | (k, HostKind::TooltipDisplay)
                if k != HostKind::JarObject =>
            {
                Some(false)
            }
            (HostKind::Component, HostKind::Component) => {
                let ja = host.component_json(a)?;
                let jb = host.component_json(b)?;
                Some(ja == jb)
            }
            (HostKind::Component, k) | (k, HostKind::Component) if k != HostKind::JarObject => {
                Some(false)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Compare a candidate patch value with a known prototype value (`Objects.equals(val, proto)`).
/// Returns `None` if equality cannot be determined without running custom Java `.equals`.
fn value_equals_prototype(
    host: &StdHost,
    val: &Option<Value>,
    proto: &Option<Value>,
) -> Option<bool> {
    match (val, proto) {
        (None, None) => Some(true),
        (None, Some(_)) | (Some(_), None) => Some(false),
        (Some(v), Some(p)) => host_component_values_equal(host, v, p),
    }
}

/// Exact `PatchedDataComponentMap.set(type, value)` semantics on a stack's patch.
pub(crate) fn patched_set(
    host: &StdHost,
    item_id: &str,
    patch: &mut ComponentPatch,
    comp_id: &str,
    new_val: &Value,
) -> Option<Value> {
    let proto = prototype_component(host, item_id, comp_id)?;
    let new_opt = (!matches!(new_val, Value::Ref(None))).then(|| new_val.clone());
    let matches_proto = value_equals_prototype(host, &new_opt, &proto)?;
    let old_entry = if matches_proto {
        patch.entries.remove(comp_id)
    } else {
        patch.entries.insert(comp_id.to_owned(), new_opt)
    };
    let proto_val = proto.unwrap_or(Value::Ref(None));
    Some(match old_entry {
        Some(Some(prev)) => prev,
        // `Objects.requireNonNullElse(Removed.removedToNull(old), proto)` -> `proto`.
        Some(None) | None => proto_val,
    })
}

/// Exact `PatchedDataComponentMap.remove(type)` semantics on a stack's patch.
pub(crate) fn patched_remove(
    host: &StdHost,
    item_id: &str,
    patch: &mut ComponentPatch,
    comp_id: &str,
) -> Option<Value> {
    let proto = prototype_component(host, item_id, comp_id)?;
    let old_entry = if proto.is_some() {
        patch.entries.insert(comp_id.to_owned(), None)
    } else {
        patch.entries.remove(comp_id)
    };
    Some(match old_entry {
        Some(Some(prev)) => prev,
        Some(None) => Value::Ref(None),
        None => proto.unwrap_or(Value::Ref(None)),
    })
}

/// Exact `PatchedDataComponentMap.applyPatch(overlay)` semantics: drops entries redundant with the
/// item's prototype and returns `None` (without mutating `patch`) if any overlay key's prototype
/// is unmodelled.
pub(crate) fn patched_apply_patch(
    host: &StdHost,
    item_id: &str,
    patch: &mut ComponentPatch,
    overlay: &ComponentPatch,
) -> Option<()> {
    let mut actions: Vec<(String, Option<Option<Value>>)> =
        Vec::with_capacity(overlay.entries.len());
    for (comp_id, entry) in &overlay.entries {
        let proto = prototype_component(host, item_id, comp_id)?;
        let action = match entry {
            Some(_) => {
                if value_equals_prototype(host, entry, &proto)? {
                    None
                } else {
                    Some(entry.clone())
                }
            }
            None => {
                if proto.is_some() {
                    Some(None)
                } else {
                    None
                }
            }
        };
        actions.push((comp_id.clone(), action));
    }
    for (comp_id, action) in actions {
        match action {
            Some(entry) => {
                patch.entries.insert(comp_id, entry);
            }
            None => {
                patch.entries.remove(&comp_id);
            }
        }
    }
    Some(())
}

pub(crate) fn max_stack_size_for_item(
    host: &StdHost,
    item_id: &str,
    patch: &ComponentPatch,
) -> Option<i32> {
    match patch.entries.get("minecraft:max_stack_size") {
        Some(Some(Value::Int(n))) | Some(Some(Value::Boxed(Boxed::Integer(n)))) => Some(*n),
        Some(Some(_)) => None,
        Some(None) => Some(1),
        None => base_max_stack_size(host, item_id),
    }
}

pub(crate) fn resolve_item_tag(host: &StdHost, raw_tag_id: &str) -> Option<BTreeSet<String>> {
    let stripped = raw_tag_id.strip_prefix('#').unwrap_or(raw_tag_id);
    let tag_id = normalize_id(stripped)?;
    let mut visiting = BTreeSet::new();
    resolve_item_tag_inner(host, &tag_id, &mut visiting)
}

/// Resolve an item tag while counting any unsupported `fabric:resource_load_order` constraint
/// forms present on loaded Fabric mods.
fn resolve_item_tag_checked(
    host: &mut StdHost,
    raw_tag_id: &str,
) -> Result<Option<BTreeSet<String>>, NativeError> {
    let unsupported: Vec<MethodRef> = host
        .ordered_data_packs()
        .unsupported_constraints
        .into_iter()
        .map(|c| c.method_ref())
        .collect();
    for mref in unsupported {
        host.unknown_host_call(&mref)?;
    }
    Ok(resolve_item_tag(host, raw_tag_id))
}

fn resolve_item_tag_inner(
    host: &StdHost,
    tag_id: &str,
    visiting: &mut BTreeSet<String>,
) -> Option<BTreeSet<String>> {
    if !visiting.insert(tag_id.to_owned()) {
        return Some(BTreeSet::new());
    }
    let mut found_any = false;
    let mut entries: Vec<(String, bool)> = Vec::new();
    // Layer 0 (lowest priority): vanilla built-in item tags.
    if let Some(vanilla_entries) = vanilla_item_data::vanilla_item_tag(tag_id) {
        found_any = true;
        for entry in vanilla_entries {
            entries.push(((*entry).to_owned(), true));
        }
    }
    // Stacked datapack/jar layers in vanilla pack order (lowest -> highest priority).
    if let Some((ns, path)) = tag_id.split_once(':') {
        let resource_path = format!("data/{ns}/tags/item/{path}.json");
        for (_, layer) in host.ordered_data_layers() {
            let Some(bytes) = layer.get(&resource_path) else {
                continue;
            };
            let Ok(serde_json::Value::Object(map)) = serde_json::from_slice(bytes) else {
                visiting.remove(tag_id);
                return None;
            };
            found_any = true;
            if map.get("replace").and_then(serde_json::Value::as_bool) == Some(true) {
                entries.clear();
            }
            if let Some(values) = map.get("values").and_then(serde_json::Value::as_array) {
                for item in values {
                    match item {
                        serde_json::Value::String(id) => entries.push((id.clone(), true)),
                        serde_json::Value::Object(obj) => {
                            if let Some(id) = obj.get("id").and_then(serde_json::Value::as_str) {
                                let required = obj
                                    .get("required")
                                    .and_then(serde_json::Value::as_bool)
                                    .unwrap_or(true);
                                entries.push((id.to_owned(), required));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    if let Some(extra) = host.recipes.extra_item_tags.get(tag_id) {
        found_any = true;
        for entry in extra {
            entries.push((entry.clone(), true));
        }
    }
    if !found_any {
        visiting.remove(tag_id);
        return None;
    }
    let mut resolved = BTreeSet::new();
    for (entry, required) in entries {
        if let Some(sub_tag) = entry.strip_prefix('#') {
            let Some(norm_sub) = normalize_id(sub_tag) else {
                if required {
                    visiting.remove(tag_id);
                    return None;
                }
                continue;
            };
            match resolve_item_tag_inner(host, &norm_sub, visiting) {
                Some(sub_items) => resolved.extend(sub_items),
                None if !required => {}
                None => {
                    visiting.remove(tag_id);
                    return None;
                }
            }
        } else {
            let Some(item_id) = normalize_id(&entry) else {
                if required {
                    visiting.remove(tag_id);
                    return None;
                }
                continue;
            };
            if is_known_item_id(host, &item_id) {
                resolved.insert(item_id);
            } else if required {
                visiting.remove(tag_id);
                return None;
            }
        }
    }
    visiting.remove(tag_id);
    Some(resolved)
}

pub(crate) fn decode_component_patch_json(
    host: &mut StdHost,
    value: &serde_json::Value,
) -> Result<ComponentPatch, DecodeError> {
    let Some(obj) = value.as_object() else {
        return Err(DecodeError::Invalid(format!(
            "Not a JSON object for DataComponentPatch: {value}"
        )));
    };
    let mut patch = ComponentPatch::default();
    for (raw_key, val) in obj {
        if let Some(removed) = raw_key.strip_prefix('!') {
            let Some(id) = normalize_id(removed) else {
                return Err(DecodeError::Invalid(format!(
                    "Invalid component id: {raw_key}"
                )));
            };
            patch.entries.insert(id, None);
            continue;
        }
        let Some(id) = normalize_id(raw_key) else {
            return Err(DecodeError::Invalid(format!(
                "Invalid component id: {raw_key}"
            )));
        };
        let decoded = match id.as_str() {
            "minecraft:max_stack_size" => {
                let Some(n) = json_number_to_i64(val).map(|v| v as i32) else {
                    return Err(DecodeError::Invalid(format!(
                        "Not a number for minecraft:max_stack_size: {val}"
                    )));
                };
                if !(1..=99).contains(&n) {
                    return Err(DecodeError::Invalid(format!(
                        "Value must be within range [1;99]: {n}"
                    )));
                }
                Value::Boxed(Boxed::Integer(n))
            }
            "minecraft:max_damage" => {
                let Some(n) = json_number_to_i64(val).map(|v| v as i32) else {
                    return Err(DecodeError::Invalid(format!(
                        "Not a number for minecraft:max_damage: {val}"
                    )));
                };
                if n < 1 {
                    return Err(DecodeError::Invalid(format!(
                        "Value must be positive for minecraft:max_damage: {n}"
                    )));
                }
                Value::Boxed(Boxed::Integer(n))
            }
            "minecraft:damage" | "minecraft:repair_cost" => {
                let Some(n) = json_number_to_i64(val).map(|v| v as i32) else {
                    return Err(DecodeError::Invalid(format!(
                        "Not a number for {id}: {val}"
                    )));
                };
                if n < 0 {
                    return Err(DecodeError::Invalid(format!(
                        "Value must be non-negative for {id}: {n}"
                    )));
                }
                Value::Boxed(Boxed::Integer(n))
            }
            "minecraft:custom_name" | "minecraft:item_name" => match val.as_str() {
                Some(text) => crate::runtime::component_value("literal", text),
                None => {
                    return Err(DecodeError::Unmodelled(format!(
                        "non-string text component JSON for {id}"
                    )));
                }
            },
            _ => {
                let _ = host;
                return Err(DecodeError::Unmodelled(format!(
                    "DataComponentPatch JSON entry for {id}"
                )));
            }
        };
        patch.entries.insert(id, Some(decoded));
    }
    Ok(patch)
}

#[derive(Clone, Debug)]
pub(crate) enum DecodeError {
    Invalid(String),
    Unmodelled(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(msg) | Self::Unmodelled(msg) => f.write_str(msg),
        }
    }
}

pub(crate) fn decode_ingredient_json(
    host: &mut StdHost,
    value: &serde_json::Value,
) -> Result<Value, DecodeError> {
    match value {
        serde_json::Value::String(text) => {
            if let Some(tag_raw) = text.strip_prefix('#') {
                let Some(tag_id) = normalize_id(tag_raw) else {
                    return Err(DecodeError::Invalid(format!(
                        "Invalid tag identifier: {text}"
                    )));
                };
                if resolve_item_tag(host, &tag_id).is_none() {
                    return Err(DecodeError::Invalid(format!(
                        "Missing tag: '{tag_id}' in 'minecraft:item'"
                    )));
                }
                Ok(host.recipes.alloc_ingredient(IngredientSpec::Tag(tag_id)))
            } else {
                let id = decode_non_air_item_id(host, text)?;
                Ok(host
                    .recipes
                    .alloc_ingredient(IngredientSpec::Items(vec![id])))
            }
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return Err(DecodeError::Invalid("List must have contents".into()));
            }
            let mut ids = Vec::with_capacity(items.len());
            for elem in items {
                let Some(text) = elem.as_str() else {
                    return Err(DecodeError::Invalid(format!(
                        "Ingredient list element is not a string: {elem}"
                    )));
                };
                if text.starts_with('#') {
                    return Err(DecodeError::Invalid(format!(
                        "Tag not allowed inside ingredient list: {text}"
                    )));
                }
                ids.push(decode_non_air_item_id(host, text)?);
            }
            Ok(host.recipes.alloc_ingredient(IngredientSpec::Items(ids)))
        }
        _ => Err(DecodeError::Invalid(format!(
            "Not a valid Ingredient JSON value: {value}"
        ))),
    }
}

pub(crate) fn decode_non_air_item_id(host: &StdHost, raw: &str) -> Result<String, DecodeError> {
    let Some(id) = normalize_id(raw) else {
        return Err(DecodeError::Invalid(format!(
            "Invalid item identifier: {raw}"
        )));
    };
    if id == "minecraft:air" {
        return Err(DecodeError::Invalid(
            "Item must not be minecraft:air".into(),
        ));
    }
    if !is_known_item_id(host, &id) {
        return Err(DecodeError::Invalid(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:item]: {id}"
        )));
    }
    Ok(id)
}

pub(crate) fn format_json_map(map: &serde_json::Map<String, serde_json::Value>) -> String {
    serde_json::to_string(&serde_json::Value::Object(map.clone()))
        .unwrap_or_else(|_| "{}".to_owned())
}

pub(crate) fn decode_item_stack_template_map(
    host: &mut StdHost,
    map: &serde_json::Map<String, serde_json::Value>,
) -> crate::runtime::dfu_codec::DfuCodecResult<Value> {
    use crate::runtime::dfu_codec::DfuCodecResult;
    // Evaluate all three RecordCodecBuilder fields (`id`, `count`, `components`) in field order
    // and accumulate errors just like DFU's `ItemStackTemplate.MAP_CODEC`. `count` and
    // `components` are `optionalFieldOf` with defaults `1` and `EMPTY`, so they always produce
    // a partial value even on error; `id` is required, so `ItemStackTemplate` produces a partial
    // whenever `id` succeeds.
    let mut invalid_errors = Vec::new();
    let mut first_unmodelled: Option<String> = None;

    let item_id = match map.get("id").filter(|v| !v.is_null()) {
        None => {
            invalid_errors.push(format!("No key id in MapLike[{}]", format_json_map(map)));
            None
        }
        Some(v) => match v.as_str() {
            Some(raw_id) => match decode_non_air_item_id(host, raw_id) {
                Ok(id) => Some(id),
                Err(DecodeError::Invalid(msg)) => {
                    invalid_errors.push(msg);
                    None
                }
                Err(DecodeError::Unmodelled(msg)) => {
                    if first_unmodelled.is_none() {
                        first_unmodelled = Some(msg);
                    }
                    None
                }
            },
            None => {
                invalid_errors.push(format!("Not a string: {v}"));
                None
            }
        },
    };

    // `ExtraCodecs.intRange(1, 99).optionalFieldOf("count", 1)`: JSON `null` = absent/default,
    // fractional JSON numbers truncate toward zero before range checking, and decode errors
    // keep partial value `1`.
    let count = match map.get("count").filter(|v| !v.is_null()) {
        None => 1,
        Some(v) => match json_number_to_i64(v).map(|n| n as i32) {
            None => {
                invalid_errors.push(format!("Not a number: {v}"));
                1
            }
            Some(n) if !(1..=99).contains(&n) => {
                invalid_errors.push(format!("Value must be within range [1;99]: {n}"));
                1
            }
            Some(n) => n,
        },
    };

    let patch = match map.get("components").filter(|v| !v.is_null()) {
        None => ComponentPatch::default(),
        Some(v) => match decode_component_patch_json(host, v) {
            Ok(p) => p,
            Err(DecodeError::Invalid(msg)) => {
                invalid_errors.push(msg);
                ComponentPatch::default()
            }
            Err(DecodeError::Unmodelled(msg)) => {
                if first_unmodelled.is_none() {
                    first_unmodelled = Some(msg);
                }
                ComponentPatch::default()
            }
        },
    };

    if let Some(msg) = first_unmodelled {
        return DfuCodecResult::Unmodelled(msg);
    }
    if !invalid_errors.is_empty() {
        // DFU's `RecordCodecBuilder` (`ap3`) combines error messages in reverse field order.
        let message = invalid_errors
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("; ");
        let partial = item_id.map(|id| {
            host.recipes.alloc_template(TemplateData {
                item_id: id,
                count,
                patch,
            })
        });
        return DfuCodecResult::Error { message, partial };
    }
    DfuCodecResult::Ok(host.recipes.alloc_template(TemplateData {
        item_id: item_id.unwrap(),
        count,
        patch,
    }))
}

pub(crate) fn decode_item_stack_template_json(
    host: &mut StdHost,
    value: &serde_json::Value,
) -> crate::runtime::dfu_codec::DfuCodecResult<Value> {
    use crate::runtime::dfu_codec::DfuCodecResult;
    // `ItemStackTemplate.CODEC = Codec.withAlternative(MAP_CODEC.codec(), Item.CODEC, ItemStackTemplate::new)`
    // (`EitherCodec`): try object form first, then string item-id form; on dual error prefer a
    // partial from first (then second), else format DFU's combined `EitherCodec` message.
    let first = match value {
        serde_json::Value::Object(map) => decode_item_stack_template_map(host, map),
        _ => DfuCodecResult::Error {
            message: format!("Not a JSON object: {value}"),
            partial: None,
        },
    };
    let (first_err, first_partial) = match first {
        DfuCodecResult::Ok(v) => return DfuCodecResult::Ok(v),
        DfuCodecResult::Unmodelled(u) => return DfuCodecResult::Unmodelled(u),
        DfuCodecResult::Error { message, partial } => (message, partial),
    };
    let second = match value {
        serde_json::Value::String(raw_id) => match decode_non_air_item_id(host, raw_id) {
            Ok(item_id) => DfuCodecResult::Ok(host.recipes.alloc_template(TemplateData {
                item_id,
                count: 1,
                patch: ComponentPatch::default(),
            })),
            Err(DecodeError::Invalid(message)) => DfuCodecResult::Error {
                message,
                partial: None,
            },
            Err(DecodeError::Unmodelled(u)) => DfuCodecResult::Unmodelled(u),
        },
        _ => DfuCodecResult::Error {
            message: format!("Not a string: {value}"),
            partial: None,
        },
    };
    let (second_err, second_partial) = match second {
        DfuCodecResult::Ok(v) => return DfuCodecResult::Ok(v),
        DfuCodecResult::Unmodelled(u) => return DfuCodecResult::Unmodelled(u),
        DfuCodecResult::Error { message, partial } => (message, partial),
    };
    if first_partial.is_some() {
        return DfuCodecResult::Error {
            message: first_err,
            partial: first_partial,
        };
    }
    if second_partial.is_some() {
        return DfuCodecResult::Error {
            message: second_err,
            partial: second_partial,
        };
    }
    DfuCodecResult::Error {
        message: format!("Failed to parse either. First: {first_err}; Second: {second_err}"),
        partial: None,
    }
}

pub(crate) fn decode_common_info_map(
    host: &mut StdHost,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<Value, DecodeError> {
    // `Codec.BOOL.optionalFieldOf("show_notification", true)`: JSON `null` = absent/default.
    let show_notification = match map.get("show_notification").filter(|v| !v.is_null()) {
        None => true,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(other) => {
            return Err(DecodeError::Invalid(format!(
                "Not a boolean for show_notification: {other}"
            )));
        }
    };
    Ok(host.recipes.alloc_common_info(show_notification))
}

/// Exact MC 26.3 `ItemStackTemplate.apply(int count, DataComponentPatch basePatch)`:
/// constructs `new ItemStack(item, count, basePatch)`, calls `applyComponents(tpl.components)`,
/// then runs `ItemStack.validateStrict(stack)`:
/// - Empty stacks (`count <= 0` or `air`) pass `validateStrict` (their effective components are
///   `DataComponentMap.EMPTY`, `getCount() == 0 <= getMaxStackSize() == 1`) and are returned as
///   the newly constructed (empty) `ItemStack` instance, NOT replaced with `ItemStack.EMPTY`.
/// - Non-empty stacks fail validation and are replaced with `ItemStack.EMPTY` if they are both
///   damageable (`has(MAX_DAMAGE)`) and stackable (`getMaxStackSize() > 1`), or if
///   `count > getMaxStackSize()`.
/// - Stacks patched with unmodelled nested-item components (`container`, `bundle_contents`,
///   `charged_projectiles`) or unknown prototypes return `None` so the call is counted.
pub(crate) fn template_apply(
    host: &mut StdHost,
    template_key: &str,
    count: i32,
    base_patch: &ComponentPatch,
) -> Option<Value> {
    let tpl = host.recipes.templates.get(template_key)?.clone();
    let mut patch = ComponentPatch::default();
    if tpl.item_id != "minecraft:air" {
        patched_apply_patch(host, &tpl.item_id, &mut patch, base_patch)?;
        patched_apply_patch(host, &tpl.item_id, &mut patch, &tpl.patch)?;
    }
    let stack = LiveStack::from_item_id(tpl.item_id, count, patch);
    if stack.is_empty() {
        return Some(host.recipes.alloc_stack(stack));
    }
    let item_id = stack.item_id.as_deref().unwrap_or("minecraft:air");
    // `ItemStack.validateComponents` validates nested item sizes on `CONTAINER`,
    // `BUNDLE_CONTENTS`, and `CHARGED_PROJECTILES` when present; default prototypes of those
    // items are empty, but patched values are not modelled here -> count rather than guess.
    for nested_comp in [
        "minecraft:container",
        "minecraft:bundle_contents",
        "minecraft:charged_projectiles",
    ] {
        if matches!(stack.patch.entries.get(nested_comp), Some(Some(_))) {
            return None;
        }
    }
    let has_max_damage = !matches!(
        stack_component_get(host, item_id, &stack.patch, "minecraft:max_damage")?,
        Value::Ref(None)
    );
    let max_stack = max_stack_size_for_item(host, item_id, &stack.patch)?;
    if (has_max_damage && max_stack > 1) || stack.count > max_stack {
        return Some(crate::runtime::menu_models::empty_stack());
    }
    Some(host.recipes.alloc_stack(stack))
}

pub(crate) fn read_live_stack_opt(host: &StdHost, stack: &Value) -> Option<LiveStack> {
    let Value::Host(handle) = stack else {
        return None;
    };
    if handle.kind() != HostKind::ItemStack {
        return None;
    }
    if let Some(live) = host.recipes.stacks.get(handle.key()) {
        return Some(live.clone());
    }
    crate::runtime::host_creative_tab::host_item_stack_data(stack)
        .map(|data| LiveStack::from_item_value(host, data.item, data.count))
}

pub(crate) fn same_item_value(
    host: &StdHost,
    left_item: &Value,
    left_id: Option<&str>,
    right: &Value,
) -> Option<bool> {
    if matches!(right, Value::Ref(None)) {
        return Some(false);
    }
    match (left_item, right) {
        (Value::Host(a), Value::Host(b)) if a.same_object(b) == Some(true) => {
            return Some(true);
        }
        _ if left_item == right => return Some(true),
        _ => {}
    }
    let left_id = left_id
        .map(str::to_owned)
        .or_else(|| resolve_item_id(host, left_item));
    let right_id = resolve_item_id(host, right);
    if let (Some(a), Some(b)) = (left_id, right_id) {
        return Some(a == b);
    }
    match (left_item, right) {
        (Value::Host(a), Value::Host(b)) => a.same_object(b),
        _ => Some(false),
    }
}

pub(crate) fn stack_patch_clone(host: &StdHost, stack: &Value) -> Option<ComponentPatch> {
    if is_empty_stack_handle(stack) {
        return Some(ComponentPatch::default());
    }
    let live = read_live_stack_opt(host, stack)?;
    if live.is_empty() {
        return Some(ComponentPatch::default());
    }
    Some(live.patch)
}

pub(crate) fn stack_is_empty(host: &StdHost, stack: &Value) -> Option<bool> {
    if is_empty_stack_handle(stack) {
        return Some(true);
    }
    Some(read_live_stack_opt(host, stack)?.is_empty())
}

pub(crate) fn stack_max_stack_size(host: &StdHost, stack: &Value) -> Option<i32> {
    if is_empty_stack_handle(stack) {
        return Some(1);
    }
    let live = read_live_stack_opt(host, stack)?;
    if live.is_empty() {
        return Some(1);
    }
    let item_id = live.resolved_item_id(host)?;
    max_stack_size_for_item(host, &item_id, &live.patch)
}

pub(crate) fn limit_stack_exact(host: &mut StdHost, stack: &Value, limit: i32) -> Option<()> {
    if is_empty_stack_handle(stack) {
        return Some(());
    }
    let Value::Host(handle) = stack else {
        return None;
    };
    if handle.kind() != HostKind::ItemStack {
        return None;
    }
    let key = handle.key().to_owned();
    let live = host.recipes.stacks.get_mut(&key)?;
    if !live.is_empty() && live.count > limit {
        live.count = limit;
    }
    Some(())
}

pub(crate) fn limit_stack_for_container(
    host: &mut StdHost,
    stack: &Value,
    container_max: i32,
) -> Option<()> {
    let item_max = stack_max_stack_size(host, stack)?;
    let limit = container_max.min(item_max);
    limit_stack_exact(host, stack, limit)
}

pub(crate) fn random_new(host: &mut StdHost, class: &str) -> Option<Value> {
    match class {
        ITEM_STACK if !host.in_client_frame() => Some(host.recipes.alloc_stack(
            LiveStack::from_item_id("minecraft:air".into(), 0, ComponentPatch::default()),
        )),
        "net/minecraft/world/item/ItemStackTemplate" => {
            Some(host.recipes.alloc_template(TemplateData {
                item_id: "minecraft:air".into(),
                count: 0,
                patch: ComponentPatch::default(),
            }))
        }
        RECIPE_COMMON_INFO => Some(host.recipes.alloc_common_info(true)),
        RECIPE_HOLDER => Some(
            host.recipes
                .alloc_holder(Value::Ref(None), Value::Ref(None)),
        ),
        _ => None,
    }
}

pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (DATA_COMPONENT_PATCH, "EMPTY", "Lnet/minecraft/core/component/DataComponentPatch;") => {
            Some(empty_patch())
        }
        (CUSTOM_DATA, "EMPTY", "Lnet/minecraft/world/item/component/CustomData;") => {
            Some(empty_custom_data())
        }
        (UNIT, "INSTANCE", "Lnet/minecraft/util/Unit;") => Some(unit_instance()),
        (DATA_COMPONENTS, field, "Lnet/minecraft/core/component/DataComponentType;") => {
            vanilla_item_data::data_component_field_id(field).map(|id| {
                crate::runtime::game_object::server_value(format!("game/data-component-type/{id}"))
            })
        }
        _ => None,
    }
}

fn npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

fn unsupported(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/UnsupportedOperationException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn illegal_state(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalStateException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

pub(crate) fn ingredient_test(
    host: &StdHost,
    spec: &IngredientSpec,
    stack: &Value,
) -> Option<bool> {
    if is_empty_stack_handle(stack) {
        return Some(false);
    }
    let live = read_live_stack_opt(host, stack)?;
    if live.is_empty() {
        return Some(false);
    }
    let item_id = live.resolved_item_id(host)?;
    Some(match spec {
        IngredientSpec::Items(ids) => ids.iter().any(|id| id == &item_id),
        IngredientSpec::Tag(tag_id) => {
            resolve_item_tag(host, tag_id).is_some_and(|members| members.contains(&item_id))
        }
    })
}

pub(crate) fn static_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (
            ITEM_STACK,
            "isSameItemSameComponents",
            "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/item/ItemStack;)Z",
        ) => {
            let [a, b] = args else {
                return None;
            };
            match stacks_same_item_same_components(host, a, b) {
                Ok(Some(same)) => Some(Ok(Value::Int(i32::from(same)))),
                Ok(None) => None,
                Err(err) => Some(Err(err)),
            }
        }
        (
            DATA_COMPONENT_PATCH,
            "builder",
            "()Lnet/minecraft/core/component/DataComponentPatch$Builder;",
        ) => Some(Ok(host.recipes.alloc_patch_builder())),
        (
            INGREDIENT,
            "of",
            "(Lnet/minecraft/world/level/ItemLike;)Lnet/minecraft/world/item/crafting/Ingredient;",
        ) => {
            let arg = args.first()?;
            if matches!(arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let item_id = resolve_item_id(host, arg)?;
            if item_id == "minecraft:air" {
                return Some(Err(unsupported("Ingredient can't contain air")));
            }
            Some(Ok(host
                .recipes
                .alloc_ingredient(IngredientSpec::Items(vec![item_id]))))
        }
        (
            INGREDIENT,
            "testOptionalIngredient",
            "(Ljava/util/Optional;Lnet/minecraft/world/item/ItemStack;)Z",
        ) => {
            let opt = args.first()?;
            let stack = args.get(1)?;
            if matches!(opt, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let Value::Host(opt_handle) = opt else {
                return None;
            };
            if opt_handle.kind() != HostKind::Optional {
                return None;
            }
            let held = opt_handle.values()?;
            if held.is_empty() {
                return stack_is_empty(host, stack).map(|empty| Ok(Value::Int(i32::from(empty))));
            }
            let Value::Host(ing_handle) = &held[0] else {
                return None;
            };
            if ing_handle.kind() != HostKind::Ingredient {
                return None;
            }
            let spec = host.recipes.ingredients.get(ing_handle.key())?;
            ingredient_test(host, spec, stack).map(|matched| Ok(Value::Int(i32::from(matched))))
        }
        (
            TRANSMUTE_RECIPE,
            "createWithOriginalComponents",
            "(Lnet/minecraft/world/item/ItemStackTemplate;Lnet/minecraft/world/item/ItemStack;)Lnet/minecraft/world/item/ItemStack;",
        ) => {
            let tpl = args.first()?;
            let base = args.get(1)?;
            if matches!(tpl, Value::Ref(None)) || matches!(base, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let Value::Host(tpl_handle) = tpl else {
                return None;
            };
            if tpl_handle.kind() != HostKind::ItemStackTemplate {
                return None;
            }
            let count = host.recipes.templates.get(tpl_handle.key())?.count;
            let base_patch = stack_patch_clone(host, base)?;
            template_apply(host, tpl_handle.key(), count, &base_patch).map(Ok)
        }
        (
            TRANSMUTE_RECIPE,
            "createWithOriginalComponents",
            "(Lnet/minecraft/world/item/ItemStackTemplate;Lnet/minecraft/world/item/ItemStack;I)Lnet/minecraft/world/item/ItemStack;",
        ) => {
            let tpl = args.first()?;
            let base = args.get(1)?;
            let extra = match args.get(2) {
                Some(Value::Int(n)) => *n,
                _ => return None,
            };
            if matches!(tpl, Value::Ref(None)) || matches!(base, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let Value::Host(tpl_handle) = tpl else {
                return None;
            };
            if tpl_handle.kind() != HostKind::ItemStackTemplate {
                return None;
            }
            let count = host
                .recipes
                .templates
                .get(tpl_handle.key())?
                .count
                .wrapping_add(extra);
            let base_patch = stack_patch_clone(host, base)?;
            template_apply(host, tpl_handle.key(), count, &base_patch).map(Ok)
        }
        _ => None,
    }
}

pub(crate) fn instance_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    // Item.asItem() on known server item handles returns `this`.
    if !host.in_client_frame()
        && matches!(
            r.owner.as_str(),
            "net/minecraft/world/item/Item" | "net/minecraft/world/level/ItemLike"
        )
        && r.name == "asItem"
        && r.descriptor == "()Lnet/minecraft/world/item/Item;"
        && crate::runtime::game_object::handle_key(receiver).is_some_and(|key| key.starts_with("game/item/"))
    {
        return Some(Ok(receiver.clone()));
    }

    // Item.builtInRegistryHolder() on known items.
    if r.owner == "net/minecraft/world/item/Item"
        && r.name == "builtInRegistryHolder"
        && r.descriptor == "()Lnet/minecraft/core/Holder$Reference;"
        && let Some(item_id) = resolve_item_id(host, receiver)
    {
        return Some(Ok(item_holder_for_id(host, &item_id)));
    }

    // ItemStack.EMPTY outside a client frame.
    if !host.in_client_frame()
        && is_empty_stack_handle(receiver)
        && matches!(
            r.owner.as_str(),
            ITEM_STACK | ITEM_INSTANCE | DATA_COMPONENT_HOLDER | DATA_COMPONENT_GETTER
        )
    {
        return stack_instance_call_empty(host, r, args);
    }

    let Value::Host(handle) = receiver else {
        return None;
    };
    match handle.kind() {
        HostKind::ItemStack if !host.in_client_frame() => {
            live_stack_instance_call(host, handle, r, args)
        }
        HostKind::DataComponentPatch => patch_instance_call(host, handle, r, args),
        HostKind::DataComponentPatchBuilder => patch_builder_instance_call(host, handle, r, args),
        HostKind::Ingredient => ingredient_instance_call(host, handle, r, args),
        HostKind::RecipeCommonInfo => common_info_instance_call(host, handle, r, args),
        HostKind::RecipeHolder => holder_instance_call(host, handle, r, args),
        HostKind::CustomData => custom_data_instance_call(host, handle, r, args),
        _ => None,
    }
}

fn custom_data_instance_call(
    host: &mut StdHost,
    handle: &HostRef,
    r: &MethodRef,
    _args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != CUSTOM_DATA {
        return None;
    }
    let tag = host.recipes.custom_data_of(&Value::Host(handle.clone()))?;
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("isEmpty", "()Z") => Some(Ok(Value::Int(i32::from(tag.is_empty())))),
        _ => None,
    }
}

fn stack_instance_call_empty(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("isEmpty", "()Z") => Some(Ok(Value::Int(1))),
        ("getCount" | "count", "()I") => Some(Ok(Value::Int(0))),
        ("setCount" | "grow" | "shrink" | "limitSize", "(I)V") => Some(Ok(Value::Void)),
        // MC 26.3: ItemInstance.getMaxStackSize() = getOrDefault(MAX_STACK_SIZE, 1); EMPTY has empty map -> 1.
        ("getMaxStackSize", "()I") => Some(Ok(Value::Int(1))),
        ("getItem", "()Lnet/minecraft/world/item/Item;") => {
            Some(Ok(crate::runtime::host_creative_tab::air_item_value()))
        }
        ("typeHolder" | "getItemHolder", "()Lnet/minecraft/core/Holder;") => {
            Some(Ok(item_holder_for_id(host, "minecraft:air")))
        }
        ("is", "(Lnet/minecraft/world/item/Item;)Z") => {
            let air = crate::runtime::host_creative_tab::air_item_value();
            let same = same_item_value(host, &air, Some("minecraft:air"), args.first()?)?;
            Some(Ok(Value::Int(i32::from(same))))
        }
        ("is", "(Lnet/minecraft/core/Holder;)Z") => {
            let arg = args.first()?;
            if matches!(arg, Value::Ref(None)) {
                return Some(Ok(Value::Int(0)));
            }
            let item_id = resolve_item_id(host, arg)?;
            Some(Ok(Value::Int(i32::from(item_id == "minecraft:air"))))
        }
        ("is", "(Lnet/minecraft/tags/TagKey;)Z") => Some(Ok(Value::Int(0))),
        ("getComponentsPatch", "()Lnet/minecraft/core/component/DataComponentPatch;") => {
            Some(Ok(empty_patch()))
        }
        (
            "get" | "remove",
            "(Lnet/minecraft/core/component/DataComponentType;)Ljava/lang/Object;",
        ) => Some(Ok(Value::Ref(None))),
        (
            "getOrDefault",
            "(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Ljava/lang/Object;",
        ) => Some(Ok(args.get(1)?.clone())),
        ("getDamageValue" | "getMaxDamage", "()I") => Some(Ok(Value::Int(0))),
        ("isDamageableItem" | "isDamaged", "()Z") => Some(Ok(Value::Int(0))),
        (
            "set",
            "(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Ljava/lang/Object;",
        ) => Some(Ok(Value::Ref(None))),
        ("has", "(Lnet/minecraft/core/component/DataComponentType;)Z") => Some(Ok(Value::Int(0))),
        ("applyComponents", "(Lnet/minecraft/core/component/DataComponentPatch;)V") => {
            Some(Ok(Value::Void))
        }
        ("copy", "()Lnet/minecraft/world/item/ItemStack;")
        | ("copyWithCount", "(I)Lnet/minecraft/world/item/ItemStack;")
        | ("split", "(I)Lnet/minecraft/world/item/ItemStack;")
        | ("copyAndClear", "()Lnet/minecraft/world/item/ItemStack;") => {
            Some(Ok(crate::runtime::menu_models::empty_stack()))
        }
        // `getMaxStackSize()` is 1, so `isStackable()` is false.
        ("isStackable", "()Z") => Some(Ok(Value::Int(0))),
        _ => None,
    }
}

fn live_stack_instance_call(
    host: &mut StdHost,
    handle: &HostRef,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let key = handle.key().to_owned();
    if r.owner == ITEM_STACK && r.name == "<init>" {
        match r.descriptor.as_str() {
            "(Lnet/minecraft/world/level/ItemLike;)V"
            | "(Lnet/minecraft/world/level/ItemLike;I)V" => {
                let count = if r.descriptor == "(Lnet/minecraft/world/level/ItemLike;)V" {
                    1
                } else {
                    match expect_int(r, args.get(1)?) {
                        Ok(c) => c,
                        Err(e) => return Some(Err(e)),
                    }
                };
                let item_arg = args.first()?;
                if matches!(item_arg, Value::Ref(None)) {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/NullPointerException".into(),
                        message: Some(
                            "Cannot invoke \"net.minecraft.world.level.ItemLike.asItem()\" because \"item\" is null"
                                .into(),
                        ),
                        frame: None,
                    }));
                }
                let item_val = host.resolve_host_item_like_impl(item_arg);
                let item_id = resolve_item_id(host, item_val.as_ref().unwrap_or(item_arg));
                if item_val.is_none() && item_id.is_none() {
                    return None;
                }
                host.recipes.stacks.insert(
                    key,
                    LiveStack {
                        item_value: item_val,
                        item_id,
                        count,
                        patch: ComponentPatch::default(),
                        opaque_components: false,
                        opaque_keys: BTreeSet::new(),
                    },
                );
                return Some(Ok(Value::Void));
            }
            "(Lnet/minecraft/core/Holder;)V"
            | "(Lnet/minecraft/core/Holder;I)V"
            | "(Lnet/minecraft/core/Holder;ILnet/minecraft/core/component/DataComponentPatch;)V" => {
                let count = if r.descriptor == "(Lnet/minecraft/core/Holder;)V" {
                    1
                } else {
                    match expect_int(r, args.get(1)?) {
                        Ok(c) => c,
                        Err(e) => return Some(Err(e)),
                    }
                };
                let patch = if r.descriptor
                    == "(Lnet/minecraft/core/Holder;ILnet/minecraft/core/component/DataComponentPatch;)V"
                {
                    let patch_arg = args.get(2)?;
                    if matches!(patch_arg, Value::Ref(None)) {
                        return Some(Err(npe()));
                    }
                    host.recipes.patch_of(patch_arg)?.clone()
                } else {
                    ComponentPatch::default()
                };
                let item_arg = args.first()?;
                if matches!(item_arg, Value::Ref(None)) {
                    return Some(Err(npe()));
                }
                let item_id = resolve_item_id(host, item_arg)?;
                let mut sanitized = ComponentPatch::default();
                if item_id != "minecraft:air" && !patch.is_empty() {
                    patched_apply_patch(host, &item_id, &mut sanitized, &patch)?;
                }
                host.recipes
                    .stacks
                    .insert(key, LiveStack::from_item_id(item_id, count, sanitized));
                return Some(Ok(Value::Void));
            }
            _ => return None,
        }
    }

    if !matches!(
        r.owner.as_str(),
        ITEM_STACK | ITEM_INSTANCE | DATA_COMPONENT_HOLDER | DATA_COMPONENT_GETTER
    ) {
        return None;
    }

    let live = read_live_stack_opt(host, &Value::Host(handle.clone()))?;
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("isEmpty", "()Z") => Some(Ok(Value::Int(i32::from(live.is_empty())))),
        ("getCount" | "count", "()I") => Some(Ok(Value::Int(live.effective_count()))),
        ("setCount", "(I)V") => {
            let count = match expect_int(r, args.first()?) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };
            let slot = host.recipes.stacks.get_mut(&key)?;
            slot.count = count;
            Some(Ok(Value::Void))
        }
        ("grow", "(I)V") => {
            let delta = match expect_int(r, args.first()?) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };
            let slot = host.recipes.stacks.get_mut(&key)?;
            slot.count = slot.effective_count().wrapping_add(delta);
            Some(Ok(Value::Void))
        }
        ("shrink", "(I)V") => {
            let delta = match expect_int(r, args.first()?) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };
            let slot = host.recipes.stacks.get_mut(&key)?;
            slot.count = slot.effective_count().wrapping_sub(delta);
            Some(Ok(Value::Void))
        }
        ("limitSize", "(I)V") => {
            let max = match expect_int(r, args.first()?) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };
            let slot = host.recipes.stacks.get_mut(&key)?;
            if !slot.is_empty() && slot.count > max {
                slot.count = max;
            }
            Some(Ok(Value::Void))
        }
        ("getMaxStackSize", "()I") => {
            if live.is_empty() {
                return Some(Ok(Value::Int(1)));
            }
            let item_id = live.resolved_item_id(host)?;
            let max = max_stack_size_for_item(host, &item_id, &live.patch)?;
            Some(Ok(Value::Int(max)))
        }
        ("getItem", "()Lnet/minecraft/world/item/Item;") => {
            Some(Ok(live.effective_item_value(host)))
        }
        ("typeHolder" | "getItemHolder", "()Lnet/minecraft/core/Holder;") => {
            let item_id = live.resolved_item_id(host)?;
            Some(Ok(item_holder_for_id(host, &item_id)))
        }
        ("is", "(Lnet/minecraft/world/item/Item;)Z") => {
            let arg = args.first()?;
            let item_val = live.effective_item_value(host);
            let item_id = live.resolved_item_id(host);
            let same = same_item_value(host, &item_val, item_id.as_deref(), arg)?;
            Some(Ok(Value::Int(i32::from(same))))
        }
        ("is", "(Lnet/minecraft/core/Holder;)Z") => {
            let arg = args.first()?;
            if matches!(arg, Value::Ref(None)) {
                return Some(Ok(Value::Int(0)));
            }
            let other_id = resolve_item_id(host, arg)?;
            let my_id = live.resolved_item_id(host)?;
            Some(Ok(Value::Int(i32::from(my_id == other_id))))
        }
        ("is", "(Lnet/minecraft/tags/TagKey;)Z") => {
            if live.is_empty() {
                return Some(Ok(Value::Int(0)));
            }
            let item_id = live.resolved_item_id(host)?;
            let arg = args.first()?;
            let (reg, loc) = tag_key_parts(arg)?;
            if reg != "minecraft:item" {
                return Some(Ok(Value::Int(0)));
            }
            match resolve_item_tag_checked(host, &loc) {
                Ok(members) => {
                    let matched = members.is_some_and(|set| set.contains(&item_id));
                    Some(Ok(Value::Int(i32::from(matched))))
                }
                Err(err) => Some(Err(err)),
            }
        }
        ("getComponentsPatch", "()Lnet/minecraft/core/component/DataComponentPatch;") => {
            if live.is_empty() {
                Some(Ok(empty_patch()))
            } else if live.has_opaque_components() {
                None
            } else if live.patch.is_empty() {
                Some(Ok(empty_patch()))
            } else {
                Some(Ok(host.recipes.alloc_patch(live.patch)))
            }
        }
        (
            "set",
            "(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Ljava/lang/Object;",
        ) => {
            let type_arg = args.first()?;
            let val_arg = args.get(1)?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let item_id = live.resolved_item_id(host)?;
            let comp_id = resolve_data_component_type_id(host, type_arg)?;
            if live.is_key_opaque(&comp_id) {
                return None;
            }
            let mut next_patch = live.patch.clone();
            let prev = patched_set(host, &item_id, &mut next_patch, &comp_id, val_arg)?;
            let slot = host.recipes.stacks.get_mut(&key)?;
            slot.patch = next_patch;
            Some(Ok(prev))
        }
        ("get", "(Lnet/minecraft/core/component/DataComponentType;)Ljava/lang/Object;") => {
            if live.is_empty() {
                return Some(Ok(Value::Ref(None)));
            }
            let item_id = live.resolved_item_id(host)?;
            let type_arg = args.first()?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let comp_id = resolve_data_component_type_id(host, type_arg)?;
            if live.is_key_opaque(&comp_id) {
                return None;
            }
            stack_component_get(host, &item_id, &live.patch, &comp_id).map(Ok)
        }
        (
            "getOrDefault",
            "(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Ljava/lang/Object;",
        ) => {
            let default_arg = args.get(1)?;
            if live.is_empty() {
                return Some(Ok(default_arg.clone()));
            }
            let item_id = live.resolved_item_id(host)?;
            let type_arg = args.first()?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let comp_id = resolve_data_component_type_id(host, type_arg)?;
            if live.is_key_opaque(&comp_id) {
                return None;
            }
            let val = stack_component_get(host, &item_id, &live.patch, &comp_id)?;
            Some(Ok(if matches!(val, Value::Ref(None)) {
                default_arg.clone()
            } else {
                val
            }))
        }
        ("remove", "(Lnet/minecraft/core/component/DataComponentType;)Ljava/lang/Object;") => {
            let type_arg = args.first()?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let item_id = live.resolved_item_id(host)?;
            let comp_id = resolve_data_component_type_id(host, type_arg)?;
            if live.is_key_opaque(&comp_id) {
                return None;
            }
            let mut next_patch = live.patch.clone();
            let prev = patched_remove(host, &item_id, &mut next_patch, &comp_id)?;
            let slot = host.recipes.stacks.get_mut(&key)?;
            slot.patch = next_patch;
            Some(Ok(prev))
        }
        ("has", "(Lnet/minecraft/core/component/DataComponentType;)Z") => {
            if live.is_empty() {
                return Some(Ok(Value::Int(0)));
            }
            let item_id = live.resolved_item_id(host)?;
            let type_arg = args.first()?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let comp_id = resolve_data_component_type_id(host, type_arg)?;
            if live.is_key_opaque(&comp_id) {
                return None;
            }
            stack_component_get(host, &item_id, &live.patch, &comp_id)
                .map(|v| Ok(Value::Int(i32::from(!matches!(v, Value::Ref(None))))))
        }
        ("getMaxDamage", "()I") => {
            if live.is_empty() {
                return Some(Ok(Value::Int(0)));
            }
            if live.is_key_opaque("minecraft:max_damage") {
                return None;
            }
            let item_id = live.resolved_item_id(host)?;
            let val = stack_component_get(host, &item_id, &live.patch, "minecraft:max_damage")?;
            let max = match val {
                Value::Ref(None) => 0,
                Value::Int(n) | Value::Boxed(Boxed::Integer(n)) => n,
                _ => return None,
            };
            Some(Ok(Value::Int(max)))
        }
        ("getDamageValue", "()I") => {
            if live.is_empty() {
                return Some(Ok(Value::Int(0)));
            }
            if live.is_key_opaque("minecraft:damage") || live.is_key_opaque("minecraft:max_damage")
            {
                return None;
            }
            let item_id = live.resolved_item_id(host)?;
            let dmg_val = stack_component_get(host, &item_id, &live.patch, "minecraft:damage")?;
            let max_val = stack_component_get(host, &item_id, &live.patch, "minecraft:max_damage")?;
            let dmg = match dmg_val {
                Value::Ref(None) => 0,
                Value::Int(n) | Value::Boxed(Boxed::Integer(n)) => n,
                _ => return None,
            };
            let max = match max_val {
                Value::Ref(None) => 0,
                Value::Int(n) | Value::Boxed(Boxed::Integer(n)) => n,
                _ => return None,
            };
            // `Mth.clamp(dmg, 0, max)` (`ItemStack.getDamageValue` offsets 0..25).
            let clamped = if max < 0 { 0 } else { dmg.clamp(0, max) };
            Some(Ok(Value::Int(clamped)))
        }
        ("isDamageableItem", "()Z") => {
            if live.is_empty() {
                return Some(Ok(Value::Int(0)));
            }
            if live.is_key_opaque("minecraft:max_damage")
                || live.is_key_opaque("minecraft:unbreakable")
                || live.is_key_opaque("minecraft:damage")
            {
                return None;
            }
            let item_id = live.resolved_item_id(host)?;
            let has_max = !matches!(
                stack_component_get(host, &item_id, &live.patch, "minecraft:max_damage")?,
                Value::Ref(None)
            );
            let has_unbreakable = !matches!(
                stack_component_get(host, &item_id, &live.patch, "minecraft:unbreakable")?,
                Value::Ref(None)
            );
            let has_dmg = !matches!(
                stack_component_get(host, &item_id, &live.patch, "minecraft:damage")?,
                Value::Ref(None)
            );
            Some(Ok(Value::Int(i32::from(
                has_max && !has_unbreakable && has_dmg,
            ))))
        }
        ("isDamaged", "()Z") => {
            if live.is_empty() {
                return Some(Ok(Value::Int(0)));
            }
            if live.is_key_opaque("minecraft:max_damage")
                || live.is_key_opaque("minecraft:unbreakable")
                || live.is_key_opaque("minecraft:damage")
            {
                return None;
            }
            let item_id = live.resolved_item_id(host)?;
            let max_val = stack_component_get(host, &item_id, &live.patch, "minecraft:max_damage")?;
            let unbreakable =
                stack_component_get(host, &item_id, &live.patch, "minecraft:unbreakable")?;
            let dmg_val = stack_component_get(host, &item_id, &live.patch, "minecraft:damage")?;
            if matches!(max_val, Value::Ref(None))
                || !matches!(unbreakable, Value::Ref(None))
                || matches!(dmg_val, Value::Ref(None))
            {
                return Some(Ok(Value::Int(0)));
            }
            let (
                Value::Int(dmg) | Value::Boxed(Boxed::Integer(dmg)),
                Value::Int(max) | Value::Boxed(Boxed::Integer(max)),
            ) = (dmg_val, max_val)
            else {
                return None;
            };
            let clamped = if max < 0 { 0 } else { dmg.clamp(0, max) };
            Some(Ok(Value::Int(i32::from(clamped > 0))))
        }
        ("applyComponents", "(Lnet/minecraft/core/component/DataComponentPatch;)V") => {
            let patch_arg = args.first()?;
            if matches!(patch_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let item_id = live.resolved_item_id(host)?;
            let overlay = host.recipes.patch_of(patch_arg)?.clone();
            if overlay
                .entries
                .keys()
                .any(|comp_id| live.is_key_opaque(comp_id))
            {
                return None;
            }
            let mut next_patch = live.patch.clone();
            patched_apply_patch(host, &item_id, &mut next_patch, &overlay)?;
            let slot = host.recipes.stacks.get_mut(&key)?;
            slot.patch = next_patch;
            Some(Ok(Value::Void))
        }
        ("copy", "()Lnet/minecraft/world/item/ItemStack;") => {
            if live.is_empty() {
                Some(Ok(crate::runtime::menu_models::empty_stack()))
            } else {
                Some(Ok(host.recipes.alloc_stack(live)))
            }
        }
        ("copyWithCount", "(I)Lnet/minecraft/world/item/ItemStack;") => {
            let count = match expect_int(r, args.first()?) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };
            // 26.3 `copyWithCount` offsets 0-22: `isEmpty() ? EMPTY : copy()` with `setCount`,
            // so a non-empty stack copies even to a count of 0 or less (an empty stack that is
            // not `EMPTY` itself).
            if live.is_empty() {
                Some(Ok(crate::runtime::menu_models::empty_stack()))
            } else {
                let mut cloned = live;
                cloned.count = count;
                Some(Ok(host.recipes.alloc_stack(cloned)))
            }
        }
        // 26.3 `split(int)` offsets 0-21: `int n = Math.min(amount, getCount()); ItemStack
        // part = copyWithCount(n); shrink(n); return part`.
        ("split", "(I)Lnet/minecraft/world/item/ItemStack;") => {
            let amount = match expect_int(r, args.first()?) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };
            let taken = amount.min(live.effective_count());
            let part = if live.is_empty() {
                crate::runtime::menu_models::empty_stack()
            } else {
                let mut cloned = live.clone();
                cloned.count = taken;
                host.recipes.alloc_stack(cloned)
            };
            let slot = host.recipes.stacks.get_mut(&key)?;
            slot.count = slot.effective_count().wrapping_sub(taken);
            Some(Ok(part))
        }
        // 26.3 `copyAndClear()` offsets 0-22: `isEmpty() ? EMPTY : copy()`, then `setCount(0)`.
        ("copyAndClear", "()Lnet/minecraft/world/item/ItemStack;") => {
            if live.is_empty() {
                return Some(Ok(crate::runtime::menu_models::empty_stack()));
            }
            let copy = host.recipes.alloc_stack(live);
            host.recipes.stacks.get_mut(&key)?.count = 0;
            Some(Ok(copy))
        }
        // 26.3 `isStackable()` offsets 0-27: `getMaxStackSize() > 1 && (!isDamageableItem() ||
        // !isDamaged())`.
        ("isStackable", "()Z") => {
            let max = live_stack_instance_call(
                host,
                handle,
                &MethodRef {
                    owner: ITEM_STACK.into(),
                    name: "getMaxStackSize".into(),
                    descriptor: "()I".into(),
                },
                &[],
            )?;
            let max = match max {
                Ok(Value::Int(max)) => max,
                Ok(_) => return None,
                Err(e) => return Some(Err(e)),
            };
            if max <= 1 {
                return Some(Ok(Value::Int(0)));
            }
            for name in ["isDamageableItem", "isDamaged"] {
                let answer = live_stack_instance_call(
                    host,
                    handle,
                    &MethodRef {
                        owner: ITEM_STACK.into(),
                        name: name.into(),
                        descriptor: "()Z".into(),
                    },
                    &[],
                )?;
                match answer {
                    // Not damageable, or damageable and not damaged: stackable.
                    Ok(Value::Int(0)) => return Some(Ok(Value::Int(1))),
                    Ok(Value::Int(_)) => {}
                    Ok(_) => return None,
                    Err(e) => return Some(Err(e)),
                }
            }
            Some(Ok(Value::Int(0)))
        }
        _ => None,
    }
}

pub(crate) fn stack_component_get(
    host: &StdHost,
    item_id: &str,
    patch: &ComponentPatch,
    comp_id: &str,
) -> Option<Value> {
    if let Some(entry) = patch.entries.get(comp_id) {
        return Some(entry.clone().unwrap_or(Value::Ref(None)));
    }
    prototype_component(host, item_id, comp_id).map(|opt| opt.unwrap_or(Value::Ref(None)))
}

fn patch_instance_call(
    host: &mut StdHost,
    handle: &HostRef,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != DATA_COMPONENT_PATCH {
        return None;
    }
    let patch = host.recipes.patch_of(&Value::Host(handle.clone()))?.clone();
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("isEmpty", "()Z") => Some(Ok(Value::Int(i32::from(patch.is_empty())))),
        ("size", "()I") => Some(Ok(Value::Int(patch.size()))),
        ("get", "(Lnet/minecraft/core/component/DataComponentType;)Ljava/util/Optional;") => {
            let type_arg = args.first()?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let comp_id = resolve_data_component_type_id(host, type_arg)?;
            Some(Ok(match patch.entries.get(&comp_id) {
                None => Value::Ref(None),
                Some(None) => crate::runtime::empty_optional(),
                Some(Some(val)) => crate::runtime::optional_wrap(val.clone()),
            }))
        }
        _ => None,
    }
}

fn patch_builder_instance_call(
    host: &mut StdHost,
    handle: &HostRef,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != DATA_COMPONENT_PATCH_BUILDER {
        return None;
    }
    let key = handle.key().to_owned();
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "set",
            "(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Lnet/minecraft/core/component/DataComponentPatch$Builder;",
        ) => {
            let type_arg = args.first()?;
            let val_arg = args.get(1)?;
            if matches!(type_arg, Value::Ref(None)) || matches!(val_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let comp_id = resolve_data_component_type_id(host, type_arg)?;
            let builder = host.recipes.patch_builders.get_mut(&key)?;
            builder.entries.insert(comp_id, Some(val_arg.clone()));
            Some(Ok(Value::Host(handle.clone())))
        }
        (
            "remove",
            "(Lnet/minecraft/core/component/DataComponentType;)Lnet/minecraft/core/component/DataComponentPatch$Builder;",
        ) => {
            let type_arg = args.first()?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let comp_id = resolve_data_component_type_id(host, type_arg)?;
            let builder = host.recipes.patch_builders.get_mut(&key)?;
            builder.entries.insert(comp_id, None);
            Some(Ok(Value::Host(handle.clone())))
        }
        ("build", "()Lnet/minecraft/core/component/DataComponentPatch;") => {
            let patch = host.recipes.patch_builders.get(&key)?.clone();
            Some(Ok(host.recipes.alloc_patch(patch)))
        }
        _ => None,
    }
}

fn ingredient_instance_call(
    host: &mut StdHost,
    handle: &HostRef,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let spec = host.recipes.ingredients.get(handle.key())?.clone();
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (
            INGREDIENT | "java/util/function/Predicate",
            "test",
            "(Lnet/minecraft/world/item/ItemStack;)Z" | "(Ljava/lang/Object;)Z",
        ) => {
            let arg = args.first()?;
            if matches!(arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            ingredient_test(host, &spec, arg).map(|matched| Ok(Value::Int(i32::from(matched))))
        }
        (INGREDIENT, "isEmpty", "()Z") => {
            let empty = match &spec {
                IngredientSpec::Items(ids) => ids.is_empty(),
                IngredientSpec::Tag(tag_id) => {
                    resolve_item_tag(host, tag_id).is_none_or(|set| set.is_empty())
                }
            };
            Some(Ok(Value::Int(i32::from(empty))))
        }
        _ => None,
    }
}

fn common_info_instance_call(
    host: &mut StdHost,
    handle: &HostRef,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != RECIPE_COMMON_INFO {
        return None;
    }
    let key = handle.key().to_owned();
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("<init>", "(Z)V") => {
            let flag = match expect_int(r, args.first()?) {
                Ok(v) => v != 0,
                Err(e) => return Some(Err(e)),
            };
            host.recipes.common_infos.insert(key, flag);
            Some(Ok(Value::Void))
        }
        ("showNotification", "()Z") => {
            let flag = *host.recipes.common_infos.get(&key)?;
            Some(Ok(Value::Int(i32::from(flag))))
        }
        _ => None,
    }
}

fn holder_instance_call(
    host: &mut StdHost,
    handle: &HostRef,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != RECIPE_HOLDER {
        return None;
    }
    let key = handle.key().to_owned();
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "<init>",
            "(Lnet/minecraft/resources/ResourceKey;Lnet/minecraft/world/item/crafting/Recipe;)V",
        ) => {
            let id = args.first()?.clone();
            let value = args.get(1)?.clone();
            host.recipes.holders.insert(key, HolderData { id, value });
            Some(Ok(Value::Void))
        }
        ("id", "()Lnet/minecraft/resources/ResourceKey;") => {
            Some(Ok(host.recipes.holders.get(&key)?.id.clone()))
        }
        ("value", "()Lnet/minecraft/world/item/crafting/Recipe;") => {
            Some(Ok(host.recipes.holders.get(&key)?.value.clone()))
        }
        _ => None,
    }
}

/// Stateful `toString` for recipe-model handles (invoked from both `Runtime` and `StdHost`).
pub(crate) fn object_to_string(host: &StdHost, handle: &HostRef) -> Option<String> {
    match handle.kind() {
        HostKind::RecipeCommonInfo => {
            let flag = *host.recipes.common_infos.get(handle.key())?;
            Some(format!("CommonInfo[showNotification={flag}]"))
        }
        HostKind::DataComponentPatch if handle.key() == EMPTY_PATCH_KEY => Some("{}".to_owned()),
        HostKind::RecipeHolder => {
            let holder = host.recipes.holders.get(handle.key())?;
            let Value::Host(id_handle) = &holder.id else {
                return None;
            };
            id_handle.to_string(&mut crate::runtime::MarkerText).ok().flatten()
        }
        _ => None,
    }
}

/// Stateful `hashCode` for recipe-model handles.
pub(crate) fn object_hash_code(host: &StdHost, handle: &HostRef) -> Option<i32> {
    match handle.kind() {
        HostKind::RecipeCommonInfo => {
            let flag = *host.recipes.common_infos.get(handle.key())?;
            Some(if flag { 1231 } else { 1237 })
        }
        _ => None,
    }
}

/// Stateful `equals` for recipe-model handles that don't need recursive jar `equals`.
pub(crate) fn host_object_equals(host: &StdHost, handle: &HostRef, other: &Value) -> Option<bool> {
    match handle.kind() {
        HostKind::DataComponentPatchBuilder => match other {
            Value::Host(other) => handle.same_object(other),
            _ => Some(false),
        },
        HostKind::RecipeCommonInfo => match other {
            Value::Host(other) if other.kind() == HostKind::RecipeCommonInfo => {
                let a = host.recipes.common_infos.get(handle.key())?;
                let b = host.recipes.common_infos.get(other.key())?;
                Some(a == b)
            }
            _ => Some(false),
        },
        HostKind::Ingredient => match other {
            Value::Host(other) if other.kind() == HostKind::Ingredient => {
                let a = host.recipes.ingredients.get(handle.key())?;
                let b = host.recipes.ingredients.get(other.key())?;
                Some(a == b)
            }
            _ => Some(false),
        },
        HostKind::RecipeHolder => match other {
            Value::Host(other) if other.kind() == HostKind::RecipeHolder => {
                let a = host.recipes.holders.get(handle.key())?;
                let b = host.recipes.holders.get(other.key())?;
                // MC 26.3 RecipeHolder.equals compares `this.id == other.id` (ResourceKey identity).
                match (&a.id, &b.id) {
                    (Value::Host(ha), Value::Host(hb)) => ha.same_object(hb),
                    (Value::Ref(None), Value::Ref(None)) => Some(true),
                    (Value::Ref(None), _) | (_, Value::Ref(None)) => Some(false),
                    _ => None,
                }
            }
            _ => Some(false),
        },
        HostKind::DataComponentPatch => match other {
            Value::Host(other) if other.kind() == HostKind::DataComponentPatch => {
                let a = host.recipes.patch_of(&Value::Host(handle.clone()))?;
                let b = host.recipes.patch_of(&Value::Host(other.clone()))?;
                if a.entries.len() != b.entries.len() {
                    return Some(false);
                }
                if a.entries.is_empty() {
                    return Some(true);
                }
                None
            }
            _ => Some(false),
        },
        HostKind::ItemStackTemplate => match other {
            Value::Host(other) if other.kind() == HostKind::ItemStackTemplate => {
                let a = host.recipes.templates.get(handle.key())?;
                let b = host.recipes.templates.get(other.key())?;
                if a.item_id != b.item_id || a.count != b.count {
                    return Some(false);
                }
                if a.patch.is_empty() && b.patch.is_empty() {
                    return Some(true);
                }
                None
            }
            _ => Some(false),
        },
        HostKind::CustomData => match other {
            Value::Host(other) if other.kind() == HostKind::CustomData => {
                let a = host.recipes.custom_data_of(&Value::Host(handle.clone()))?;
                let b = host.recipes.custom_data_of(&Value::Host(other.clone()))?;
                Some(a == b)
            }
            _ => Some(false),
        },
        HostKind::Unit => Some(matches!(
            other,
            Value::Host(other) if other.kind() == HostKind::Unit
        )),
        HostKind::TooltipDisplay => {
            let self_val = Value::Host(handle.clone());
            // Keep the legacy opaque `"default"` placeholder counted.
            host.recipes.tooltip_display_of(&self_val)?;
            match other {
                Value::Host(other_h) if other_h.kind() == HostKind::TooltipDisplay => {
                    host_component_values_equal(host, &self_val, other)
                }
                other
                    if crate::runtime::jar_object::object_class(other)
                        == Some("net/minecraft/world/item/component/TooltipDisplay") =>
                {
                    None
                }
                _ => Some(false),
            }
        }
        _ => None,
    }
}

/// Runtime-side `equals` for patches and templates that may hold arbitrary Java values.
pub(crate) fn runtime_object_equals<H: Host>(
    runtime: &mut Runtime<'_, H>,
    handle: &HostRef,
    other: &Value,
) -> Result<Option<bool>, NativeError> {
    if let Some(std_host) = runtime.host.as_std_host()
        && let Some(ans) = host_object_equals(std_host, handle, other)
    {
        return Ok(Some(ans));
    }
    let Some(std_host) = runtime.host.as_std_host() else {
        return Ok(None);
    };
    match handle.kind() {
        HostKind::DataComponentPatch => {
            let Value::Host(other_h) = other else {
                return Ok(Some(false));
            };
            if other_h.kind() != HostKind::DataComponentPatch {
                return Ok(Some(false));
            }
            let (Some(a), Some(b)) = (
                std_host
                    .recipes
                    .patch_of(&Value::Host(handle.clone()))
                    .cloned(),
                std_host
                    .recipes
                    .patch_of(&Value::Host(other_h.clone()))
                    .cloned(),
            ) else {
                return Ok(None);
            };
            patches_equal_runtime(runtime, &a, &b)
        }
        HostKind::ItemStackTemplate => {
            let Value::Host(other_h) = other else {
                return Ok(Some(false));
            };
            if other_h.kind() != HostKind::ItemStackTemplate {
                return Ok(Some(false));
            }
            let (Some(a), Some(b)) = (
                std_host.recipes.templates.get(handle.key()).cloned(),
                std_host.recipes.templates.get(other_h.key()).cloned(),
            ) else {
                return Ok(None);
            };
            if a.item_id != b.item_id || a.count != b.count {
                return Ok(Some(false));
            }
            patches_equal_runtime(runtime, &a.patch, &b.patch)
        }
        _ => Ok(None),
    }
}

fn patches_equal_runtime<H: Host>(
    runtime: &mut Runtime<'_, H>,
    a: &ComponentPatch,
    b: &ComponentPatch,
) -> Result<Option<bool>, NativeError> {
    if a.entries.len() != b.entries.len() {
        return Ok(Some(false));
    }
    for (k, va) in &a.entries {
        let Some(vb) = b.entries.get(k) else {
            return Ok(Some(false));
        };
        match (va, vb) {
            (None, None) => {}
            (Some(x), Some(y)) => {
                let before = runtime.host.counted_unknown_calls();
                let equal = runtime.objects_equals(x, y)?;
                if runtime.host.counted_unknown_calls() != before {
                    return Ok(None);
                }
                if !equal {
                    return Ok(Some(false));
                }
            }
            _ => return Ok(Some(false)),
        }
    }
    Ok(Some(true))
}

/// Runtime-side `Ingredient.of(ItemLike...)` (reads array elements from `Runtime::arrays`).
pub(crate) fn runtime_static_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    if r.owner == INGREDIENT
        && r.name == "of"
        && r.descriptor
            == "([Lnet/minecraft/world/level/ItemLike;)Lnet/minecraft/world/item/crafting/Ingredient;"
    {
        let Some(array_arg) = args.first() else {
            return Ok(None);
        };
        if matches!(array_arg, Value::Ref(None)) {
            return Err(npe());
        }
        let Ok(elements) = runtime.array(array_arg).map(std::borrow::Cow::into_owned) else {
            return Ok(None);
        };
        let Some(std_host) = runtime.host.as_std_host_mut() else {
            return Ok(None);
        };
        let mut ids = Vec::with_capacity(elements.len());
        for elem in &elements {
            if matches!(elem, Value::Ref(None)) {
                return Err(npe());
            }
            let Some(id) = resolve_item_id(std_host, elem) else {
                return Ok(None);
            };
            if id == "minecraft:air" {
                return Err(unsupported("Ingredient can't contain air"));
            }
            ids.push(id);
        }
        return Ok(Some(
            std_host
                .recipes
                .alloc_ingredient(IngredientSpec::Items(ids)),
        ));
    }
    Ok(None)
}

fn cache_fingerprint(host: &StdHost) -> (usize, usize, u64, usize) {
    let serializers = host
        .registrations()
        .iter()
        .filter(|e| e.registry == "minecraft:recipe_serializer")
        .count();
    let items = host
        .registrations()
        .iter()
        .filter(|e| e.registry == "minecraft:item")
        .count();
    let res_gen = host.resource_generation();
    let tags = host.recipes.extra_item_tags.len();
    (serializers, items, res_gen, tags)
}

/// Read the `MapCodec` component from PR #614's canonical `RecipeSerializer` record handle.
fn serializer_codec(value: &Value) -> Option<Value> {
    let Value::Host(handle) = value else {
        return None;
    };
    let (codec, _) = crate::runtime::recipe_serializer::parts(handle)?;
    (!matches!(codec, Value::Ref(None))).then_some(codec)
}

fn parse_recipe_resource_id(path: &str) -> Option<String> {
    let rest = path.strip_prefix("data/")?;
    let (ns, after_ns) = rest.split_once("/recipe/")?;
    let recipe_path = after_ns.strip_suffix(".json")?;
    if ns.is_empty() || ns.contains('/') || recipe_path.is_empty() {
        return None;
    }
    normalize_id(&format!("{ns}:{recipe_path}"))
}

/// Extract a recipe id from a `ResourceKey`, enforcing that its registry is `minecraft:recipe`.
/// Returns `None` if `key` is not a modelled `ResourceKey` (caller must count), `Some(None)` if
/// `key` is null or belongs to another registry (e.g. `minecraft:item`), or `Some(Some(id))`.
fn recipe_key_id(key: &Value) -> Option<Option<String>> {
    if matches!(key, Value::Ref(None)) {
        return Some(None);
    }
    if !crate::runtime::host_ids::is_resource_key(key) {
        return None;
    }
    let reg = crate::runtime::host_ids::registry_name(&crate::runtime::host_ids::key_registry(key)?)?;
    if reg != "minecraft:recipe" {
        return Some(None);
    }
    let loc = crate::runtime::host_ids::identifier_key(&crate::runtime::host_ids::key_location(key)?)?.to_owned();
    Some(Some(loc))
}

/// Load and cache all `data/<ns>/recipe/**/*.json` resources across every loaded jar and datapack
/// layer in vanilla's pack order (higher-priority layer overrides lower-priority layer for
/// duplicate recipe ids), decoding each mod-serializer recipe through its own DFU `MapCodec`.
pub(crate) fn ensure_recipes_loaded<H: Host>(
    runtime: &mut Runtime<'_, H>,
) -> Result<(), NativeError> {
    let (fingerprint, serializer_codecs, recipe_entries, unsupported_pack_order) = {
        let Some(std_host) = runtime.host.as_std_host() else {
            return Ok(());
        };
        let fp = cache_fingerprint(std_host);
        if std_host
            .recipes
            .loaded
            .as_ref()
            .is_some_and(|c| c.fingerprint == fp)
        {
            return Ok(());
        }
        let mut ser_codecs: BTreeMap<String, Value> = BTreeMap::new();
        for entry in std_host.registrations() {
            if entry.registry == "minecraft:recipe_serializer"
                && let Some(codec) = serializer_codec(&entry.value)
            {
                ser_codecs.insert(entry.id.clone(), codec);
            }
        }
        // Build ordered layer map (lowest -> highest priority) per MC 26.3 PackRepository +
        // fabric-resource-loader-v1 (`pack_order.rs`): later packs override earlier packs for
        // duplicate recipe ids while retaining source-pack provenance.
        let ordered = std_host.ordered_data_packs();
        let unsupported: Vec<MethodRef> = ordered
            .unsupported_constraints
            .into_iter()
            .map(|c| c.method_ref())
            .collect();
        let mut by_id: BTreeMap<String, (String, Vec<u8>)> = BTreeMap::new();
        for (pack_id, layer) in ordered.layers {
            for (path, bytes) in layer {
                if let Some(id) = parse_recipe_resource_id(path) {
                    by_id.insert(id, (pack_id.to_owned(), bytes.clone()));
                }
            }
        }
        (fp, ser_codecs, by_id, unsupported)
    };

    for mref in unsupported_pack_order {
        runtime.host.unknown_operand_call(&mref)?;
    }

    let mut loaded_recipes = Vec::new();
    let mut errors = Vec::new();
    let mut unmodelled_serializers = BTreeSet::new();
    let mut skipped_recipe_ids = BTreeSet::new();

    for (recipe_id, (source_pack, bytes)) in recipe_entries {
        let parsed: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(err) => {
                errors.push((
                    recipe_id,
                    format!("[{source_pack}] invalid recipe JSON: {err}"),
                ));
                continue;
            }
        };
        let Some(obj) = parsed.as_object() else {
            errors.push((
                recipe_id,
                format!("[{source_pack}] recipe JSON root is not an object"),
            ));
            continue;
        };
        let Some(raw_type) = obj.get("type").and_then(serde_json::Value::as_str) else {
            errors.push((
                recipe_id,
                format!("[{source_pack}] missing recipe serializer type"),
            ));
            continue;
        };
        let Some(norm_type) = normalize_id(raw_type) else {
            errors.push((
                recipe_id,
                format!("[{source_pack}] invalid recipe serializer id: {raw_type}"),
            ));
            continue;
        };
        let Some(map_codec) = serializer_codecs.get(&norm_type) else {
            skipped_recipe_ids.insert(recipe_id);
            if !vanilla_item_data::is_vanilla_recipe_serializer(&norm_type) {
                unmodelled_serializers.insert(norm_type);
            }
            continue;
        };

        let before_decode = runtime.host.counted_unknown_calls();
        let decode_result = crate::runtime::dfu_codec::decode_map_json(runtime, map_codec, obj)?;
        let decode_counted = runtime.host.counted_unknown_calls() != before_decode;
        let outcome = if decode_counted {
            Err(DecodeError::Unmodelled(
                "counted unknown call during recipe decode".into(),
            ))
        } else {
            decode_result
        };

        match outcome {
            Ok(recipe_value) => {
                let before_type = runtime.host.counted_unknown_calls();
                let recipe_type = runtime.call_virtual(
                    crate::runtime::jdk_call::Site::Interface,
                    "net/minecraft/world/item/crafting/Recipe",
                    "getType",
                    "()Lnet/minecraft/world/item/crafting/RecipeType;",
                    &recipe_value,
                    &[],
                )?;
                if runtime.host.counted_unknown_calls() != before_type {
                    let _ = runtime.host.unknown_operand_call(&MethodRef {
                        owner: RECIPE_MANAGER.into(),
                        name: "decodeRecipe".into(),
                        descriptor: format!("({norm_type})V"),
                    });
                    unmodelled_serializers.insert(norm_type);
                    skipped_recipe_ids.insert(recipe_id.clone());
                    errors.push((
                        recipe_id,
                        format!("[{source_pack}] counted unknown call inside Recipe.getType"),
                    ));
                    continue;
                }
                let key = crate::runtime::host_ids::resource_key_named("minecraft:recipe", &recipe_id);
                let holder = {
                    let Some(std_host) = runtime.host.as_std_host_mut() else {
                        return Ok(());
                    };
                    std_host.recipes.alloc_holder(key, recipe_value.clone())
                };
                loaded_recipes.push(LoadedRecipe {
                    id: recipe_id,
                    source_pack,
                    recipe_type,
                    recipe: recipe_value,
                    holder,
                });
            }
            Err(DecodeError::Invalid(msg)) => {
                errors.push((recipe_id, format!("[{source_pack}] {msg}")));
            }
            Err(DecodeError::Unmodelled(msg)) => {
                let _ = runtime.host.unknown_operand_call(&MethodRef {
                    owner: RECIPE_MANAGER.into(),
                    name: "decodeRecipe".into(),
                    descriptor: format!("({norm_type})V"),
                });
                unmodelled_serializers.insert(norm_type);
                skipped_recipe_ids.insert(recipe_id.clone());
                errors.push((
                    recipe_id,
                    format!("[{source_pack}] unmodelled codec: {msg}"),
                ));
            }
        }
    }

    if let Some(std_host) = runtime.host.as_std_host_mut() {
        std_host.recipes.loaded = Some(LoadedRecipeCache {
            fingerprint,
            recipes: loaded_recipes,
            errors,
            unmodelled_serializers,
            skipped_recipe_ids,
        });
    }
    Ok(())
}

/// Exact MC 26.3 `ItemStack.isSameItemSameComponents(ItemStack a, ItemStack b)`:
/// checks `a.is(b.getItem())`, empty-stack equivalence, and component-patch equality.
/// Returns `Ok(None)` (caller counts) if either stack carries opaque components or an
/// incomparable component value.
pub(crate) fn stacks_same_item_same_components(
    host: &StdHost,
    a: &Value,
    b: &Value,
) -> Result<Option<bool>, NativeError> {
    if matches!(a, Value::Ref(None)) {
        return Err(NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: Some(
                "Cannot invoke \"net.minecraft.world.item.ItemStack.is(Object)\" because \"a\" is null"
                    .into(),
            ),
            frame: None,
        });
    }
    if matches!(b, Value::Ref(None)) {
        return Err(NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: Some(
                "Cannot invoke \"net.minecraft.world.item.ItemStack.getItem()\" because \"b\" is null"
                    .into(),
            ),
            frame: None,
        });
    }
    let (Some((item_a, _, empty_a)), Some((item_b, _, empty_b))) =
        (host.read_item_stack_impl(a), host.read_item_stack_impl(b))
    else {
        return Ok(None);
    };
    let Some(same_item) = same_item_value(host, &item_a, None, &item_b) else {
        return Ok(None);
    };
    if !same_item {
        return Ok(Some(false));
    }
    if empty_a && empty_b {
        return Ok(Some(true));
    }
    if empty_a != empty_b {
        return Ok(Some(false));
    }
    let (Some(live_a), Some(live_b)) = (read_live_stack_opt(host, a), read_live_stack_opt(host, b))
    else {
        return Ok(None);
    };
    if live_a.has_opaque_components() || live_b.has_opaque_components() {
        return Ok(None);
    }
    if live_a.patch.entries.len() != live_b.patch.entries.len() {
        return Ok(Some(false));
    }
    for (comp_id, val_a) in &live_a.patch.entries {
        let Some(val_b) = live_b.patch.entries.get(comp_id) else {
            return Ok(Some(false));
        };
        match (val_a, val_b) {
            (None, None) => {}
            (Some(va), Some(vb)) => {
                let Some(eq) = host_component_values_equal(host, va, vb) else {
                    return Ok(None);
                };
                if !eq {
                    return Ok(Some(false));
                }
            }
            _ => return Ok(Some(false)),
        }
    }
    Ok(Some(true))
}

fn runtime_value_from_host(host: &StdHost, value: &Value) -> RuntimeComponentValue {
    match value {
        Value::Int(n) | Value::Boxed(Boxed::Integer(n)) => RuntimeComponentValue::Int(*n),
        Value::Host(handle) => match handle.kind() {
            HostKind::Unit => RuntimeComponentValue::Unit,
            HostKind::CustomData => match host.recipes.custom_data_of(value) {
                Some(nbt) => RuntimeComponentValue::CustomData(nbt.clone()),
                None => RuntimeComponentValue::Unsupported,
            },
            HostKind::Component => match host.component_json(value) {
                Some(json) => RuntimeComponentValue::ComponentJson(json),
                None => RuntimeComponentValue::Unsupported,
            },
            HostKind::TooltipDisplay => match host.recipes.tooltip_display_of(value) {
                Some(td) => RuntimeComponentValue::TooltipDisplay {
                    hide_tooltip: td.hide_tooltip,
                    hidden_components: td.hidden_components,
                    hidden_known: td.hidden_known,
                },
                None => RuntimeComponentValue::Unsupported,
            },
            _ => RuntimeComponentValue::Unsupported,
        },
        _ => RuntimeComponentValue::Unsupported,
    }
}

pub(crate) fn extract_stack_components(host: &StdHost, stack: &Value) -> Option<StackComponents> {
    if is_empty_stack_handle(stack) {
        return Some(StackComponents {
            item_id: "minecraft:air".to_owned(),
            count: 0,
            patch: Vec::new(),
            opaque_components: false,
        });
    }
    let live = read_live_stack_opt(host, stack)?;
    let item_id = live.resolved_item_id(host)?;
    let count = live.effective_count();
    let patch = if live.is_empty() {
        Vec::new()
    } else {
        live.patch
            .entries
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_ref().map(|val| runtime_value_from_host(host, val)),
                )
            })
            .collect()
    };
    Some(StackComponents {
        item_id,
        count,
        patch,
        opaque_components: live.has_opaque_components(),
    })
}

pub(crate) fn build_host_stack(
    host: &mut StdHost,
    item_override: Option<Value>,
    item_id: &str,
    count: i32,
    patch: Vec<(String, Option<RuntimeComponentValue>)>,
    mut opaque_components: bool,
    opaque_keys: Vec<String>,
) -> Value {
    let mut opaque_set: BTreeSet<String> = opaque_keys.into_iter().collect();
    let mut built_patch = ComponentPatch::default();
    for (comp_id, entry) in patch {
        match entry {
            None => {
                built_patch.entries.insert(comp_id, None);
            }
            Some(RuntimeComponentValue::Int(n)) => {
                built_patch
                    .entries
                    .insert(comp_id, Some(Value::Boxed(Boxed::Integer(n))));
            }
            Some(RuntimeComponentValue::Unit) => {
                built_patch.entries.insert(comp_id, Some(unit_instance()));
            }
            Some(RuntimeComponentValue::CustomData(nbt)) => {
                let handle = host.recipes.alloc_custom_data(nbt);
                built_patch.entries.insert(comp_id, Some(handle));
            }
            Some(RuntimeComponentValue::ComponentJson(json)) => {
                match serde_json::from_str::<serde_json::Value>(&json) {
                    Ok(parsed) => {
                        let handle = crate::runtime::host_component::component(&parsed);
                        built_patch.entries.insert(comp_id, Some(handle));
                    }
                    Err(_) => {
                        opaque_components = true;
                        opaque_set.insert(comp_id);
                    }
                }
            }
            Some(RuntimeComponentValue::TooltipDisplay {
                hide_tooltip,
                hidden_components,
                hidden_known,
            }) => {
                let handle = host.recipes.alloc_tooltip_display(TooltipDisplayData {
                    hide_tooltip,
                    hidden_components,
                    hidden_known,
                });
                built_patch.entries.insert(comp_id, Some(handle));
            }
            Some(RuntimeComponentValue::Unsupported | RuntimeComponentValue::Nbt(_)) => {
                opaque_components = true;
                opaque_set.insert(comp_id);
            }
        }
    }
    let live = LiveStack::from_host_stack(
        host,
        item_override,
        item_id,
        count,
        built_patch,
        opaque_components,
        opaque_set,
    );
    host.recipes.alloc_stack(live)
}

fn runtime_custom_data_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    match (receiver, r.name.as_str(), r.descriptor.as_str()) {
        (
            None,
            "of",
            "(Lnet/minecraft/nbt/CompoundTag;)Lnet/minecraft/world/item/component/CustomData;",
        ) => {
            let Some(tag_arg) = args.first() else {
                return Ok(None);
            };
            if matches!(tag_arg, Value::Ref(None)) {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some(
                        "Cannot invoke \"net.minecraft.nbt.CompoundTag.copy()\" because \"tag\" is null"
                            .into(),
                    ),
                    frame: None,
                });
            }
            let Some(marker) = crate::runtime::jar_object::object_key(tag_arg) else {
                return Ok(None);
            };
            let Ok(crate::runtime::Nbt::Compound(nbt)) = runtime.nbt_read(marker) else {
                return Ok(None);
            };
            let Some(std_host) = runtime.host.as_std_host_mut() else {
                return Ok(None);
            };
            Ok(Some(std_host.recipes.alloc_custom_data(nbt)))
        }
        (
            None,
            "set",
            "(Lnet/minecraft/core/component/DataComponentType;Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/nbt/CompoundTag;)V",
        ) => {
            let [type_arg, stack_arg, tag_arg] = args else {
                return Ok(None);
            };
            if matches!(tag_arg, Value::Ref(None)) {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some(
                        "Cannot invoke \"net.minecraft.nbt.CompoundTag.isEmpty()\" because \"tag\" is null"
                            .into(),
                    ),
                    frame: None,
                });
            }
            if matches!(stack_arg, Value::Ref(None)) {
                return Err(npe());
            }
            if matches!(type_arg, Value::Ref(None)) {
                return Err(npe());
            }
            let Some(marker) = crate::runtime::jar_object::object_key(tag_arg) else {
                return Ok(None);
            };
            let Ok(crate::runtime::Nbt::Compound(nbt)) = runtime.nbt_read(marker) else {
                return Ok(None);
            };
            let Some(std_host) = runtime.host.as_std_host_mut() else {
                return Ok(None);
            };
            if is_empty_stack_handle(stack_arg) {
                return Ok(Some(Value::Void));
            }
            let Value::Host(handle) = stack_arg else {
                return Ok(None);
            };
            if handle.kind() != HostKind::ItemStack {
                return Ok(None);
            }
            let key = handle.key().to_owned();
            let Some(live) = read_live_stack_opt(std_host, stack_arg) else {
                return Ok(None);
            };
            let Some(item_id) = live.resolved_item_id(std_host) else {
                return Ok(None);
            };
            let Some(comp_id) = resolve_data_component_type_id(std_host, type_arg) else {
                return Ok(None);
            };
            if live.is_key_opaque(&comp_id) {
                return Ok(None);
            }
            let mut next_patch = live.patch.clone();
            if nbt.is_empty() {
                let Some(_) = patched_remove(std_host, &item_id, &mut next_patch, &comp_id) else {
                    return Ok(None);
                };
            } else {
                let cd = std_host.recipes.alloc_custom_data(nbt);
                let Some(_) = patched_set(std_host, &item_id, &mut next_patch, &comp_id, &cd)
                else {
                    return Ok(None);
                };
            }
            let Some(slot) = std_host.recipes.stacks.get_mut(&key) else {
                return Ok(None);
            };
            slot.patch = next_patch;
            Ok(Some(Value::Void))
        }
        (Some(recv), "copyTag", "()Lnet/minecraft/nbt/CompoundTag;") => {
            let nbt = {
                let Some(std_host) = runtime.host.as_std_host() else {
                    return Ok(None);
                };
                let Some(tag) = std_host.recipes.custom_data_of(recv) else {
                    return Ok(None);
                };
                tag.clone()
            };
            Ok(Some(runtime.nbt_alloc(&crate::runtime::Nbt::Compound(nbt))))
        }
        _ => Ok(None),
    }
}

/// Dispatch `RecipeManager` / `RecipeAccess` methods on the real `game/recipe-manager` handle,
/// plus `CustomData` methods that read or allocate heap `CompoundTag` objects.
pub(crate) fn runtime_recipe_manager_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    if r.owner == CUSTOM_DATA {
        return runtime_custom_data_call(runtime, r, receiver, args);
    }
    let Some(receiver) = receiver else {
        return Ok(None);
    };
    if crate::runtime::game_object::handle_key(receiver) != Some(RECIPE_MANAGER_KEY) {
        return Ok(None);
    }
    if r.owner != RECIPE_MANAGER && r.owner != RECIPE_ACCESS {
        return Ok(None);
    }
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "getRecipeFor",
            "(Lnet/minecraft/world/item/crafting/RecipeType;Lnet/minecraft/world/item/crafting/RecipeInput;Lnet/minecraft/world/level/Level;)Ljava/util/Optional;",
        ) => {
            let [recipe_type, input, level] = args else {
                return Ok(None);
            };
            find_matching_recipe(runtime, recipe_type, input, level, None, None)
        }
        (
            "getRecipeFor",
            "(Lnet/minecraft/world/item/crafting/RecipeType;Lnet/minecraft/world/item/crafting/RecipeInput;Lnet/minecraft/world/level/Level;Lnet/minecraft/resources/ResourceKey;)Ljava/util/Optional;",
        ) => {
            let [recipe_type, input, level, last_key] = args else {
                return Ok(None);
            };
            let hint_key = (!matches!(last_key, Value::Ref(None))).then_some(last_key);
            find_matching_recipe(runtime, recipe_type, input, level, hint_key, None)
        }
        (
            "getRecipeFor",
            "(Lnet/minecraft/world/item/crafting/RecipeType;Lnet/minecraft/world/item/crafting/RecipeInput;Lnet/minecraft/world/level/Level;Lnet/minecraft/world/item/crafting/RecipeHolder;)Ljava/util/Optional;",
        ) => {
            let [recipe_type, input, level, last_holder] = args else {
                return Ok(None);
            };
            let hint_holder = (!matches!(last_holder, Value::Ref(None))).then_some(last_holder);
            find_matching_recipe(runtime, recipe_type, input, level, None, hint_holder)
        }
        ("byKey", "(Lnet/minecraft/resources/ResourceKey;)Ljava/util/Optional;") => {
            let Some(key) = args.first() else {
                return Ok(None);
            };
            let Some(wanted_opt) = recipe_key_id(key) else {
                return Ok(None);
            };
            let Some(wanted_id) = wanted_opt else {
                // Null key or non-`minecraft:recipe` ResourceKey (e.g. `minecraft:item`): never matches.
                return Ok(Some(crate::runtime::empty_optional()));
            };
            ensure_recipes_loaded(runtime)?;
            let Some(std_host) = runtime.host.as_std_host() else {
                return Ok(None);
            };
            let cache = std_host.recipes.loaded.as_ref();
            if let Some(holder) = cache.and_then(|c| {
                c.recipes
                    .iter()
                    .find(|rec| rec.id == wanted_id)
                    .map(|rec| rec.holder.clone())
            }) {
                return Ok(Some(crate::runtime::optional_wrap(holder)));
            }
            // Never return an uncounted empty Optional for vanilla recipes or skipped/unmodelled
            // recipes whose serializer we could not decode.
            if wanted_id.starts_with("minecraft:")
                || cache.is_some_and(|c| c.skipped_recipe_ids.contains(&wanted_id))
            {
                return Ok(None);
            }
            Ok(Some(crate::runtime::empty_optional()))
        }
        _ => Ok(None),
    }
}

fn objects_equals_exact<H: Host>(
    runtime: &mut Runtime<'_, H>,
    a: &Value,
    b: &Value,
) -> Result<Option<bool>, NativeError> {
    let before = runtime.host.counted_unknown_calls();
    let equal = runtime.objects_equals(a, b)?;
    if runtime.host.counted_unknown_calls() != before {
        return Ok(None);
    }
    Ok(Some(equal))
}

fn recipe_matches_exact<H: Host>(
    runtime: &mut Runtime<'_, H>,
    recipe: &Value,
    input: &Value,
    level: &Value,
) -> Result<Option<bool>, NativeError> {
    let before = runtime.host.counted_unknown_calls();
    let result = runtime.call_virtual(
        crate::runtime::jdk_call::Site::Interface,
        "net/minecraft/world/item/crafting/Recipe",
        "matches",
        "(Lnet/minecraft/world/item/crafting/RecipeInput;Lnet/minecraft/world/level/Level;)Z",
        recipe,
        &[input.clone(), level.clone()],
    )?;
    if runtime.host.counted_unknown_calls() != before {
        return Ok(None);
    }
    Ok(Some(matches!(result, Value::Int(v) if v != 0)))
}

/// Evaluate `RecipeInput.isEmpty()` virtually: runs the mod's override when defined, otherwise
/// runs MC 26.3's default `RecipeInput.isEmpty()` loop over virtual `size()` and `getItem(i)`.
/// Returns `Ok(None)` if any inner call was counted or an unknown `ItemStack` was encountered.
fn recipe_input_is_empty<H: Host>(
    runtime: &mut Runtime<'_, H>,
    input: &Value,
) -> Result<Option<bool>, NativeError> {
    if matches!(input, Value::Ref(None)) {
        return Err(npe());
    }
    let Some(class_name) = crate::runtime::jar_object::object_key(input)
        .and_then(|key| key.split_once('#').map(|(cls, _)| cls.to_owned()))
    else {
        return Ok(None);
    };
    if runtime
        .jar_method_owner(&class_name, "isEmpty", "()Z")
        .is_some()
    {
        let before = runtime.host.counted_unknown_calls();
        let res = runtime.call_virtual(
            crate::runtime::jdk_call::Site::Interface,
            "net/minecraft/world/item/crafting/RecipeInput",
            "isEmpty",
            "()Z",
            input,
            &[],
        )?;
        if runtime.host.counted_unknown_calls() != before {
            return Ok(None);
        }
        return Ok(Some(matches!(res, Value::Int(v) if v != 0)));
    }
    // Default `RecipeInput.isEmpty()` from MC 26.3:
    // `for (int i = 0; i < this.size(); ++i) if (!this.getItem(i).isEmpty()) return false; return true;`
    let before_size = runtime.host.counted_unknown_calls();
    let size_val = runtime.call_virtual(
        crate::runtime::jdk_call::Site::Interface,
        "net/minecraft/world/item/crafting/RecipeInput",
        "size",
        "()I",
        input,
        &[],
    )?;
    if runtime.host.counted_unknown_calls() != before_size {
        return Ok(None);
    }
    let Value::Int(size) = size_val else {
        return Ok(None);
    };
    for i in 0..size {
        let before_item = runtime.host.counted_unknown_calls();
        let stack = runtime.call_virtual(
            crate::runtime::jdk_call::Site::Interface,
            "net/minecraft/world/item/crafting/RecipeInput",
            "getItem",
            "(I)Lnet/minecraft/world/item/ItemStack;",
            input,
            &[Value::Int(i)],
        )?;
        if runtime.host.counted_unknown_calls() != before_item {
            return Ok(None);
        }
        if matches!(stack, Value::Ref(None)) {
            return Err(npe());
        }
        let Some(empty) = runtime
            .host
            .as_std_host()
            .and_then(|h| stack_is_empty(h, &stack))
        else {
            return Ok(None);
        };
        if !empty {
            return Ok(Some(false));
        }
    }
    Ok(Some(true))
}

/// Whether `recipe_type` is a mod-registered non-vanilla `RecipeType` whose recipes are completely
/// loaded (no unmodelled or unregistered non-vanilla serializers in the datapack layers).
/// Vanilla `RecipeType`s and unknown/incomplete types return `Some(false)` so `getRecipeFor` stays
/// counted instead of returning a false empty `Optional`.
fn is_known_complete_recipe_type<H: Host>(
    runtime: &mut Runtime<'_, H>,
    recipe_type: &Value,
) -> Result<Option<bool>, NativeError> {
    if matches!(recipe_type, Value::Ref(None)) {
        return Ok(Some(false));
    }
    let (registered_types, has_unmodelled) = {
        let Some(std_host) = runtime.host.as_std_host() else {
            return Ok(None);
        };
        let types: Vec<(String, Value)> = std_host
            .registrations()
            .iter()
            .filter(|e| e.registry == "minecraft:recipe_type")
            .map(|e| (e.id.clone(), e.value.clone()))
            .collect();
        let unmodelled = std_host
            .recipes
            .loaded
            .as_ref()
            .is_some_and(|c| !c.unmodelled_serializers.is_empty());
        (types, unmodelled)
    };
    if has_unmodelled {
        return Ok(Some(false));
    }
    for (id, reg_val) in &registered_types {
        let Some(eq) = objects_equals_exact(runtime, reg_val, recipe_type)? else {
            return Ok(None);
        };
        if eq {
            let is_vanilla =
                vanilla_item_data::is_vanilla_recipe_type(id) || id.starts_with("minecraft:");
            return Ok(Some(!is_vanilla));
        }
    }
    Ok(Some(false))
}

fn find_matching_recipe<H: Host>(
    runtime: &mut Runtime<'_, H>,
    recipe_type: &Value,
    input: &Value,
    level: &Value,
    hint_key: Option<&Value>,
    hint_holder: Option<&Value>,
) -> Result<Option<Value>, NativeError> {
    ensure_recipes_loaded(runtime)?;

    // Keep vanilla / unregistered / incomplete recipe types counted rather than returning an
    // uncounted `Optional.empty()`.
    if is_known_complete_recipe_type(runtime, recipe_type)? != Some(true) {
        return Ok(None);
    }

    let mut candidates: Vec<LoadedRecipe> = runtime
        .host
        .as_std_host()
        .and_then(|h| h.recipes.loaded.as_ref())
        .map(|c| c.recipes.clone())
        .unwrap_or_default();
    // 26.3's `RecipeMap.byType` keeps the recipe registry's order, which
    // `ResourceManagerRegistryLoadTask` (offsets 0-30) registers sorted by the resource file's
    // `Identifier` (`Map.Entry.comparingByKey()`): `Identifier.compareTo` compares the path
    // (`recipe/<id path>.json`) before the namespace.
    candidates.sort_by(|a, b| vanilla_recipe_order(&a.id, &b.id));

    // Exact MC 26.3 `RecipeManager.getRecipeFor(type, input, level, ResourceKey)`:
    // resolves `byKeyTyped(type, lastKey)` (checking registry `minecraft:recipe` and `getType()`)
    // then delegates to the 4-arg `RecipeHolder` overload.
    let resolved_hint_holder = if let Some(holder_val) = hint_holder {
        Some(holder_val.clone())
    } else if let Some(key_val) = hint_key {
        let Some(wanted_opt) = recipe_key_id(key_val) else {
            return Ok(None);
        };
        let mut found = None;
        if let Some(wanted_id) = wanted_opt
            && let Some(candidate) = candidates.iter().find(|c| c.id == wanted_id)
        {
            let Some(same_type) =
                objects_equals_exact(runtime, &candidate.recipe_type, recipe_type)?
            else {
                return Ok(None);
            };
            if same_type {
                found = Some(candidate.holder.clone());
            }
        }
        found
    } else {
        None
    };

    // Exact MC 26.3 4-arg `RecipeHolder` fast path: check `lastRecipe.value().matches(input, level)`.
    if let Some(holder_val) = resolved_hint_holder {
        let holder_recipe = runtime.host.as_std_host().and_then(|h| {
            let Value::Host(handle) = &holder_val else {
                return None;
            };
            (handle.kind() == HostKind::RecipeHolder)
                .then(|| h.recipes.holders.get(handle.key()).map(|d| d.value.clone()))
                .flatten()
        });
        let Some(recipe) = holder_recipe else {
            return Ok(None);
        };
        let Some(matched) = recipe_matches_exact(runtime, &recipe, input, level)? else {
            return Ok(None);
        };
        if matched {
            return Ok(Some(crate::runtime::optional_wrap(holder_val)));
        }
    }

    // Exact MC 26.3 `RecipeMap.getRecipesFor`: precheck `input.isEmpty()` before testing any recipe.
    let Some(empty) = recipe_input_is_empty(runtime, input)? else {
        return Ok(None);
    };
    if empty {
        return Ok(Some(crate::runtime::empty_optional()));
    }

    for candidate in &candidates {
        let Some(same_type) = objects_equals_exact(runtime, &candidate.recipe_type, recipe_type)?
        else {
            return Ok(None);
        };
        if !same_type {
            continue;
        }
        let Some(matched) = recipe_matches_exact(runtime, &candidate.recipe, input, level)? else {
            return Ok(None);
        };
        if matched {
            return Ok(Some(crate::runtime::optional_wrap(candidate.holder.clone())));
        }
    }
    Ok(Some(crate::runtime::empty_optional()))
}

/// The order 26.3 registers recipes in (see `find_matching_recipe`): the file `Identifier`
/// `<namespace>:recipe/<path>.json`, path first, namespace second.
fn vanilla_recipe_order(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |id: &str| -> (String, String) {
        match id.split_once(':') {
            Some((namespace, path)) => (format!("recipe/{path}.json"), namespace.to_owned()),
            None => (format!("recipe/{id}.json"), "minecraft".to_owned()),
        }
    };
    split(a).cmp(&split(b))
}

pub(crate) fn item_stack_template_illegal_state(message: &str) -> NativeError {
    illegal_state(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_tags_support_nested_vanilla_replace_optional_and_cycles() {
        // Nested vanilla tag
        let host = StdHost::new().with_resource_bytes(
            "data/demo/tags/item/stone_like.json",
            br##"{"values":["#minecraft:stone_tool_materials",{"id":"demo:missing_optional","required":false}]}"##
                .to_vec(),
        );
        let stone_like = resolve_item_tag(&host, "demo:stone_like").unwrap();
        assert!(stone_like.contains("minecraft:cobblestone"));
        assert!(stone_like.contains("minecraft:blackstone"));
        assert!(stone_like.contains("minecraft:cobbled_deepslate"));
        assert!(!stone_like.contains("minecraft:dirt"));

        // Replace vanilla tag cleanly
        let host = host.with_resource_bytes(
            "data/minecraft/tags/item/stone_tool_materials.json",
            br#"{"replace":true,"values":["minecraft:granite"]}"#.to_vec(),
        );
        let replaced = resolve_item_tag(&host, "demo:stone_like").unwrap();
        assert_eq!(replaced.len(), 1);
        assert!(replaced.contains("minecraft:granite"));

        // Cycle detection terminates cleanly
        let host = host
            .with_resource_bytes(
                "data/demo/tags/item/cycle_a.json",
                br##"{"values":["#demo:cycle_b","minecraft:stick"]}"##.to_vec(),
            )
            .with_resource_bytes(
                "data/demo/tags/item/cycle_b.json",
                br##"{"values":["#demo:cycle_a"]}"##.to_vec(),
            );
        let cyclic = resolve_item_tag(&host, "demo:cycle_a").unwrap();
        assert!(cyclic.contains("minecraft:stick"));

        // Missing required tag entry fails resolution
        let host = host.with_resource_bytes(
            "data/demo/tags/item/broken.json",
            br#"{"values":["demo:does_not_exist"]}"#.to_vec(),
        );
        assert!(resolve_item_tag(&host, "demo:broken").is_none());
    }

    #[test]
    fn component_patch_overlay_and_template_max_stack_validation() {
        let mut host = StdHost::new();
        let mut base = ComponentPatch::default();
        base.entries.insert(
            "minecraft:repair_cost".into(),
            Some(Value::Boxed(Boxed::Integer(5))),
        );
        base.entries.insert(
            "minecraft:custom_name".into(),
            Some(crate::runtime::component_value("literal", "Old")),
        );
        let mut tpl_patch = ComponentPatch::default();
        tpl_patch.entries.insert(
            "minecraft:repair_cost".into(),
            Some(Value::Boxed(Boxed::Integer(11))),
        );
        let tpl = host.recipes.alloc_template(TemplateData {
            item_id: "minecraft:ender_pearl".into(),
            count: 16,
            patch: tpl_patch,
        });
        let Value::Host(tpl_handle) = &tpl else {
            panic!("expected template handle");
        };
        let valid = template_apply(&mut host, tpl_handle.key(), 16, &base).unwrap();
        assert_eq!(stack_is_empty(&host, &valid), Some(false));
        let merged = stack_patch_clone(&host, &valid).unwrap();
        assert_eq!(
            merged.entries.get("minecraft:repair_cost"),
            Some(&Some(Value::Boxed(Boxed::Integer(11))))
        );
        assert!(merged.entries.contains_key("minecraft:custom_name"));

        // Count > 16 for ender_pearl returns ItemStack.EMPTY
        let overflow = template_apply(&mut host, tpl_handle.key(), 17, &base).unwrap();
        assert!(is_empty_stack_handle(&overflow));
    }
}
