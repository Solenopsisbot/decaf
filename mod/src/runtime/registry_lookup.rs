//! Registry lookup calls used while a mod resolves its registry by key.

use crate::runtime::{MethodRef, NativeError, StdHost, Value};

pub(crate) fn instance_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if !crate::runtime::registry_contents::is_registry_owner(&r.owner)
        || r.name != "getOptional"
        || r.descriptor != "(Lnet/minecraft/resources/ResourceKey;)Ljava/util/Optional;"
    {
        return None;
    }
    let Some(registry) = crate::runtime::host_registry::registry_id(receiver) else {
        return Some(host.unknown_host_call(r));
    };
    let Some(id) = args.first().and_then(crate::runtime::host_ids::named_id) else {
        return Some(host.unknown_host_call(r));
    };
    if registry != "minecraft:root" || !crate::runtime::vanilla_registries::is_built_in(&id) {
        return Some(host.unknown_host_call(r));
    }
    Some(Ok(crate::runtime::host_optional::optional_wrap(
        crate::runtime::host_registry::registry(&id),
    )))
}
