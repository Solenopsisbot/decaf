//! Gson 2.14's `Gson.toJsonTree(Object)` for bounded plain values.
//!
//! The existing writer owns the exact field/collection/primitive JSON shape;
//! this adapter parses that text back through the existing Gson tree model so
//! the returned value is a real `JsonElement` tree, with the same insertion
//! order and null policy as `toJson`.

use crate::{Host, MethodRef, NativeError, Runtime, Value};

const GSON: &str = "com/google/gson/Gson";
const TO_JSON_TREE: &str = "(Ljava/lang/Object;)Lcom/google/gson/JsonElement;";

pub(crate) fn instance_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    if r.owner != GSON || r.name != "toJsonTree" || r.descriptor != TO_JSON_TREE {
        return Ok(None);
    }
    let style = runtime.gson_style_of(Some(receiver));
    if style.is_counted() {
        return runtime.host.invoke_instance(r, receiver, args).map(Some);
    }
    let Some(value) = args.first() else {
        return Ok(None);
    };
    let text = match runtime.gson_to_json_tree(value, &style, 0)? {
        Some(text) => text,
        None => return runtime.host.invoke_instance(r, receiver, args).map(Some),
    };
    if style.special_floats() {
        crate::gson_tree::parse::parse_text_with_special(runtime, &text).map(Some)
    } else {
        crate::gson_tree::parse::parse_text(runtime, &text).map(Some)
    }
}
