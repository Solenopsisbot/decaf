//! `java.util.Base64`, as JDK 25's encoder and decoder answer it.
//!
//! Mods move bytes through text: a config's or a texture's bytes become a `String` a file or a
//! packet carries (`Base64.getEncoder().encodeToString(bytes)`), and the same text becomes
//! bytes again (`Base64.getDecoder().decode(text)`). Before this model each of those was a
//! counted unknown call that answered `null`, so the first `encodeToString` was a
//! `NullPointerException` the JDK never throws.
//!
//! The byte rules are in `base64_codec.rs` (RFC 4648 and RFC 2045, plus what the oracle shows
//! the JDK does with them); this file is how the calls reach them. What the oracle shows and
//! this model answers:
//!
//! * `Base64.getEncoder()`, `getUrlEncoder()`, `getMimeEncoder()`, `getMimeEncoder(int, byte[])`,
//!   `getDecoder()`, `getUrlDecoder()`, `getMimeDecoder()`. The five no-argument calls answer
//!   one object each, as the JDK does (`getEncoder() == getEncoder()`).
//!   `getMimeEncoder(int, byte[])` rounds the length down to a multiple of four; below four it
//!   is `getEncoder()` itself, from four up a new object every time. A null separator is a
//!   `NullPointerException` without a message, and a separator byte that is in the standard
//!   alphabet or is `=` is `IllegalArgumentException("Illegal base64 line separator character
//!   0x41")` (the first one, in order, and also when the length is below four).
//! * The separator array is not copied: the JDK's encoder reads it again at every call, so a
//!   later `sep[0] = '#'` shows in the output. The model keeps the array itself.
//! * `Encoder.encode(byte[])`, `encodeToString(byte[])`, `encode(byte[], byte[])` (the
//!   destination checks, in the oracle's order: a null source, a null destination, then
//!   `IllegalArgumentException("Output byte array is too small for encoding all input bytes")`
//!   with nothing written), and `withoutPadding()` (the same object when the encoder already
//!   has no padding, a new one otherwise).
//! * `Decoder.decode(byte[])` and `decode(String)` (the string is read as ISO-8859-1: a char
//!   above U+00FF is one `?`, which no decoder accepts as data). Every error is the oracle's
//!   `IllegalArgumentException` and message; a null array or string is the JDK's helpful
//!   `NullPointerException` naming `src`.
//!
//! Counted, never guessed: `encode(ByteBuffer)`, `decode(ByteBuffer)`, `wrap(OutputStream)`,
//! `wrap(InputStream)`, `decode(byte[], byte[])` (the JDK writes the bytes before the one that
//! fails, and sizes its check from a pre-count this model does not reproduce), and an
//! object's `toString`, `hashCode`, `getClass` and `instanceof` of a type other than its own.
//!
//! State: the five singletons are handles keyed `std`, `url`, `mime` (encoders) and `basic`,
//! `url`, `mime` (decoders). An encoder the calls make (`withoutPadding()` of a padded one,
//! `getMimeEncoder(int, byte[])` from four up) is a handle allocated per call, with its
//! configuration text and the separator array kept under its key in the runtime's field table,
//! so it follows the heap's collection and snapshots.

use super::*;
use crate::runtime::array_data::ArrayData;
use crate::runtime::base64_codec::{self as codec, DecoderKind, EncoderConfig};
use crate::runtime::host_ref::{HostKind, HostRef, Methods, instance_of_class};

const OWNER: &str = "java/util/Base64";
const ENCODER: &str = "java/util/Base64$Encoder";
const DECODER: &str = "java/util/Base64$Decoder";
const CONFIG: &str = "#base64Config";
const SEPARATOR: &str = "#base64Separator";

const GET_ENCODER: &str = "()Ljava/util/Base64$Encoder;";
const GET_DECODER: &str = "()Ljava/util/Base64$Decoder;";

fn thrown(class: &str, message: Option<&str>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: message.map(str::to_owned),
        frame: None,
    }
}

/// The JDK's helpful `NullPointerException` for a null `byte[]` parameter named `name`.
fn null_array(name: &str) -> NativeError {
    thrown(
        "java/lang/NullPointerException",
        Some(&format!(
            "Cannot read the array length because \"{name}\" is null"
        )),
    )
}

fn illegal_argument(message: &str) -> NativeError {
    thrown("java/lang/IllegalArgumentException", Some(message))
}

/// A `byte[]` argument's bytes; `None` when it is not a byte array this runtime holds.
fn bytes_of<H: Host>(rt: &Runtime<'_, H>, value: &Value) -> Option<Vec<u8>> {
    match rt.array_data(value).ok()? {
        ArrayData::Byte(bytes) => Some(bytes.iter().map(|byte| *byte as u8).collect()),
        ArrayData::Ref(_) => None,
    }
}

fn byte_array<H: Host>(rt: &mut Runtime<'_, H>, bytes: Vec<u8>) -> Value {
    rt.new_array_data(
        "[B",
        ArrayData::Byte(bytes.into_iter().map(|byte| byte as i8).collect()),
    )
}

/// `Base64`'s static calls and an encoder's or a decoder's calls; `Ok(None)` for any other.
pub(super) fn invoke<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    match receiver {
        None if r.owner == OWNER => invoke_static(rt, r, args),
        Some(Value::Host(handle)) if handle.kind() == HostKind::Base64Encoder => {
            invoke_encoder(rt, r, handle, args)
        }
        Some(Value::Host(handle)) if handle.kind() == HostKind::Base64Decoder => {
            invoke_decoder(rt, r, handle.key(), args)
        }
        _ => Ok(None),
    }
}

fn singleton(kind: HostKind, key: &str) -> Value {
    Value::Host(HostRef::new(kind, key))
}

fn invoke_static<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let encoder = |key| Ok(Some(singleton(HostKind::Base64Encoder, key)));
    let decoder = |key| Ok(Some(singleton(HostKind::Base64Decoder, key)));
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("getEncoder", GET_ENCODER) => encoder("std"),
        ("getUrlEncoder", GET_ENCODER) => encoder("url"),
        ("getMimeEncoder", GET_ENCODER) => encoder("mime"),
        ("getDecoder", GET_DECODER) => decoder("basic"),
        ("getUrlDecoder", GET_DECODER) => decoder("url"),
        ("getMimeDecoder", GET_DECODER) => decoder("mime"),
        ("getMimeEncoder", "(I[B)Ljava/util/Base64$Encoder;") => {
            let (Some(Value::Int(line_length)), Some(separator)) = (args.first(), args.get(1))
            else {
                return Ok(None);
            };
            if *separator == Value::Ref(None) {
                return Err(thrown("java/lang/NullPointerException", None));
            }
            let Some(bytes) = bytes_of(rt, separator) else {
                return Ok(None);
            };
            if let Some(byte) = bytes
                .iter()
                .find(|byte| codec::refused_in_separator(**byte))
            {
                return Err(illegal_argument(&format!(
                    "Illegal base64 line separator character 0x{byte:x}"
                )));
            }
            let line_max = codec::mime_line_max(*line_length);
            if line_max == 0 {
                return encoder("std");
            }
            let config = EncoderConfig {
                url: false,
                padding: true,
                line_max,
                separator: Vec::new(),
            };
            Ok(Some(new_encoder(rt, &config, Some(separator.clone()))))
        }
        _ => Ok(None),
    }
}

/// A new encoder handle with `config` (its separator bytes live in `separator`, the array
/// the JDK's encoder would keep).
fn new_encoder<H: Host>(
    rt: &mut Runtime<'_, H>,
    config: &EncoderConfig,
    separator: Option<Value>,
) -> Value {
    let key = rt.new_object_key("base64-encoder");
    let text = format!(
        "u{}p{}m{}",
        u8::from(config.url),
        u8::from(config.padding),
        config.line_max
    );
    rt.fields
        .insert(format!("{key}|{CONFIG}"), Value::Ref(Some(text)));
    if let Some(separator) = separator {
        rt.fields.insert(format!("{key}|{SEPARATOR}"), separator);
    }
    singleton(HostKind::Base64Encoder, &key)
}

/// What an encoder's key stands for: its configuration and the array its separator is read
/// from. The separator's bytes are read from that array each time (the JDK keeps the array,
/// not a copy). `None` for a key this model did not make.
fn encoder_config<H: Host>(
    rt: &Runtime<'_, H>,
    key: &str,
) -> Option<(EncoderConfig, Option<Value>)> {
    let plain = |url, padding, line_max, separator: &[u8]| {
        (
            EncoderConfig {
                url,
                padding,
                line_max,
                separator: separator.to_vec(),
            },
            None,
        )
    };
    match key {
        "std" => return Some(plain(false, true, 0, b"")),
        "url" => return Some(plain(true, true, 0, b"")),
        "mime" => return Some(plain(false, true, 76, b"\r\n")),
        _ => {}
    }
    let text = rt
        .fields
        .get(&format!("{key}|{CONFIG}"))
        .and_then(expect_ref)?;
    let rest = text.strip_prefix('u')?;
    let (url, rest) = rest.split_once('p')?;
    let (padding, line_max) = rest.split_once('m')?;
    let line_max: usize = line_max.parse().ok()?;
    let separator = rt.fields.get(&format!("{key}|{SEPARATOR}")).cloned();
    let bytes = match (&separator, line_max) {
        (_, 0) => Vec::new(),
        (Some(array), _) => bytes_of(rt, array)?,
        (None, _) => return None,
    };
    let config = EncoderConfig {
        url: match url {
            "0" => false,
            "1" => true,
            _ => return None,
        },
        padding: match padding {
            "0" => false,
            "1" => true,
            _ => return None,
        },
        line_max,
        separator: bytes,
    };
    Some((config, separator))
}

fn invoke_encoder<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    handle: &HostRef,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let key = handle.key();
    let Some((config, separator)) = encoder_config(rt, key) else {
        return Ok(None);
    };
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("encode", "([B)[B") | ("encodeToString", "([B)Ljava/lang/String;") => {
            let Some(source) = args.first() else {
                return Ok(None);
            };
            if *source == Value::Ref(None) {
                return Err(null_array("src"));
            }
            let Some(bytes) = bytes_of(rt, source) else {
                return Ok(None);
            };
            let encoded = codec::encode(&config, &bytes);
            if r.name == "encode" {
                Ok(Some(byte_array(rt, encoded)))
            } else {
                // `new String(encoded, 0, 0, len)`: one char per byte (a separator may be
                // above 0x7F).
                Ok(Some(Value::Ref(Some(
                    encoded.iter().map(|byte| *byte as char).collect(),
                ))))
            }
        }
        ("encode", "([B[B)I") => {
            let (Some(source), Some(destination)) = (args.first(), args.get(1)) else {
                return Ok(None);
            };
            if *source == Value::Ref(None) {
                return Err(null_array("src"));
            }
            if *destination == Value::Ref(None) {
                return Err(null_array("dst"));
            }
            let Some(bytes) = bytes_of(rt, source) else {
                return Ok(None);
            };
            let Ok(target) = rt.array_data(destination) else {
                return Ok(None);
            };
            let Some(capacity) = target.bytes().map(<[i8]>::len) else {
                return Ok(None);
            };
            let encoded = codec::encode(&config, &bytes);
            if encoded.len() > capacity {
                return Err(illegal_argument(
                    "Output byte array is too small for encoding all input bytes",
                ));
            }
            if !encoded.is_empty() {
                let Some(key) = host_array::array_key(destination).map(str::to_owned) else {
                    return Ok(None);
                };
                let signed: Vec<i8> = encoded.iter().map(|byte| *byte as i8).collect();
                let Some(target) = rt.arrays.get_mut(&key) else {
                    return Ok(None);
                };
                if target.copy_bytes(0, &signed, 0, signed.len()).is_none() {
                    return Ok(None);
                }
                rt.push_array_views(&key);
            }
            Ok(Some(Value::Int(encoded.len() as i32)))
        }
        ("withoutPadding", GET_ENCODER) => {
            if !config.padding {
                return Ok(Some(Value::Host(handle.clone())));
            }
            let config = EncoderConfig {
                padding: false,
                ..config
            };
            // The separator array travels with it: a later write to the caller's array shows
            // in both encoders, as in the JDK. The built-in MIME separator has no caller.
            let separator = match (separator, key) {
                (Some(array), _) => Some(array),
                (None, "mime") => Some(byte_array(rt, config.separator.clone())),
                (None, _) => None,
            };
            Ok(Some(new_encoder(rt, &config, separator)))
        }
        _ => Ok(None),
    }
}

fn invoke_decoder<H: Host>(
    rt: &mut Runtime<'_, H>,
    r: &MethodRef,
    key: &str,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let kind = match key {
        "basic" => DecoderKind::Basic,
        "url" => DecoderKind::Url,
        "mime" => DecoderKind::Mime,
        _ => return Ok(None),
    };
    let bytes = match (r.name.as_str(), r.descriptor.as_str()) {
        ("decode", "([B)[B") => {
            let Some(source) = args.first() else {
                return Ok(None);
            };
            if *source == Value::Ref(None) {
                return Err(null_array("src"));
            }
            let Some(bytes) = bytes_of(rt, source) else {
                return Ok(None);
            };
            bytes
        }
        ("decode", "(Ljava/lang/String;)[B") => match args.first() {
            Some(Value::Ref(Some(text))) => latin1(text),
            Some(Value::Ref(None)) => {
                return Err(thrown(
                    "java/lang/NullPointerException",
                    Some(
                        "Cannot invoke \"String.getBytes(java.nio.charset.Charset)\" \
                         because \"src\" is null",
                    ),
                ));
            }
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    match codec::decode(kind, &bytes) {
        Ok(decoded) => Ok(Some(byte_array(rt, decoded))),
        Err(message) => Err(illegal_argument(&message)),
    }
}

/// `text.getBytes(StandardCharsets.ISO_8859_1)`: a char above U+00FF is one `?`, a surrogate
/// pair too (a lone surrogate is U+FFFD in this runtime, which is also above U+00FF).
fn latin1(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

/// The types an encoder is, whatever else the class declares.
pub(crate) fn encoder_types(_: &str) -> &'static [&'static str] {
    &[ENCODER, "java/lang/Object"]
}

/// The types a decoder is, whatever else the class declares.
pub(crate) fn decoder_types(_: &str) -> &'static [&'static str] {
    &[DECODER, "java/lang/Object"]
}

/// Encoders and decoders have `Object`'s `equals` (identity); their `hashCode` and `toString`
/// (`java.util.Base64$Encoder@1b6d3586`) are identity-based and counted.
pub(crate) const OBJECT_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| {
        Ok(match other {
            Value::Host(other) => handle.same_object(other),
            _ => Some(false),
        })
    },
    instance_of: instance_of_class,
    crosses_jars: |_, _| false,
    compare_to: None,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin1_keeps_the_first_256_chars_and_marks_the_rest() {
        assert_eq!(latin1("Az\u{e9}\u{ff}"), vec![b'A', b'z', 0xe9, 0xff]);
        assert_eq!(latin1("a\u{100}b"), b"a?b");
        assert_eq!(latin1("a\u{1f600}b"), b"a?b");
        assert_eq!(latin1("a\u{fffd}b"), b"a?b");
    }
}
