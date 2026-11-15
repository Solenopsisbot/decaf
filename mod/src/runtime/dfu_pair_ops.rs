//! Runtime-owned operations for DataFixerUpper Pair.
use super::*;

const OWNER: &str = "com/mojang/datafixers/util/Pair";

pub(crate) fn runtime_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
    stack: &mut [Value],
    locals: &mut [Value],
) -> Result<Option<Value>, NativeError> {
    let inherited_pair_method = r.owner == OWNER || runtime.class_is_instance(&r.owner, OWNER);
    if !inherited_pair_method {
        return Ok(None);
    }
    if r.name == "<init>"
        && r.descriptor == "(Ljava/lang/Object;Ljava/lang/Object;)V"
        && let Some(receiver) = receiver
        && jar_object::is_object(receiver)
    {
        let [first, second] = args else {
            return Ok(None);
        };
        let class = jar_object::object_class(receiver);
        if class == Some(OWNER) {
            let pair = crate::runtime::host_codec::pair(first, second);
            for slot in stack.iter_mut().chain(locals.iter_mut()) {
                if slot == receiver {
                    *slot = pair.clone();
                }
            }
            return Ok(Some(Value::Void));
        }
        // A jar subclass remains its own object. Keep only the inherited Pair
        // state in the runtime field map; the subclass's fields and overrides
        // continue through the normal jar dispatch path.
        if class.is_some_and(|class| class != OWNER) {
            runtime.set_dfu_pair_subclass_state(receiver, first, second);
            return Ok(Some(Value::Void));
        }
    }
    let Some(receiver) = receiver else {
        return Ok(None);
    };
    let Some((first, second)) = crate::runtime::host_codec::pair_parts(receiver)
        .or_else(|| runtime.dfu_pair_subclass_state(receiver))
    else {
        return Ok(None);
    };
    let callback = args.first();
    match r.name.as_str() {
        "mapFirst"
            if r.descriptor
                == "(Ljava/util/function/Function;)Lcom/mojang/datafixers/util/Pair;" =>
        {
            let Some(callback) = callback else {
                return Ok(None);
            };
            let Some(mapped) = runtime.invoke_callback(callback, &[first])? else {
                return Ok(None);
            };
            Ok(Some(crate::runtime::host_codec::pair(&mapped, &second)))
        }
        "mapSecond"
            if r.descriptor
                == "(Ljava/util/function/Function;)Lcom/mojang/datafixers/util/Pair;" =>
        {
            let Some(callback) = callback else {
                return Ok(None);
            };
            let Some(mapped) = runtime.invoke_callback(callback, &[second])? else {
                return Ok(None);
            };
            Ok(Some(crate::runtime::host_codec::pair(&first, &mapped)))
        }
        "getFirst" if r.descriptor == "()Ljava/lang/Object;" => Ok(Some(first)),
        "getSecond" if r.descriptor == "()Ljava/lang/Object;" => Ok(Some(second)),
        "swap" if r.descriptor == "()Lcom/mojang/datafixers/util/Pair;" => {
            Ok(Some(crate::runtime::host_codec::pair(&second, &first)))
        }
        _ => Ok(None),
    }
}
