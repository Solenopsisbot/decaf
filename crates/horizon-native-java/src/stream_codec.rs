//! Pure construction of the game's network codecs.
///
/// `StreamCodec.composite` and `CustomPacketPayload.codec` only allocate a
/// codec object and retain the component codecs/callbacks. The actual network
/// `encode`/`decode` methods remain host calls and are counted.
use std::sync::atomic::{AtomicU64, Ordering};

use crate::host_ref::{
    ClassHierarchy, ClassSide, HostKind, HostRef, Methods, exact_class_rules_out,
};
use crate::{Host, MethodRef, NativeError, Runtime, Value};

pub(crate) const STREAM_CODEC: &str = "net/minecraft/network/codec/StreamCodec";
const STREAM_ENCODER: &str = "net/minecraft/network/codec/StreamEncoder";
const STREAM_DECODER: &str = "net/minecraft/network/codec/StreamDecoder";
const CUSTOM_CODEC: &str = "net/minecraft/network/protocol/common/custom/CustomPacketPayload$1";
const STREAM_CODEC_PREFIX: &str = "net/minecraft/network/codec/StreamCodec$";

static NEXT_CODEC: AtomicU64 = AtomicU64::new(0);

/// The static factories whose body is only `new` plus a constructor.
pub(crate) fn static_call(r: &MethodRef, args: &[Value]) -> Option<Value> {
    let result = format!(")L{STREAM_CODEC};");
    if r.descriptor.ends_with(&result) {
        if r.owner == "net/minecraft/network/codec/ByteBufCodecs"
            && ((r.name == "map"
                && r.descriptor
                    == "(Ljava/util/function/IntFunction;Lnet/minecraft/network/codec/StreamCodec;Lnet/minecraft/network/codec/StreamCodec;)Lnet/minecraft/network/codec/StreamCodec;")
                || (r.name == "collection"
                    && r.descriptor
                        == "(Ljava/util/function/IntFunction;Lnet/minecraft/network/codec/StreamCodec;)Lnet/minecraft/network/codec/StreamCodec;"))
        {
            return Some(codec(&format!("{STREAM_CODEC_PREFIX}4"), args));
        }
        if r.owner == STREAM_CODEC
            && r.name == "composite"
            && args.len() >= 3
            && args.len() % 2 == 1
        {
            let components = (args.len() - 1) / 2;
            // 26.3 has composite arities 1 through 14, implemented by
            // StreamCodec$7 through StreamCodec$20.
            if (1..=14).contains(&components) {
                return Some(codec(
                    &format!("{STREAM_CODEC_PREFIX}{}", components + 6),
                    args,
                ));
            }
        }
        if r.owner == STREAM_CODEC && r.name == "of" {
            return Some(codec(&format!("{STREAM_CODEC_PREFIX}1"), args));
        }
        if r.owner == STREAM_CODEC && r.name == "ofMember" {
            return Some(codec(&format!("{STREAM_CODEC_PREFIX}2"), args));
        }
        if r.owner == STREAM_CODEC
            && r.name == "unit"
            && r.descriptor == "(Ljava/lang/Object;)Lnet/minecraft/network/codec/StreamCodec;"
        {
            return Some(codec(&format!("{STREAM_CODEC_PREFIX}3"), args));
        }
        if r.owner == "net/minecraft/network/protocol/common/custom/CustomPacketPayload"
            && r.name == "codec"
        {
            let class = match r.descriptor.as_str() {
                "(Lnet/minecraft/network/codec/StreamMemberEncoder;Lnet/minecraft/network/codec/StreamDecoder;)Lnet/minecraft/network/codec/StreamCodec;" =>
                {
                    // The two-argument overload delegates to
                    // StreamCodec.ofMember, whose implementation is $2.
                    Some(format!("{STREAM_CODEC_PREFIX}2"))
                }
                "(Lnet/minecraft/network/protocol/common/custom/CustomPacketPayload$FallbackProvider;Ljava/util/List;)Lnet/minecraft/network/codec/StreamCodec;" => {
                    Some(CUSTOM_CODEC.to_owned())
                }
                _ => None,
            }?;
            return Some(codec(&class, args));
        }
    }
    None
}

/// A fresh implementation object retaining every constructor argument in
/// declaration order. The serial keeps repeated construction distinct even
/// when the same components are supplied.
fn codec(class: &str, args: &[Value]) -> Value {
    let serial = NEXT_CODEC.fetch_add(1, Ordering::Relaxed);
    let mut values = Vec::with_capacity(args.len() + 2);
    values.push(Value::Long(serial as i64));
    values.push(Value::Ref(Some(class.to_owned())));
    values.extend_from_slice(args);
    Value::Host(HostRef::of_values(HostKind::StreamCodec, &values))
}

/// The default `map` method allocates StreamCodec$4 and retains its receiver
/// and both callback handles. Its encode/decode behavior remains counted.
pub(crate) fn instance_call(
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if !is_stream_codec(receiver) {
        return None;
    }
    if class_of_value(receiver).as_deref() == Some("net/minecraft/network/codec/StreamCodec$3") {
        let held = match receiver {
            Value::Host(handle) => handle.held(),
            _ => &[],
        };
        let instance = held.get(2).cloned();
        match r.name.as_str() {
            "decode" if r.descriptor == "(Ljava/lang/Object;)Ljava/lang/Object;" => {
                return instance.map(Ok);
            }
            "encode" if r.descriptor == "(Ljava/lang/Object;Ljava/lang/Object;)V" => {
                let expected = instance?;
                let actual = args.get(1)?.clone();
                if actual == expected {
                    return Some(Ok(Value::Void));
                }
                let text = |value: &Value| match value {
                    Value::Ref(Some(text)) => text.clone(),
                    Value::Int(value) => value.to_string(),
                    Value::Long(value) => value.to_string(),
                    Value::Float(value) => value.to_string(),
                    Value::Void => "null".to_owned(),
                    other => format!("{other:?}"),
                };
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/IllegalStateException".into(),
                    message: Some(format!(
                        "Can't encode '{}', expected '{}'",
                        text(&actual),
                        text(&expected)
                    )),
                    frame: None,
                }));
            }
            _ => {}
        }
    }
    if r.owner == STREAM_CODEC
        && r.name == "cast"
        && r.descriptor == "()Lnet/minecraft/network/codec/StreamCodec;"
        && args.is_empty()
    {
        return Some(Ok(receiver.clone()));
    }
    if r.owner != STREAM_CODEC
        || r.name != "map"
        || r.descriptor
            != "(Ljava/util/function/Function;Ljava/util/function/Function;)Lnet/minecraft/network/codec/StreamCodec;"
        || args.len() != 2
    {
        return None;
    }
    let mut values = Vec::with_capacity(3);
    values.push(receiver.clone());
    values.extend_from_slice(args);
    Some(Ok(codec(&format!("{STREAM_CODEC_PREFIX}4"), &values)))
}

/// Runtime-dependent behavior of `StreamCodec$3` (`StreamCodec.unit`). The expected object's
/// own virtual `equals` is invoked; comparing interpreter keys would silently change Java.
pub(crate) fn runtime_instance_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let receiver = receiver?;
    if !is_stream_codec(receiver)
        || class_of_value(receiver).as_deref() != Some("net/minecraft/network/codec/StreamCodec$3")
    {
        return None;
    }
    let expected = match receiver {
        Value::Host(handle) => handle.held().get(2)?.clone(),
        _ => return None,
    };
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("decode", "(Ljava/lang/Object;)Ljava/lang/Object;") => Some(Ok(expected)),
        ("encode", "(Ljava/lang/Object;Ljava/lang/Object;)V") => {
            let actual = args.get(1)?.clone();
            if matches!(expected, Value::Ref(None)) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some(
                        "Cannot invoke \"Object.equals(Object)\" because \"value\" is null".into(),
                    ),
                    frame: None,
                }));
            }
            let equals = runtime.call_virtual(
                crate::jdk_call::Site::Virtual,
                "java/lang/Object",
                "equals",
                "(Ljava/lang/Object;)Z",
                &expected,
                std::slice::from_ref(&actual),
            );
            let equal = match equals {
                Ok(Value::Int(value)) => value != 0,
                Ok(_) => return None,
                Err(error) => return Some(Err(error)),
            };
            if equal {
                return Some(Ok(Value::Void));
            }
            let text = |value: &Value| match value {
                Value::Ref(Some(text)) => Some(text.clone()),
                Value::Ref(None) => Some("null".to_owned()),
                Value::Int(value) => Some(value.to_string()),
                Value::Long(value) => Some(value.to_string()),
                Value::Float(value) => Some(value.to_string()),
                _ => None,
            };
            let (Some(actual), Some(expected)) = (text(&actual), text(&expected)) else {
                return None;
            };
            Some(Err(NativeError::UncaughtException {
                class: "java/lang/IllegalStateException".into(),
                message: Some(format!("Can't encode '{actual}', expected '{expected}'")),
                frame: None,
            }))
        }
        _ => None,
    }
}

/// The implementation class carried by a modeled network codec handle.
fn class_of_value(value: &Value) -> Option<String> {
    match value {
        Value::Host(handle) => Some(class_of_key(handle.key()).to_owned()),
        _ => None,
    }
}

/// Whether a value is a modeled network codec handle.
pub(crate) fn is_stream_codec(value: &Value) -> bool {
    matches!(value, Value::Host(handle) if handle.kind() == HostKind::StreamCodec)
}

/// The declared type used by content declarations.
pub(crate) fn codec_type(value: &Value) -> Option<&'static str> {
    is_stream_codec(value).then_some(STREAM_CODEC)
}

pub(crate) fn class_of_key(key: &str) -> &str {
    key.split(',')
        .nth(1)
        .and_then(|marker| marker.strip_prefix('r'))
        .unwrap_or(STREAM_CODEC)
}

fn instance_of_class(class: &str, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
    if wanted == class
        || wanted == "java/lang/Object"
        || matches!(wanted, STREAM_CODEC | STREAM_ENCODER | STREAM_DECODER)
    {
        return Some(true);
    }
    if hierarchy.is_instance(class, wanted) == Some(true) {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false) || exact_class_rules_out(class, wanted))
        .then_some(false)
}

fn stream_codec_instance_of(
    handle: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    instance_of_class(class_of_key(handle.key()), wanted, hierarchy)
}

pub(crate) const STREAM_CODEC_METHODS: Methods = Methods {
    instance_of: stream_codec_instance_of,
    ..crate::host_events::DECLARED_TYPE_METHODS
};

fn owns(class: &str) -> bool {
    class == CUSTOM_CODEC
        || class
            .strip_prefix(STREAM_CODEC_PREFIX)
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=20).contains(&number))
}

fn class_instance_of(class: &str, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
    instance_of_class(class, wanted, hierarchy)
}

fn superclass(_: &str) -> Option<String> {
    Some("java/lang/Object".to_owned())
}

fn is_interface(_: &str) -> Option<bool> {
    Some(false)
}

fn classes() -> Vec<String> {
    let mut classes = (1..=20)
        .map(|number| format!("{STREAM_CODEC_PREFIX}{number}"))
        .collect::<Vec<_>>();
    classes.push(CUSTOM_CODEC.to_owned());
    classes
}

pub(crate) const STREAM_CODEC_CLASS_SIDE: ClassSide = ClassSide {
    owns,
    instance_of: class_instance_of,
    superclass,
    is_interface,
    classes,
};
