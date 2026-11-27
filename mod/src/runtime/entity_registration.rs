//! Vanilla entity registration and synchronized-data handles.
//!
//! Every object this model hands the jar (an `EntityType$Builder`, an `EntityType`, its
//! `EntityDimensions`, an `EntityDataAccessor`, the `MobCategory` and `EntityDataSerializers`
//! constants) is keyed as a jar object, `class#n`, with `n` from the running heap's allocator
//! (`jar_object::Numbers`, lent through `Host::allocating_call` / `allocating_static_field` /
//! `allocating_enum_values`). #669: they used to count from
//! `700_000`/`710_000`/`720_000`, and an accessor was keyed by its data id, numbers the heap
//! also hands out (and frees): two holders' accessors with one id were one object, and a
//! model's object could be born collected. A call that reaches the model without the
//! allocator answers only what needs no new object.
use crate::runtime::jar_object::{self, Numbers};
use crate::runtime::{MethodRef, NativeError, Value, expect_float, expect_int, expect_ref};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
struct Builder {
    category: String,
    width: f64,
    height: f64,
    fire_immune: bool,
    summon: bool,
    serialize: bool,
    far_from_player: bool,
    peaceful: bool,
    track_deltas: bool,
    eye_height: f64,
    tracking: i32,
    interval: i32,
}
#[derive(Debug, Clone)]
struct Entity {
    category: String,
    width: f64,
    height: f64,
    fire_immune: bool,
    summon: bool,
    serialize: bool,
    far_from_player: bool,
    peaceful: bool,
    track_deltas: bool,
    eye_height: f64,
    tracking: i32,
    interval: i32,
    description: String,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct State {
    builders: BTreeMap<String, Builder>,
    entities: BTreeMap<String, Entity>,
    dimensions: BTreeMap<String, (f64, f64, f64)>,
    ids: BTreeMap<String, u16>,
    supers: BTreeMap<String, String>,
    registered: BTreeSet<String>,
    /// Each accessor's data id, by the accessor's key: a new object per `defineId`, so the
    /// key cannot be the id.
    accessors: BTreeMap<String, u16>,
    /// The static constants made so far (`MobCategory.CREATURE`, `EntityDataSerializers.INT`),
    /// by `<class>.<NAME>`: one object for the run, numbered on first read. Collection roots
    /// ([`Self::root_texts`]): a static final field is never unreachable.
    constants: BTreeMap<String, Value>,
}

impl State {
    pub(crate) fn entry_count(&self) -> usize {
        self.builders.len() + self.entities.len() + self.dimensions.len() + self.accessors.len()
    }

    pub(crate) fn retain_keys(&mut self, keep: impl Fn(&str) -> bool) {
        self.builders.retain(|key, _| keep(key));
        self.entities.retain(|key, _| keep(key));
        self.dimensions.retain(|key, _| keep(key));
        self.registered.retain(|key| keep(key));
        self.accessors.retain(|key, _| keep(key));
    }

    /// The constants' keys: roots of a collection, so their numbers are never freed.
    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        out.extend(self.constants.values().filter_map(jar_object::object_key));
    }

    pub(crate) fn register_entity_type(&mut self, value: &Value) -> bool {
        let Some(key) = jar_object::instance_key(value) else {
            return false;
        };
        if self.entities.contains_key(key) {
            self.registered.insert(key.to_owned());
            true
        } else {
            false
        }
    }
}

const BUILDER: &str = "net/minecraft/world/entity/EntityType$Builder";
const ENTITY: &str = "net/minecraft/world/entity/EntityType";
const ACCESSOR: &str = "net/minecraft/network/syncher/EntityDataAccessor";
const SERIALIZER: &str = "net/minecraft/network/syncher/EntityDataSerializer";
const MOB_CATEGORY: &str = "net/minecraft/world/entity/MobCategory";
/// `MobCategory`'s constants, in ordinal order (26.3).
const CATEGORIES: [&str; 8] = [
    "MONSTER",
    "CREATURE",
    "AMBIENT",
    "AXOLOTLS",
    "UNDERGROUND_WATER_CREATURE",
    "WATER_CREATURE",
    "WATER_AMBIENT",
    "MISC",
];

/// A new object of `class` numbered by the heap; `None` (unanswered) with no allocator.
fn new_object(
    numbers: Option<&mut Numbers<'_>>,
    class: &str,
) -> Option<Result<Value, NativeError>> {
    Some(numbers?.handle(class))
}

/// The static constant `class.name`: the object made on its first read, numbered by the heap
/// then. `None` (unanswered) when it is not made yet and there is no allocator.
fn constant(
    state: &mut State,
    numbers: Option<&mut Numbers<'_>>,
    class: &str,
    name: &str,
) -> Option<Result<Value, NativeError>> {
    let slot = format!("{class}.{name}");
    if let Some(value) = state.constants.get(&slot) {
        return Some(Ok(value.clone()));
    }
    let value = match numbers?.handle(class) {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };
    state.constants.insert(slot, value.clone());
    Some(Ok(value))
}
fn class(value: &Value) -> Option<&str> {
    jar_object::value_marker_class(value)
}
fn key(value: &Value) -> Option<String> {
    jar_object::instance_key(value).map(str::to_owned)
}
fn arg_string(v: &Value) -> Option<String> {
    crate::runtime::host_ids::identifier_key(v)
        .map(str::to_owned)
        .or_else(|| expect_ref(v).cloned())
}
/// `MobCategory.values()`: its constants, in ordinal order, made on first use.
pub(crate) fn enum_values(
    state: &mut State,
    owner: &str,
    mut numbers: Option<&mut Numbers<'_>>,
) -> Option<Result<Vec<Value>, NativeError>> {
    if owner != MOB_CATEGORY {
        return None;
    }
    let constants = CATEGORIES
        .iter()
        .map(|name| category_constant(state, numbers.as_deref_mut(), name))
        .collect::<Option<Vec<_>>>()?;
    Some(constants.into_iter().collect())
}

/// The `MobCategory` constant `name`.
pub(crate) fn category_constant(
    state: &mut State,
    numbers: Option<&mut Numbers<'_>>,
    name: &str,
) -> Option<Result<Value, NativeError>> {
    CATEGORIES.contains(&name).then_some(())?;
    constant(state, numbers, MOB_CATEGORY, name)
}

/// The name of the `MobCategory` constant `value` is, if it is one this model made.
fn category_name(state: &State, value: &Value) -> Option<&'static str> {
    CATEGORIES.iter().copied().find(|name| {
        state
            .constants
            .get(&format!("{MOB_CATEGORY}.{name}"))
            .is_some_and(|constant| constant == value)
    })
}

/// The `EntityDataSerializers` constant `name`, if 26.3 declares it.
fn serializer_handle(
    state: &mut State,
    numbers: Option<&mut Numbers<'_>>,
    name: &str,
) -> Option<Result<Value, NativeError>> {
    let names = [
        "BYTE",
        "INT",
        "LONG",
        "FLOAT",
        "STRING",
        "COMPONENT",
        "OPTIONAL_COMPONENT",
        "ITEM_STACK",
        "BLOCK_STATE",
        "OPTIONAL_BLOCK_STATE",
        "BOOLEAN",
        "PARTICLE",
        "PARTICLES",
        "ROTATIONS",
        "BLOCK_POS",
        "OPTIONAL_BLOCK_POS",
        "DIRECTION",
        "OPTIONAL_LIVING_ENTITY_REFERENCE",
        "POSE",
        "CAT_VARIANT",
        "FROG_VARIANT",
        "WOLF_VARIANT",
        "WOLF_SOUND_VARIANT",
        "PIG_VARIANT",
        "PIG_SOUND_VARIANT",
        "COW_VARIANT",
        "COW_SOUND_VARIANT",
        "CHICKEN_VARIANT",
        "CHICKEN_SOUND_VARIANT",
        "ARMADILLO_STATE",
        "SNIFFER_STATE",
        "WEATHERING_COPPER_STATE",
        "COPPER_GOLEM_STATE",
        "VECTOR3",
        "QUATERNION",
        "RESOLVABLE_PROFILE",
        "HUMANOID_ARM",
        "DYE_COLOR",
        "VILLAGER_DATA",
        "OPTIONAL_UNSIGNED_INT",
    ];
    names.contains(&name).then_some(())?;
    constant(state, numbers, SERIALIZER, name)
}

fn category(state: &State, v: &Value) -> Option<String> {
    category_name(state, v).map(str::to_owned).or_else(|| {
        expect_ref(v)
            .and_then(|s| s.strip_prefix("enum/net/minecraft/world/entity/MobCategory/"))
            .map(str::to_owned)
    })
}

/// A `getstatic` of a `MobCategory` or `EntityDataSerializers` constant: the one object for
/// it, made (with `numbers`) on its first read.
pub(crate) fn static_field(
    state: &mut State,
    r: &MethodRef,
    numbers: Option<&mut Numbers<'_>>,
) -> Option<Result<Value, NativeError>> {
    if r.owner == MOB_CATEGORY && r.descriptor == "Lnet/minecraft/world/entity/MobCategory;" {
        return category_constant(state, numbers, &r.name);
    }
    if r.owner == "net/minecraft/network/syncher/EntityDataSerializers"
        && r.descriptor == "Lnet/minecraft/network/syncher/EntityDataSerializer;"
    {
        let names = [
            "BYTE",
            "INT",
            "LONG",
            "FLOAT",
            "STRING",
            "COMPONENT",
            "OPTIONAL_COMPONENT",
            "ITEM_STACK",
            "BLOCK_STATE",
            "OPTIONAL_BLOCK_STATE",
            "OPTIONAL_GLOBAL_POS",
            "BOOLEAN",
            "PARTICLE",
            "PARTICLES",
            "ROTATIONS",
            "BLOCK_POS",
            "OPTIONAL_BLOCK_POS",
            "DIRECTION",
            "OPTIONAL_LIVING_ENTITY_REFERENCE",
            "POSE",
            "CAT_VARIANT",
            "CAT_SOUND_VARIANT",
            "FROG_VARIANT",
            "WOLF_VARIANT",
            "WOLF_SOUND_VARIANT",
            "PIG_VARIANT",
            "PIG_SOUND_VARIANT",
            "COW_VARIANT",
            "COW_SOUND_VARIANT",
            "CHICKEN_VARIANT",
            "CHICKEN_SOUND_VARIANT",
            "ARMADILLO_STATE",
            "SNIFFER_STATE",
            "WEATHERING_COPPER_STATE",
            "COPPER_GOLEM_STATE",
            "VECTOR3",
            "QUATERNION",
            "RESOLVABLE_PROFILE",
            "HUMANOID_ARM",
            "DYE_COLOR",
            "VILLAGER_DATA",
            "OPTIONAL_UNSIGNED_INT",
        ];
        if names.iter().any(|n| *n == r.name) {
            return serializer_handle(state, numbers, &r.name);
        }
    }
    None
}

/// `SynchedEntityData.defineId` and `EntityType.Builder.of`/`createNothing`: each a new object,
/// numbered by `numbers`; unanswered without it.
pub(crate) fn static_call(
    state: &mut State,
    r: &MethodRef,
    args: &[Value],
    numbers: Option<&mut Numbers<'_>>,
) -> Option<Result<Value, NativeError>> {
    if r.owner == "net/minecraft/network/syncher/SynchedEntityData" && r.name == "defineId" {
        let class_name = args.first().and_then(crate::runtime::host_class::class_name)?;
        return define_id(state, class_name, numbers);
    }
    if r.owner == BUILDER && (r.name == "of" || r.name == "createNothing") {
        let cat = args
            .get(1)
            .or_else(|| args.first())
            .and_then(|arg| category(state, arg))?;
        let far_from_player = matches!(cat.as_str(), "CREATURE" | "MISC");
        let v = match new_object(numbers, BUILDER)? {
            Ok(v) => v,
            Err(error) => return Some(Err(error)),
        };
        let k = key(&v).unwrap();
        state.builders.insert(
            k,
            Builder {
                category: cat,
                width: 0.6,
                height: 1.8,
                fire_immune: false,
                summon: true,
                serialize: true,
                far_from_player,
                peaceful: true,
                track_deltas: true,
                eye_height: 1.53,
                tracking: 5,
                interval: 3,
            },
        );
        return Some(Ok(v));
    }
    None
}

/// An instance call on one of this model's objects. `build`, `getDimensions` and
/// `getCategory` hand out an object, numbered by `numbers`; without it they are unanswered.
pub(crate) fn instance_call(
    state: &mut State,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
    numbers: Option<&mut Numbers<'_>>,
) -> Option<Result<Value, NativeError>> {
    let marker = class(receiver)?;
    let k = key(receiver)?;
    if marker == ACCESSOR && r.name == "id" && r.descriptor == "()I" {
        return Some(Ok(Value::Int(accessor_id(state, receiver)?)));
    }
    if marker == BUILDER {
        if r.name == "build" {
            // No allocator: unanswered, and the builder stays.
            let numbers = numbers?;
            if !state.builders.contains_key(&k) {
                return None;
            }
            let id = args
                .first()
                .and_then(crate::runtime::host_ids::key_location)
                .and_then(|v| crate::runtime::host_ids::identifier_key(&v).map(str::to_owned))
                .or_else(|| args.first().and_then(arg_string))?;
            let v = match numbers.handle(ENTITY) {
                Ok(v) => v,
                Err(error) => return Some(Err(error)),
            };
            let mut b = state.builders.remove(&k)?;
            let ek = key(&v).unwrap();
            b.width = b.width.max(0.0);
            b.height = b.height.max(0.0);
            state.entities.insert(
                ek,
                Entity {
                    category: b.category,
                    width: b.width,
                    height: b.height,
                    fire_immune: b.fire_immune,
                    summon: b.summon,
                    serialize: b.serialize,
                    far_from_player: b.far_from_player,
                    peaceful: b.peaceful,
                    track_deltas: b.track_deltas,
                    eye_height: b.eye_height,
                    tracking: b.tracking,
                    interval: b.interval,
                    description: format!("entity.{}", id.replace(':', ".")),
                },
            );
            return Some(Ok(v));
        }
        let b = state.builders.get_mut(&k)?;
        match r.name.as_str() {
            "sized" if args.len() >= 2 => {
                b.width = expect_float(r, &args[0]).ok()?;
                b.height = expect_float(r, &args[1]).ok()?;
                b.eye_height = b.height * 0.85;
            }
            "clientTrackingRange" => b.tracking = expect_int(r, args.first()?).ok()?,
            "updateInterval" => b.interval = expect_int(r, args.first()?).ok()?,
            "fireImmune" => b.fire_immune = true,
            "noSummon" => b.summon = false,
            "noSave" => b.serialize = false,
            "canSpawnFarFromPlayer" => b.far_from_player = true,
            "notInPeaceful" => b.peaceful = false,
            "dontTrackDeltas" => b.track_deltas = false,
            "eyeHeight" => b.eye_height = expect_float(r, args.first()?).ok()?,
            "noUpdateInterval" => b.interval = i32::MAX,
            _ => return None,
        }
        return Some(Ok(receiver.clone()));
    }
    if marker == "net/minecraft/world/entity/EntityDimensions" {
        let (width, height, eye_height) = state.dimensions.get(&k)?;
        return Some(Ok(match r.name.as_str() {
            "width" => Value::Float(*width),
            "height" => Value::Float(*height),
            "eyeHeight" => Value::Float(*eye_height),
            _ => return None,
        }));
    }
    if marker == MOB_CATEGORY {
        let name = category_name(state, receiver)?;
        let (max, friendly, persistent, despawn) = match name {
            "MONSTER" => (70, false, false, 128),
            "CREATURE" => (10, true, true, 128),
            "AMBIENT" => (15, true, false, 128),
            "AXOLOTLS" => (5, true, false, 128),
            "UNDERGROUND_WATER_CREATURE" => (5, true, false, 128),
            "WATER_CREATURE" => (5, true, false, 128),
            "WATER_AMBIENT" => (20, true, false, 64),
            "MISC" => (-1, true, true, 128),
            _ => return None,
        };
        return Some(Ok(match r.name.as_str() {
            "name" => Value::Ref(Some(name.to_owned())),
            "getName" | "getSerializedName" => Value::Ref(Some(name.to_ascii_lowercase())),
            "toString" => Value::Ref(Some(name.to_owned())),
            "getDebugAbbreviation" => Value::Ref(Some(
                match name {
                    "MONSTER" => "MO",
                    "CREATURE" => "C",
                    "AMBIENT" => "AM",
                    "AXOLOTLS" => "AX",
                    "UNDERGROUND_WATER_CREATURE" => "UWC",
                    "WATER_CREATURE" => "WC",
                    "WATER_AMBIENT" => "WA",
                    _ => "MI",
                }
                .into(),
            )),
            "getMaxInstancesPerChunk" => Value::Int(max),
            "isFriendly" => Value::Int(friendly as i32),
            "isPersistent" => Value::Int(persistent as i32),
            "getDespawnDistance" => Value::Int(despawn),
            "getNoDespawnDistance" => Value::Int(32),
            _ => return None,
        }));
    }
    if marker == ENTITY {
        if !state.registered.contains(&k) {
            return None;
        }
        let e = state.entities.get(&k)?;
        let (entity_width, entity_height, entity_eye_height) = (e.width, e.height, e.eye_height);
        if r.name == "getCategory" {
            let category = e.category.clone();
            return category_constant(state, numbers, &category);
        }
        let out = match (r.name.as_str(), r.descriptor.as_str()) {
            ("canSerialize", "()Z") => Value::Int(e.serialize as i32),
            ("canSpawnFarFromPlayer", "()Z") => Value::Int(e.far_from_player as i32),
            ("isAllowedInPeaceful", "()Z") => Value::Int(e.peaceful as i32),
            ("trackDeltas", "()Z") => Value::Int(e.track_deltas as i32),
            ("clientTrackingRange", "()I") => Value::Int(e.tracking),
            ("updateInterval", "()I") => Value::Int(e.interval),
            ("fireImmune", "()Z") => Value::Int(e.fire_immune as i32),
            ("canSummon", "()Z") => Value::Int(e.summon as i32),
            ("getDescriptionId", "()Ljava/lang/String;") | ("toString", "()Ljava/lang/String;") => {
                Value::Ref(Some(e.description.clone()))
            }
            ("getDimensions", _) => {
                let v = match new_object(numbers, "net/minecraft/world/entity/EntityDimensions")? {
                    Ok(v) => v,
                    Err(error) => return Some(Err(error)),
                };
                let dk = key(&v).unwrap();
                state
                    .dimensions
                    .insert(dk, (entity_width, entity_height, entity_eye_height));
                return Some(Ok(v));
            }
            ("getWidth", "()F") => Value::Float(e.width),
            ("getHeight", "()F") => Value::Float(e.height),
            _ => return None,
        };
        return Some(Ok(out));
    }
    None
}

fn pool_last(state: &State, class: &str) -> Option<u16> {
    if let Some(last) = state.ids.get(class) {
        return Some(*last);
    }
    let (_parent, max) = crate::runtime::vanilla_entity_data_pool::row(class)?;
    max
}

pub(crate) fn note_hierarchy(state: &mut State, entries: &[(String, String)]) {
    state.supers.extend(entries.iter().cloned());
}

/// `SynchedEntityData.defineId(class, …)`: the next data id of `class`, on a new
/// `EntityDataAccessor` (numbered by `numbers`; unanswered without it, and no id is taken).
pub(crate) fn define_id(
    state: &mut State,
    class_name: &str,
    numbers: Option<&mut Numbers<'_>>,
) -> Option<Result<Value, NativeError>> {
    let mut current = class_name.to_owned();
    let id = loop {
        if let Some(last) = pool_last(state, &current) {
            break last.saturating_add(1);
        }
        if current == "net/minecraft/world/entity/Entity" {
            break 0;
        }
        if let Some((table_parent, _)) = crate::runtime::vanilla_entity_data_pool::row(&current) {
            current = table_parent?.to_owned();
        } else {
            if current.starts_with("net/minecraft/") {
                return None;
            }
            let Some(parent) = state.supers.get(&current) else {
                break 0;
            };
            current = parent.clone();
        }
    };
    if id > 254 {
        return Some(Err(NativeError::UncaughtException {
            class: "java/lang/IllegalArgumentException".into(),
            message: Some(format!("Data value id is too big with {id}! (Max is 254)")),
            frame: None,
        }));
    }
    // A new object per call, as `new EntityDataAccessor<>(id, serializer)` is on 26.3: two
    // holders' accessors with one id are two objects.
    let accessor = match new_object(numbers, ACCESSOR)? {
        Ok(accessor) => accessor,
        Err(error) => return Some(Err(error)),
    };
    state.ids.insert(class_name.to_owned(), id);
    state.accessors.insert(key(&accessor).unwrap(), id);
    Some(Ok(accessor))
}

/// The data id of an accessor this model made.
pub(crate) fn accessor_id(state: &State, value: &Value) -> Option<i32> {
    (class(value) == Some(ACCESSOR))
        .then(|| state.accessors.get(jar_object::instance_key(value)?))
        .flatten()
        .map(|id| i32::from(*id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_subclass_pool_rows_match_java25() {
        let mut state = State::default();
        note_hierarchy(
            &mut state,
            &[
                (
                    "demo/Child".into(),
                    "net/minecraft/world/entity/Entity".into(),
                ),
                (
                    "demo/Sibling".into(),
                    "net/minecraft/world/entity/Entity".into(),
                ),
                ("demo/Grandchild".into(), "demo/Child".into()),
                (
                    "demo/ProjectileChild".into(),
                    "net/minecraft/world/entity/projectile/Projectile".into(),
                ),
                (
                    "demo/PathfinderChild".into(),
                    "net/minecraft/world/entity/PathfinderMob".into(),
                ),
                (
                    "demo/TamableChild".into(),
                    "net/minecraft/world/entity/TamableAnimal".into(),
                ),
            ],
        );
        let rows = [
            ("child", "demo/Child"),
            ("sibling", "demo/Sibling"),
            ("grandchild", "demo/Grandchild"),
            ("child2", "demo/Child"),
            ("projectile", "demo/ProjectileChild"),
            ("pathfinder", "demo/PathfinderChild"),
            ("tamable", "demo/TamableChild"),
        ];
        let mut next = 0;
        let mut numbers = Numbers::new(&mut next);
        let captured = rows
            .iter()
            .map(|(label, class)| {
                let id = define_id(&mut state, class, Some(&mut numbers))
                    .unwrap()
                    .unwrap();
                format!("{label}={}", accessor_id(&state, &id).unwrap())
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(captured, crate::runtime::vendored_vanilla_entity_data_subclass_txt::VENDORED_DATA);
    }

    #[test]
    fn unlisted_vanilla_ancestor_is_counted() {
        let mut state = State::default();
        note_hierarchy(
            &mut state,
            &[(
                "demo/Unknown".into(),
                "net/minecraft/world/entity/Unknown".into(),
            )],
        );
        let mut next = 0;
        assert!(
            define_id(
                &mut state,
                "demo/Unknown",
                Some(&mut Numbers::new(&mut next))
            )
            .is_none()
        );
    }

    #[test]
    fn builder_and_accessor_surface_keeps_vanilla_defaults() {
        let mut state = State::default();
        let mut next = 0;
        let mut numbers = Numbers::new(&mut next);
        let category = category_constant(&mut state, Some(&mut numbers), "CREATURE")
            .unwrap()
            .unwrap();
        let b = static_call(&mut state, &MethodRef { owner: BUILDER.into(), name: "of".into(), descriptor: "(Lnet/minecraft/world/entity/EntityType$EntityFactory;Lnet/minecraft/world/entity/MobCategory;)Lnet/minecraft/world/entity/EntityType$Builder;".into() }, &[Value::Ref(Some("factory".into())), category], Some(&mut numbers)).unwrap().unwrap();
        instance_call(
            &mut state,
            &MethodRef {
                owner: BUILDER.into(),
                name: "sized".into(),
                descriptor: "(FF)Lnet/minecraft/world/entity/EntityType$Builder;".into(),
            },
            &b,
            &[Value::Float(1.25), Value::Float(2.5)],
            None,
        )
        .unwrap()
        .unwrap();
        let id = define_id(
            &mut state,
            "net/minecraft/world/entity/Entity",
            Some(&mut numbers),
        )
        .unwrap()
        .unwrap();
        assert_eq!(accessor_id(&state, &id), Some(8));
        // Every object is numbered by the one allocator, from where it stood.
        assert_eq!(next, 3);
    }
}
