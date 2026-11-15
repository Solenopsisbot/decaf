//! Small exact Apache Commons Lang operations used by mod libraries.
//
/// `StringUtils.isNotEmpty(CharSequence)` is a pure predicate. Leaving the
/// call to the neutral static fallback turns every non-empty mod id into
/// `false`, so Guava's `checkArgument` reports a bogus invalid id.
use crate::{MethodRef, NativeError, StdHost, Value};

const STRING_UTILS: &str = "org/apache/commons/lang3/StringUtils";

pub(crate) fn static_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != STRING_UTILS
        || r.name != "isNotEmpty"
        || r.descriptor != "(Ljava/lang/CharSequence;)Z"
    {
        return None;
    }
    Some(match args.first() {
        Some(Value::Ref(None)) => Ok(Value::Int(0)),
        Some(Value::Ref(Some(text))) => Ok(Value::Int((!text.is_empty()) as i32)),
        _ => host.unknown_host_call(r),
    })
}
