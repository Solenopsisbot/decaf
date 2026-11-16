//! The Guava surface a mod's bootstrap reaches through the copy the game
//! ships: `Preconditions`, `TypeToken`, `CacheBuilder`/`LoadingCache`,
//! `HashMultimap` and friends, and `Sets.immutableEnumSet`.
//!
//! Guava is a platform library on the native path (the game provides it, a
//! mod does not bundle it), so its calls reached the host, which answered
//! null: `checkNotNull(x)` lost `x`, a cache builder was null, and WorldEdit's
//! `Key.of(TypeToken.of(String.class))` failed `Null typeToken`. The answers
//! here are Guava's documented behaviour over the host's own collections.
//!
//! * A `TypeToken` is `typetoken/<Type marker>` (the class name or `type/`
//!   node the reflection surface uses), or a jar subclass object
//!   (`new TypeToken<List<String>>() {}`) whose captured type is read from
//!   its class `Signature` when the super constructor runs. Equality and the
//!   hash are the type's.
//! * A cache is a host map; a `LoadingCache` also keeps its loader, and a miss
//!   runs `CacheLoader.load` (or the function `CacheLoader.from` wrapped) and
//!   stores the answer. Eviction settings are accepted and have no effect, as
//!   nothing in a bootstrap fills a cache past them.
//! * A multimap is a host map from each key to a host set (`HashMultimap`,
//!   `LinkedHashMultimap`) or list (`ArrayListMultimap`); `get` hands out that
//!   collection, so adding to it writes through as Guava's view does.

use super::*;

const TYPE_TOKEN: &str = "com/google/common/reflect/TypeToken";
/// The captured type a `TypeToken` subclass object keeps, under its marker.
const RUNTIME_TYPE: &str = "#runtimeTypeLjava/lang/reflect/Type;";

/// The concrete JDK class returned by Guava's no-argument collection
/// factories, as constructed by Guava 33.6.0.
pub(crate) fn empty_factory_class(r: &MethodRef) -> Option<&'static str> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        ("com/google/common/collect/Maps", "newHashMap", "()Ljava/util/HashMap;") => {
            Some("java/util/HashMap")
        }
        ("com/google/common/collect/Maps", "newLinkedHashMap", "()Ljava/util/LinkedHashMap;") => {
            Some("java/util/LinkedHashMap")
        }
        (
            "com/google/common/collect/Maps",
            "newIdentityHashMap",
            "()Ljava/util/IdentityHashMap;",
        ) => Some("java/util/IdentityHashMap"),
        (
            "com/google/common/collect/Maps",
            "newConcurrentMap",
            "()Ljava/util/concurrent/ConcurrentMap;",
        ) => Some("java/util/concurrent/ConcurrentHashMap"),
        ("com/google/common/collect/Lists", "newArrayList", "()Ljava/util/ArrayList;") => {
            Some("java/util/ArrayList")
        }
        ("com/google/common/collect/Lists", "newLinkedList", "()Ljava/util/LinkedList;") => {
            Some("java/util/LinkedList")
        }
        ("com/google/common/collect/Sets", "newHashSet", "()Ljava/util/HashSet;") => {
            Some("java/util/HashSet")
        }
        ("com/google/common/collect/Sets", "newLinkedHashSet", "()Ljava/util/LinkedHashSet;") => {
            Some("java/util/LinkedHashSet")
        }
        ("com/google/common/collect/Sets", "newConcurrentHashSet", "()Ljava/util/Set;") => {
            Some("java/util/concurrent/ConcurrentHashMap$KeySetView")
        }
        ("com/google/common/collect/Sets", "newIdentityHashSet", "()Ljava/util/Set;") => {
            Some("java/util/Collections$SetFromMap")
        }
        _ => None,
    }
}

/// Exact Guava expected-size/capacity factories. Negative arguments are
/// checked before constructing the JDK collection, with Guava's message.
pub(crate) fn sized_factory_class(
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<&'static str, NativeError>> {
    let (class, label) = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (
            "com/google/common/collect/Maps",
            "newHashMapWithExpectedSize",
            "(I)Ljava/util/HashMap;",
        ) => ("java/util/HashMap", "expectedSize"),
        (
            "com/google/common/collect/Maps",
            "newLinkedHashMapWithExpectedSize",
            "(I)Ljava/util/LinkedHashMap;",
        ) => ("java/util/LinkedHashMap", "expectedSize"),
        (
            "com/google/common/collect/Lists",
            "newArrayListWithCapacity",
            "(I)Ljava/util/ArrayList;",
        ) => ("java/util/ArrayList", "initialArraySize"),
        (
            "com/google/common/collect/Lists",
            "newArrayListWithExpectedSize",
            "(I)Ljava/util/ArrayList;",
        ) => ("java/util/ArrayList", "arraySize"),
        (
            "com/google/common/collect/Sets",
            "newHashSetWithExpectedSize",
            "(I)Ljava/util/HashSet;",
        ) => ("java/util/HashSet", "expectedSize"),
        (
            "com/google/common/collect/Sets",
            "newLinkedHashSetWithExpectedSize",
            "(I)Ljava/util/LinkedHashSet;",
        ) => ("java/util/LinkedHashSet", "expectedSize"),
        _ => return None,
    };
    let size = match args.first() {
        Some(Value::Int(size)) => *size,
        _ => {
            return Some(Err(NativeError::Metadata(
                "Guava size argument is not int".into(),
            )));
        }
    };
    if size < 0 {
        return Some(Err(NativeError::UncaughtException {
            class: "java/lang/IllegalArgumentException".into(),
            message: Some(format!("{label} cannot be negative but was: {size}")),
            frame: None,
        }));
    }
    let backing = if class == "java/util/ArrayList" {
        if label == "arraySize" {
            (size as u128)
                .saturating_add(5)
                .saturating_add((size / 10) as u128)
        } else {
            size as u128
        }
    } else {
        // Guava's Maps.capacity feeds HashMap's tableSizeFor.  The table
        // starts at one for expected sizes below three, then ceil(n/.75),
        // and caps at the JDK's maximum table size.
        let capacity = if size < 3 {
            (size + 1) as u128
        } else if size < (1 << 30) {
            ((size as u128) * 4).div_ceil(3)
        } else {
            1u128 << 30
        };
        capacity.next_power_of_two()
    };
    if backing > crate::runtime::MAX_ARRAY_ELEMENTS as u128 {
        return Some(Err(crate::runtime::array_bound_error(backing)));
    }
    Some(Ok(class))
}

fn map_ref(name: &str, descriptor: &str) -> MethodRef {
    MethodRef {
        owner: "java/util/Map".into(),
        name: name.into(),
        descriptor: descriptor.into(),
    }
}

fn collection_ref(owner: &str, name: &str, descriptor: &str) -> MethodRef {
    MethodRef {
        owner: owner.into(),
        name: name.into(),
        descriptor: descriptor.into(),
    }
}

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

/// The box class of a primitive class marker, for `TypeToken.wrap`.
fn boxed_class(primitive: &str) -> Option<&'static str> {
    Some(match primitive {
        "boolean" => "java/lang/Boolean",
        "byte" => "java/lang/Byte",
        "char" => "java/lang/Character",
        "short" => "java/lang/Short",
        "int" => "java/lang/Integer",
        "long" => "java/lang/Long",
        "float" => "java/lang/Float",
        "double" => "java/lang/Double",
        "void" => "java/lang/Void",
        _ => return None,
    })
}

const BIMAP_INVERSE_FIELD: &str = "|guava#inverse";
const BIMAP_KIND_FIELD: &str = "|guava#kind";
const BIMAP_VIEW_FIELD: &str = "|guava#view";
const BIMAP_VIEW_SOURCE_FIELD: &str = "|guava#view-source";
const BIMAP_ENTRY_FIELD: &str = "|guava#entry";
const BIMAP_ITERATOR_PREFIX: &str = "|guava#bimap-iterator|";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BimapKind {
    Root,
    Inverse,
    Keys,
    Values,
    Entries,
}

impl BimapKind {
    fn tag(self) -> i32 {
        match self {
            Self::Root => 0,
            Self::Inverse => 1,
            Self::Keys => 2,
            Self::Values => 4,
            Self::Entries => 3,
        }
    }

    fn from_tag(tag: i32) -> Option<Self> {
        Some(match tag {
            0 => Self::Root,
            1 => Self::Inverse,
            2 => Self::Keys,
            3 => Self::Entries,
            4 => Self::Values,
            _ => return None,
        })
    }
}

fn bimap_method(name: &str, descriptor: &str) -> MethodRef {
    MethodRef {
        owner: "java/util/Map".into(),
        name: name.into(),
        descriptor: descriptor.into(),
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The Guava calls this module answers, or `None`.
    pub(super) fn invoke_guava(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if (r.owner == "com/google/common/collect/ImmutableMap"
            || r.owner == "com/google/common/collect/ImmutableBiMap")
            && r.name == "copyOf"
            && (r.descriptor.starts_with("(Ljava/util/Map;")
                || r.descriptor.starts_with("(Ljava/lang/Iterable;"))
        {
            return self.guava_immutable_map_copy(r, args);
        }
        if let Some(receiver) = receiver {
            if self.bimap_entry_source(receiver).is_some() {
                return self.invoke_bimap_entry(r, receiver, args);
            }
            if self.bimap_iterator_source(receiver).is_some() {
                return self.invoke_bimap_iterator(r, receiver, args);
            }
            if self.bimap_handle_info(receiver).is_some() {
                return self.invoke_bimap(r, Some(receiver), args);
            }
        }
        if let Some(result) = sized_factory_class(r, args) {
            let class = result?;
            return Ok(self.host.empty_mutable_collection(class));
        }
        if let Some(class) = empty_factory_class(r) {
            return Ok(self.host.empty_mutable_collection(class));
        }
        if let Some(value) = crate::runtime::guava_multimap::invoke(self, r, receiver, args)? {
            return Ok(Some(value));
        }
        if let Some((class, values, map)) = self.collection_factory_values(r, args)? {
            let collection = self.host.empty_mutable_collection(class).ok_or_else(|| {
                NativeError::Metadata("Guava collection factory has no host collection".into())
            })?;
            let (owner, name, descriptor) = if map {
                (
                    "java/util/Map",
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                )
            } else {
                ("java/util/Collection", "add", "(Ljava/lang/Object;)Z")
            };
            if map {
                for pair in values.as_chunks::<2>().0 {
                    self.host.invoke_instance(
                        &MethodRef {
                            owner: owner.into(),
                            name: name.into(),
                            descriptor: descriptor.into(),
                        },
                        &collection,
                        pair,
                    )?;
                }
            } else {
                for value in values {
                    self.host.invoke_instance(
                        &MethodRef {
                            owner: owner.into(),
                            name: name.into(),
                            descriptor: descriptor.into(),
                        },
                        &collection,
                        &[value],
                    )?;
                }
            }
            return Ok(Some(collection));
        }
        if let Some(value) = self.invoke_memoized_supplier(r, receiver, args)? {
            return Ok(Some(value));
        }
        if !r.owner.starts_with("com/google/common/")
            && !receiver.is_some_and(|token| crate::runtime::host_type_token::token_type(token).is_some())
        {
            // A jar `TypeToken` subclass object answers through its type even
            // when the call site names the subclass.
            // The receiver's own class decides: a member the subclass does
            // not declare is `TypeToken`'s (`toString` through `Object`).
            if let Some(marker) = receiver.and_then(crate::runtime::jar_object::object_key)
                && self
                    .fields
                    .contains_key(&format!("{marker}|{TYPE_TOKEN}{RUNTIME_TYPE}"))
                && let Some(class) = crate::runtime::jar_object::key_class(marker)
                && self
                    .member_owner(class, &r.name, &r.descriptor)
                    .is_some_and(|(owner, jar_owned)| !jar_owned && owner == TYPE_TOKEN)
            {
                return self.invoke_type_token(r, receiver, args);
            }
            return Ok(None);
        }
        match r.owner.as_str() {
            "com/google/common/base/Preconditions" => self.invoke_preconditions(r, args),
            "com/google/common/collect/Sets" | "com/google/common/collect/ImmutableSet"
                if r.name == "immutableEnumSet" =>
            {
                let mut values = Vec::new();
                for (index, value) in args.iter().enumerate() {
                    if index == 0 && !r.descriptor.starts_with("(Ljava/lang/Iterable;") {
                        values.push(value.clone());
                        continue;
                    }
                    match value {
                        array if self.owned_array(array).is_some() => {
                            values.extend(self.array(value)?.into_owned());
                        }
                        other => {
                            let items = runtime_collection_values(&*self.host, other)?;
                            values.extend(items);
                        }
                    }
                }
                if values.iter().any(|value| matches!(value, Value::Ref(None))) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                let mut unique: Vec<Value> = Vec::with_capacity(values.len());
                for value in values {
                    if !unique.contains(&value) {
                        unique.push(value);
                    }
                }
                // An `ImmutableEnumSet` (an `EnumSet` inside) iterates in
                // ordinal order, each constant's own `ordinal()`.
                if unique.len() > 1 {
                    let mut ordered = Vec::with_capacity(unique.len());
                    for value in unique {
                        // Only a jar enum's `ordinal()` is its own: a JDK
                        // enum's (`TimeUnit`) is not modelled, so the factory
                        // is counted (#253 review).
                        let jar_enum = crate::runtime::jar_object::object_class(&value)
                            .map(str::to_owned)
                            .or_else(|| {
                                expect_ref(&value)
                                    .and_then(|marker| marker.strip_prefix("enum/"))
                                    .and_then(|rest| rest.rsplit_once('/'))
                                    .map(|(class, _)| class.to_owned())
                            })
                            .is_some_and(|class| self.class(&class).is_some());
                        if !jar_enum {
                            return self.host.unknown_operand_call(r).map(Some);
                        }
                        let ordinal = self.call_virtual(
                            crate::runtime::jdk_call::Site::Virtual,
                            "java/lang/Enum",
                            "ordinal",
                            "()I",
                            &value,
                            &[],
                        )?;
                        let Value::Int(ordinal) = ordinal else {
                            return self.host.unknown_operand_call(r).map(Some);
                        };
                        ordered.push((ordinal, value));
                    }
                    ordered.sort_by_key(|(ordinal, _)| *ordinal);
                    unique = ordered.into_iter().map(|(_, value)| value).collect();
                }
                Ok(Some(self.factory_or_count(r, "set", unique)?))
            }
            "com/google/common/collect/ImmutableSet"
            | "com/google/common/collect/ImmutableMap"
            | "com/google/common/collect/ImmutableBiMap"
                if r.name == "of" =>
            {
                self.invoke_immutable_of(r, args)
            }
            "com/google/common/collect/ImmutableMap"
                if r.name == "builder"
                    && r.descriptor == "()Lcom/google/common/collect/ImmutableMap$Builder;" =>
            {
                Ok(Some(self.new_object("guava-immutable-map-builder")))
            }
            "com/google/common/collect/ImmutableMap$Builder" => {
                let Some(receiver) = receiver else {
                    return Ok(None);
                };
                let Some(marker) = crate::runtime::jar_object::object_key(receiver).map(str::to_owned)
                else {
                    return Ok(None);
                };
                let map_field = format!("{marker}|guava#builder-map");
                match r.name.as_str() {
                    "put" if args.len() == 2 => {
                        let map = match self.fields.get(&map_field).cloned() {
                            Some(map) => map,
                            None => {
                                let Some(map) = self.host.empty_mutable_collection("map") else {
                                    return Ok(None);
                                };
                                self.fields.insert(map_field.clone(), map.clone());
                                map
                            }
                        };
                        let existing = runtime_collection_values(&*self.host, &map)?;
                        if let Some(index) = existing
                            .iter()
                            .step_by(2)
                            .position(|key| self.java_equals(key, &args[0]).unwrap_or(false))
                        {
                            let old_value = &existing[index * 2 + 1];
                            return Err(thrown(
                                "java/lang/IllegalArgumentException",
                                Some(format!(
                                    "Multiple entries with same key: {}={} and {}={}",
                                    crate::runtime::value_text(&args[0]),
                                    crate::runtime::value_text(&args[1]),
                                    crate::runtime::value_text(&args[0]),
                                    crate::runtime::value_text(old_value),
                                )),
                            ));
                        }
                        self.host.invoke_instance(
                            &MethodRef {
                                owner: "java/util/Map".into(),
                                name: "put".into(),
                                descriptor:
                                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"
                                        .into(),
                            },
                            &map,
                            args,
                        )?;
                        Ok(Some(receiver.clone()))
                    }
                    "build"
                        if args.is_empty()
                            && (r.descriptor == "()Lcom/google/common/collect/ImmutableMap;"
                                || r.descriptor == "()Ljava/util/Map;") =>
                    {
                        // `buildOrThrow`: what `ImmutableMap.of` makes for the
                        // entries put (the shared `RegularImmutableMap.EMPTY`,
                        // `SingletonImmutableBiMap`, `RegularImmutableMap`),
                        // counted on a host that makes none (#253: never a
                        // collection literal, never a JDK `Map.of` class).
                        let values = match self.fields.get(&map_field) {
                            Some(map) => runtime_collection_values(&*self.host, map)?,
                            None => Vec::new(),
                        };
                        let of = MethodRef {
                            owner: "com/google/common/collect/ImmutableMap".into(),
                            name: "of".into(),
                            descriptor: r.descriptor.clone(),
                        };
                        match self
                            .host
                            .factory_collection(&of.owner, &of.name, "map", values)
                        {
                            Some(made) => Ok(Some(made)),
                            None => self.host.unknown_operand_call(r).map(Some),
                        }
                    }
                    _ => Ok(None),
                }
            }
            TYPE_TOKEN => self.invoke_type_token(r, receiver, args),
            "com/google/common/cache/CacheBuilder" => self.invoke_cache_builder(r, receiver, args),
            "com/google/common/cache/CacheLoader" => match (r.name.as_str(), r.descriptor.as_str())
            {
                (
                    "from",
                    "(Lcom/google/common/base/Function;)Lcom/google/common/cache/CacheLoader;"
                    | "(Lcom/google/common/base/Supplier;)Lcom/google/common/cache/CacheLoader;",
                ) => {
                    let Some(function) = args
                        .first()
                        .filter(|function| **function != Value::Ref(None))
                    else {
                        return Err(thrown("java/lang/NullPointerException", None));
                    };
                    Ok(Some(crate::runtime::host_guava::cache_loader(function)))
                }
                // A jar `CacheLoader` subclass runs its own constructor body.
                ("<init>", "()V") => Ok(Some(Value::Void)),
                _ => Ok(None),
            },
            "com/google/common/cache/LoadingCache" | "com/google/common/cache/Cache" => {
                self.invoke_cache(r, receiver, args)
            }
            "com/google/common/collect/HashBiMap"
                if r.name == "create"
                    && r.descriptor == "()Lcom/google/common/collect/HashBiMap;" =>
            {
                let map = self
                    .host
                    .empty_mutable_collection("com/google/common/collect/HashBiMap")
                    .ok_or_else(|| NativeError::Metadata("HashBiMap host missing".into()))?;
                let inverse = self
                    .host
                    .empty_mutable_collection("com/google/common/collect/HashBiMap$Inverse")
                    .ok_or_else(|| {
                        NativeError::Metadata("HashBiMap inverse host missing".into())
                    })?;
                let map_marker = crate::runtime::expect_ref(&map)
                    .cloned()
                    .ok_or_else(|| NativeError::Metadata("HashBiMap marker missing".into()))?;
                let inverse_marker = crate::runtime::expect_ref(&inverse).cloned().ok_or_else(|| {
                    NativeError::Metadata("HashBiMap inverse marker missing".into())
                })?;
                self.fields.insert(
                    format!("{map_marker}{BIMAP_INVERSE_FIELD}"),
                    inverse.clone(),
                );
                self.fields.insert(
                    format!("{inverse_marker}{BIMAP_INVERSE_FIELD}"),
                    map.clone(),
                );
                self.fields.insert(
                    format!("{map_marker}{BIMAP_KIND_FIELD}"),
                    Value::Int(BimapKind::Root.tag()),
                );
                self.fields.insert(
                    format!("{inverse_marker}{BIMAP_KIND_FIELD}"),
                    Value::Int(BimapKind::Inverse.tag()),
                );
                Ok(Some(map))
            }
            "com/google/common/collect/HashMultimap"
            | "com/google/common/collect/LinkedHashMultimap"
            | "com/google/common/collect/ArrayListMultimap"
            | "com/google/common/collect/Multimap"
            | "com/google/common/collect/SetMultimap"
            | "com/google/common/collect/ListMultimap" => self.invoke_multimap(r, receiver, args),
            // `Maps.newConcurrentMap()` is a new `ConcurrentHashMap`. The
            // bounded runtime is single-threaded: no second thread to serve.
            "com/google/common/collect/Maps"
                if r.name == "newConcurrentMap"
                    && r.descriptor == "()Ljava/util/concurrent/ConcurrentMap;" =>
            {
                Ok(Some(self.new_host_collection(
                    "java/util/concurrent/ConcurrentHashMap",
                    Vec::new(),
                    (&r.owner, &r.name, &r.descriptor),
                )?))
            }
            "com/google/common/collect/Maps"
                if r.name == "newIdentityHashMap"
                    && r.descriptor == "()Ljava/util/IdentityHashMap;" =>
            {
                Ok(self
                    .host
                    .empty_mutable_collection("java/util/IdentityHashMap"))
            }
            // `Maps.immutableEnumMap` copies the supplied map into an
            // immutable map. The host map representation already preserves
            // the key/value pairs and lookup contract used by ImmutableMap.
            "com/google/common/collect/Maps"
                if r.name == "immutableEnumMap"
                    && (r.descriptor
                        == "(Ljava/util/Map;)Lcom/google/common/collect/ImmutableMap;"
                        || r.descriptor == "(Ljava/util/Map;)Ljava/util/Map;") =>
            {
                let Some(source) = args.first() else {
                    return Ok(None);
                };
                let values = runtime_collection_values(&*self.host, source)?;
                if !values.len().is_multiple_of(2) {
                    return Ok(None);
                }
                let mut ordered = values;
                for key in ordered.iter().step_by(2) {
                    let known = self.enum_ordinal_of(key).or_else(|| {
                        crate::runtime::expect_ref(key)
                            .and_then(|marker| self.declared_enum_ordinal(marker).ok().flatten())
                            .and_then(|value| match value {
                                Value::Int(ordinal) => Some(ordinal),
                                _ => None,
                            })
                    });
                    if known.is_none() {
                        return Ok(None);
                    }
                }
                let mut pairs = ordered
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|[key, value]| (key.clone(), value.clone()))
                    .collect::<Vec<_>>();
                pairs.sort_by_key(|(key, _)| {
                    self.enum_ordinal_of(key)
                        .or_else(|| {
                            crate::runtime::expect_ref(key)
                                .and_then(|marker| {
                                    self.declared_enum_ordinal(marker).ok().flatten()
                                })
                                .and_then(|value| match value {
                                    Value::Int(ordinal) => Some(ordinal),
                                    _ => None,
                                })
                        })
                        .unwrap_or(i32::MAX)
                });
                ordered.clear();
                for (key, value) in pairs {
                    ordered.extend([key, value]);
                }
                Ok(Some(
                    self.host
                        .immutable_collection("map", ordered)
                        .ok_or_else(|| {
                            NativeError::Metadata("immutable map host missing".into())
                        })?,
                ))
            }
            // Guava's queue factory returns a fresh mutable
            // `ConcurrentLinkedQueue`; the single-threaded host uses its
            // ordinary mutable list table for the same FIFO operations.
            "com/google/common/collect/Queues"
                if r.name == "newConcurrentLinkedQueue"
                    && r.descriptor == "()Ljava/util/concurrent/ConcurrentLinkedQueue;" =>
            {
                Ok(self
                    .host
                    .empty_mutable_collection("java/util/concurrent/ConcurrentLinkedQueue"))
            }
            "com/google/common/collect/Sets"
                if r.name == "newIdentityHashSet" && r.descriptor == "()Ljava/util/Set;" =>
            {
                Ok(self
                    .host
                    .empty_mutable_collection("java/util/Collections$SetFromMap"))
            }
            "com/google/common/collect/MapMaker" => self.invoke_map_maker(r, receiver, args),
            _ => Ok(None),
        }
    }

    fn guava_immutable_map_copy(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(source) = args.first() else {
            return Ok(None);
        };
        if matches!(source, Value::Ref(None)) {
            return Err(thrown("java/lang/NullPointerException", None));
        }
        if let Some(marker) = crate::runtime::expect_ref(source) {
            let class = crate::runtime::jar_object::collection_class(marker);
            let immutable_map = matches!(
                class,
                "com/google/common/collect/RegularImmutableMap"
                    | "com/google/common/collect/SingletonImmutableMap"
            );
            let immutable_bimap = matches!(
                class,
                "com/google/common/collect/RegularImmutableBiMap"
                    | "com/google/common/collect/SingletonImmutableBiMap"
            );
            if (r.owner.ends_with("ImmutableMap") && (immutable_map || immutable_bimap))
                || (r.owner.ends_with("ImmutableBiMap") && immutable_bimap)
            {
                return Ok(Some(source.clone()));
            }
        }
        let values = if r.descriptor.starts_with("(Ljava/lang/Iterable;") {
            let entries = runtime_collection_values(&*self.host, source)?;
            let mut values = Vec::with_capacity(entries.len() * 2);
            for entry in entries {
                if entry == Value::Ref(None) {
                    return Err(thrown(
                        "java/lang/NullPointerException",
                        Some("null".into()),
                    ));
                }
                let key = self.call_virtual(
                    crate::runtime::jdk_call::Site::Interface,
                    "java/util/Map$Entry",
                    "getKey",
                    "()Ljava/lang/Object;",
                    &entry,
                    &[],
                )?;
                let value = self.call_virtual(
                    crate::runtime::jdk_call::Site::Interface,
                    "java/util/Map$Entry",
                    "getValue",
                    "()Ljava/lang/Object;",
                    &entry,
                    &[],
                )?;
                values.extend([key, value]);
            }
            values
        } else {
            runtime_collection_values(&*self.host, source)?
        };
        if !values.len().is_multiple_of(2) {
            return Ok(None);
        }
        let mut prior = Vec::new();
        for pair in values.chunks(2) {
            if matches!(pair[0], Value::Ref(None)) {
                return Err(thrown(
                    "java/lang/NullPointerException",
                    Some(format!(
                        "null key in entry: null={}",
                        crate::runtime::value_text(&pair[1])
                    )),
                ));
            }
            if matches!(pair[1], Value::Ref(None)) {
                return Err(thrown(
                    "java/lang/NullPointerException",
                    Some(format!(
                        "null value in entry: {}=null",
                        crate::runtime::value_text(&pair[0])
                    )),
                ));
            }
            for old in prior.chunks(2) {
                if self.java_equals(&pair[0], &old[0])? {
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        Some(format!(
                            "Multiple entries with same key: {}={} and {}={}",
                            crate::runtime::value_text(&pair[0]),
                            crate::runtime::value_text(&pair[1]),
                            crate::runtime::value_text(&old[0]),
                            crate::runtime::value_text(&old[1])
                        )),
                    ));
                }
            }
            if r.owner.ends_with("ImmutableBiMap")
                && prior
                    .chunks(2)
                    .any(|old| self.java_equals(&pair[1], &old[1]).unwrap_or(false))
            {
                return Err(thrown(
                    "java/lang/IllegalArgumentException",
                    Some(format!(
                        "value already present: {}",
                        crate::runtime::value_text(&pair[1])
                    )),
                ));
            }
            prior.extend_from_slice(pair);
        }
        let made = self
            .host
            .immutable_collection_from(r.owner.as_str(), "copyOf", "map", values)
            .ok_or_else(|| NativeError::Metadata("Guava immutable map host missing".into()))?;
        Ok(Some(made))
    }

    fn bimap_handle_info(&self, value: &Value) -> Option<(String, BimapKind)> {
        let Value::Host(handle) = value else {
            return None;
        };
        if handle.kind() != crate::runtime::host_ref::HostKind::Collection {
            return None;
        }
        let tag = match self
            .fields
            .get(&format!("{}{}", handle.key(), BIMAP_KIND_FIELD))
        {
            Some(Value::Int(tag)) => *tag,
            _ => return None,
        };
        Some((handle.key().to_owned(), BimapKind::from_tag(tag)?))
    }

    fn bimap_view_source(&self, marker: &str) -> Option<Value> {
        self.fields
            .get(&format!("{marker}{BIMAP_VIEW_SOURCE_FIELD}"))
            .cloned()
    }

    fn bimap_entry_source(&self, value: &Value) -> Option<Value> {
        let Value::Host(handle) = value else {
            return None;
        };
        self.fields
            .get(&format!("{}{}", handle.key(), BIMAP_ENTRY_FIELD))
            .cloned()
    }

    fn bimap_view_values(
        &mut self,
        source: &Value,
        kind: BimapKind,
    ) -> Result<Vec<Value>, NativeError> {
        let pairs = self.bimap_pairs(source)?;
        if !pairs.len().is_multiple_of(2) {
            return Err(NativeError::Metadata("HashBiMap has odd entries".into()));
        }
        if matches!(kind, BimapKind::Keys | BimapKind::Values) {
            return Ok(pairs.chunks(2).map(|pair| pair[0].clone()).collect());
        }
        let entries =
            self.host
                .mint_map_entries(source, &pairs, crate::runtime::host_map_entry::EntryMade::Node);
        for entry in &entries {
            if let Value::Host(handle) = entry {
                self.fields.insert(
                    format!("{}{}", handle.key(), BIMAP_ENTRY_FIELD),
                    source.clone(),
                );
            }
        }
        Ok(entries)
    }

    fn bimap_view(
        &mut self,
        receiver: &Value,
        source: &Value,
        kind: BimapKind,
    ) -> Result<Value, NativeError> {
        let marker = crate::runtime::expect_ref(receiver)
            .ok_or_else(|| NativeError::Metadata("HashBiMap view marker missing".into()))?;
        let cache = format!("{marker}{BIMAP_VIEW_FIELD}{}", kind.tag());
        if let Some(view) = self.fields.get(&cache).cloned() {
            self.bimap_sync_view(&view, source, kind)?;
            return Ok(view);
        }
        let values = self.bimap_view_values(source, kind)?;
        let view = self
            .host
            .empty_mutable_collection("java/util/LinkedHashSet")
            .ok_or_else(|| NativeError::Metadata("HashBiMap view host missing".into()))?;
        self.host.replace_mutable_collection(&view, values);
        let view_marker = crate::runtime::expect_ref(&view)
            .cloned()
            .ok_or_else(|| NativeError::Metadata("HashBiMap view handle missing".into()))?;
        self.fields.insert(cache, view.clone());
        self.fields.insert(
            format!("{view_marker}{BIMAP_KIND_FIELD}"),
            Value::Int(kind.tag()),
        );
        self.fields.insert(
            format!("{view_marker}{BIMAP_VIEW_SOURCE_FIELD}"),
            source.clone(),
        );
        Ok(view)
    }

    fn bimap_sync_view(
        &mut self,
        view: &Value,
        source: &Value,
        kind: BimapKind,
    ) -> Result<(), NativeError> {
        let values = self.bimap_view_values(source, kind)?;
        self.host.replace_mutable_collection(view, values);
        Ok(())
    }

    fn bimap_view_contains(
        &mut self,
        source: &Value,
        kind: BimapKind,
        wanted: &Value,
    ) -> Result<bool, NativeError> {
        let pairs = self.bimap_pairs(source)?;
        for pair in pairs.chunks(2) {
            if matches!(kind, BimapKind::Keys | BimapKind::Values)
                && self.java_equals(&pair[0], wanted)?
            {
                return Ok(true);
            }
            if kind == BimapKind::Entries
                && let Some((key, value)) = self.host.map_entry_parts(wanted)
                && self.java_equals(&pair[0], &key)?
                && self.java_equals(&pair[1], &value)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn bimap_view_remove(
        &mut self,
        source: &Value,
        kind: BimapKind,
        wanted: &Value,
    ) -> Result<bool, NativeError> {
        let pairs = self.bimap_pairs(source)?;
        let wanted_entry = (kind == BimapKind::Entries)
            .then(|| self.host.map_entry_parts(wanted))
            .flatten();
        let index = pairs.chunks(2).position(|pair| {
            if matches!(kind, BimapKind::Keys | BimapKind::Values) {
                self.java_equals(&pair[0], wanted).unwrap_or(false)
            } else if let Some((key, value)) = &wanted_entry {
                self.java_equals(&pair[0], key).unwrap_or(false)
                    && self.java_equals(&pair[1], value).unwrap_or(false)
            } else {
                false
            }
        });
        let Some(index) = index else {
            return Ok(false);
        };
        let old_key = pairs[index * 2].clone();
        let old_value = pairs[index * 2 + 1].clone();
        let partner = self
            .bimap_inverse(
                crate::runtime::expect_ref(source)
                    .ok_or_else(|| NativeError::Metadata("HashBiMap source missing".into()))?,
            )
            .ok_or_else(|| NativeError::Metadata("HashBiMap inverse missing".into()))?;
        self.bimap_host_remove(source, &old_key)?;
        self.bimap_host_remove(&partner, &old_value)?;
        Ok(true)
    }

    fn bimap_view_clear(&mut self, source: &Value) -> Result<(), NativeError> {
        let partner = self
            .bimap_inverse(
                crate::runtime::expect_ref(source)
                    .ok_or_else(|| NativeError::Metadata("HashBiMap source missing".into()))?,
            )
            .ok_or_else(|| NativeError::Metadata("HashBiMap inverse missing".into()))?;
        self.bimap_host_clear(source)?;
        self.bimap_host_clear(&partner)?;
        Ok(())
    }

    fn bimap_view_item_equals(
        &mut self,
        kind: BimapKind,
        left: &Value,
        right: &Value,
    ) -> Result<bool, NativeError> {
        if matches!(kind, BimapKind::Keys | BimapKind::Values) {
            return self.java_equals(left, right);
        }
        let Some((left_key, left_value)) = self.host.map_entry_parts(left) else {
            return Ok(false);
        };
        let Some((right_key, right_value)) = self.host.map_entry_parts(right) else {
            return Ok(false);
        };
        Ok(self.java_equals(&left_key, &right_key)?
            && self.java_equals(&left_value, &right_value)?)
    }

    fn bimap_view_bulk(
        &mut self,
        source: &Value,
        kind: BimapKind,
        other: &Value,
        retain: bool,
    ) -> Result<bool, NativeError> {
        let wanted = crate::runtime::runtime_collection_values(&*self.host, other)?;
        let current = self.bimap_view_values(source, kind)?;
        let mut removed = Vec::new();
        let mut removed_entries = Vec::new();
        for item in current.iter().cloned() {
            let present = wanted.iter().any(|candidate| {
                self.bimap_view_item_equals(kind, &item, candidate)
                    .unwrap_or(false)
            });
            if present != retain {
                if kind == BimapKind::Entries {
                    if let Some((key, value)) = self.host.map_entry_parts(&item) {
                        removed_entries.push((key, value));
                    }
                } else {
                    removed.push(item);
                }
            }
        }
        let changed = !removed.is_empty() || !removed_entries.is_empty();
        for item in removed {
            self.bimap_view_remove(source, kind, &item)?;
        }
        for (key, value) in removed_entries {
            let pairs = self.bimap_pairs(source)?;
            let Some(index) = pairs.chunks(2).position(|pair| {
                self.java_equals(&pair[0], &key).unwrap_or(false)
                    && self.java_equals(&pair[1], &value).unwrap_or(false)
            }) else {
                continue;
            };
            let old_key = pairs[index * 2].clone();
            let old_value = pairs[index * 2 + 1].clone();
            let partner = self
                .bimap_inverse(
                    crate::runtime::expect_ref(source)
                        .ok_or_else(|| NativeError::Metadata("HashBiMap source missing".into()))?,
                )
                .ok_or_else(|| NativeError::Metadata("HashBiMap inverse missing".into()))?;
            self.bimap_host_remove(source, &old_key)?;
            self.bimap_host_remove(&partner, &old_value)?;
        }
        Ok(changed)
    }

    fn invoke_bimap_view(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
        source: &Value,
        kind: BimapKind,
    ) -> Result<Option<Value>, NativeError> {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("size", "()I") => {
                let values = self.bimap_view_values(source, kind)?;
                return Ok(Some(Value::Int(values.len() as i32)));
            }
            ("isEmpty", "()Z") => {
                let values = self.bimap_view_values(source, kind)?;
                return Ok(Some(Value::Int(values.is_empty() as i32)));
            }
            ("contains", "(Ljava/lang/Object;)Z") => {
                let wanted = args.first().cloned().unwrap_or(Value::Ref(None));
                return Ok(Some(Value::Int(
                    self.bimap_view_contains(source, kind, &wanted)? as i32,
                )));
            }
            ("remove", "(Ljava/lang/Object;)Z") => {
                let wanted = args.first().cloned().unwrap_or(Value::Ref(None));
                return Ok(Some(Value::Int(
                    self.bimap_view_remove(source, kind, &wanted)? as i32,
                )));
            }
            ("removeAll", "(Ljava/util/Collection;)Z") => {
                let other = args.first().cloned().unwrap_or(Value::Ref(None));
                return Ok(Some(Value::Int(
                    self.bimap_view_bulk(source, kind, &other, false)? as i32,
                )));
            }
            ("retainAll", "(Ljava/util/Collection;)Z") => {
                let other = args.first().cloned().unwrap_or(Value::Ref(None));
                return Ok(Some(Value::Int(
                    self.bimap_view_bulk(source, kind, &other, true)? as i32,
                )));
            }
            ("clear", "()V") => {
                self.bimap_view_clear(source)?;
                return Ok(Some(Value::Void));
            }
            ("iterator", "()Ljava/util/Iterator;") => {
                let values = self.bimap_view_values(source, kind)?;
                let view_marker = crate::runtime::expect_ref(receiver).cloned().ok_or_else(|| {
                    NativeError::Metadata("HashBiMap iterator view missing".into())
                })?;
                let iterator = self
                    .host
                    .new_host_iterator(
                        values,
                        Some(format!(
                            "{BIMAP_ITERATOR_PREFIX}{view_marker}|generation={}",
                            self.host
                                .collection_generation(
                                    crate::runtime::expect_ref(source).map_or("", String::as_str)
                                )
                                .unwrap_or(0)
                        )),
                    )
                    .ok_or_else(|| {
                        NativeError::Metadata("HashBiMap iterator host missing".into())
                    })?;
                return Ok(Some(iterator));
            }
            _ => {}
        }
        if matches!(
            r.name.as_str(),
            "add" | "addAll" | "removeAll" | "retainAll" | "removeIf"
        ) {
            return self.host.unknown_operand_call(r).map(Some);
        }
        self.bimap_sync_view(receiver, source, kind)?;
        Ok(None)
    }

    fn bimap_iterator_source(&self, value: &Value) -> Option<(String, u64, bool)> {
        let marker = crate::runtime::host_iterator::marker(value)?;
        let source = self.host.host_iterator(marker)?.2?;
        let rest = source.strip_prefix(BIMAP_ITERATOR_PREFIX)?;
        let removed = rest.ends_with("|removed");
        let rest = rest.strip_suffix("|removed").unwrap_or(rest);
        let (view, generation) = rest.rsplit_once("|generation=")?;
        Some((view.to_owned(), generation.parse().ok()?, removed))
    }

    fn invoke_bimap_iterator(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        _args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(iterator) = crate::runtime::host_iterator::marker(receiver).cloned() else {
            return Ok(None);
        };
        let Some((view_marker, expected_generation, removed)) =
            self.bimap_iterator_source(receiver)
        else {
            return Ok(None);
        };
        if !matches!(
            r.owner.as_str(),
            "java/util/Iterator" | "java/util/ListIterator"
        ) {
            return Ok(None);
        }
        let (cursor, length, _source) = self
            .host
            .host_iterator(&iterator)
            .ok_or_else(|| NativeError::Metadata("unknown bimap iterator".into()))?;
        let source_value = self
            .bimap_view_source(&view_marker)
            .ok_or_else(|| NativeError::Metadata("bimap iterator source missing".into()))?;
        let current_generation = self
            .host
            .collection_generation(crate::runtime::expect_ref(&source_value).map_or("", String::as_str))
            .unwrap_or(0);
        if current_generation != expected_generation
            && matches!(r.name.as_str(), "hasNext" | "next" | "remove")
        {
            return Err(NativeError::UncaughtException {
                class: "java/util/ConcurrentModificationException".into(),
                message: None,
                frame: None,
            });
        }
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("hasNext", "()Z") => Ok(Some(Value::Int((cursor < length) as i32))),
            ("next", "()Ljava/lang/Object;") => {
                let value = self.host.host_iterator_next(&iterator)?;
                self.host.set_host_iterator_source(
                    &iterator,
                    format!("{BIMAP_ITERATOR_PREFIX}{view_marker}|generation={current_generation}"),
                );
                Ok(Some(value))
            }
            ("remove", "()V") => {
                if cursor == 0 || removed {
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalStateException".into(),
                        message: None,
                        frame: None,
                    });
                }
                let values = self
                    .host
                    .host_iterator_values(&iterator)
                    .ok_or_else(|| NativeError::Metadata("bimap iterator values missing".into()))?;
                let last = values[cursor - 1].clone();
                let view = Value::Host(crate::runtime::host_ref::HostRef::new(
                    crate::runtime::host_ref::HostKind::Collection,
                    view_marker.clone(),
                ));
                let (_, kind) = self
                    .bimap_handle_info(&view)
                    .ok_or_else(|| NativeError::Metadata("bimap iterator view missing".into()))?;
                self.bimap_view_remove(&source_value, kind, &last)?;
                let new_generation = self
                    .host
                    .collection_generation(
                        crate::runtime::expect_ref(&source_value).map_or("", String::as_str),
                    )
                    .unwrap_or(current_generation);
                self.host.set_host_iterator_source(
                    &iterator,
                    format!(
                        "{BIMAP_ITERATOR_PREFIX}{view_marker}|generation={new_generation}|removed"
                    ),
                );
                Ok(Some(Value::Void))
            }
            _ => Ok(None),
        }
    }

    fn invoke_bimap_entry(
        &mut self,
        r: &MethodRef,
        entry: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.name != "setValue" || r.descriptor != "(Ljava/lang/Object;)Ljava/lang/Object;" {
            return Ok(None);
        }
        let source = self
            .bimap_entry_source(entry)
            .ok_or_else(|| NativeError::Metadata("bimap entry source missing".into()))?;
        let value = args.first().cloned().unwrap_or(Value::Ref(None));
        if value == Value::Ref(None) {
            return Err(thrown("java/lang/NullPointerException", None));
        }
        let (old_key, old_value) = self
            .host
            .map_entry_parts(entry)
            .ok_or_else(|| NativeError::Metadata("bimap entry contents missing".into()))?;
        let pairs = self.bimap_pairs(&source)?;
        let index = self
            .bimap_find(&pairs, 0, &old_key)?
            .ok_or_else(|| NativeError::Metadata("bimap entry detached".into()))?;
        if pairs
            .chunks(2)
            .enumerate()
            .any(|(at, pair)| at != index && self.java_equals(&pair[1], &value).unwrap_or(false))
        {
            return Err(thrown(
                "java/lang/IllegalArgumentException",
                Some(format!(
                    "value already present: {}",
                    crate::runtime::value_text(&value)
                )),
            ));
        }
        let partner = self
            .bimap_inverse(crate::runtime::expect_ref(&source).unwrap())
            .ok_or_else(|| NativeError::Metadata("bimap entry inverse missing".into()))?;
        self.bimap_host_put(&source, old_key.clone(), value.clone())?;
        self.bimap_host_replace_key(&partner, &old_value, value)?;
        Ok(Some(old_value))
    }

    fn bimap_inverse(&self, marker: &str) -> Option<Value> {
        self.fields
            .get(&format!("{marker}{BIMAP_INVERSE_FIELD}"))
            .cloned()
    }

    fn bimap_pairs(&self, map: &Value) -> Result<Vec<Value>, NativeError> {
        runtime_collection_values(&*self.host, map)
    }

    fn bimap_find(
        &mut self,
        pairs: &[Value],
        offset: usize,
        wanted: &Value,
    ) -> Result<Option<usize>, NativeError> {
        for (index, pair) in pairs.chunks(2).enumerate() {
            if let Some(candidate) = pair.get(offset)
                && self.java_equals(candidate, wanted)?
            {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    fn bimap_host_put(
        &mut self,
        map: &Value,
        key: Value,
        value: Value,
    ) -> Result<Value, NativeError> {
        let mut pairs = self.bimap_pairs(map)?;
        let found = pairs
            .chunks(2)
            .position(|pair| self.java_equals(&pair[0], &key).unwrap_or(false));
        let old = found.map(|index| pairs[index * 2 + 1].clone());
        if let Some(index) = found
            && self.java_equals(&pairs[index * 2 + 1], &value)?
        {
            return Ok(old.unwrap_or(Value::Ref(None)));
        }
        if let Some(index) = found {
            pairs[index * 2 + 1] = value;
        } else {
            pairs.extend([key, value]);
        }
        self.host.replace_mutable_collection(map, pairs);
        Ok(old.unwrap_or(Value::Ref(None)))
    }

    fn bimap_host_replace_key(
        &mut self,
        map: &Value,
        old_key: &Value,
        new_key: Value,
    ) -> Result<(), NativeError> {
        let mut pairs = self.bimap_pairs(map)?;
        if let Some(index) = self.bimap_find(&pairs, 0, old_key)? {
            pairs[index * 2] = new_key;
            self.host.replace_mutable_collection(map, pairs);
        }
        Ok(())
    }

    fn bimap_host_remove(&mut self, map: &Value, key: &Value) -> Result<Value, NativeError> {
        let mut pairs = self.bimap_pairs(map)?;
        let Some(index) = pairs
            .chunks(2)
            .position(|pair| self.java_equals(&pair[0], key).unwrap_or(false))
        else {
            return Ok(Value::Ref(None));
        };
        let old = pairs[index * 2 + 1].clone();
        pairs.drain(index * 2..index * 2 + 2);
        self.host.replace_mutable_collection(map, pairs);
        Ok(old)
    }

    fn bimap_host_clear(&mut self, map: &Value) -> Result<(), NativeError> {
        self.host.replace_mutable_collection(map, Vec::new());
        Ok(())
    }

    fn invoke_bimap(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        let Some((marker, kind)) = self.bimap_handle_info(receiver) else {
            return Ok(None);
        };
        if matches!(
            kind,
            BimapKind::Keys | BimapKind::Values | BimapKind::Entries
        ) {
            let source = self
                .bimap_view_source(&marker)
                .ok_or_else(|| NativeError::Metadata("HashBiMap view source missing".into()))?;
            return self.invoke_bimap_view(r, receiver, args, &source, kind);
        }
        let inverse = kind == BimapKind::Inverse;
        let Some(other) = self.bimap_inverse(&marker) else {
            return Ok(None);
        };
        if r.name == "inverse" && r.descriptor.ends_with("Lcom/google/common/collect/BiMap;") {
            return Ok(Some(other));
        }
        let root = if inverse {
            other.clone()
        } else {
            receiver.clone()
        };
        let partner = if inverse {
            receiver.clone()
        } else {
            other.clone()
        };
        if r.name == "toString" && r.descriptor == "()Ljava/lang/String;" {
            return Ok(Some(self.host.invoke_instance(
                &bimap_method("toString", "()Ljava/lang/String;"),
                receiver,
                &[],
            )?));
        }
        let reverse = |key: &Value, value: &Value| {
            if inverse {
                (value.clone(), key.clone())
            } else {
                (key.clone(), value.clone())
            }
        };
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;")
            | ("forcePut", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
                let key = args.first().cloned().unwrap_or(Value::Ref(None));
                let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
                if matches!(key, Value::Ref(None)) || matches!(value, Value::Ref(None)) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                let pairs = self.bimap_pairs(&root)?;
                if !pairs.len().is_multiple_of(2) {
                    return Err(NativeError::Metadata("HashBiMap has odd entries".into()));
                }
                let key_index = self.bimap_find(&pairs, usize::from(inverse), &key)?;
                let value_index = self.bimap_find(&pairs, usize::from(!inverse), &value)?;
                let (root_key, root_value) = reverse(&key, &value);
                if let Some(value_index) = value_index
                    && Some(value_index) != key_index
                    && r.name == "put"
                {
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        Some(format!(
                            "value already present: {}",
                            crate::runtime::value_text(&value)
                        )),
                    ));
                }
                let old = key_index.map(|index| {
                    if inverse {
                        pairs[index * 2].clone()
                    } else {
                        pairs[index * 2 + 1].clone()
                    }
                });
                if inverse && let Some(index) = key_index {
                    let existing_key = pairs[index * 2].clone();
                    self.bimap_host_replace_key(&root, &existing_key, root_key.clone())?;
                } else if let Some(index) = key_index {
                    let existing_value = pairs[index * 2 + 1].clone();
                    self.bimap_host_replace_key(&partner, &existing_value, value.clone())?;
                }
                if r.name == "forcePut"
                    && let Some(index) = value_index
                    && Some(index) != key_index
                {
                    let old_key = pairs[index * 2].clone();
                    let old_value = pairs[index * 2 + 1].clone();
                    self.bimap_host_remove(&root, &old_key)?;
                    self.bimap_host_remove(&partner, &old_value)?;
                }
                self.bimap_host_put(&root, root_key, root_value)?;
                let (partner_key, partner_value) =
                    if inverse { (key, value) } else { (value, key) };
                self.bimap_host_put(&partner, partner_key, partner_value)?;
                Ok(Some(old.unwrap_or(Value::Ref(None))))
            }
            ("get", "(Ljava/lang/Object;)Ljava/lang/Object;")
            | ("containsKey", "(Ljava/lang/Object;)Z")
            | ("containsValue", "(Ljava/lang/Object;)Z") => {
                let wanted = args.first().cloned().unwrap_or(Value::Ref(None));
                let pairs = self.bimap_pairs(&root)?;
                let offset = if r.name == "containsValue" {
                    usize::from(!inverse)
                } else {
                    usize::from(inverse)
                };
                let found = self.bimap_find(&pairs, offset, &wanted)?;
                if r.name == "get" {
                    Ok(Some(found.map_or(Value::Ref(None), |index| {
                        if inverse {
                            pairs[index * 2].clone()
                        } else {
                            pairs[index * 2 + 1].clone()
                        }
                    })))
                } else {
                    Ok(Some(Value::Int(i32::from(found.is_some()))))
                }
            }
            ("remove", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
                let wanted = args.first().cloned().unwrap_or(Value::Ref(None));
                let pairs = self.bimap_pairs(&root)?;
                let index = self.bimap_find(&pairs, usize::from(inverse), &wanted)?;
                let Some(index) = index else {
                    return Ok(Some(Value::Ref(None)));
                };
                let old_key = pairs[index * 2].clone();
                let old_value = pairs[index * 2 + 1].clone();
                self.bimap_host_remove(&root, &old_key)?;
                let partner_key = old_value.clone();
                self.bimap_host_remove(&partner, &partner_key)?;
                Ok(Some(if inverse { old_key } else { old_value }))
            }
            ("clear", "()V") => {
                self.bimap_host_clear(&root)?;
                self.bimap_host_clear(&partner)?;
                Ok(Some(Value::Void))
            }
            ("size", "()I") => Ok(Some(Value::Int(
                (self.bimap_pairs(&root)?.len() / 2) as i32,
            ))),
            ("isEmpty", "()Z") => Ok(Some(Value::Int(i32::from(
                self.bimap_pairs(&root)?.is_empty(),
            )))),
            ("values", "()Ljava/util/Collection;" | "()Ljava/util/Set;") => {
                // HashBiMap values() is a Set in Guava. It is the key set of
                // the inverse map, so the same live view machinery handles
                // values removal and iterator removal.
                self.bimap_view(receiver, &other, BimapKind::Values)
                    .map(Some)
            }
            ("keySet", "()Ljava/util/Set;") => self
                .bimap_view(receiver, receiver, BimapKind::Keys)
                .map(Some),
            ("entrySet", "()Ljava/util/Set;") => self
                .bimap_view(receiver, receiver, BimapKind::Entries)
                .map(Some),
            _ => Ok(None),
        }
    }

    /// Materialize the array and map-copy overloads whose elements are owned
    /// by the interpreter. Iterator/Iterable overloads remain in guava_text,
    /// where the host already owns their bounded iteration state.
    fn collection_factory_values(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<(&'static str, Vec<Value>, bool)>, NativeError> {
        let (class, map, array) = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                "com/google/common/collect/Lists",
                "newArrayList",
                "([Ljava/lang/Object;)Ljava/util/ArrayList;",
            ) => ("java/util/ArrayList", false, true),
            (
                "com/google/common/collect/Sets",
                "newHashSet",
                "([Ljava/lang/Object;)Ljava/util/HashSet;",
            ) => ("java/util/HashSet", false, true),
            (
                "com/google/common/collect/Maps",
                "newHashMap",
                "(Ljava/util/Map;)Ljava/util/HashMap;",
            ) => ("java/util/HashMap", true, false),
            (
                "com/google/common/collect/Maps",
                "newLinkedHashMap",
                "(Ljava/util/Map;)Ljava/util/LinkedHashMap;",
            ) => ("java/util/LinkedHashMap", true, false),
            _ => return Ok(None),
        };
        let source = args
            .first()
            .ok_or_else(|| NativeError::Metadata("Guava factory argument is missing".into()))?;
        if matches!(source, Value::Ref(None)) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            });
        }
        if array {
            return Ok(Some((class, self.array(source)?.into_owned(), map)));
        }
        let values = runtime_collection_values(&*self.host, source)?;
        Ok(Some((class, values, map)))
    }

    /// `ImmutableSet.of(...)` and `ImmutableMap.of(...)`/`ImmutableBiMap.of`,
    /// Guava's documented answers over the host's own collections.
    ///
    /// A set keeps the first of equal elements, in argument order, and its
    /// `of(e1, ..., e6, E... others)` overload spreads the array. A map keeps
    /// its pairs in order and refuses a repeated key with Guava's
    /// `IllegalArgumentException` (a bi-map also a repeated value). A `null`
    /// element, key or value is Guava's `NullPointerException`.
    fn invoke_immutable_of(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let mut values = args.to_vec();
        if r.descriptor.contains("[Ljava/lang/Object;)")
            && let Some(rest) = values.pop()
        {
            if matches!(rest, Value::Ref(None)) {
                return Err(thrown("java/lang/NullPointerException", None));
            }
            values.extend(self.array(&rest)?.into_owned());
        }
        if values.iter().any(|value| matches!(value, Value::Ref(None))) {
            return Err(thrown("java/lang/NullPointerException", None));
        }
        if r.owner == "com/google/common/collect/ImmutableSet" {
            let mut unique: Vec<Value> = Vec::with_capacity(values.len());
            for value in values {
                if !unique.contains(&value) {
                    unique.push(value);
                }
            }
            return Ok(Some(self.factory_or_count(r, "set", unique)?));
        }
        if !values.len().is_multiple_of(2) {
            return Ok(None);
        }
        let bimap = r.owner == "com/google/common/collect/ImmutableBiMap";
        for (index, pair) in values.chunks(2).enumerate() {
            for earlier in values.chunks(2).take(index) {
                let (kind, clash) = if earlier[0] == pair[0] {
                    ("key", true)
                } else if bimap && earlier[1] == pair[1] {
                    ("value", true)
                } else {
                    ("", false)
                };
                if clash {
                    let text = |value: &Value| expect_string(value).to_owned();
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        Some(format!(
                            "Multiple entries with same {kind}: {}={} and {}={}",
                            text(&pair[0]),
                            text(&pair[1]),
                            text(&earlier[0]),
                            text(&earlier[1])
                        )),
                    ));
                }
            }
        }
        Ok(Some(self.factory_or_count(r, "map", values)?))
    }

    /// `Suppliers.memoize(s)` (and `memoizeWithExpiration`, whose expiry a
    /// bootstrap never outlives) answers a supplier that runs `s` once and
    /// keeps the answer; `Suppliers.ofInstance(v)` always answers `v`. The
    /// memo is the supplier object's own state, so every `get` through
    /// `com.google.common.base.Supplier` or `java.util.function.Supplier`
    /// sees it.
    fn invoke_memoized_supplier(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        const MEMO: &str = "com/google/common/base/Suppliers$Memoizing";
        if r.owner == "com/google/common/base/Suppliers" {
            let (delegate, done) = match r.name.as_str() {
                "memoize" | "memoizeWithExpiration" => (args.first().cloned(), false),
                "ofInstance" => (args.first().cloned(), true),
                _ => return Ok(None),
            };
            let Some(delegate) = delegate else {
                return Ok(None);
            };
            if r.name != "ofInstance" && matches!(delegate, Value::Ref(None)) {
                return Err(thrown("java/lang/NullPointerException", None));
            }
            let supplier = self.new_object(MEMO);
            let marker = expect_ref(&supplier).cloned().unwrap_or_default();
            let field = if done { "value" } else { "delegate" };
            self.fields
                .insert(format!("{marker}|{MEMO}#{field}"), delegate);
            if done {
                self.fields
                    .insert(format!("{marker}|{MEMO}#done"), Value::Int(1));
            }
            return Ok(Some(supplier));
        }
        let Some(marker) = receiver.and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        if !marker.starts_with(&format!("{MEMO}#")) {
            return Ok(None);
        }
        if r.name == "toString" {
            return Ok(Some(Value::Ref(Some("Suppliers.memoize(...)".into()))));
        }
        if r.name != "get" || r.descriptor != "()Ljava/lang/Object;" {
            return Ok(None);
        }
        if self.fields.contains_key(&format!("{marker}|{MEMO}#done")) {
            return Ok(Some(
                self.fields
                    .get(&format!("{marker}|{MEMO}#value"))
                    .cloned()
                    .unwrap_or(Value::Ref(None)),
            ));
        }
        let delegate = self
            .fields
            .get(&format!("{marker}|{MEMO}#delegate"))
            .cloned()
            .unwrap_or(Value::Ref(None));
        let value = match self.invoke_callback(&delegate, &[])? {
            Some(value) => value,
            None => {
                // A jar `Supplier` implementation runs its own `get`.
                let class = crate::runtime::jar_object::object_class(&delegate)
                    .map(str::to_owned)
                    .unwrap_or_default();
                const GET: &str = "()Ljava/lang/Object;";
                match self.member_owner(&class, "get", GET) {
                    Some((owner, true)) => self.invoke(&owner, "get", GET, Some(delegate), &[])?,
                    _ => self.host.invoke_instance(r, &delegate, &[])?,
                }
            }
        };
        self.fields
            .insert(format!("{marker}|{MEMO}#value"), value.clone());
        self.fields
            .insert(format!("{marker}|{MEMO}#done"), Value::Int(1));
        Ok(Some(value))
    }

    fn invoke_preconditions(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let message = |this: &Self| -> Option<String> {
            // Guava fills `%s` placeholders; the template alone is enough to
            // name the failure.
            args.get(1).map(|value| match value {
                Value::Ref(Some(text)) => this
                    .string_builders
                    .get(text)
                    .cloned()
                    .unwrap_or_else(|| text.clone()),
                other => value_text(other),
            })
        };
        match r.name.as_str() {
            "checkNotNull" => match args.first() {
                Some(Value::Ref(None)) | None => {
                    Err(thrown("java/lang/NullPointerException", message(self)))
                }
                Some(value) => Ok(Some(value.clone())),
            },
            "checkArgument" | "checkState" => {
                let holds = args.first().and_then(boxed_boolean).unwrap_or(false)
                    || matches!(args.first(), Some(Value::Int(flag)) if *flag != 0);
                if holds {
                    Ok(Some(Value::Void))
                } else {
                    let class = if r.name == "checkArgument" {
                        "java/lang/IllegalArgumentException"
                    } else {
                        "java/lang/IllegalStateException"
                    };
                    Err(thrown(class, message(self)))
                }
            }
            "checkElementIndex" | "checkPositionIndex" => {
                let index = args.first().and_then(boxed_integer).unwrap_or(-1);
                let size = args.get(1).and_then(boxed_integer).unwrap_or(0);
                let fits = if r.name == "checkElementIndex" {
                    index >= 0 && index < size
                } else {
                    index >= 0 && index <= size
                };
                if fits {
                    Ok(Some(Value::Int(index)))
                } else {
                    Err(thrown(
                        "java/lang/IndexOutOfBoundsException",
                        Some(format!("index ({index}) must be less than size ({size})")),
                    ))
                }
            }
            _ => Ok(None),
        }
    }

    /// The `Type` marker a token names: a `TypeToken.of` handle's
    /// (`host_type_token.rs`), or the type a jar subclass object captured.
    pub(crate) fn token_type(&self, token: &Value) -> Option<Value> {
        if let Some(inner) = crate::runtime::host_type_token::token_type(token) {
            return Some(inner);
        }
        let marker = crate::runtime::jar_object::object_key(token)?;
        self.fields
            .get(&format!("{marker}|{TYPE_TOKEN}{RUNTIME_TYPE}"))
            .filter(|inner| crate::runtime::host_gson_type::any_node(inner).is_some())
            .cloned()
    }

    fn token_value(inner: &Value) -> Value {
        crate::runtime::host_type_token::token(inner)
    }

    /// The raw class a type erases to.
    fn raw_class(node: &GenericType) -> String {
        match node {
            GenericType::Parameterized { raw, .. } => raw.clone(),
            GenericType::Wildcard { upper, .. } => upper.first().map_or_else(
                || "java/lang/Object".to_owned(),
                |bound| match bound {
                    GenericType::Class(name) => name.clone(),
                    GenericType::Parameterized { raw, .. } => raw.clone(),
                    _ => "java/lang/Object".to_owned(),
                },
            ),
            // A variable's erasure is its first bound's, which its declaration
            // holds; the model keeps `Object` (as it did for a use-site
            // variable, whose bounds were never written).
            GenericType::Variable { .. } => "java/lang/Object".to_owned(),
            GenericType::Array(component) => match component.as_ref() {
                GenericType::Parameterized { raw, .. } => format!("[L{raw};"),
                _ => "[Ljava/lang/Object;".to_owned(),
            },
            GenericType::Class(name) => name.clone(),
        }
    }

    pub(crate) fn invoke_type_token(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        match (r.name.as_str(), r.descriptor.as_str()) {
            (
                "of",
                "(Ljava/lang/Class;)Lcom/google/common/reflect/TypeToken;"
                | "(Ljava/lang/reflect/Type;)Lcom/google/common/reflect/TypeToken;",
            ) => {
                let Some(inner) = args.first().filter(|value| **value != Value::Ref(None)) else {
                    return Err(thrown("java/lang/NullPointerException", None));
                };
                // A `Type` the model holds: a `Class`, a JDK or a Gson node;
                // anything else (a jar's own `Type`) counted. Text is never a
                // `Type` (#156 P13c): a `(Type)` cast of a String throws first.
                let modelled = crate::runtime::host_gson_type::any_node(inner).is_some();
                if !modelled {
                    return self.host.unknown_operand_call(r).map(Some);
                }
                return Ok(Some(Self::token_value(inner)));
            }
            // `new TypeToken<T>() {}`: the anonymous subclass's generic
            // superclass names `T`. A subclass that does not parameterize
            // `TypeToken` is Guava's "must be parameterized" error.
            ("<init>", "()V") => {
                let Some(marker) = receiver
                    .and_then(crate::runtime::jar_object::object_key)
                    .map(str::to_owned)
                else {
                    return Ok(None);
                };
                let Some(class) = crate::runtime::jar_object::key_class(&marker) else {
                    return Ok(None);
                };
                let scope = crate::runtime::reflect_generic::class_scope(class, self.class(class));
                let captured = self
                    .class(class)
                    .and_then(ClassFile::class_signature)
                    .as_deref()
                    .and_then(|signature| {
                        crate::runtime::reflect_generic::parse_class_signature(signature, scope)
                    })
                    .and_then(|(superclass, _)| match superclass {
                        GenericType::Parameterized { raw, args, .. } if raw == TYPE_TOKEN => {
                            args.into_iter().next()
                        }
                        _ => None,
                    });
                let Some(captured) = captured else {
                    return Err(thrown(
                        "java/lang/IllegalStateException",
                        Some(format!("{} isn't parameterized", class.replace('/', "."))),
                    ));
                };
                // The `Type` itself, as Guava's `runtimeType` field holds it.
                self.fields.insert(
                    format!("{marker}|{TYPE_TOKEN}{RUNTIME_TYPE}"),
                    generic_type_value(&captured),
                );
                return Ok(Some(Value::Void));
            }
            _ => {}
        }
        let Some(inner) = receiver.and_then(|token| self.token_type(token)) else {
            return Ok(None);
        };
        let Some(node) = crate::runtime::host_gson_type::any_node(&inner) else {
            return Ok(None);
        };
        let class = match &node {
            GenericType::Class(name) => Some(name.as_str()),
            _ => None,
        };
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            // `runtimeType`: the very `Type` the token was made from.
            ("getType", "()Ljava/lang/reflect/Type;") => inner.clone(),
            ("getRawType", "()Ljava/lang/Class;") => {
                crate::runtime::host_class::class_value(&Self::raw_class(&node))
            }
            ("wrap", "()Lcom/google/common/reflect/TypeToken;") => {
                match class.and_then(boxed_class) {
                    Some(boxed) => Self::token_value(&crate::runtime::host_class::class_value(boxed)),
                    None => return self.host.unknown_operand_call(r).map(Some),
                }
            }
            ("unwrap", "()Lcom/google/common/reflect/TypeToken;") => {
                let primitive = [
                    "boolean", "byte", "char", "short", "int", "long", "float", "double", "void",
                ]
                .into_iter()
                .find(|primitive| class.is_some() && boxed_class(primitive) == class);
                match primitive {
                    Some(primitive) => {
                        Self::token_value(&crate::runtime::host_class::class_value(primitive))
                    }
                    None => return self.host.unknown_operand_call(r).map(Some),
                }
            }
            ("isPrimitive", "()Z") => Value::Int(i32::from(class.and_then(boxed_class).is_some())),
            ("isArray", "()Z") => Value::Int(i32::from(
                class.is_some_and(|name| name.starts_with('['))
                    || matches!(node, GenericType::Array(_)),
            )),
            ("getComponentType", "()Lcom/google/common/reflect/TypeToken;") => match &node {
                GenericType::Class(name) => match name.strip_prefix('[') {
                    Some(element) => Self::token_value(&crate::runtime::host_class::class_value(
                        &erased_field_type(element),
                    )),
                    None => Value::Ref(None),
                },
                GenericType::Array(component) => Self::token_value(&generic_type_value(component)),
                _ => Value::Ref(None),
            },
            // `runtimeType.equals(that.runtimeType)` for another token;
            // `false` for anything that is not one. Two types the model
            // cannot compare are counted.
            ("equals", "(Ljava/lang/Object;)Z") => {
                let other = args
                    .first()
                    .ok_or_else(|| thrown("java/lang/NullPointerException", None))?;
                let equal = match self.token_type(other) {
                    Some(theirs) => crate::runtime::host_type_token::types_equal(&inner, &theirs),
                    None if crate::runtime::jar_object::object_key(other).is_some() => None,
                    None => Some(false),
                };
                match equal {
                    Some(equal) => Value::Int(i32::from(equal)),
                    None => return self.host.unknown_operand_call(r).map(Some),
                }
            }
            // `runtimeType.hashCode()`: a class's identity hash.
            ("hashCode", "()I") => return self.host.unknown_operand_call(r).map(Some),
            ("toString", "()Ljava/lang/String;") => {
                Value::Ref(Some(crate::runtime::host_type_token::guava_type_text(&node)))
            }
            (
                "isSubtypeOf" | "isSupertypeOf",
                "(Lcom/google/common/reflect/TypeToken;)Z" | "(Ljava/lang/reflect/Type;)Z",
            ) => {
                let other = args
                    .first()
                    .and_then(|value| self.token_type(value).or_else(|| Some(value.clone())))
                    .and_then(|value| crate::runtime::host_gson_type::any_node(&value))
                    .ok_or_else(|| thrown("java/lang/NullPointerException", None))?;
                let (sub, sup) = if r.name == "isSubtypeOf" {
                    (Self::raw_class(&node), Self::raw_class(&other))
                } else {
                    (Self::raw_class(&other), Self::raw_class(&node))
                };
                // Raw assignability; type arguments are not compared.
                Value::Int(i32::from(
                    sub == sup || sup == "java/lang/Object" || self.class_is_instance(&sub, &sup),
                ))
            }
            _ => return Ok(None),
        }))
    }

    fn invoke_cache_builder(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        // A fresh builder per `newBuilder()`/`from(spec)` (`host_guava.rs`).
        if r.name == "newBuilder" || r.name == "from" {
            let marker = self.new_object("guava-cache-builder");
            return Ok(Some(crate::runtime::host_guava::cache_builder(marker)));
        }
        let Some(builder) = receiver.filter(|builder| crate::runtime::host_guava::is_cache_builder(builder))
        else {
            return Ok(None);
        };
        if r.name == "build" {
            let Some(cache) = self.host.empty_mutable_collection("map") else {
                return Ok(None);
            };
            let marker = expect_ref(&cache).cloned().unwrap_or_default();
            if let Some(loader) = args.first() {
                if matches!(loader, Value::Ref(None)) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                self.fields
                    .insert(format!("{marker}|guava#loader"), loader.clone());
            }
            return Ok(Some(cache));
        }
        // Size, expiry, reference strength, listeners and stats settings
        // answer the builder itself.
        Ok(r.descriptor
            .ends_with(")Lcom/google/common/cache/CacheBuilder;")
            .then(|| builder.clone()))
    }

    /// Run a cache's loader for `key`.
    fn cache_load(&mut self, loader: &Value, key: &Value) -> Result<Value, NativeError> {
        if let Some(function) = crate::runtime::host_guava::cache_loader_function(loader) {
            return Ok(self
                .invoke_callback(&function, std::slice::from_ref(key))?
                .unwrap_or(Value::Ref(None)));
        }
        let class = crate::runtime::jar_object::object_class(loader)
            .map(str::to_owned)
            .unwrap_or_default();
        const LOAD: &str = "(Ljava/lang/Object;)Ljava/lang/Object;";
        match self.member_owner(&class, "load", LOAD) {
            Some((owner, true)) => self.invoke(
                &owner,
                "load",
                LOAD,
                Some(loader.clone()),
                std::slice::from_ref(key),
            ),
            _ => Err(NativeError::Metadata(format!(
                "LoadingCache loader {} is not a modelled CacheLoader",
                describe_value(loader)
            ))),
        }
    }

    fn invoke_cache(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(cache) = receiver.cloned() else {
            return Ok(None);
        };
        let Some(marker) = expect_ref(&cache).cloned() else {
            return Ok(None);
        };
        let get = map_ref("get", "(Ljava/lang/Object;)Ljava/lang/Object;");
        let put = map_ref(
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        );
        Ok(Some(match r.name.as_str() {
            "get" | "getUnchecked" | "apply" if args.len() == 1 => {
                let key = args[0].clone();
                if matches!(key, Value::Ref(None)) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                let cached = self
                    .host
                    .invoke_instance(&get, &cache, std::slice::from_ref(&key))?;
                if !matches!(cached, Value::Ref(None)) {
                    cached
                } else {
                    let Some(loader) = self.fields.get(&format!("{marker}|guava#loader")).cloned()
                    else {
                        return Ok(None);
                    };
                    let loaded = self.cache_load(&loader, &key)?;
                    if matches!(loaded, Value::Ref(None)) {
                        // Guava refuses a null load.
                        return Err(thrown(
                            "com/google/common/cache/CacheLoader$InvalidCacheLoadException",
                            Some(format!(
                                "CacheLoader returned null for key {}.",
                                value_text(&key)
                            )),
                        ));
                    }
                    self.host
                        .invoke_instance(&put, &cache, &[key, loaded.clone()])?;
                    loaded
                }
            }
            // `get(key, Callable)` on a plain cache.
            "get" if args.len() == 2 => {
                let key = args[0].clone();
                let cached = self
                    .host
                    .invoke_instance(&get, &cache, std::slice::from_ref(&key))?;
                if !matches!(cached, Value::Ref(None)) {
                    cached
                } else {
                    let loaded = self
                        .invoke_callback(&args[1], &[])?
                        .unwrap_or(Value::Ref(None));
                    self.host
                        .invoke_instance(&put, &cache, &[key, loaded.clone()])?;
                    loaded
                }
            }
            "getIfPresent" => self.host.invoke_instance(&get, &cache, args)?,
            "put" => {
                self.host.invoke_instance(&put, &cache, args)?;
                Value::Void
            }
            "invalidate" => {
                self.host.invoke_instance(
                    &map_ref("remove", "(Ljava/lang/Object;)Ljava/lang/Object;"),
                    &cache,
                    args,
                )?;
                Value::Void
            }
            "invalidateAll" if args.is_empty() => {
                self.host
                    .invoke_instance(&map_ref("clear", "()V"), &cache, &[])?;
                Value::Void
            }
            "size" => {
                let size = self
                    .host
                    .invoke_instance(&map_ref("size", "()I"), &cache, &[])?;
                Value::Long(i64::from(boxed_integer(&size).unwrap_or(0)))
            }
            "asMap" => cache,
            "cleanUp" | "refresh" => Value::Void,
            _ => return Ok(None),
        }))
    }

    fn invoke_multimap(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.name == "create" {
            let Some(map) = self.host.empty_mutable_collection("map") else {
                return Ok(None);
            };
            let marker = expect_ref(&map).cloned().unwrap_or_default();
            let values = if r.owner.ends_with("ArrayListMultimap") {
                "list"
            } else {
                "set"
            };
            self.fields.insert(
                format!("{marker}|guava#multimap"),
                Value::Ref(Some(values.into())),
            );
            return Ok(Some(map));
        }
        let Some(map) = receiver.cloned() else {
            return Ok(None);
        };
        let Some(marker) = expect_ref(&map).cloned() else {
            return Ok(None);
        };
        let Some(Value::Ref(Some(kind))) = self
            .fields
            .get(&format!("{marker}|guava#multimap"))
            .cloned()
        else {
            return Ok(None);
        };
        let owner = if kind == "list" {
            "java/util/List"
        } else {
            "java/util/Set"
        };
        let get = map_ref("get", "(Ljava/lang/Object;)Ljava/lang/Object;");
        // The collection a key maps to, made (and stored) on first use.
        let values_of = |this: &mut Self, key: &Value| -> Result<Value, NativeError> {
            let existing = this
                .host
                .invoke_instance(&get, &map, std::slice::from_ref(key))?;
            if !matches!(existing, Value::Ref(None)) {
                return Ok(existing);
            }
            let fresh = this.host.empty_mutable_collection(&kind).ok_or_else(|| {
                NativeError::Metadata("the host models no mutable collection".into())
            })?;
            this.host.invoke_instance(
                &map_ref(
                    "put",
                    "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                ),
                &map,
                &[key.clone(), fresh.clone()],
            )?;
            Ok(fresh)
        };
        let size_of = |this: &mut Self, collection: &Value| -> Result<i32, NativeError> {
            let size = this.host.invoke_instance(
                &collection_ref(owner, "size", "()I"),
                collection,
                &[],
            )?;
            Ok(boxed_integer(&size).unwrap_or(0))
        };
        let keys = |this: &mut Self| -> Result<Vec<Value>, NativeError> {
            let set =
                this.host
                    .invoke_instance(&map_ref("keySet", "()Ljava/util/Set;"), &map, &[])?;
            runtime_collection_values(&*this.host, &set)
        };
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("put", "(Ljava/lang/Object;Ljava/lang/Object;)Z") => {
                let collection = values_of(self, &args[0])?;
                self.host.invoke_instance(
                    &collection_ref(owner, "add", "(Ljava/lang/Object;)Z"),
                    &collection,
                    &args[1..2],
                )?
            }
            ("get", _) => values_of(self, &args[0])?,
            ("containsKey", "(Ljava/lang/Object;)Z") => {
                let existing = self.host.invoke_instance(&get, &map, &args[..1])?;
                let present =
                    !matches!(existing, Value::Ref(None)) && size_of(self, &existing)? > 0;
                Value::Int(i32::from(present))
            }
            ("containsEntry", "(Ljava/lang/Object;Ljava/lang/Object;)Z") => {
                let existing = self.host.invoke_instance(&get, &map, &args[..1])?;
                if matches!(existing, Value::Ref(None)) {
                    Value::Int(0)
                } else {
                    self.host.invoke_instance(
                        &collection_ref(owner, "contains", "(Ljava/lang/Object;)Z"),
                        &existing,
                        &args[1..2],
                    )?
                }
            }
            ("remove", "(Ljava/lang/Object;Ljava/lang/Object;)Z") => {
                let existing = self.host.invoke_instance(&get, &map, &args[..1])?;
                if matches!(existing, Value::Ref(None)) {
                    Value::Int(0)
                } else {
                    self.host.invoke_instance(
                        &collection_ref(owner, "remove", "(Ljava/lang/Object;)Z"),
                        &existing,
                        &args[1..2],
                    )?
                }
            }
            ("size", "()I") | ("isEmpty", "()Z") => {
                let mut total = 0;
                for key in keys(self)? {
                    let collection = self.host.invoke_instance(&get, &map, &[key])?;
                    total += size_of(self, &collection)?;
                }
                if r.name == "size" {
                    Value::Int(total)
                } else {
                    Value::Int(i32::from(total == 0))
                }
            }
            ("keySet", "()Ljava/util/Set;") => {
                let mut present = Vec::new();
                for key in keys(self)? {
                    let collection =
                        self.host
                            .invoke_instance(&get, &map, std::slice::from_ref(&key))?;
                    if size_of(self, &collection)? > 0 {
                        present.push(key);
                    }
                }
                // `AbstractMultimap` caches its key view: one object per
                // multimap, following it (a set stands in for its class).
                match self.host.owned_collection(
                    &map,
                    "multimap.keySet",
                    crate::runtime::stand_in_class("set"),
                    true,
                    present,
                ) {
                    Some(view) => view,
                    None => self.host.unknown_operand_call(r)?,
                }
            }
            ("values", "()Ljava/util/Collection;") => {
                let mut all = Vec::new();
                for key in keys(self)? {
                    let collection = self.host.invoke_instance(&get, &map, &[key])?;
                    all.extend(runtime_collection_values(&*self.host, &collection)?);
                }
                // `AbstractMultimap` caches its values view likewise.
                match self.host.owned_collection(
                    &map,
                    "multimap.values",
                    crate::runtime::stand_in_class("list"),
                    true,
                    all,
                ) {
                    Some(view) => view,
                    None => self.host.unknown_operand_call(r)?,
                }
            }
            ("asMap", "()Ljava/util/Map;") => map,
            ("clear", "()V") => {
                self.host
                    .invoke_instance(&map_ref("clear", "()V"), &map, &[])?;
                Value::Void
            }
            _ => return Ok(None),
        }))
    }
}

/// The synthetic field key a `MapMaker` marker keeps its option text under.
const MAP_MAKER_OPTIONS: &str = "#mapMakerOptions";

impl<H: Host> Runtime<'_, H> {
    /// `MapMaker`, the builder Guava's `makeMap()` comes from.
    ///
    /// Guava 33.6's `MapMaker` is a fluent builder. Its option setters return
    /// the receiver, reject an invalid value with `IllegalArgumentException`
    /// (no message: `Preconditions.checkArgument`) and a repeated option with
    /// an `IllegalStateException` naming the old value. `makeMap()` without any
    /// strength setting is a plain `ConcurrentHashMap`, which the runtime's own
    /// map model stands in for.
    ///
    /// Named departure: `weakKeys()`/`weakValues()` are accepted, but the map
    /// the runtime answers holds its keys and values strongly. The bounded
    /// runtime has no garbage collector and one thread, so a weak identity is
    /// not observable, and nothing here evicts an entry.
    fn invoke_map_maker(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = receiver.and_then(expect_ref) else {
            return Err(NativeError::Metadata(
                "malformed MapMaker receiver marker".into(),
            ));
        };
        let marker = marker.to_owned();
        let answer = || Ok(Some(crate::runtime::jar_object::stored_value(&marker)));
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("<init>", "()V") => Ok(Some(Value::Void)),
            ("initialCapacity" | "concurrencyLevel", "(I)Lcom/google/common/collect/MapMaker;") => {
                let Some(Value::Int(value)) = args.first() else {
                    return Err(NativeError::Metadata(
                        "MapMaker capacity needs an int".into(),
                    ));
                };
                let (value, label) = (
                    *value,
                    if r.name == "initialCapacity" {
                        "initial capacity"
                    } else {
                        "concurrency level"
                    },
                );
                if let Some(previous) = self.map_maker_option(&marker, &r.name) {
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalStateException".into(),
                        message: Some(format!("{label} was already set to {previous}")),
                        frame: None,
                    });
                }
                // `checkArgument(value >= 0)` for the capacity,
                // `checkArgument(value > 0)` for the concurrency level.
                let invalid = if r.name == "initialCapacity" {
                    value < 0
                } else {
                    value <= 0
                };
                if invalid {
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: None,
                        frame: None,
                    });
                }
                self.set_map_maker_option(&marker, &r.name, &value.to_string());
                answer()
            }
            ("weakKeys" | "weakValues", "()Lcom/google/common/collect/MapMaker;") => {
                let option = if r.name == "weakKeys" {
                    "keyStrength"
                } else {
                    "valueStrength"
                };
                if let Some(previous) = self.map_maker_option(&marker, option) {
                    // `checkState(keyStrength == null, "Key strength was
                    // already set to %s", keyStrength)`.
                    let label = if option == "keyStrength" {
                        "Key strength"
                    } else {
                        "Value strength"
                    };
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalStateException".into(),
                        message: Some(format!("{label} was already set to {previous}")),
                        frame: None,
                    });
                }
                // `MoreObjects` lower-cases a strength's `toString`.
                self.set_map_maker_option(&marker, option, "WEAK");
                answer()
            }
            ("makeMap", "()Ljava/util/concurrent/ConcurrentMap;") => {
                // A new `ConcurrentHashMap` (no second thread to serve).
                Ok(Some(self.new_host_collection(
                    "java/util/concurrent/ConcurrentHashMap",
                    Vec::new(),
                    (&r.owner, &r.name, &r.descriptor),
                )?))
            }
            ("toString", "()Ljava/lang/String;") => {
                let mut out = String::from("MapMaker{");
                for (index, (key, value)) in self.map_maker_options(&marker).iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(key);
                    out.push('=');
                    if key.ends_with("Strength") {
                        out.push_str(&value.to_ascii_lowercase());
                    } else {
                        out.push_str(value);
                    }
                }
                out.push('}');
                Ok(Some(Value::Ref(Some(out))))
            }
            _ => Ok(None),
        }
    }

    /// One option a `MapMaker` marker has set, if any.
    fn map_maker_option(&self, marker: &str, option: &str) -> Option<String> {
        self.map_maker_options(marker)
            .into_iter()
            .find(|(key, _)| key == option)
            .map(|(_, value)| value)
    }

    /// Every option a `MapMaker` marker has set, in Guava's `toString` order.
    fn map_maker_options(&self, marker: &str) -> Vec<(String, String)> {
        let Some(text) = self
            .fields
            .get(&format!("{marker}|{MAP_MAKER_OPTIONS}"))
            .and_then(expect_ref)
        else {
            return Vec::new();
        };
        let mut options: Vec<(String, String)> = Vec::new();
        for entry in text.split(';').filter(|entry| !entry.is_empty()) {
            let Some((key, value)) = entry.split_once('=') else {
                continue;
            };
            // Guava's `toString` order is fixed: capacity, concurrency, then
            // the strengths.
            let order = match key {
                "initialCapacity" => 0,
                "concurrencyLevel" => 1,
                "keyStrength" => 2,
                _ => 3,
            };
            options.push((format!("{order}|{key}"), value.to_owned()));
        }
        options.sort();
        options
            .into_iter()
            .map(|(key, value)| {
                let key = key.split_once('|').map_or(key.as_str(), |(_, k)| k);
                (key.to_owned(), value)
            })
            .collect()
    }

    fn set_map_maker_option(&mut self, marker: &str, option: &str, value: &str) {
        let mut options = self.map_maker_options(marker);
        options.push((option.to_owned(), value.to_owned()));
        let mut encoded = String::new();
        for (key, value) in options {
            if !encoded.is_empty() {
                encoded.push(';');
            }
            encoded.push_str(&key);
            encoded.push('=');
            encoded.push_str(&value);
        }
        self.fields.insert(
            format!("{marker}|{MAP_MAKER_OPTIONS}"),
            Value::Ref(Some(encoded)),
        );
    }
}
