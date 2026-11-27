//! A jar's own client item code in Sunrise (`docs/java-item-parts.md`, steps 7 and 8).
//!
//! * **Numeric item model properties.** A jar puts a property's codec into
//!   `RangeSelectItemModelProperties.ID_MAPPER` (a Mixin `TAIL` into its `bootstrap()`, which the
//!   client bootstrap pass runs at its measured turn). Each recorded id is registered with
//!   `Hooks::register_range_select_item_model_property`; Sunrise's decoder runs the jar's codec on
//!   the `range_dispatch` node, and the bound callback runs the decoded property's
//!   `get(ItemStack, ClientLevel, ItemOwner, int)` on the client runtime, over the item-part
//!   `ItemStack` façade (`item_parts::java_stack`).
//! * **Tooltip images.** `ItemPart::tooltip_image` runs the jar item's own `getTooltipImage`,
//!   maps its data with the jar's `HEAD` callbacks into `ClientTooltipComponent.create`, and
//!   measures the component (`getWidth` / `getHeight`); the answer is a [`JavaTooltipData`],
//!   whose factory (registered in `Hooks::register_client_tooltip_component_factories`) draws it
//!   with the component's own `extractImage` through the client surface the menu screens use
//!   (`menu_screen::gui_draw_with`).
//!
//! Nothing here names a mod. A call that counted leaves Sunrise's answer standing (a property
//! reads `0.0`, as an unregistered one does; a tooltip shows no image, as vanilla's
//! `Item.getTooltipImage` does) and is counted on the jar.

use std::sync::{Arc, Mutex, TryLockError};

use mod_api::{
    ClientHost, ClientTooltipComponent, DrawContext, Identifier, ItemModelPropertyInput,
    RegisterRangeSelectItemModelPropertyEvent, Tag, TooltipComponent, TooltipComponentData,
    TooltipFont,
};

use super::{LoadedJar, State};
use crate::runtime::{
    ExecutionBudget, ItemHook, ItemModelHolder, TooltipComponentSize, Value,
    client_surface::ClientSnapshot,
};
use crate::{JAR_BUDGET, LOG};

/// The client state, or `None` when it is held (a re-entrant call has no answer here).
fn try_lock(client: &Mutex<State>) -> Option<std::sync::MutexGuard<'_, State>> {
    match client.try_lock() {
        Ok(state) => Some(state),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

fn note(loaded: &mut LoadedJar, what: &str) {
    loaded.host.note_unknown_game_hook(what);
    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
}

/// A value the jar's host retains for Sunrise, released when Sunrise drops what holds it.
struct Retained {
    client: Arc<Mutex<State>>,
    jar: usize,
    value: Value,
}

impl Drop for Retained {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(mut state) = try_lock(&self.client)
                && let Some(loaded) = state.jars.get_mut(self.jar)
            {
                loaded.host.release(&self.value);
            }
        }));
    }
}

/// Register every numeric item model property the resident jars recorded, each under its own id
/// (`Hooks::register_range_select_item_model_property`). Two jars that put one id: the
/// `LateBoundIdMapper` keeps the last `put`, in the order the jars' bootstraps ran; Sunrise's
/// registration is last-writer-wins too.
pub(crate) fn register_range_select_properties(
    client: &Arc<Mutex<State>>,
    event: &mut RegisterRangeSelectItemModelPropertyEvent<'_>,
) {
    let Some(mut state) = try_lock(client) else {
        log::warn!("{LOG}: client item model properties asked re-entrantly");
        return;
    };
    let mut registered = 0usize;
    for (index, loaded) in state.jars.iter_mut().enumerate() {
        for (id, codec) in loaded.host.client_range_select_properties() {
            let Ok(identifier) = Identifier::parse(&id) else {
                note(loaded, "range_select_property:id");
                continue;
            };
            let client = Arc::clone(client);
            event.register(identifier, move |config: &Tag| {
                // The jar's codec refusing the node rejects the item definition, as the game's
                // decoding does; a decode that counted reads `0.0`.
                let property = decode_property(&client, index, &codec, &id, config)?;
                Ok(
                    move |host: &dyn ClientHost, input: &ItemModelPropertyInput<'_>| {
                        property
                            .as_ref()
                            .map_or(0.0, |property| property_value(host, property, input))
                    },
                )
            });
            registered += 1;
        }
    }
    if registered > 0 {
        log::info!("{LOG}: registered {registered} Java item model propert(y/ies)");
    }
}

/// One decoded property of one `range_dispatch` node.
struct BoundProperty {
    id: String,
    held: Retained,
}

/// The jar's codec on the node (its JSON object, which Sunrise hands as a `Tag`). `Ok(None)` when
/// it counted: the property then reads `0.0`, as an unregistered one does. `Err` when the codec
/// refused the node: the item definition is rejected.
fn decode_property(
    client: &Arc<Mutex<State>>,
    jar: usize,
    codec: &Value,
    id: &str,
    config: &Tag,
) -> Result<Option<BoundProperty>, String> {
    let Some(mut state) = try_lock(client) else {
        return Ok(None);
    };
    let Some(loaded) = state.jars.get_mut(jar) else {
        return Ok(None);
    };
    // A `Tag` holds no JSON booleans (`Byte`) apart from numbers: a node with a byte decodes
    // only when its codec reads no boolean, which can't be told here, so it counts.
    let node = super::nbt_to_json_value(config, false).filter(|_| !has_byte(config));
    let Some(node) = node else {
        note(loaded, &format!("range_select_property:{id}:node"));
        return Ok(None);
    };
    let outcome = loaded.jar.decode_range_select_property(
        &mut loaded.host,
        codec,
        &node,
        &ExecutionBudget::within(JAR_BUDGET),
    );
    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
    let Some(property) = outcome.exact() else {
        return Ok(None);
    };
    Ok(Some(BoundProperty {
        id: id.to_owned(),
        held: Retained {
            client: Arc::clone(client),
            jar,
            value: property?,
        },
    }))
}

fn has_byte(tag: &Tag) -> bool {
    match tag {
        Tag::Byte(_) => true,
        Tag::List(items) => items.iter().any(has_byte),
        Tag::Compound(fields) => fields.values().any(has_byte),
        _ => false,
    }
}

/// The property's `get` for the stack being drawn.
fn property_value(
    host: &dyn ClientHost,
    property: &BoundProperty,
    input: &ItemModelPropertyInput<'_>,
) -> f32 {
    let Some(mut state) = try_lock(&property.held.client) else {
        return 0.0;
    };
    let Some(loaded) = state.jars.get_mut(property.held.jar) else {
        return 0.0;
    };
    // A jar item's stack is its `Item` object's; any other item's is the game's.
    let item = input.stack.item();
    let item_override = item.id(host).and_then(|id| {
        loaded
            .host
            .observed_registry_entry("minecraft:item", id)
            .map(|entry| entry.value.clone())
    });
    let jar_types = crate::item_parts::persistent_jar_types(&loaded.host);
    let context = input.context;
    let jar = &loaded.jar;
    // A draw runs every frame: what it makes is garbage once it has its number, and the stack
    // is the one the last draw of this game stack used while nothing changed it.
    let outcome = loaded.host.with_transient_rows(|jar_host| {
        let stack = crate::item_parts::java_draw_stack(
            host,
            jar,
            jar_host,
            item_override,
            input.stack,
            &jar_types,
        )?;
        Some(jar.range_select_property_value(
            jar_host,
            &property.held.value,
            &stack,
            ItemModelHolder {
                level: context.level.is_some(),
                owner: context.owner.is_some(),
            },
            context.seed,
            &ExecutionBudget::within(JAR_BUDGET),
        ))
    });
    let Some(outcome) = outcome else {
        note(
            loaded,
            &format!("range_select_property:{}:stack unreadable", property.id),
        );
        return 0.0;
    };
    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
    outcome.exact().unwrap_or(0.0)
}

/// A jar's tooltip image: its client component on the resident jar, measured when it was made.
pub(crate) struct JavaTooltipData {
    held: Arc<Retained>,
    size: TooltipComponentSize,
}

impl TooltipComponentData for JavaTooltipData {}

impl State {
    /// `getTooltipImage` of the jar item `item_id` on `stack`, mapped to the jar's client
    /// component and measured (`ItemPart::tooltip_image`). `None` (counted when a call counted)
    /// leaves Sunrise's vanilla answer: no image.
    pub(crate) fn java_tooltip_image(
        &mut self,
        client: &Arc<Mutex<State>>,
        host: &dyn ClientHost,
        item_id: &str,
        stack: &mod_api::ItemStack,
    ) -> Option<TooltipComponent> {
        let (index, loaded) = self.jars.iter_mut().enumerate().find(|(_, loaded)| {
            loaded
                .host
                .observed_registry_entry("minecraft:item", item_id)
                .is_some()
        })?;
        let item = loaded
            .host
            .observed_registry_entry("minecraft:item", item_id)?
            .value
            .clone();
        let owner = loaded
            .jar
            .item_overrides(&item, &loaded.host)
            .owners
            .get(&ItemHook::TooltipImage)?
            .clone();
        let jar_types = crate::item_parts::persistent_jar_types(&loaded.host);
        let jar = &loaded.jar;
        // Asked on every frame the tooltip shows: what the calls make is garbage once the
        // component is measured (the component itself is retained), and the stack is the one
        // the last draw of this game stack used while nothing changed it.
        let made = loaded.host.with_transient_rows(|jar_host| {
            let Some(value) = crate::item_parts::java_draw_stack(
                host,
                jar,
                jar_host,
                Some(item.clone()),
                stack,
                &jar_types,
            ) else {
                return TooltipMade::Unreadable;
            };
            let budget = ExecutionBudget::within(JAR_BUDGET);
            let image = jar
                .item_tooltip_image(jar_host, &item, &owner, &value, &budget)
                .exact();
            let data = match image {
                Some(Some(data)) => data,
                // `Optional.empty()`: no image, exactly.
                Some(None) => return TooltipMade::Empty,
                None => return TooltipMade::ImageCounted,
            };
            let component = jar
                .client_tooltip_component(jar_host, &data, &budget)
                .exact();
            jar_host.release(&data);
            let Some(component) = component else {
                return TooltipMade::Empty;
            };
            jar_host.begin_client_frame(tooltip_frame(), &[]);
            let size = jar
                .tooltip_component_size(jar_host, &component, &budget)
                .exact();
            // A draw that would count paints nothing: Sunrise would keep an empty box where
            // vanilla draws the image. The image is drawn once here, in its own box, to know.
            let drawable = size.is_some_and(|size| {
                jar.draw_tooltip_component_image(
                    jar_host,
                    &component,
                    0,
                    0,
                    size.width,
                    size.height,
                    &budget,
                )
                .exact()
                .is_some_and(|ops| {
                    ops.iter()
                        .all(|op| super::menu_screen::gui_draw_with(op, &|_| None).is_ok())
                })
            });
            jar_host.end_client_frame();
            TooltipMade::Measured(component, size.filter(|_| drawable))
        });
        let (component, size) = match made {
            TooltipMade::Unreadable => {
                note(loaded, "tooltip_image:stack unreadable");
                return None;
            }
            TooltipMade::Empty => return None,
            TooltipMade::ImageCounted => {
                loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                return None;
            }
            TooltipMade::Measured(component, size) => (component, size),
        };
        loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
        // A component that can't measure or draw itself shows nothing rather than a guessed box.
        let Some(size) = size else {
            loaded.host.release(&component);
            return None;
        };
        let held = Arc::new(Retained {
            client: Arc::clone(client),
            jar: index,
            value: component,
        });
        Some(TooltipComponent::custom(JavaTooltipData { held, size }))
    }

    /// The component's own `extractImage` at `(x, y)` in a `width` x `height` tooltip box, its
    /// draws painted on `draw`. A draw that counted paints nothing of that call.
    fn draw_java_tooltip_image(
        &mut self,
        data: &JavaTooltipData,
        [x, y, width, height]: [i32; 4],
        draw: &mut dyn DrawContext,
    ) {
        let Some(loaded) = self.jars.get_mut(data.held.jar) else {
            return;
        };
        let jar = &loaded.jar;
        // Drawn every frame the tooltip shows: what the draw makes is garbage once it painted.
        let outcome = loaded.host.with_transient_rows(|jar_host| {
            jar_host.begin_client_frame(tooltip_frame(), &[]);
            let outcome = jar.draw_tooltip_component_image(
                jar_host,
                &data.held.value,
                x,
                y,
                width,
                height,
                &ExecutionBudget::within(JAR_BUDGET),
            );
            jar_host.end_client_frame();
            outcome
        });
        let Some(ops) = outcome.exact() else {
            loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
            return;
        };
        let mut draws = Vec::with_capacity(ops.len());
        for op in &ops {
            // `draw_image` has no host to form a text draw's text with: such a draw counts.
            match super::menu_screen::gui_draw_with(op, &|_| None) {
                Ok(gui) => draws.push(gui),
                Err(reason) => {
                    note(loaded, &format!("tooltip_image:undrawable_{reason}"));
                    return;
                }
            }
        }
        if !draws.is_empty() && draw.gui_paint(&draws).is_err() {
            note(loaded, "tooltip_image:gui_paint_refused");
        }
    }
}

/// How making a tooltip image ended ([`State::java_tooltip_image`]).
enum TooltipMade {
    /// The game stack has no Java stack (counted by the caller).
    Unreadable,
    /// No image: `Optional.empty()`, or a component `create` that counted.
    Empty,
    /// `getTooltipImage` counted.
    ImageCounted,
    /// The component (retained), and its size when it measured and drew exactly.
    Measured(Value, Option<TooltipComponentSize>),
}

/// A frame for a tooltip component's Java code: no player, no level, nothing a screen reads.
fn tooltip_frame() -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        screen_scope: true,
        ..ClientSnapshot::default()
    })
}

/// The client component Sunrise prepares from a [`JavaTooltipData`].
pub(crate) struct JavaTooltip {
    held: Arc<Retained>,
    size: TooltipComponentSize,
}

impl ClientTooltipComponent for JavaTooltip {
    fn height(&self, _font: &dyn TooltipFont) -> i32 {
        self.size.height
    }

    fn width(&self, _font: &dyn TooltipFont) -> i32 {
        self.size.width
    }

    fn draw_image(
        &self,
        _font: &dyn TooltipFont,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        draw: &mut dyn DrawContext,
    ) {
        let Some(mut state) = try_lock(&self.held.client) else {
            return;
        };
        let data = JavaTooltipData {
            held: Arc::clone(&self.held),
            size: self.size,
        };
        state.draw_java_tooltip_image(&data, [x, y, width, height], draw);
    }
}

/// `Hooks::register_client_tooltip_component_factories`: the factory of [`JavaTooltipData`].
pub(crate) fn register_tooltip_factory(
    factories: &mut dyn mod_api::ClientTooltipComponentFactoriesRegistrar,
) {
    factories.register::<JavaTooltipData, JavaTooltip>(|data| JavaTooltip {
        held: Arc::clone(&data.held),
        size: data.size,
    });
}
