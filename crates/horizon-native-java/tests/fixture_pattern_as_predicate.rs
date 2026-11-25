//! `java.util.regex.Pattern#asPredicate()` and the returned
//! `Predicate#test(Object)`, run through the interpreter against the JDK 25
//! answers in `fixtures/pattern_as_predicate/java25.txt`.
//!
//! Modelled exactly: a `Pattern` from `Pattern.compile(String)` (the only
//! `Pattern` the runtime makes, so flags 0) and a `test` on a `String`, which
//! is `matcher(s).find()` on a fresh matcher through the `java_regex` engine.
//! A jar's own `Predicate` (a lambda, a method reference, an anonymous or a
//! named class) runs as it always did. Everything else stays counted, never
//! guessed: a `Pattern` with flags, a pattern the engine does not compile, the class of the
//! predicate, and a `test` argument that is not a
//! `String`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/pattern_as_predicate/demo/Probe.java");
const ABANDONED: &str = "JDK body stopped at a counted host answer";
const ORACLE: &str = include_str!("fixtures/pattern_as_predicate/java25.txt");

/// Every oracle row, answered with no unknown host call.
#[test]
fn pattern_as_predicate_rows_match_java25() {
    let fixture = Fixture::build("pattern-as-predicate", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows"), ORACLE);
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}

/// `Pattern.compile(String, int)` is not modelled, so a `Pattern` with flags is
/// never a handle `asPredicate` answers for (the JDK prints `true` for
/// `CASE_INSENSITIVE`): the calls are counted, never answered as flags 0.
#[test]
fn a_pattern_with_flags_is_counted() {
    let fixture = Fixture::build("pattern-as-predicate-flags", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let outcome = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "flagged",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    let unknown = host.unknown_host_calls();
    assert!(
        unknown
            .iter()
            .any(|(call, _)| call
                .starts_with("java/util/regex/Pattern#compile(Ljava/lang/String;I)")),
        "outcome {outcome:?}, counted {unknown:?}"
    );
    // The flag-0 answer for "ABC" against "abc" is `false`: it must not appear.
    assert_ne!(
        outcome.ok(),
        Some(horizon_native_java::Value::Ref(Some(
            "flagged ABC = false".into()
        )))
    );
}

/// A supplementary character in the input: the engine refuses it, so `test` is
/// counted (the JDK prints `true`, `false`, and `removeIf` removes both elements),
/// never answered as UTF-16 units; `removeIf` stops at it rather than remove from a neutral `false`.
#[test]
fn a_supplementary_input_is_counted() {
    let fixture = Fixture::build("pattern-as-predicate-sup", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let outcome = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "supplementary",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    // Two direct `test` calls, then `removeIf`'s test of the first element, which stops it
    // (`removeIf left` is never printed as a definite size).
    assert_eq!(
        host.unknown_host_calls(),
        [(
            "java/util/function/Predicate#test(Ljava/lang/Object;)Z".to_owned(),
            3
        )],
        "answered {outcome:?}"
    );
    let error = outcome
        .expect_err("removeIf answered a supplementary input")
        .to_string();
    assert!(error.contains(ABANDONED), "{error}");
}

/// `test` on a `StringBuilder`, `null` or an `Integer`: the JDK throws
/// (`ClassCastException`, `NullPointerException`, `ClassCastException`), the
/// runtime counts the call and does not answer it as a match against the
/// object's marker.
#[test]
fn a_test_argument_that_is_not_a_string_is_counted() {
    let fixture = Fixture::build("pattern-as-predicate-args", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let outcome = fixture.text_on(&mut host, "nonString");
    assert_eq!(
        host.unknown_host_calls(),
        [(
            "java/util/function/Predicate#test(Ljava/lang/Object;)Z".to_owned(),
            3
        )],
        "answered {outcome:?}"
    );
}

/// Run `demo.Probe.<method>(row)` on a fresh host; the outcome text and the calls the runtime counted.
fn row_on_fresh_host(fixture: &Fixture, method: &str, row: &str) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let outcome = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            method,
            "(Ljava/lang/String;)Ljava/lang/String;",
            None,
            &[horizon_native_java::Value::Ref(Some(row.to_owned()))],
            &mut host,
        )
        .map(|value| format!("{value:?}"))
        .unwrap_or_else(|error| format!("error: {error}"));
    (outcome, host.unknown_host_calls())
}

fn oracle_rows(names: &[&str]) -> Vec<String> {
    let counted = include_str!("fixtures/pattern_as_predicate/java25_counted.txt");
    names
        .iter()
        .map(|name| {
            counted
                .lines()
                .find(|line| line.starts_with(&format!("{name} = ")))
                .unwrap_or_else(|| panic!("no JDK row for {name}"))
                .to_owned()
        })
        .collect()
}

/// `test` on an object that is not a `String` (a jar object, `new Object()`, a collection, a
/// `File`, a `BigInteger`, ...): the JDK throws `ClassCastException` whatever the pattern says
/// (`java25_counted.txt`), so the runtime counts every one of them, through the direct call and
/// through `removeIf`, `Stream.filter` and `anyMatch`. A host handle's key text is never read as
/// the `String` it is not.
#[test]
fn a_test_argument_that_is_an_object_is_counted() {
    let fixture = Fixture::build(
        "pattern-as-predicate-objects",
        &[("demo/Probe.java", PROBE)],
    );
    let rows = [
        "object",
        "object-nomatch",
        "object-keytext",
        "jar-object",
        "arraylist",
        "hashmap",
        "hashset",
        "list-of",
        "linkedlist",
        "file",
        "atomic-integer",
        "biginteger",
        "runtime-exception",
        "random",
        "string-joiner",
        "removeIf-object",
        "filter-object",
        "filter-object-nomatch",
        "anyMatch-object",
        // A real String that spells an object's key: counted as well, since the runtime cannot
        // tell it from the object (the JDK answers `true`).
        "string-key-text",
    ];
    for row in rows {
        let (outcome, unknown) = row_on_fresh_host(&fixture, "objectArgument", row);
        assert!(
            unknown
                .iter()
                .any(|(call, _)| call == "java/util/function/Predicate#test(Ljava/lang/Object;)Z"),
            "{row}: answered {outcome}, counted {unknown:?}"
        );
    }
    assert_eq!(
        oracle_rows(&["object", "file", "removeIf-object", "string-key-text"]),
        [
            "object = ClassCastException",
            "file = ClassCastException",
            "removeIf-object = ClassCastException",
            "string-key-text = true",
        ]
    );
}

/// `test` run by a JDK operation on an element the runtime cannot answer (a non-`String`, or a
/// `String` with a supplementary character): the count belongs to the enclosing operation too.
/// `removeIf` removes nothing from a list the JDK throws out of, a stream's `filter` (ended by
/// `collect` or `count`), `anyMatch`, `allMatch` and `noneMatch` give no answer, and neither do
/// `Optional.filter` and a `negate`d predicate. The call is counted and the run stops there
/// (`jdk_call::ABANDONED`); it never ends in a definite `removeIf` result, list, count or
/// match that the JDK did not print.
#[test]
fn an_unanswered_test_stops_the_enclosing_operation() {
    let fixture = Fixture::build(
        "pattern-as-predicate-enclosing",
        &[("demo/Probe.java", PROBE)],
    );
    let rows = [
        "removeIf-array-list-object",
        "removeIf-linked-list-object",
        "removeIf-hash-set-object",
        "removeIf-supplementary",
        "filter-collect-object",
        "filter-count-object",
        "filter-collect-supplementary",
        "filter-count-supplementary",
        "anyMatch-supplementary",
        "allMatch-object",
        "allMatch-supplementary",
        "noneMatch-object",
        "noneMatch-supplementary",
        "negate-removeIf-object",
        "optional-filter-object",
    ];
    for row in rows {
        let (outcome, unknown) = row_on_fresh_host(&fixture, "enclosing", row);
        assert!(
            unknown
                .iter()
                .any(|(call, _)| call == "java/util/function/Predicate#test(Ljava/lang/Object;)Z"),
            "{row}: answered {outcome}, counted {unknown:?}"
        );
        assert!(
            outcome.starts_with("error: ") && outcome.contains(ABANDONED),
            "{row}: the enclosing operation answered {outcome}"
        );
    }
    for row in [
        "removeIf-object",
        "filter-object",
        "filter-object-nomatch",
        "anyMatch-object",
    ] {
        let (outcome, _) = row_on_fresh_host(&fixture, "objectArgument", row);
        assert!(
            outcome.starts_with("error: ") && outcome.contains(ABANDONED),
            "{row}: the enclosing operation answered {outcome}"
        );
    }
    // What the JDK does for them: throws, or (a supplementary character) answers.
    assert_eq!(
        oracle_rows(&[
            "removeIf-array-list-object",
            "filter-collect-object",
            "allMatch-object",
            "removeIf-supplementary",
            "filter-count-supplementary",
            "noneMatch-supplementary",
        ]),
        [
            "removeIf-array-list-object = ClassCastException",
            "filter-collect-object = ClassCastException",
            "allMatch-object = ClassCastException",
            "removeIf-supplementary = true 1",
            "filter-count-supplementary = 1",
            "noneMatch-supplementary = false",
        ]
    );
}

/// The JDK's predicate is a hidden lambda class (`Pattern$$Lambda/0x...`), not
/// `java.util.function.Predicate`: every reflective question about its class is counted, none
/// answered from the interface.
#[test]
fn the_class_of_the_predicate_is_counted() {
    let fixture = Fixture::build("pattern-as-predicate-class", &[("demo/Probe.java", PROBE)]);
    for row in [
        "name-is-Predicate",
        "isInterface",
        "class-eq-Predicate",
        "simpleName-is-Predicate",
        "packageName",
        "modifiers",
        "name-starts-with-Pattern-Lambda",
        "toString-starts-with-class-Pattern-Lambda",
        "isHidden",
        "superclass",
        "interfaces",
    ] {
        let (outcome, unknown) = row_on_fresh_host(&fixture, "reflection", row);
        assert!(
            unknown
                .iter()
                .any(|(call, _)| call == "java/util/function/Predicate#getClass()Ljava/lang/Class;"),
            "{row}: answered {outcome}, counted {unknown:?}"
        );
    }
    // What the JDK prints for the rows the runtime does not answer.
    assert_eq!(
        oracle_rows(&[
            "name-is-Predicate",
            "isInterface",
            "modifiers",
            "interfaces"
        ]),
        [
            "name-is-Predicate = false",
            "isInterface = false",
            "modifiers = 4112",
            "interfaces = [interface java.util.function.Predicate]",
        ]
    );
}

/// A pattern the JDK rejects at `compile` (a `PatternSyntaxException`), or a null text (a
/// `NullPointerException`): `Pattern.compile(String)` does not check its text, so these reach
/// `asPredicate`, which counts them rather than answer a predicate for a call that never returns.
#[test]
fn a_pattern_the_jdk_rejects_is_counted() {
    let fixture = Fixture::build("pattern-as-predicate-bad", &[("demo/Probe.java", PROBE)]);
    for row in [
        "unclosed-group",
        "unclosed-class",
        "dangling-star",
        "bad-repeat",
        "trailing-backslash",
        "null",
    ] {
        let (outcome, unknown) = row_on_fresh_host(&fixture, "badPattern", row);
        assert!(
            unknown.iter().any(
                |(call, _)| call.starts_with("java/util/regex/Pattern#asPredicate")
                    || call.starts_with("java/util/regex/Pattern#compile")
            ),
            "{row}: answered {outcome}, counted {unknown:?}"
        );
        assert!(
            !outcome.contains("= true") && !outcome.contains("= false"),
            "{row}: answered {outcome}"
        );
    }
    assert_eq!(
        oracle_rows(&["unclosed-group", "null"]),
        [
            "unclosed-group = PatternSyntaxException",
            "null = NullPointerException"
        ]
    );
}
