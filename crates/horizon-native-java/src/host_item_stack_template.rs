use crate::{
    MethodRef, NativeError, Value,
    host_ref::{HostKind, HostRef, Methods, Nested},
    jar_object,
};
fn ts(_: &HostRef, _: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    Ok(None)
}
fn hc(_: &HostRef, _: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    Ok(None)
}
fn eq(_: &HostRef, _: &Value, _: &mut dyn Nested) -> Result<Option<bool>, NativeError> {
    Ok(None)
}
fn io(_: &HostRef, _: &str, _: &dyn crate::host_ref::ClassHierarchy) -> Option<bool> {
    None
}
fn cr(_: &HostRef, _: &dyn Fn(&Value) -> bool) -> bool {
    false
}
pub(crate) const METHODS: Methods = Methods {
    to_string: ts,
    hash_code: hc,
    equals: eq,
    instance_of: io,
    crosses_jars: cr,
    compare_to: None,
};
pub(crate) fn static_call(r: &MethodRef, args: &[Value]) -> Option<Result<Value, NativeError>> {
    if r.owner == "net/minecraft/world/item/ItemStackTemplate" && r.name == "fromNonEmptyStack" {
        let key = jar_object::instance_key(args.first()?)?.to_owned();
        return Some(Ok(Value::Host(HostRef::new(
            HostKind::ItemStackTemplate,
            key,
        ))));
    }
    None
}
pub(crate) fn instance_call(r: &MethodRef, receiver: &Value) -> Option<Result<Value, NativeError>> {
    let _ = (r, receiver);
    None
}
