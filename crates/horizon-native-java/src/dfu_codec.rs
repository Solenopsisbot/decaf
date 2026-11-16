//! Bounded DataFixerUpper codec construction handles.
//!
//! This family records construction structure and callback roots exactly. Codec
//! execution (encode/decode/parse) and the broad factory surface remain counted.

use std::collections::{BTreeSet, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested};
use crate::{Host, MethodRef, NativeError, Runtime, Value};

const CODEC: &str = "com/mojang/serialization/Codec";
const MAP_CODEC: &str = "com/mojang/serialization/MapCodec";
const PRIMITIVE: &str = "com/mojang/serialization/codecs/PrimitiveCodec";
const RECORD_BUILDER: &str = "com/mojang/serialization/codecs/RecordCodecBuilder";
const APP: &str = "com/mojang/datafixers/kinds/App";
const OBJECT: &str = "java/lang/Object";
const DATA_RESULT: &str = "com/mojang/serialization/DataResult";

const MAX_FIELD_NAME_BYTES: usize = 256;
const MAX_GRAPH_NODES: usize = 256;
const MAX_CALLBACK_ROOTS: usize = 64;
const MAX_NODE_DEPTH: usize = 64;
static NEXT_NODE: AtomicU64 = AtomicU64::new(1);

const CONSTANT: &str = "dfu-constant";
const FIELD: &str = "dfu-field";
const OPTIONAL: &str = "dfu-optional";
const XMAP: &str = "dfu-xmap";
const MAP_XMAP: &str = "dfu-map-xmap";
const GETTER: &str = "dfu-builder-getter";
const UNIT: &str = "dfu-unit";
const UNIT_CODEC: &str = "dfu-unit-codec";

const CODEC_TYPES: &[&str] = &[
    CODEC,
    "com/mojang/serialization/Encoder",
    "com/mojang/serialization/Decoder",
];
const MAP_TYPES: &[&str] = &[
    MAP_CODEC,
    "com/mojang/serialization/CompressorHolder",
    "com/mojang/serialization/Compressable",
    "com/mojang/serialization/MapDecoder",
    "com/mojang/serialization/MapEncoder",
    "com/mojang/serialization/Keyable",
];
const BUILDER_TYPES: &[&str] = &[RECORD_BUILDER, APP];

fn held_values(value: &Value) -> Option<Vec<Value>> {
    let Value::Host(handle) = value else {
        return None;
    };
    (handle.kind() == HostKind::DfuCodec).then(|| handle.held().to_vec())
}

fn tag(value: &Value) -> Option<String> {
    held_values(value)?.first().and_then(|value| match value {
        Value::Ref(Some(tag)) => Some(tag.clone()),
        _ => None,
    })
}

fn constant_info(value: &Value) -> Option<(String, String)> {
    let values = held_values(value)?;
    let [
        Value::Ref(Some(tag)),
        Value::Ref(Some(declared)),
        _,
        Value::Ref(Some(name)),
    ] = values.as_slice()
    else {
        return None;
    };
    (tag == CONSTANT).then(|| (declared.clone(), name.clone()))
}

fn is_callback(value: &Value) -> bool {
    matches!(value, Value::Host(handle) if handle.kind() == HostKind::Callback)
}

fn map_codec(value: &Value) -> bool {
    let Some(tag) = tag(value) else {
        return false;
    };
    if matches!(tag.as_str(), FIELD | OPTIONAL | MAP_XMAP | GETTER | UNIT) {
        return tag != GETTER;
    }
    constant_info(value).is_some_and(|(declared, name)| declared == MAP_CODEC || name == "EMPTY")
}

fn codec_receiver(value: &Value) -> bool {
    let Some(tag) = tag(value) else {
        return false;
    };
    matches!(tag.as_str(), CONSTANT | XMAP | UNIT_CODEC) && !map_codec(value)
}

fn map_receiver(value: &Value) -> bool {
    map_codec(value) && !matches!(tag(value).as_deref(), Some(GETTER))
}

#[derive(Default)]
struct GraphStats {
    nodes: usize,
    depth: usize,
    callback_roots: BTreeSet<String>,
    seen: HashSet<String>,
}

fn walk(value: &Value, depth: usize, stats: &mut GraphStats) {
    if is_callback(value) {
        if let Value::Host(handle) = value {
            stats.callback_roots.insert(handle.key().to_owned());
            for child in handle.held() {
                walk(child, depth, stats);
            }
        }
        return;
    }
    let Value::Host(handle) = value else {
        return;
    };
    if handle.kind() != HostKind::DfuCodec || !stats.seen.insert(handle.key().to_owned()) {
        return;
    }
    stats.nodes = stats.nodes.saturating_add(1);
    stats.depth = stats.depth.max(depth);
    for child in handle.held() {
        walk(child, depth.saturating_add(1), stats);
    }
}

fn node(host: &mut dyn Host, tag: &str, values: Vec<Value>) -> Result<Value, String> {
    let mut stats = GraphStats::default();
    for value in &values {
        walk(value, 1, &mut stats);
    }
    if stats.nodes.saturating_add(1) > MAX_GRAPH_NODES {
        return Err(
            "DFU codec graph budget exceeded: current graph exceeds maximum 256 nodes".into(),
        );
    }
    if stats.depth > MAX_NODE_DEPTH {
        return Err("DFU codec nesting depth exceeded: current depth exceeds maximum 64".into());
    }
    if stats.callback_roots.len() > MAX_CALLBACK_ROOTS {
        return Err(
            "DFU codec callback-root budget exceeded: current graph exceeds maximum 64 roots"
                .into(),
        );
    }
    host.reserve_dfu_codec(1, &stats.callback_roots)?;
    let mut held = Vec::with_capacity(values.len() + 2);
    held.push(Value::Ref(Some(tag.to_owned())));
    held.push(Value::Long(NEXT_NODE.fetch_add(1, Ordering::Relaxed) as i64));
    held.extend(values);
    Ok(Value::Host(HostRef::of_values(HostKind::DfuCodec, &held)))
}

fn constant(declared: &str, owner: &str, name: &str) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::DfuCodec,
        &[
            Value::Ref(Some(CONSTANT.to_owned())),
            Value::Ref(Some(declared.to_owned())),
            Value::Ref(Some(owner.to_owned())),
            Value::Ref(Some(name.to_owned())),
        ],
    ))
}

fn instance_of(handle: &HostRef, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
    let declared = dfu_codec_types(handle.key());
    if wanted == OBJECT
        || declared.contains(&wanted)
        || hierarchy.is_instance(declared[0], wanted) == Some(true)
    {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false)
        || crate::host_ref::exact_class_rules_out(declared[0], wanted))
    .then_some(false)
}

fn primitive_text(name: &str) -> Option<&'static str> {
    Some(match name {
        "BOOL" => "Bool",
        "BYTE" => "Byte",
        "SHORT" => "Short",
        "INT" => "Int",
        "LONG" => "Long",
        "FLOAT" => "Float",
        "DOUBLE" => "Double",
        "STRING" => "String",
        "BYTE_BUFFER" => "ByteBuffer",
        "INT_STREAM" => "IntStream",
        "LONG_STREAM" => "LongStream",
        "PASSTHROUGH" => "passthrough",
        "EMPTY" => "Unit[Unit]",
        _ => return None,
    })
}

fn text(handle: &HostRef, nested: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    if handle.kind() != HostKind::DfuCodec {
        return Ok(None);
    }
    let values = handle.held();
    let Some(Value::Ref(Some(kind))) = values.first() else {
        return Ok(None);
    };
    match kind.as_str() {
        CONSTANT => {
            let Some(Value::Ref(Some(name))) = values.get(3) else {
                return Ok(None);
            };
            Ok(primitive_text(name).map(str::to_owned))
        }
        FIELD | OPTIONAL => {
            let (Some(inner), Some(Value::Ref(Some(name)))) = (values.get(2), values.get(3)) else {
                return Ok(None);
            };
            let Some(inner) = nested.text(inner)? else {
                return Ok(None);
            };
            let mut text = if kind == FIELD {
                format!("Field[{name}: {inner}]")
            } else {
                format!("OptionalFieldCodec[{name}: {inner}]")
            };
            if kind == OPTIONAL {
                if !matches!(values.get(6), Some(Value::Void) | None) {
                    text.push_str("[flatXmapped]");
                } else if !matches!(values.get(5), Some(Value::Void) | None) {
                    text.push_str("[xmapped]");
                }
            }
            Ok(Some(text))
        }
        UNIT | UNIT_CODEC => {
            let Some(value) = values.get(2) else {
                return Ok(None);
            };
            Ok(nested.text(value)?.map(|text| format!("Unit[{text}]")))
        }
        XMAP | MAP_XMAP => {
            let Some(inner) = values.get(2) else {
                return Ok(None);
            };
            Ok(nested.text(inner)?.map(|text| format!("{text}[xmapped]")))
        }
        _ => Ok(None),
    }
}

fn crosses(handle: &HostRef, f: &dyn Fn(&Value) -> bool) -> bool {
    handle.held().iter().all(f)
}

pub(crate) const DFU_CODEC_METHODS: Methods = Methods {
    to_string: text,
    hash_code: crate::host_events::DECLARED_TYPE_METHODS.hash_code,
    equals: crate::host_events::DECLARED_TYPE_METHODS.equals,
    instance_of,
    crosses_jars: crosses,
    compare_to: None,
};

pub(crate) fn dfu_codec_types(key: &str) -> &'static [&'static str] {
    let tag = key
        .split(',')
        .next()
        .unwrap_or_default()
        .strip_prefix('r')
        .unwrap_or_default();
    let declared = key
        .split(',')
        .nth(1)
        .and_then(|part| part.strip_prefix('r'));
    if matches!(tag, FIELD | OPTIONAL | MAP_XMAP | UNIT)
        || (tag == CONSTANT && declared == Some(MAP_CODEC))
    {
        MAP_TYPES
    } else if tag == GETTER {
        BUILDER_TYPES
    } else {
        CODEC_TYPES
    }
}

pub(crate) fn codec_type(value: &Value) -> Option<&'static str> {
    let Value::Host(handle) = value else {
        return None;
    };
    (handle.kind() == HostKind::DfuCodec).then(|| dfu_codec_types(handle.key())[0])
}

pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    if r.owner != CODEC
        || !matches!(
            r.descriptor.as_str(),
            "Lcom/mojang/serialization/codecs/PrimitiveCodec;"
                | "Lcom/mojang/serialization/Codec;"
                | "Lcom/mojang/serialization/MapCodec;"
        )
    {
        return None;
    }
    let names = [
        "BOOL",
        "BYTE",
        "SHORT",
        "INT",
        "LONG",
        "FLOAT",
        "DOUBLE",
        "STRING",
        "BYTE_BUFFER",
        "INT_STREAM",
        "LONG_STREAM",
        "PASSTHROUGH",
        "EMPTY",
    ];
    names
        .contains(&r.name.as_str())
        .then(|| constant(&r.descriptor[1..r.descriptor.len() - 1], &r.owner, &r.name))
}

fn field_name(args: &[Value]) -> Result<String, String> {
    let Some(Value::Ref(Some(name))) = args.first() else {
        return Err("DFU codec field name is not a String".into());
    };
    if name.len() > MAX_FIELD_NAME_BYTES {
        return Err(format!(
            "DFU codec field-name budget exceeded: current {}, maximum {}",
            name.len(),
            MAX_FIELD_NAME_BYTES
        ));
    }
    Ok(name.clone())
}

fn refuse(host: &mut dyn Host, r: &MethodRef, reason: String) -> Option<Value> {
    host.dfu_codec_refused(format!(
        "{}#{}{}: {}",
        r.owner, r.name, r.descriptor, reason
    ));
    None
}

fn build(host: &mut dyn Host, r: &MethodRef, tag: &str, values: Vec<Value>) -> Option<Value> {
    match node(host, tag, values) {
        Ok(value) => Some(value),
        Err(reason) => refuse(host, r, reason),
    }
}

fn optional_parts(descriptor: &str, args: &[Value]) -> Option<(Value, Value)> {
    let has_lifecycle_first =
        descriptor.starts_with("(Ljava/lang/String;Lcom/mojang/serialization/Lifecycle;");
    match (has_lifecycle_first, args.get(1), args.get(2)) {
        (true, Some(lifecycle), Some(default)) => Some((default.clone(), lifecycle.clone())),
        (false, Some(default), Some(lifecycle)) => Some((default.clone(), lifecycle.clone())),
        (false, Some(default), None) => Some((default.clone(), Value::Void)),
        (true, _, _) => None,
        _ => Some((Value::Void, Value::Void)),
    }
}

fn receiver_held_value(value: &Value) -> Option<Value> {
    held_values(value)?.get(2).cloned()
}

fn unit_source(value: &Value) -> Option<Value> {
    let tag = tag(value)?;
    matches!(tag.as_str(), UNIT | UNIT_CODEC).then(|| receiver_held_value(value))?
}

fn unit_value<H: Host>(runtime: &mut Runtime<'_, H>, source: &Value) -> Result<Value, NativeError> {
    if matches!(source, Value::Host(handle) if handle.kind() == HostKind::Callback) {
        Ok(runtime
            .invoke_callback(source, &[])?
            .unwrap_or(Value::Ref(None)))
    } else {
        Ok(source.clone())
    }
}

fn value_text<H: Host>(
    _runtime: &mut Runtime<'_, H>,
    value: &Value,
) -> Result<Option<String>, NativeError> {
    Ok(match value {
        Value::Ref(Some(text)) => Some(text.clone()),
        Value::Ref(None) => Some("null".to_owned()),
        Value::Int(value) => Some(value.to_string()),
        Value::Long(value) => Some(value.to_string()),
        Value::Float(value) => Some(value.to_string()),
        _ => None,
    })
}

fn runtime_text<H: Host>(
    runtime: &mut Runtime<'_, H>,
    value: &Value,
) -> Result<Option<String>, NativeError> {
    let Some(handle) = (match value {
        Value::Host(handle) if handle.kind() == HostKind::DfuCodec => Some(handle),
        _ => None,
    }) else {
        return Ok(None);
    };
    let values = handle.held();
    let Some(Value::Ref(Some(kind))) = values.first() else {
        return Ok(None);
    };
    match kind.as_str() {
        UNIT | UNIT_CODEC => {
            let Some(source) = values.get(2) else {
                return Ok(None);
            };
            let value = unit_value(runtime, source)?;
            Ok(value_text(runtime, &value)?.map(|text| format!("Unit[{text}]")))
        }
        FIELD | OPTIONAL => {
            let (Some(inner), Some(Value::Ref(Some(name)))) = (values.get(2), values.get(3)) else {
                return Ok(None);
            };
            let Some(inner) = runtime_text(runtime, inner)? else {
                return Ok(None);
            };
            let mut text = if kind == FIELD {
                format!("Field[{name}: {inner}]")
            } else {
                format!("OptionalFieldCodec[{name}: {inner}]")
            };
            if kind == OPTIONAL {
                if !matches!(values.get(6), Some(Value::Void) | None) {
                    text.push_str("[flatXmapped]");
                } else if !matches!(values.get(5), Some(Value::Void) | None) {
                    text.push_str("[xmapped]");
                }
            }
            Ok(Some(text))
        }
        XMAP | MAP_XMAP => {
            let Some(inner) = values.get(2) else {
                return Ok(None);
            };
            Ok(runtime_text(runtime, inner)?.map(|text| format!("{text}[xmapped]")))
        }
        CONSTANT => {
            let Some(Value::Ref(Some(name))) = values.get(3) else {
                return Ok(None);
            };
            Ok(primitive_text(name).map(str::to_owned))
        }
        _ => Ok(None),
    }
}

/// Runtime-dependent members of the unit MapCodec nodes. Construction stays in [`invoke`]
/// because it only needs the host's per-jar budget; execution needs the callback/runtime.
pub(crate) fn runtime_instance_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    if r.owner == DATA_RESULT && r.name == "result" && r.descriptor == "()Ljava/util/Optional;" {
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        return Ok(Some(match runtime.read_data_result(receiver) {
            Ok(Value::Ref(None)) => crate::empty_optional(),
            Ok(value) => crate::optional_wrap(value),
            Err(_) => crate::empty_optional(),
        }));
    }
    if r.owner == DATA_RESULT && r.name == "isSuccess" && r.descriptor == "()Z" {
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        return Ok(Some(Value::Int(
            runtime.read_data_result(receiver).is_ok() as i32
        )));
    }
    let Some(receiver) = receiver else {
        return Ok(None);
    };
    if r.name == "toString" && r.descriptor == "()Ljava/lang/String;" {
        return Ok(runtime_text(runtime, receiver)?.map(|text| Value::Ref(Some(text))));
    }
    let Some(source) = unit_source(receiver) else {
        return Ok(None);
    };
    let tag = tag(receiver).unwrap_or_default();
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("keys", "(Lcom/mojang/serialization/DynamicOps;)Ljava/util/stream/Stream;")
            if tag == UNIT =>
        {
            Ok(Some(runtime.new_stream(
                "java/util/stream/Stream",
                "empty",
                Vec::new(),
            )))
        }
        (
            "decode",
            "(Lcom/mojang/serialization/DynamicOps;Lcom/mojang/serialization/MapLike;)Lcom/mojang/serialization/DataResult;",
        ) if tag == UNIT => {
            let value = unit_value(runtime, &source)?;
            if matches!(value, Value::Ref(None)) {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: None,
                    frame: None,
                });
            }
            Ok(Some(runtime.data_result("success", value)))
        }
        (
            "encode",
            "(Ljava/lang/Object;Lcom/mojang/serialization/DynamicOps;Lcom/mojang/serialization/RecordBuilder;)Lcom/mojang/serialization/RecordBuilder;",
        ) if tag == UNIT => match args.get(2) {
            Some(value) => Ok(Some(value.clone())),
            None => Ok(None),
        },
        ("toString", "()Ljava/lang/String;") => {
            let value = unit_value(runtime, &source)?;
            Ok(value_text(runtime, &value)?.map(|text| Value::Ref(Some(format!("Unit[{text}]")))))
        }
        _ => Ok(None),
    }
}

pub(crate) fn invoke(
    host: &mut dyn Host,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Option<Value> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (MAP_CODEC, "unit", "(Ljava/lang/Object;)Lcom/mojang/serialization/MapCodec;")
        | (
            MAP_CODEC,
            "unit",
            "(Ljava/util/function/Supplier;)Lcom/mojang/serialization/MapCodec;",
        ) => build(host, r, UNIT, vec![args.first().cloned()?]),
        (
            CODEC | PRIMITIVE,
            "fieldOf",
            "(Ljava/lang/String;)Lcom/mojang/serialization/MapCodec;",
        ) => {
            let inner = receiver.filter(|value| codec_receiver(value))?.clone();
            let name = match field_name(args) {
                Ok(name) => name,
                Err(reason) => return refuse(host, r, reason),
            };
            build(
                host,
                r,
                FIELD,
                vec![
                    inner,
                    Value::Ref(Some(name)),
                    Value::Ref(Some("required".to_owned())),
                    Value::Void,
                    Value::Void,
                ],
            )
        }
        (CODEC | PRIMITIVE, "optionalFieldOf", descriptor)
            if descriptor.starts_with("(Ljava/lang/String;")
                && descriptor.ends_with(")Lcom/mojang/serialization/MapCodec;") =>
        {
            let inner = receiver.filter(|value| codec_receiver(value))?.clone();
            let name = match field_name(args) {
                Ok(name) => name,
                Err(reason) => return refuse(host, r, reason),
            };
            let (default, lifecycle) = optional_parts(descriptor, args)?;
            build(
                host,
                r,
                OPTIONAL,
                vec![
                    inner,
                    Value::Ref(Some(name)),
                    Value::Ref(Some("optional".to_owned())),
                    default,
                    lifecycle,
                ],
            )
        }
        (
            MAP_CODEC,
            "forGetter",
            "(Ljava/util/function/Function;)Lcom/mojang/serialization/codecs/RecordCodecBuilder;",
        ) => {
            let inner = receiver.filter(|value| map_receiver(value))?.clone();
            let getter = args.first().filter(|value| is_callback(value))?.clone();
            build(host, r, GETTER, vec![inner, getter])
        }
        (MAP_CODEC, "codec", "()Lcom/mojang/serialization/Codec;") => {
            let receiver = receiver.filter(|value| map_receiver(value))?;
            (tag(receiver).as_deref() == Some(UNIT))
                .then(|| build(host, r, UNIT_CODEC, vec![receiver_held_value(receiver)?]))?
        }
        (
            CODEC | PRIMITIVE | MAP_CODEC,
            "xmap",
            "(Ljava/util/function/Function;Ljava/util/function/Function;)Lcom/mojang/serialization/Codec;",
        )
        | (
            CODEC | PRIMITIVE | MAP_CODEC,
            "xmap",
            "(Ljava/util/function/Function;Ljava/util/function/Function;)Lcom/mojang/serialization/MapCodec;",
        ) => {
            let inner = receiver?;
            let is_map = map_codec(inner);
            if (r.owner == MAP_CODEC && !is_map) || (r.owner != MAP_CODEC && is_map) {
                return None;
            }
            if !is_map && !codec_receiver(inner) || is_map && !map_receiver(inner) {
                return None;
            }
            let to = args.first().filter(|value| is_callback(value))?.clone();
            let from = args.get(1).filter(|value| is_callback(value))?.clone();
            build(
                host,
                r,
                if is_map { MAP_XMAP } else { XMAP },
                vec![inner.clone(), to, from],
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::callback_marker;

    fn method(owner: &str, name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    #[test]
    fn nodes_hold_callbacks_and_reject_forged_strings_and_other_families() {
        let callback = callback_marker::lambda_from_text(
            "demo/P",
            0,
            "java/util/function/Function",
            "demo/P#f(Ljava/lang/Object;)Ljava/lang/Object;",
            "",
        );
        let mut host = crate::StdHost::new();
        let inner = constant(
            "com/mojang/serialization/codecs/PrimitiveCodec",
            CODEC,
            "INT",
        );
        let field = invoke(
            &mut host,
            &method(
                CODEC,
                "fieldOf",
                "(Ljava/lang/String;)Lcom/mojang/serialization/MapCodec;",
            ),
            Some(&inner),
            &[Value::Ref(Some("n".into()))],
        )
        .unwrap();
        assert_eq!(tag(&field).as_deref(), Some(FIELD));
        let forged_text = held_values(&field).unwrap();
        assert!(matches!(forged_text.first(), Some(Value::Ref(Some(tag))) if tag == FIELD));
        let getter = invoke(
            &mut host,
            &method(
                MAP_CODEC,
                "forGetter",
                "(Ljava/util/function/Function;)Lcom/mojang/serialization/codecs/RecordCodecBuilder;",
            ),
            Some(&field),
            std::slice::from_ref(&callback),
        )
        .unwrap();
        assert_eq!(tag(&getter).as_deref(), Some(GETTER));
        assert!(held_values(&getter).unwrap().iter().any(is_callback));
        let stream = Value::Host(HostRef::new(HostKind::StreamCodec, "stream"));
        assert!(
            invoke(
                &mut host,
                &method(
                    CODEC,
                    "xmap",
                    "(Ljava/util/function/Function;Ljava/util/function/Function;)Lcom/mojang/serialization/Codec;",
                ),
                Some(&stream),
                &[callback.clone(), callback.clone()],
            )
            .is_none()
        );
        let forged = Value::Ref(Some("dfu-field,1".into()));
        assert!(
            invoke(
                &mut host,
                &method(
                    MAP_CODEC,
                    "forGetter",
                    "(Ljava/util/function/Function;)Lcom/mojang/serialization/codecs/RecordCodecBuilder;",
                ),
                Some(&forged),
                std::slice::from_ref(&callback),
            )
            .is_none()
        );
    }

    #[test]
    fn collector_keeps_xmap_callback_after_sources_are_dropped() {
        let callback = callback_marker::lambda_from_text(
            "demo/P",
            1,
            "java/util/function/Function",
            "demo/P#g(Ljava/lang/Object;)Ljava/lang/Object;",
            "",
        );
        let inner = constant(
            "com/mojang/serialization/codecs/PrimitiveCodec",
            CODEC,
            "INT",
        );
        let mut host = crate::StdHost::new();
        let mapped = invoke(
            &mut host,
            &method(
                CODEC,
                "xmap",
                "(Ljava/util/function/Function;Ljava/util/function/Function;)Lcom/mojang/serialization/Codec;",
            ),
            Some(&inner),
            &[callback.clone(), callback.clone()],
        )
        .unwrap();
        let mut host = crate::StdHost::new();
        host.persistent_static_fields
            .insert("demo/Main.CODEC".into(), mapped.clone());
        drop(callback);
        drop(inner);
        let mut heap = crate::heap::JavaHeap::default();
        for _ in 0..3 {
            crate::heap::collect(&mut heap, &mut host, &[]);
        }
        assert!(held_values(&mapped).unwrap().iter().any(is_callback));
    }
    #[test]
    fn per_jar_budgets_refuse_with_current_and_maximum() {
        let field_method = method(
            CODEC,
            "fieldOf",
            "(Ljava/lang/String;)Lcom/mojang/serialization/MapCodec;",
        );
        let inner = constant(
            "com/mojang/serialization/codecs/PrimitiveCodec",
            CODEC,
            "INT",
        );
        let mut host = crate::StdHost::new();
        for index in 0..256 {
            let name = Value::Ref(Some(format!("field-{index}")));
            assert!(invoke(&mut host, &field_method, Some(&inner), &[name]).is_some());
        }
        assert!(
            invoke(
                &mut host,
                &field_method,
                Some(&inner),
                &[Value::Ref(Some("overflow".into()))]
            )
            .is_none()
        );
        assert!(host.dfu_codec_refusals().iter().any(|text| {
            text.contains("node budget exceeded")
                && text.contains("current 256")
                && text.contains("maximum 256")
        }));

        let mut callback_host = crate::StdHost::new();
        let xmap_method = method(
            CODEC,
            "xmap",
            "(Ljava/util/function/Function;Ljava/util/function/Function;)Lcom/mojang/serialization/Codec;",
        );
        for index in 0..64 {
            let callback = callback_marker::lambda_from_text(
                "demo/Budget",
                index as u16,
                "java/util/function/Function",
                "demo/Budget#map(Ljava/lang/Object;)Ljava/lang/Object;",
                "",
            );
            assert!(
                invoke(
                    &mut callback_host,
                    &xmap_method,
                    Some(&inner),
                    &[callback.clone(), callback],
                )
                .is_some()
            );
        }
        let callback = callback_marker::lambda_from_text(
            "demo/Budget",
            64,
            "java/util/function/Function",
            "demo/Budget#map(Ljava/lang/Object;)Ljava/lang/Object;",
            "",
        );
        assert!(
            invoke(
                &mut callback_host,
                &xmap_method,
                Some(&inner),
                &[callback.clone(), callback],
            )
            .is_none()
        );
        assert!(callback_host.dfu_codec_refusals().iter().any(|text| {
            text.contains("callback-root budget exceeded")
                && text.contains("current 64")
                && text.contains("maximum 64")
        }));

        let mut name_host = crate::StdHost::new();
        let long_name = Value::Ref(Some("x".repeat(MAX_FIELD_NAME_BYTES + 1)));
        assert!(invoke(&mut name_host, &field_method, Some(&inner), &[long_name]).is_none());
        assert!(name_host.dfu_codec_refusals().iter().any(|text| {
            text.contains("field-name budget exceeded")
                && text.contains("current 257")
                && text.contains("maximum 256")
        }));
    }
}
