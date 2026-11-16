//! Small side-effect-free library helpers whose contracts are independent of host state.

use crate::runtime::{MethodRef, NativeError, StdHost, Value};

const LOG4J_STRINGS: &str = "org/apache/logging/log4j/util/Strings";
const LWJGL_MATH: &str = "org/lwjgl/system/MathUtil";

pub(crate) fn static_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == LWJGL_MATH {
        let [Value::Int(value)] = args else {
            return Some(host.unknown_host_call(r));
        };
        return Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("mathIsPoT", "(I)Z") => Ok(Value::Int(((*value as u32).count_ones() == 1) as i32)),
            ("mathRoundPoT", "(I)I") => {
                let predecessor = value.wrapping_sub(1) as u32;
                let shift = (32 - predecessor.leading_zeros()) & 31;
                Ok(Value::Int(1i32.wrapping_shl(shift)))
            }
            _ => host.unknown_host_call(r),
        });
    }
    if r.owner != LOG4J_STRINGS {
        return None;
    }
    let Some(value) = args.first() else {
        return Some(host.unknown_host_call(r));
    };
    Some(match (r.name.as_str(), r.descriptor.as_str()) {
        ("isEmpty", "(Ljava/lang/CharSequence;)Z") => match string_text(value) {
            Some(text) => Ok(Value::Int(text.is_none_or(String::is_empty) as i32)),
            None => host.unknown_host_call(r),
        },
        ("isNotEmpty", "(Ljava/lang/CharSequence;)Z") => match string_text(value) {
            Some(text) => Ok(Value::Int((!text.is_none_or(String::is_empty)) as i32)),
            None => host.unknown_host_call(r),
        },
        ("isBlank", "(Ljava/lang/String;)Z") => match string_text(value) {
            Some(text) => Ok(Value::Int(is_blank(text) as i32)),
            None => host.unknown_host_call(r),
        },
        ("isNotBlank", "(Ljava/lang/String;)Z") => match string_text(value) {
            Some(text) => Ok(Value::Int((!is_blank(text)) as i32)),
            None => host.unknown_host_call(r),
        },
        _ => host.unknown_host_call(r),
    })
}

pub(crate) fn empty_factory_class(r: &MethodRef) -> Option<&'static str> {
    matches!(
        (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()),
        (
            "org/apache/commons/compress/utils/Lists",
            "newArrayList",
            "()Ljava/util/ArrayList;"
        )
    )
    .then_some("java/util/ArrayList")
}

fn string_text(value: &Value) -> Option<Option<&String>> {
    match value {
        Value::Ref(None) => Some(None),
        Value::Ref(Some(text)) if crate::runtime::value_class::is_plain_string(text) => Some(Some(text)),
        _ => None,
    }
}

fn is_blank(text: Option<&String>) -> bool {
    text.is_none_or(|text| {
        text.is_empty()
            || text
                .encode_utf16()
                .all(|unit| crate::runtime::jdk_character::is_whitespace(i32::from(unit)))
    })
}
