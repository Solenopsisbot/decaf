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
//!   those answers ([`FontAnswers`]), until it asks nothing new
//!   (`NativeJar::tooltip_image_with_font`, which rewinds a discarded run's counts only when all
//!   it counted was the font's refusals).
//!
//! Nothing here names a mod. A call that counted leaves Sunrise's answer standing (a property
//! reads `0.0`, as an unregistered one does; a tooltip shows no image, as vanilla's
//! `Item.getTooltipImage` does) and is counted on the jar.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, TryLockError};

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
/// one before it asked. A run stops at its first unanswered text (the refusal ends the Java
/// call), so a component asks one text per run: a pouch's description and label take three runs,
/// a component that measures each of a dozen lines about fifteen. The last run's counts stand.
const FONT_RUNS: usize = 16;

/// What the tooltip's Java draws reach Sunrise with, formed once with the client host (which
/// `draw_image` has none of): each text the game's font answered by its exact `Text`, each stack
/// the component draws as Sunrise's, and the width of each stack's count text
/// (`Font.width(String)`, which vanilla's `itemCount` places it by).
#[derive(Default)]
struct TooltipForms {
    answers: FontAnswers,
    texts: BTreeMap<ClientText, mod_api::Text>,
    stacks: Vec<(StackComponents, mod_api::ItemStack)>,
    count_widths: BTreeMap<String, i32>,
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
        let mut texts = BTreeMap::new();
        let image = loaded.jar.tooltip_image_with_font(
            &mut loaded.host,
            &component,
            &tooltip_frame,
            FONT_RUNS,
            &mut |questions, answers| answer_font_questions(host, questions, answers, &mut texts),
            &budget,
        );
        let drawable = image.and_then(|image| {
            let Some(forms) = tooltip_forms(host, &image.draws, image.answers, texts) else {
                note(loaded, "tooltip_image:undrawable_stack_form");
                return None;
            };
            if let Err(reason) = paint_plan(&image.draws, &forms) {
                note(loaded, &format!("tooltip_image:undrawable_{reason}"));
                return None;
            }
            Some((image.size, forms))
        });
        loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
        // A component that can't measure or draw itself shows nothing rather than a guessed box.
        let Some((size, forms)) = drawable else {
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
                    if !draw.item_stack(stack, at, ITEM_SIZE) {
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
/// answers, and each text's exact `Text` in `texts` (a split's lines among them, which a later
/// question or draw names). `false` when a text has no exact `Text` here (a literal with `§`
/// codes, JSON Sunrise doesn't read): the run that counted stands.
pub(crate) fn answer_font_questions(
    host: &dyn ClientHost,
    questions: &[FontQuestion],
    answers: &mut FontAnswers,
    texts: &mut BTreeMap<ClientText, mod_api::Text>,
) -> bool {
    for question in questions {
        let asked = match question {
            FontQuestion::Width(text) | FontQuestion::Split(text, _) => text,
        };
        let Some(text) = texts
            .get(asked)
            .cloned()
            .or_else(|| super::menu_screen::api_text(host, asked))
        else {
            return false;
        };
        match question {
            FontQuestion::Width(asked) => {
                answers.widths.insert(asked.clone(), host.text_width(&text));
            }
            FontQuestion::Split(asked, width) => {
                let mut lines = Vec::new();
                for line in host.text_split(&text, *width) {
                    let Some(key) = line_text(host, &line) else {
                        return false;
                    };
                    texts.insert(key.clone(), line);
                    lines.push(key);
                }
                answers.splits.insert((asked.clone(), *width), lines);
            }
        }
        texts.insert(asked.clone(), text);
    }
    true
}

/// A split line as the runtime's text: a plain literal without `§` codes as itself (what the
/// glyph model's lines are), anything styled as its component JSON (the codec's own NBT form).
pub(crate) fn line_text(host: &dyn ClientHost, line: &mod_api::Text) -> Option<ClientText> {
    if let mod_api::text::TextContent::Literal(text) = &line.content
        && *line == mod_api::Text::literal(text.clone())
        && !text.contains('§')
    {
        return Some(ClientText::Literal(text.clone()));
    }
    let json = super::nbt_component_to_json(&host.text_to_nbt(line).ok()?)?;
    Some(ClientText::Json(json))
}

/// The forms the image's draws reach Sunrise with ([`TooltipForms`]), made with the client host:
/// each stack it draws ([`DrawOp::Stack`]) as Sunrise's (`client::api_stack_from_components`), and
/// each count text's width, the game's `Font.width` of it (`ClientTextClientHost::text_width` of
/// the literal: a string with no `§` code measures as its literal component does). `None` when a
/// stack or a count text can't be formed.
fn tooltip_forms(
    host: &dyn ClientHost,
    draws: &[DrawOp],
    answers: FontAnswers,
    texts: BTreeMap<ClientText, mod_api::Text>,
) -> Option<TooltipForms> {
    let mut forms = TooltipForms {
        answers,
        texts,
        ..TooltipForms::default()
    };
    for op in draws {
        let DrawOp::Stack {
            stack, decorations, ..
        } = op
        else {
            continue;
        };
        if !forms.stacks.iter().any(|(known, _)| known == stack) {
            let api = super::api_stack_from_components(host, stack.clone())?;
            forms.stacks.push((stack.clone(), api));
        }
        if let Some(text) = decorations
            .as_ref()
            .and_then(|text| count_text(stack, text))
        {
            if text.contains('§') {
                return None;
            }
            let width = host.text_width(&mod_api::Text::literal(text.clone()));
            forms.count_widths.insert(text, width);
        }
    }
    Some(forms)
}

/// 26.3 `GuiGraphicsExtractor.itemCount`'s text (offsets 0-30): the given text, else the stack's
/// count when it isn't 1; `None` draws nothing.
fn count_text(stack: &StackComponents, given: &Option<String>) -> Option<String> {
    match given {
        Some(text) => Some(text.clone()),
        None if stack.count != 1 => Some(stack.count.to_string()),
        None => None,
    }
}

/// A slot's item and the GUI pixels it is drawn in.
const ITEM_SIZE: f32 = 16.0;

/// One step of painting a tooltip image: a run of GUI ops, or a stack.
enum Paint<'a> {
    Gui(Vec<GuiDraw>),
    Stack(&'a mod_api::ItemStack, [f32; 2]),
}

/// The tooltip's draws as Sunrise paints them, in order, or why it can't.
///
/// A text draws with the `Text` the game's font answered for it, else a plain literal or
/// translation as itself. A stack is vanilla's `item(stack, x, y, seed)` followed at once by
/// `itemDecorations(font, stack, x, y[, text])` of that stack at that place. 26.3's
/// `itemDecorations` (offsets 16-40) draws `itemBar`, then `itemCooldown`, then `itemCount`, and
/// Sunrise's `DrawContext::item_stack` draws the model, the bar and the cooldown (with an empty
/// count, `native_item_stack` at the pin) into the same batch the GUI ops go to, in order. So the
/// pair is `item_stack`, then `itemCount`'s text as vanilla places it (offsets 32-58):
/// `text(font, s, x + 19 - 2 - font.width(s), y + 6 + 3, -1, true)`, in the pose
/// `itemDecorations` pushed and left unchanged. A model draw alone, decorations alone, a scaled
/// stack or one off the pixel grid refuse. The draw names no holder and no seed: Sunrise draws the
/// stack as its GUI does, which differs only for an item model that reads its holder or seed.
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
        let Some(DrawOp::Stack {
            stack: next,
            at: next_at,
            scale: next_scale,
            model: false,
            decorations: Some(given),
            ..
        }) = ops.peek()
        else {
            return Err("stack_draw");
        };
        if next != stack || next_at != at || next_scale != scale || *scale != [1.0, 1.0] {
            return Err("stack_draw");
        }
        let [x, y] = *at;
        if !(x.fract() == 0.0 && y.fract() == 0.0 && x.abs() < 1e6 && y.abs() < 1e6) {
            return Err("stack_off_pixel");
        }
        let count = count_text(stack, given);
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
        if let Some(count) = count {
            let width = *forms.count_widths.get(&count).ok_or("stack_count_form")?;
            let (x, y) = (x as i32, y as i32);
            gui.push(GuiDraw::Text {
                text: mod_api::Text::literal(count),
                x: x + 19 - 2 - width,
                y: y + 6 + 3,
                color: 0xffff_ffff,
                shadow: true,
            });
        }
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

#[cfg(test)]
mod tests {
    //! The Sunrise half of a tooltip image: asking the game's font (`answer_font_questions`,
    //! `line_text`) and painting a slot (`paint_plan`). The runs between the questions are the
    //! runtime's (`NativeJar::tooltip_image_with_font`, `fixture_client_font.rs` and
    //! `fixture_runes_client_items.rs` on the oracle).

    use super::*;
    use mod_api::__private::game::BackendRefOf;
    use mod_api::{Backend, BackendRef, Host, ItemStack, Text};

    /// The oracle's advance table (`FontOracle.advance` in `fixtures/client_font`).
    fn advance(c: char) -> Option<i32> {
        Some(match c {
            ' ' => 4,
            c if "i!.,:;'|".contains(c) => 2,
            'l' => 3,
            c if "tI[]".contains(c) => 4,
            c if c.is_ascii_graphic() => 6,
            _ => return None,
        })
    }

    /// A client whose game font is the oracle's table over plain literals and two translations,
    /// splitting at spaces only (enough for these texts; the split itself is the runtime's and the
    /// game's, tested on the oracle elsewhere). Its items are the given ids.
    struct GameFont {
        fake: mod_api::testing::FakeHost,
    }

    impl GameFont {
        fn new(items: &[&str]) -> Self {
            let mut fake = mod_api::testing::FakeHost::new();
            fake.registries
                .resolve_items(
                    items
                        .iter()
                        .map(|id| Identifier::parse(id).unwrap())
                        .collect::<Vec<_>>(),
                )
                .expect("resolve test items");
            Self { fake }
        }

        fn plain(text: &Text) -> String {
            match &text.content {
                mod_api::text::TextContent::Literal(text) => text.clone(),
                mod_api::text::TextContent::Translatable { key, .. } => match key.as_str() {
                    "demo.description" => "Can hold mixed stacks".to_owned(),
                    "demo.title" => "Full".to_owned(),
                    other => panic!("no translation {other}"),
                },
                other => panic!("no plain text {other:?}"),
            }
        }

        fn stack(&self, id: &str, count: u32) -> ItemStack {
            let key = mod_api::prelude::ItemsHost::item_key(self, &Identifier::parse(id).unwrap())
                .unwrap();
            ItemStack::new(key, count).unwrap()
        }
    }

    impl Host for GameFont {
        fn backend(&self) -> Option<BackendRef<'_>> {
            Some(BackendRef::of(self))
        }
    }
    impl Backend for GameFont {}
    mod_api::impl_backend_areas!(GameFont);
    mod_api::impl_host_areas!(GameFont, except(items));
    impl ClientHost for GameFont {}
    mod_api::impl_client_host_areas!(GameFont, except(client_text));

    impl mod_api::prelude::ItemsHost for GameFont {
        fn item_key(&self, id: &Identifier) -> Option<mod_api::ItemKey> {
            mod_api::prelude::ItemsHost::item_key(&self.fake, id)
        }
    }

    impl mod_api::prelude::ClientTextClientHost for GameFont {
        fn text_visual_order(&self, _text: &Text) -> Vec<mod_api::VisualTextCharacter> {
            unimplemented!("not asked here")
        }
        fn text_width(&self, text: &Text) -> i32 {
            Self::plain(text).chars().map(|c| advance(c).unwrap()).sum()
        }
        fn text_split(&self, text: &Text, width: i32) -> Vec<Text> {
            let mut lines: Vec<String> = Vec::new();
            for word in Self::plain(text).split(' ') {
                let joined = lines.last().map(|line| format!("{line} {word}"));
                match joined {
                    Some(joined)
                        if joined.chars().map(|c| advance(c).unwrap()).sum::<i32>() <= width =>
                    {
                        *lines.last_mut().unwrap() = joined;
                    }
                    _ => lines.push(word.to_owned()),
                }
            }
            lines.into_iter().map(Text::literal).collect()
        }
    }

    /// Each question is answered with the game font's own answer, a split's lines kept by their
    /// plain literal (what a later width question and draw name), and every text's `Text` kept.
    #[test]
    fn questions_are_answered_by_the_game_font() {
        let host = GameFont::new(&[]);
        let description = ClientText::Translatable("demo.description".into());
        let title = ClientText::Translatable("demo.title".into());
        let mut answers = FontAnswers::default();
        let mut texts = BTreeMap::new();
        assert!(answer_font_questions(
            &host,
            &[
                FontQuestion::Split(description.clone(), 60),
                FontQuestion::Width(title.clone()),
            ],
            &mut answers,
            &mut texts,
        ));
        let lines = vec![
            ClientText::Literal("Can hold".into()),
            ClientText::Literal("mixed".into()),
            ClientText::Literal("stacks".into()),
        ];
        assert_eq!(answers.splits[&(description.clone(), 60)], lines);
        // "Full": 6 + 6 + 3 + 3.
        assert_eq!(answers.widths[&title], 18);
        assert_eq!(texts[&lines[0]], Text::literal("Can hold"));
        assert_eq!(texts[&title], Text::translate("demo.title", Vec::new()));
        // A line asked next is measured through the `Text` its split answered.
        assert!(answer_font_questions(
            &host,
            &[FontQuestion::Width(lines[0].clone())],
            &mut answers,
            &mut texts,
        ));
        // "Can hold": 6 + 6 + 6, 4, 6 + 6 + 3 + 6.
        assert_eq!(answers.widths[&lines[0]], 43);
        // A literal with a `§` code has no exact `Text` here: no answer, the run's counts stand.
        assert!(!answer_font_questions(
            &host,
            &[FontQuestion::Width(ClientText::Literal("§lbold".into()))],
            &mut answers,
            &mut texts,
        ));
        assert_eq!(
            line_text(&host, &Text::literal("plain")),
            Some(ClientText::Literal("plain".into()))
        );
    }

    fn components(id: &str, count: i32) -> StackComponents {
        StackComponents {
            item_id: id.to_owned(),
            count,
            patch: Vec::new(),
            opaque_components: false,
        }
    }

    /// `item` then `itemDecorations` of a slot paints as `item_stack` (model, bar, cooldown),
    /// then `itemCount`'s text: `x + 19 - 2 - width`, `y + 6 + 3`, white with a shadow (26.3
    /// `GuiGraphicsExtractor.itemCount` offsets 32-58), after the ops before it and before those
    /// after. A count of 1 draws no text; a model draw without its decorations refuses.
    #[test]
    fn a_slot_paints_its_stack_then_its_count_text() {
        let host = GameFont::new(&["demo:rune"]);
        let sprite = DrawOp::Sprite {
            sprite: "container/bundle/slot_background".into(),
            rect: [80.0, 8.0, 24.0, 24.0],
            argb: u32::MAX,
        };
        let slot = |count: i32| {
            let stack = components("demo:rune", count);
            [
                DrawOp::Stack {
                    stack: stack.clone(),
                    at: [84.0, 12.0],
                    scale: [1.0, 1.0],
                    model: true,
                    seed: 1,
                    decorations: None,
                },
                DrawOp::Stack {
                    stack,
                    at: [84.0, 12.0],
                    scale: [1.0, 1.0],
                    model: false,
                    seed: 0,
                    decorations: Some(None),
                },
            ]
        };
        for count in [10, 1] {
            let ops: Vec<DrawOp> = std::iter::once(sprite.clone())
                .chain(slot(count))
                .chain(std::iter::once(sprite.clone()))
                .collect();
            let forms = tooltip_forms(&host, &ops, FontAnswers::default(), BTreeMap::new())
                .expect("the stack forms");
            let plan = paint_plan(&ops, &forms).expect("the slot paints");
            let [
                Paint::Gui(before),
                Paint::Stack(stack, at),
                Paint::Gui(after),
            ] = &plan[..]
            else {
                panic!("three steps");
            };
            assert_eq!(before.len(), 1);
            assert_eq!(**stack, host.stack("demo:rune", count as u32));
            assert_eq!(*at, [84.0, 12.0]);
            if count == 10 {
                // "10" is 12 wide: x = 84 + 19 - 2 - 12, y = 12 + 9.
                assert_eq!(
                    after[0],
                    GuiDraw::Text {
                        text: Text::literal("10"),
                        x: 89,
                        y: 21,
                        color: 0xffff_ffff,
                        shadow: true,
                    }
                );
                assert_eq!(after.len(), 2);
            } else {
                assert_eq!(after.len(), 1);
            }
        }
        let model_alone = [slot(10)[0].clone()];
        let forms = tooltip_forms(&host, &model_alone, FontAnswers::default(), BTreeMap::new())
            .expect("the stack forms");
        assert!(matches!(
            paint_plan(&model_alone, &forms),
            Err("stack_draw")
        ));
    }
}
