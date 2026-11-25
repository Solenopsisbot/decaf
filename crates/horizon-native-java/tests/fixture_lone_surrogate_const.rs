//! A `String` constant (constant pool) holding an unpaired surrogate (#447, hunt H24 `sur.*`).
//!
//! Rust text cannot hold one, so the constant used to be read with U+FFFD in its place:
//! `"\ud83d".hashCode()` answered 65533 where Java 25 answers 55357, and `"\ud83d".equals("\ude00")`
//! answered `true` where Java answers `false`. Every use of such a constant is now counted; a constant
//! holding only pairs and genuine U+FFFD is still answered exactly. `java25.txt` is Java 25's output
//! of `demo/Probe.java`, one `label=value` line per row (see `fixtures/lone_surrogate_const/README.md`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{NativeError, StdHost, Value};
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/lone_surrogate_const/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/lone_surrogate_const/java25.txt");

/// What one row did on a fresh host: the answer and the unknown host calls it counted.
struct Row {
    label: String,
    answer: Result<String, String>,
    counted: Vec<String>,
}

fn run_row(
    fixture: &Fixture,
    host: StdHost,
    index: i32,
) -> (Result<Value, NativeError>, Vec<String>) {
    let mut host = fixture.jar.seed_host(host);
    let got = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "row",
        "(I)Ljava/lang/String;",
        None,
        &[Value::Int(index)],
        &mut host,
    );
    let counted = host
        .unknown_host_calls()
        .into_iter()
        .map(|(call, _)| call)
        .collect();
    (got, counted)
}

fn rows(fixture: &Fixture) -> Vec<Row> {
    ORACLE
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let label = line.split_once('=').unwrap().0.to_owned();
            let (got, counted) = run_row(fixture, StdHost::new(), index as i32);
            let answer = match got {
                Ok(Value::Ref(Some(text))) => Ok(text),
                Ok(other) => Err(format!("{other:?}")),
                Err(error) => Err(format!("{error:?}")),
            };
            Row {
                label,
                answer,
                counted,
            }
        })
        .collect()
}

#[test]
fn the_oracle_has_a_line_for_every_row() {
    // `Probe.ROWS` is 36: 11 `ok.*` rows and 25 `lone.*` rows.
    assert_eq!(ORACLE.lines().count(), 36);
    assert_eq!(ORACLE.lines().filter(|l| l.starts_with("ok.")).count(), 11);
    assert_eq!(
        ORACLE.lines().filter(|l| l.starts_with("lone.")).count(),
        25
    );
}

/// A constant with no unpaired surrogate is answered exactly and counts nothing: a supplementary
/// character (a pair), a genuine U+FFFD, a folded `"\ud83d" + "\ude00"`, and a concatenation or a
/// `switch` over them.
#[test]
fn constants_without_an_unpaired_surrogate_are_answered_exactly() {
    let fixture = Fixture::build("lone-const-ok", &[("demo/Probe.java", PROBE)]);
    let mut failures = Vec::new();
    let mut checked = 0;
    for (row, line) in rows(&fixture).iter().zip(ORACLE.lines()) {
        if !row.label.starts_with("ok.") {
            continue;
        }
        checked += 1;
        if row.answer.as_deref() != Ok(line) {
            failures.push(format!(
                "{}\n  java25: {line}\n  ours:   {:?}",
                row.label, row.answer
            ));
        }
        if !row.counted.is_empty() {
            failures.push(format!("{} counted {:?}", row.label, row.counted));
        }
    }
    assert_eq!(checked, 11);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A use of a constant with an unpaired surrogate is never answered as if it were exact: the row
/// counted a call (`hashCode`, `charAt`, `length`, `equals`, a map key, a `switch`, a concatenation,
/// a type switch ...) or the run refused by name. The old answers were silently wrong
/// (`sur.litHigh=65533`, `equalsOther=true`), and a silent answer that happens to equal Java's is a
/// use of an inexact value too.
#[test]
fn every_use_of_a_lone_surrogate_constant_is_counted() {
    let fixture = Fixture::build("lone-const-counted", &[("demo/Probe.java", PROBE)]);
    let mut failures = Vec::new();
    let mut checked = 0;
    for row in rows(&fixture) {
        if !row.label.starts_with("lone.") {
            continue;
        }
        checked += 1;
        let refused = row
            .answer
            .as_ref()
            .is_err_and(|error| error.starts_with("UnknownHostCall"));
        if row.counted.is_empty() && !refused {
            failures.push(format!("{} counted nothing: {:?}", row.label, row.answer));
        }
    }
    assert_eq!(checked, 25);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What each kind of use counts: `ldc` loads, a `StringConcatFactory` recipe with the half in it, and
/// a `typeSwitch` label.
#[test]
fn the_counted_calls_name_the_constant_load_the_concatenation_and_the_label() {
    let fixture = Fixture::build("lone-const-names", &[("demo/Probe.java", PROBE)]);
    let all = rows(&fixture);
    let counted = |label: &str| {
        all.iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("no row {label}"))
            .counted
            .clone()
    };
    let ldc = "java/lang/String#ldc()Ljava/lang/String;";
    assert!(counted("lone.highHash").iter().any(|call| call == ldc));
    assert!(counted("lone.switchHigh").iter().any(|call| call == ldc));
    assert!(counted("lone.finalHash").iter().any(|call| call == ldc));
    assert!(counted("lone.fieldLength").iter().any(|call| call == ldc));
    assert!(
        counted("lone.concatRecipeHash")
            .iter()
            .any(|call| call
                .starts_with("java/lang/invoke/StringConcatFactory#makeConcatWithConstants")),
        "{:?}",
        counted("lone.concatRecipeHash")
    );
    assert!(
        counted("lone.concatTailHash")
            .iter()
            .any(|call| call
                .starts_with("java/lang/invoke/StringConcatFactory#makeConcatWithConstants")),
        "{:?}",
        counted("lone.concatTailHash")
    );
    assert!(
        counted("lone.typeSwitch").iter().any(|call| call == ldc),
        "{:?}",
        counted("lone.typeSwitch")
    );
}

/// A strict host (it refuses a call it cannot count) stops at the load, naming it.
#[test]
fn a_strict_host_refuses_the_load() {
    let fixture = Fixture::build("lone-const-strict", &[("demo/Probe.java", PROBE)]);
    let index = ORACLE
        .lines()
        .position(|line| line.starts_with("lone.highHash="))
        .unwrap() as i32;
    let (got, _) = run_row(
        &fixture,
        StdHost::new().with_strict_unknown_host_calls(),
        index,
    );
    match got {
        Err(NativeError::UnknownHostCall(call)) => {
            assert_eq!(call.owner, "java/lang/String");
            assert_eq!(call.name, "ldc");
        }
        other => panic!("expected the strict refusal, got {other:?}"),
    }
}

/// A `ConstantValue` field read through reflection (javac inlines the constant, so only reflection
/// or hand-made bytecode reads it) is refused by name, never answered with the replaced text.
#[test]
fn a_constant_value_field_with_a_lone_surrogate_is_refused() {
    let fixture = Fixture::build("lone-const-field", &[("demo/Probe.java", PROBE)]);
    let index = ORACLE
        .lines()
        .position(|line| line.starts_with("lone.reflectFinal="))
        .unwrap() as i32;
    let (got, _) = run_row(&fixture, StdHost::new(), index);
    match got {
        Err(NativeError::UnknownHostCall(call)) => {
            assert_eq!(call.owner, "java/lang/String");
            assert_eq!(call.name, "ldc");
        }
        other => panic!("reflection answered a lone-surrogate constant: {other:?}"),
    }
}
