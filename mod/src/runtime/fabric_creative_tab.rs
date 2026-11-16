//! Fabric creative-tab event lookup.
use crate::runtime::{MethodRef, Value};

pub(crate) fn static_call(r: &MethodRef, args: &[Value]) -> Option<Value> {
    if r.owner == "net/fabricmc/fabric/api/creativetab/v1/CreativeModeTabEvents"
        && r.name == "modifyOutputEvent"
        && r.descriptor
            == "(Lnet/minecraft/resources/ResourceKey;)Lnet/fabricmc/fabric/api/event/Event;"
    {
        let key = crate::runtime::host_ids::resource_key_parts(args.first()?)?;
        return Some(crate::runtime::host_events::fabric_tab_event(&format!(
            "{}:{}",
            key.0, key.1
        )));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::host_ref::HostKind;

    #[test]
    fn lookup_returns_canonical_tab_event() {
        let r = MethodRef {
            owner: "net/fabricmc/fabric/api/creativetab/v1/CreativeModeTabEvents".into(),
            name: "modifyOutputEvent".into(),
            descriptor:
                "(Lnet/minecraft/resources/ResourceKey;)Lnet/fabricmc/fabric/api/event/Event;"
                    .into(),
        };
        let key =
            crate::runtime::host_ids::resource_key_named("minecraft:creative_mode_tab", "spell_engine:main");
        let first = static_call(&r, std::slice::from_ref(&key)).unwrap();
        let second = static_call(&r, &[key]).unwrap();
        assert_eq!(first, second);
        assert!(matches!(first, Value::Host(handle) if handle.kind() == HostKind::FabricEvent));
    }
}
