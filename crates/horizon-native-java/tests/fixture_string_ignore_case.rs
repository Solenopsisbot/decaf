//! Case-insensitive `String` comparisons: `equalsIgnoreCase`,
//! `compareToIgnoreCase`, both `regionMatches` overloads and
//! `String.CASE_INSENSITIVE_ORDER`, answered as Java 25 answers them.
//!
//! `equalsIgnoreCase` was modelled with `eq_ignore_ascii_case`, so a pair such
//! as `"\u00e4"`/`"\u00c4"`, `"k"`/`"\u212a"` (Kelvin sign) or `"I"`/`"\u0131"`
//! answered `false` where Java answers `true`, with nothing counted. The other
//! three were not modelled, so every call stayed a counted unknown host call
//! (`CASE_INSENSITIVE_ORDER` was not even a known field).
//!
//! `fixtures/string_ignore_case/demo/Probe.java` runs unchanged on both sides:
//! its own `main` on Java 25 (output is `java25.txt`), its static probes in
//! Horizon. Every row is answered, and answered exactly: nothing is counted.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{HostKind, HostRef, StdHost, Value};
use javac_fixture::Fixture;

/// What `demo.Probe`'s `main` printed on OpenJDK 25.0.5.1 (`javac --release
/// 21`), the same on two runs; see the README beside it.
const ORACLE: &str = include_str!("fixtures/string_ignore_case/java25.txt");
const PROBE: &str = include_str!("fixtures/string_ignore_case/demo/Probe.java");

fn fixture() -> Fixture {
    Fixture::build("string-ignore-case", &[("demo/Probe.java", PROBE)])
}

/// Every probe answers as Java 25 did, with nothing counted.
#[test]
fn string_ignore_case_matches_java_25() {
    let fixture = fixture();
    let mut failures = Vec::new();
    for line in ORACLE.lines() {
        let (method, java) = line.split_once('=').unwrap();
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture
            .jar
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{method} failed: {error}"));
        let text = match answer {
            horizon_native_java::Value::Ref(Some(text)) => text,
            other => format!("{other:?}"),
        };
        let unknown = host.unknown_host_calls();
        if text != java || !unknown.is_empty() {
            failures.push(format!(
                "{method}\n  java:   {java}\n  native: {text}\n  unknown: {unknown:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const COUNTED: &str = include_str!("fixtures/string_ignore_case/demo/Counted.java");

fn text(value: &str) -> Value {
    Value::Ref(Some(value.into()))
}

/// What each call runs on, named as the host counts it.
const STRINGS: &str = "(Ljava/lang/String;Ljava/lang/String;)";

/// A marker or host handle as receiver or argument of every method is counted;
/// a real `String` pair is answered and counts nothing.
#[test]
fn markers_and_host_handles_are_counted() {
    let fixture = Fixture::build(
        "string-ignore-case-counted",
        &[("demo/Counted.java", COUNTED)],
    );
    let marker = text("java/lang/String#3");
    // A jar object whose key text is a `String`'s, and a handle with no text.
    let object = Value::Host(HostRef::new(HostKind::JarObject, "java/lang/String#4"));
    let handle = Value::Host(HostRef::new(HostKind::GameEntity, "game/player/test"));
    let calls: [(&str, &str, &str, &str); 5] = [
        (
            "equalsIgnoreCase",
            "Z",
            "java/lang/String#equalsIgnoreCase(Ljava/lang/String;)Z",
            "",
        ),
        (
            "compareToIgnoreCase",
            "I",
            "java/lang/String#compareToIgnoreCase(Ljava/lang/String;)I",
            "",
        ),
        (
            "regionMatchesIgnoreCase",
            "Z",
            "java/lang/String#regionMatches(ZILjava/lang/String;II)Z",
            "",
        ),
        (
            "regionMatchesExact",
            "Z",
            "java/lang/String#regionMatches(ILjava/lang/String;II)Z",
            "",
        ),
        (
            "caseInsensitiveOrder",
            "I",
            "java/lang/String#compareToIgnoreCase(Ljava/lang/String;)I",
            "Ljava/lang/Object;",
        ),
    ];
    let mut failures = Vec::new();
    for (method, ret, counted, object_params) in calls {
        let descriptor = if object_params.is_empty() {
            format!("{STRINGS}{ret}")
        } else {
            format!("({object_params}{object_params}){ret}")
        };
        let run = |a: &Value, b: &Value| {
            let mut host = fixture.jar.seed_host(StdHost::new());
            let answer = fixture.jar.invoke_method_with_args(
                "demo/Counted",
                method,
                &descriptor,
                None,
                &[a.clone(), b.clone()],
                &mut host,
            );
            (answer, host.unknown_host_calls())
        };
        // Real strings: answered, nothing counted.
        let (answer, unknown) = run(&text("ab"), &text("AB"));
        if answer.is_err() || !unknown.is_empty() {
            failures.push(format!(
                "{method}: real strings gave {answer:?}, counted {unknown:?}"
            ));
        }
        for operand in [&marker, &object, &handle] {
            for (label, a, b) in [
                ("receiver", operand, &text("ab")),
                ("argument", &text("ab"), operand),
                ("both", operand, operand),
            ] {
                let (answer, unknown) = run(a, b);
                if answer.is_err() || !unknown.iter().any(|(call, _)| call == counted) {
                    failures.push(format!(
                        "{method} {label} {operand:?}: answered {answer:?}, counted {unknown:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The comparator casts its first argument first, so a wrong class there wins
/// over a marker in the second; a marker in the first is counted before the
/// second is looked at, and a null beside a marker is counted, not thrown.
#[test]
fn case_insensitive_order_counts_markers_after_the_casts() {
    let fixture = Fixture::build(
        "string-ignore-case-counted-order",
        &[("demo/Counted.java", COUNTED)],
    );
    let marker = text("java/lang/String#3");
    let run = |a: Value, b: Value| {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Counted",
            "caseInsensitiveOrder",
            "(Ljava/lang/Object;Ljava/lang/Object;)I",
            None,
            &[a, b],
            &mut host,
        );
        (answer, host.unknown_host_calls())
    };
    let (answer, unknown) = run(Value::Int(1), marker.clone());
    assert!(
        format!("{answer:?}").contains("ClassCastException"),
        "{answer:?}"
    );
    assert!(unknown.is_empty(), "{unknown:?}");
    let (answer, unknown) = run(marker.clone(), Value::Int(1));
    assert!(answer.is_ok(), "{answer:?}");
    assert!(!unknown.is_empty());
    let (answer, unknown) = run(Value::Ref(None), marker);
    assert!(answer.is_ok(), "{answer:?}");
    assert!(!unknown.is_empty());
}
