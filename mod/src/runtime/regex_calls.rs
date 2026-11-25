//! The JDK calls answered by [`crate::runtime::java_regex`]: `String`'s regex methods
//! and `Pattern`'s static helpers.
//!
//! Every answer is Java's exact one, including the exceptions a replacement
//! string throws. A pattern, input or group outside the modelled subset is
//! handed to the host, which records it as an unknown host call; a null
//! argument is too (Java's helpful `NullPointerException` text is not
//! reproduced), except for `String.split`, whose null throws were already
//! modelled.

use crate::runtime::java_regex::{self, Thrown};
use crate::runtime::{Host, MethodRef, NativeError, Runtime, StdHost, Value, expect_int, expect_ref};

pub(crate) fn thrown(thrown: Thrown) -> NativeError {
    NativeError::UncaughtException {
        class: thrown.class.into(),
        message: Some(thrown.message),
        frame: None,
    }
}

/// Java's `String.split(regex, limit)`, or why it is not answered.
pub(crate) fn split_java_string(
    input: &str,
    pattern: &str,
    limit: i32,
) -> Result<Vec<String>, String> {
    // The engine stops a split past `java_regex::MAX_SPLIT_PARTS` parts
    // while it builds them, so a caller-visible array never grows unbounded.
    java_regex::string_split(input, pattern, limit)
        .map_err(|unsupported| format!("pattern {pattern:?}: {}", unsupported.0))
}

impl<H: Host> Runtime<'_, H> {
    /// A `CharSequence` argument's text: a tracked `StringBuilder`'s
    /// contents, or the string itself. Any other object (a jar's own
    /// `CharSequence`, a host marker) has text this runtime cannot read, so
    /// it answers `None` and the call is counted rather than matched against
    /// the marker.
    fn char_sequence(&self, value: &Value) -> Option<String> {
        let marker = expect_ref(value)?;
        if let Some(text) = self.builder_text(value) {
            return Some(text.clone());
        }
        crate::runtime::value_class::is_plain_string(marker).then(|| marker.clone())
    }

    /// The regex surface; `Ok(None)` leaves the call to the rest of the
    /// dispatch chain (and so to the host, which counts it).
    pub(crate) fn invoke_regex(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let text = |index: usize| args.get(index).and_then(expect_ref).cloned();
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("java/lang/String", "split", _) => self.invoke_string_split(r, receiver, args),
            ("java/lang/String", "matches", "(Ljava/lang/String;)Z") => {
                let (Some(input), Some(pattern)) = (receiver.and_then(expect_ref), text(0)) else {
                    return Ok(None);
                };
                Ok(java_regex::matches(&pattern, input)
                    .ok()
                    .map(|matched| Value::Int(i32::from(matched))))
            }
            (
                "java/util/regex/Pattern",
                "matches",
                "(Ljava/lang/String;Ljava/lang/CharSequence;)Z",
            ) => {
                let (Some(pattern), Some(input)) = (
                    text(0),
                    args.get(1).and_then(|value| self.char_sequence(value)),
                ) else {
                    return Ok(None);
                };
                Ok(java_regex::matches(&pattern, &input)
                    .ok()
                    .map(|matched| Value::Int(i32::from(matched))))
            }
            (
                "java/lang/String",
                "replaceAll" | "replaceFirst",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            ) => {
                let (Some(input), Some(pattern), Some(replacement)) =
                    (receiver.and_then(expect_ref), text(0), text(1))
                else {
                    return Ok(None);
                };
                match java_regex::replace(input, &pattern, &replacement, r.name == "replaceAll") {
                    Ok(Ok(result)) => Ok(Some(Value::Ref(Some(result)))),
                    Ok(Err(exception)) => Err(thrown(exception)),
                    Err(_) => Ok(None),
                }
            }
            // `pattern.matcher(input)` makes a new `Matcher` holding the pattern
            // text and the input. `matches()` (JEI validates config names with
            // `CONFIG_NAME_PATTERN.matcher(name).matches()`), `find()`,
            // `find(int)`, `group`, `start`, `end`, `groupCount` and `reset()` are
            // answered (`regex_matcher.rs`); every other `Matcher` method, and a
            // pattern or input the engine cannot run exactly, is counted.
            (
                "java/util/regex/Pattern",
                "matcher",
                "(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;",
            ) => {
                let (Some(pattern), Some(input)) = (
                    receiver.and_then(crate::runtime::host_jdk_constant::pattern_text),
                    args.first().and_then(|value| self.char_sequence(value)),
                ) else {
                    return Ok(None);
                };
                // A `StringBuilder`'s text is copied: only a `String` is fixed.
                let fixed = args
                    .first()
                    .is_some_and(|value| self.builder_text(value).is_none());
                Ok(Some(crate::runtime::host_jdk_constant::matcher(
                    &pattern, &input, fixed,
                )))
            }
            // `pattern.asPredicate()` makes a `Predicate` whose `test` runs the
            // pattern against its argument. Only a `Pattern` handle is a
            // receiver, and the only handle `Pattern.compile(String)` makes
            // has flags 0 (`compile(String, int)` is not modelled, so a
            // `Pattern` with flags never reaches here and stays counted); a
            // pattern the engine cannot compile is counted here, and an input
            // it cannot run exactly when `test` runs.
            ("java/util/regex/Pattern", "asPredicate", "()Ljava/util/function/Predicate;") => {
                let Some(pattern) = receiver.and_then(crate::runtime::host_jdk_constant::pattern_text)
                else {
                    return Ok(None);
                };
                // `Pattern.compile(String)` does not check its text (a
                // documented departure), so a pattern the JDK rejects with a
                // `PatternSyntaxException` reaches here. The engine refuses it
                // as it refuses one outside its subset, and the two cannot be
                // told apart: both are counted, never a predicate.
                if !java_regex::compiles(&pattern) {
                    return Ok(None);
                }
                Ok(Some(crate::runtime::host_jdk_constant::predicate(&pattern)))
            }
            // `predicate.test(object)` on a `Predicate` this runtime made from
            // a `Pattern.asPredicate()` it knows. The JDK's predicate is a
            // `Predicate<String>`: `test(Object)` casts its argument to `String`
            // (a `ClassCastException` for anything else) and runs
            // `matcher(s).find()` on a fresh matcher. Only a `Value::Ref` text
            // that is a plain `String` is answered. A host handle (a jar
            // object, a collection, a `File`, a `BigInteger`, ...) is never a
            // `String`, though `expect_ref` reads its key text, so it is not
            // taken through that; null, a `StringBuilder`/`StringBuffer`, a
            // runtime marker, and a text that spells an object's key (which
            // cannot be told from the object) stay counted, and so does an
            // input the engine refuses (a supplementary character). Any other
            // receiver (a jar's own `Predicate`, a lambda) is not this arm's:
            // the call goes on down the dispatch chain.
            ("java/util/function/Predicate", "test", "(Ljava/lang/Object;)Z") => {
                let (Some(pattern), Some(input)) = (
                    receiver.and_then(crate::runtime::host_jdk_constant::predicate_text),
                    args.first()
                        .filter(|value| self.builder_text(value).is_none())
                        .and_then(|value| match value {
                            Value::Ref(Some(text)) => Some(text),
                            _ => None,
                        })
                        .filter(|text| {
                            crate::runtime::value_class::is_plain_string(text)
                                && !crate::runtime::jar_object::spells_key_text(text)
                        }),
                ) else {
                    return Ok(None);
                };
                Ok(java_regex::find(&pattern, input)
                    .ok()
                    .map(|matched| Value::Int(i32::from(matched))))
            }
            ("java/util/regex/Pattern", "quote", "(Ljava/lang/String;)Ljava/lang/String;") => {
                Ok(text(0).map(|value| Value::Ref(Some(java_regex::quote(&value)))))
            }
            (
                "java/util/regex/Matcher",
                "quoteReplacement",
                "(Ljava/lang/String;)Ljava/lang/String;",
            ) => Ok(text(0).map(|value| Value::Ref(Some(java_regex::quote_replacement(&value))))),
            // The `Matcher` queries and `find` (`regex_matcher.rs`).
            ("java/util/regex/Matcher", _, _) => crate::runtime::regex_matcher::invoke(r, receiver, args),
            _ => Ok(None),
        }
    }

    /// `String.split(regex)` / `split(regex, limit)` into a runtime-owned
    /// `String[]`.
    ///
    /// The host layer has no array table, so the result array belongs to this
    /// execution, exactly like a `toArray()` from a stream. A pattern outside
    /// the modelled subset is counted (the host answers it).
    pub(crate) fn invoke_string_split(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != "java/lang/String" || r.name != "split" {
            return Ok(None);
        }
        let limit = match r.descriptor.as_str() {
            "(Ljava/lang/String;)[Ljava/lang/String;" => 0,
            "(Ljava/lang/String;I)[Ljava/lang/String;" => {
                expect_int(r, StdHost::argument(r, args, 1)?)?
            }
            _ => return Ok(None),
        };
        // A null receiver or pattern is an NPE on the JVM, and config code
        // commonly catches it around an optional value; keep it a real throw
        // so the caller's handler runs instead of turning into a hard stop.
        // A null pattern fails inside `String.split` on `regex.length()`,
        // whose text JDK 27 gives exactly. A null receiver's text names the
        // caller's variable (`because "<local1>" is null`); like every other
        // call on null in this runtime it leaves that clause out
        // (`null_checks`).
        let receiver = receiver
            .and_then(expect_ref)
            .ok_or_else(|| crate::runtime::null_checks::invoke_on_null(r))?;
        let pattern = expect_ref(StdHost::argument(r, args, 0)?).ok_or_else(|| {
            NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: Some("Cannot invoke \"String.length()\" because \"regex\" is null".into()),
                frame: None,
            }
        })?;
        let Ok(parts) = split_java_string(receiver.as_str(), pattern, limit) else {
            return Ok(None);
        };
        let parts = parts
            .into_iter()
            .map(|part| Value::Ref(Some(part)))
            .collect();
        Ok(Some(self.new_array_value("[Ljava/lang/String;", parts)))
    }
}
