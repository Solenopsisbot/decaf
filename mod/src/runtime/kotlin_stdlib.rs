//! Bounded Kotlin stdlib surfaces used during mod bootstrap.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested};
use crate::runtime::{MethodRef, NativeError, Runtime, Value};

const ENUM_ENTRIES: &str = "kotlin/enums/EnumEntries";
const ENUM_ENTRIES_LIST: &str = "kotlin/enums/EnumEntriesList";
const LAZY: &str = "kotlin/Lazy";
const INITIALIZED_LAZY: &str = "kotlin/InitializedLazyImpl";
const EMPTY_LIST: &str = "kotlin/collections/EmptyList";
const EMPTY_SET: &str = "kotlin/collections/EmptySet";
const EMPTY_MAP: &str = "kotlin/collections/EmptyMap";
const OBJECT: &str = "java/lang/Object";
const MAP: &str = "java/util/Map";
const SET: &str = "java/util/Set";
const LIST: &str = "java/util/List";
const COLLECTION: &str = "java/util/Collection";
const ITERABLE: &str = "java/lang/Iterable";
const SERIALIZABLE: &str = "java/io/Serializable";
const KMAPPED: &str = "kotlin/jvm/internal/markers/KMappedMarker";

static NEXT_ENTRIES: AtomicU64 = AtomicU64::new(0);
static NEXT_LAZY: AtomicU64 = AtomicU64::new(0);

fn identity_equals(
    handle: &HostRef,
    other: &Value,
    _nested: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) => handle.same_object(other),
        _ => Some(false),
    })
}

fn collection_family(handle: &HostRef) -> Option<&'static str> {
    if handle.kind() == HostKind::KotlinEnumEntries {
        return Some("list");
    }
    if handle.kind() == HostKind::KotlinEmptyCollection {
        return Some(match handle.key() {
            "set" => "set",
            "map" => "map",
            _ => "list",
        });
    }
    let class = handle.class()?;
    let class = class.rsplit('/').next().unwrap_or(class);
    if class.contains("List") || class == "ArrayList" {
        Some("list")
    } else if class.contains("Set") {
        Some("set")
    } else if class.contains("Map") {
        Some("map")
    } else {
        None
    }
}

fn collection_values(handle: &HostRef, nested: &mut dyn Nested) -> Option<Vec<Value>> {
    if handle.kind() == HostKind::KotlinEnumEntries {
        return handle
            .values()
            .and_then(|values| values.get(1..).map(ToOwned::to_owned));
    }
    if handle.kind() == HostKind::KotlinEmptyCollection {
        return Some(Vec::new());
    }
    nested.collection_entries(&Value::Host(handle.clone()))
}

fn list_like_equals(
    handle: &HostRef,
    other: &Value,
    nested: &mut dyn Nested,
    family: &str,
) -> Result<Option<bool>, NativeError> {
    let Value::Host(other) = other else {
        return Ok(Some(false));
    };
    if collection_family(other) != Some(family) {
        return Ok(None);
    }
    let Some(left) = collection_values(handle, nested) else {
        return Ok(None);
    };
    let Some(right) = collection_values(other, nested) else {
        return Ok(None);
    };
    if left.len() != right.len() {
        return Ok(Some(false));
    }
    for (a, b) in left.iter().zip(&right) {
        if !nested.equals(a, b)?.unwrap_or(false) {
            return Ok(Some(false));
        }
    }
    Ok(Some(true))
}

fn enum_entries_equals(
    handle: &HostRef,
    other: &Value,
    nested: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    list_like_equals(handle, other, nested, "list")
}

fn enum_entries_hash(
    handle: &HostRef,
    nested: &mut dyn Nested,
) -> Result<Option<i32>, NativeError> {
    let Some(values) = handle
        .values()
        .and_then(|values| values.get(1..).map(ToOwned::to_owned))
    else {
        return Ok(None);
    };
    let mut hash = 1_i32;
    for value in values {
        let Some(part) = nested.hash(&value)? else {
            return Ok(None);
        };
        hash = hash.wrapping_mul(31).wrapping_add(part);
    }
    Ok(Some(hash))
}

fn enum_entries_text(
    handle: &HostRef,
    nested: &mut dyn Nested,
) -> Result<Option<String>, NativeError> {
    let Some(values) = handle
        .values()
        .and_then(|values| values.get(1..).map(ToOwned::to_owned))
    else {
        return Ok(None);
    };
    let mut text = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            text.push_str(", ");
        }
        let Some(part) = nested.text(value)? else {
            return Ok(None);
        };
        text.push_str(&part);
    }
    text.push(']');
    Ok(Some(text))
}

fn empty_equals(
    handle: &HostRef,
    other: &Value,
    nested: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    let Value::Host(other) = other else {
        return Ok(Some(false));
    };
    let Some(family) = collection_family(handle) else {
        return Ok(None);
    };
    if collection_family(other) != Some(family) {
        return Ok(None);
    }
    let Some(values) = collection_values(other, nested) else {
        return Ok(None);
    };
    Ok(Some(values.is_empty()))
}

fn empty_hash(handle: &HostRef, _nested: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    Ok(Some(if handle.key() == "list" { 1 } else { 0 }))
}

fn empty_text(handle: &HostRef, _nested: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    Ok(Some(
        match handle.key() {
            "map" => "{}",
            _ => "[]",
        }
        .to_owned(),
    ))
}

fn class_for_entries(_: &str) -> &'static str {
    ENUM_ENTRIES_LIST
}

fn class_for_lazy(key: &str) -> &'static str {
    // Only the mode, the key's first marker, is read: the initializer after
    // it is a callback, which no text reads back (#261), so reading every
    // value would fail and fall back to the synchronized class.
    let mode = crate::runtime::split_capture_markers(key)
        .ok()
        .and_then(|markers| markers.into_iter().next())
        .and_then(|marker| match crate::runtime::parse_value_marker(&marker) {
            Ok(Value::Ref(Some(mode))) => Some(mode),
            _ => None,
        });
    match mode.as_deref().unwrap_or("SYNCHRONIZED") {
        "NONE" => "kotlin/UnsafeLazyImpl",
        "PUBLICATION" => "kotlin/SafePublicationLazyImpl",
        "INITIALIZED" => INITIALIZED_LAZY,
        _ => "kotlin/SynchronizedLazyImpl",
    }
}

fn class_for_empty(key: &str) -> &'static str {
    match key {
        "set" => EMPTY_SET,
        "map" => EMPTY_MAP,
        _ => EMPTY_LIST,
    }
}

fn family_instance(class: &str, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
    if wanted == OBJECT || wanted == class || hierarchy.is_instance(class, wanted) == Some(true) {
        return Some(true);
    }
    let known = match class {
        ENUM_ENTRIES_LIST => [
            ENUM_ENTRIES,
            LIST,
            COLLECTION,
            ITERABLE,
            KMAPPED,
            SERIALIZABLE,
        ]
        .as_slice(),
        EMPTY_LIST => [LIST, COLLECTION, ITERABLE, KMAPPED, SERIALIZABLE].as_slice(),
        EMPTY_SET => [SET, COLLECTION, ITERABLE, KMAPPED, SERIALIZABLE].as_slice(),
        EMPTY_MAP => [MAP, SERIALIZABLE, KMAPPED].as_slice(),
        _ => return None,
    };
    if known.contains(&wanted) {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false)).then_some(false)
}

pub(crate) const ENUM_ENTRIES_METHODS: Methods = Methods {
    to_string: enum_entries_text,
    hash_code: enum_entries_hash,
    equals: enum_entries_equals,
    instance_of: |handle, wanted, hierarchy| {
        family_instance(class_for_entries(handle.key()), wanted, hierarchy)
    },
    crosses_jars: |_, _| true,
    compare_to: None,
};

pub(crate) const LAZY_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: identity_equals,
    instance_of: |handle, wanted, hierarchy| {
        let class = class_for_lazy(handle.key());
        if wanted == LAZY || wanted == OBJECT || hierarchy.is_instance(class, wanted) == Some(true)
        {
            Some(true)
        } else {
            hierarchy
                .is_interface(wanted)
                .map(|is_interface| !is_interface)
        }
    },
    crosses_jars: |_, _| true,
    compare_to: None,
};

pub(crate) const EMPTY_COLLECTION_METHODS: Methods = Methods {
    to_string: empty_text,
    hash_code: empty_hash,
    equals: empty_equals,
    instance_of: |handle, wanted, hierarchy| {
        family_instance(class_for_empty(handle.key()), wanted, hierarchy)
    },
    crosses_jars: |_, _| true,
    compare_to: None,
};

pub(crate) fn enum_entries_class(_: &str) -> &'static str {
    ENUM_ENTRIES_LIST
}

pub(crate) fn lazy_class(key: &str) -> &'static str {
    class_for_lazy(key)
}

pub(crate) fn empty_class(key: &str) -> &'static str {
    class_for_empty(key)
}

fn lazy_key(mode: &str, serial: u64, initializer: &Value) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::KotlinLazy,
        &[
            Value::Ref(Some(mode.to_owned())),
            Value::Long(serial as i64),
            initializer.clone(),
        ],
    ))
}

fn lazy_parts(value: &Value) -> Option<(String, Value)> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::KotlinLazy {
        return None;
    }
    let values = handle.values()?;
    match values.as_slice() {
        [Value::Ref(Some(mode)), Value::Long(_), initializer] => {
            Some((mode.clone(), initializer.clone()))
        }
        _ => None,
    }
}

fn lazy_state_key(value: &Value) -> Option<String> {
    let Value::Host(handle) = value else {
        return None;
    };
    (handle.kind() == HostKind::KotlinLazy).then(|| format!("kotlin-lazy|{}", handle.key()))
}

fn null_exception(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

fn caller_name() -> Option<String> {
    crate::runtime::stack_walk::snapshot()
        .first()
        .map(|(class, method)| format!("{}.{}", class.replace('/', "."), method))
}

fn parameter_message(name: &str) -> String {
    format!(
        "Parameter specified as non-null is null: method {}, parameter {}",
        caller_name().unwrap_or_else(|| "<unknown>.<unknown>".into()),
        name
    )
}

fn is_null(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Ref(None)))
}

pub(crate) fn invoke<H: crate::runtime::Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == "kotlin/enums/EnumEntriesKt" && r.name == "enumEntries" {
        if r.descriptor == "([Ljava/lang/Enum;)Lkotlin/enums/EnumEntries;" {
            let values = runtime.owned_array(args.first()?)?.into_owned();
            let serial = NEXT_ENTRIES.fetch_add(1, Ordering::Relaxed);
            return Some(Ok(Value::Host(HostRef::of_values(
                HostKind::KotlinEnumEntries,
                &std::iter::once(Value::Long(serial as i64))
                    .chain(values)
                    .collect::<Vec<_>>(),
            ))));
        }
        if r.descriptor == "(Lkotlin/jvm/functions/Function0;)Lkotlin/enums/EnumEntries;" {
            let array = runtime.invoke_callback(args.first()?, &[]);
            return match array {
                Ok(Some(value)) => invoke(
                    runtime,
                    &MethodRef {
                        owner: r.owner.clone(),
                        name: r.name.clone(),
                        descriptor: "([Ljava/lang/Enum;)Lkotlin/enums/EnumEntries;".into(),
                    },
                    &[value],
                ),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            };
        }
    }

    if r.owner == "kotlin/jvm/internal/Intrinsics" {
        let value = args.first();
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("areEqual", "(Ljava/lang/Object;Ljava/lang/Object;)Z") => {
                return Some(
                    runtime
                        .objects_equals(args.first()?, args.get(1)?)
                        .map(|v| Value::Int(i32::from(v))),
                );
            }
            ("checkNotNull", "(Ljava/lang/Object;)V") => {
                return Some(if is_null(value) {
                    Err(null_exception("java/lang/NullPointerException", None))
                } else {
                    Ok(Value::Void)
                });
            }
            ("checkNotNull", "(Ljava/lang/Object;Ljava/lang/String;)V") => {
                return Some(if is_null(value) {
                    Err(null_exception(
                        "java/lang/NullPointerException",
                        Some(crate::runtime::expect_ref(args.get(1)?)?.clone()),
                    ))
                } else {
                    Ok(Value::Void)
                });
            }
            ("checkNotNullParameter", "(Ljava/lang/Object;Ljava/lang/String;)V") => {
                return Some(if is_null(value) {
                    let name = crate::runtime::expect_ref(args.get(1)?).cloned().unwrap_or_default();
                    Err(null_exception(
                        "java/lang/NullPointerException",
                        Some(parameter_message(&name)),
                    ))
                } else {
                    Ok(Value::Void)
                });
            }
            ("checkParameterIsNotNull", "(Ljava/lang/Object;Ljava/lang/String;)V") => {
                return Some(if is_null(value) {
                    let name = crate::runtime::expect_ref(args.get(1)?).cloned().unwrap_or_default();
                    Err(null_exception(
                        "java/lang/IllegalArgumentException",
                        Some(parameter_message(&name)),
                    ))
                } else {
                    Ok(Value::Void)
                });
            }
            ("checkNotNullExpressionValue", "(Ljava/lang/Object;Ljava/lang/String;)V") => {
                return Some(if is_null(value) {
                    let name = crate::runtime::expect_ref(args.get(1)?).cloned().unwrap_or_default();
                    Err(null_exception(
                        "java/lang/NullPointerException",
                        Some(format!("{name} must not be null")),
                    ))
                } else {
                    Ok(Value::Void)
                });
            }
            ("checkExpressionValueIsNotNull", "(Ljava/lang/Object;Ljava/lang/String;)V") => {
                return Some(if is_null(value) {
                    let name = crate::runtime::expect_ref(args.get(1)?).cloned().unwrap_or_default();
                    Err(null_exception(
                        "java/lang/IllegalStateException",
                        Some(format!("{name} must not be null")),
                    ))
                } else {
                    Ok(Value::Void)
                });
            }
            ("throwUninitializedPropertyAccessException", "(Ljava/lang/String;)V") => {
                let name = crate::runtime::expect_ref(args.first()?)
                    .cloned()
                    .unwrap_or_default();
                return Some(Err(null_exception(
                    "kotlin/UninitializedPropertyAccessException",
                    Some(format!("lateinit property {name} has not been initialized")),
                )));
            }
            _ => {}
        }
    }

    if (r.owner == "kotlin/LazyKt"
        || r.owner == "kotlin/LazyKt__LazyJVMKt"
        || r.owner == "kotlin/LazyKt__LazyKt")
        && r.name == "lazy"
    {
        let (mode, initializer) = match r.descriptor.as_str() {
            "(Lkotlin/jvm/functions/Function0;)Lkotlin/Lazy;" => {
                ("SYNCHRONIZED", args.first()?.clone())
            }
            "(Lkotlin/LazyThreadSafetyMode;Lkotlin/jvm/functions/Function0;)Lkotlin/Lazy;" => {
                let mode = crate::runtime::expect_ref(args.first()?).and_then(|v| v.rsplit('/').next())?;
                // The exact slice models Kotlin's synchronized holder. The
                // JVM's publication/none implementations have distinct
                // concurrency contracts, so leave those calls counted.
                if mode != "SYNCHRONIZED" {
                    return None;
                }
                (mode, args.get(1)?.clone())
            }
            _ => return None,
        };
        if initializer == Value::Ref(None) {
            return Some(Err(null_exception(
                "java/lang/NullPointerException",
                Some(parameter_message("initializer")),
            )));
        }
        let value = lazy_key(
            mode,
            NEXT_LAZY.fetch_add(1, Ordering::Relaxed),
            &initializer,
        );
        Some(Ok(value))
    } else if (r.owner == "kotlin/LazyKt" || r.owner == "kotlin/LazyKt__LazyKt")
        && r.name == "lazyOf"
        && r.descriptor == "(Ljava/lang/Object;)Lkotlin/Lazy;"
    {
        let initializer = Value::Ref(Some("@fn:function.identity".into()));
        let value = lazy_key(
            "INITIALIZED",
            NEXT_LAZY.fetch_add(1, Ordering::Relaxed),
            &initializer,
        );
        if let Some(key) = lazy_state_key(&value) {
            runtime
                .fields
                .insert(key, args.first().cloned().unwrap_or(Value::Ref(None)));
        }
        Some(Ok(value))
    } else {
        kotlin_collections(runtime, r, args)
    }
}

fn kotlin_collections<H: crate::runtime::Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let owner = r.owner.as_str();
    if !matches!(
        owner,
        "kotlin/collections/CollectionsKt"
            | "kotlin/collections/CollectionsKt__CollectionsKt"
            | "kotlin/collections/MapsKt"
            | "kotlin/collections/MapsKt__MapsKt"
            | "kotlin/collections/SetsKt"
            | "kotlin/collections/SetsKt__SetsKt"
    ) {
        return None;
    }
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("emptyList", "()Ljava/util/List;") => Some(Ok(Value::Host(HostRef::new(
            HostKind::KotlinEmptyCollection,
            "list",
        )))),
        ("emptySet", "()Ljava/util/Set;") => Some(Ok(Value::Host(HostRef::new(
            HostKind::KotlinEmptyCollection,
            "set",
        )))),
        ("emptyMap", "()Ljava/util/Map;") => Some(Ok(Value::Host(HostRef::new(
            HostKind::KotlinEmptyCollection,
            "map",
        )))),
        ("mutableListOf", "([Ljava/lang/Object;)Ljava/util/List;")
            if runtime
                .owned_array(args.first()?)
                .is_some_and(|values| values.is_empty()) =>
        {
            runtime.host.empty_mutable_collection("list").map(Ok)
        }
        ("mutableListOf", "()Ljava/util/List;") => {
            runtime.host.empty_mutable_collection("list").map(Ok)
        }
        ("listOf", "([Ljava/lang/Object;)Ljava/util/List;")
            if runtime
                .owned_array(args.first()?)
                .is_some_and(|values| values.is_empty()) =>
        {
            Some(Ok(Value::Host(HostRef::new(
                HostKind::KotlinEmptyCollection,
                "list",
            ))))
        }
        _ => None,
    }
}

pub(crate) fn instance<H: crate::runtime::Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if let Value::Host(handle) = receiver {
        match handle.kind() {
            HostKind::KotlinLazy => {
                let key = lazy_state_key(receiver)?;
                match (r.name.as_str(), r.descriptor.as_str()) {
                    ("isInitialized", "()Z") => {
                        return Some(Ok(Value::Int(i32::from(runtime.fields.contains_key(&key)))));
                    }
                    ("toString", "()Ljava/lang/String;") => {
                        if let Some(value) = runtime.fields.get(&key).cloned() {
                            return Some(
                                runtime
                                    .value_of_text(
                                        &value,
                                        "Ljava/lang/Object;",
                                        "kotlin/Lazy",
                                        "toString",
                                    )
                                    .map(|v| Value::Ref(Some(v))),
                            );
                        }
                        return Some(Ok(Value::Ref(Some(
                            "Lazy value not initialized yet.".into(),
                        ))));
                    }
                    ("getValue", "()Ljava/lang/Object;") => {
                        if let Some(value) = runtime.fields.get(&key).cloned() {
                            return Some(Ok(value));
                        }
                        let (_, initializer) = lazy_parts(receiver)?;
                        let initialized = if let Value::Host(initializer_handle) = &initializer {
                            if initializer_handle.kind() == HostKind::JarObject {
                                crate::runtime::jar_object::marker_class(initializer_handle.key()).map(
                                    |class| {
                                        runtime.invoke(
                                            class,
                                            "invoke",
                                            "()Ljava/lang/Object;",
                                            Some(initializer.clone()),
                                            &[],
                                        )
                                    },
                                )
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let initialized = match initialized {
                            Some(result) => result,
                            None => match runtime.invoke_callback(&initializer, &[]) {
                                Ok(Some(value)) => Ok(value),
                                Ok(None) => Err(NativeError::UnknownHostCall(r.clone())),
                                Err(error) => Err(error),
                            },
                        };
                        return Some(match initialized {
                            Ok(value) => {
                                runtime.fields.insert(key, value.clone());
                                Ok(value)
                            }
                            Err(error) => Err(error),
                        });
                    }
                    _ => {}
                }
            }
            HostKind::KotlinEmptyCollection => {
                let kind = handle.key();
                match (r.name.as_str(), r.descriptor.as_str()) {
                    // EmptyList/EmptySet/EmptyMap are Serializable singletons.
                    ("readResolve", "()Ljava/lang/Object;") => {
                        return Some(Ok(receiver.clone()));
                    }
                    ("size", "()I") => return Some(Ok(Value::Int(0))),
                    ("isEmpty", "()Z") => return Some(Ok(Value::Int(1))),
                    ("get", "(I)Ljava/lang/Object;") if kind == "list" => {
                        return Some(Err(null_exception(
                            "java/lang/IndexOutOfBoundsException",
                            Some("Empty list doesn't contain element at index 0.".into()),
                        )));
                    }
                    ("get", "(Ljava/lang/Object;)Ljava/lang/Object;") if kind == "map" => {
                        return Some(Ok(Value::Ref(None)));
                    }
                    ("containsKey", "(Ljava/lang/Object;)Z") if kind == "map" => {
                        return Some(Ok(Value::Int(0)));
                    }
                    ("contains", "(Ljava/lang/Object;)Z") => return Some(Ok(Value::Int(0))),
                    ("add" | "remove" | "clear" | "put" | "putAll", _) => {
                        return Some(Err(null_exception(
                            "java/lang/UnsupportedOperationException",
                            Some("Operation is not supported for read-only collection".into()),
                        )));
                    }
                    _ => {}
                }
            }
            HostKind::KotlinEnumEntries => {
                let values = handle.values()?;
                let values = values.get(1..)?;
                match (r.name.as_str(), r.descriptor.as_str()) {
                    ("size", "()I") => return Some(Ok(Value::Int(values.len() as i32))),
                    ("isEmpty", "()Z") => {
                        return Some(Ok(Value::Int(i32::from(values.is_empty()))));
                    }
                    ("get", "(I)Ljava/lang/Object;") => {
                        let index = match args.first() {
                            Some(Value::Int(index)) => *index,
                            _ => -1,
                        };
                        return Some(values.get(index as usize).cloned().ok_or_else(|| {
                            null_exception(
                                "java/lang/IndexOutOfBoundsException",
                                Some(format!("Index: {index}, Size: {}", values.len())),
                            )
                        }));
                    }
                    ("add" | "remove" | "clear", _) => {
                        return Some(Err(null_exception(
                            "java/lang/UnsupportedOperationException",
                            Some("Operation is not supported for read-only collection".into()),
                        )));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lazy holder's class follows its mode when its initializer is a
    /// callback handle, whose marker no text reader takes back (#261).
    #[test]
    fn a_lazy_holder_over_a_lambda_keeps_its_modes_class() {
        let lambda = crate::runtime::callback_marker::test_token("demo/A#0");
        for (mode, class) in [
            ("NONE", "kotlin/UnsafeLazyImpl"),
            ("PUBLICATION", "kotlin/SafePublicationLazyImpl"),
            ("INITIALIZED", INITIALIZED_LAZY),
            ("SYNCHRONIZED", "kotlin/SynchronizedLazyImpl"),
        ] {
            let Value::Host(handle) = lazy_key(mode, 7, &lambda) else {
                unreachable!()
            };
            assert_eq!(lazy_class(handle.key()), class, "{mode}");
        }
    }
}
