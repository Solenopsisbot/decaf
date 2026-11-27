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
//! * **The tooltip's font.** Sunrise exposes no glyph table, so the runtime's glyph model
//!   (`client_font.rs`) has nothing to read; the game's own font answers instead.
//!   `ClientTextClientHost::text_width` and `text_split` are vanilla's `Font.width` and
//!   `Font.split` with the active font and language. The component runs once, each text it
//!   measures or wraps is asked of the game ([`answer_font_questions`]), and it runs again with
//!   those answers ([`FontAnswers`]), until it asks nothing new. Every run but the last is a
//!   probe whose result is thrown away and whose counted calls are rewound
//!   (`StdHost::rewind_unknown_calls`). Vanilla runs `getHeight` and `extractImage` every frame,
//!   so running them again changes nothing.
//!
//! Nothing here names a mod. A call that counted leaves Sunrise's answer standing (a property
//! reads `0.0`, as an unregistered one does; a tooltip shows no image, as vanilla's
//! `Item.getTooltipImage` does) and is counted on the jar.

use std::sync::{Arc, Mutex, TryLockError};

use std::collections::BTreeMap;

use mod_api::{
    ClientHost, ClientTooltipComponent, DrawContext, GuiDraw, Identifier, ItemModelPropertyInput,
    RegisterRangeSelectItemModelPropertyEvent, Tag, TooltipComponent, TooltipComponentData,
    TooltipFont,
};

use super::{LoadedJar, State};
use crate::runtime::{
    ExecutionBudget, ItemHook, ItemModelHolder, StackComponents, TooltipComponentSize, Value,
    client_surface::{ClientSnapshot, ClientText, DrawOp, FontAnswers, FontQuestion, TextMetrics},
};

/// Runs a tooltip component's measure-and-draw may take, each answering the font questions the
/// one before it asked: a component asks one text at a time (the first unanswered one stops the
/// run), so a description, its lines and a label take a few. The last run's counts stand.
const FONT_PASSES: usize = 8;

/// What the tooltip's Java draws reach Sunrise with, formed once with the client host (which
/// `draw_image` has none of): each text the game's font answered by its exact `Text`, and each
/// stack the component draws as Sunrise's.
#[derive(Default)]
struct TooltipForms {
    answers: FontAnswers,
    texts: BTreeMap<ClientText, mod_api::Text>,
    stacks: Vec<(StackComponents, mod_api::ItemStack)>,
}
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
    let Some(stack) = crate::item_parts::java_stack(
        host,
        &loaded.jar,
        &mut loaded.host,
        item_override,
        input.stack,
        &jar_types,
    ) else {
        note(
            loaded,
            &format!("range_select_property:{}:stack unreadable", property.id),
        );
        return 0.0;
    };
    let context = input.context;
    let outcome = loaded.jar.range_select_property_value(
        &mut loaded.host,
        &property.held.value,
        &stack,
        ItemModelHolder {
            level: context.level.is_some(),
            owner: context.owner.is_some(),
        },
        context.seed,
        &ExecutionBudget::within(JAR_BUDGET),
    );
    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
    outcome.exact().unwrap_or(0.0)
}

/// A jar's tooltip image: its client component on the resident jar, measured when it was made,
/// with the game's font answers and the forms its draws reach Sunrise with.
pub(crate) struct JavaTooltipData {
    held: Arc<Retained>,
    size: TooltipComponentSize,
    forms: Arc<TooltipForms>,
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
        let Some(value) = crate::item_parts::java_stack(
            host,
            &loaded.jar,
            &mut loaded.host,
            Some(item.clone()),
            stack,
            &jar_types,
        ) else {
            note(loaded, "tooltip_image:stack unreadable");
            return None;
        };
        let budget = ExecutionBudget::within(JAR_BUDGET);
        let image = loaded
            .jar
            .item_tooltip_image(&mut loaded.host, &item, &owner, &value, &budget)
            .exact();
        let data = match image {
            Some(Some(data)) => data,
            // `Optional.empty()`: no image, exactly.
            Some(None) => return None,
            None => {
                loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                return None;
            }
        };
        let component = loaded
            .jar
            .client_tooltip_component(&mut loaded.host, &data, &budget)
            .exact();
        loaded.host.release(&data);
        let component = component?;
        // A draw that would count paints nothing: Sunrise would keep an empty box where vanilla
        // draws the image. The image is drawn once here, in its own box, to know, and the font
        // questions it asks are answered by the game's font before the run that counts.
        let mut forms = TooltipForms::default();
        let mut measured = None;
        for pass in 0..FONT_PASSES {
            let mark = loaded.host.unknown_calls_mark();
            loaded
                .host
                .begin_client_frame(tooltip_frame(&forms.answers), &[]);
            let size = loaded
                .jar
                .tooltip_component_size(&mut loaded.host, &component, &budget)
                .exact();
            let ops = size.and_then(|size| {
                loaded
                    .jar
                    .draw_tooltip_component_image(
                        &mut loaded.host,
                        &component,
                        0,
                        0,
                        size.width,
                        size.height,
                        &budget,
                    )
                    .exact()
            });
            let questions = loaded.host.end_client_frame().font_questions;
            if let (Some(size), Some(ops)) = (size, &ops) {
                measured = Some((size, ops.clone()));
                break;
            }
            let last = pass + 1 == FONT_PASSES;
            if last || questions.is_empty() || !answer_font_questions(host, &questions, &mut forms)
            {
                break;
            }
            // The probe's result is thrown away: only the run whose answer is used counts.
            loaded.host.rewind_unknown_calls(mark);
        }
        let drawable = measured.and_then(|(size, ops)| {
            let Some(stacks) = tooltip_stacks(host, &ops) else {
                note(loaded, "tooltip_image:undrawable_stack_form");
                return None;
            };
            forms.stacks = stacks;
            let paint = paint_plan(&ops, &forms);
            if let Err(reason) = paint {
                note(loaded, &format!("tooltip_image:undrawable_{reason}"));
                return None;
            }
            Some(size)
        });
        loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
        // A component that can't measure or draw itself shows nothing rather than a guessed box.
        let Some(size) = drawable else {
            loaded.host.release(&component);
            return None;
        };
        let held = Arc::new(Retained {
            client: Arc::clone(client),
            jar: index,
            value: component,
        });
        Some(TooltipComponent::custom(JavaTooltipData {
            held,
            size,
            forms: Arc::new(forms),
        }))
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
        loaded
            .host
            .begin_client_frame(tooltip_frame(&data.forms.answers), &[]);
        let outcome = loaded.jar.draw_tooltip_component_image(
            &mut loaded.host,
            &data.held.value,
            x,
            y,
            width,
            height,
            &ExecutionBudget::within(JAR_BUDGET),
        );
        loaded.host.end_client_frame();
        let Some(ops) = outcome.exact() else {
            loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
            return;
        };
        // A text or stack the forms made at `tooltip_image` don't hold (the component drew
        // something else this time) counts, and nothing of the call is painted.
        let plan = match paint_plan(&ops, &data.forms) {
            Ok(plan) => plan,
            Err(reason) => {
                note(loaded, &format!("tooltip_image:undrawable_{reason}"));
                return;
            }
        };
        for step in plan {
            match step {
                Paint::Gui(draws) => {
                    if draw.gui_paint(&draws).is_err() {
                        note(loaded, "tooltip_image:gui_paint_refused");
                        return;
                    }
                }
                Paint::Stack(stack, at) => {
                    if !draw.item_stack(stack, at, 16.0) {
                        note(loaded, "tooltip_image:item_stack_refused");
                        return;
                    }
                }
            }
        }
    }
}

/// A frame for a tooltip component's Java code: no player, no level, nothing a screen reads; its
/// font answers from the game's own answers (`answers`), Sunrise having no glyph table.
fn tooltip_frame(answers: &FontAnswers) -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        screen_scope: true,
        metrics: Arc::new(TextMetrics {
            answers: answers.clone(),
            ..TextMetrics::default()
        }),
        ..ClientSnapshot::default()
    })
}

/// Ask the game's font each question (`Font.width` / `Font.split` of an exact text) and keep the
/// answers. `false` when a text has no exact `Text` here (a literal with `§` codes, JSON Sunrise
/// doesn't read): the run that counted stands.
fn answer_font_questions(
    host: &dyn ClientHost,
    questions: &[FontQuestion],
    forms: &mut TooltipForms,
) -> bool {
    for question in questions {
        let asked = match question {
            FontQuestion::Width(text) | FontQuestion::Split(text, _) => text,
        };
        let Some(text) = forms
            .texts
            .get(asked)
            .cloned()
            .or_else(|| super::menu_screen::api_text(host, asked))
        else {
            return false;
        };
        match question {
            FontQuestion::Width(asked) => {
                forms
                    .answers
                    .widths
                    .insert(asked.clone(), host.text_width(&text));
            }
            FontQuestion::Split(asked, width) => {
                let mut lines = Vec::new();
                for line in host.text_split(&text, *width) {
                    let Some(key) = line_text(host, &line) else {
                        return false;
                    };
                    forms.texts.insert(key.clone(), line);
                    lines.push(key);
                }
                forms.answers.splits.insert((asked.clone(), *width), lines);
            }
        }
        forms.texts.insert(asked.clone(), text);
    }
    true
}

/// A split line as the runtime's text: a plain literal without `§` codes as itself (what the
/// glyph model's lines are), anything styled as its component JSON (the codec's own NBT form).
fn line_text(host: &dyn ClientHost, line: &mod_api::Text) -> Option<ClientText> {
    if let mod_api::text::TextContent::Literal(text) = &line.content
        && *line == mod_api::Text::literal(text.clone())
        && !text.contains('§')
    {
        return Some(ClientText::Literal(text.clone()));
    }
    let json = super::nbt_component_to_json(&host.text_to_nbt(line).ok()?)?;
    Some(ClientText::Json(json))
}

/// Each stack the component draws ([`DrawOp::Stack`]) as Sunrise's, formed with the client host;
/// `None` when one can't be (`client::api_stack_from_components`).
fn tooltip_stacks(
    host: &dyn ClientHost,
    ops: &[DrawOp],
) -> Option<Vec<(StackComponents, mod_api::ItemStack)>> {
    let mut stacks: Vec<(StackComponents, mod_api::ItemStack)> = Vec::new();
    for op in ops {
        if let DrawOp::Stack { stack, .. } = op
            && !stacks.iter().any(|(known, _)| known == stack)
        {
            let api = super::api_stack_from_components(host, stack.clone())?;
            stacks.push((stack.clone(), api));
        }
    }
    Some(stacks)
}

/// One step of painting a tooltip image: a run of GUI ops, or a stack.
enum Paint<'a> {
    Gui(Vec<GuiDraw>),
    Stack(&'a mod_api::ItemStack, [f32; 2]),
}

/// The tooltip's draws as Sunrise paints them, in order, or why it can't.
///
/// A text draws with the `Text` the game's font answered for it, else a plain literal or
/// translation as itself. A stack is vanilla's `item(stack, x, y, seed)` followed at once by
/// `itemDecorations(font, stack, x, y)` of that stack at that place: Sunrise's
/// `DrawContext::item_stack` draws the model and the decorations but the count, which is exact
/// only when there is no count to draw (a count of 1). Vanilla draws the count text and then the
/// durability bar and the cooldown over it, which `item_stack` and a text after it can't order
/// (the SDK has no draw of vanilla's whole `itemDecorations`): a larger count, a model draw alone,
/// a scaled one, or decorations with their own count text, refuse.
fn paint_plan<'a>(ops: &[DrawOp], forms: &'a TooltipForms) -> Result<Vec<Paint<'a>>, &'static str> {
    let text_of = |text: &ClientText| match forms.texts.get(text) {
        Some(text) => Some(text.clone()),
        None => match text {
            ClientText::Literal(literal) if !literal.contains('§') => {
                Some(mod_api::Text::literal(literal.clone()))
            }
            ClientText::Translatable(key) => {
                Some(mod_api::Text::translate(key.clone(), Vec::new()))
            }
            _ => None,
        },
    };
    let mut plan = Vec::new();
    let mut gui = Vec::new();
    let mut ops = ops.iter().peekable();
    while let Some(op) = ops.next() {
        let DrawOp::Stack {
            stack,
            at,
            scale,
            model: true,
            ..
        } = op
        else {
            gui.push(super::menu_screen::gui_draw_with(op, &text_of)?);
            continue;
        };
        let decorated = matches!(ops.peek(), Some(DrawOp::Stack {
            stack: next, at: next_at, scale: next_scale, model: false, decorations: Some(None), ..
        }) if next == stack && next_at == at && next_scale == scale);
        if !decorated || *scale != [1.0, 1.0] {
            return Err("stack_draw");
        }
        if stack.count != 1 {
            return Err("stack_count");
        }
        ops.next();
        let api = forms
            .stacks
            .iter()
            .find(|(known, _)| known == stack)
            .map(|(_, api)| api)
            .ok_or("stack_form")?;
        if !gui.is_empty() {
            plan.push(Paint::Gui(std::mem::take(&mut gui)));
        }
        plan.push(Paint::Stack(api, *at));
    }
    if !gui.is_empty() {
        plan.push(Paint::Gui(gui));
    }
    Ok(plan)
}

/// The client component Sunrise prepares from a [`JavaTooltipData`].
pub(crate) struct JavaTooltip {
    held: Arc<Retained>,
    size: TooltipComponentSize,
    forms: Arc<TooltipForms>,
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
            forms: Arc::clone(&self.forms),
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
        forms: Arc::clone(&data.forms),
    });
}
