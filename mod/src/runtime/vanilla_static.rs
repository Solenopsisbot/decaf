//! Vanilla static fields whose initializers produce host handles.

use crate::runtime::{MethodRef, Value, host_codec};

const COMMON_COMPONENTS: &str = "net/minecraft/network/chat/CommonComponents";
const IDENTIFIER: &str = "net/minecraft/resources/Identifier";
const COMPONENT: &str = "Lnet/minecraft/network/chat/Component;";
const CODEC: &str = "Lcom/mojang/serialization/Codec;";

pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (COMMON_COMPONENTS, "EMPTY", COMPONENT) => {
            Some(Value::Host(crate::runtime::host_ref::HostRef::new(
                crate::runtime::host_ref::HostKind::Component,
                "CommonComponents.EMPTY:{\"text\":\"\"}",
            )))
        }
        (IDENTIFIER, "CODEC", CODEC) => Some(host_codec::static_codec(
            "com/mojang/serialization/Codec",
            IDENTIFIER,
            "CODEC",
        )),
        _ => None,
    }
}
