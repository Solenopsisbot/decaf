//! Client item parts: a jar's own numeric item model properties and tooltip components, run on
//! the client runtime for an embedder's item model and tooltip hooks.
//!
//! Vanilla 26.3 reaches a mod's client item code through three doors, each of which this module
//! opens on the jar's own bytecode (`docs/java-item-parts.md`, steps 7 and 8):
//!
//! * **A numeric `range_dispatch` property.** `RangeSelectItemModelProperties.ID_MAPPER` (a
//!   private `ExtraCodecs.LateBoundIdMapper<Identifier, MapCodec<? extends
//!   RangeSelectItemModelProperty>>`) maps a property id to the codec an item definition's
//!   `"property"` names. A jar reaches it through a Mixin (`@Shadow` of the field, a `TAIL` into
//!   `bootstrap()`, run by `bootstrap_mixins.rs` at its measured client turn); its `put` is
//!   recorded by `client_registrations.rs` ([`StdHost::client_range_select_properties`]). The
//!   embedder registers each id with its own item model loader, decodes a node through the jar's
//!   codec ([`NativeJar::decode_range_select_property`], `RangeSelectItemModel.Unbaked`'s
//!   `dispatchMap` reads the property from the whole node map) and asks the decoded property
//!   per stack ([`NativeJar::range_select_property_value`]: its
//!   `get(ItemStack, ClientLevel, ItemOwner, int)`).
//! * **`Item.getTooltipImage(ItemStack)`.** The jar item's own override, run on the stack
//!   ([`NativeJar::item_tooltip_image`]); its `Optional<TooltipComponent>`'s value is the jar's
//!   data object.
//! * **`ClientTooltipComponent.create(TooltipComponent)`.** Vanilla's static interface method
//!   knows only `BundleTooltip` and `ClientActivatorTooltip`; a mod maps its own data with a
//!   cancellable Mixin `@Inject(at = @At("HEAD"))` into it. [`NativeJar::client_tooltip_component`]
//!   runs those callbacks in Mixin's apply order and takes the first `setReturnValue`; the
//!   component's `getWidth(Font)`, `getHeight(Font)` and `extractImage(Font, int, int, int, int,
//!   GuiGraphicsExtractor)` then run on the client surface the jar menu screens draw through
//!   ([`NativeJar::tooltip_component_size`], [`NativeJar::draw_tooltip_component_image`]), whose
//!   `GuiGraphicsExtractor` records the draws (`client_surface.rs`).
//!
//! Nothing here names a mod. Every answer is the jar's own bytecode on the runtime's models, or
//! the call counts (its [`ItemCallReport`]) and the embedder keeps vanilla's answer: a property
//! reads as an unregistered one would, a tooltip shows no image.

use std::collections::BTreeMap;

use crate::runtime::item_parts::{ItemCallReport, ItemHook};
use crate::runtime::jdk_call::Site;
use crate::runtime::{
    ExecutionBudget, Host, MethodRef, MixinInjectionPlan, MixinSide, MixinSkipReason, NativeError,
    NativeJar, Runtime, StdHost, Value, block_use, client_surface::DrawOp, game_object,
};

const RANGE_SELECT_PROPERTY: &str =
    "net/minecraft/client/renderer/item/properties/numeric/RangeSelectItemModelProperty";
const RANGE_SELECT_GET: &str = "(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/client/multiplayer/ClientLevel;Lnet/minecraft/world/entity/ItemOwner;I)F";
const CLIENT_TOOLTIP_COMPONENT: &str =
    "net/minecraft/client/gui/screens/inventory/tooltip/ClientTooltipComponent";
const CREATE: &str = "create";
const CREATE_DESC: &str = "(Lnet/minecraft/world/inventory/tooltip/TooltipComponent;)Lnet/minecraft/client/gui/screens/inventory/tooltip/ClientTooltipComponent;";
const CREATE_HANDLER_DESC: &str = "(Lnet/minecraft/world/inventory/tooltip/TooltipComponent;Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable;)V";
const FONT_SIZE_DESC: &str = "(Lnet/minecraft/client/gui/Font;)I";
const EXTRACT_IMAGE_DESC: &str =
    "(Lnet/minecraft/client/gui/Font;IIIILnet/minecraft/client/gui/GuiGraphicsExtractor;)V";
const OPTIONAL: &str = "java/util/Optional";

/// What one client item call answered (`None` when it counted or failed), with its report.
#[derive(Clone, Debug)]
pub struct ClientItemOutcome<T> {
    pub value: Option<T>,
    pub report: ItemCallReport,
}

impl<T> ClientItemOutcome<T> {
    /// The value, only when the call was exact.
    pub fn exact(self) -> Option<T> {
        if self.report.exact() {
            self.value
        } else {
            None
        }
    }
}

/// Whether the item being drawn has a level and a holder (`ItemModelResolver` passes the client
/// level and the `ItemOwner`, each `null` when there is none).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemModelHolder {
    pub level: bool,
    pub owner: bool,
}

/// A `ClientTooltipComponent`'s `getWidth(Font)` and `getHeight(Font)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooltipComponentSize {
    pub width: i32,
    pub height: i32,
}

/// The client level an item model property sees: an object of its own whose every member counts
/// (the embedder's level facts are not on this call).
fn item_model_level() -> Value {
    game_object::client_value("client/item-model/level".to_owned())
}

/// The item's holder: likewise.
fn item_model_owner() -> Value {
    game_object::client_value("client/item-model/owner".to_owned())
}

/// The `Font` a tooltip component measures and draws with: the client surface's font, which
/// answers `width` from the frame's metrics; anything else it is asked counts.
fn tooltip_font() -> Value {
    game_object::client_value("client/font".to_owned())
}

impl StdHost {
    /// The numeric item model properties the jar's Java code put into
    /// `RangeSelectItemModelProperties.ID_MAPPER`, in `put` order: each id with its `MapCodec`
    /// (`ExtraCodecs.LateBoundIdMapper.put`; a later `put` of an id replaces its codec, as the
    /// mapper's `HashBiMap` does).
    pub fn client_range_select_properties(&self) -> Vec<(String, Value)> {
        crate::runtime::client_surface::range_select_properties(&self.client.registrations)
    }
}

impl NativeJar {
    /// Run `run` on the persistent client runtime with `roots`, and report its counted calls.
    fn run_client_item_call<T>(
        &self,
        host: &mut StdHost,
        roots: &[Value],
        budget: &ExecutionBudget,
        run: impl FnOnce(&mut Runtime<'_, StdHost>) -> Result<Option<T>, NativeError>,
    ) -> ClientItemOutcome<T> {
        let unknowns_before: BTreeMap<String, usize> =
            host.unknown_host_calls().into_iter().collect();
        let mark = host.call_log_mark();
        let mut out = None;
        let result = self.with_persistent_runtime_with_budget(host, roots, budget, |rt| {
            out = run(rt)?;
            Ok(Value::Void)
        });
        host.rewind_call_log(mark);
        let (unknown_host_calls, unknown_host_call_count) =
            block_use::unknown_calls_since(host, &unknowns_before);
        let report = ItemCallReport {
            unknown_host_calls,
            unknown_host_call_count,
            failure: result.err().map(|error| error.to_string()),
        };
        ClientItemOutcome {
            value: out.filter(|_| report.failure.is_none()),
            report,
        }
    }

    /// Decode one `range_dispatch` node (`node`, its JSON object) through the property codec a
    /// jar registered ([`StdHost::client_range_select_properties`]), as
    /// `RangeSelectItemModelProperties.MAP_CODEC`'s `dispatchMap` does: the codec reads the whole
    /// node map. The property is retained on the host ([`StdHost::retain`]) for later
    /// [`Self::range_select_property_value`] calls; the embedder releases it
    /// ([`StdHost::release`]) when its model is dropped.
    pub fn decode_range_select_property(
        &self,
        host: &mut StdHost,
        codec: &Value,
        node: &serde_json::Value,
        budget: &ExecutionBudget,
    ) -> ClientItemOutcome<Value> {
        let outcome = self.run_client_item_call(host, std::slice::from_ref(codec), budget, |rt| {
            let unknown = |what: &str| MethodRef {
                owner: "com/mojang/serialization/MapCodec".into(),
                name: format!("decode <range_select property {what}>"),
                descriptor: "(Lcom/mojang/serialization/DynamicOps;Lcom/mojang/serialization/MapLike;)Lcom/mojang/serialization/DataResult;".into(),
            };
            let Some(map) = node.as_object() else {
                rt.host.unknown_operand_call(&unknown("node is not an object"))?;
                return Ok(None);
            };
            let before = rt.host.counted_unknown_calls();
            match crate::runtime::dfu_codec::decode_map_json(rt, codec, map)? {
                // A counted call inside the decode leaves the value unknown.
                Ok(property) if rt.host.counted_unknown_calls() == before => Ok(Some(property)),
                Ok(_) => Ok(None),
                // A DFU error is an item definition the game rejects with its own message,
                // which the embedder's loader does not have: counted.
                Err(crate::runtime::recipe_models::DecodeError::Invalid(_)) => {
                    rt.host.unknown_operand_call(&unknown("error"))?;
                    Ok(None)
                }
                Err(crate::runtime::recipe_models::DecodeError::Unmodelled(_)) => {
                    rt.host.unknown_operand_call(&unknown("unmodelled"))?;
                    Ok(None)
                }
            }
        });
        if outcome.report.exact()
            && let Some(property) = &outcome.value
        {
            host.retain(property);
        }
        outcome
    }

    /// A decoded property's `get(stack, level, owner, seed)` (`RangeSelectItemModelProperty`),
    /// the number `RangeSelectItemModel.update` scales and looks up. `level` and `owner` are
    /// `null` when the draw has none, as vanilla passes them, and otherwise objects whose every
    /// member counts.
    pub fn range_select_property_value(
        &self,
        host: &mut StdHost,
        property: &Value,
        stack: &Value,
        holder: ItemModelHolder,
        seed: i32,
        budget: &ExecutionBudget,
    ) -> ClientItemOutcome<f32> {
        let level = if holder.level {
            item_model_level()
        } else {
            Value::Ref(None)
        };
        let owner = if holder.owner {
            item_model_owner()
        } else {
            Value::Ref(None)
        };
        let roots = [property.clone(), stack.clone()];
        self.run_client_item_call(host, &roots, budget, |rt| {
            let value = rt.call_virtual(
                Site::Interface,
                RANGE_SELECT_PROPERTY,
                "get",
                RANGE_SELECT_GET,
                property,
                &[stack.clone(), level, owner, Value::Int(seed)],
            )?;
            match value {
                Value::Float(number) => Ok(Some(number as f32)),
                _ => Err(NativeError::Metadata(
                    "RangeSelectItemModelProperty.get answered a non-float value".into(),
                )),
            }
        })
    }

    /// A jar item's own `getTooltipImage(stack)` (`owner` is the class
    /// [`NativeJar::item_overrides`] found declaring it): `Some(None)` for `Optional.empty()`,
    /// `Some(Some(data))` for the jar's `TooltipComponent` object. A present value is retained
    /// on the host ([`StdHost::retain`]) until the embedder releases it.
    pub fn item_tooltip_image(
        &self,
        host: &mut StdHost,
        item: &Value,
        owner: &str,
        stack: &Value,
        budget: &ExecutionBudget,
    ) -> ClientItemOutcome<Option<Value>> {
        let roots = [item.clone(), stack.clone()];
        let outcome = self.run_client_item_call(host, &roots, budget, |rt| {
            let (name, descriptor) = ItemHook::TooltipImage.method();
            let optional = rt.invoke(
                owner,
                name,
                descriptor,
                Some(item.clone()),
                std::slice::from_ref(stack),
            )?;
            if matches!(optional, Value::Ref(None)) {
                return Err(NativeError::Metadata(
                    "getTooltipImage answered null, not an Optional".into(),
                ));
            }
            let present =
                rt.call_virtual(Site::Virtual, OPTIONAL, "isPresent", "()Z", &optional, &[])?;
            if present == Value::Int(0) {
                return Ok(Some(None));
            }
            let data = rt.call_virtual(
                Site::Virtual,
                OPTIONAL,
                "get",
                "()Ljava/lang/Object;",
                &optional,
                &[],
            )?;
            Ok(Some(Some(data)))
        });
        if outcome.report.exact()
            && let Some(Some(data)) = &outcome.value
        {
            host.retain(data);
        }
        outcome
    }

    /// The `@Inject(method = "create(...)", at = @At("HEAD"), cancellable = true)` callbacks
    /// into `ClientTooltipComponent.create` of this jar's mods (its own and the nested jars it
    /// selects on `host`'s side), in Mixin's apply order; `Err` names why they can't run exactly.
    fn tooltip_create_callbacks(&self, host: &StdHost) -> Result<Vec<MixinInjectionPlan>, String> {
        let side = host.mixin_side();
        if side != MixinSide::Client {
            return Err("ClientTooltipComponent is a client class".into());
        }
        let nested =
            crate::runtime::nested_jars::select_checked_for_side(self, host.environment_type.as_str())
                .unwrap_or_default();
        let jars: Vec<&NativeJar> = std::iter::once(self)
            .chain(nested.iter().map(|candidate| &candidate.jar))
            .collect();
        let mut owners = 0;
        let mut plans = Vec::new();
        for jar in jars {
            let configs = jar.mixin_configs();
            let config_sides = jar.mixin_config_sides();
            let mut found = false;
            for plan in jar.mixin_plans_in_apply_order() {
                if plan.target.owner != CLIENT_TOOLTIP_COMPONENT || plan.target.name != CREATE {
                    continue;
                }
                match crate::runtime::mixin_config_gate(&configs, &config_sides, &plan.callback.owner, side)
                {
                    Ok(()) => {}
                    Err(MixinSkipReason::ConfigPlugin) => {
                        return Err("a config plugin could veto a create callback".into());
                    }
                    Err(_) => continue,
                }
                // The declaring jar's class (a nested jar's own), which the runtime runs.
                let handler_static = jar
                    .classes
                    .get(&plan.callback.owner)
                    .and_then(|class| class.method(&plan.callback.name, &plan.callback.descriptor))
                    .is_some_and(|method| method.is_static());
                let shape = plan.at == "HEAD"
                    && plan.cancellable
                    && matches!(plan.target.descriptor.as_str(), "" | CREATE_DESC)
                    && plan.ordinal.is_none()
                    && plan.slice.is_none()
                    && plan.shift.is_none()
                    && plan.callback.descriptor == CREATE_HANDLER_DESC
                    && handler_static;
                if !shape {
                    return Err(format!(
                        "create callback {}#{} has a shape not run here",
                        plan.callback.owner, plan.callback.name
                    ));
                }
                found = true;
                plans.push(plan);
            }
            owners += usize::from(found);
        }
        // Two mods' HEAD callbacks into one method are ordered by Mixin across their configs.
        if owners > 1 {
            return Err("create callbacks of more than one mod".into());
        }
        Ok(plans)
    }

    /// `ClientTooltipComponent.create(data)` for a jar's `TooltipComponent` object: the jar's
    /// cancellable `HEAD` callbacks into it run in Mixin's apply order with one
    /// `CallbackInfoReturnable`, and the first that cancels answers its return value. When none
    /// does, vanilla's body runs: it knows only `BundleTooltip` and `ClientActivatorTooltip`
    /// (neither of which a jar's own record is) and otherwise throws, which Fabric API's own
    /// `TooltipComponentCallback` would intercept: counted. The component is retained on the host
    /// ([`StdHost::retain`]) until the embedder releases it.
    pub fn client_tooltip_component(
        &self,
        host: &mut StdHost,
        data: &Value,
        budget: &ExecutionBudget,
    ) -> ClientItemOutcome<Value> {
        let callbacks = self.tooltip_create_callbacks(host);
        let outcome = self.run_client_item_call(host, std::slice::from_ref(data), budget, |rt| {
            let target = MethodRef {
                owner: CLIENT_TOOLTIP_COMPONENT.into(),
                name: CREATE.into(),
                descriptor: CREATE_DESC.into(),
            };
            let callbacks = match callbacks {
                Ok(callbacks) => callbacks,
                // Counted with why, so a report says which callback could not run.
                Err(reason) => {
                    rt.host.unknown_operand_call(&MethodRef {
                        name: format!("{CREATE} <{reason}>"),
                        ..target.clone()
                    })?;
                    return Ok(None);
                }
            };
            let marker = crate::runtime::callback_info_marker(&target);
            rt.callback_infos.open(&marker)?;
            for plan in &callbacks {
                let info = crate::runtime::host_mixin::callback_info(&marker, &plan.callback, true);
                rt.invoke(
                    &plan.callback.owner,
                    &plan.callback.name,
                    &plan.callback.descriptor,
                    None,
                    &[data.clone(), info],
                )?;
                if rt.callback_infos.flag(&marker)? {
                    let component = rt.callback_infos.current(&marker)?;
                    if matches!(component, Value::Ref(None)) {
                        return Err(NativeError::Metadata(
                            "ClientTooltipComponent.create answered null".into(),
                        ));
                    }
                    return Ok(Some(component));
                }
            }
            rt.host.unknown_operand_call(&target)?;
            Ok(None)
        });
        if outcome.report.exact()
            && let Some(component) = &outcome.value
        {
            host.retain(component);
        }
        outcome
    }

    /// A client tooltip component's `getWidth(Font)` and `getHeight(Font)`, with the client
    /// surface's `Font` (its `width` answers from the frame's metrics; nothing else of it does,
    /// so a component that wraps text counts). Runs inside a client frame
    /// ([`StdHost::begin_client_frame`]).
    pub fn tooltip_component_size(
        &self,
        host: &mut StdHost,
        component: &Value,
        budget: &ExecutionBudget,
    ) -> ClientItemOutcome<TooltipComponentSize> {
        self.run_client_item_call(host, std::slice::from_ref(component), budget, |rt| {
            let ask = |rt: &mut Runtime<'_, StdHost>, name: &str| match rt.call_virtual(
                Site::Interface,
                CLIENT_TOOLTIP_COMPONENT,
                name,
                FONT_SIZE_DESC,
                component,
                &[tooltip_font()],
            )? {
                Value::Int(n) => Ok(n),
                _ => Err(NativeError::Metadata(format!(
                    "ClientTooltipComponent.{name} answered a non-int value"
                ))),
            };
            let width = ask(rt, "getWidth")?;
            let height = ask(rt, "getHeight")?;
            Ok(Some(TooltipComponentSize { width, height }))
        })
    }

    /// A client tooltip component's `extractImage(font, x, y, width, height, graphics)` (`width`
    /// and `height` are the whole tooltip's content box, as `ClientTooltip`'s caller passes
    /// them), drawn through the client surface's `GuiGraphicsExtractor`: the draws it recorded,
    /// in order. Runs inside a client frame ([`StdHost::begin_client_frame`]).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_tooltip_component_image(
        &self,
        host: &mut StdHost,
        component: &Value,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        budget: &ExecutionBudget,
    ) -> ClientItemOutcome<Vec<DrawOp>> {
        let before = host.client_draws().len();
        let outcome =
            self.run_client_item_call(host, std::slice::from_ref(component), budget, |rt| {
                rt.call_virtual(
                    Site::Interface,
                    CLIENT_TOOLTIP_COMPONENT,
                    "extractImage",
                    EXTRACT_IMAGE_DESC,
                    component,
                    &[
                        tooltip_font(),
                        Value::Int(x),
                        Value::Int(y),
                        Value::Int(width),
                        Value::Int(height),
                        game_object::client_value("client/gui-graphics".to_owned()),
                    ],
                )?;
                Ok(Some(()))
            });
        let draws = host
            .client_draws()
            .get(before..)
            .unwrap_or_default()
            .to_vec();
        ClientItemOutcome {
            value: outcome.value.map(|()| draws),
            report: outcome.report,
        }
    }
}
