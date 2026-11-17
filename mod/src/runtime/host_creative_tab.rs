//! Fabric's `FabricCreativeModeTab.builder()` and the vanilla `CreativeModeTab.Builder` it returns
//! (#453), from the creative-tab module's (`fabric-creative-tab-api-v1` 5.0.22) and the 26.3 jar's
//! bytecode:
//!
//! * every `builder()` is a new `FabricCreativeModeTabBuilderImpl`;
//! * each public setter (`title`, `icon`, `displayItems`, `backgroundTexture`, `alignedRight`,
//!   `hideTitle`, `noScrollBar`) stores its argument, unchecked, and returns the builder; the last
//!   `title` wins;
//! * Fabric's `build()` throws `IllegalStateException("No display name set for CreativeModeTab")`
//!   until `title` has been called (with any argument, null too); then each `build()` is a new tab
//!   holding the builder's fields. Vanilla's own check (a special tab with display items) can't
//!   fire: only the protected `type` makes a tab special;
//! * a tab's `getDisplayName()` is the `title` its builder had.
//!
//! The host keeps every setter's argument (an `icon` supplier or `displayItems` generator is a jar
//! lambda the builder and its tabs hold) as collector roots, but answers only the display name.
//! Counted: `getDisplayName()` after `title(null)`, because a null may be a counted call's answer;
//! every other tab getter; the protected `type`.
use crate::runtime::{
    MethodRef, NativeError, Value,
    host_ref::{HostKind, HostRef},
};
use std::collections::BTreeMap;
const FABRIC: &str = "net/fabricmc/fabric/api/creativetab/v1/FabricCreativeModeTab";
const BUILDER: &str = "net/minecraft/world/item/CreativeModeTab$Builder";
const TAB: &str = "net/minecraft/world/item/CreativeModeTab";
const RETURNS_BUILDER: &str = ")Lnet/minecraft/world/item/CreativeModeTab$Builder;";

/// One builder: whether `title` was called, the title it holds (`None` when that was a null,
/// which the host doesn't answer), and every argument a setter stored.
#[derive(Debug, Default, Clone)]
struct Builder {
    titled: bool,
    title: Option<Value>,
    held: Vec<Value>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct State {
    next: u64,
    builders: BTreeMap<String, Builder>,
    /// Tab key → its display name (`None`: counted).
    tabs: BTreeMap<String, Option<Value>>,
}
impl State {
    /// The values the host holds: collector roots.
    pub(crate) fn values(&self) -> impl Iterator<Item = &Value> {
        self.builders
            .values()
            .flat_map(|builder| builder.held.iter())
            .chain(self.tabs.values().flatten())
    }
}

/// The argument a public setter stores, by name and parameter list; `None` for any other method.
fn setter(name: &str, parameters: &str) -> Option<bool> {
    Some(match (name, parameters) {
        ("title", "(Lnet/minecraft/network/chat/Component;")
        | ("icon", "(Ljava/util/function/Supplier;")
        | ("displayItems", "(Lnet/minecraft/world/item/CreativeModeTab$DisplayItemsGenerator;")
        | ("backgroundTexture", "(Lnet/minecraft/resources/Identifier;") => true,
        ("alignedRight" | "hideTitle" | "noScrollBar", "(") => false,
        _ => return None,
    })
}

pub(crate) fn static_call(
    state: &mut State,
    r: &MethodRef,
    _: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == FABRIC
        && r.name == "builder"
        && r.descriptor == "()Lnet/minecraft/world/item/CreativeModeTab$Builder;"
    {
        state.next += 1;
        let k = format!("b#{}", state.next);
        state.builders.insert(k.clone(), Builder::default());
        return Some(Ok(Value::Host(HostRef::new(
            HostKind::CreativeModeTabBuilder,
            k,
        ))));
    }
    None
}
pub(crate) fn instance_call(
    state: &mut State,
    r: &MethodRef,
    recv: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let Value::Host(h) = recv else { return None };
    match h.kind() {
        HostKind::CreativeModeTabBuilder => {
            if r.owner != BUILDER {
                return None;
            };
            if r.name == "build" && r.descriptor == "()Lnet/minecraft/world/item/CreativeModeTab;" {
                let builder = state.builders.get(h.key())?;
                if !builder.titled {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalStateException".into(),
                        message: Some("No display name set for CreativeModeTab".into()),
                        frame: None,
                    }));
                }
                let title = builder.title.clone();
                state.next += 1;
                let k = format!("t#{}", state.next);
                state.tabs.insert(k.clone(), title);
                return Some(Ok(Value::Host(HostRef::new(HostKind::CreativeModeTab, k))));
            }
            let parameters = r.descriptor.strip_suffix(RETURNS_BUILDER)?;
            let stores = setter(&r.name, parameters)?;
            let builder = state.builders.get_mut(h.key())?;
            if stores {
                let [arg] = args else { return None };
                builder.held.push(arg.clone());
                if r.name == "title" {
                    builder.titled = true;
                    builder.title = (!matches!(arg, Value::Ref(None))).then(|| arg.clone());
                }
            }
            Some(Ok(recv.clone()))
        }
        HostKind::CreativeModeTab => {
            if r.owner == TAB
                && r.name == "getDisplayName"
                && r.descriptor == "()Lnet/minecraft/network/chat/Component;"
            {
                return Some(Ok(state.tabs.get(h.key())?.clone()?));
            }
            None
        }
        _ => None,
    }
}
