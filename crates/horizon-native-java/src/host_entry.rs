//! `toString()` of the host's map entries, as JDK 25 prints them.
//!
//! A `HashMap`/`LinkedHashMap` node, a `TreeMap` entry and `Map.entry(..)`'s `KeyValueHolder`
//! are host handles (`host_map_entry.rs`). Each of those JDK classes answers
//! `key + "=" + value`, which is `String.valueOf(key) + "=" + String.valueOf(value)`: a jar key or
//! value prints through its own `toString` (#70), a box or string as Java does. The kind's row
//! prints it through the runtime, so a jar object's text is its own.

use super::*;

fn hashtable_null_entry(handle: &HostRef, value: &Value) -> bool {
    crate::host_map_entry::entry_class(handle.key()) == "java/util/Hashtable$Entry"
        && matches!(value, Value::Ref(None))
}

impl<H: Host> Runtime<'_, H> {
    /// `entry.toString()` for a host map entry; `None` for any other value. An entry whose
    /// contents or parts' text are unknown is counted.
    pub(crate) fn host_entry_text(
        &mut self,
        value: &Value,
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        let Value::Host(handle) = value else {
            return Ok(None);
        };
        if !crate::host_map_entry::is_entry(value) {
            return Ok(None);
        }
        if hashtable_null_entry(handle, value) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: Some(
                    "Cannot invoke \"Object.toString()\" because \"this.value\" is null".into(),
                ),
                frame: None,
            });
        }
        match self.host_ref_text(handle, class, method)? {
            Some(text) => Ok(Some(Value::Ref(Some(text)))),
            None => self
                .host
                .unknown_operand_call(&MethodRef {
                    owner: "java/util/Map$Entry".into(),
                    name: "toString".into(),
                    descriptor: "()Ljava/lang/String;".into(),
                })
                .map(Some),
        }
    }
}
