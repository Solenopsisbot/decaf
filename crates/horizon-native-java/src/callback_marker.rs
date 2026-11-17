//! Lambdas, method references and functional combinators as the interpreter
//! holds them: [`HostKind::Callback`] handles, with one constructor, one
//! reader and one `instanceof` (#156 P15a, P15b).
//!
//! A callback is a handle, never a `Value::Ref`: no `String` model (`charAt`,
//! a natural-order compare, `String.join`) reads it, so its key cannot be
//! read out, and no jar `String` is one, however it is spelled (#261 review
//! B1–B3).
//!
//! Each handle carries a [`Row`] (#156 P15b): the target, its handle's
//! reference kind, the interfaces, and the captures or operands as values,
//! exactly. Readers take them from the handle they hold, in an invocation or
//! between them, and the collector traces them (`heap::value_texts`). The key
//! names the object: one per call site for a lambda that captures nothing
//! (HotSpot 25), one per evaluation (`~<n>`) for everything else, so `==` is
//! key equality. Its text is the bounded fallback for a handle read back
//! from another handle's key (an optional of a lambda), which has no row.
//!
//! Keys:
//!
//! * a lambda: `@indy:<class#method>:<site>[~<n>]:<interfaces>:<target>:<captures>`;
//! * a combinator's result: `@fn:<op>[~<n>]:<operands>` (`predicate.and`,
//!   `cmp.then`, the operand-free `identity/function`, `cmp.natural`...).
//!
//! A key holds no secret, and no text becomes a callback: `HostRef::decode`
//! never yields one, and a handle it yields reads its key back as untrusted
//! text again, at every depth. A handle that holds a callback (an optional,
//! a map entry) keeps the value itself (`HostRef::of_values`), row and all.
//! Every callback the runtime makes has a row; the key's text is only its
//! name. So a structure that holds a callback for a jar is a handle too
//! (comparators here, `HostKind::MapEntry`). `<site>` is the call site's
//! instruction offset in `<class#method>`: HotSpot links each
//! `invokedynamic` instruction once, and javac duplicates a `finally` body's
//! lambda across instructions that share one constant. `<interfaces>` are
//! the call site's functional interface (the `invokedynamic` descriptor's
//! return type) and `altMetafactory`'s marker interfaces, `,`-joined: a
//! lambda's class implements exactly those (javap and HotSpot 25:
//! `instanceof Runnable` true, `Comparable`/`Serializable` false). Captures
//! and operands are escaped value markers (`value_marker`), bounded: the row
//! holds them exactly.

use crate::host_ref::{ClassHierarchy, HostKind, HostRef, JdkHierarchy, Methods};
use crate::{MethodRef, NativeError, Value};

/// A lambda as its readers see it: from its row, or read back from its key.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Callback {
    /// The implementation method the call site's bootstrap names.
    pub(crate) target: MethodRef,
    /// The functional interface the call site makes (`java/lang/Runnable`);
    /// empty when the producer did not know it.
    pub(crate) sam: String,
    /// The captured values, in the implementation method's argument order.
    pub(crate) captures: Vec<Value>,
    /// The implementation handle's reference kind (JVMS 5.4.3.5: 5
    /// `invokevirtual`, 6 `invokestatic`, 7 `invokespecial`, 8
    /// `newInvokeSpecial`, 9 `invokeinterface`), when the handle was made
    /// from a class file rather than read back from text.
    pub(crate) ref_kind: Option<u8>,
}

/// What a callback handle carries exactly (#156 P15b): the values its key
/// can only name in bounded text. Shared by every copy of the handle.
#[derive(Debug)]
pub(crate) enum Row {
    /// A lambda or method reference.
    Lambda {
        callback: Callback,
        interfaces: Vec<String>,
    },
    /// A combinator's result: `op` over `operands`.
    Composite { op: String, operands: Vec<Value> },
}

impl Row {
    /// The values the row holds: a lambda's captures, a combinator's
    /// operands. The collector traces them (`heap::value_texts`).
    pub(crate) fn values(&self) -> &[Value] {
        match self {
            Row::Lambda { callback, .. } => &callback.captures,
            Row::Composite { operands, .. } => operands,
        }
    }

    /// The values, taken out (for `HostRef`'s drop, which unwinds a deep
    /// chain in a loop).
    pub(crate) fn take_values(&mut self) -> Vec<Value> {
        match self {
            Row::Lambda { callback, .. } => std::mem::take(&mut callback.captures),
            Row::Composite { operands, .. } => std::mem::take(operands),
        }
    }
}

/// A number no other callback of this process has: the suffix that makes a
/// key name one evaluation (`~<n>`).
fn next_evaluation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// `@indy:`, the start of every lambda's key.
fn prefix() -> &'static str {
    "@indy:"
}

/// `@fn:`, the start of every combinator result's key.
fn composite_prefix() -> &'static str {
    "@fn:"
}

/// What a callback prints where its text is counted (`toString`, string
/// concatenation, a collection's text): never its marker, which names the
/// object and could be fed back.
pub(crate) const CALLBACK_TEXT: &str = "null";

#[cfg(test)]
/// The key of a new lambda from `source` (`class#method`), call site
/// `index`, implementing `interfaces` (`,`-joined), run by `target`
/// (`owner#name(desc)`, or `?` when the bootstrap names none), with
/// `captures` already escaped.
pub(crate) fn make(
    source: &str,
    index: u16,
    interfaces: &str,
    target: &str,
    captures: &str,
) -> String {
    format!(
        "{}{source}:{index}:{interfaces}:{target}:{captures}",
        prefix()
    )
}

/// A new lambda's handle from `source`'s call site (the `invokedynamic`
/// instruction's offset, `site`), implementing
/// `interfaces` (`,`-joined), run by `target` (`None` when the bootstrap
/// names none) through a handle of `ref_kind`, holding `captures`.
///
/// One key names one object (HotSpot 25, the plan's evidence): a lambda that
/// captures nothing is one object per call site, so its key is the site's;
/// a capturing one, and every reflective `metafactory` result (`fresh`), is
/// a new object per evaluation, so its key carries a number of its own
/// (`<index>~<n>`). The key's captures are bounded text, for a handle read
/// back from text; the row holds them exactly.
pub(crate) fn lambda(
    source: &str,
    site: usize,
    interfaces: &str,
    target: Option<MethodRef>,
    ref_kind: Option<u8>,
    captures: Vec<Value>,
    fresh: bool,
) -> Value {
    let site = if captures.is_empty() && !fresh {
        site.to_string()
    } else {
        format!("{site}~{}", next_evaluation())
    };
    let target_text = target.as_ref().map_or_else(
        || "?".to_owned(),
        |target| format!("{}#{}{}", target.owner, target.name, target.descriptor),
    );
    let key = format!(
        "{}{source}:{site}:{interfaces}:{target_text}:{}",
        prefix(),
        crate::bounded_captured_markers(&captures)
    );
    let Some(target) = target else {
        return handle(key);
    };
    let row = Row::Lambda {
        callback: Callback {
            target,
            sam: interfaces.split(',').next().unwrap_or_default().to_owned(),
            captures,
            ref_kind,
        },
        interfaces: interfaces
            .split(',')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect(),
    };
    Value::Host(HostRef::with_row(HostKind::Callback, key, row))
}

/// A lambda as tests spell it: `target` as `owner#name(desc)` text and
/// `captures` as escaped value markers.
#[cfg(test)]
pub(crate) fn lambda_from_text(
    source: &str,
    index: u16,
    interfaces: &str,
    target: &str,
    captures: &str,
) -> Value {
    let target = target.split_once('#').and_then(|(owner, rest)| {
        let at = rest.find('(')?;
        Some(MethodRef {
            owner: owner.to_owned(),
            name: rest[..at].to_owned(),
            descriptor: rest[at..].to_owned(),
        })
    });
    let captures = if captures.is_empty() {
        Vec::new()
    } else {
        crate::split_capture_markers(captures)
            .unwrap()
            .iter()
            .map(|marker| crate::parse_value_marker(marker).unwrap())
            .collect()
    };
    lambda(
        source,
        usize::from(index),
        interfaces,
        target,
        None,
        captures,
        false,
    )
}

/// The lambda a callback handle is: its row, or (a handle read back from
/// text) its key's bounded text. `None` for any other value, a combinator
/// result included.
pub(crate) fn lambda_of(value: &Value) -> Option<Callback> {
    read_lambda(value)?.ok()
}

/// [`lambda_of`], keeping why a lambda handle could not be read (its key's
/// text was cut): `None` only for a value that is no lambda.
pub(crate) fn read_lambda(value: &Value) -> Option<Result<Callback, NativeError>> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::Callback {
        return None;
    }
    match handle.row() {
        Some(Row::Lambda { callback, .. }) => Some(Ok(callback.clone())),
        Some(Row::Composite { .. }) => None,
        None => is_callback(handle.key()).then(|| parse(handle.key())),
    }
}

fn handle(key: String) -> Value {
    Value::Host(HostRef::new(HostKind::Callback, key))
}

/// Whether `text` is a lambda's key of this process.
pub(crate) fn is_callback(text: &str) -> bool {
    text.starts_with(prefix())
}

/// The key of `value` when it is a callback handle.
pub(crate) fn callback_text(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::Callback => Some(handle.key()),
        _ => None,
    }
}

/// Whether a runtime marker's `text` (a host collection's) embeds a
/// callback handle (`hcb:…`): such text is never printed. A plain `String`
/// holds none, however it is spelled. An `HttpClient.Builder`'s marker
/// (`hhcb:…`, tag `hcb`) is no callback.
pub(crate) fn holds_callback(text: &str) -> bool {
    text.match_indices("hcb:")
        .any(|(at, _)| at == 0 || text.as_bytes()[at - 1] != b'h')
        && !crate::value_class::is_plain_string(text)
}

/// The interfaces a callback keyed `key` implements: a lambda's call site
/// interface and markers, a combinator's own interface.
pub(crate) fn interfaces(key: &str) -> Vec<&str> {
    if let Some(rest) = key.strip_prefix(prefix()) {
        return rest
            .split(':')
            .nth(2)
            .map(|list| list.split(',').filter(|name| !name.is_empty()).collect())
            .unwrap_or_default();
    }
    let op = key
        .strip_prefix(composite_prefix())
        .and_then(|rest| rest.split_once(':'))
        .map_or("", |(op, _)| op.split('~').next().unwrap_or_default());
    match op {
        "predicate.and" | "predicate.or" | "predicate.not" => vec!["java/util/function/Predicate"],
        "consumer.then" => vec!["java/util/function/Consumer"],
        "biconsumer.then" => vec!["java/util/function/BiConsumer"],
        "function.then" | "function.compose" | IDENTITY_FUNCTION => {
            vec!["java/util/function/Function"]
        }
        "bifunction.then" => vec!["java/util/function/BiFunction"],
        IDENTITY_UNARY => vec!["java/util/function/UnaryOperator"],
        op if op.starts_with("cmp.") => vec!["java/util/Comparator"],
        _ => Vec::new(),
    }
}

/// Read a live callback's text back.
pub(crate) fn parse(text: &str) -> Result<Callback, NativeError> {
    let malformed = |what: &str| NativeError::Metadata(format!("callback marker has no {what}"));
    let rest = text
        .strip_prefix(prefix())
        .ok_or_else(|| NativeError::Metadata("not a callback of this run".into()))?;
    // The first four fields hold no `:` (a class, an index, an interface and
    // a descriptor have none); the captures may (a handle's `h<tag>:<key>`).
    let mut parts = rest.splitn(5, ':');
    let _source = parts.next().ok_or_else(|| malformed("source"))?;
    let _index = parts.next().ok_or_else(|| malformed("call-site index"))?;
    let interfaces = parts.next().ok_or_else(|| malformed("interface"))?;
    let target = parts.next().ok_or_else(|| malformed("target"))?;
    let captures = parts.next().ok_or_else(|| malformed("captures"))?;
    let (owner, method_and_descriptor) = target
        .split_once('#')
        .ok_or_else(|| NativeError::Metadata("callback target has no owner separator".into()))?;
    let descriptor_at = method_and_descriptor
        .find('(')
        .ok_or_else(|| NativeError::Metadata("callback target has no descriptor".into()))?;
    let captures = if captures.is_empty() {
        Vec::new()
    } else {
        crate::split_capture_markers(captures)?
            .into_iter()
            .map(|capture| crate::parse_value_marker(&capture))
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(Callback {
        target: MethodRef {
            owner: owner.to_owned(),
            name: method_and_descriptor[..descriptor_at].to_owned(),
            descriptor: method_and_descriptor[descriptor_at..].to_owned(),
        },
        sam: interfaces.split(',').next().unwrap_or_default().to_owned(),
        captures,
        ref_kind: None,
    })
}

/// `Function.identity()`'s and `UnaryOperator.identity()`'s operations: one
/// object each (JDK 25: non-capturing lambdas in static methods).
pub(crate) const IDENTITY_FUNCTION: &str = "identity/function";
pub(crate) const IDENTITY_UNARY: &str = "identity/unary";

/// A combinator's result: `op` over `operands` (`p.and(q)`: `predicate.and`
/// over `p`, `q`). The row holds the operands; the key names them as value
/// markers too, for a handle read back from text. Only the runtime writes
/// either, so a side is the value it was (a handle the jar holds, a nested
/// composite) and never comes from jar text. A combinator result is a new
/// object per call (`~<n>`); the operand-free `identity()` singletons are
/// one object each.
pub(crate) fn composite(op: &str, operands: &[&Value]) -> Value {
    // The row holds the operands; the key's text is only the fallback for
    // a result read back from another handle's key, so it is bounded short:
    // a chain a jar builds in a loop (`p = p.negate()`) keeps one short key
    // per link, never its whole left side again (#256's growth).
    let operands_owned: Vec<Value> = operands.iter().map(|value| (*value).clone()).collect();
    let markers = crate::bounded_markers(&operands_owned, 256);
    let op_field = if operands.is_empty() {
        op.to_owned()
    } else {
        format!("{op}~{}", next_evaluation())
    };
    let key = format!("{}{op_field}:{markers}", composite_prefix());
    let row = Row::Composite {
        op: op.to_owned(),
        operands: operands_owned,
    };
    Value::Host(HostRef::with_row(HostKind::Callback, key, row))
}

/// The operation and operands of `value` when it is a combinator result,
/// from its row: every combinator result the runtime makes has one, and no
/// text becomes one.
pub(crate) fn composite_of(value: &Value) -> Option<(String, Vec<Value>)> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::Callback => match handle.row()? {
            Row::Composite { op, operands } => Some((op.clone(), operands.clone())),
            Row::Lambda { .. } => None,
        },
        _ => None,
    }
}

/// The operands of `value` when it is a combinator result of `op`.
pub(crate) fn composite_operands(value: &Value, op: &str) -> Option<Vec<Value>> {
    composite_of(value)
        .filter(|(own, _)| own == op)
        .map(|(_, operands)| operands)
}

/// Whether two callbacks with key `key` are one object: always. A key
/// names exactly one object ([`lambda`], [`composite`]): a non-capturing
/// lambda's is its call site's, every other one carries its evaluation's
/// number. So `==` is key equality, never counted.
pub(crate) fn canonical(_: &str) -> bool {
    true
}

/// The types a callback surely is, for the class rule: `Object` (its
/// interfaces depend on the key; [`instance_of`] reads them).
pub(crate) fn declared_types(_: &str) -> &'static [&'static str] {
    &["java/lang/Object"]
}

/// `callback instanceof wanted`: its class implements its interfaces and
/// their superinterfaces, and is a final hidden class extending `Object`,
/// so it is no other class (`String`, `Number`: an exact `false`). `None`
/// where an interface's supertypes are not known (a game or library
/// interface the run does not hold): the caller counts it.
fn instance_of(handle: &HostRef, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
    if wanted == "java/lang/Object" {
        return Some(true);
    }
    let mut queue: Vec<String> = match handle.row() {
        Some(Row::Lambda { interfaces, .. }) => interfaces.clone(),
        _ => interfaces(handle.key())
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };
    let mut unknown = queue.is_empty();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(interface) = queue.pop() {
        if seen.len() >= 64 || !seen.insert(interface.clone()) {
            continue;
        }
        if interface == wanted || hierarchy.is_instance(&interface, wanted) == Some(true) {
            return Some(true);
        }
        if let Some(supers) = hierarchy.declared_interfaces(&interface) {
            queue.extend(supers);
        } else if !crate::jdk_closure::in_jdk_package(&interface)
            || JdkHierarchy.is_instance(&interface, wanted).is_none()
        {
            unknown = true;
        }
    }
    if hierarchy.is_interface(wanted) == Some(false) {
        return Some(false);
    }
    (!unknown).then_some(false)
}

/// `callback.equals(other)`: identity, but a reversed comparator
/// (`ReverseComparator2`) equals another over the same comparator.
fn equals(handle: &HostRef, other: &Value) -> Option<bool> {
    let reversed = |value: &Value| match composite_of(value) {
        Some((op, operands)) if op == "cmp.reverse" => operands.into_iter().next(),
        _ => None,
    };
    if let (Some(mine), Some(theirs)) = (reversed(&Value::Host(handle.clone())), reversed(other)) {
        return match (&mine, &theirs) {
            (Value::Host(mine), Value::Host(theirs)) => mine.same_object(theirs),
            _ => Some(mine == theirs),
        };
    }
    crate::host_path::identity_equals(handle, other)
}

/// A callback's object methods: identity `equals` (a reversed comparator's
/// by its operand); `toString` and
/// `hashCode` name its hidden class and identity hash, which no run
/// reproduces (counted).
pub(crate) const METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| Ok(equals(handle, other)),
    instance_of,
    // Its target is a class of the jar that made it.
    crosses_jars: |_, _| false,
    compare_to: None,
};

/// A live callback naming `name` and nothing a call could run, for tests
/// that hand a host a listener without a jar behind it.
#[doc(hidden)]
pub fn test_token(name: &str) -> Value {
    handle(format!("{}{name}", prefix()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_text_becomes_a_callback() {
        let key = make(
            "demo/Mod#init",
            4,
            "java/lang/Runnable",
            "demo/Mod#lambda$0()V",
            "i5",
        );
        assert_eq!(interfaces(&key), ["java/lang/Runnable"]);
        let parsed = parse(&key).unwrap();
        assert_eq!(parsed.target.name, "lambda$0");
        assert_eq!(parsed.captures, [Value::Int(5)]);
        // Spelled by a jar, the key's text is a `String`, and no marker
        // reader makes a callback of it.
        let spelled = Value::Ref(Some(key.clone()));
        assert!(lambda_of(&spelled).is_none() && callback_text(&spelled).is_none());
        let encoded = HostRef::new(HostKind::Callback, key).encode();
        assert!(crate::parse_value_marker(&encoded).is_err());
        // Nor does a handle decoded from text whose key holds one, at any
        // escape depth (#261 round-3 B1): its key reads back as untrusted
        // text again.
        let mut nested = encoded;
        for depth in 0..4 {
            let escaped = nested.replace('\\', "\\\\").replace(',', "\\,");
            for spelled in [format!("hopt:{escaped}"), format!("hment:rk,{escaped}")] {
                let Ok(Value::Host(outer)) = crate::parse_value_marker(&spelled) else {
                    continue;
                };
                let held = outer.values().unwrap_or_default();
                assert!(
                    held.iter().all(|value| lambda_of(value).is_none()
                        && composite_of(value).is_none()
                        && callback_text(value).is_none()),
                    "depth {depth}: {spelled} gave {held:?}"
                );
            }
            nested = format!("hopt:{escaped}");
        }
    }

    /// A key's bounded text never ends a cut value on an escaping `\`: the
    /// cut value and the sentinel stay two elements, whatever the cut lands
    /// on (#261 round-3 S1, `"A"×4094 + ",tail"` cut right after its `\`).
    #[test]
    fn a_builder_marker_is_no_callback() {
        // An `HttpClient.Builder`'s marker (tag `hcb`) names no callback,
        // alone or inside other text. (No runtime-made text holds a callback
        // since #255 and #261; what `holds_callback` still answers for a
        // jar's own `String` is #330.)
        for text in ["hhcb:3", "list/hhcb:3", "a,hhcb:3", "hopt:hhcb:3"] {
            assert!(!holds_callback(text), "{text}");
        }
    }

    #[test]
    fn a_cut_never_escapes_its_sentinel() {
        for length in 4090..4100 {
            let text = format!("{},tail", "A".repeat(length));
            let bounded = crate::bounded_markers(&[Value::Ref(Some(text.clone()))], 4096);
            let parts = crate::split_capture_markers(&bounded).unwrap();
            if bounded.ends_with(",\u{2026}") {
                assert_eq!(
                    parts.last().map(String::as_str),
                    Some("\u{2026}"),
                    "{length}"
                );
                assert!(
                    parts.len() == 2 && text.starts_with(&parts[0][1..]),
                    "{length}"
                );
            } else {
                assert_eq!(parts, [format!("r{text}")], "{length}");
            }
        }
    }

    #[test]
    fn a_nested_composite_reads_back_whole() {
        let left = test_token("left");
        let pipe = Value::Ref(Some("a|1,b\\".into()));
        let inner = composite("consumer.then", &[&left, &pipe]);
        let outer = composite("consumer.then", &[&inner, &left]);
        assert_eq!(
            composite_operands(&outer, "consumer.then"),
            Some(vec![inner.clone(), left.clone()])
        );
        assert_eq!(
            composite_operands(&inner, "consumer.then"),
            Some(vec![left, pipe])
        );
        assert_eq!(composite_operands(&outer, "function.then"), None);
        // A `String` spelled like a composite's key is no composite.
        let spelled = Value::Ref(Some(callback_text(&outer).unwrap().to_owned()));
        assert_eq!(composite_operands(&spelled, "consumer.then"), None);
    }
}
