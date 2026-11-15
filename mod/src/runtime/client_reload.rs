//! Registration-only models for Fabric resource-loader v0/v1 and NeoForge's
//! client reload listener event.  The listener callbacks themselves never run.

use crate::runtime::host_ref::{HostKind, HostRef, Methods, Nested, instance_of_class};
use crate::runtime::{Host, MethodRef, NativeError, Runtime, StdHost, Value};
use std::collections::BTreeMap;

const HELPER_API: &str = "net/fabricmc/fabric/api/resource/ResourceManagerHelper";
const CONVERTER: &str = "net/minecraft/resources/FileToIdConverter";
const PREFIX: &str = "\u{0}client-reload/";

fn identity_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) if other.kind() == handle.kind() => handle.same_object(other),
        _ => Some(false),
    })
}
fn converter_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) if other.kind() == HostKind::FileToIdConverter => {
            Some(handle.values() == other.values())
        }
        _ => Some(false),
    })
}

pub(crate) const HELPER_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: identity_equals,
    instance_of: instance_of_class,
    crosses_jars: |_, _| false,
    compare_to: None,
};
pub(crate) const CONVERTER_METHODS: Methods = Methods {
    to_string: |handle, _| {
        let Some((prefix, extension)) = parts(handle) else {
            return Ok(None);
        };
        Ok(Some(format!(
            "FileToIdConverter[prefix={prefix}, extension={extension}]"
        )))
    },
    hash_code: |handle, _| {
        Ok(parts(handle)
            .map(|(prefix, extension)| {
                Some(
                    crate::runtime::jdk_model::java_string_hash(&prefix)
                        .wrapping_mul(31)
                        .wrapping_add(crate::runtime::jdk_model::java_string_hash(&extension)),
                )
            })
            .unwrap_or(None))
    },
    equals: converter_equals,
    instance_of: instance_of_class,
    crosses_jars: |_, _| true,
    compare_to: None,
};

pub(crate) fn helper_types(_: &str) -> &'static [&'static str] {
    &[HELPER_API]
}
pub(crate) fn converter_class(_: &str) -> &str {
    CONVERTER
}

fn parts(handle: &HostRef) -> Option<(String, String)> {
    let values = handle.values()?;
    match values.as_slice() {
        [Value::Ref(Some(prefix)), Value::Ref(Some(extension))] => {
            Some((prefix.clone(), extension.clone()))
        }
        _ => None,
    }
}
fn pack_name(value: &Value) -> Option<&str> {
    let Value::Ref(Some(text)) = value else {
        return None;
    };
    text.strip_prefix("enum/net/minecraft/server/packs/PackType/")
}
fn helper(pack: &Value) -> Option<Value> {
    Some(Value::Host(HostRef::of_values(
        HostKind::FabricResourceManagerHelper,
        &[Value::Ref(Some(pack_name(pack)?.to_owned()))],
    )))
}
fn converter(prefix: &str) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::FileToIdConverter,
        &[
            Value::Ref(Some(prefix.to_owned())),
            Value::Ref(Some(".json".into())),
        ],
    ))
}
fn id_text(value: &Value) -> Option<&str> {
    crate::runtime::host_ids::identifier_key(value)
}
fn registration_id(value: &Value) -> Option<String> {
    id_text(value)
        .map(str::to_owned)
        .or_else(|| crate::runtime::jar_object::object_key(value).map(|key| format!("object/{key}")))
}
fn state_key(resource: &str, id: &str) -> String {
    format!("{PREFIX}{resource}/{id}")
}
fn duplicate(class: &str, message: String) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: Some(message),
        frame: None,
    }
}

fn null_pointer(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn listener_identity(value: &Value) -> Option<String> {
    match value {
        Value::Host(handle) => Some(format!("{}:{}", handle.kind() as usize, handle.key())),
        _ => None,
    }
}

fn listener_key(resource: &str, listener: &Value) -> Option<String> {
    Some(format!(
        "{PREFIX}{resource}\0listener\0{}",
        listener_identity(listener)?
    ))
}

fn order_key(resource: &str) -> String {
    format!("{PREFIX}{resource}\0order")
}

fn append_order(fields: &mut BTreeMap<String, Value>, resource: &str, id: &str) {
    let key = order_key(resource);
    let mut order = match fields.get(&key) {
        Some(Value::Ref(Some(order))) => order.clone(),
        _ => String::new(),
    };
    if !order.is_empty() {
        order.push('\0');
    }
    order.push_str(id);
    fields.insert(key, Value::Ref(Some(order)));
}

fn register_in(
    fields: &mut BTreeMap<String, Value>,
    resource: &str,
    id: &str,
    listener: &Value,
    duplicate_class: &str,
    duplicate_id_message: String,
    duplicate_listener_message: String,
) -> Result<(), NativeError> {
    let key = state_key(resource, id);
    if fields.contains_key(&key) {
        return Err(duplicate(duplicate_class, duplicate_id_message));
    }
    if let Some(listener_key) = listener_key(resource, listener)
        && let Some(Value::Ref(Some(old_id))) = fields.get(&listener_key)
    {
        return Err(duplicate(
            duplicate_class,
            duplicate_listener_message.replace("{old}", old_id),
        ));
    }
    fields.insert(key, Value::Ref(Some("registered".into())));
    if let Some(listener_key) = listener_key(resource, listener) {
        fields.insert(listener_key, Value::Ref(Some(id.into())));
    }
    append_order(fields, resource, id);
    Ok(())
}

fn add_order(
    fields: &mut BTreeMap<String, Value>,
    resource: &str,
    first: &str,
    second: &str,
) -> Result<(), NativeError> {
    if first == second {
        return Err(duplicate(
            "java/lang/IllegalArgumentException",
            "Tried to add a phase that depends on itself.".into(),
        ));
    }
    fields.insert(
        format!("{PREFIX}{resource}\0edge\0{first}\0{second}"),
        Value::Ref(Some("ordered".into())),
    );
    Ok(())
}

fn id_argument(value: Option<&Value>, message: &str) -> Result<Option<String>, NativeError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if matches!(value, Value::Ref(None)) {
        return Err(null_pointer(message));
    }
    Ok(registration_id(value))
}

fn collection_items<H: Host>(runtime: &Runtime<'_, H>, value: &Value) -> Option<Vec<Value>> {
    runtime.host.mutable_collection_values(value).or_else(|| {
        matches!(value, Value::Ref(Some(_)))
            .then(|| crate::runtime::collection_values(value).ok())
            .flatten()
    })
}

fn invoke_dependency_method<H: Host>(
    runtime: &mut Runtime<'_, H>,
    class: &str,
    listener: Value,
) -> Result<Value, NativeError> {
    match runtime.invoke(
        class,
        "getFabricDependencies",
        "()Ljava/util/Collection;",
        Some(listener.clone()),
        &[],
    ) {
        Err(NativeError::MissingMethod(_)) => Ok(Value::Ref(Some("list/".into()))),
        result => result,
    }
}

pub(crate) fn static_field(owner: &str, name: &str, descriptor: &str) -> Option<Value> {
    if matches!(
        owner,
        "net/fabricmc/fabric/api/resource/v1/pack/PackActivationType"
            | "net/fabricmc/fabric/api/resource/ResourcePackActivationType"
    ) && matches!(name, "NORMAL" | "DEFAULT_ENABLED" | "ALWAYS_ENABLED")
        && descriptor == format!("L{owner};")
    {
        return Some(Value::Ref(Some(format!("enum/{owner}/{name}"))));
    }
    let text = match owner {
        "net/fabricmc/fabric/api/resource/v1/reloader/ResourceReloaderKeys" => match name {
            "BEFORE_VANILLA" => "fabric:before_vanilla",
            "AFTER_VANILLA" => "fabric:after_vanilla",
            _ => return None,
        },
        "net/fabricmc/fabric/api/resource/v1/reloader/ResourceReloaderKeys$Client" => match name {
            "BLOCK_ENTITY_RENDER_DISPATCHER" => "minecraft:block_entity_render_dispatcher",
            "CLOUD_RENDERER" => "minecraft:cloud_renderer",
            "EQUIPMENT_ASSETS" => "minecraft:equipment_assets",
            "ENTITY_RENDER_DISPATCHER" => "minecraft:entity_render_dispatcher",
            "DRY_FOLIAGE_COLOR" => "minecraft:dry_foliage_color",
            "FOLIAGE_COLOR" => "minecraft:foliage_color",
            "FONTS" => "minecraft:fonts",
            "GAME_RENDERER" => "minecraft:game_renderer",
            "GRASS_COLOR" => "minecraft:grass_color",
            "ATLAS" => "minecraft:atlas",
            "LANGUAGES" => "minecraft:languages",
            "MODELS" => "minecraft:models",
            "PARTICLES" => "minecraft:particles",
            "SHADERS" => "minecraft:shaders",
            "SOUNDS" => "minecraft:sounds",
            "SPLASH_TEXTS" => "minecraft:splash_texts",
            "TEXTURES" => "minecraft:textures",
            "WAYPOINT_STYLE" => "minecraft:waypoint_style",
            _ => return None,
        },
        _ => return None,
    };
    (descriptor == "Lnet/minecraft/resources/Identifier;")
        .then(|| crate::runtime::host_ids::identifier_of_text(text))
}

pub(crate) fn static_call(
    _host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (
            "net/fabricmc/fabric/api/resource/ResourceManagerHelper",
            "get",
            "(Lnet/minecraft/server/packs/PackType;)Lnet/fabricmc/fabric/api/resource/ResourceManagerHelper;",
        ) => Some(
            helper(args.first()?)
                .map(Ok)
                .unwrap_or_else(|| Err(NativeError::UnknownHostCall(r.clone()))),
        ),
        (
            "net/fabricmc/fabric/api/resource/v1/DataResourceLoader",
            "get",
            "()Lnet/fabricmc/fabric/api/resource/v1/DataResourceLoader;",
        ) => Some(Ok(Value::Host(HostRef::of_values(
            HostKind::FabricResourceLoader,
            &[Value::Ref(Some("DATA_RESOURCE".into()))],
        )))),
        (
            "net/minecraft/resources/FileToIdConverter",
            "json",
            "(Ljava/lang/String;)Lnet/minecraft/resources/FileToIdConverter;",
        ) => Some(Ok(converter(crate::runtime::expect_string(args.first()?)))),
        _ => None,
    }
}

fn substring_error(begin: i64, end: i64, length: i64) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/StringIndexOutOfBoundsException".into(),
        message: Some(format!(
            "Range [{begin}, {end}) out of bounds for length {length}"
        )),
        frame: None,
    }
}

fn id_transform(handle: &HostRef, id: &Value, to_file: bool) -> Result<Value, NativeError> {
    let Some((prefix, extension)) = parts(handle) else {
        return Err(NativeError::UnknownHostCall(MethodRef {
            owner: CONVERTER.into(),
            name: "fileToId".into(),
            descriptor:
                "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/resources/Identifier;".into(),
        }));
    };
    let Some(id) = id_text(id) else {
        return Err(null_pointer(
            "Cannot invoke \"net.minecraft.resources.Identifier.getPath()\" because \"id\" is null",
        ));
    };
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    if to_file {
        return Ok(crate::runtime::host_ids::identifier(
            namespace,
            &format!("{prefix}/{path}{extension}"),
        ));
    }
    // FileToIdConverter.fileToId deliberately performs two unchecked
    // String.substring calls.  It does not first test prefix/extension.
    let begin = prefix.len() as i64 + 1;
    let end = path.len() as i64 - extension.len() as i64;
    if begin < 0 || end < begin || end > path.len() as i64 {
        return Err(substring_error(begin, end, path.len() as i64));
    }
    Ok(crate::runtime::host_ids::identifier(
        namespace,
        &path[begin as usize..end as usize],
    ))
}

#[allow(clippy::question_mark)]
pub(crate) fn host_instance_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if crate::runtime::host_game_event::fired_event(receiver)
        == Some(crate::runtime::events::GameEvent::ClientReloadListeners)
    {
        if r.name == "addListener" {
            let Some(id) =
                (match id_argument(args.first(), "The listener identifier should not be null.") {
                    Ok(id) => id,
                    Err(error) => return Some(Err(error)),
                })
            else {
                return None;
            };
            let Some(listener) = args.get(1) else {
                return None;
            };
            if matches!(listener, Value::Ref(None)) {
                return Some(Err(null_pointer("The listener should not be null.")));
            }
            return Some(
                register_in(
                    &mut host.persistent_static_fields,
                    "neoforge-client",
                    &id,
                    listener,
                    "java/lang/IllegalArgumentException",
                    format!("Attempted to register two reload listeners for the same key: {id}"),
                    format!("Attempted to register two reload listeners for the same key: {id}"),
                )
                .map(|_| Value::Void),
            );
        }
        if r.name == "addDependency" {
            let Some(first) = (match id_argument(
                args.first(),
                "The first listener identifier should not be null.",
            ) {
                Ok(id) => id,
                Err(error) => return Some(Err(error)),
            }) else {
                return None;
            };
            let Some(second) = (match id_argument(
                args.get(1),
                "The second listener identifier should not be null.",
            ) {
                Ok(id) => id,
                Err(error) => return Some(Err(error)),
            }) else {
                return None;
            };
            for id in [&first, &second] {
                if !host
                    .persistent_static_fields
                    .contains_key(&state_key("neoforge-client", id))
                {
                    return Some(Err(duplicate(
                        "java/lang/IllegalArgumentException",
                        format!("Unknown reload listener: {id}"),
                    )));
                }
            }
            return Some(
                add_order(
                    &mut host.persistent_static_fields,
                    "neoforge-client",
                    &first,
                    &second,
                )
                .map(|_| Value::Void),
            );
        }
    }
    let Value::Host(handle) = receiver else {
        return None;
    };
    if handle.kind() != HostKind::FileToIdConverter {
        let _ = host;
        return None;
    }
    let missing = || NativeError::UnknownHostCall(r.clone());
    Some(match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "idToFile",
            "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/resources/Identifier;",
        ) => id_transform(handle, args.first()?, true),
        (
            "fileToId",
            "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/resources/Identifier;",
        ) => id_transform(handle, args.first()?, false),
        ("extensionMatches", "(Lnet/minecraft/resources/Identifier;)Z") => {
            let Some(id) = id_text(args.first()?) else {
                if matches!(args.first(), Some(Value::Ref(None))) {
                    return Some(Err(null_pointer(
                        "Cannot invoke \"net.minecraft.resources.Identifier.getPath()\" because \"id\" is null",
                    )));
                }
                return Some(Err(missing()));
            };
            let (_, path) = id.split_once(':').unwrap_or(("minecraft", id));
            let answer = parts(handle).is_some_and(|(_, ext)| path.ends_with(&ext));
            Ok(Value::Int(i32::from(answer)))
        }
        ("prefixMatches", "(Lnet/minecraft/resources/Identifier;)Z") => {
            let Some(id) = id_text(args.first()?) else {
                if matches!(args.first(), Some(Value::Ref(None))) {
                    return Some(Err(null_pointer(
                        "Cannot invoke \"net.minecraft.resources.Identifier.getPath()\" because \"id\" is null",
                    )));
                }
                return Some(Err(missing()));
            };
            let (_, path) = id.split_once(':').unwrap_or(("minecraft", id));
            let answer = parts(handle).is_some_and(|(prefix, _)| path.starts_with(&prefix));
            Ok(Value::Int(i32::from(answer)))
        }
        ("matches", "(Lnet/minecraft/resources/Identifier;)Z") => {
            let Some(id) = id_text(args.first()?) else {
                if matches!(args.first(), Some(Value::Ref(None))) {
                    return Some(Err(null_pointer(
                        "Cannot invoke \"net.minecraft.resources.Identifier.getPath()\" because \"id\" is null",
                    )));
                }
                return Some(Err(missing()));
            };
            let (_, path) = id.split_once(':').unwrap_or(("minecraft", id));
            let answer = parts(handle)
                .is_some_and(|(prefix, ext)| path.starts_with(&prefix) && path.ends_with(&ext));
            Ok(Value::Int(i32::from(answer)))
        }
        ("prefix", "()Ljava/lang/String;") => parts(handle)
            .map(|(prefix, _)| Value::Ref(Some(prefix.to_owned())))
            .ok_or_else(missing),
        ("extension", "()Ljava/lang/String;") => parts(handle)
            .map(|(_, extension)| Value::Ref(Some(extension.to_owned())))
            .ok_or_else(missing),
        _ => return None,
    })
}

fn register(
    runtime: &mut Runtime<'_, impl Host>,
    resource: &str,
    id: &str,
    listener: &Value,
    duplicate_class: &str,
    duplicate_id_message: String,
    duplicate_listener_message: String,
) -> Result<Value, NativeError> {
    register_in(
        &mut runtime.static_fields,
        resource,
        id,
        listener,
        duplicate_class,
        duplicate_id_message,
        duplicate_listener_message,
    )?;
    Ok(Value::Void)
}

#[allow(clippy::question_mark)]
pub(crate) fn runtime_instance_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let kind = match receiver {
        Value::Host(handle) => handle.kind(),
        _ => return None,
    };
    if matches!(
        kind,
        HostKind::FabricResourceManagerHelper | HostKind::FabricResourceLoader
    ) {
        let resource = match receiver {
            Value::Host(handle) => handle.values()?.first()?.clone(),
            _ => return None,
        };
        let resource = crate::runtime::expect_ref(&resource)?.to_owned();
        if matches!(
            r.name.as_str(),
            "registerReloadListener" | "registerReloader" | "addReloadListener"
        ) {
            let has_explicit_id = r
                .descriptor
                .starts_with("(Lnet/minecraft/resources/Identifier;");
            let (id, listener, invokes_dependencies) = if has_explicit_id {
                let Some(id) =
                    (match id_argument(args.first(), "The listener identifier should not be null.")
                    {
                        Ok(id) => id,
                        Err(error) => return Some(Err(error)),
                    })
                else {
                    return None;
                };
                let Some(listener) = args.get(1).cloned() else {
                    return None;
                };
                if matches!(listener, Value::Ref(None)) {
                    return Some(Err(null_pointer("The listener should not be null.")));
                }
                (id, listener, false)
            } else {
                let Some(listener) = args.first().cloned() else {
                    return None;
                };
                if matches!(listener, Value::Ref(None)) {
                    return Some(Err(null_pointer(
                        "Cannot invoke \"net.fabricmc.fabric.api.resource.IdentifiableResourceReloadListener.getFabricId()\" because \"listener\" is null",
                    )));
                }
                let Some(class) = crate::runtime::jar_object::object_class(&listener).map(str::to_owned)
                else {
                    return None;
                };
                let id_value = match runtime.invoke(
                    &class,
                    "getFabricId",
                    "()Lnet/minecraft/resources/Identifier;",
                    Some(listener.clone()),
                    &[],
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let Some(id) = (match id_argument(
                    Some(&id_value),
                    "The listener identifier should not be null.",
                ) {
                    Ok(id) => id,
                    Err(error) => return Some(Err(error)),
                }) else {
                    return None;
                };
                (id, listener, true)
            };
            let duplicate_listener = format!(
                "Resource listener with ID {id} already in resource listener set with ID {{old}}!"
            );
            if let Err(error) = register(
                runtime,
                &resource,
                &id,
                &listener,
                "java/lang/IllegalStateException",
                format!("Tried to register resource listener {id} twice!"),
                duplicate_listener,
            ) {
                return Some(Err(error));
            }
            if invokes_dependencies {
                let Some(class) = crate::runtime::jar_object::object_class(&listener).map(str::to_owned)
                else {
                    return None;
                };
                let deps = match invoke_dependency_method(runtime, &class, listener) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                if matches!(deps, Value::Ref(None)) {
                    return Some(Err(null_pointer(
                        "Cannot invoke \"java.util.Collection.forEach(java.util.function.Consumer)\" because the return value of \"net.fabricmc.fabric.api.resource.IdentifiableResourceReloadListener.getFabricDependencies()\" is null",
                    )));
                }
                let Some(items) = collection_items(runtime, &deps) else {
                    return None;
                };
                for dependency in items {
                    let Some(dependency) = (match id_argument(
                        Some(&dependency),
                        "The dependency identifier should not be null.",
                    ) {
                        Ok(id) => id,
                        Err(error) => return Some(Err(error)),
                    }) else {
                        return None;
                    };
                    if let Err(error) =
                        add_order(&mut runtime.static_fields, &resource, &dependency, &id)
                    {
                        return Some(Err(error));
                    }
                }
            }
            return Some(Ok(Value::Void));
        }
        if r.name == "addListenerOrdering" {
            let Some(first) = (match id_argument(
                args.first(),
                "The first listener identifier should not be null.",
            ) {
                Ok(id) => id,
                Err(error) => return Some(Err(error)),
            }) else {
                return None;
            };
            let Some(second) = (match id_argument(
                args.get(1),
                "The second listener identifier should not be null.",
            ) {
                Ok(id) => id,
                Err(error) => return Some(Err(error)),
            }) else {
                return None;
            };
            return Some(
                add_order(&mut runtime.static_fields, &resource, &first, &second)
                    .map(|_| Value::Void),
            );
        }
        return None;
    }
    if crate::runtime::host_game_event::fired_event(receiver)
        == Some(crate::runtime::events::GameEvent::ClientReloadListeners)
    {
        if r.name == "addListener" {
            let Some(id) =
                (match id_argument(args.first(), "The listener identifier should not be null.") {
                    Ok(id) => id,
                    Err(error) => return Some(Err(error)),
                })
            else {
                return None;
            };
            let Some(listener) = args.get(1).cloned() else {
                return None;
            };
            if matches!(listener, Value::Ref(None)) {
                return Some(Err(null_pointer("The listener should not be null.")));
            }
            return Some(register(
                runtime,
                "neoforge-client",
                &id,
                &listener,
                "java/lang/IllegalArgumentException",
                format!("Attempted to register two reload listeners for the same key: {id}"),
                format!("Attempted to register two reload listeners for the same key: {id}"),
            ));
        }
        if r.name == "addDependency" {
            let Some(first) = (match id_argument(
                args.first(),
                "The first listener identifier should not be null.",
            ) {
                Ok(id) => id,
                Err(error) => return Some(Err(error)),
            }) else {
                return None;
            };
            let Some(second) = (match id_argument(
                args.get(1),
                "The second listener identifier should not be null.",
            ) {
                Ok(id) => id,
                Err(error) => return Some(Err(error)),
            }) else {
                return None;
            };
            for id in [&first, &second] {
                if !runtime
                    .static_fields
                    .contains_key(&state_key("neoforge-client", id))
                {
                    return Some(Err(duplicate(
                        "java/lang/IllegalArgumentException",
                        format!("Unknown reload listener: {id}"),
                    )));
                }
            }
            return Some(
                add_order(
                    &mut runtime.static_fields,
                    "neoforge-client",
                    &first,
                    &second,
                )
                .map(|_| Value::Void),
            );
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn converter_transformations_follow_java_bytecode() {
        let converter = converter("textures");
        let Value::Host(ref handle) = converter else {
            unreachable!()
        };
        let id = crate::runtime::host_ids::identifier_of_text("demo:foo/bar");
        assert_eq!(
            id_transform(handle, &id, true)
                .ok()
                .and_then(|v| crate::runtime::host_ids::identifier_key(&v).map(str::to_owned)),
            Some("demo:textures/foo/bar.json".into())
        );
        assert_eq!(
            id_transform(
                handle,
                &crate::runtime::host_ids::identifier_of_text("demo:textures/foo/bar.json"),
                false
            )
            .ok()
            .and_then(|v| crate::runtime::host_ids::identifier_key(&v).map(str::to_owned)),
            Some("demo:foo/bar".into())
        );
        let mut host = StdHost::new();
        let call = MethodRef {
            owner: CONVERTER.into(),
            name: "fileToId".into(),
            descriptor:
                "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/resources/Identifier;".into(),
        };
        assert!(
            host.invoke_instance(
                &call,
                &converter,
                &[crate::runtime::host_ids::identifier_of_text(
                    "demo:textures/foo.json"
                )]
            )
            .is_ok()
        );
    }
}
