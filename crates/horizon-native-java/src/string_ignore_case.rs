//! `String`'s case-insensitive comparisons as Java 25 answers them:
//! `equalsIgnoreCase`, `compareToIgnoreCase`, `regionMatches` (both overloads)
//! and `String.CASE_INSENSITIVE_ORDER`.
//!
//! Two characters match when they are equal, equal after
//! `Character.toUpperCase`, or equal after `Character.toLowerCase` of that,
//! which is equality of `fold` below. Java folds a whole code point when both
//! strings are UTF-16 coded (a supplementary letter such as Deseret `U+10400`
//! matches `U+10428`, and compares as one value), and folds each UTF-16 unit
//! when exactly one of them is Latin-1 coded (all units `<= 0xFF`): there a
//! surrogate unit is compared as itself. `compareToIgnoreCase` answers the
//! difference of the first folded pair that differs, else the difference of
//! the lengths in UTF-16 units. A `regionMatches` region is cut out first, so
//! a pair it splits is compared as the lone units that remain.
//!
//! The case mappings are `jdk_character`'s Java 25 tables. A runtime `String`
//! is valid Unicode (no unpaired surrogate), so every operand is well formed.
//!
//! Only a real `String` is compared. A `java/lang/String#n` jar-object marker
//! (an unmodelled constructor's result) or a host handle has no text to
//! compare, so a call with either as the receiver or an argument is counted
//! (`unreadable`), never answered from the key text it carries.

use crate::jdk_character::{to_lower_case, to_upper_case};
use crate::{NativeError, Value, checkcast};

/// Whether `value` is no `String` whose text can be compared: a text that
/// spells an object's key (`java/lang/String#3`) or a host handle. Such a call
/// is counted, as `strip` and `indexOf(int)` count theirs.
pub(crate) fn unreadable(value: &Value) -> bool {
    crate::jar_object::spells_key(value) || matches!(value, Value::Host(_))
}

/// What both `equalsIgnoreCase` characters turn into before they are compared.
fn fold(code_point: i32) -> i32 {
    to_lower_case(to_upper_case(code_point))
}

fn is_latin1(units: &[u16]) -> bool {
    units.iter().all(|&unit| unit <= 0xFF)
}

/// The character at `at`, and how many units it takes: a high surrogate
/// followed by a low one inside `units` is one code point when `pairs` is set.
fn character_at(units: &[u16], at: usize, pairs: bool) -> (i32, usize) {
    let unit = units[at];
    if pairs
        && (0xD800..0xDC00).contains(&unit)
        && let Some(&low) = units.get(at + 1)
        && (0xDC00..0xE000).contains(&low)
    {
        let point = 0x10000 + ((i32::from(unit) - 0xD800) << 10) + (i32::from(low) - 0xDC00);
        return (point, 2);
    }
    (i32::from(unit), 1)
}

/// The first position (in units) at which the folded characters of two
/// sequences differ, with the two folded values; `None` when one runs out
/// with every compared character equal. `pairs` says whether a surrogate pair
/// is one character.
fn first_difference(a: &[u16], b: &[u16], pairs: bool) -> Option<(i32, i32)> {
    let mut at = 0;
    while at < a.len() && at < b.len() {
        let (left, left_width) = character_at(a, at, pairs);
        let (right, right_width) = character_at(b, at, pairs);
        // Case mappings never change a character's width, so equal folds are equally wide.
        debug_assert!(fold(left) != fold(right) || left_width == right_width);
        let (left, right) = (fold(left), fold(right));
        if left != right {
            return Some((left, right));
        }
        at += left_width;
    }
    None
}

/// Whether a comparison of `a` and `b` takes a surrogate pair as one character:
/// Java does when both are UTF-16 coded, and compares units when one is Latin-1.
fn compares_pairs(a: &[u16], b: &[u16]) -> bool {
    is_latin1(a) == is_latin1(b)
}

/// `a.equalsIgnoreCase(b)` for a non-null `b`.
pub(crate) fn equals_ignore_case(a: &str, b: &str) -> bool {
    let (a, b): (Vec<u16>, Vec<u16>) = (a.encode_utf16().collect(), b.encode_utf16().collect());
    a.len() == b.len() && first_difference(&a, &b, compares_pairs(&a, &b)).is_none()
}

/// `a.compareToIgnoreCase(b)` for non-null strings.
fn compare_to_ignore_case(a: &str, b: &str) -> i32 {
    let (a, b): (Vec<u16>, Vec<u16>) = (a.encode_utf16().collect(), b.encode_utf16().collect());
    match first_difference(&a, &b, compares_pairs(&a, &b)) {
        Some((left, right)) => left.wrapping_sub(right),
        None => (a.len() as i32).wrapping_sub(b.len() as i32),
    }
}

/// `a.regionMatches(ignoreCase, toffset, other, ooffset, len)`; with `other`
/// null it throws, once the offsets and length pass the checks that come first.
pub(crate) fn region_matches(
    ignore_case: bool,
    a: &str,
    toffset: i32,
    other: Option<&str>,
    ooffset: i32,
    len: i32,
) -> Result<bool, NativeError> {
    let a: Vec<u16> = a.encode_utf16().collect();
    if ooffset < 0 || toffset < 0 || i64::from(toffset) > a.len() as i64 - i64::from(len) {
        return Ok(false);
    }
    let Some(other) = other else {
        return Err(NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: Some("Cannot invoke \"String.length()\" because \"other\" is null".into()),
            frame: None,
        });
    };
    let b: Vec<u16> = other.encode_utf16().collect();
    if i64::from(ooffset) > b.len() as i64 - i64::from(len) {
        return Ok(false);
    }
    if len <= 0 {
        return Ok(true);
    }
    let (toffset, ooffset, len) = (toffset as usize, ooffset as usize, len as usize);
    let (left, right) = (&a[toffset..toffset + len], &b[ooffset..ooffset + len]);
    Ok(if ignore_case {
        // The coding is the whole string's, not the region's.
        first_difference(left, right, compares_pairs(&a, &b)).is_none()
    } else {
        left == right
    })
}

/// What a `String.CASE_INSENSITIVE_ORDER` operand is once the bridge cast it.
enum Operand<'a> {
    Null,
    Text(&'a str),
    /// A marker or host handle: no text to compare (`unreadable`).
    Counted,
}

/// The `String` an operand of `String.CASE_INSENSITIVE_ORDER` is, as the
/// bridge `compare(Object, Object)` casts it: null, a `ClassCastException` for
/// another class, counted for a marker or a host handle.
fn string_operand(value: &Value) -> Result<Operand<'_>, NativeError> {
    if unreadable(value) {
        return Ok(Operand::Counted);
    }
    match value {
        Value::Ref(None) => Ok(Operand::Null),
        // A text with no class of its own (`class_of_value` has none for a plain
        // `String`) is a `String`, as `natural_order` takes it.
        Value::Ref(Some(text)) => match crate::class_of_value(value).as_deref() {
            None | Some("java/lang/String") => Ok(Operand::Text(text)),
            Some(class) => Err(checkcast::class_cast_exception(class, "java/lang/String")),
        },
        _ => match crate::class_of_value(value) {
            Some(class) => Err(checkcast::class_cast_exception(&class, "java/lang/String")),
            None => Err(NativeError::Metadata(
                "String.CASE_INSENSITIVE_ORDER compared an object of an unknown class".into(),
            )),
        },
    }
}

/// `CaseInsensitiveComparator.compare(s1, s2)` over what the casts left: a null
/// `s1` throws first, then a null `s2`.
pub(crate) fn compare_strings(a: Option<&str>, b: Option<&str>) -> Result<i32, NativeError> {
    let null = |name: &str| NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: Some(format!(
            "Cannot read field \"value\" because \"{name}\" is null"
        )),
        frame: None,
    };
    let a = a.ok_or_else(|| null("s1"))?;
    let b = b.ok_or_else(|| null("s2"))?;
    Ok(compare_to_ignore_case(a, b))
}

/// `String.CASE_INSENSITIVE_ORDER.compare(a, b)`: the bridge casts both
/// arguments first (the first one first), so a wrong class wins over a null.
/// `None` when an operand cast is a marker or a host handle: counted.
pub(crate) fn case_insensitive_order(a: &Value, b: &Value) -> Result<Option<i32>, NativeError> {
    let a = string_operand(a)?;
    if matches!(a, Operand::Counted) {
        return Ok(None);
    }
    let b = string_operand(b)?;
    let (a, b) = match (a, b) {
        (_, Operand::Counted) => return Ok(None),
        (Operand::Null, Operand::Null) => (None, None),
        (Operand::Null, Operand::Text(b)) => (None, Some(b)),
        (Operand::Text(a), Operand::Null) => (Some(a), None),
        (Operand::Text(a), Operand::Text(b)) => (Some(a), Some(b)),
        (Operand::Counted, _) => return Ok(None),
    };
    compare_strings(a, b).map(Some)
}
