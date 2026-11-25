//! `i2c` (0x92) keeps the low 16 bits of an int as an unsigned char: JVMS 6.5 truncates to `char`, then
//! zero-extends. `i2b` and `i2s` truncate to 8 and 16 bits and sign-extend. The probe feeds values outside
//! each target range (from a static array, so javac cannot fold the casts) through casts, compound
//! assignments, increments, field and array stores, calls and strings. `java25.txt` is Java 25's output
//! for each method, one `name=value` line per method (see `fixtures/i2c_char_cast/README.md`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

const ORACLE: &str = include_str!("fixtures/i2c_char_cast/java25.txt");
const PROBE: &str = include_str!("fixtures/i2c_char_cast/demo/Probe.java");

#[test]
fn i2c_char_cast_matches_java25() {
    let fixture = javac_fixture::Fixture::build("i2c-char-cast", &[("demo/Probe.java", PROBE)]);
    let mut mismatches = Vec::new();
    let mut checked = 0;
    for line in ORACLE.lines() {
        let (method, expected) = line.split_once('=').unwrap();
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let got = fixture
            .jar
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap();
        let got = match got {
            horizon_native_java::Value::Ref(Some(s)) => s,
            other => format!("{other:?}"),
        };
        if got != expected {
            mismatches.push(format!("{method}\n  java25: {expected}\n  ours:   {got}"));
        }
        assert!(
            host.unknown_host_calls().is_empty(),
            "{method}: unmodelled host calls {:?}",
            host.unknown_host_calls()
        );
        checked += 1;
    }
    assert_eq!(checked, 13, "every probe method has an oracle line");
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

const LONE: &str = include_str!("fixtures/i2c_char_cast/demo/Lone.java");

/// A `(char)` cast now yields the real 16-bit value, so a lone surrogate half can reach a string
/// concatenation. Rust text cannot hold one, so the runtime must count it, not print its number.
#[test]
fn a_lone_surrogate_in_a_concatenation_is_counted_and_other_chars_are_answered() {
    let fixture = javac_fixture::Fixture::build("i2c-lone", &[("demo/Lone.java", LONE)]);
    let run = |method: &str| {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let got = fixture.jar.invoke_method_with_args(
            "demo/Lone",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        (got, host.unknown_host_calls())
    };
    // Java 25: "a中", two chars (`java25` run, 2026-11-22). Answered, nothing counted.
    let (got, counted) = run("plainConcat");
    assert_eq!(
        got.unwrap(),
        horizon_native_java::Value::Ref(Some("a\u{4e2d}".to_string()))
    );
    assert!(counted.is_empty(), "plainConcat counted {counted:?}");
    // Java 25: "a\ud800", two chars. Not representable here, so it must be counted.
    let (_, counted) = run("loneConcat");
    assert!(
        counted
            .iter()
            .any(|(call, _)| call == "java/lang/String#valueOf(C)Ljava/lang/String;"),
        "loneConcat counted {counted:?}"
    );
}
