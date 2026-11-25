//! The byte-level rules of `java.util.Base64`, with no runtime in them (`jdk_base64.rs` is the
//! model that answers the calls; this file is what it computes with, and it also compiles on
//! its own for the differential run in `tests/fixtures/base64/differential`).
//!
//! The alphabet and the padding are RFC 4648 (sections 4 and 5), the line length and the
//! separator of the MIME encoder are RFC 2045 (section 6.8). What the JDK does on top of them,
//! which the RFCs leave to the implementation, is read from JDK 25.0.5.1 runs of the probes in
//! that directory: the order in which the decoder meets its errors, which bytes it skips (the
//! MIME decoder only), the message of each error, and how the MIME encoder rounds its line
//! length. Every byte string of up to six to nine bytes over four alphabets of five to ten
//! bytes, through the three decoders (26,206,755 answers), and 191,052 encodings, are equal to
//! the JDK's (the README there has the four runs).

/// What a decoder accepts and skips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecoderKind {
    /// `Base64.getDecoder()`: the standard alphabet, nothing else is accepted.
    Basic,
    /// `Base64.getUrlDecoder()`: `-` and `_` for `+` and `/`.
    Url,
    /// `Base64.getMimeDecoder()`: the standard alphabet; every other byte is skipped.
    Mime,
}

/// How an encoder writes: its alphabet, its padding and its line breaks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EncoderConfig {
    pub(crate) url: bool,
    pub(crate) padding: bool,
    /// The characters on a line before the separator; `0` for no line breaks. A multiple of
    /// four.
    pub(crate) line_max: usize,
    pub(crate) separator: Vec<u8>,
}

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The value of an alphabet byte, `None` for any other byte (`=` included).
fn value(kind: DecoderKind, byte: u8) -> Option<u32> {
    let value = match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        b'+' if kind != DecoderKind::Url => 62,
        b'/' if kind != DecoderKind::Url => 63,
        b'-' if kind == DecoderKind::Url => 62,
        b'_' if kind == DecoderKind::Url => 63,
        _ => return None,
    };
    Some(u32::from(value))
}

/// The characters per line of `Base64.getMimeEncoder(lineLength, ..)`: the length rounded down
/// to a multiple of four, and no line breaks (`0`) below four.
pub(crate) fn mime_line_max(line_length: i32) -> usize {
    if line_length < 4 {
        0
    } else {
        (line_length & !3) as usize
    }
}

/// Whether `Base64.getMimeEncoder(_, lineSeparator)` refuses `byte` in a line separator: the
/// 64 characters of the standard alphabet and `=` (the URL alphabet's `-` and `_` are fine).
pub(crate) fn refused_in_separator(byte: u8) -> bool {
    byte == b'=' || value(DecoderKind::Basic, byte).is_some()
}

/// The bytes `src` encodes to.
pub(crate) fn encode(config: &EncoderConfig, src: &[u8]) -> Vec<u8> {
    let alphabet = if config.url { URL_SAFE } else { STANDARD };
    let mut out = Vec::with_capacity(encoded_len(config, src.len()));
    // Characters on the current line; the separator goes before the next character that
    // would pass the limit, never after the last one.
    let mut on_line = 0usize;
    let mut put = |out: &mut Vec<u8>, byte: u8| {
        if config.line_max != 0 && on_line == config.line_max {
            out.extend_from_slice(&config.separator);
            on_line = 0;
        }
        out.push(byte);
        on_line += 1;
    };
    for chunk in src.chunks(3) {
        let word = chunk.iter().enumerate().fold(0u32, |word, (i, byte)| {
            word | u32::from(*byte) << (16 - 8 * i)
        });
        let chars = chunk.len() + 1;
        for i in 0..4 {
            if i < chars {
                put(&mut out, alphabet[(word >> (18 - 6 * i) & 63) as usize]);
            } else if config.padding {
                put(&mut out, b'=');
            }
        }
    }
    out
}

/// The length of `encode(config, src)` for `len` input bytes.
pub(crate) fn encoded_len(config: &EncoderConfig, len: usize) -> usize {
    let groups = len / 3;
    let rest = len % 3;
    let chars = match (rest, config.padding) {
        (0, _) => groups * 4,
        (_, true) => groups * 4 + 4,
        (_, false) => groups * 4 + rest + 1,
    };
    if config.line_max == 0 || chars == 0 {
        chars
    } else {
        chars + config.separator.len() * ((chars - 1) / config.line_max)
    }
}

/// `Integer.toString(b, 16)` of the signed byte: `21`, `-1`, `-80`.
fn signed_hex(byte: u8) -> String {
    let signed = i32::from(byte as i8);
    if signed < 0 {
        format!("-{:x}", -signed)
    } else {
        format!("{signed:x}")
    }
}

const WRONG_UNIT: &str = "Input byte array has wrong 4-byte ending unit";
const FEW_BITS: &str = "Last unit does not have enough valid bits";

/// The bytes `src` decodes to, or the message of the `IllegalArgumentException` the JDK throws.
pub(crate) fn decode(kind: DecoderKind, src: &[u8]) -> Result<Vec<u8>, String> {
    if kind != DecoderKind::Mime && src.len() == 1 {
        return Err("Input byte[] should at least have 2 bytes for base64 bytes".into());
    }
    let mut out = Vec::with_capacity(src.len() / 4 * 3 + 3);
    let mut word = 0u32;
    let mut count = 0usize;
    let mut i = 0usize;
    while i < src.len() {
        let byte = src[i];
        if let Some(v) = value(kind, byte) {
            word = word << 6 | v;
            count += 1;
            i += 1;
            if count == 4 {
                out.extend_from_slice(&[(word >> 16) as u8, (word >> 8) as u8, word as u8]);
                word = 0;
                count = 0;
            }
        } else if byte == b'=' {
            // Padding ends the data: `xx==` or `xxx=` is the only shape it may take.
            match count {
                0 => return Err(WRONG_UNIT.into()),
                1 => return Err(FEW_BITS.into()),
                2 => {
                    if src.get(i + 1) != Some(&b'=') {
                        return Err(WRONG_UNIT.into());
                    }
                    out.push((word >> 4) as u8);
                    i += 2;
                }
                _ => {
                    out.extend_from_slice(&[(word >> 10) as u8, (word >> 2) as u8]);
                    i += 1;
                }
            }
            // Past the padding: the basic and URL decoders accept nothing; the MIME decoder
            // skips what is not in the alphabet and refuses the first byte that is (its
            // error counts from one, the others from zero).
            while i < src.len() {
                if kind != DecoderKind::Mime {
                    return Err(format!("Input byte array has incorrect ending byte at {i}"));
                }
                if value(kind, src[i]).is_some() {
                    return Err(format!(
                        "Input byte array has incorrect ending byte at {}",
                        i + 1
                    ));
                }
                i += 1;
            }
            return Ok(out);
        } else if kind == DecoderKind::Mime {
            i += 1;
        } else {
            return Err(format!("Illegal base64 character {}", signed_hex(byte)));
        }
    }
    match count {
        0 => {}
        1 => return Err(FEW_BITS.into()),
        2 => out.push((word >> 4) as u8),
        _ => out.extend_from_slice(&[(word >> 10) as u8, (word >> 2) as u8]),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(kind: DecoderKind, text: &[u8]) -> Result<Vec<u8>, String> {
        decode(kind, text)
    }

    fn config(url: bool, padding: bool, line_max: usize, separator: &[u8]) -> EncoderConfig {
        EncoderConfig {
            url,
            padding,
            line_max,
            separator: separator.to_vec(),
        }
    }

    #[test]
    fn rfc_4648_test_vectors_encode() {
        let std = config(false, true, 0, b"");
        for (plain, text) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(&std, plain.as_bytes()), text.as_bytes());
        }
        let bare = config(false, false, 0, b"");
        assert_eq!(encode(&bare, b"foob"), b"Zm9vYg");
        assert_eq!(encode(&bare, b"fooba"), b"Zm9vYmE");
    }

    #[test]
    fn the_two_alphabets_differ_in_the_last_two_characters() {
        let src = [0xfb, 0xff, 0xbf];
        assert_eq!(encode(&config(false, true, 0, b""), &src), b"+/+/");
        assert_eq!(encode(&config(true, true, 0, b""), &src), b"-_-_");
        assert_eq!(decoded(DecoderKind::Basic, b"+/+/"), Ok(src.to_vec()));
        assert_eq!(decoded(DecoderKind::Url, b"-_-_"), Ok(src.to_vec()));
        assert_eq!(
            decoded(DecoderKind::Url, b"+/+/"),
            Err("Illegal base64 character 2b".into())
        );
        assert_eq!(
            decoded(DecoderKind::Basic, b"-_-_"),
            Err("Illegal base64 character 2d".into())
        );
        assert_eq!(decoded(DecoderKind::Mime, b"-_-_"), Ok(vec![]));
    }

    #[test]
    fn mime_lines_break_between_chunks_never_at_the_end() {
        let mime = config(false, true, 76, b"\r\n");
        // 57 bytes are exactly one line of 76 characters.
        assert_eq!(encode(&mime, &[0; 57]).len(), 76);
        assert_eq!(encode(&mime, &[0; 58]).len(), 76 + 2 + 4);
        assert_eq!(encoded_len(&mime, 58), 82);
        assert_eq!(&encode(&mime, &[0; 58])[76..78], b"\r\n");
        assert_eq!(encoded_len(&mime, 0), 0);
        let short = config(false, true, 8, b"<>");
        assert_eq!(encode(&short, &[0; 7]), b"AAAAAAAA<>AA==".as_slice());
    }

    #[test]
    fn mime_line_length_rounds_down_to_four_and_below_four_is_none() {
        assert_eq!(mime_line_max(-1), 0);
        assert_eq!(mime_line_max(3), 0);
        assert_eq!(mime_line_max(4), 4);
        assert_eq!(mime_line_max(7), 4);
        assert_eq!(mime_line_max(76), 76);
        assert_eq!(mime_line_max(i32::MAX), 2_147_483_644);
        assert_eq!(mime_line_max(i32::MIN), 0);
    }

    #[test]
    fn separators_the_alphabet_has_are_refused() {
        for byte in b"AZaz09+/=" {
            assert!(refused_in_separator(*byte), "{byte}");
        }
        for byte in [b'-', b'_', b' ', b'\r', b'\n', 0, 0x80, 0xff] {
            assert!(!refused_in_separator(byte), "{byte}");
        }
    }

    #[test]
    fn padding_and_trailing_bytes_are_as_the_oracle_shows() {
        let error = |kind, text: &[u8], message: &str| {
            assert_eq!(decoded(kind, text), Err(message.into()), "{text:?}");
        };
        error(
            DecoderKind::Basic,
            b"A",
            "Input byte[] should at least have 2 bytes for base64 bytes",
        );
        error(
            DecoderKind::Mime,
            b"A",
            "Last unit does not have enough valid bits",
        );
        error(
            DecoderKind::Basic,
            b"AAAAA",
            "Last unit does not have enough valid bits",
        );
        error(
            DecoderKind::Basic,
            b"AA=",
            "Input byte array has wrong 4-byte ending unit",
        );
        error(
            DecoderKind::Basic,
            b"AAAA=",
            "Input byte array has wrong 4-byte ending unit",
        );
        error(
            DecoderKind::Basic,
            b"=AAA",
            "Input byte array has wrong 4-byte ending unit",
        );
        error(
            DecoderKind::Basic,
            b"A=AA",
            "Last unit does not have enough valid bits",
        );
        error(
            DecoderKind::Basic,
            b"AA===",
            "Input byte array has incorrect ending byte at 4",
        );
        error(
            DecoderKind::Basic,
            b"AA==\n",
            "Input byte array has incorrect ending byte at 4",
        );
        // The MIME decoder skips what follows the padding until a data byte, counted from one.
        assert_eq!(decoded(DecoderKind::Mime, b"AA===\n"), Ok(vec![0]));
        error(
            DecoderKind::Mime,
            b"AA==\nAA==",
            "Input byte array has incorrect ending byte at 6",
        );
        error(
            DecoderKind::Basic,
            b"AA==AAAA",
            "Input byte array has incorrect ending byte at 4",
        );
        error(
            DecoderKind::Mime,
            b"AA==AAAA",
            "Input byte array has incorrect ending byte at 5",
        );
        // Bits past the end of the data are not checked.
        assert_eq!(decoded(DecoderKind::Basic, b"QR=="), Ok(vec![65]));
        assert_eq!(decoded(DecoderKind::Basic, b"QUF="), Ok(vec![65, 65]));
        // A byte that is not data: its signed hex in the basic decoders, nothing in MIME.
        error(DecoderKind::Basic, b"AA!A", "Illegal base64 character 21");
        error(
            DecoderKind::Url,
            &[b'A', 0xe9, b'A', b'A'],
            "Illegal base64 character -17",
        );
        error(
            DecoderKind::Basic,
            &[0x80, b'A', b'A', b'A'],
            "Illegal base64 character -80",
        );
        assert_eq!(
            decoded(DecoderKind::Mime, &[0x80, b'A', b'A', b'A', b'A']),
            Ok(vec![0, 0, 0])
        );
        assert_eq!(decoded(DecoderKind::Mime, b"!"), Ok(vec![]));
        assert_eq!(decoded(DecoderKind::Basic, b""), Ok(vec![]));
    }

    #[test]
    fn every_length_round_trips_through_every_decoder() {
        let data: Vec<u8> = (0..=255u8).collect();
        for len in 0..=data.len() {
            let src = &data[..len];
            for (url, padding, line_max) in [
                (false, true, 0),
                (false, false, 0),
                (true, true, 0),
                (true, false, 0),
                (false, true, 76),
                (false, false, 76),
            ] {
                let text = encode(&config(url, padding, line_max, b"\r\n"), src);
                assert_eq!(
                    text.len(),
                    encoded_len(&config(url, padding, line_max, b"\r\n"), len)
                );
                let kind = if line_max != 0 {
                    DecoderKind::Mime
                } else if url {
                    DecoderKind::Url
                } else {
                    DecoderKind::Basic
                };
                assert_eq!(
                    decoded(kind, &text),
                    Ok(src.to_vec()),
                    "{len} {url} {padding}"
                );
            }
        }
    }
}
