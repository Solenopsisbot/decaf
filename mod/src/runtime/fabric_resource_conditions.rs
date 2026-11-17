//! Fabric's `ResourceConditionType.create` and `ResourceConditions.register` (#454).
//!
//! What Fabric API 0.166.2 (`fabric-resource-conditions-api-v1` 6.2.1) does, read from its
//! bytecode:
//!
//! * `ResourceConditionType.create(id, codec)`: `requireNonNull(id, "id cannot be null")`, then
//!   `requireNonNull(codec, "codec cannot be null")`, then a new anonymous
//!   `ResourceConditionType$1` holding both. It overrides only `id()` and `codec()`, so `equals`
//!   is identity, and `hashCode`/`toString` print the identity hash.
//! * `ResourceConditions.register(type)`: `requireNonNull(type, "Condition may not be null.")`,
//!   then `REGISTERED_CONDITIONS.put(type.id(), type)`, and if that replaced an entry,
//!   `IllegalArgumentException("Duplicate resource condition registered with id " + id)`. The
//!   map holds the new type when the exception is thrown.
//!
//! Counted rather than answered:
//! * a null argument: a counted call answers null too, so a null here may be a value Java never
//!   saw as null, and throwing Fabric's `NullPointerException` could be wrong (`open_options.rs`
//!   counts a null operand the same way);
//! * an id among Fabric's own conditions (`fabric:true`, `fabric:and`, …): its module's `main`
//!   entrypoint registers them, and whether that ran before this jar's init depends on load
//!   order (#457);
//! * a type that isn't one `create` made (a jar's own implementation): its `id()` is the jar's;
//! * evaluating a condition, and the type's `hashCode`/`toString`.

use crate::runtime::host_ref::{HostKind, HostRef};
use crate::runtime::{MethodRef, NativeError, Value, host_ids};
use std::collections::BTreeMap;

const TYPE: &str = "net/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType";
const CONDITIONS: &str = "net/fabricmc/fabric/api/resource/conditions/v1/ResourceConditions";
const CREATE: &str = "(Lnet/minecraft/resources/Identifier;Lcom/mojang/serialization/MapCodec;)Lnet/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType;";
const REGISTER: &str = "(Lnet/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType;)V";

/// The paths of the types `DefaultResourceConditionTypes` registers under `fabric:`.
const FABRIC_DEFAULTS: &[&str] = &[
    "true",
    "false",
    "not",
    "or",
    "and",
    "all_mods_loaded",
    "any_mods_loaded",
    "tags_populated",
    "features_enabled",
    "registry_contains",
];

/// One host's condition types and its `REGISTERED_CONDITIONS`.
#[derive(Debug, Default, Clone)]
pub(crate) struct State {
    /// Handle key → (`namespace:path`, the `Identifier` and the codec `create` was given).
    types: BTreeMap<String, (String, Value, Value)>,
    /// Registered id → the handle key of the type registered last under it.
    registered: BTreeMap<String, String>,
    next: u64,
}

impl State {
    /// The values the host holds: collector roots (a codec may hold a jar object).
    pub(crate) fn values(&self) -> impl Iterator<Item = &Value> {
        self.types.values().flat_map(|(_, id, codec)| [id, codec])
    }
}

/// A type handle's declared types: it is surely a `ResourceConditionType` (its class is the
/// anonymous `ResourceConditionType$1`, which `getClass` would need: counted).
pub(crate) fn declared_types(_: &str) -> &'static [&'static str] {
    &[TYPE]
}

fn thrown(class: &str, message: String) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: Some(message),
        frame: None,
    }
}

fn is_fabric_default(id: &str) -> bool {
    id.strip_prefix("fabric:")
        .is_some_and(|path| FABRIC_DEFAULTS.contains(&path))
}

/// `create` and `register`; `None` leaves the call counted.
pub(crate) fn static_call(
    state: &mut State,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (TYPE, "create", CREATE) => {
            let [id, codec] = args else { return None };
            if matches!(codec, Value::Ref(None)) {
                return None;
            }
            let text = host_ids::identifier_key(id)?.to_owned();
            state.next += 1;
            let key = format!("rct#{}", state.next);
            state
                .types
                .insert(key.clone(), (text, id.clone(), codec.clone()));
            Some(Ok(Value::Host(HostRef::new(
                HostKind::FabricResourceConditionType,
                key,
            ))))
        }
        (CONDITIONS, "register", REGISTER) => {
            let [Value::Host(handle)] = args else {
                return None;
            };
            if handle.kind() != HostKind::FabricResourceConditionType {
                return None;
            }
            let (id, _, _) = state.types.get(handle.key())?;
            if is_fabric_default(id) {
                return None;
            }
            let id = id.clone();
            // `put` first: the new type replaces the old one even when it then throws.
            match state.registered.insert(id.clone(), handle.key().to_owned()) {
                Some(_) => Some(Err(thrown(
                    "java/lang/IllegalArgumentException",
                    format!("Duplicate resource condition registered with id {id}"),
                ))),
                None => Some(Ok(Value::Void)),
            }
        }
        _ => None,
    }
}

/// `id()` and `codec()` on a type `create` made.
pub(crate) fn instance_call(
    state: &State,
    r: &MethodRef,
    receiver: &Value,
) -> Option<Result<Value, NativeError>> {
    let Value::Host(handle) = receiver else {
        return None;
    };
    if handle.kind() != HostKind::FabricResourceConditionType {
        return None;
    }
    let (_, id, codec) = state.types.get(handle.key())?;
    match (r.name.as_str(), r.descriptor.as_str()) {
        // The very `Identifier` `create` was given, as Java's `val$id`.
        ("id", "()Lnet/minecraft/resources/Identifier;") => Some(Ok(id.clone())),
        ("codec", "()Lcom/mojang/serialization/MapCodec;") => Some(Ok(codec.clone())),
        _ => None,
    }
}
