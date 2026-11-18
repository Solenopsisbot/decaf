//! `com.google.gson`'s builder, formatting style and JSON writer.
//!
//! A config bootstrap builds its `Gson` through the fluent builder:
//! `new GsonBuilder().setPrettyPrinting().registerTypeAdapter(..).create()`.
//! Every one of those calls returns the same builder in Gson, so the chain is
//! the object the constructor made; only `create()` makes the `Gson`. The
//! builder's options live beside its marker in the interpreter's field table
//! (the same place a `TypeToken` keeps its captured type), and `create()`
//! copies them onto the `Gson` marker.
//!
//! Options are never silently dropped. Each one either **changes the writer**,
//! exactly as Gson 2.14 does, or marks the `Gson` so every `toJson`/`fromJson`
//! is a counted unknown (the host answers its neutral value) instead of text
//! that ignores the option.
//!
//! Applied, from Gson's bytecode (`GsonBuilder`, `FieldNamingPolicy`,
//! `Excluder`, `JsonWriter`):
//! * `setPrettyPrinting`/`setFormattingStyle`: `FormattingStyle`'s newline,
//!   indent and space-after-separators shape the document.
//! * `disableHtmlEscaping`: the HTML-unsafe set (`<`, `>`, `&`, `=`, `'`) is
//!   escaped unless this was called.
//! * `serializeNulls`: jar-object fields with a null value are written.
//! * `setFieldNamingPolicy`: every `FieldNamingPolicy` constant's transform
//!   names the JSON keys, both when writing and when reading a jar object.
//! * `excludeFieldsWithModifiers`: `Excluder.withModifiers` **replaces** its
//!   default `STATIC|TRANSIENT` mask with the arguments' OR, so
//!   `excludeFieldsWithModifiers(0)` really does write static and transient
//!   fields — a Gson quirk the model reproduces (synthetic fields are always
//!   excluded, as `Excluder.excludeField` does). Static values come from the
//!   invocation's static-field table.
//! * `generateNonExecutableJson`: the `)]}'\n` prefix on the text.
//! * `serializeSpecialFloatingPointValues`: `NaN`/`Infinity`/`-Infinity`
//!   literals; without it Gson's `IllegalArgumentException` is raised.
//! * `setLongSerializationPolicy`: `STRING` writes a `long` as a quoted
//!   number, `DEFAULT` as a number.
//!
//! Recorded as counted, because applying them needs data this runtime does not
//! carry: `setFieldNamingStrategy` and the `registerType*Adapter` calls (their
//! `translateName`/`write` is arbitrary jar code over a `Field`/`JsonWriter`
//! this model does not build), `setExclusionStrategies` and the two `add*` forms
//! (the same, over `FieldAttributes`), `excludeFieldsWithoutExposeAnnotation`
//! (field annotations are not parsed), `enableComplexMapKeySerialization`
//! (non-primitive map keys), `setDateFormat` (a `Date` model), `setVersion`
//! (`@Since`/`@Until`), `setObjectToNumberStrategy`/`setNumberToNumberStrategy`
//! (number parsing) and `disableInnerClassSerialization` (Gson refuses an inner
//! class there; this writer would emit it).
//!
//! Accepted and modeled for reading: `setLenient`/`setStrictness` record the
//! reader's handling of non-finite numeric tokens. `disableJdkUnsafe` (nothing here instantiates
//! through `Unsafe`; `fromJson` already builds an object without running a
//! constructor) and `addReflectionAccessFilter` (filters Gson's reflection into
//! JDK classes, which this model never reflects into).
//!
//! A value the writer cannot reproduce faithfully (an opaque host marker) makes
//! the whole `toJson` a counted unknown, never invented text.

use super::*;

/// The synthetic field key a builder/Gson marker keeps its [`GsonStyle`]
/// under. It is not a jar field name, so no reflective read sees it.
pub(super) const STYLE_FIELD: &str = "#gsonStyle";
const NAMING_OWNER: &str = "com/google/gson/FieldNamingPolicy";
const LONG_POLICY_OWNER: &str = "com/google/gson/LongSerializationPolicy";
/// Gson's `Excluder()` default: `Modifier.STATIC | Modifier.TRANSIENT`.
const DEFAULT_FIELD_MODIFIERS: u16 = 0x0088;
/// `ACC_SYNTHETIC`, which `Excluder.excludeField` always excludes.
const SYNTHETIC: u16 = 0x1000;
/// The prefix `generateNonExecutableJson()` writes.
const NON_EXECUTABLE_PREFIX: &str = ")]}'\n";

/// `FieldNamingPolicy`'s seven constants and their `translateName` transform.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Naming {
    #[default]
    Identity,
    UpperCamelCase,
    UpperCamelCaseWithSpaces,
    UpperCaseWithUnderscores,
    LowerCaseWithUnderscores,
    LowerCaseWithDashes,
    LowerCaseWithDots,
}

impl Naming {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "IDENTITY" => Self::Identity,
            "UPPER_CAMEL_CASE" => Self::UpperCamelCase,
            "UPPER_CAMEL_CASE_WITH_SPACES" => Self::UpperCamelCaseWithSpaces,
            "UPPER_CASE_WITH_UNDERSCORES" => Self::UpperCaseWithUnderscores,
            "LOWER_CASE_WITH_UNDERSCORES" => Self::LowerCaseWithUnderscores,
            "LOWER_CASE_WITH_DASHES" => Self::LowerCaseWithDashes,
            "LOWER_CASE_WITH_DOTS" => Self::LowerCaseWithDots,
            _ => return None,
        })
    }

    fn index(self) -> u8 {
        match self {
            Self::Identity => 0,
            Self::UpperCamelCase => 1,
            Self::UpperCamelCaseWithSpaces => 2,
            Self::UpperCaseWithUnderscores => 3,
            Self::LowerCaseWithUnderscores => 4,
            Self::LowerCaseWithDashes => 5,
            Self::LowerCaseWithDots => 6,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            1 => Self::UpperCamelCase,
            2 => Self::UpperCamelCaseWithSpaces,
            3 => Self::UpperCaseWithUnderscores,
            4 => Self::LowerCaseWithUnderscores,
            5 => Self::LowerCaseWithDashes,
            6 => Self::LowerCaseWithDots,
            _ => Self::Identity,
        }
    }

    /// Gson's `translateName` for a field named `name`.
    fn transform(self, name: &str) -> String {
        match self {
            Self::Identity => name.to_owned(),
            Self::UpperCamelCase => upper_case_first_letter(name),
            Self::UpperCamelCaseWithSpaces => {
                upper_case_first_letter(&separate_camel_case(name, ' '))
            }
            Self::UpperCaseWithUnderscores => separate_camel_case(name, '_').to_uppercase(),
            Self::LowerCaseWithUnderscores => separate_camel_case(name, '_').to_lowercase(),
            Self::LowerCaseWithDashes => separate_camel_case(name, '-').to_lowercase(),
            Self::LowerCaseWithDots => separate_camel_case(name, '.').to_lowercase(),
        }
    }
}

/// `FieldNamingPolicy.separateCamelCase`: a separator before every uppercase
/// letter that is not the first character.
fn separate_camel_case(name: &str, separator: char) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if index > 0 && ch.is_uppercase() {
            out.push(separator);
        }
        out.push(ch);
    }
    out
}

/// `FieldNamingPolicy.upperCaseFirstLetter`: uppercase the first letter, or
/// answer the name unchanged when that letter is already uppercase.
fn upper_case_first_letter(name: &str) -> String {
    for (index, ch) in name.char_indices() {
        if !ch.is_alphabetic() {
            continue;
        }
        if ch.is_uppercase() {
            return name.to_owned();
        }
        let mut out = String::with_capacity(name.len());
        out.push_str(&name[..index]);
        out.extend(ch.to_uppercase());
        out.push_str(&name[index + ch.len_utf8()..]);
        return out;
    }
    name.to_owned()
}

/// `FormattingStyle`: the newline, indent and space-after-separators of the
/// document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Formatting {
    newline: String,
    indent: String,
    space_after_separators: bool,
}

impl Default for Formatting {
    /// Gson's `DEFAULT_FORMATTING_STYLE` is `FormattingStyle.COMPACT`.
    fn default() -> Self {
        Self {
            newline: String::new(),
            indent: String::new(),
            space_after_separators: false,
        }
    }
}

impl Formatting {
    /// `FormattingStyle.PRETTY` from gson-2.14.0's `<clinit>`: a newline, a
    /// two-space indent, spaces after the separators.
    fn pretty() -> Self {
        Self {
            newline: "\n".into(),
            indent: "  ".into(),
            space_after_separators: true,
        }
    }

    /// This style as a new `FormattingStyle` object, or the class's
    /// constant `constant` (`host_gson.rs`).
    fn value(&self, constant: Option<&str>) -> Value {
        crate::runtime::host_gson::formatting_style(
            &crate::runtime::host_gson::Style {
                newline: self.newline.clone(),
                indent: self.indent.clone(),
                space_after_separators: self.space_after_separators,
            },
            constant,
        )
    }

    /// The style a `FormattingStyle` handle holds; `None` for any other value.
    fn of_value(value: &Value) -> Option<Self> {
        let style = crate::runtime::host_gson::style_of(value)?;
        Some(Self {
            newline: style.newline,
            indent: style.indent,
            space_after_separators: style.space_after_separators,
        })
    }
}

/// The document shape and writer options one `Gson` was created with.
#[derive(Clone)]
pub(super) struct GsonStyle {
    formatting: Formatting,
    html_safe: bool,
    serialize_nulls: bool,
    naming: Naming,
    /// `Excluder`'s field-modifier mask; `excludeFieldsWithModifiers` replaces
    /// it, exactly as `Excluder.withModifiers` does.
    field_modifiers: u16,
    non_executable: bool,
    special_floats: bool,
    long_strings: bool,
    /// Gson's reader strictness. `false` is the default/LENIENT setting;
    /// STRICT rejects non-finite numeric values while still accepting the
    /// regular JSON number grammar.
    strict: bool,
    /// An option that changes the output is set but cannot be applied here, so
    /// the writer must not produce text.
    counted: bool,
}

impl Default for GsonStyle {
    fn default() -> Self {
        Self {
            formatting: Formatting::default(),
            html_safe: true,
            serialize_nulls: false,
            naming: Naming::Identity,
            field_modifiers: DEFAULT_FIELD_MODIFIERS,
            non_executable: false,
            special_floats: false,
            long_strings: false,
            strict: false,
            counted: false,
        }
    }
}

impl GsonStyle {
    /// The key this option set is kept under (`host_gson.rs`): the
    /// runtime's own encoding, hex text and digits.
    fn key(&self) -> String {
        format!(
            "newline={};indent={};space={};html={};nulls={};\
             naming={};mods={};nonexec={};special={};longs={};strict={};counted={}",
            hex(&self.formatting.newline),
            hex(&self.formatting.indent),
            u8::from(self.formatting.space_after_separators),
            u8::from(self.html_safe),
            u8::from(self.serialize_nulls),
            self.naming.index(),
            self.field_modifiers,
            u8::from(self.non_executable),
            u8::from(self.special_floats),
            u8::from(self.long_strings),
            u8::from(self.strict),
            u8::from(self.counted),
        )
    }

    fn from_key(rest: &str) -> Option<Self> {
        let mut style = Self::default();
        for entry in rest.split(';') {
            let (key, value) = entry.split_once('=')?;
            let flag = |value: &str| match value {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            };
            match key {
                "newline" => style.formatting.newline = unhex(value)?,
                "indent" => style.formatting.indent = unhex(value)?,
                "space" => style.formatting.space_after_separators = flag(value)?,
                "html" => style.html_safe = flag(value)?,
                "nulls" => style.serialize_nulls = flag(value)?,
                "naming" => style.naming = Naming::from_index(value.parse().ok()?),
                "mods" => style.field_modifiers = value.parse().ok()?,
                "nonexec" => style.non_executable = flag(value)?,
                "special" => style.special_floats = flag(value)?,
                "longs" => style.long_strings = flag(value)?,
                "strict" => style.strict = flag(value)?,
                "counted" => style.counted = flag(value)?,
                _ => return None,
            }
        }
        Some(style)
    }

    /// The writer options `Gson.toJson` applies to a `JsonElement`
    /// (`gson_tree.rs`).
    pub(super) fn tree_style(&self) -> super::gson_tree::TreeStyle {
        super::gson_tree::TreeStyle {
            newline: self.formatting.newline.clone(),
            indent: self.formatting.indent.clone(),
            space_after_separators: self.formatting.space_after_separators,
            html_safe: self.html_safe,
            serialize_nulls: self.serialize_nulls,
            lenient: false,
        }
    }

    /// Whether a `toJson`/`fromJson` on this `Gson` must be a counted unknown.
    pub(crate) fn is_counted(&self) -> bool {
        self.counted
    }

    fn non_executable(&self) -> bool {
        self.non_executable
    }

    fn long_strings(&self) -> bool {
        self.long_strings
    }

    pub(crate) fn special_floats(&self) -> bool {
        self.special_floats
    }

    pub(super) fn allows_special_floats(&self) -> bool {
        !self.strict
    }

    fn serialize_nulls(&self) -> bool {
        self.serialize_nulls
    }

    /// The JSON key a jar field with this name gets.
    pub(super) fn field_name(&self, name: &str) -> String {
        self.naming.transform(name)
    }

    /// Whether `Excluder.excludeField` keeps a field with these access flags:
    /// a synthetic field is always excluded, and the modifier mask (which
    /// `excludeFieldsWithModifiers` replaces) excludes the rest.
    pub(super) fn includes_field(&self, access: u16) -> bool {
        access & SYNTHETIC == 0 && access & self.field_modifiers == 0
    }

    /// Gson's `JsonWriter.string` escaping: the JSON control characters, plus
    /// the HTML-unsafe set (`<`, `>`, `&`, `=`, `'`) while HTML escaping is on.
    fn escape(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0c}' => out.push_str("\\f"),
                '<' | '>' | '&' | '=' | '\'' if self.html_safe => {
                    out.push_str(&format!("\\u{:04x}", ch as u32));
                }
                ch if (ch as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", ch as u32));
                }
                ch => out.push(ch),
            }
        }
        out
    }

    /// The whitespace before one element or member at `depth`, and `""` in
    /// compact style.
    fn line(&self, depth: usize) -> String {
        if self.formatting.newline.is_empty() {
            self.formatting.indent.repeat(depth)
        } else {
            format!(
                "{}{}",
                self.formatting.newline,
                self.formatting.indent.repeat(depth)
            )
        }
    }

    /// The text between a member's name and its value.
    fn colon(&self) -> &'static str {
        if self.formatting.space_after_separators {
            ": "
        } else {
            ":"
        }
    }
}

fn hex(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<String> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks(2) {
        let pair = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair, 16).ok()?);
    }
    String::from_utf8(bytes).ok()
}

/// The Gson constants a builder takes as arguments: `FormattingStyle`'s two
/// styles, `FieldNamingPolicy`'s seven policies and
/// `LongSerializationPolicy`'s two modes.
pub(super) fn static_field(r: &MethodRef) -> Option<Value> {
    match r.owner.as_str() {
        "com/google/gson/FormattingStyle" => {
            let style = match r.name.as_str() {
                "COMPACT" => Formatting::default(),
                "PRETTY" => Formatting::pretty(),
                _ => return None,
            };
            Some(style.value(Some(&r.name)))
        }
        NAMING_OWNER => Naming::from_name(&r.name)
            .map(|_| Value::Ref(Some(format!("enum/{NAMING_OWNER}/{}", r.name)))),
        LONG_POLICY_OWNER => matches!(r.name.as_str(), "DEFAULT" | "STRING")
            .then(|| Value::Ref(Some(format!("enum/{LONG_POLICY_OWNER}/{}", r.name)))),
        _ => None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Answer a `GsonBuilder`, `Gson` or `FormattingStyle` call, or `None`.
    pub(super) fn invoke_gson_builder(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        match r.owner.as_str() {
            "com/google/gson/FormattingStyle" => {
                let Some(style) = receiver.and_then(Formatting::of_value) else {
                    return Ok(None);
                };
                Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
                    ("getNewline", "()Ljava/lang/String;") => {
                        Value::Ref(Some(style.newline.clone()))
                    }
                    ("getIndent", "()Ljava/lang/String;") => Value::Ref(Some(style.indent.clone())),
                    ("usesSpaceAfterSeparators", "()Z") => {
                        Value::Int(i32::from(style.space_after_separators))
                    }
                    ("withNewline", "(Ljava/lang/String;)Lcom/google/gson/FormattingStyle;") => {
                        // `FormattingStyle`'s constructor checks (gson 2.14.0).
                        let Some(newline) = args.first().and_then(expect_ref) else {
                            return Err(npe_with("newline == null"));
                        };
                        if !newline.chars().all(|c| matches!(c, '\r' | '\n')) {
                            return Err(illegal_argument(
                                "Only combinations of \\n and \\r are allowed in newline.",
                            ));
                        }
                        Formatting {
                            newline: newline.clone(),
                            ..style
                        }
                        .value(None)
                    }
                    ("withIndent", "(Ljava/lang/String;)Lcom/google/gson/FormattingStyle;") => {
                        let Some(indent) = args.first().and_then(expect_ref) else {
                            return Err(npe_with("indent == null"));
                        };
                        if !indent.chars().all(|c| matches!(c, ' ' | '\t')) {
                            return Err(illegal_argument(
                                "Only combinations of spaces and tabs are allowed in indent.",
                            ));
                        }
                        Formatting {
                            indent: indent.clone(),
                            ..style
                        }
                        .value(None)
                    }
                    ("withSpaceAfterSeparators", "(Z)Lcom/google/gson/FormattingStyle;") => {
                        let space = matches!(args.first(), Some(Value::Int(flag)) if *flag != 0);
                        Formatting {
                            space_after_separators: space,
                            ..style
                        }
                        .value(None)
                    }
                    _ => return Ok(None),
                }))
            }
            "com/google/gson/GsonBuilder" => {
                let receiver = receiver.and_then(expect_ref).cloned();
                self.invoke_gson_builder_call(r, receiver, args)
            }
            _ => Ok(None),
        }
    }

    fn invoke_gson_builder_call(
        &mut self,
        r: &MethodRef,
        receiver: Option<String>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = receiver else {
            return Ok(None);
        };
        // Every fluent method answers the receiver, so the chain stays on the
        // builder the constructor made, exactly as Gson's `return this` does.
        let answer = || Ok(Some(crate::runtime::jar_object::stored_value(&marker)));
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("<init>", "()V") => {
                self.set_gson_style(&marker, GsonStyle::default());
                Ok(Some(Value::Void))
            }
            ("create", "()Lcom/google/gson/Gson;") => {
                let style = self.gson_style(&marker);
                let object = self.new_object("com/google/gson/Gson");
                let created = expect_ref(&object).cloned().unwrap_or_default();
                self.set_gson_style(&created, style);
                if let Some(adapters) = self.gson_type_adapters.get(&marker).cloned() {
                    self.gson_type_adapters.insert(created.clone(), adapters);
                }
                Ok(Some(object))
            }
            // Gson's own null checks, so a bootstrap that passes null fails
            // where the real builder fails. Every option below records what it
            // changes; none is silently dropped.
            (
                "excludeFieldsWithModifiers"
                | "registerTypeAdapterFactory"
                | "registerTypeAdapter"
                | "registerTypeHierarchyAdapter"
                | "setExclusionStrategies"
                | "setFieldNamingStrategy"
                | "setLongSerializationPolicy"
                | "setFormattingStyle",
                _,
            ) => {
                if args.iter().any(|value| matches!(value, Value::Ref(None))) {
                    return Err(npe());
                }
                match r.name.as_str() {
                    "setFormattingStyle" => {
                        // Only a `FormattingStyle` the model made is a style.
                        let Some(formatting) = args.first().and_then(Formatting::of_value) else {
                            return Ok(None);
                        };
                        let mut style = self.gson_style(&marker);
                        style.formatting = formatting;
                        self.set_gson_style(&marker, style);
                    }
                    "setLongSerializationPolicy" => {
                        let mut style = self.gson_style(&marker);
                        style.long_strings = args
                            .first()
                            .and_then(expect_ref)
                            .and_then(|policy| policy.rsplit_once('/').map(|(_, name)| name))
                            == Some("STRING");
                        self.set_gson_style(&marker, style);
                    }
                    "excludeFieldsWithModifiers" => {
                        let mut style = self.gson_style(&marker);
                        style.field_modifiers = self.field_modifier_mask(args)?;
                        self.set_gson_style(&marker, style);
                    }
                    // A registered adapter, naming strategy, exclusion
                    // strategy or hierarchy adapter is arbitrary jar code whose
                    // effect on the text this writer cannot reproduce.
                    _ => {
                        if r.name == "registerTypeAdapter" {
                            self.check_gson_type_adapter(args)?;
                            let mut style = self.gson_style(&marker);
                            style.counted = true;
                            self.set_gson_style(&marker, style);
                            let type_name = args
                                .first()
                                .and_then(crate::runtime::host_class::class_name)
                                .map(str::to_owned);
                            let adapter = args.get(1).cloned();
                            if let (Some(type_name), Some(adapter)) = (type_name, adapter)
                                && let Some(adapter_class) =
                                    crate::runtime::jar_object::object_class(&adapter)
                            {
                                let mut current = Some(adapter_class);
                                let mut is_type_adapter = false;
                                for _ in 0..MAX_SUPERCLASS_WALK {
                                    if current == Some("com/google/gson/TypeAdapter") {
                                        is_type_adapter = true;
                                        break;
                                    }
                                    current = current.and_then(|name| self.class(name)).and_then(
                                        |file| {
                                            (!file.super_name.is_empty())
                                                .then_some(file.super_name.as_str())
                                        },
                                    );
                                }
                                if is_type_adapter {
                                    eprintln!(
                                        "GSON_DEBUG register marker={marker} type={type_name} adapter={adapter_class}"
                                    );
                                    self.gson_type_adapters
                                        .entry(marker.clone())
                                        .or_default()
                                        .push((type_name, adapter));
                                }
                                // Keep every registration counted until callback dispatch can prove an exact write.
                                let mut style = self.gson_style(&marker);
                                style.counted = true;
                                self.set_gson_style(&marker, style);
                            }
                        } else {
                            let mut style = self.gson_style(&marker);
                            style.counted = true;
                            self.set_gson_style(&marker, style);
                        }
                    }
                }
                answer()
            }
            ("setPrettyPrinting", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.formatting = Formatting::pretty();
                self.set_gson_style(&marker, style);
                answer()
            }
            ("disableHtmlEscaping", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.html_safe = false;
                self.set_gson_style(&marker, style);
                answer()
            }
            ("serializeNulls", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.serialize_nulls = true;
                self.set_gson_style(&marker, style);
                answer()
            }
            ("generateNonExecutableJson", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.non_executable = true;
                self.set_gson_style(&marker, style);
                answer()
            }
            ("serializeSpecialFloatingPointValues", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.special_floats = true;
                self.set_gson_style(&marker, style);
                answer()
            }
            (
                "setFieldNamingPolicy",
                "(Lcom/google/gson/FieldNamingPolicy;)Lcom/google/gson/GsonBuilder;",
            ) => {
                let mut style = self.gson_style(&marker);
                style.naming = args
                    .first()
                    .and_then(expect_ref)
                    .and_then(|policy| policy.rsplit_once('/').map(|(_, name)| name))
                    .and_then(Naming::from_name)
                    .unwrap_or_default();
                self.set_gson_style(&marker, style);
                answer()
            }
            // Output-changing options this model cannot apply: every
            // `toJson`/`fromJson` on the created `Gson` becomes a counted
            // unknown instead of text that ignores the option.
            (
                "excludeFieldsWithoutExposeAnnotation"
                | "enableComplexMapKeySerialization"
                | "setDateFormat"
                | "setVersion"
                | "setObjectToNumberStrategy"
                | "setNumberToNumberStrategy"
                | "disableInnerClassSerialization"
                | "addSerializationExclusionStrategy"
                | "addDeserializationExclusionStrategy"
                | "addReflectionAccessFilter",
                _,
            ) => {
                if r.name == "addReflectionAccessFilter"
                    && args.iter().any(|value| matches!(value, Value::Ref(None)))
                {
                    return Err(npe());
                }
                if matches!(
                    r.name.as_str(),
                    "addSerializationExclusionStrategy" | "addDeserializationExclusionStrategy"
                ) && args.iter().any(|value| matches!(value, Value::Ref(None)))
                {
                    return Err(npe());
                }
                // `addReflectionAccessFilter` filters Gson's reflection into
                // JDK classes, which this model never reflects into; it is
                // inert here, not counted.
                if r.name != "addReflectionAccessFilter" {
                    let mut style = self.gson_style(&marker);
                    style.counted = true;
                    self.set_gson_style(&marker, style);
                }
                answer()
            }
            ("setLenient", "()Lcom/google/gson/GsonBuilder;") => {
                let mut style = self.gson_style(&marker);
                style.strict = false;
                self.set_gson_style(&marker, style);
                answer()
            }
            // Nothing here instantiates through `Unsafe`.
            ("disableJdkUnsafe", _) => answer(),
            ("setStrictness", "(Lcom/google/gson/Strictness;)Lcom/google/gson/GsonBuilder;") => {
                if args.iter().any(|value| matches!(value, Value::Ref(None))) {
                    return Err(npe());
                }
                let mut style = self.gson_style(&marker);
                // Enum constants are represented by a stable marker containing
                // the declaring class and constant name. Unknown markers are
                // conservatively treated as STRICT; Gson's default remains
                // lenient until this method is called.
                style.strict = args
                    .first()
                    .and_then(expect_ref)
                    .and_then(|constant| {
                        self.enum_constants
                            .get(constant)
                            .map(|(name, _)| name == "STRICT")
                            .or_else(|| Some(!constant.contains("LENIENT")))
                    })
                    .unwrap_or(true);
                self.set_gson_style(&marker, style);
                answer()
            }
            _ => Ok(None),
        }
    }

    /// Gson's `registerTypeAdapter` argument checks: a non-null adapter that
    /// implements one of the four supported interfaces, and no override of the
    /// built-in `Object` adapter. The interface test is exact for a jar class;
    /// a lambda marker is accepted because it does not record its functional
    /// interface, and a value the runtime holds as text is reported as
    /// `java.lang.String`, the class Gson would name.
    fn check_gson_type_adapter(&mut self, args: &[Value]) -> Result<(), NativeError> {
        if let Some(class) = args.first().and_then(expect_ref)
            && class == "java/lang/Object"
        {
            return Err(NativeError::UncaughtException {
                class: "java/lang/IllegalArgumentException".into(),
                message: Some("Cannot override built-in adapter for class java.lang.Object".into()),
                frame: None,
            });
        }
        let Some(adapter) = args.get(1).filter(|adapter| **adapter != Value::Ref(None)) else {
            return Err(npe());
        };
        if crate::runtime::callback_marker::callback_text(adapter).is_some() {
            return Ok(());
        }
        let Some(class) = crate::runtime::jar_object::object_class(adapter) else {
            return Err(not_a_supported_adapter("java/lang/String"));
        };
        if self.class(class).is_none() {
            return Err(not_a_supported_adapter(class));
        }
        let supported = [
            "com/google/gson/JsonSerializer",
            "com/google/gson/JsonDeserializer",
            "com/google/gson/InstanceCreator",
            "com/google/gson/TypeAdapter",
        ];
        if supported
            .iter()
            .any(|wanted| self.class_is_instance(class, wanted))
        {
            return Ok(());
        }
        Err(not_a_supported_adapter(class))
    }

    /// The OR of an `excludeFieldsWithModifiers(int...)` array. Gson's
    /// `Excluder.withModifiers` **replaces** its `STATIC|TRANSIENT` default
    /// with this mask, so a zero mask really does include both.
    fn field_modifier_mask(&self, args: &[Value]) -> Result<u16, NativeError> {
        let Some(array) = args.first() else {
            return Ok(DEFAULT_FIELD_MODIFIERS);
        };
        let mut mask = 0u16;
        for value in self.array(array)?.iter() {
            let Value::Int(bits) = value else {
                return Err(NativeError::Metadata(
                    "excludeFieldsWithModifiers needs an int array".into(),
                ));
            };
            mask |= *bits as u16;
        }
        Ok(mask)
    }

    fn gson_style(&self, marker: &str) -> GsonStyle {
        self.fields
            .get(&format!("{marker}|{STYLE_FIELD}"))
            .and_then(crate::runtime::host_gson::options_key)
            .and_then(GsonStyle::from_key)
            .unwrap_or_default()
    }

    /// The style a builder or `Gson` receiver carries, or Gson's default.
    pub(crate) fn gson_style_of(&self, receiver: Option<&Value>) -> GsonStyle {
        receiver
            .and_then(expect_ref)
            .map_or_else(GsonStyle::default, |marker| self.gson_style(marker))
    }

    pub(super) fn set_gson_style(&mut self, marker: &str, style: GsonStyle) {
        self.fields.insert(
            format!("{marker}|{STYLE_FIELD}"),
            crate::runtime::host_gson::options(style.key()),
        );
    }

    /// Gson's `toJson` text for a modelled value, or `None` when the value has
    /// no honest JSON shape (an object the runtime does not own and cannot
    /// reflect over). The caller counts the call instead of writing fake text.
    pub(crate) fn gson_to_json(
        &mut self,
        value: &Value,
        style: &GsonStyle,
        depth: usize,
        gson_marker: Option<&str>,
    ) -> Result<Option<String>, NativeError> {
        self.gson_to_json_mode(value, style, depth, false, gson_marker)
    }

    /// The tree adapter uses the same bounded reflection model as the writer,
    /// but its extra answers must not alter `Gson.toJson` itself.  In
    /// particular, Gson's writer keeps source-number spelling and does not
    /// know the native collection markers used by the interpreter.
    pub(crate) fn gson_to_json_tree(
        &mut self,
        value: &Value,
        style: &GsonStyle,
        depth: usize,
        gson_marker: Option<&str>,
    ) -> Result<Option<String>, NativeError> {
        self.gson_to_json_mode(value, style, depth, true, gson_marker)
    }

    fn gson_to_json_mode(
        &mut self,
        value: &Value,
        style: &GsonStyle,
        depth: usize,
        tree_mode: bool,
        gson_marker: Option<&str>,
    ) -> Result<Option<String>, NativeError> {
        // One level of a nested value (#250): Gson recurses through a cycle until a
        // `StackOverflowError`, and so does this writer.
        let _level = crate::runtime::host_recursion::enter()?;
        // Exact registered TypeAdapter dispatch precedes reflective serialization.
        if let Some(class) = class_of_value(value)
            && {
                eprintln!(
                    "GSON_DEBUG dispatch value={value:?} class={class} marker={gson_marker:?} adapters={:?}",
                    self.gson_type_adapters
                );
                true
            }
            && let Some(adapter) = self
                .gson_type_adapters
                .get(gson_marker.unwrap_or_default())
                .into_iter()
                .flat_map(|v| v.iter())
                .rev()
                .find(|(ty, _)| ty == class.as_str())
                .map(|(_, adapter)| adapter.clone())
        {
            let sink = format!("gson-adapter-{depth}");
            let sink_key = format!("mem:{sink}");
            self.io_writers.insert(sink_key.clone(), String::new());
            let writer = self.new_object("com/google/gson/stream/JsonWriter");
            if let Some(wm) = expect_ref(&writer) {
                let mut state = gson_writer::State::new(sink.clone());
                state.apply_tree_style(&style.tree_style());
                self.gson_writers.insert(wm.clone(), state);
                let Some(ac) = crate::runtime::jar_object::object_class(&adapter) else {
                    return Ok(None);
                };
                let result = self.invoke(
                    ac,
                    "write",
                    "(Lcom/google/gson/stream/JsonWriter;Ljava/lang/Object;)V",
                    Some(adapter.clone()),
                    &[writer, value.clone()],
                );
                if let Err(error) = &result {
                    eprintln!("GSON_DEBUG adapter invoke error={error}");
                    return Ok(None);
                }
                return Ok(self.io_writers.get(&sink_key).cloned());
            }
            return Ok(None);
        }
        // A `JsonElement` is written by its own adapter (`gson_tree.rs`).
        if let Some(text) = self.gson_tree_json(value, &style.tree_style(), depth)? {
            return Ok(text);
        }
        let text = match value {
            Value::Ref(None) | Value::Void => Some("null".to_owned()),
            // Arrays are reflected as JSON arrays, preserving primitive and
            // reference elements exactly as Gson's TypeAdapterRuntimeTypeWrapper.
            Value::Host(handle) if tree_mode && handle.kind() == crate::runtime::HostKind::Array => {
                // `array` borrows the runtime (a `Cow` over the backing since #338),
                // so own the elements before the writer takes `&mut self`.
                let values = self.array(value)?.into_owned();
                self.gson_array_text(&values, style, depth, tree_mode, gson_marker)?
            }
            // A host model's object (`Optional`): Gson reflects its private
            // fields, which JDK 25's module system refuses; not answered.
            Value::Host(handle) if !crate::runtime::jar_object::is_object_handle(handle) => None,
            // A `String` spelling an object's key is a JSON string.
            Value::Ref(Some(text)) if crate::runtime::jar_object::spells_key(value) => {
                Some(format!("\"{}\"", style.escape(text)))
            }
            // A boxed number or boolean is a JSON number or literal; a
            // `Character` is a one-character string.
            Value::Boxed(boxed @ Boxed::Character(_)) => {
                Some(format!("\"{}\"", style.escape(&boxed.text())))
            }
            // A `Float` box (a `float` field too): `Gson`'s float adapter
            // checks the value widened to a `double` before writing it.
            Value::Boxed(Boxed::Float(bits)) if !f32::from_bits(*bits).is_finite() => {
                Some(non_finite_text(f64::from(f32::from_bits(*bits)), style)?)
            }
            Value::Boxed(boxed) => Some(boxed.text()),
            Value::Int(value) => Some(value.to_string()),
            Value::Long(value) => Some(if style.long_strings() {
                format!("\"{value}\"")
            } else {
                value.to_string()
            }),
            Value::Float(value) => {
                if value.is_finite() {
                    Some(if tree_mode {
                        format_java_float(*value)
                    } else {
                        crate::runtime::jdk_float_text::double_to_string(*value)
                    })
                } else {
                    Some(non_finite_text(*value, style)?)
                }
            }
            // A marker, or an object by its key (`jar_object.rs`).
            Value::Ref(Some(_)) | Value::Host(_) => {
                let marker = expect_ref(value).map_or("", String::as_str);
                // One EnumTypeAdapter name path serves values and map keys.
                if let Some(name) = self.gson_enum_text(value) {
                    Some(format!("\"{}\"", style.escape(&name)))
                } else if let Some(kind) = gson_marker_kind(self.host, value, tree_mode) {
                    let values = runtime_collection_values(self.host, value)?;
                    if kind == "map" {
                        self.gson_object_text(&values, style, depth, tree_mode, gson_marker)?
                    } else {
                        self.gson_array_text(&values, style, depth, tree_mode, gson_marker)?
                    }
                } else if let Some(class) = crate::runtime::jar_object::object_class(value)
                    && self.class(class).is_some()
                {
                    self.gson_fields_text(class, marker, style, depth, tree_mode, gson_marker)?
                } else {
                    // A plain `String` value (and a marker the runtime only
                    // models as text) is a JSON string.
                    Some(format!("\"{}\"", style.escape(marker)))
                }
            }
        };
        Ok(text)
    }

    fn gson_array_text(
        &mut self,
        values: &[Value],
        style: &GsonStyle,
        depth: usize,
        tree_mode: bool,
        gson_marker: Option<&str>,
    ) -> Result<Option<String>, NativeError> {
        if values.is_empty() {
            return Ok(Some("[]".into()));
        }
        let mut out = String::from("[");
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                out.push(',');
                if style.formatting.space_after_separators && style.formatting.newline.is_empty() {
                    out.push(' ');
                }
            }
            out.push_str(&style.line(depth + 1));
            let Some(text) =
                self.gson_to_json_mode(value, style, depth + 1, tree_mode, gson_marker)?
            else {
                eprintln!("GSON_DEBUG recursive value={:?}", value);
                return Ok(None);
            };
            out.push_str(&text);
        }
        out.push_str(&style.line(depth));
        out.push(']');
        Ok(Some(out))
    }

    fn gson_object_text(
        &mut self,
        values: &[Value],
        style: &GsonStyle,
        depth: usize,
        tree_mode: bool,
        gson_marker: Option<&str>,
    ) -> Result<Option<String>, NativeError> {
        if !values.len().is_multiple_of(2) {
            return Err(NativeError::Metadata(
                "native map marker has an odd value count".into(),
            ));
        }
        if values.is_empty() {
            return Ok(Some("{}".into()));
        }
        let mut out = String::from("{");
        let mut first = true;
        for pair in values.chunks(2) {
            let (key, value) = (pair[0].clone(), pair[1].clone());
            // Gson's map adapter writes the name, then `JsonWriter.nullValue()` (and the tree
            // writer's `put`) drops that deferred name unless `serializeNulls`: the entry is gone.
            if !style.serialize_nulls() && matches!(value, Value::Ref(None)) {
                continue;
            }
            if !first {
                out.push(',');
                if style.formatting.space_after_separators && style.formatting.newline.is_empty() {
                    out.push(' ');
                }
            }
            first = false;
            out.push_str(&style.line(depth + 1));
            let Some(key_text) = self.gson_map_key_text(&key) else {
                return Ok(None);
            };
            out.push('"');
            out.push_str(&style.escape(&key_text));
            out.push('"');
            out.push_str(style.colon());
            let Some(text) =
                self.gson_to_json_mode(&value, style, depth + 1, tree_mode, gson_marker)?
            else {
                return Ok(None);
            };
            out.push_str(&text);
        }
        // Every entry dropped: `endObject` on an object with no name written is `{}`, with no
        // line break even when pretty printing.
        if first {
            return Ok(Some("{}".into()));
        }
        out.push_str(&style.line(depth));
        out.push('}');
        Ok(Some(out))
    }

    /// A jar object as Gson reflects it: its fields in declaration order, the
    /// `Excluder` modifier mask (which `excludeFieldsWithModifiers` replaces)
    /// and the naming policy, with `null` fields omitted unless
    /// `serializeNulls()` was set.
    fn gson_fields_text(
        &mut self,
        class: &str,
        marker: &str,
        style: &GsonStyle,
        depth: usize,
        tree_mode: bool,
        gson_marker: Option<&str>,
    ) -> Result<Option<String>, NativeError> {
        if tree_mode && self.class(class).is_some_and(ClassFile::is_record) {
            return Ok(None);
        }
        let mut members: Vec<(String, Value)> = Vec::new();
        for (declaring, name, descriptor, access) in self.gson_fields(class) {
            if !style.includes_field(access) {
                continue;
            }
            let field = MethodRef {
                owner: declaring.clone(),
                name: name.clone(),
                descriptor: descriptor.clone(),
            };
            let value = if access & 0x0008 != 0 {
                self.stored_static(&field)?
            } else {
                self.fields
                    .get(&format!("{marker}|{declaring}#{name}{descriptor}"))
                    .cloned()
            }
            .unwrap_or_else(|| Value::default_for(&descriptor));
            if !style.serialize_nulls() && matches!(value, Value::Ref(None)) {
                continue;
            }
            let name = if tree_mode {
                self.gson_field_name(&declaring, &name, &descriptor, style)
            } else {
                style.field_name(&name)
            };
            members.push((name, primitive_box(&descriptor, value)));
        }
        if members.is_empty() {
            return Ok(Some("{}".into()));
        }
        let mut out = String::from("{");
        for (index, (name, value)) in members.iter().enumerate() {
            if index > 0 {
                out.push(',');
                if style.formatting.space_after_separators && style.formatting.newline.is_empty() {
                    out.push(' ');
                }
            }
            out.push_str(&style.line(depth + 1));
            out.push('"');
            out.push_str(&style.escape(name));
            out.push('"');
            out.push_str(style.colon());
            let Some(text) =
                self.gson_to_json_mode(value, style, depth + 1, tree_mode, gson_marker)?
            else {
                eprintln!("GSON_DEBUG recursive value={:?}", value);
                return Ok(None);
            };
            out.push_str(&text);
        }
        out.push_str(&style.line(depth));
        out.push('}');
        Ok(Some(out))
    }

    fn gson_enum_name(&self, class: &str, name: &str) -> Option<String> {
        let file = self.class(class)?;
        let field = file
            .fields
            .iter()
            .find(|field| field.name == name && field.descriptor == format!("L{class};"))?;
        file.field_annotations(&field.name, &field.descriptor)
            .into_iter()
            .find(|annotation| {
                annotation.descriptor == "Lcom/google/gson/annotations/SerializedName;"
            })
            .and_then(|annotation| match annotation.values.get("value") {
                Some(crate::runtime::AnnotationValue::String(value)) => Some(value.clone()),
                _ => None,
            })
    }

    /// The enum class whose constant field carries Gson's @SerializedName.
    ///
    /// A constant-specific enum body is an anonymous subclass (for example
    /// Probe$Split$1), but EnumTypeAdapter uses the declaring enum class
    /// (Probe$Split) when it reads the constant field annotation.
    fn gson_enum_declaring_class(&self, class: &str) -> Option<String> {
        let mut current = class.to_owned();
        loop {
            let file = self.class(&current)?;
            if file.super_name == "java/lang/Enum" {
                return Some(current);
            }
            if file.super_name.is_empty() || file.super_name == current {
                return None;
            }
            current = file.super_name.clone();
        }
    }

    fn gson_enum_text(&self, value: &Value) -> Option<String> {
        let marker = expect_ref(value)?;
        let (name, class) = if let Some((name, _)) = self.enum_constants.get(marker) {
            let class = crate::runtime::jar_object::object_class(value)
                .and_then(|class| self.gson_enum_declaring_class(class));
            (name.clone(), class)
        } else {
            let rest = marker.strip_prefix("enum/")?;
            let (class, name) = rest.rsplit_once('/')?;
            (name.to_owned(), Some(class.to_owned()))
        };
        class
            .as_deref()
            .and_then(|class| self.gson_enum_name(class, &name))
            .or(Some(name))
    }

    fn gson_map_key_text(&self, key: &Value) -> Option<String> {
        if let Some(marker) = expect_ref(key)
            && (marker.starts_with("enum/") || self.enum_constants.contains_key(marker))
        {
            return self.gson_enum_text(key);
        }
        Some(value_text(key))
    }

    fn gson_field_name(
        &self,
        declaring: &str,
        name: &str,
        descriptor: &str,
        style: &GsonStyle,
    ) -> String {
        let serialized = self
            .class(declaring)
            .into_iter()
            .flat_map(|file| file.field_annotations(name, descriptor))
            .find(|annotation| {
                annotation.descriptor == "Lcom/google/gson/annotations/SerializedName;"
            })
            .and_then(|annotation| match annotation.values.get("value") {
                Some(crate::runtime::AnnotationValue::String(value)) => Some(value.clone()),
                _ => None,
            });
        serialized.unwrap_or_else(|| style.field_name(name))
    }

    /// Every declared field of `class` and its superclasses with its access
    /// flags, the class's own fields first, exactly Gson's reflection order.
    /// Static fields are included: whether they are written is the `Excluder`
    /// mask's decision.
    pub(super) fn gson_fields(&self, class: &str) -> Vec<(String, String, String, u16)> {
        let mut fields: Vec<(String, String, String, u16)> = Vec::new();
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(file) = self.class(&current) else {
                break;
            };
            for field in &file.fields {
                if fields.iter().any(|(_, _, name, _)| name == &field.name) {
                    continue;
                }
                fields.push((
                    current.clone(),
                    field.name.clone(),
                    field.descriptor.clone(),
                    field.access,
                ));
            }
            if file.super_name.is_empty() || file.super_name == current {
                break;
            }
            current = file.super_name.clone();
        }
        fields
    }
}

/// `java.lang.NullPointerException`, as Gson's `Objects.requireNonNull` raises
/// it for a null builder argument.
fn npe_with(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn illegal_argument(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

/// Gson's `registerTypeAdapter` refusal for an object that implements none of
/// its adapter interfaces.
fn not_a_supported_adapter(class: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(format!(
            "Class {} does not implement any supported type adapter class or interface",
            class.replace('/', ".")
        )),
        frame: None,
    }
}

/// The non-executable JSON prefix one `Gson` writes.
pub(super) fn non_executable_prefix(style: &GsonStyle) -> Option<&'static str> {
    style.non_executable().then_some(NON_EXECUTABLE_PREFIX)
}

/// Whether one marker is a collection this host can read, and its shape.
///
/// A neutral marker names its kind directly; a `new`-backed marker is a class
/// name whose list/set/map family decides how its pairs or elements serialize.
fn gson_marker_kind<H: Host>(host: &H, value: &Value, tree_mode: bool) -> Option<&'static str> {
    let marker = expect_ref(value).map(String::as_str).or_else(|| {
        tree_mode
            .then(|| crate::runtime::jar_object::object_key(value))
            .flatten()
    })?;
    if host.holds_collection(value) {
        let class = marker.split_once('#').map_or(marker, |(class, _)| class);
        // A map whichever class the host made it (Guava's
        // `RegularImmutableMap`, `Map.of`'s `MapN`): its supertypes say
        // (#253 review).
        return if is_map_owner(class)
            || crate::runtime::java_immutable::supertypes(class).contains(&"java/util/Map")
        {
            Some("map")
        } else {
            Some("list")
        };
    }
    // A collection is a handle the host holds (#156 P14d2): no text is one.
    None
}

/// A primitive field's value as Gson's adapter for its type writes it: the
/// interpreter holds a `boolean` and a `char` as an `int`, so `true` would
/// print `1` and `'a'` `97` (#253 re-check: a config written with booleans
/// then failed to read back), the same two rows as #307's writer; and a
/// `float` as a `double`, so `0.1f` would print `0.10000000149011612`: it is
/// boxed, and the box goes through Gson's NaN/infinity check (#253 re-check
/// B7). A `byte`/`short` prints the same either way.
fn primitive_box(descriptor: &str, value: Value) -> Value {
    match (descriptor, &value) {
        ("Z", Value::Int(int)) => Value::Boxed(Boxed::Boolean(*int != 0)),
        ("C", Value::Int(int)) => Value::Boxed(Boxed::Character(*int as u16)),
        ("F", Value::Float(float)) => Value::Boxed(Boxed::float(*float)),
        _ => value,
    }
}

/// A NaN or infinite `double` (or `float`, widened) as Gson writes it:
/// `NaN`/`Infinity`/`-Infinity` under `serializeSpecialFloatingPointValues`,
/// else `Gson.checkValidFloatingPoint`'s `IllegalArgumentException`, naming
/// the value as `Double.toString` prints it (#253 re-check B7).
fn non_finite_text(value: f64, style: &GsonStyle) -> Result<String, NativeError> {
    let shown = if value.is_nan() {
        "NaN"
    } else if value > 0.0 {
        "Infinity"
    } else {
        "-Infinity"
    };
    if style.special_floats() {
        return Ok(shown.into());
    }
    Err(NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(format!(
            "{shown} is not a valid double value as per JSON specification. \
             To override this behavior, use \
             GsonBuilder.serializeSpecialFloatingPointValues() method."
        )),
        frame: None,
    })
}
