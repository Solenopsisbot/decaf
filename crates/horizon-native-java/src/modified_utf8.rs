//! A class file's `CONSTANT_Utf8` text: modified UTF-8 (JVMS §4.4.7).
//!
//! It differs from UTF-8 in two ways the old `from_utf8_lossy` read got wrong:
//! `U+0000` is the two bytes `C0 80`, and a supplementary character is its
//! UTF-16 surrogate pair, each half three bytes (CESU-8). So a string literal
//! `"ab😀"` came out as `ab` followed by six `U+FFFD`, and every `charAt`,
//! `length` or comparison after it was wrong.
//!
//! The bytes decode to UTF-16 code units, which then become Rust text. A
//! constant that holds a lone surrogate cannot be Rust text, and neither can
//! malformed bytes: each reads as U+FFFD, and [`Decoded::exact`] says so, so a
//! `String` constant built from one is counted where it is used instead of
//! passing for the text Java would have (#447).

/// One `CONSTANT_Utf8_info` payload, read.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Decoded {
    /// The text. A lone surrogate half or a malformed byte is one U+FFFD.
    pub(crate) text: String,
    /// `false` when `text` is not what the bytes spell (a U+FFFD stands for a
    /// lone surrogate or a malformed byte). A genuine U+FFFD in the bytes is
    /// exact.
    pub(crate) exact: bool,
}

/// Decode one `CONSTANT_Utf8_info` payload.
pub(crate) fn decode(bytes: &[u8]) -> Decoded {
    let mut exact = true;
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let first = bytes[at];
        let continuation = |offset: usize| {
            bytes
                .get(at + offset)
                .filter(|byte| *byte & 0xC0 == 0x80)
                .map(|byte| u16::from(byte & 0x3F))
        };
        match first {
            0x00..=0x7F => {
                units.push(u16::from(first));
                at += 1;
            }
            0xC0..=0xDF => match continuation(1) {
                Some(low) => {
                    units.push((u16::from(first & 0x1F) << 6) | low);
                    at += 2;
                }
                None => {
                    exact = false;
                    units.push(0xFFFD);
                    at += 1;
                }
            },
            0xE0..=0xEF => match (continuation(1), continuation(2)) {
                (Some(middle), Some(low)) => {
                    units.push((u16::from(first & 0x0F) << 12) | (middle << 6) | low);
                    at += 3;
                }
                _ => {
                    exact = false;
                    units.push(0xFFFD);
                    at += 1;
                }
            },
            // Four-byte forms and stray continuation bytes are not modified
            // UTF-8.
            _ => {
                exact = false;
                units.push(0xFFFD);
                at += 1;
            }
        }
    }
    match String::from_utf16(&units) {
        Ok(text) => Decoded { text, exact },
        Err(_) => Decoded {
            text: String::from_utf16_lossy(&units),
            exact: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{Decoded, decode};

    fn exact(text: &str) -> Decoded {
        Decoded {
            text: text.to_owned(),
            exact: true,
        }
    }

    fn lossy(text: &str) -> Decoded {
        Decoded {
            text: text.to_owned(),
            exact: false,
        }
    }

    // The bytes are what `javac` writes for each literal (`javap -v` shows
    // them in the constant pool; `DataOutputStream.writeUTF` writes the same).
    #[test]
    fn supplementary_characters_and_nul_decode_as_javac_encoded_them() {
        // "ab😀" with U+1F600 as the pair D83D DE00, three bytes each.
        assert_eq!(
            decode(&[0x61, 0x62, 0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]),
            exact("ab\u{1F600}")
        );
        // "a\0b": NUL is C0 80.
        assert_eq!(decode(&[0x61, 0xC0, 0x80, 0x62]), exact("a\u{0}b"));
        // "é€": two- and three-byte forms, as in UTF-8.
        assert_eq!(decode(&[0xC3, 0xA9, 0xE2, 0x82, 0xAC]), exact("é€"));
    }

    #[test]
    fn a_genuine_replacement_character_is_exact() {
        // "\ufffd" is EF BF BD, three bytes like any other BMP character.
        assert_eq!(decode(&[0xEF, 0xBF, 0xBD]), exact("\u{FFFD}"));
    }

    #[test]
    fn a_lone_surrogate_or_malformed_byte_reads_as_a_replacement_character() {
        // A high surrogate with no low one after it.
        assert_eq!(decode(&[0x78, 0xED, 0xA0, 0xBD]), lossy("x\u{FFFD}"));
        // A low surrogate first, then a high one: two lone halves, not a pair.
        assert_eq!(
            decode(&[0xED, 0xB0, 0x80, 0xED, 0xA0, 0xBD]),
            lossy("\u{FFFD}\u{FFFD}")
        );
        // A four-byte UTF-8 form is not modified UTF-8.
        assert_eq!(
            decode(&[0xF0, 0x9F, 0x98, 0x80]),
            lossy("\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}")
        );
    }

    #[test]
    fn a_pair_beside_a_lone_half_keeps_the_pair() {
        // "\ud83d\ude00\ud83d": the pair stays one character, the last half is lost.
        assert_eq!(
            decode(&[0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80, 0xED, 0xA0, 0xBD]),
            lossy("\u{1F600}\u{FFFD}")
        );
    }
}
