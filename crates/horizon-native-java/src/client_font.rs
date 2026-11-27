//! The client font a jar measures and wraps text with: `Font.width` and `Font.split`, answered
//! exactly or counted.
//!
//! Two sources answer, and nothing else does:
//!
//! * **The glyph model.** 26.3's `StringSplitter` over the frame's advance table
//!   ([`TextMetrics::advances`]), for *plain* text: one run in `Style.EMPTY` (so every glyph is
//!   the default font's, never bold or obfuscated), no `§` formatting codes, and every glyph's
//!   advance in the table. [`TextMetrics::plain_split`] is `StringSplitter.splitLines(text,
//!   width, Style.EMPTY)` with its `LineBreakFinder` and `FlatComponents` (javap of the 26.3
//!   client jar), and [`TextMetrics::plain_width`] is `Mth.ceil(stringWidth(text))`.
//! * **The embedder's answers** ([`FontAnswers`]): the game's own `Font.width` and `Font.split`
//!   of an exact text, which an embedder reads from its font (Sunrise's
//!   `ClientTextClientHost::text_width` / `text_split`) after a frame asked for it
//!   ([`FontQuestion`], [`crate::client_surface::ClientFrameOutput::font_questions`]).
//!
//! A text neither answers (a styled component, a glyph past the table, a translation the frame
//! doesn't hold) is counted and its question recorded, so an embedder can answer it and run the
//! call again.
//!
//! `Font.split`'s lines are `FormattedCharSequence`s: here, client handles
//! (`client/char-sequence/<n>`) holding the line's text, in a Guava `ImmutableList` as
//! `Language.getVisualOrder(List)` collects them (`ImmutableList.toImmutableList()`). The surface
//! draws and measures them (`GuiGraphicsExtractor.text`, `Font.width`); any member of their own
//! counts. A line keeps the logical order of its characters: the embedder draws it as vanilla
//! draws a component, reordering it for display once, as `getVisualOrder` would have.

use std::collections::BTreeMap;

use super::{ClientSnapshot, ClientText, MAX_TEXT, TextMetrics, int_arg, marker};
use crate::{MethodRef, StdHost, Value};

/// The marker prefix of a `FormattedCharSequence` line handle.
pub(crate) const CHAR_SEQUENCE: &str = "client/char-sequence/";

/// A font question a frame could not answer exactly: the call counted. An embedder that answers
/// it ([`FontAnswers`]) and runs the call again gets the exact answer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FontQuestion {
    /// `Font.width` of this text (a `String`, a component or a split line).
    Width(ClientText),
    /// `Font.split(text, width)`.
    Split(ClientText, i32),
}

/// The game's own answers to font questions, for exact texts: what the embedder's font measured
/// (`Font.width(FormattedText)`) and wrapped (`Font.split(FormattedText, int)`). A line of a split
/// is the text the game's line is, as [`ClientText`]: a plain line is a
/// [`ClientText::Literal`], a styled one its component JSON.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontAnswers {
    pub widths: BTreeMap<ClientText, i32>,
    pub splits: BTreeMap<(ClientText, i32), Vec<ClientText>>,
}

impl FontAnswers {
    pub fn is_empty(&self) -> bool {
        self.widths.is_empty() && self.splits.is_empty()
    }
}

/// Font questions one frame records; more are counted without a record (a loop asking for
/// thousands of texts is not one an embedder answers).
const MAX_FONT_QUESTIONS: usize = 64;

/// `Font.width` overloads and `Font.split` (26.3 descriptors).
const WIDTH_STRING: &str = "(Ljava/lang/String;)I";
const WIDTH_FORMATTED_TEXT: &str = "(Lnet/minecraft/network/chat/FormattedText;)I";
const WIDTH_CHAR_SEQUENCE: &str = "(Lnet/minecraft/util/FormattedCharSequence;)I";
const SPLIT: &str = "(Lnet/minecraft/network/chat/FormattedText;I)Ljava/util/List;";

/// 26.3 `Mth.ceil(float)`: `(int) Math.ceil((double) f)` (offsets 0-6). Java's `d2i` saturates
/// (and answers 0 for NaN), as Rust's `as` does: `3e9f` and `+inf` are `Integer.MAX_VALUE`.
pub(crate) fn mth_ceil(value: f32) -> i32 {
    f64::from(value).ceil() as i32
}

/// The order a `Font.width` sums its glyphs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthOrder {
    /// The text's own order: `width(String)` and `width(FormattedText)` run
    /// `StringDecomposer.iterateFormatted` over it.
    Logical,
    /// A `FormattedCharSequence`'s, which `Language.getVisualOrder` made: `width(FormattedCharSequence)`
    /// and `centeredText` of a component (`getVisualOrderText()`). For bidirectional text that
    /// order is not the logical one, and a float sum can round differently.
    Visual,
}

impl TextMetrics {
    /// The text of `text` when the glyph model reads it exactly: one run in `Style.EMPTY` with no
    /// `§` codes. A literal is its own text; a translatable component the translation the frame
    /// holds for its key, when that has no `%` (a format argument or an escaped `%` is
    /// `TranslatableContents.decompose`'s, not modelled) and no `§`. A missing translation is
    /// not guessed to be the key (vanilla's answer only when the language truly lacks it), and a
    /// component with a style, siblings or arguments (its JSON) is not plain.
    pub(crate) fn plain<'a>(&'a self, text: &'a ClientText) -> Option<&'a str> {
        let plain = match text {
            ClientText::Literal(literal) => literal.as_str(),
            ClientText::Translatable(key) => {
                let translation = self.translations.get(key)?;
                if translation.contains('%') {
                    return None;
                }
                translation.as_str()
            }
            ClientText::Json(_) => return None,
        };
        (!plain.contains('§')).then_some(plain)
    }

    /// A glyph's advance in `Style.EMPTY` (`GlyphInfo.getAdvance(false)`), when the table has it.
    fn advance(&self, c: char) -> Option<f32> {
        self.advances.get(c as usize).copied()
    }

    /// `Font.width` of plain text: `Mth.ceil` of `StringSplitter.stringWidth`, the advances summed
    /// in a `float` (`MutableFloat.add`) in the text's order. `None` when a glyph is past the
    /// table, and, for a [`WidthOrder::Visual`] sum, when a partial sum rounds: the glyphs are
    /// then summed in an order the model doesn't know (bidirectional text reorders), which could
    /// change the answer. A sum with no rounding is the same in every order.
    pub(crate) fn plain_width(&self, text: &str, order: WidthOrder) -> Option<i32> {
        let mut total = 0.0f32;
        let mut wide = 0.0f64;
        for c in text.chars() {
            let advance = self.advance(c)?;
            total += advance;
            wide += f64::from(advance);
            if order == WidthOrder::Visual && f64::from(total) != wide {
                return None;
            }
        }
        Some(mth_ceil(total))
    }

    /// `StringSplitter.splitLines(text, width, Style.EMPTY)` of plain text (26.3, javap): the
    /// lines `Font.split` makes before `Language.getVisualOrder`. `None` when a glyph the finder
    /// would measure is past the table.
    ///
    /// Each pass runs a new `LineBreakFinder` (`maxWidth = Math.max(width, 1.0f)`) over what is
    /// left: a `\n` breaks there; any other character adds its advance, a space first noting
    /// itself as the last one, and once a character with a nonzero advance has been seen, a sum
    /// past `maxWidth` breaks at the last space (else at that character). The line is the text
    /// before the break (`FlatComponents.splitAt`; an empty head is `FormattedText.EMPTY`), and a
    /// space or `\n` at the break is dropped. What is left at the end is the last line; when
    /// nothing is, and the last break was a `\n`, an empty line follows it.
    pub(crate) fn plain_split(&self, text: &str, width: i32) -> Option<Vec<String>> {
        let chars: Vec<char> = text.chars().collect();
        // Every character but `\n` is measured on some pass (a line's own characters before its
        // break, and the rest on later ones), so all of them must be known.
        let advances: Vec<f32> = chars
            .iter()
            .map(|&c| {
                if c == '\n' {
                    Some(0.0)
                } else {
                    self.advance(c)
                }
            })
            .collect::<Option<_>>()?;
        let max_width = (width as f32).max(1.0);
        let mut lines = Vec::new();
        let mut force_new_line = false;
        let mut start = 0usize;
        while start < chars.len() {
            let mut sum = 0.0f32;
            let mut seen_width = false;
            let mut last_space = None;
            let mut line_break = None;
            for (at, (&c, &advance)) in chars[start..].iter().zip(&advances[start..]).enumerate() {
                if c == '\n' {
                    line_break = Some(at);
                    break;
                }
                if c == ' ' {
                    last_space = Some(at);
                }
                sum += advance;
                if seen_width && sum > max_width {
                    line_break = Some(last_space.unwrap_or(at));
                    break;
                }
                seen_width |= advance != 0.0;
            }
            let Some(index) = line_break else {
                break;
            };
            let at_break = chars[start + index];
            let new_line = at_break == '\n';
            force_new_line = new_line;
            lines.push(chars[start..start + index].iter().collect());
            start += index + usize::from(new_line || at_break == ' ');
        }
        let rest: String = chars[start..].iter().collect();
        if !rest.is_empty() {
            lines.push(rest);
        } else if force_new_line {
            lines.push(String::new());
        }
        Some(lines)
    }

    /// `Font.width` of `text` summed in `order`, exactly: the embedder's answer, else the glyph
    /// model's.
    pub fn exact_width(&self, text: &ClientText, order: WidthOrder) -> Option<i32> {
        if let Some(width) = self.answers.widths.get(text) {
            return Some(*width);
        }
        self.plain_width(self.plain(text)?, order)
    }

    /// `Font.split(text, width)`, exactly: the embedder's lines, else the glyph model's (each a
    /// plain literal).
    pub fn exact_split(&self, text: &ClientText, width: i32) -> Option<Vec<ClientText>> {
        if let Some(lines) = self.answers.splits.get(&(text.clone(), width)) {
            return Some(lines.clone());
        }
        let lines = self.plain_split(self.plain(text)?, width)?;
        Some(lines.into_iter().map(ClientText::Literal).collect())
    }
}

impl StdHost {
    /// A `Font` member during a firing: `width` of a `String`, a `FormattedText` (a component)
    /// or a `FormattedCharSequence` (a split line), and `split(FormattedText, int)`. `None`
    /// refuses (counted): an argument that is no text the surface reads, or a text no source
    /// answers exactly ([`TextMetrics::exact_width`], [`TextMetrics::exact_split`]), whose
    /// question is then recorded. Every frame's font answers only exactly.
    pub(super) fn font_call(
        &mut self,
        frame: &ClientSnapshot,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Value> {
        let arg = args.first()?;
        match (r.name.as_str(), r.descriptor.as_str()) {
            // `StringSplitter.stringWidth(String)` answers 0 for null (offsets 0-5).
            ("width", WIDTH_STRING) if *arg == Value::Ref(None) => Some(Value::Int(0)),
            ("width", WIDTH_STRING) => {
                let text = string_text(arg)?;
                self.frame_width(frame, text, WidthOrder::Logical)
                    .map(Value::Int)
            }
            ("width", WIDTH_FORMATTED_TEXT) => {
                let text = self.component_text(arg)?;
                self.frame_width(frame, text, WidthOrder::Logical)
                    .map(Value::Int)
            }
            ("width", WIDTH_CHAR_SEQUENCE) => {
                let text = self.char_sequence_text(arg)?;
                self.frame_width(frame, text, WidthOrder::Visual)
                    .map(Value::Int)
            }
            ("split", SPLIT) => {
                let text = self.component_text(arg)?;
                let width = int_arg(args, 1)?;
                let lines = self.frame_split(frame, text, width)?;
                Some(self.char_sequence_list(lines))
            }
            _ => None,
        }
    }

    /// `Font.width` of `text` summed in `order` in `frame`: exact, else the question is recorded
    /// and `None`. No frame guesses: a fallback advance, a missing translation read as its key,
    /// a style or font ignored would each be an answer the game might not give.
    pub(super) fn frame_width(
        &mut self,
        frame: &ClientSnapshot,
        text: ClientText,
        order: WidthOrder,
    ) -> Option<i32> {
        if let Some(width) = frame.metrics.exact_width(&text, order) {
            return Some(width);
        }
        self.ask_font(FontQuestion::Width(text));
        None
    }

    /// `Font.split(text, width)` in `frame`: exact, else the question is recorded and `None`.
    pub(super) fn frame_split(
        &mut self,
        frame: &ClientSnapshot,
        text: ClientText,
        width: i32,
    ) -> Option<Vec<ClientText>> {
        if let Some(lines) = frame.metrics.exact_split(&text, width) {
            return Some(lines);
        }
        self.ask_font(FontQuestion::Split(text, width));
        None
    }

    /// Record `question`, and that the refusal the caller is about to make is the font's
    /// ([`ClientFrameOutput::font_refusals`](super::ClientFrameOutput::font_refusals)).
    fn ask_font(&mut self, question: FontQuestion) {
        self.client.font_refusal_pending = true;
        let questions = &mut self.client.font_questions;
        if questions.len() < MAX_FONT_QUESTIONS && !questions.contains(&question) {
            questions.push(question);
        }
    }

    /// A `FormattedText` argument the surface reads whole: a host component (a bare literal, a
    /// bare translatable, else its JSON). A jar's own `FormattedText` object, or a literal or
    /// JSON past the surface's text bound, is none.
    pub(super) fn component_text(&self, value: &Value) -> Option<ClientText> {
        if !crate::host_component::is_component(value) {
            return None;
        }
        let json = crate::facade_text::json_now(value, Some(&self.component_states))?;
        let text = if let Some(literal) = crate::host_component::bare_literal(&json) {
            ClientText::Literal(literal)
        } else if let Some(key) = crate::host_component::bare_key(&json) {
            ClientText::Translatable(key)
        } else {
            ClientText::Json(json.to_string())
        };
        within_bound(text)
    }

    /// The text of a `Font.split` line handle this frame made.
    pub(super) fn char_sequence_text(&self, value: &Value) -> Option<ClientText> {
        let key = crate::game_object::handle_key(value)?;
        if !key.starts_with(CHAR_SEQUENCE) {
            return None;
        }
        self.client.char_sequences.get(key).cloned()
    }

    /// `Language.getVisualOrder(lines)`: a Guava `ImmutableList` (`toImmutableList()`: the shared
    /// empty one, a `SingletonImmutableList`, or a `RegularImmutableList`) of new line handles.
    fn char_sequence_list(&mut self, lines: Vec<ClientText>) -> Value {
        let values = lines
            .into_iter()
            .map(|line| {
                let key = format!("{CHAR_SEQUENCE}{}", self.client.next_char_sequence);
                self.client.next_char_sequence += 1;
                self.client.char_sequences.insert(key.clone(), line);
                marker(key)
            })
            .collect();
        self.unmodifiable_from(
            "com/google/common/collect/ImmutableList",
            "toImmutableList",
            "list",
            values,
        )
    }
}

/// A `String` argument's text: the jar's string itself, never an object it might be, and never
/// one past the surface's text bound (whose width would be of a cut text).
pub(super) fn string_text(value: &Value) -> Option<ClientText> {
    if crate::jar_object::is_object(value) || crate::host_component::is_component(value) {
        return None;
    }
    match value {
        Value::Ref(Some(text)) => within_bound(ClientText::Literal(text.clone())),
        _ => None,
    }
}

/// `text` when it is within the surface's bound ([`MAX_TEXT`] characters, 64 KiB of JSON).
fn within_bound(text: ClientText) -> Option<ClientText> {
    let fits = match &text {
        ClientText::Literal(text) | ClientText::Translatable(text) => {
            text.chars().count() <= MAX_TEXT
        }
        ClientText::Json(json) => json.len() <= 64 * 1024,
    };
    fits.then_some(text)
}

#[cfg(test)]
mod tests {
    //! Expected values are rows of `tests/fixtures/client_font/cases25.txt`: 26.3's own
    //! `Font.split` / `Font.width` / `Mth.ceil` on Java 25.0.5 (`oracle/FontCasesOracle.java`),
    //! over the same advance table (`fixture_client_font.rs` checks every row of that file
    //! against the runtime too). Refusals are the model's own (no oracle row: a counted case).
    use super::*;

    /// The oracle's advance table (`FontOracle.advance`), for chars 0..127.
    fn metrics() -> TextMetrics {
        let advances = (0u8..128)
            .map(|c| match c as char {
                c if (c as u32) < 32 => 0.0,
                ' ' => 4.0,
                c if "i!.,:;'|".contains(c) => 2.0,
                'l' => 3.0,
                c if "tI[]".contains(c) => 4.0,
                c if "fk<>{}()\"*".contains(c) => 5.0,
                'q' => 5.5,
                '@' | '~' => 7.0,
                _ => 6.0,
            })
            .collect();
        TextMetrics {
            advances,
            ..TextMetrics::default()
        }
    }

    fn split(text: &str, width: i32) -> Vec<String> {
        metrics().plain_split(text, width).expect("known glyphs")
    }

    #[test]
    fn splits_as_26_3_does() {
        for (text, width, java) in [
            ("ab cd", 20, &["ab", "cd"][..]),
            ("ab cd", 28, &["ab cd"]),
            ("abcd", 13, &["ab", "cd"]),
            ("a\nb", 100, &["a", "b"]),
            ("a\n", 100, &["a", ""]),
            ("a\n\n", 100, &["a", "", ""]),
            ("", 100, &[]),
            ("ab", 0, &["a", "b"]),
            ("ab", -5, &["a", "b"]),
            ("qqqqqqq qq", 40, &["qqqqqqq", "qq"]),
            ("  lead", 10, &[" ", "le", "a", "d"]),
            ("trailing   ", 30, &["trailin", "g   "]),
            ("\n", 5, &["", ""]),
            ("ab  cd", 12, &["ab", "", "cd"]),
        ] {
            assert_eq!(split(text, width), java, "{text:?} at {width}");
        }
    }

    #[test]
    fn widths_round_up_as_26_3_does() {
        let metrics = metrics();
        for (text, java) in [("q", 6), ("qq", 11), ("qqq", 17), ("Iq6", 16), ("", 0)] {
            for order in [WidthOrder::Logical, WidthOrder::Visual] {
                assert_eq!(metrics.plain_width(text, order), Some(java), "{text:?}");
            }
        }
    }

    #[test]
    fn mth_ceil_saturates_as_26_3_does() {
        for (value, java) in [
            (38.5, 39),
            (39.0, 39),
            (3e9, i32::MAX),
            (f32::INFINITY, i32::MAX),
            (f32::NAN, 0),
            (-0.5, 0),
            (-3e9, i32::MIN),
        ] {
            assert_eq!(mth_ceil(value), java, "{value}");
        }
    }

    #[test]
    fn an_unknown_glyph_or_a_styled_text_is_no_answer() {
        let metrics = metrics();
        assert_eq!(metrics.plain_split("a\u{3042}", 100), None);
        assert_eq!(metrics.plain_width("\u{3042}", WidthOrder::Logical), None);
        assert_eq!(metrics.plain(&ClientText::Literal("§lab".into())), None);
        assert_eq!(
            metrics.plain(&ClientText::Translatable("missing".into())),
            None
        );
        assert_eq!(metrics.plain(&ClientText::Json("{}".into())), None);
    }

    /// A sum in the text's own order answers whatever rounds; one in a visual order the model
    /// doesn't know answers only when no partial sum rounds.
    #[test]
    fn only_a_visual_sum_refuses_rounding() {
        let mut metrics = metrics();
        metrics.advances['b' as usize] = 0.1;
        let logical = metrics.plain_width("bbb", WidthOrder::Logical);
        // 0.1f + 0.1f + 0.1f is 0.3 and a little: `Mth.ceil` makes it 1.
        assert_eq!(logical, Some(1));
        assert_eq!(metrics.plain_width("bbb", WidthOrder::Visual), None);
    }

    #[test]
    fn the_embedders_answers_come_first() {
        let mut metrics = metrics();
        let text = ClientText::Json(r#"{"text":"ab","bold":true}"#.into());
        metrics.answers.widths.insert(text.clone(), 14);
        metrics
            .answers
            .splits
            .insert((text.clone(), 10), vec![ClientText::Literal("ab".into())]);
        assert_eq!(metrics.exact_width(&text, WidthOrder::Visual), Some(14));
        assert_eq!(
            metrics.exact_split(&text, 10),
            Some(vec![ClientText::Literal("ab".into())])
        );
        assert_eq!(metrics.exact_split(&text, 11), None);
    }
}
