//! Styles on the client surface: `Style`, `TextColor` and the
//! `ChatFormatting` constants a client callback styles text with.
//!
//! A component's own style lives in the interpreter's component model
//! (`facade_text.rs`): `withStyle(ChatFormatting)`, `withColor(int)` and
//! `append` build vanilla's component JSON there (`host_component.rs`).
//! What that model leaves to the host is the style *objects*, and those are
//! answered here: `Style.EMPTY` and every derived style is
//! `client/style/<codes>`, its formatting kept as legacy codes (an RGB
//! colour as the client-only `§#rrggbb`); `TextColor.fromRgb(rgb)` /
//! `fromLegacyFormat(f)` is `client/text-color/<code>`. A component styled
//! with one (`withStyle(style)`, `setStyle(style)`) changes in place to the
//! same JSON the façade builds, with the style's keys set.
//!
//! `ChatFormatting.<NAME>.toString()` is its code (`§6`), so
//! `ChatFormatting.GOLD + "Day"` is a legacy-coded string, which every client
//! drawing path formats by.

use crate::runtime::client_values::{Colour, FLAGS, STYLE, Style, TEXT_COLOR};
use crate::runtime::{Host, MethodRef, NativeError, Runtime, Value, expect_ref, return_desc};

use super::{flag, int_arg, marker};

const FORMATTING: &str = "net/minecraft/ChatFormatting";
const FORMATTING_MARKER: &str = "enum/net/minecraft/ChatFormatting/";

/// `ChatFormatting`: name, code, colour (RGB) for the colours.
const FORMATS: &[(&str, char, Option<u32>)] = &[
    ("BLACK", '0', Some(0x000000)),
    ("DARK_BLUE", '1', Some(0x0000aa)),
    ("DARK_GREEN", '2', Some(0x00aa00)),
    ("DARK_AQUA", '3', Some(0x00aaaa)),
    ("DARK_RED", '4', Some(0xaa0000)),
    ("DARK_PURPLE", '5', Some(0xaa00aa)),
    ("GOLD", '6', Some(0xffaa00)),
    ("GRAY", '7', Some(0xaaaaaa)),
    ("DARK_GRAY", '8', Some(0x555555)),
    ("BLUE", '9', Some(0x5555ff)),
    ("GREEN", 'a', Some(0x55ff55)),
    ("AQUA", 'b', Some(0x55ffff)),
    ("RED", 'c', Some(0xff5555)),
    ("LIGHT_PURPLE", 'd', Some(0xff55ff)),
    ("YELLOW", 'e', Some(0xffff55)),
    ("WHITE", 'f', Some(0xffffff)),
    ("OBFUSCATED", 'k', None),
    ("BOLD", 'l', None),
    ("STRIKETHROUGH", 'm', None),
    ("UNDERLINE", 'n', None),
    ("ITALIC", 'o', None),
    ("RESET", 'r', None),
];

fn format_named(name: &str) -> Option<&'static (&'static str, char, Option<u32>)> {
    FORMATS.iter().find(|(n, ..)| *n == name)
}

/// The code of a `ChatFormatting` marker (`6`, `l`).
fn formatting_code(value: &Value) -> Option<char> {
    let name = expect_ref(value)?.strip_prefix(FORMATTING_MARKER)?;
    format_named(name).map(|(_, code, _)| *code)
}

/// `withColor`'s argument: an `int` (`TextColor.fromRgb`), a `TextColor`, a
/// `ChatFormatting` (`TextColor.fromLegacyFormat`: null for a format), or
/// null.
fn colour_arg(value: &Value) -> Option<Option<Colour>> {
    match value {
        Value::Int(rgb) => Some(Some(Colour::Rgb(*rgb as u32 & 0xff_ffff))),
        Value::Ref(None) => Some(None),
        _ => match crate::runtime::game_object::text(value)?.strip_prefix(TEXT_COLOR) {
            Some(code) => Some(Some(Colour::parse(code)?)),
            None => Some(Colour::parse(&format!("§{}", formatting_code(value)?))),
        },
    }
}

/// A `Boolean` argument (`withBold(true)` boxes it): `None` inside for null.
fn boolean_arg(value: &Value) -> Option<Option<bool>> {
    match value {
        Value::Int(v) => Some(Some(*v != 0)),
        Value::Boxed(crate::runtime::Boxed::Boolean(v)) => Some(Some(*v)),
        Value::Ref(None) => Some(None),
        _ => None,
    }
}

/// The style keys a client style marker's `codes` set.
const STYLE_KEYS: [&str; 6] = [
    "color",
    "bold",
    "italic",
    "underlined",
    "strikethrough",
    "obfuscated",
];

/// A style's `codes` applied to a component's JSON `object`: over its own
/// style as `withStyle(Style)` merges them (`Style.applyTo`: the style's set
/// fields win, an explicit `false` included), or in its place (`setStyle`).
pub(super) fn apply_style(
    object: &mut serde_json::Map<String, serde_json::Value>,
    codes: &str,
    replace: bool,
) {
    let Some(style) = Style::parse(codes) else {
        return;
    };
    if replace {
        for key in STYLE_KEYS {
            object.remove(key);
        }
    }
    if let Some(colour) = style.colour {
        object.insert("color".into(), colour.text().into());
    }
    for ((_, key), value) in FLAGS.iter().zip(style.flags) {
        if let Some(value) = value {
            object.insert((*key).into(), value.into());
        }
    }
}

/// Static fields: `ChatFormatting.<NAME>` and `Style.EMPTY`.
pub(super) fn text_static(r: &MethodRef) -> Option<Value> {
    match (r.owner.as_str(), r.name.as_str()) {
        (FORMATTING, name) if format_named(name).is_some() => {
            Some(marker(format!("{FORMATTING_MARKER}{name}")))
        }
        ("net/minecraft/network/chat/Style", "EMPTY") => Some(marker(STYLE)),
        _ => None,
    }
}

/// Static calls: `TextColor.fromRgb`, `TextColor.fromLegacyFormat`,
/// `ChatFormatting.getByName`/`getByCode`.
pub(super) fn text_invoke(r: &MethodRef, args: &[Value]) -> Option<Value> {
    match (r.owner.as_str(), r.name.as_str()) {
        ("net/minecraft/network/chat/TextColor", "parseColor") => {
            let text = expect_ref(args.first()?)?;
            let colour = if let Some(hex) = text.strip_prefix('#') {
                // TextColor.parseColor delegates to Integer.parseInt(hex, 16)
                // and accepts any spelling whose value fits 24 bits (including
                // leading zeroes), rather than requiring six digits.
                let significant = hex.trim_start_matches('0');
                let value = if hex.is_empty() {
                    None
                } else if significant.is_empty() {
                    Some(0)
                } else if significant.chars().all(|c| c.is_ascii_hexdigit()) {
                    u32::from_str_radix(significant, 16).ok()
                } else {
                    None
                };
                value.filter(|value| *value <= 0xff_ffff).map(Colour::Rgb)
            } else {
                Colour::from_text(text)
            };
            Some(marker(match colour {
                Some(colour) => format!("style-result/success/{}", colour.code()),
                None => "style-result/error".into(),
            }))
        }
        ("net/minecraft/network/chat/TextColor", "fromRgb") => Some(marker(format!(
            "{TEXT_COLOR}{}",
            Colour::Rgb(int_arg(args, 0)? as u32 & 0xff_ffff).code()
        ))),
        // A named colour's constant; null for a format (`BOLD`).
        ("net/minecraft/network/chat/TextColor", "fromLegacyFormat") => Some(
            match Colour::parse(&format!("§{}", formatting_code(args.first()?)?)) {
                Some(colour) => marker(format!("{TEXT_COLOR}{}", colour.code())),
                None => Value::Ref(None),
            },
        ),
        (FORMATTING, "getByName") => {
            let name = expect_ref(args.first()?)?.to_ascii_uppercase();
            Some(match format_named(&name) {
                Some((name, ..)) => marker(format!("{FORMATTING_MARKER}{name}")),
                None => Value::Ref(None),
            })
        }
        (FORMATTING, "getByCode") => {
            let code = char::from_u32(int_arg(args, 0)? as u32)?.to_ascii_lowercase();
            Some(match FORMATS.iter().find(|(_, c, _)| *c == code) {
                Some((name, ..)) => marker(format!("{FORMATTING_MARKER}{name}")),
                None => Value::Ref(None),
            })
        }
        _ => None,
    }
}

/// Instance calls on components, styles, text colours and formatting
/// constants; `None` when the receiver is none of them or the member is not
/// listed.
/// Instance calls on a component (`host_component.rs`): `withStyle`/`setStyle`
/// with a style object and `getStyle`. `withStyle(ChatFormatting)` and
/// `withColor(int)` are the interpreter's (`facade_text.rs`).
/// What a client style call on a component does.
pub(super) enum ComponentCall {
    /// Change the component's style in place to these codes (`replace`:
    /// `setStyle`), then answer the component itself.
    Restyle { codes: String, replace: bool },
    /// Answer the component's style as it is now ([`style_of`]).
    GetStyle,
    /// A style with a field the model does not keep (a click event...):
    /// applying it would drop the field, so the call is counted.
    Unknown,
}

/// The client style of a component's JSON style keys (its fields, in the
/// key's one order), as `getStyle()` answers it now.
pub(super) fn style_of(json: &serde_json::Value) -> Value {
    let mut style = Style {
        colour: json
            .get("color")
            .and_then(serde_json::Value::as_str)
            .and_then(Colour::from_text),
        ..Style::default()
    };
    for ((_, key), value) in FLAGS.iter().zip(&mut style.flags) {
        *value = json.get(*key).and_then(serde_json::Value::as_bool);
    }
    style.extra = [
        "click_event",
        "hover_event",
        "insertion",
        "font",
        "shadow_color",
    ]
    .iter()
    .any(|key| json.get(key).is_some());
    marker(format!("{STYLE}{}", style.codes()))
}

pub(super) fn component_call(r: &MethodRef, args: &[Value]) -> Option<ComponentCall> {
    match (r.name.as_str(), return_desc(&r.descriptor)) {
        (name @ ("withStyle" | "setStyle"), _) => {
            let codes = crate::runtime::game_object::text(args.first()?)?.strip_prefix(STYLE)?;
            if Style::parse(codes)?.extra {
                return Some(ComponentCall::Unknown);
            }
            Some(ComponentCall::Restyle {
                codes: codes.to_owned(),
                replace: name == "setStyle",
            })
        }
        ("getStyle", _) => Some(ComponentCall::GetStyle),
        _ => None,
    }
}

/// A call on a style, a text colour or a `ChatFormatting` constant;
/// `receiver` is the object itself, which a `with…` that changes nothing
/// hands back (Java's `return this`).
pub(crate) fn runtime_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    receiver: Option<&Value>,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if let Some(result) = receiver
        .and_then(crate::runtime::game_object::text)
        .and_then(|text| text.strip_prefix("style-result/"))
    {
        let (success, payload) = match result.strip_prefix("success/") {
            Some(code) => (true, Some(code)),
            None if result == "error" => (false, None),
            None => return None,
        };
        return Some(Ok(match (r.name.as_str(), r.descriptor.as_str()) {
            ("result", "()Ljava/util/Optional;") => {
                crate::runtime::host_optional::optional_wrap(match (payload, success) {
                    (Some(code), true) => marker(format!("{TEXT_COLOR}{code}")),
                    _ => Value::Ref(None),
                })
            }
            ("isError", "()Z") => flag(!success),
            _ => return None,
        }));
    }
    if r.owner != "net/minecraft/network/chat/Style"
        || r.name != "applyFormats"
        || r.descriptor != "([Lnet/minecraft/ChatFormatting;)Lnet/minecraft/network/chat/Style;"
    {
        return None;
    }
    let object = receiver.and_then(crate::runtime::game_object::text)?.clone();
    let codes = object.strip_prefix(STYLE)?;
    let array = args.first()?;
    let formats = runtime.array(array).ok()?;
    Some(match apply_formats(codes, &formats) {
        Some(value) => Ok(value),
        None => Err(NativeError::UnknownHostCall(r.clone())),
    })
}

pub(super) fn apply_formats(codes: &str, formats: &[Value]) -> Option<Value> {
    let mut style = Style::parse(codes)?;
    for value in formats {
        style = style.apply(formatting_code(value)?, false)?;
        if style == Style::default() {
            return Some(marker(STYLE));
        }
    }
    Some(marker(format!("{STYLE}{}", style.codes())))
}

pub(super) fn text_call(
    object: &str,
    receiver: &Value,
    r: &MethodRef,
    args: &[Value],
) -> Option<Value> {
    let name = r.name.as_str();
    let returns = return_desc(&r.descriptor);
    if let Some(codes) = object.strip_prefix(STYLE) {
        let style = Style::parse(codes)?;
        let setter = [
            "withBold",
            "withItalic",
            "withUnderlined",
            "withStrikethrough",
            "withObfuscated",
        ]
        .iter()
        .position(|setter| *setter == name);
        let next = match (name, returns) {
            ("isEmpty", "Z") => return Some(flag(codes.is_empty())),
            ("getColor", _) => {
                return Some(style.colour.map_or(Value::Ref(None), |colour| {
                    marker(format!("{TEXT_COLOR}{}", colour.code()))
                }));
            }
            ("getShadowColor", _) => {
                // A style without an unsupported field has no shadow color;
                // a shadow-bearing style is counted because its field is not
                // represented separately in the handle key.
                if style.extra {
                    return None;
                }
                return Some(Value::Ref(None));
            }
            ("isBold" | "isItalic" | "isUnderlined" | "isStrikethrough" | "isObfuscated", "Z") => {
                let at = match name {
                    "isBold" => 0,
                    "isItalic" => 1,
                    "isUnderlined" => 2,
                    "isStrikethrough" => 3,
                    _ => 4,
                };
                return Some(flag(style.flags[at] == Some(true)));
            }
            ("getClickEvent" | "getHoverEvent" | "getInsertion", _) => {
                if style.extra {
                    return None;
                }
                return Some(Value::Ref(None));
            }
            ("withColor", _) => match colour_arg(args.first()?)? {
                colour if colour.map(Colour::value) == style.colour.map(Colour::value) => style,
                colour => Style { colour, ..style },
            },
            ("withClickEvent" | "withHoverEvent" | "withInsertion" | "withFont", _) => {
                if matches!(args.first(), Some(Value::Ref(None))) && !style.extra {
                    return Some(receiver.clone());
                }
                return None;
            }
            ("applyFormat" | "applyLegacyFormat", _) => {
                style.apply(formatting_code(args.first()?)?, name == "applyLegacyFormat")?
            }
            ("applyTo", _) => {
                let other_text = crate::runtime::game_object::text(args.first()?)?;
                let other = Style::parse(other_text.strip_prefix(STYLE)?)?;
                if style == Style::default() {
                    return Some(args.first()?.clone());
                }
                if other == Style::default() {
                    return Some(receiver.clone());
                }
                style.apply_to(other)
            }
            _ => {
                let mut next = style;
                next.flags[setter?] = boolean_arg(args.first()?)?;
                next
            }
        };
        if next == style && name.starts_with("with") {
            return Some(receiver.clone());
        }
        return Some(marker(format!("{STYLE}{}", next.codes())));
    }
    if let Some(code) = object.strip_prefix(TEXT_COLOR) {
        return match (name, returns) {
            ("getValue", "I") => Some(Value::Int(Colour::parse(code)?.value() as i32)),
            ("serialize" | "toString", _) => Some(Value::Ref(Some(Colour::parse(code)?.text()))),
            _ => None,
        };
    }
    let format = object
        .strip_prefix(FORMATTING_MARKER)
        .and_then(format_named)?;
    let (fname, code, colour) = *format;
    match (name, returns) {
        ("toString", _) => Some(marker(format!("§{code}"))),
        ("getChar", "C") => Some(Value::Int(code as i32)),
        ("getName" | "getSerializedName", _) => Some(marker(fname.to_ascii_lowercase())),
        ("isColor", "Z") => Some(flag(colour.is_some())),
        ("isFormat", "Z") => Some(flag(colour.is_none() && code != 'r')),
        // A boxed `Integer`, null for a format.
        ("getColor", _) => Some(match colour {
            Some(rgb) => Value::Boxed(crate::runtime::Boxed::Integer(rgb as i32)),
            None => Value::Ref(None),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method(owner: &str, name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    #[test]
    fn a_style_object_styles_a_component_as_vanilla_json() {
        let with_color = method(
            "net/minecraft/network/chat/Style",
            "withColor",
            "(I)Lnet/minecraft/network/chat/Style;",
        );
        let empty = marker(STYLE);
        let style = text_call(STYLE, &empty, &with_color, &[Value::Int(0x33ebff)]).unwrap();
        assert_eq!(style, marker("client/style/§#33ebff"));
        let bold = text_call(
            crate::runtime::game_object::text(&style).unwrap(),
            &style,
            &method(
                "net/minecraft/network/chat/Style",
                "withBold",
                "(Ljava/lang/Boolean;)Lnet/minecraft/network/chat/Style;",
            ),
            &[Value::Boxed(crate::runtime::Boxed::Boolean(true))],
        )
        .unwrap();
        let with_style = method(
            "net/minecraft/network/chat/MutableComponent",
            "withStyle",
            "(Lnet/minecraft/network/chat/Style;)Lnet/minecraft/network/chat/MutableComponent;",
        );
        let Some(ComponentCall::Restyle { codes, replace }) = component_call(&with_style, &[bold])
        else {
            panic!("not a restyle")
        };
        let mut object = serde_json::Map::from_iter([("text".to_owned(), "Day 3".into())]);
        apply_style(&mut object, &codes, replace);
        assert_eq!(
            serde_json::Value::Object(object),
            serde_json::json!({"text": "Day 3", "color": "#33EBFF", "bold": true})
        );
        // `ChatFormatting.GOLD + "x"` concatenates its code.
        let gold = text_static(&method(
            FORMATTING,
            "GOLD",
            "Lnet/minecraft/ChatFormatting;",
        ))
        .unwrap();
        let to_string = method(FORMATTING, "toString", "()Ljava/lang/String;");
        assert_eq!(
            text_call(expect_ref(&gold).unwrap(), &gold, &to_string, &[]),
            Some(marker("§6"))
        );
    }

    /// `getStyle()` is the component's style now: what `withStyle` set, in one order however it
    /// was built, the empty style before any, and a counted one with a field the model lacks.
    #[test]
    fn style_builders_keep_identity_and_apply_fields() {
        let empty = marker(STYLE);
        let bold = method(
            "net/minecraft/network/chat/Style",
            "withBold",
            "(Ljava/lang/Boolean;)Lnet/minecraft/network/chat/Style;",
        );
        assert_eq!(
            text_call(
                crate::runtime::game_object::text(&empty).unwrap(),
                &empty,
                &bold,
                &[Value::Ref(None)]
            ),
            Some(empty.clone())
        );
        let color = method(
            "net/minecraft/network/chat/Style",
            "withColor",
            "(I)Lnet/minecraft/network/chat/Style;",
        );
        let colored = text_call(
            crate::runtime::game_object::text(&empty).unwrap(),
            &empty,
            &color,
            &[Value::Int(0x123456)],
        )
        .unwrap();
        let get_color = method(
            "net/minecraft/network/chat/Style",
            "getColor",
            "()Lnet/minecraft/network/chat/TextColor;",
        );
        assert_eq!(
            text_call(
                crate::runtime::game_object::text(&colored).unwrap(),
                &colored,
                &get_color,
                &[]
            ),
            Some(marker("client/text-color/§#123456"))
        );
        let apply_to = method(
            "net/minecraft/network/chat/Style",
            "applyTo",
            "(Lnet/minecraft/network/chat/Style;)Lnet/minecraft/network/chat/Style;",
        );
        assert_eq!(
            text_call(
                crate::runtime::game_object::text(&empty).unwrap(),
                &empty,
                &apply_to,
                std::slice::from_ref(&colored),
            ),
            Some(colored)
        );
        assert_eq!(
            apply_formats(
                "",
                &[
                    marker("enum/net/minecraft/ChatFormatting/BOLD"),
                    marker("enum/net/minecraft/ChatFormatting/RED"),
                ],
            ),
            Some(marker("client/style/§c§l"))
        );
    }

    #[test]
    fn text_color_parse_color_returns_a_result_handle() {
        let parse = method(
            "net/minecraft/network/chat/TextColor",
            "parseColor",
            "(Ljava/lang/String;)Lcom/mojang/serialization/DataResult;",
        );
        assert_eq!(
            text_invoke(&parse, &[Value::Ref(Some("#123456".into()))]),
            Some(marker("style-result/success/§#123456"))
        );
        assert_eq!(
            text_invoke(&parse, &[Value::Ref(Some("missing".into()))]),
            Some(marker("style-result/error"))
        );
    }

    #[test]
    fn a_component_style_is_its_current_style() {
        let plain = serde_json::json!({"text": "a"});
        assert_eq!(style_of(&plain), marker(STYLE));
        let styled = serde_json::json!({"text": "a", "bold": true, "color": "dark_red"});
        assert_eq!(style_of(&styled), marker(format!("{STYLE}§4§l")));
        let rgb = serde_json::json!({"text": "a", "color": "#33EBFF", "italic": false});
        assert_eq!(style_of(&rgb), marker(format!("{STYLE}§#33ebff§!o")));
        let click = serde_json::json!({"text": "a", "insertion": "x"});
        assert_eq!(style_of(&click), marker(format!("{STYLE}§?")));
    }
}
