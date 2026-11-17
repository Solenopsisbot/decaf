//! Typed, immutable handles for NeoForge's `PayloadRegistrar` builder.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::{
    Value,
    host_ref::{HostKind, HostRef},
};

static NEXT_REGISTRAR: AtomicU64 = AtomicU64::new(1);
const CLASS: &str = "net/neoforged/neoforge/network/registration/PayloadRegistrar";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegistrarState {
    /// The active mod context that owns this registrar.
    pub mod_id: Value,
    pub version: Value,
    pub optional: bool,
    pub thread: Value,
}

fn handle(state: RegistrarState) -> Value {
    let serial = NEXT_REGISTRAR.fetch_add(1, Ordering::Relaxed);
    Value::Host(HostRef::of_values(
        HostKind::NeoForgePayloadRegistrar,
        &[
            Value::Long(serial as i64),
            state.mod_id,
            state.version,
            Value::Int(i32::from(state.optional)),
            state.thread,
        ],
    ))
}

pub(crate) fn registrar(version: &Value, mod_id: &Value) -> Value {
    handle(RegistrarState {
        mod_id: mod_id.clone(),
        version: version.clone(),
        optional: false,
        // PayloadRegistrar's constructor initializes HandlerThread.MAIN.
        thread: Value::Ref(Some("MAIN".into())),
    })
}

pub(crate) fn state(receiver: &Value) -> Option<RegistrarState> {
    let Value::Host(handle) = receiver else {
        return None;
    };
    if handle.kind() != HostKind::NeoForgePayloadRegistrar {
        return None;
    }
    let values = handle.values()?;
    if values.len() != 5 {
        return None;
    }
    Some(RegistrarState {
        mod_id: values[1].clone(),
        version: values[2].clone(),
        optional: matches!(values[3], Value::Int(value) if value != 0),
        thread: values[4].clone(),
    })
}

pub(crate) fn instance_call(
    r: &crate::runtime::MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Value> {
    if r.owner != CLASS {
        return None;
    }
    let mut next = state(receiver)?;
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "versioned",
            "(Ljava/lang/String;)Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;",
        ) => {
            next.version = args.first()?.clone();
            Some(handle(next))
        }
        ("optional", "()Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;") => {
            next.optional = true;
            Some(handle(next))
        }
        (
            "executesOn",
            "(Lnet/neoforged/neoforge/network/registration/HandlerThread;)Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;",
        ) => {
            next.thread = args.first()?.clone();
            Some(handle(next))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{MethodRef, host_ref::ClassHierarchy};

    struct Empty;
    impl ClassHierarchy for Empty {
        fn is_instance(&self, class: &str, wanted: &str) -> Option<bool> {
            Some(class == wanted || wanted == "java/lang/Object")
        }
        fn is_interface(&self, _: &str) -> Option<bool> {
            None
        }
    }

    #[test]
    fn registrar_refinements_are_distinct_and_copy_state() {
        let value = registrar(
            &Value::Ref(Some("1".into())),
            &Value::Ref(Some("demo".into())),
        );
        assert!(matches!(
            &value,
            Value::Host(handle)
                if handle.kind() == HostKind::NeoForgePayloadRegistrar
                    && handle.instance_of(CLASS, &Empty) == Some(true)
        ));
        let versioned = instance_call(&MethodRef {
            owner: CLASS.into(), name: "versioned".into(),
            descriptor: "(Ljava/lang/String;)Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;".into(),
        }, &value, &[Value::Ref(Some("2".into()))]).unwrap();
        assert_ne!(value, versioned);
        assert_eq!(
            state(&versioned).unwrap().mod_id,
            Value::Ref(Some("demo".into()))
        );
        assert_eq!(
            state(&versioned).unwrap().version,
            Value::Ref(Some("2".into()))
        );
        assert!(!state(&versioned).unwrap().optional);
        let optional = instance_call(
            &MethodRef {
                owner: CLASS.into(),
                name: "optional".into(),
                descriptor: "()Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;"
                    .into(),
            },
            &value,
            &[],
        )
        .unwrap();
        assert!(state(&optional).unwrap().optional);
        let threaded = instance_call(&MethodRef {
            owner: CLASS.into(), name: "executesOn".into(),
            descriptor: "(Lnet/neoforged/neoforge/network/registration/HandlerThread;)Lnet/neoforged/neoforge/network/registration/PayloadRegistrar;".into(),
        }, &value, &[Value::Ref(Some("NETWORK".into()))]).unwrap();
        assert_eq!(
            state(&threaded).unwrap().thread,
            Value::Ref(Some("NETWORK".into()))
        );
    }
}
