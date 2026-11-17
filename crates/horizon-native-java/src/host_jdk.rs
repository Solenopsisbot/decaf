//! JDK runtime objects as host handles (#156 P2, second half): the main
//! thread, loggers, the stack walker and its frames, stack trace elements and
//! modules. Their text forms (`thread/main`, `logger/<name>`, `stack-walker`,
//! `stack-frame/…`, `stack-trace-element/…`, `module/…`) were Strings a jar
//! could spell. Expected answers: `tests/fixture_host_values_p2b.rs` (Java 25).
//!
//! None of these classes overrides `Object.hashCode`, and only `Module` (for
//! a named module) and `StackTraceElement` override `toString`, so most
//! `hashCode`/`toString` answers are identity hashes no run can reproduce:
//! counted. `equals` is identity ([`identity_equals`]) except
//! `StackTraceElement`'s. None of them is `Comparable` (`compare_to: None`:
//! a sort of them, a `ClassCastException` on Java, is counted).
//!
//! | kind | key | canonical | class |
//! |---|---|---|---|
//! | `Thread` | `main` | yes: callbacks run on one thread | `java/lang/Thread` |
//! | `Logger` | `of_values([api, name])` | per API (below) | unknown: declared by API |
//! | `StackWalker` | `""` (`getInstance()`), `options` | `getInstance()`'s `DEFAULT_WALKER` only | `java/lang/StackWalker` |
//! | `StackFrame` | `of_values([class, method])` | no: each walk makes new frames | `java/lang/StackFrameInfo` |
//! | `StackTraceElement` | `of_values([class, method])` | no | `java/lang/StackTraceElement` |
//! | `Module` | `unnamed`, `java.base` | yes | `java/lang/Module` |
//!
//! Keys that hold jar text (a logger's name, a frame's class and method) are
//! built only through `HostRef::of_values`, which escapes, and read back with
//! `values`: a name holding `,`, `\` or `@` is its own literal name.
//!
//! `Thread` and `StackTraceElement` objects are also made by `new` (a jar's
//! own thread is a `java/lang/Thread#n` object), so their kinds read their
//! class from the key (`ClassRule::FromKey`) rather than claiming the class
//! for handles alone: a fixed class would make every cast of a
//! `java/lang/Thread#n` object to `Thread` throw.
//!
//! A logger's exact class is its logging backend's (log4j-core, an slf4j
//! binding, `java.util.logging`'s `LogManager`), which the runtime does not
//! know: `ClassRule::Unknown`, declared by the API it came from, so
//! `getClass()` is counted and `instanceof` answers from the declared type.
//! The APIs also decide whether one name is one object: log4j's
//! `LogManager.getLogger` (per message factory, so `getFormatterLogger` is a
//! separate API, and an overload taking a `MessageFactory` one more, never
//! canonical and counted), slf4j's `LoggerFactory.getLogger` and
//! `java.util.logging.Logger.getLogger` cache one logger per name;
//! `LogUtils.getLogger()` names its caller (unknown here),
//! `Logger.getAnonymousLogger()` and `System.getLogger` hand out new objects.
//!
//! Every class check reaches the rows through `instance_of_class` with the
//! generated Java 25 hierarchy (`jdk_closure.rs` lists these classes).

use crate::host_path::identity_equals;
use crate::host_ref::{HostKind, HostRef, Methods, Nested, instance_of_class};
use crate::{NativeError, Value};

fn unknown_text(_: &HostRef, _: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    Ok(None)
}

fn identity_hash(_: &HostRef, _: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    Ok(None)
}

const fn methods(to_string: crate::host_ref::ToStringFn) -> Methods {
    Methods {
        to_string,
        hash_code: identity_hash,
        equals: |handle, other, _| Ok(identity_equals(handle, other)),
        instance_of: instance_of_class,
        crosses_jars: |_, _| true,
        compare_to: None,
    }
}

/// The `[a, b]` strings a two-value key holds.
fn two_strings(handle: &HostRef) -> Option<(String, String)> {
    match handle.values()?.as_slice() {
        // A logger made with a mod's own `MessageFactory` holds the factory third.
        [Value::Ref(Some(a)), Value::Ref(Some(b)), ..] => Some((a.clone(), b.clone())),
        _ => None,
    }
}

/// `Thread.currentThread()`: the one thread a native run's callbacks run on.
pub(crate) fn main_thread() -> Value {
    Value::Host(HostRef::new(HostKind::Thread, "main"))
}

pub(crate) fn is_main_thread(value: &Value) -> bool {
    matches!(value, Value::Host(handle) if handle.kind() == HostKind::Thread)
}

/// `Thread.toString` prints the thread's id and name, which are the
/// process's: Java 25 printed `Thread[#3,main,5,main]` for a plain program's
/// main thread, and a game's callbacks run on its server thread. Counted.
pub(crate) const THREAD: Methods = methods(unknown_text);

/// A logger of `api` named `name` (see the module docs for the APIs).
pub(crate) fn logger_value(api: &str, name: &str) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::Logger,
        &[Value::Ref(Some(api.into())), Value::Ref(Some(name.into()))],
    ))
}

/// A log4j logger made by `LogManager.getLogger(name, factory)` with a mod's own factory object.
/// Log4j 2.26's `InternalLoggerRegistry` keys loggers by (factory, name), and a factory that
/// doesn't override `equals`/`hashCode` compares by identity, so the handle holds the very factory
/// and two calls with the same pair are the same logger. `exact`: the factory's class extends
/// `AbstractMessageFactory` or `FormattedMessageFactory`, which Log4j keeps as is, so
/// `getMessageFactory()` returns it (Java 25 capture, `tests/fixture_log4j_get_logger_factory.rs`);
/// any other factory is wrapped in an adapter the host doesn't hold. Logging through it is any
/// logger's no-op, and level checks stay counted (`jdk_model.rs`).
pub(crate) fn logger_factory_value(name: &str, factory: Value, exact: bool) -> Value {
    let api = if exact {
        "log4j-factory-exact"
    } else {
        "log4j-factory-jar"
    };
    Value::Host(HostRef::of_values(
        HostKind::Logger,
        &[
            Value::Ref(Some(api.into())),
            Value::Ref(Some(name.into())),
            factory,
        ],
    ))
}

/// The factory a [`logger_factory_value`] handle holds, and whether `getMessageFactory()` returns
/// it as is.
pub(crate) fn logger_factory(value: &Value) -> Option<(bool, Value)> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::Logger {
        return None;
    }
    match handle.values()?.as_slice() {
        [Value::Ref(Some(api)), _, factory] if api == "log4j-factory-exact" => {
            Some((true, factory.clone()))
        }
        [Value::Ref(Some(api)), _, factory] if api == "log4j-factory-jar" => {
            Some((false, factory.clone()))
        }
        _ => None,
    }
}

/// The name of a logger handle, `None` for any other value.
pub(crate) fn logger_name(value: &Value) -> Option<String> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::Logger => {
            two_strings(handle).map(|(_, name)| name)
        }
        _ => None,
    }
}

fn logger_api(key: &str) -> Option<String> {
    two_strings(&HostRef::new(HostKind::Logger, key)).map(|(api, _)| api)
}

/// The types a logger surely is: what its API's factory declares (log4j's
/// `LoggerContext.getLogger` answers an `ExtendedLogger`).
pub(crate) fn logger_declared(key: &str) -> &'static [&'static str] {
    match logger_api(key).as_deref() {
        // `ExtendedLogger` extends `Logger`; the runtime holds neither.
        Some(
            "log4j"
            | "log4j-formatter"
            | "log4j-factory"
            | "log4j-factory-exact"
            | "log4j-factory-jar",
        ) => &[
            "org/apache/logging/log4j/spi/ExtendedLogger",
            "org/apache/logging/log4j/Logger",
        ],
        Some("slf4j" | "mojang") => &["org/slf4j/Logger"],
        Some("jul" | "jul-anonymous") => &["java/util/logging/Logger"],
        Some("system") => &["java/lang/System$Logger"],
        _ => &[],
    }
}

/// One object per name for the caching factories.
pub(crate) fn logger_canonical(key: &str) -> bool {
    matches!(
        logger_api(key).as_deref(),
        Some(
            "log4j"
                | "log4j-formatter"
                | "log4j-factory-exact"
                | "log4j-factory-jar"
                | "slf4j"
                | "jul"
        )
    )
}

pub(crate) const LOGGER: Methods = methods(unknown_text);

/// `StackWalker.getInstance(...)`: the key records whether the walker has
/// `RETAIN_CLASS_REFERENCE` access.
pub(crate) fn stack_walker(retain_class_reference: bool) -> Value {
    Value::Host(HostRef::new(
        HostKind::StackWalker,
        if retain_class_reference { "retain" } else { "" },
    ))
}

pub(crate) fn stack_walker_retain(value: &Value) -> Option<bool> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::StackWalker => {
            Some(handle.key() == "retain")
        }
        _ => None,
    }
}

pub(crate) const STACK_WALKER: Methods = methods(unknown_text);

/// One stack frame (`StackFrame`) or element (`StackTraceElement`) of the
/// jar's own frames.
pub(crate) fn frame_value(kind: HostKind, class: &str, method: &str) -> Value {
    frame_value_with_retain(kind, class, method, true)
}

pub(crate) fn frame_value_with_retain(
    kind: HostKind,
    class: &str,
    method: &str,
    retain_class_reference: bool,
) -> Value {
    Value::Host(HostRef::of_values(
        kind,
        &[
            Value::Ref(Some(class.into())),
            Value::Ref(Some(method.into())),
            Value::Int(i32::from(retain_class_reference)),
        ],
    ))
}

/// `(class, method)` of a frame handle of `kind`.
pub(crate) fn frame_parts(value: &Value, kind: HostKind) -> Option<(String, String)> {
    let Value::Host(handle) = value else {
        return None;
    };
    (handle.kind() == kind).then(|| {
        let values = handle.values()?;
        let [Value::Ref(Some(class)), Value::Ref(Some(method)), ..] = values.as_slice() else {
            return None;
        };
        Some((class.clone(), method.clone()))
    })?
}

pub(crate) fn frame_retain_class_reference(value: &Value, kind: HostKind) -> Option<bool> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != kind {
        return None;
    }
    let values = handle.values()?;
    match values.as_slice() {
        [_, _, Value::Int(flag), ..] => Some(*flag != 0),
        _ => Some(true),
    }
}

/// `StackFrameInfo.toString` names the frame's file and line, which the
/// runtime does not keep: counted.
pub(crate) const STACK_FRAME: Methods = methods(unknown_text);

/// `StackTraceElement.equals` compares the class, method, file and line
/// (and loader and module): elements of two methods are unequal, two of one
/// method may be two lines, so only the same element is known equal. Its
/// `toString`/`hashCode` read the file and line: counted.
pub(crate) const STACK_TRACE_ELEMENT: Methods = Methods {
    equals: |handle, other, _| {
        Ok(match other {
            Value::Host(other) if other.kind() == HostKind::StackTraceElement => {
                handle.same_object(other)
            }
            _ => Some(false),
        })
    },
    ..methods(unknown_text)
};

/// The unnamed module every jar class is in.
pub(crate) const UNNAMED_MODULE: &str = "unnamed";
/// `java.base`.
pub(crate) const JAVA_BASE_MODULE: &str = "java.base";

pub(crate) fn module_value(name: &str) -> Value {
    Value::Host(HostRef::new(HostKind::Module, name))
}

pub(crate) fn module_name(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::Module => Some(handle.key()),
        _ => None,
    }
}

/// `Module.toString`: `module <name>`; an unnamed module prints its identity
/// hash (counted).
pub(crate) const MODULE: Methods = methods(|handle, _| {
    Ok((handle.key() != UNNAMED_MODULE).then(|| format!("module {}", handle.key())))
});

#[cfg(test)]
mod tests {
    use super::*;

    /// Jar text in a key (a logger's name, a frame's class and method) is
    /// its own literal text: names spelled with every separator a key uses
    /// read back whole and never alias another object.
    #[test]
    fn names_with_separators_never_alias() {
        let names = [
            "",
            "a",
            "a,b",
            "a\\",
            "\\,",
            "a@b",
            "@",
            "a/b",
            "rlog4j,ra",
            "hlog:x",
            ",",
            "\\\\",
            "null",
            "😀,é",
        ];
        let mut keys = std::collections::HashSet::new();
        for api in ["log4j", "slf4j", "jul"] {
            for name in names {
                let logger = logger_value(api, name);
                assert_eq!(
                    logger_name(&logger).as_deref(),
                    Some(name),
                    "{api} {name:?}"
                );
                let Value::Host(handle) = &logger else {
                    unreachable!()
                };
                assert_eq!(logger_api(handle.key()).as_deref(), Some(api));
                assert!(
                    keys.insert(handle.key().to_owned()),
                    "{api} {name:?} aliased"
                );
            }
        }
        // Frames are another kind: their own set of keys.
        let mut keys = std::collections::HashSet::new();
        for class in names {
            for method in names {
                let frame = frame_value(HostKind::StackFrame, class, method);
                assert_eq!(
                    frame_parts(&frame, HostKind::StackFrame),
                    Some((class.to_owned(), method.to_owned()))
                );
                let Value::Host(handle) = &frame else {
                    unreachable!()
                };
                assert!(
                    keys.insert(handle.key().to_owned()),
                    "{class:?} {method:?} aliased"
                );
            }
        }
    }
}
