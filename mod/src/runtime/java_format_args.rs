//! The runtime side of [`crate::runtime::java_format`]: a `String.format`/`printf`
//! call's locale, format and `Object[]` read into the formatter's arguments.
//!
//! The interpreter keeps values loosely typed, so each argument is classed the
//! way the rest of the runtime classes it (`Object.getClass`, `instanceof
//! String`), and every value whose class or text it cannot vouch for becomes
//! [`Arg::Opaque`] or an ambiguous variant the formatter refuses where the
//! readings differ. A refused call goes to the host as an unknown call.

use super::*;
use crate::runtime::java_format::{self, Arg};

impl<H: Host> Runtime<'_, H> {
    /// `String.format(locale, format, args)`'s text, or `None` when the
    /// formatter cannot answer it exactly. `locale` is `None` for the
    /// overloads without one (the player's default FORMAT locale, which the
    /// bounded host cannot provide).
    pub(crate) fn java_format(
        &mut self,
        locale: Option<&Value>,
        format: &Value,
        args: &Value,
    ) -> Result<Option<String>, NativeError> {
        if let Some(locale) = locale
            && !formats_like_us(locale)
        {
            return Ok(None);
        }
        // The player's default FORMAT locale is outside the bounded host.
        // Keep only conversions whose output cannot observe it; grouping,
        // floating-point decimal symbols, and `%s` of a localized host value
        // are counted instead of silently using US symbols.
        // The parameter is a `String`, so any reference is its text; a null
        // format throws.
        let Value::Ref(Some(format)) = format else {
            return Ok(None);
        };
        let args = match args {
            // `String.format(f, (Object[]) null)` formats every argument as
            // null.
            Value::Ref(None) => None,
            array => match self.array(array) {
                Ok(values) => Some(values.into_owned()),
                // An argument array the runtime doesn't hold: counted.
                Err(_) => return Ok(None),
            },
        };
        if locale.is_none() && default_locale_sensitive(format, args.as_deref()) {
            return Ok(None);
        }
        let values = args;
        let mut resolve = |index: usize, conversion: char| {
            let Some(values) = values.as_deref() else {
                return Ok(Some(Arg::Null));
            };
            let Some(value) = values.get(index) else {
                return Ok(None);
            };
            if conversion != 's' && crate::runtime::jar_object::packaged_object_class(value).is_some() {
                return Ok(None);
            }
            self.format_arg(value).map(Some)
        };
        java_format::format_with(format, values.is_some(), &mut resolve)
    }

    /// One element of the `Object[]`, classed as `Object.getClass` would
    /// class it (`class_of_value`).
    fn format_arg(&mut self, value: &Value) -> Result<Arg, NativeError> {
        // Java dispatches %s on Formattable through the jar object's
        // formatTo(Formatter, flags, width, precision), not toString(). The
        // bounded runtime does not yet execute that callback, so refuse the
        // whole format rather than silently printing the wrong text. A jar
        // object whose class chain can't be settled (`None`, a superclass or
        // interface the runtime doesn't hold) might be Formattable too. No
        // 26.3 game class implements it, and the JDK table settles JDK classes.
        if let Value::Host(handle) = value
            && let Some(class) = handle.class()
            && match self.class_known_instance_of(class, "java/util/Formattable") {
                Some(known) => known,
                None => handle.kind() == crate::runtime::host_ref::HostKind::JarObject,
            }
        {
            return Ok(Arg::Opaque);
        }
        let marker = match value {
            Value::Ref(None) => return Ok(Arg::Null),
            Value::Int(value) => return Ok(Arg::Int(*value)),
            Value::Long(value) => return Ok(Arg::Long(*value)),
            // A bare floating-point value is a `Double`; a `Float` is boxed.
            Value::Float(value) => return Ok(Arg::Double(*value)),
            Value::Boxed(boxed) => return Ok(boxed_arg(*boxed)),
            Value::Host(handle) => {
                return match self.host_ref_text(handle, "", "") {
                    Ok(Some(text)) => Ok(Arg::Str(text)),
                    _ => Ok(Arg::Opaque),
                };
            }
            Value::Void => return Ok(Arg::Opaque),
            Value::Ref(Some(marker)) => marker,
        };
        if self.is_counted_string(value) {
            return Ok(Arg::Opaque);
        }
        if let Some(text) = self.builder_text(value) {
            return Ok(Arg::Str(text.clone()));
        }
        if value_class::is_plain_string(marker) {
            return Ok(Arg::Str(marker.clone()));
        }
        // A jar object prints its own `toString` (`jar_text.rs`); one that
        // throws, or that only the JVM can print (an identity hash), is
        // refused here and counted by the host.
        if crate::runtime::jar_object::packaged_object_class(value).is_some() {
            return match self.jar_to_string(value, "java/util/Formatter", "format") {
                Ok(Some(result)) if self.is_counted_string(&result) => Ok(Arg::Opaque),
                Ok(Some(Value::Ref(Some(text)))) => Ok(Arg::Str(text)),
                Ok(Some(Value::Ref(None))) => Ok(Arg::Null),
                Ok(Some(_)) => Ok(Arg::Opaque),
                Ok(None) => Ok(Arg::Opaque),
                Err(error) => return Err(error),
            };
        }
        // A model object whose `toString` the host answers; anything else
        // (a marker no model prints) cannot be printed or hashed exactly.
        match self.host.modelled_to_string(value) {
            Some(text) => Ok(Arg::Str(text)),
            None => Ok(Arg::Opaque),
        }
    }
}

/// Whether a no-locale format can observe the player's default FORMAT locale.
fn default_locale_sensitive(format: &str, args: Option<&[Value]>) -> bool {
    let localized = args.is_some_and(|values| {
        values.iter().any(|value| {
            matches!(value, Value::Host(_))
                && crate::runtime::host_jdk_constant::locale_parts(value).is_some()
        })
    });
    let bytes = format.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'%' {
            at += 1;
            continue;
        }
        at += 1;
        if at >= bytes.len() {
            break;
        }
        if bytes[at] == b'%' || bytes[at] == b'n' {
            at += 1;
            continue;
        }
        let mut grouped = false;
        while at < bytes.len() && !bytes[at].is_ascii_alphabetic() {
            grouped |= bytes[at] == b',';
            at += 1;
        }
        let Some(&conversion) = bytes.get(at) else {
            break;
        };
        if grouped
            || matches!(conversion, b'e' | b'E' | b'f' | b'F' | b'g' | b'G')
            || (localized && matches!(conversion, b's' | b'S'))
        {
            return true;
        }
        at += 1;
    }
    false
}

/// A boxed primitive's marker as its argument.
fn boxed_arg(boxed: Boxed) -> Arg {
    match boxed {
        Boxed::Boolean(value) => Arg::Bool(value),
        Boxed::Integer(value) => Arg::Int(value),
        Boxed::Short(value) => Arg::Short(value),
        Boxed::Byte(value) => Arg::Byte(value),
        Boxed::Character(unit) => Arg::Char(unit),
        Boxed::Float(bits) => Arg::Float(f32::from_bits(bits)),
    }
}

/// Whether a `Locale` argument formats with the symbols the formatter
/// models: `.` decimal point, `,` grouping in threes, ASCII digits. A null
/// locale applies no localization, which is the same.
fn formats_like_us(locale: &Value) -> bool {
    match locale {
        Value::Ref(None) => true,
        // `Locale.US`, `ROOT`, `ENGLISH` (`host_jdk_constant.rs`).
        value @ Value::Host(_) => matches!(
            crate::runtime::host_jdk_constant::locale_parts(value),
            Some(("en", "US" | "") | ("", ""))
        ),
        _ => false,
    }
}
