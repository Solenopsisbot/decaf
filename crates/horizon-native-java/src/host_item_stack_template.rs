//! Vanilla 26.3 `ItemStackTemplate` model (`net/minecraft/world/item/ItemStackTemplate`).
//!
//! Backed by `StdHost.recipes.templates` so templates created by constructors,
//! `fromNonEmptyStack`, or DFU codec decoding share exact validation and `apply`/`create`
//! behaviour.

use crate::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested, exact_class_rules_out};
use crate::recipe_models::{
    ComponentPatch, DATA_COMPONENT_GETTER, ITEM_INSTANCE, TemplateData, empty_patch,
    is_empty_stack_handle, item_holder_for_id, item_stack_template_illegal_state,
    read_live_stack_opt, resolve_item_id, resolve_item_tag, stack_component_get, template_apply,
};
use crate::{MethodRef, NativeError, StdHost, Value, expect_int};

const OWNER: &str = "net/minecraft/world/item/ItemStackTemplate";

fn ts(_: &HostRef, _: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    Ok(None)
}
fn hc(_: &HostRef, _: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    Ok(None)
}
fn eq(_: &HostRef, _: &Value, _: &mut dyn Nested) -> Result<Option<bool>, NativeError> {
    Ok(None)
}
fn io(_: &HostRef, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
    if wanted == "java/lang/Object"
        || wanted == OWNER
        || wanted == ITEM_INSTANCE
        || wanted == DATA_COMPONENT_GETTER
        || wanted == "java/lang/Record"
        || hierarchy.is_instance(OWNER, wanted) == Some(true)
    {
        return Some(true);
    }
    (hierarchy.is_interface(wanted) == Some(false) || exact_class_rules_out(OWNER, wanted))
        .then_some(false)
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
fn npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

pub(crate) fn static_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER {
        return None;
    }
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "fromNonEmptyStack",
            "(Lnet/minecraft/world/item/ItemStack;)Lnet/minecraft/world/item/ItemStackTemplate;",
        ) => {
            let stack = args.first()?;
            if matches!(stack, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            if is_empty_stack_handle(stack) {
                return Some(Err(item_stack_template_illegal_state(
                    "Stack must be non-empty",
                )));
            }
            if let Some(live) = read_live_stack_opt(host, stack) {
                if live.is_empty() {
                    return Some(Err(item_stack_template_illegal_state(
                        "Stack must be non-empty",
                    )));
                }
                let item_id = live.resolved_item_id(host)?;
                return Some(Ok(host.recipes.alloc_template(TemplateData {
                    item_id,
                    count: live.count,
                    patch: live.patch,
                })));
            }
            None
        }
        (
            "fromNonEmptyStack",
            "(Lnet/minecraft/world/item/ItemStack;I)Lnet/minecraft/world/item/ItemStackTemplate;",
        ) => {
            let stack = args.first()?;
            let count = match expect_int(r, args.get(1)?) {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };
            if matches!(stack, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            if is_empty_stack_handle(stack) || count <= 0 {
                return Some(Err(item_stack_template_illegal_state(
                    "Stack must be non-empty",
                )));
            }
            if let Some(live) = read_live_stack_opt(host, stack) {
                if live.is_empty() {
                    return Some(Err(item_stack_template_illegal_state(
                        "Stack must be non-empty",
                    )));
                }
                let item_id = live.resolved_item_id(host)?;
                return Some(Ok(host.recipes.alloc_template(TemplateData {
                    item_id,
                    count,
                    patch: live.patch,
                })));
            }
            None
        }
        _ => None,
    }
}

pub(crate) fn instance_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let Value::Host(handle) = receiver else {
        return None;
    };
    if handle.kind() != HostKind::ItemStackTemplate {
        return None;
    }
    let key = handle.key().to_owned();
    if r.owner == OWNER && r.name == "<init>" {
        let (item_arg, count, patch) = match r.descriptor.as_str() {
            "(Lnet/minecraft/world/item/Item;)V" => (args.first()?, 1, ComponentPatch::default()),
            "(Lnet/minecraft/world/item/Item;I)V" => {
                let count = match expect_int(r, args.get(1)?) {
                    Ok(n) => n,
                    Err(e) => return Some(Err(e)),
                };
                (args.first()?, count, ComponentPatch::default())
            }
            "(Lnet/minecraft/world/item/Item;Lnet/minecraft/core/component/DataComponentPatch;)V" =>
            {
                let patch_arg = args.get(1)?;
                if matches!(patch_arg, Value::Ref(None)) {
                    return Some(Err(npe()));
                }
                let patch = host.recipes.patch_of(patch_arg)?.clone();
                (args.first()?, 1, patch)
            }
            "(Lnet/minecraft/core/Holder;ILnet/minecraft/core/component/DataComponentPatch;)V" => {
                let count = match expect_int(r, args.get(1)?) {
                    Ok(n) => n,
                    Err(e) => return Some(Err(e)),
                };
                let patch_arg = args.get(2)?;
                if matches!(patch_arg, Value::Ref(None)) {
                    return Some(Err(npe()));
                }
                let patch = host.recipes.patch_of(patch_arg)?.clone();
                (args.first()?, count, patch)
            }
            _ => return None,
        };
        if matches!(item_arg, Value::Ref(None)) {
            return Some(Err(npe()));
        }
        let item_id = resolve_item_id(host, item_arg)?;
        if count == 0 || item_id == "minecraft:air" {
            return Some(Err(item_stack_template_illegal_state(
                "Item must be non-empty",
            )));
        }
        host.recipes.templates.insert(
            key,
            TemplateData {
                item_id,
                count,
                patch,
            },
        );
        return Some(Ok(Value::Void));
    }

    if !matches!(
        r.owner.as_str(),
        OWNER | ITEM_INSTANCE | DATA_COMPONENT_GETTER
    ) {
        return None;
    }
    let data = host.recipes.templates.get(&key)?.clone();
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("count", "()I") => Some(Ok(Value::Int(data.count))),
        ("item" | "typeHolder", "()Lnet/minecraft/core/Holder;") => {
            Some(Ok(item_holder_for_id(host, &data.item_id)))
        }
        ("components", "()Lnet/minecraft/core/component/DataComponentPatch;") => {
            if data.patch.is_empty() {
                Some(Ok(empty_patch()))
            } else {
                Some(Ok(host.recipes.alloc_patch(data.patch)))
            }
        }
        ("withCount", "(I)Lnet/minecraft/world/item/ItemStackTemplate;") => {
            let count = match expect_int(r, args.first()?) {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };
            if count == data.count {
                return Some(Ok(receiver.clone()));
            }
            if count == 0 || data.item_id == "minecraft:air" {
                return Some(Err(item_stack_template_illegal_state(
                    "Item must be non-empty",
                )));
            }
            Some(Ok(host.recipes.alloc_template(TemplateData {
                item_id: data.item_id,
                count,
                patch: data.patch,
            })))
        }
        ("create", "()Lnet/minecraft/world/item/ItemStack;") => {
            template_apply(host, &key, data.count, &ComponentPatch::default()).map(Ok)
        }
        (
            "apply",
            "(Lnet/minecraft/core/component/DataComponentPatch;)Lnet/minecraft/world/item/ItemStack;",
        ) => {
            let patch_arg = args.first()?;
            if matches!(patch_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let base_patch = host.recipes.patch_of(patch_arg)?.clone();
            template_apply(host, &key, data.count, &base_patch).map(Ok)
        }
        (
            "apply",
            "(ILnet/minecraft/core/component/DataComponentPatch;)Lnet/minecraft/world/item/ItemStack;",
        ) => {
            let count = match expect_int(r, args.first()?) {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };
            let patch_arg = args.get(1)?;
            if matches!(patch_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let base_patch = host.recipes.patch_of(patch_arg)?.clone();
            template_apply(host, &key, count, &base_patch).map(Ok)
        }
        ("is", "(Lnet/minecraft/world/item/Item;)Z") | ("is", "(Lnet/minecraft/core/Holder;)Z") => {
            let arg = args.first()?;
            if matches!(arg, Value::Ref(None)) {
                return Some(Ok(Value::Int(0)));
            }
            let other_id = resolve_item_id(host, arg)?;
            Some(Ok(Value::Int(i32::from(data.item_id == other_id))))
        }
        ("is", "(Lnet/minecraft/tags/TagKey;)Z") => {
            let arg = args.first()?;
            let (reg, loc) = crate::recipe_models::tag_key_parts(arg)?;
            if reg != "minecraft:item" {
                return Some(Ok(Value::Int(0)));
            }
            let matched =
                resolve_item_tag(host, &loc).is_some_and(|members| members.contains(&data.item_id));
            Some(Ok(Value::Int(i32::from(matched))))
        }
        ("get", "(Lnet/minecraft/core/component/DataComponentType;)Ljava/lang/Object;") => {
            let type_arg = args.first()?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let comp_id = crate::recipe_models::resolve_data_component_type_id(host, type_arg)?;
            stack_component_get(host, &data.item_id, &data.patch, &comp_id).map(Ok)
        }
        // 26.3 `DataComponentGetter.getOrDefault` (default method) offsets 0-17:
        // `T v = get(type); return v != null ? v : defaultValue`.
        (
            "getOrDefault",
            "(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Ljava/lang/Object;",
        ) => {
            let type_arg = args.first()?;
            if matches!(type_arg, Value::Ref(None)) {
                return Some(Err(npe()));
            }
            let comp_id = crate::recipe_models::resolve_data_component_type_id(host, type_arg)?;
            let value = stack_component_get(host, &data.item_id, &data.patch, &comp_id)?;
            Some(Ok(if matches!(value, Value::Ref(None)) {
                args.get(1)?.clone()
            } else {
                value
            }))
        }
        _ => None,
    }
}
