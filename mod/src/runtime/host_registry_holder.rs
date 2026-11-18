//! `Holder.Reference` and `Holder.Direct` handles produced by a jar's own
//! registry calls.
//!
//! A registration carries the registry key, entry id, registered value and
//! binding state. A reference returned for an observed registration is bound
//! when the registry lifecycle has supplied its value; a direct holder keeps
//! only the value and therefore has no invented registry id. A jar can obtain
//! these handles only from modeled host calls; spelling an old marker in a
//! `String` never aliases one. Vanilla holders whose registry contents or tags
//! belong to the game stay outside this model.

use crate::runtime::dfu_either;
use crate::runtime::host_ids;
use crate::runtime::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested, exact_class_rules_out};
use crate::runtime::{Host, MethodRef, NativeError, StdHost, Value};

const OWNER: &str = "net/minecraft/core/Holder$Reference";
const HOLDER: &str = "net/minecraft/core/Holder";
const KIND: &str = "net/minecraft/core/Holder$Kind";
const HOLDER_TYPES: &[&str] = &[OWNER, HOLDER, "java/lang/Object"];

/// A holder reference is a concrete game object. Its object hash is the
/// identity hash and therefore remains counted; the identity itself is exact
/// because each observed registration carries one registry key and value.
fn holder_with_owner(
    registry_key: Value,
    id: Value,
    value: Value,
    bound: bool,
    game_owned: bool,
    owner: Value,
) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::RegistryHolder,
        &[
            registry_key,
            id,
            value,
            Value::Int(i32::from(bound)),
            Value::Int(i32::from(game_owned)),
            owner,
        ],
    ))
}

fn holder(registry_key: Value, id: Value, value: Value, bound: bool, game_owned: bool) -> Value {
    holder_with_owner(registry_key, id, value, bound, game_owned, Value::Ref(None))
}

struct Parts {
    registry_key: Value,
    id: String,
    value: Value,
    bound: bool,
    game_owned: bool,
    owner: Value,
}

fn parts(value: &Value) -> Option<Parts> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::RegistryHolder {
        return None;
    }
    let [registry_key, id, value, bound, game_owned, owner] =
        <[Value; 6]>::try_from(handle.values()?).ok()?;
    let id = host_ids::identifier_key(&id)?.to_owned();
    host_ids::resource_key_parts(&registry_key)?;
    let bound = match bound {
        Value::Int(0) => false,
        Value::Int(1) => true,
        _ => return None,
    };
    let game_owned = match game_owned {
        Value::Int(0) => false,
        Value::Int(1) => true,
        _ => return None,
    };
    Some(Parts {
        registry_key,
        id,
        value,
        bound,
        game_owned,
        owner,
    })
}

fn entry_key(parts: &Parts) -> Value {
    host_ids::resource_key(
        &parts.registry_key,
        &host_ids::identifier_of_text(&parts.id),
        Some(parts.id.clone()),
    )
}

fn registry_key_for_registry(registry: &Value) -> Option<Value> {
    let id = crate::runtime::host_registry::registry_id(registry)?;
    Some(host_ids::registry_key_of(id))
}

fn key_parts(key: &Value) -> Option<(Value, Value, String)> {
    let registry_key = host_ids::key_registry(key)?;
    let id = host_ids::key_location(key)?;
    let id_text = host_ids::identifier_key(&id)?.to_owned();
    Some((registry_key, id, id_text))
}

fn same_registry(registry: &Value, registry_key: &Value) -> bool {
    let Some(registry_id) = crate::runtime::host_registry::registry_id(registry) else {
        return false;
    };
    host_ids::registry_name(registry_key).as_deref() == Some(registry_id)
}

fn make_for_identifier(registry: &Value, id: &Value, value: &Value, bound: bool) -> Option<Value> {
    let registry_key = registry_key_for_registry(registry)?;
    let id_text = host_ids::identifier_key(id)?.to_owned();
    Some(holder(
        registry_key,
        host_ids::identifier_of_text(&id_text),
        value.clone(),
        bound,
        false,
    ))
}

fn make_for_key(registry: &Value, key: &Value, value: &Value, bound: bool) -> Option<Value> {
    let (registry_key, id, _id_text) = key_parts(key)?;
    if !same_registry(registry, &registry_key) {
        return None;
    }
    Some(holder(registry_key, id, value.clone(), bound, false))
}

/// A holder for an entry the 26.3 game registered before the mod runs.
///
/// The registry id and entry id are exact facts from the game's static
/// initialisers. `value` remains a game object handle; this module does not
/// invent any of that object's methods.
pub(crate) fn vanilla(registry: &str, id: &str, value: Value) -> Value {
    holder(
        host_ids::registry_key_of(registry),
        host_ids::identifier_of_text(id),
        value,
        true,
        true,
    )
}

/// The registered id of a vanilla holder, for game surfaces that accept a
/// holder as an argument while retaining the holder's typed identity.
pub(crate) fn registered_id(value: &Value) -> Option<String> {
    parts(value).map(|parts| parts.id)
}

fn reference_from_key(key: &Value, value: Value, bound: bool, owner: Value) -> Option<Value> {
    let (registry_key, id, _id_text) = key_parts(key)?;
    Some(holder_with_owner(
        registry_key,
        id,
        value,
        bound,
        false,
        owner,
    ))
}

fn owner_supported(owner: &Value) -> bool {
    match owner {
        Value::Ref(None) => true,
        Value::Host(handle) if handle.kind() == HostKind::Registry => {
            handle.key().chars().next().is_some_and(|implementation| {
                implementation == crate::runtime::host_registry::MAPPED
                    || implementation == crate::runtime::host_registry::DEFAULTED_MAPPED
            })
        }
        _ => false,
    }
}

fn intrusive_holder(owner: Value, value: Value) -> Value {
    holder_with_owner(
        Value::Ref(None),
        Value::Ref(None),
        value,
        false,
        false,
        owner,
    )
}

fn intrusive_value(value: &Value) -> Option<(Value, Value)> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::RegistryHolder {
        return None;
    }
    let [
        Value::Ref(None),
        Value::Ref(None),
        value,
        Value::Int(0),
        Value::Int(0),
        owner,
    ] = <[Value; 6]>::try_from(handle.values()?).ok()?
    else {
        return None;
    };
    Some((value, owner))
}

fn exact_text(host: &mut StdHost, value: &Value) -> Result<Option<String>, NativeError> {
    if crate::runtime::jar_object::is_object(value) {
        return Ok(None);
    }
    if let Some(text) = host.modelled_to_string_checked(value)? {
        return Ok(Some(text));
    }
    Ok(match value {
        Value::Ref(Some(text)) if crate::runtime::value_class::is_plain_string(text) => Some(text.clone()),
        Value::Ref(None) => Some("null".into()),
        Value::Host(_) => None,
        _ => Some(crate::runtime::value_text(value)),
    })
}

fn intrusive_unbound_value(
    host: &mut StdHost,
    value: &Value,
    owner: &Value,
) -> Result<Option<NativeError>, NativeError> {
    let Some(text) = exact_text(host, value)? else {
        return Ok(None);
    };
    let Some(owner) = exact_text(host, owner)? else {
        return Ok(None);
    };
    Ok(Some(NativeError::UncaughtException {
        class: "java/lang/IllegalStateException".into(),
        message: Some(format!(
            "Trying to access unbound value '{text}' from registry {owner}"
        )),
        frame: None,
    }))
}

/// `Registry.registerForHolder`'s two 26.3 overloads. An unmodelled registry,
/// id or value shape stays counted instead of inventing a holder.
pub(crate) fn static_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == OWNER && r.name == "createStandAlone" {
        if r.descriptor
            != "(Lnet/minecraft/core/HolderOwner;Lnet/minecraft/resources/ResourceKey;)Lnet/minecraft/core/Holder$Reference;"
        {
            return Some(host.unknown_host_call(r));
        }
        let (Some(owner), Some(key)) = (args.first(), args.get(1)) else {
            return Some(host.unknown_host_call(r));
        };
        if !owner_supported(owner) {
            return Some(host.unknown_host_call(r));
        }
        return Some(
            reference_from_key(key, Value::Ref(None), false, owner.clone())
                .map_or_else(|| host.unknown_host_call(r), Ok),
        );
    }
    if r.owner == OWNER && r.name == "createIntrusive" {
        if r.descriptor
            != "(Lnet/minecraft/core/HolderOwner;Ljava/lang/Object;)Lnet/minecraft/core/Holder$Reference;"
        {
            return Some(host.unknown_host_call(r));
        }
        let (Some(owner), Some(value)) = (args.first(), args.get(1)) else {
            return Some(host.unknown_host_call(r));
        };
        if !owner_supported(owner) {
            return Some(host.unknown_host_call(r));
        }
        return Some(Ok(intrusive_holder(owner.clone(), value.clone())));
    }
    if r.owner == HOLDER && r.name == "direct" {
        let valid = matches!(
            r.descriptor.as_str(),
            "(Ljava/lang/Object;)Lnet/minecraft/core/Holder;"
                | "(Ljava/lang/Object;Lnet/minecraft/core/component/DataComponentMap;)Lnet/minecraft/core/Holder;"
        );
        if !valid {
            return Some(host.unknown_host_call(r));
        }
        let value = args.first().cloned().unwrap_or(Value::Void);
        let components = args.get(1).cloned().unwrap_or(Value::Ref(None));
        return Some(Ok(direct_holder(value, components)));
    }
    if r.owner != "net/minecraft/core/Registry" || r.name != "registerForHolder" {
        return None;
    }
    let [registry, id_or_key, value] = args else {
        return Some(host.unknown_host_call(r));
    };
    // MappedRegistry.register binds both the key and value before returning;
    // the returned reference is therefore bound in every loader.
    let answer = match r.descriptor.as_str() {
        "(Lnet/minecraft/core/Registry;Lnet/minecraft/resources/Identifier;Ljava/lang/Object;)Lnet/minecraft/core/Holder$Reference;" =>
        {
            let holder = make_for_identifier(registry, id_or_key, value, true);
            if holder.is_some()
                && let (Some(registry), Some(id)) = (
                    crate::runtime::host_registry::registry_id(registry),
                    host_ids::identifier_key(id_or_key),
                )
            {
                host.record_observed_registry(registry, id.to_owned(), value.clone());
            }
            holder
        }
        "(Lnet/minecraft/core/Registry;Lnet/minecraft/resources/ResourceKey;Ljava/lang/Object;)Lnet/minecraft/core/Holder$Reference;" =>
        {
            let holder = make_for_key(registry, id_or_key, value, true);
            if holder.is_some()
                && let (Some(registry), Some((_, id))) = (
                    crate::runtime::host_registry::registry_id(registry),
                    host_ids::resource_key_parts(id_or_key),
                )
            {
                host.record_observed_registry(registry, id, value.clone());
            }
            holder
        }
        _ => None,
    };
    Some(answer.map_or_else(|| host.unknown_host_call(r), Ok))
}

fn observed_holder(host: &StdHost, registry: &Value, id: &str) -> Option<Value> {
    let registry_id = crate::runtime::host_registry::registry_id(registry)?;
    let entry = host.observed_registry_entry(registry_id, id)?;
    Some(holder(
        host_ids::registry_key_of(registry_id),
        host_ids::identifier_of_text(id),
        entry.value.clone(),
        true,
        false,
    ))
}

fn id_argument(registry: &str, value: &Value) -> Option<String> {
    if let Some(id) = host_ids::identifier_key(value) {
        return Some(id.to_owned());
    }
    let (key_registry, id) = host_ids::resource_key_parts(value)?;
    (key_registry == registry).then_some(id)
}

fn vanilla_value(registry: &str, id: &str) -> Option<Value> {
    match registry {
        "minecraft:block" => crate::runtime::vanilla_blocks::handle(id),
        "minecraft:item" => crate::runtime::vanilla_items::handle(id),
        _ => None,
    }
}

fn vanilla_value_id<'a>(registry: &str, value: &'a Value) -> Option<&'a str> {
    let marker = crate::runtime::game_object::handle_key(value)?;
    let prefix = match registry {
        "minecraft:block" => "game/block/",
        "minecraft:item" => "game/item/",
        _ => return None,
    };
    let id = marker.strip_prefix(prefix)?;
    vanilla_value(registry, id).as_ref()?;
    Some(id)
}

fn registry_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let registry = crate::runtime::host_registry::registry_id(receiver)?.to_owned();
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("get", "(Lnet/minecraft/resources/Identifier;)Ljava/util/Optional;")
        | ("getOptional", "(Lnet/minecraft/resources/Identifier;)Ljava/util/Optional;")
        | ("get", "(Lnet/minecraft/resources/ResourceKey;)Ljava/util/Optional;") => {
            let Some(id) = args.first().and_then(|value| id_argument(&registry, value)) else {
                return Some(host.unknown_host_call(r));
            };
            if matches!(registry.as_str(), "minecraft:block" | "minecraft:item") {
                if let Some(value) = vanilla_value(&registry, &id) {
                    return Some(Ok(crate::runtime::host_optional::optional_wrap(value)));
                }
                if let Some(holder) = observed_holder(host, receiver, &id) {
                    return Some(Ok(crate::runtime::host_optional::optional_wrap(holder)));
                }
                return Some(host.unknown_host_call(r));
            }
            if registry == "minecraft:entity_type" {
                if let Some(value) = crate::runtime::vanilla_entity_types::handle(&id) {
                    return Some(Ok(crate::runtime::host_optional::optional_wrap(value)));
                }
                if id.starts_with("minecraft:") {
                    return Some(Ok(crate::runtime::host_optional::empty_optional()));
                }
            }
            let Some(holder) = observed_holder(host, receiver, &id) else {
                return Some(host.unknown_host_call(r));
            };
            Some(Ok(crate::runtime::host_optional::optional_wrap(holder)))
        }
        ("wrapAsHolder", "(Ljava/lang/Object;)Lnet/minecraft/core/Holder;") => {
            let Some(value) = args.first() else {
                return Some(host.unknown_host_call(r));
            };
            if let Some(entry) = host
                .registrations()
                .iter()
                .rev()
                .find(|entry| entry.registry == registry && entry.value == *value)
            {
                return Some(Ok(holder(
                    host_ids::registry_key_of(&registry),
                    host_ids::identifier_of_text(&entry.id),
                    entry.value.clone(),
                    true,
                    false,
                )));
            }
            Some(Ok(direct_holder(value.clone(), Value::Ref(None))))
        }
        ("getValue", "(Lnet/minecraft/resources/Identifier;)Ljava/lang/Object;")
        | ("getValue", "(Lnet/minecraft/resources/ResourceKey;)Ljava/lang/Object;")
        | ("getValueOrThrow", "(Lnet/minecraft/resources/ResourceKey;)Ljava/lang/Object;") => {
            let Some(id) = args.first().and_then(|value| id_argument(&registry, value)) else {
                return Some(host.unknown_host_call(r));
            };
            if let Some(entry) = host.observed_registry_entry(&registry, &id) {
                return Some(Ok(entry.value.clone()));
            }
            if matches!(registry.as_str(), "minecraft:block" | "minecraft:item") {
                if let Some(value) = vanilla_value(&registry, &id) {
                    return Some(Ok(value));
                }
                return Some(host.unknown_host_call(r));
            }
            if registry == "minecraft:entity_type" {
                if let Some(value) = crate::runtime::vanilla_entity_types::handle(&id) {
                    return Some(Ok(value));
                }
                if id.starts_with("minecraft:") {
                    return Some(Ok(crate::runtime::vanilla_entity_types::handle("minecraft:pig")
                        .unwrap_or(Value::Ref(None))));
                }
            }
            Some(host.unknown_host_call(r))
        }
        ("getResourceKey", "(Ljava/lang/Object;)Ljava/util/Optional;") => {
            let Some(value) = args.first() else {
                return Some(host.unknown_host_call(r));
            };
            let Some(entry) = host
                .registrations()
                .iter()
                .rev()
                .find(|entry| entry.registry == registry && entry.value == *value)
            else {
                return Some(host.unknown_host_call(r));
            };
            Some(Ok(crate::runtime::host_optional::optional_wrap(
                host_ids::resource_key(
                    &host_ids::registry_key_of(&registry),
                    &host_ids::identifier_of_text(&entry.id),
                    Some(entry.id.clone()),
                ),
            )))
        }
        ("getKey", "(Ljava/lang/Object;)Lnet/minecraft/resources/Identifier;") => {
            let Some(value) = args.first() else {
                return Some(host.unknown_host_call(r));
            };
            if let Some(id) = vanilla_value_id(&registry, value) {
                return Some(Ok(host_ids::identifier_of_text(id)));
            }
            if registry == "minecraft:block_entity_type"
                && let Some(id) = crate::runtime::game_object::text(value)
                && let Some(id) = id.strip_prefix("client/block-entity-type/")
            {
                return Some(Ok(host_ids::identifier_of_text(id)));
            }
            let Some(entry) = host
                .registrations()
                .iter()
                .rev()
                .find(|entry| entry.registry == registry && entry.value == *value)
            else {
                return Some(host.unknown_host_call(r));
            };
            Some(Ok(host_ids::identifier_of_text(&entry.id)))
        }
        ("containsKey", "(Lnet/minecraft/resources/Identifier;)Z")
        | ("containsKey", "(Lnet/minecraft/resources/ResourceKey;)Z") => {
            let Some(id) = args.first().and_then(|value| id_argument(&registry, value)) else {
                return Some(host.unknown_host_call(r));
            };
            if host.observed_registry_entry(&registry, &id).is_some() {
                return Some(Ok(Value::Int(1)));
            }
            if matches!(registry.as_str(), "minecraft:block" | "minecraft:item") {
                if vanilla_value(&registry, &id).is_some() {
                    return Some(Ok(Value::Int(1)));
                }
                return Some(host.unknown_host_call(r));
            }
            if registry == "minecraft:entity_type" {
                return Some(Ok(Value::Int(i32::from(
                    crate::runtime::vanilla_entity_types::handle(&id).is_some(),
                ))));
            }
            Some(Ok(Value::Int(0)))
        }
        _ => None,
    }
}

fn is_id(parts: &Parts, id: &Value) -> Option<bool> {
    Some(host_ids::identifier_key(id)? == parts.id)
}

fn is_key(parts: &Parts, key: &Value) -> Option<bool> {
    Some(entry_key(parts) == *key)
}

fn holder_to_string(
    handle: &HostRef,
    nested: &mut dyn Nested,
) -> Result<Option<String>, NativeError> {
    let value = Value::Host(handle.clone());
    if let Some((value, _owner)) = intrusive_value(&value) {
        let text = nested.text(&value)?;
        return Ok(text.map(|text| format!("Reference{{null={text}}}")));
    }
    let parts =
        parts(&value).ok_or_else(|| NativeError::Metadata("malformed registry holder".into()))?;
    let key = nested.text(&entry_key(&parts))?;
    let value = if parts.bound {
        nested.text(&parts.value)?
    } else {
        Some("null".into())
    };
    Ok(key
        .zip(value)
        .map(|(key, value)| format!("Reference{{{key}={value}}}")))
}

fn unbound_value(host: &mut StdHost, parts: &Parts) -> Result<Option<NativeError>, NativeError> {
    let Some(registry) = host_ids::registry_name(&parts.registry_key) else {
        return Ok(None);
    };
    let Some(owner) = exact_text(host, &parts.owner)? else {
        return Ok(None);
    };
    let key = format!("ResourceKey[{registry} / {}]", parts.id);
    Ok(Some(NativeError::UncaughtException {
        class: "java/lang/IllegalStateException".into(),
        message: Some(format!(
            "Trying to access unbound value '{key}' from registry {owner}"
        )),
        frame: None,
    }))
}

const DIRECT_OWNER: &str = "net/minecraft/core/Holder$Direct";

fn direct_holder(value: Value, components: Value) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::DirectHolder,
        &[value, components],
    ))
}

fn direct_parts(value: &Value) -> Option<(Value, Value)> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::DirectHolder {
        return None;
    }
    <[Value; 2]>::try_from(handle.values()?)
        .ok()
        .map(|[value, components]| (value, components))
}

fn direct_identity_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) if other.kind() == HostKind::DirectHolder => handle.same_object(other),
        _ => Some(false),
    })
}

fn direct_instance_of(
    _handle: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if wanted == "java/lang/Object"
        || wanted == HOLDER
        || hierarchy.is_instance(DIRECT_OWNER, wanted) == Some(true)
    {
        return Some(true);
    }
    if wanted == OWNER || wanted == DIRECT_OWNER {
        return Some(wanted == DIRECT_OWNER);
    }
    (hierarchy.is_interface(wanted) == Some(false) || exact_class_rules_out(DIRECT_OWNER, wanted))
        .then_some(false)
}

pub(crate) const DIRECT_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: direct_identity_equals,
    instance_of: direct_instance_of,
    crosses_jars: |_, _| false,
    compare_to: None,
};

fn direct_holder_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let (value, _components) = direct_parts(receiver)?;
    let answer = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (HOLDER | DIRECT_OWNER, "value", "()Ljava/lang/Object;") => Ok(value),
        (HOLDER | DIRECT_OWNER, "isBound", "()Z")
        | (HOLDER | DIRECT_OWNER, "areComponentsBound", "()Z") => Ok(Value::Int(1)),
        (HOLDER | DIRECT_OWNER, "is", "(Lnet/minecraft/resources/Identifier;)Z")
        | (HOLDER | DIRECT_OWNER, "is", "(Lnet/minecraft/resources/ResourceKey;)Z")
        | (HOLDER | DIRECT_OWNER, "is", "(Lnet/minecraft/tags/TagKey;)Z") => Ok(Value::Int(0)),
        (HOLDER | DIRECT_OWNER, "is", "(Lnet/minecraft/core/Holder;)Z") => {
            let Some(Value::Host(other)) = args.first() else {
                return Some(host.unknown_host_call(r));
            };
            if other.kind() == HostKind::DirectHolder
                && matches!(receiver, Value::Host(handle) if handle.same_object(other).is_some_and(|same| same))
            {
                Ok(Value::Int(1))
            } else {
                return Some(host.unknown_host_call(r));
            }
        }
        (HOLDER | DIRECT_OWNER, "unwrapKey", "()Ljava/util/Optional;")
        | (HOLDER | DIRECT_OWNER, "getRegisteredNameIfPresent", "()Ljava/util/Optional;") => {
            Ok(crate::runtime::host_optional::empty_optional())
        }
        (HOLDER | DIRECT_OWNER, "getRegisteredName", "()Ljava/lang/String;") => {
            Ok(Value::Ref(Some("[unregistered]".into())))
        }
        (HOLDER | DIRECT_OWNER, "unwrap", "()Lcom/mojang/datafixers/util/Either;") => {
            Ok(dfu_either::right(&value))
        }
        (HOLDER | DIRECT_OWNER, "kind", "()Lnet/minecraft/core/Holder$Kind;") => {
            Ok(Value::Ref(Some(format!("enum/{KIND}/DIRECT"))))
        }
        _ => return Some(host.unknown_host_call(r)),
    };
    Some(answer)
}

fn identity_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) if other.kind() == HostKind::RegistryHolder => handle.same_object(other),
        _ => Some(false),
    })
}

fn instance_of(_handle: &HostRef, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
    if HOLDER_TYPES.contains(&wanted) || hierarchy.is_instance(OWNER, wanted) == Some(true) {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false) || exact_class_rules_out(OWNER, wanted))
        .then_some(false)
}

pub(crate) const METHODS: Methods = Methods {
    to_string: holder_to_string,
    // `Holder.Reference` inherits `Object.hashCode`: an identity hash.
    hash_code: |_, _| Ok(None),
    equals: identity_equals,
    instance_of,
    crosses_jars: |_, _| false,
    compare_to: None,
};

pub(crate) fn instance_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if direct_parts(receiver).is_some() {
        return direct_holder_call(host, r, receiver, args);
    }
    if let Some((value, owner)) = intrusive_value(receiver) {
        let error_method = matches!(
            (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()),
            (
                HOLDER | OWNER,
                "key",
                "()Lnet/minecraft/resources/ResourceKey;"
            ) | (HOLDER | OWNER, "getRegisteredName", "()Ljava/lang/String;")
                | (
                    HOLDER | OWNER,
                    "getRegisteredNameIfPresent",
                    "()Ljava/util/Optional;"
                )
                | (HOLDER | OWNER, "unwrapKey", "()Ljava/util/Optional;")
                | (
                    HOLDER | OWNER,
                    "unwrap",
                    "()Lcom/mojang/datafixers/util/Either;"
                )
                | (
                    HOLDER | OWNER,
                    "is",
                    "(Lnet/minecraft/resources/Identifier;)Z"
                )
                | (
                    HOLDER | OWNER,
                    "is",
                    "(Lnet/minecraft/resources/ResourceKey;)Z"
                )
        );
        if error_method {
            return match intrusive_unbound_value(host, &value, &owner) {
                Ok(Some(error)) => Some(Err(error)),
                Ok(None) => Some(host.unknown_host_call(r)),
                Err(error) => Some(Err(error)),
            };
        }
        let answer = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (HOLDER | OWNER, "value", "()Ljava/lang/Object;") => Ok(value),
            (HOLDER | OWNER, "isBound", "()Z") | (HOLDER | OWNER, "areComponentsBound", "()Z") => {
                Ok(Value::Int(0))
            }
            (HOLDER | OWNER, "kind", "()Lnet/minecraft/core/Holder$Kind;") => {
                Ok(Value::Ref(Some(format!("enum/{KIND}/REFERENCE"))))
            }
            _ => return Some(host.unknown_host_call(r)),
        };
        return Some(answer);
    }
    if crate::runtime::host_registry::registry_id(receiver).is_some() {
        return registry_call(host, r, receiver, args);
    }
    let parts = parts(receiver)?;
    let answer = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (HOLDER | OWNER, "key", "()Lnet/minecraft/resources/ResourceKey;") => Ok(entry_key(&parts)),
        (HOLDER | OWNER, "value", "()Ljava/lang/Object;") => {
            // Vanilla constants are game-owned and already bound during
            // bootstrap; jar registrations use the same bound-state rule
            // after their registry lifecycle binds them.
            if parts.game_owned || parts.bound {
                Ok(parts.value.clone())
            } else {
                match unbound_value(host, &parts) {
                    Ok(Some(error)) => Err(error),
                    Ok(None) => return Some(host.unknown_host_call(r)),
                    Err(error) => return Some(Err(error)),
                }
            }
        }
        (HOLDER | OWNER, "getRegisteredName", "()Ljava/lang/String;") => {
            Ok(Value::Ref(Some(parts.id.clone())))
        }
        (HOLDER | OWNER, "getRegisteredNameIfPresent", "()Ljava/util/Optional;") => Ok(
            crate::runtime::host_optional::optional_wrap(Value::Ref(Some(parts.id.clone()))),
        ),
        (HOLDER | OWNER, "unwrapKey", "()Ljava/util/Optional;") => {
            Ok(crate::runtime::host_optional::optional_wrap(entry_key(&parts)))
        }
        (HOLDER | OWNER, "unwrap", "()Lcom/mojang/datafixers/util/Either;") => {
            Ok(dfu_either::left(&entry_key(&parts)))
        }
        (HOLDER | OWNER, "kind", "()Lnet/minecraft/core/Holder$Kind;") => {
            Ok(Value::Ref(Some(format!("enum/{KIND}/REFERENCE"))))
        }
        (HOLDER | OWNER, "isBound", "()Z") => Ok(Value::Int(i32::from(parts.bound))),
        (HOLDER | OWNER, "areComponentsBound", "()Z") => Ok(Value::Int(0)),
        (HOLDER | OWNER, "is", "(Lnet/minecraft/resources/Identifier;)Z") => {
            let Some(id) = args.first().and_then(|value| is_id(&parts, value)) else {
                return Some(host.unknown_host_call(r));
            };
            Ok(Value::Int(i32::from(id)))
        }
        (HOLDER | OWNER, "is", "(Lnet/minecraft/resources/ResourceKey;)Z") => {
            let Some(key) = args.first().and_then(|value| is_key(&parts, value)) else {
                return Some(host.unknown_host_call(r));
            };
            Ok(Value::Int(i32::from(key)))
        }
        // Tags, predicates, holder comparisons and component state depend on
        // game-owned binding, so they remain counted by design.
        _ => return Some(host.unknown_host_call(r)),
    };
    Some(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Host, MethodRef};

    fn method(name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: "net/minecraft/core/Registry".into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    #[test]
    fn register_for_holder_keeps_registry_id_value_and_forgery_separate() {
        let mut host = StdHost::new();
        let registry = crate::runtime::host_registry::registry("minecraft:item");
        let id = crate::runtime::host_ids::identifier("demo", "item");
        let holder = host
            .invoke(
                &method(
                    "registerForHolder",
                    "(Lnet/minecraft/core/Registry;Lnet/minecraft/resources/Identifier;Ljava/lang/Object;)Lnet/minecraft/core/Holder$Reference;",
                ),
                &[registry, id, Value::Ref(Some("value".into()))],
            )
            .unwrap();
        assert!(
            matches!(&holder, Value::Host(handle) if handle.kind() == HostKind::RegistryHolder)
        );
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "getRegisteredName".into(),
                    descriptor: "()Ljava/lang/String;".into(),
                },
                &holder,
                &[],
            )
            .unwrap(),
            Value::Ref(Some("demo:item".into()))
        );
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "value".into(),
                    descriptor: "()Ljava/lang/Object;".into(),
                },
                &holder,
                &[],
            )
            .unwrap(),
            Value::Ref(Some("value".into()))
        );
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "isBound".into(),
                    descriptor: "()Z".into(),
                },
                &holder,
                &[],
            )
            .unwrap(),
            Value::Int(1)
        );
        assert!(matches!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "is".into(),
                    descriptor: "(Lnet/minecraft/resources/Identifier;)Z".into(),
                },
                &holder,
                &[crate::runtime::host_ids::identifier("demo", "item")],
            )
            .unwrap(),
            Value::Int(1)
        ));
    }

    #[test]
    fn every_registry_register_for_holder_is_bound_at_return() {
        let mut host = StdHost::new()
            .with_run_loader(crate::runtime::class_precedence::RunLoader::Fabric)
            .with_mod("fabric-registry-sync-v0", "8.0.4");
        let holder = host
            .invoke(
                &method(
                    "registerForHolder",
                    "(Lnet/minecraft/core/Registry;Lnet/minecraft/resources/Identifier;Ljava/lang/Object;)Lnet/minecraft/core/Holder$Reference;",
                ),
                &[
                    crate::runtime::host_registry::registry("minecraft:item"),
                    crate::runtime::host_ids::identifier("demo", "item"),
                    Value::Ref(Some("value".into())),
                ],
            )
            .unwrap();
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "value".into(),
                    descriptor: "()Ljava/lang/Object;".into(),
                },
                &holder,
                &[],
            )
            .unwrap(),
            Value::Ref(Some("value".into()))
        );
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "isBound".into(),
                    descriptor: "()Z".into(),
                },
                &holder,
                &[],
            )
            .unwrap(),
            Value::Int(1)
        );
    }

    #[test]
    fn registry_register_for_holder_is_bound_without_fabric_sync() {
        let mut host = StdHost::new().with_run_loader(crate::runtime::class_precedence::RunLoader::Fabric);
        let holder = host
            .invoke(
                &method(
                    "registerForHolder",
                    "(Lnet/minecraft/core/Registry;Lnet/minecraft/resources/Identifier;Ljava/lang/Object;)Lnet/minecraft/core/Holder$Reference;",
                ),
                &[
                    crate::runtime::host_registry::registry("minecraft:item"),
                    crate::runtime::host_ids::identifier("demo", "item"),
                    Value::Ref(Some("value".into())),
                ],
            )
            .unwrap();
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "isBound".into(),
                    descriptor: "()Z".into(),
                },
                &holder,
                &[],
            )
            .unwrap(),
            Value::Int(1)
        );
    }

    #[test]
    fn direct_holder_answers_unregistered_facts_without_an_invented_id() {
        let mut host = StdHost::new();
        let value = Value::Ref(Some("effect".into()));
        let direct = host
            .invoke(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "direct".into(),
                    descriptor: "(Ljava/lang/Object;)Lnet/minecraft/core/Holder;".into(),
                },
                std::slice::from_ref(&value),
            )
            .unwrap();
        assert!(matches!(
            &direct,
            Value::Host(handle) if handle.kind() == HostKind::DirectHolder
        ));
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "getRegisteredName".into(),
                    descriptor: "()Ljava/lang/String;".into(),
                },
                &direct,
                &[],
            )
            .unwrap(),
            Value::Ref(Some("[unregistered]".into()))
        );
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "value".into(),
                    descriptor: "()Ljava/lang/Object;".into(),
                },
                &direct,
                &[],
            )
            .unwrap(),
            value
        );
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "kind".into(),
                    descriptor: "()Lnet/minecraft/core/Holder$Kind;".into(),
                },
                &direct,
                &[],
            )
            .unwrap(),
            Value::Ref(Some("enum/net/minecraft/core/Holder$Kind/DIRECT".into()))
        );
        assert!(host.unknown_host_calls().is_empty());
    }

    #[test]
    fn a_bound_reference_answers_value_and_prints_it() {
        let mut host = StdHost::new();
        let bound = holder(
            crate::runtime::host_ids::registry_key_of("minecraft:item"),
            crate::runtime::host_ids::identifier("demo", "item"),
            Value::Ref(Some("value".into())),
            true,
            false,
        );
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "value".into(),
                    descriptor: "()Ljava/lang/Object;".into(),
                },
                &bound,
                &[],
            )
            .unwrap(),
            Value::Ref(Some("value".into()))
        );
        assert_eq!(
            host.invoke_instance(
                &MethodRef {
                    owner: HOLDER.into(),
                    name: "isBound".into(),
                    descriptor: "()Z".into(),
                },
                &bound,
                &[],
            )
            .unwrap(),
            Value::Int(1)
        );
    }
}
