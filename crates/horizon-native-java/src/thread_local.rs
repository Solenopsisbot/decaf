//! The synchronous subset of `java.lang.ThreadLocal` used by mod bootstrap code.
//!
//! Each native interpreter run has one executing Java thread. A holder therefore
//! keeps one slot in `Runtime::fields`; calls from a real parallel host thread
//! are outside this model and remain counted.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::host_ref::{HostKind, HostRef, Methods};
use crate::{MethodRef, NativeError, Runtime, Value};

const THREAD_LOCAL: &str = "java/lang/ThreadLocal";

/// `withInitial` returns the private JDK implementation, not ThreadLocal itself.
pub(crate) fn class_of_key(_key: &str) -> &str {
    "java/lang/ThreadLocal$SuppliedThreadLocal"
}
static NEXT_HOLDER: AtomicU64 = AtomicU64::new(0);

pub(crate) const THREAD_LOCAL_METHODS: Methods = crate::host_events::DECLARED_TYPE_METHODS;

fn parts(value: &Value) -> Option<(String, Value)> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::ThreadLocal {
        return None;
    }
    let values = handle.values()?;
    let [Value::Long(_serial), supplier] = values.as_slice() else {
        return None;
    };
    Some((format!("thread-local|{}", handle.key()), supplier.clone()))
}

fn jar_parts<H: crate::Host>(runtime: &Runtime<'_, H>, value: &Value) -> Option<(String, String)> {
    let class = crate::jar_object::object_class(value)?;
    runtime.jar_extends(class, THREAD_LOCAL).then(|| {
        let key = match value {
            Value::Host(handle) => handle.key().to_owned(),
            _ => String::new(),
        };
        (format!("thread-local|{key}"), class.to_owned())
    })
}

fn null_supplier() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

/// Handles `ThreadLocal.withInitial`, `get`, `set`, and `remove`.
/// Everything else deliberately falls through to the counted host path.
pub(crate) fn call<H: crate::Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if receiver.is_none()
        && r.owner == THREAD_LOCAL
        && r.name == "withInitial"
        && r.descriptor == "(Ljava/util/function/Supplier;)Ljava/lang/ThreadLocal;"
    {
        let supplier = args.first()?.clone();
        if matches!(supplier, Value::Ref(None)) {
            return Some(Err(null_supplier()));
        }
        let serial = NEXT_HOLDER.fetch_add(1, Ordering::Relaxed) as i64;
        return Some(Ok(Value::Host(HostRef::of_values(
            HostKind::ThreadLocal,
            &[Value::Long(serial), supplier],
        ))));
    }

    let receiver = receiver?;
    if let Some((state_key, class)) = jar_parts(runtime, receiver) {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("get", "()Ljava/lang/Object;") => {
                if let Some(value) = runtime.fields.get(&state_key).cloned() {
                    return Some(Ok(value));
                }
                let initial = MethodRef {
                    owner: class.clone(),
                    name: "initialValue".into(),
                    descriptor: "()Ljava/lang/Object;".into(),
                };
                let owner = runtime.jar_method_owner(&class, &initial.name, &initial.descriptor);
                let value = if let Some(owner) = owner {
                    match runtime.invoke(
                        &owner,
                        "initialValue",
                        "()Ljava/lang/Object;",
                        Some(receiver.clone()),
                        &[],
                    ) {
                        Ok(value) => value,
                        Err(error) => return Some(Err(error)),
                    }
                } else {
                    Value::Ref(None)
                };
                runtime.fields.insert(state_key, value.clone());
                return Some(Ok(value));
            }
            ("set", "(Ljava/lang/Object;)V") => {
                let value = args.first()?.clone();
                runtime.fields.insert(state_key, value);
                return Some(Ok(Value::Void));
            }
            ("remove", "()V") => {
                runtime.fields.remove(&state_key);
                return Some(Ok(Value::Void));
            }
            _ => {
                let _ = class;
            }
        }
    }
    if r.owner != THREAD_LOCAL {
        return None;
    }
    if r.owner != THREAD_LOCAL {
        return None;
    }
    let (state_key, supplier) = parts(receiver)?;
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("get", "()Ljava/lang/Object;") => {
            if let Some(value) = runtime.fields.get(&state_key).cloned() {
                return Some(Ok(value));
            }
            let result = match runtime.invoke_callback(&supplier, &[]) {
                Ok(Some(value)) => value,
                Ok(None) => return Some(runtime.host.invoke(r, &[])),
                Err(error) => return Some(Err(error)),
            };
            runtime.fields.insert(state_key, result.clone());
            Some(Ok(result))
        }
        ("set", "(Ljava/lang/Object;)V") => {
            let value = args.first()?.clone();
            runtime.fields.insert(state_key, value);
            Some(Ok(Value::Void))
        }
        ("remove", "()V") => {
            runtime.fields.remove(&state_key);
            Some(Ok(Value::Void))
        }
        _ => None,
    }
}
