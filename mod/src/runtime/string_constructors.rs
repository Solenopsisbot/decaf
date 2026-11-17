//! `new String(...)`: counted, never answered with the allocation marker.
//!
//! `new java/lang/String` pushes an object marker (`java/lang/String#n`), and
//! no model turns that marker into the text the constructor builds. Every
//! overload therefore left the marker in place of the string: on `main`,
//! `new String(new char[] {'a', 'b'})` answered an 18-character string whose
//! `equals("ab")` was `false`, and `new String("xy")`, `new String()`,
//! `new String(byte[], UTF_8)` and `new String(StringBuilder)` answered the
//! same marker, all without a count, so the run still read clean.
//!
//! [`build`] now builds the text for the `char[]` and code-point constructors,
//! as JDK 25 does, and the interpreter rewrites the allocation marker to that
//! text wherever the frame holds it (the `dup`ed copy on the operand stack, and
//! the locals), so the string is a string from then on. night-config's TOML
//! parser builds every key with `new String(char[], int, int)`; with the
//! marker in its place a key read `java/lang/String#477`, and every Forge
//! Config API Port user on Fabric failed to parse its config (#124).
//!
//! From JDK 25 (`java.lang.String`, Temurin 25.0.5+8 `src.zip`, checked on a
//! JVM):
//!
//! * `String(char[] value)` and `String(char[] value, int offset, int count)`
//!   copy the units (a later write to the array does not change the string).
//!   A null array throws `NullPointerException: Cannot read the array length
//!   because "value" is null`; a bad range throws
//!   `StringIndexOutOfBoundsException: Range [offset, offset + count) out of
//!   bounds for length N`.
//! * `String(int[] codePoints, int offset, int count)` checks the range the
//!   same way (`"codePoints"` in the NPE), then each code point: an invalid one
//!   throws `IllegalArgumentException` with the code point in decimal.
//!
//! * `String()` and `String(String)` remain counted because each constructs a
//!   distinct object whose identity the text value cannot preserve.
//! * The `byte[]` constructors decode with the charset they name, or UTF-8
//!   (the default charset since JDK 18) when they name none. The charset is
//!   checked first: a null `Charset` or name throws `NullPointerException`
//!   with no message, an unknown name `UnsupportedEncodingException`. Then a
//!   null array throws `NullPointerException: Cannot read the array length
//!   because "bytes" is null`, and a bad range the same
//!   `StringIndexOutOfBoundsException` as above.
//!
//! The `byte[]` constructors are answered for ISO-8859-1 (one byte, one
//! char), US-ASCII (a byte over 0x7F is U+FFFD) and UTF-8. UTF-8 malformed
//! input uses JDK 25's replacement lengths; other charsets (UTF-16 and a name
//! the host does not model) stay counted.
//!
//! Every other constructor (`String()`, `String(String)`, `StringBuilder`/
//! `StringBuffer`, and the deprecated hibyte `(byte[], int)` pair) is still
//! counted. So is a `char[]`/`int[]`
//! constructor whose text has an unpaired surrogate, which the runtime's
//! strings cannot hold. A `String` constructor reached through a
//! `MethodHandle` (`String::new`) is answered or counted the same way.

use super::*;

fn thrown(class: &str, message: String) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: Some(message),
        frame: None,
    }
}

/// `Objects.checkFromIndexSize(offset, count, length)` as `String` reports it.
fn check_range(offset: i32, count: i32, length: usize) -> Result<(), NativeError> {
    let length_i = length as i64;
    let (offset_i, count_i) = (i64::from(offset), i64::from(count));
    if offset_i < 0 || count_i < 0 || offset_i + count_i > length_i {
        return Err(thrown(
            "java/lang/StringIndexOutOfBoundsException",
            format!("Range [{offset}, {offset} + {count}) out of bounds for length {length}"),
        ));
    }
    Ok(())
}

/// The charsets whose `byte[]` decoding is exact here.
#[derive(Clone, Copy)]
enum ExactCharset {
    Utf8,
    UsAscii,
    Latin1,
}

impl ExactCharset {
    fn named(canonical: &str) -> Option<Self> {
        match canonical {
            "UTF-8" => Some(Self::Utf8),
            "US-ASCII" => Some(Self::UsAscii),
            "ISO-8859-1" => Some(Self::Latin1),
            _ => None,
        }
    }

    fn decode(self, bytes: &[u8]) -> String {
        match self {
            Self::Utf8 => decode_utf8_java(bytes),
            Self::UsAscii => bytes
                .iter()
                .map(|byte| {
                    if byte.is_ascii() {
                        char::from(*byte)
                    } else {
                        '\u{FFFD}'
                    }
                })
                .collect(),
            Self::Latin1 => bytes.iter().map(|byte| char::from(*byte)).collect(),
        }
    }
}

/// Decode UTF-8 with the JDK's malformed-sequence lengths and replacement.
/// A malformed lead consumes one byte, a bad third/fourth byte consumes the
/// valid prefix, and a structurally complete surrogate sequence consumes all
/// three bytes. This is the `sun.nio.cs.UTF_8` decoder's replacement contract.
fn decode_utf8_java(bytes: &[u8]) -> String {
    fn continuation(byte: u8) -> bool {
        byte & 0xc0 == 0x80
    }
    fn structural_second_3(first: u8, second: u8) -> bool {
        match first {
            0xe0 => (0xa0..=0xbf).contains(&second),
            0xe1..=0xef => continuation(second),
            _ => false,
        }
    }
    fn codepoint_second_3(first: u8, second: u8) -> bool {
        structural_second_3(first, second) && !(first == 0xed && second >= 0xa0)
    }
    fn valid_second_4(first: u8, second: u8) -> bool {
        match first {
            0xf0 => (0x90..=0xbf).contains(&second),
            0xf4 => (0x80..=0x8f).contains(&second),
            0xf1..=0xf3 => continuation(second),
            _ => false,
        }
    }
    // sun.nio.cs.UTF_8.Decoder.malformedN: the malformed result consumes the
    // valid prefix, while a non-continuation at the second byte consumes only
    // the lead. This is observable because String replaces one malformed
    // result with one U+FFFD.
    fn malformed_3(first: u8, second: u8) -> usize {
        if (first == 0xe0 && second & 0xe0 == 0x80) || !continuation(second) {
            1
        } else {
            2
        }
    }
    fn malformed_4(first: u8, second: u8, third: u8) -> usize {
        if first > 0xf4 || !valid_second_4(first, second) {
            1
        } else if !continuation(third) {
            2
        } else {
            3
        }
    }

    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index];
        if first <= 0x7f {
            out.push(char::from(first));
            index += 1;
            continue;
        }
        let needed = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => {
                out.push('\u{FFFD}');
                index += 1;
                continue;
            }
        };
        let remaining = bytes.len() - index;
        if needed == 2 {
            if remaining < 2 {
                out.push('\u{FFFD}');
                index += remaining;
                continue;
            }
            let second = bytes[index + 1];
            if !continuation(second) {
                out.push('\u{FFFD}');
                index += 1;
                continue;
            }
            let code = (u32::from(first & 0x1f) << 6) | u32::from(second & 0x3f);
            out.push(char::from_u32(code).unwrap());
            index += 2;
            continue;
        }
        if needed == 3 {
            if remaining < 3 {
                let malformed = if remaining > 1 && !structural_second_3(first, bytes[index + 1]) {
                    malformed_3(first, bytes[index + 1])
                } else {
                    remaining
                };
                out.push('\u{FFFD}');
                index += malformed;
                continue;
            }
            let second = bytes[index + 1];
            let third = bytes[index + 2];
            if !structural_second_3(first, second) || !continuation(third) {
                out.push('\u{FFFD}');
                index += if !structural_second_3(first, second) {
                    malformed_3(first, second)
                } else {
                    2
                };
                continue;
            }
            let code = (u32::from(first & 0x0f) << 12)
                | (u32::from(second & 0x3f) << 6)
                | u32::from(third & 0x3f);
            if !codepoint_second_3(first, second) {
                out.push('\u{FFFD}');
            } else {
                out.push(char::from_u32(code).unwrap());
            }
            index += 3;
            continue;
        }
        if remaining < 4 {
            let malformed = if remaining > 1 {
                let second = bytes[index + 1];
                if !valid_second_4(first, second) {
                    1
                } else if remaining > 2 && !continuation(bytes[index + 2]) {
                    2
                } else {
                    remaining
                }
            } else {
                remaining
            };
            out.push('\u{FFFD}');
            index += malformed;
            continue;
        }
        let second = bytes[index + 1];
        let third = bytes[index + 2];
        let fourth = bytes[index + 3];
        let code = (u32::from(first & 0x07) << 18)
            | (u32::from(second & 0x3f) << 12)
            | (u32::from(third & 0x3f) << 6)
            | u32::from(fourth & 0x3f);
        if !valid_second_4(first, second)
            || !continuation(third)
            || !continuation(fourth)
            || !(0x10000..=0x10ffff).contains(&code)
        {
            out.push('\u{FFFD}');
            index += malformed_4(first, second, third);
            continue;
        }
        out.push(char::from_u32(code).unwrap());
        index += 4;
    }
    out
}

/// The charset argument of a `byte[]` constructor: a `Charset` (`charset/<name>`
/// marker) or a name. `Ok(None)` when it is not one decoded exactly here.
fn byte_charset(value: &Value, by_name: bool) -> Result<Option<ExactCharset>, NativeError> {
    let Value::Ref(Some(marker)) = value else {
        return Err(NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: None,
            frame: None,
        });
    };
    let canonical = if by_name {
        jdk_model::canonical_charset(marker).ok_or_else(|| NativeError::UncaughtException {
            class: "java/io/UnsupportedEncodingException".into(),
            message: Some(marker.clone()),
            frame: None,
        })?
    } else {
        marker.strip_prefix("charset/").unwrap_or("")
    };
    Ok(ExactCharset::named(canonical))
}

/// The text a `byte[]` constructor builds, after its charset (if any) was
/// resolved: `offset`/`count` index the array when the descriptor has them.
fn decode_bytes<H: Host>(
    runtime: &Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
    range: bool,
    charset: ExactCharset,
) -> Result<Option<String>, NativeError> {
    let values = match args.first() {
        Some(array @ (Value::Host(_) | Value::Ref(Some(_)))) => runtime.array(array)?.clone(),
        _ => {
            return Err(thrown(
                "java/lang/NullPointerException",
                "Cannot read the array length because \"bytes\" is null".into(),
            ));
        }
    };
    // A byte the runtime does not hold as an int is not guessed.
    let Some(bytes) = values
        .iter()
        .map(|value| match value {
            Value::Int(byte) => Some(*byte as u8),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()
    else {
        return Ok(None);
    };
    let bytes = if range {
        let offset = expect_int(r, StdHost::argument(r, args, 1)?)?;
        let count = expect_int(r, StdHost::argument(r, args, 2)?)?;
        check_range(offset, count, bytes.len())?;
        &bytes[offset as usize..(offset + count) as usize]
    } else {
        &bytes[..]
    };
    Ok(Some(charset.decode(bytes)))
}

/// Read a `char[]` and build the same UTF-16 text as the corresponding
/// `String(char[])` constructor. `String.valueOf` and `copyValueOf` delegate
/// to that constructor in the JDK, including its range and null checks.
fn char_array_text<H: Host>(
    runtime: &Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
    range: bool,
) -> Result<Option<String>, NativeError> {
    let units = match args.first() {
        Some(array @ Value::Host(_)) => runtime
            .array(array)?
            .iter()
            .map(|value| match value {
                Value::Int(code) => *code,
                _ => 0,
            })
            .collect::<Vec<_>>(),
        _ => {
            return Err(thrown(
                "java/lang/NullPointerException",
                "Cannot read the array length because \"value\" is null".into(),
            ));
        }
    };
    let (offset, count) = if range {
        let offset = expect_int(r, StdHost::argument(r, args, 1)?)?;
        let count = expect_int(r, StdHost::argument(r, args, 2)?)?;
        check_range(offset, count, units.len())?;
        (offset as usize, count as usize)
    } else {
        (0, units.len())
    };
    let slice = &units[offset..offset + count];
    Ok(String::from_utf16(&slice.iter().map(|unit| *unit as u16).collect::<Vec<_>>()).ok())
}

/// The text a `String` constructor builds; `Ok(None)` for a constructor
/// counted by [`count`] (see the module comment).
pub(crate) fn build<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<String>, NativeError> {
    if r.owner != "java/lang/String" || r.name != "<init>" {
        return Ok(None);
    }
    let codes = |runtime: &Runtime<'_, H>, parameter: &str| -> Result<Vec<i32>, NativeError> {
        match args.first() {
            Some(array @ Value::Host(_)) => Ok(runtime
                .array(array)?
                .iter()
                .map(|value| match value {
                    Value::Int(code) => *code,
                    _ => 0,
                })
                .collect()),
            _ => Err(thrown(
                "java/lang/NullPointerException",
                format!("Cannot read the array length because \"{parameter}\" is null"),
            )),
        }
    };
    let int = |index: usize| -> Result<i32, NativeError> {
        expect_int(r, StdHost::argument(r, args, index)?)
    };
    let text = match r.descriptor.as_str() {
        "()V" => Some(String::new()),
        "(Ljava/lang/String;)V" => match args.first() {
            Some(Value::Ref(text)) => text.clone(),
            Some(Value::Host(_)) => None,
            _ => {
                return Err(thrown(
                    "java/lang/NullPointerException",
                    "Cannot invoke \"java.lang.String.length()\" because \"original\" is null"
                        .into(),
                ));
            }
        },
        "([B)V" | "([BII)V" => {
            return decode_bytes(
                runtime,
                r,
                args,
                r.descriptor == "([BII)V",
                ExactCharset::Utf8,
            );
        }
        "([BLjava/nio/charset/Charset;)V"
        | "([BLjava/lang/String;)V"
        | "([BIILjava/nio/charset/Charset;)V"
        | "([BIILjava/lang/String;)V" => {
            let range = r.descriptor.starts_with("([BII");
            let by_name = r.descriptor.ends_with("Ljava/lang/String;)V");
            let charset = StdHost::argument(r, args, if range { 3 } else { 1 })?;
            return match byte_charset(charset, by_name)? {
                Some(charset) => decode_bytes(runtime, r, args, range, charset),
                None => Ok(None),
            };
        }
        "([C)V" => char_array_text(runtime, r, args, false)?,
        "([CII)V" => char_array_text(runtime, r, args, true)?,
        "([III)V" => {
            let points = codes(runtime, "codePoints")?;
            let (offset, count) = (int(1)?, int(2)?);
            check_range(offset, count, points.len())?;
            let mut text = String::new();
            for point in &points[offset as usize..(offset + count) as usize] {
                if !(0..=0x10FFFF).contains(point) {
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        point.to_string(),
                    ));
                }
                match char::from_u32(*point as u32) {
                    Some(c) => text.push(c),
                    // A surrogate code point is valid but unpaired here.
                    None => return Ok(None),
                }
            }
            Some(text)
        }
        _ => None,
    };
    Ok(text)
}

/// Model the static `String.valueOf(char[])`, `valueOf(char[], int, int)`,
/// `copyValueOf(char[])` and `copyValueOf(char[], int, int)` overloads.
/// An unpaired surrogate cannot be represented by the native string value, so
/// the exact call is retained as an unknown just as the constructor is.
pub(crate) fn value_of<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let range = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        ("java/lang/String", "valueOf" | "copyValueOf", "([C)Ljava/lang/String;") => false,
        ("java/lang/String", "valueOf" | "copyValueOf", "([CII)Ljava/lang/String;") => true,
        _ => return Ok(None),
    };
    match char_array_text(runtime, r, args, range)? {
        Some(text) => Ok(Some(Value::Ref(Some(text)))),
        None => runtime.host.unknown_operand_call(r).map(Some),
    }
}

/// Count `r` when it is a `java.lang.String` constructor; `Some(Void)` then
/// stands for the constructor's (void) answer.
pub(crate) fn count<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
) -> Result<Option<Value>, NativeError> {
    if r.owner != "java/lang/String" || r.name != "<init>" {
        return Ok(None);
    }
    runtime.host.unknown_operand_call(r)?;
    Ok(Some(Value::Void))
}
