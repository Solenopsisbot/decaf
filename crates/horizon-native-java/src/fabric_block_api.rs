use crate::host_ref::{HostKind, HostRef};
use crate::{MethodRef, NativeError, Value, host_class, host_ids};
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone)]
pub(crate) struct State {
    lookups: BTreeMap<String, (String, String)>,
}

fn thrown(class: &str, message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn canonical(name: &str) -> Option<String> {
    let dotted = name.replace('/', ".");
    if dotted
        .split('$')
        .any(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit()))
    {
        None
    } else {
        Some(dotted.replace('$', "."))
    }
}
pub(crate) fn static_call(
    state: &mut State,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != "net/fabricmc/fabric/api/lookup/v1/block/BlockApiLookup" || r.name != "get" {
        return None;
    }
    let [id, api, ctx] = args else { return None };
    let id = host_ids::identifier_key(id)?.to_string();
    let api = host_class::class_name(api)?.to_string();
    let ctx = host_class::class_name(ctx)?.to_string();
    let key = format!("{id}|{api}|{ctx}");
    if let Some((old_api, old_ctx)) = state.lookups.get(&id) {
        if old_api != &api || old_ctx != &ctx {
            let (Some(oa), Some(oc), Some(na), Some(nc)) = (
                canonical(old_api),
                canonical(old_ctx),
                canonical(&api),
                canonical(&ctx),
            ) else {
                return None;
            };
            return Some(Err(thrown(
                "java/lang/IllegalArgumentException",
                &format!(
                    "Lookup with id {id} is already registered with api class {oa} and context class {oc}. It can\'t be registered with api class {na} and context class {nc}."
                ),
            )));
        }
    } else {
        state.lookups.insert(id, (api.clone(), ctx.clone()));
    }
    Some(Ok(Value::Host(HostRef::new(
        HostKind::FabricBlockApiLookup,
        key,
    ))))
}
pub(crate) fn instance_call(
    _state: &mut State,
    r: &MethodRef,
    recv: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let Value::Host(h) = recv else { return None };
    if h.kind() != HostKind::FabricBlockApiLookup {
        return None;
    };
    let p = h.key().split('|').collect::<Vec<_>>();
    match r.name.as_str() {
        "getId" => Some(Ok(host_ids::identifier_of_text(p.first().copied()?))),
        "apiClass" => Some(Ok(host_class::class_value(p.get(1).copied()?))),
        "contextClass" => Some(Ok(host_class::class_value(p.get(2).copied()?))),
        "registerFallback" => {
            if matches!(args.first(), Some(Value::Ref(None))) {
                Some(Err(thrown(
                    "java/lang/NullPointerException",
                    "BlockApiProvider may not be null.",
                )))
            } else {
                Some(Ok(Value::Void))
            }
        }
        _ => None,
    }
}
