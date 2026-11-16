//! DataFixerUpper Either values as typed host handles.
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested, exact_class_rules_out};
use crate::runtime::{Host, MethodRef, NativeError, Runtime, Value};

const OWNER: &str = "com/mojang/datafixers/util/Either";
const APP: &str = "com/mojang/datafixers/kinds/App";
const OBJECT: &str = "java/lang/Object";
static NEXT_EITHER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn left(value: &Value) -> Value {
    either(true, value)
}

pub(crate) fn right(value: &Value) -> Value {
    either(false, value)
}

fn either(left: bool, value: &Value) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::DfuEither,
        &[
            Value::Long(NEXT_EITHER.fetch_add(1, Ordering::Relaxed) as i64),
            Value::Ref(Some(if left { "left" } else { "right" }.into())),
            value.clone(),
        ],
    ))
}

fn parts(value: &Value) -> Option<(bool, Value)> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::DfuEither {
        return None;
    }
    let [_, Value::Ref(Some(side)), value] = <[Value; 3]>::try_from(handle.values()?).ok()? else {
        return None;
    };
    Some((side == "left", value))
}

pub(crate) fn static_call(r: &MethodRef, args: &[Value]) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER {
        return None;
    }
    let [value] = args else {
        return None;
    };
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("left", "(Ljava/lang/Object;)Lcom/mojang/datafixers/util/Either;") => {
            Some(Ok(either(true, value)))
        }
        ("right", "(Ljava/lang/Object;)Lcom/mojang/datafixers/util/Either;") => {
            Some(Ok(either(false, value)))
        }
        _ => None,
    }
}

pub(crate) fn host_instance_call(
    r: &MethodRef,
    receiver: &Value,
    _args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER {
        return None;
    }
    let (left, value) = parts(receiver)?;
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("left", "()Ljava/util/Optional;") => Some(Ok(if left {
            crate::runtime::host_optional::optional_wrap(value)
        } else {
            crate::runtime::host_optional::empty_optional()
        })),
        ("right", "()Ljava/util/Optional;") => Some(Ok(if left {
            crate::runtime::host_optional::empty_optional()
        } else {
            crate::runtime::host_optional::optional_wrap(value)
        })),
        ("swap", "()Lcom/mojang/datafixers/util/Either;") => Some(Ok(either(!left, &value))),
        _ => None,
    }
}

pub(crate) fn runtime_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    if r.owner != OWNER {
        return Ok(None);
    }
    let Some(receiver) = receiver else {
        return Ok(None);
    };
    let Some((left, value)) = parts(receiver) else {
        return Ok(None);
    };
    let invoke = |runtime: &mut Runtime<'_, H>, callback: &Value, value: Value| {
        runtime.invoke_callback(callback, &[value])
    };
    match r.name.as_str() {
        "map"
            if r.descriptor
                == "(Ljava/util/function/Function;Ljava/util/function/Function;)Ljava/lang/Object;" =>
        {
            let Some(callback) = args.get(if left { 0 } else { 1 }) else {
                return Ok(None);
            };
            Ok(invoke(runtime, callback, value)?)
        }
        "mapBoth"
            if r.descriptor
                == "(Ljava/util/function/Function;Ljava/util/function/Function;)Lcom/mojang/datafixers/util/Either;" =>
        {
            let Some(callback) = args.get(if left { 0 } else { 1 }) else {
                return Ok(None);
            };
            let Some(mapped) = invoke(runtime, callback, value)? else {
                return Ok(None);
            };
            Ok(Some(either(left, &mapped)))
        }
        "mapLeft"
            if r.descriptor
                == "(Ljava/util/function/Function;)Lcom/mojang/datafixers/util/Either;"
                && left =>
        {
            let Some(callback) = args.first() else {
                return Ok(None);
            };
            let Some(mapped) = invoke(runtime, callback, value)? else {
                return Ok(None);
            };
            Ok(Some(either(true, &mapped)))
        }
        "mapLeft"
            if r.descriptor
                == "(Ljava/util/function/Function;)Lcom/mojang/datafixers/util/Either;" =>
        {
            Ok(Some(either(false, &value)))
        }
        "mapRight"
            if r.descriptor
                == "(Ljava/util/function/Function;)Lcom/mojang/datafixers/util/Either;"
                && !left =>
        {
            let Some(callback) = args.first() else {
                return Ok(None);
            };
            let Some(mapped) = invoke(runtime, callback, value)? else {
                return Ok(None);
            };
            Ok(Some(either(false, &mapped)))
        }
        "mapRight"
            if r.descriptor
                == "(Ljava/util/function/Function;)Lcom/mojang/datafixers/util/Either;" =>
        {
            Ok(Some(either(true, &value)))
        }
        "ifLeft"
            if r.descriptor
                == "(Ljava/util/function/Consumer;)Lcom/mojang/datafixers/util/Either;"
                && left =>
        {
            let Some(callback) = args.first() else {
                return Ok(None);
            };
            let _ = invoke(runtime, callback, value)?;
            Ok(Some(receiver.clone()))
        }
        "ifLeft"
            if r.descriptor
                == "(Ljava/util/function/Consumer;)Lcom/mojang/datafixers/util/Either;" =>
        {
            Ok(Some(receiver.clone()))
        }
        "ifRight"
            if r.descriptor
                == "(Ljava/util/function/Consumer;)Lcom/mojang/datafixers/util/Either;"
                && !left =>
        {
            let Some(callback) = args.first() else {
                return Ok(None);
            };
            let _ = invoke(runtime, callback, value)?;
            Ok(Some(receiver.clone()))
        }
        "ifRight"
            if r.descriptor
                == "(Ljava/util/function/Consumer;)Lcom/mojang/datafixers/util/Either;" =>
        {
            Ok(Some(receiver.clone()))
        }
        "orThrow" if r.descriptor == "()Ljava/lang/Object;" && left => Ok(Some(value)),
        "orThrow" if r.descriptor == "()Ljava/lang/Object;" => {
            // The right branch's RuntimeException/cause construction depends
            // on the payload's toString and Throwable identity. Keep the
            // boundary counted rather than returning a neutral value.
            runtime.host.invoke_instance(r, receiver, args).map(Some)
        }
        _ => Ok(None),
    }
}

fn either_text(handle: &HostRef, nested: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    let (left, value) = parts(&Value::Host(handle.clone()))
        .ok_or_else(|| NativeError::Metadata("malformed DFU Either handle".into()))?;
    Ok(nested
        .text(&value)?
        .map(|text| format!("{}[{text}]", if left { "Left" } else { "Right" })))
}

fn either_hash(handle: &HostRef, nested: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    let (_, value) = parts(&Value::Host(handle.clone()))
        .ok_or_else(|| NativeError::Metadata("malformed DFU Either handle".into()))?;
    nested.hash(&value)
}

fn either_equals(
    handle: &HostRef,
    other: &Value,
    nested: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    let (left, value) = parts(&Value::Host(handle.clone()))
        .ok_or_else(|| NativeError::Metadata("malformed DFU Either handle".into()))?;
    let Some((other_left, other_value)) = parts(other) else {
        return Ok(Some(false));
    };
    if left != other_left {
        return Ok(Some(false));
    }
    nested.equals(&value, &other_value)
}

fn either_instance_of(class: &str, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
    if [OWNER, APP, OBJECT].contains(&wanted) || hierarchy.is_instance(class, wanted) == Some(true)
    {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false) || exact_class_rules_out(class, wanted))
        .then_some(false)
}

pub(crate) fn either_types(_: &str) -> &'static [&'static str] {
    &[OWNER, APP, OBJECT]
}

pub(crate) const EITHER_METHODS: Methods = Methods {
    to_string: either_text,
    hash_code: either_hash,
    equals: either_equals,
    instance_of: |_, wanted, hierarchy| either_instance_of(OWNER, wanted, hierarchy),
    crosses_jars: |handle, crosses| {
        parts(&Value::Host(handle.clone())).is_some_and(|(_, value)| crosses(&value))
    },
    compare_to: None,
};
