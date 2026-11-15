//! A jar object cast to a JDK type it is not throws Java's
//! `ClassCastException` (#242), and `instanceof` agrees. Its class chain is
//! known: the held class files up to the class it leaves the jar at, then
//! Java 25's table above (`jdk_closure.rs`). The cast used to pass silently
//! whenever its target was a JDK type.
//!
//! `fixtures/jar_object_cast/demo/Probe.java` runs unchanged on both sides
//! (its `main` on Java 25 is `java25.txt`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

/// What `java -cp . demo.Probe` printed on Java 25 (Temurin 25.0.5, `javac
/// --release 21`), the same on two runs.
const ORACLE: &str = include_str!("fixtures/jar_object_cast/java25.txt");

const PROBE: &str = include_str!("fixtures/jar_object_cast/demo/Probe.java");

/// `Leaves`'s superclass: compiled with the probe, but left out of its jar
/// (only `demo/` is packed).
const UNHELD: &str = include_str!("fixtures/jar_object_cast/missing/UnheldBase.java");

const SOURCES: &[(&str, &str)] = &[
    ("demo/Probe.java", PROBE),
    ("missing/UnheldBase.java", UNHELD),
];

fn expected(method: &str) -> &'static str {
    ORACLE
        .lines()
        .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no oracle line for {method}"))
}

/// Each case casts with `checkcast` and tests with `instanceof`, as Java 25
/// answers, its messages exact, with nothing counted:
/// - `klass`: a plain jar class against `String`, `Number`, `Comparable`,
///   `Runnable`, `List`, `Object[]`, `String[]`, `int[]` and `Object`;
/// - `myList`: a jar subclass of `ArrayList` is a `List`, `Collection`,
///   `RandomAccess`, `ArrayList`, `AbstractList`, `Cloneable` and
///   `Serializable`, and no `Set`, `LinkedList` or `String`;
/// - `color`: a jar enum is an `Enum`, `Comparable` and `Serializable`;
/// - `point`: a jar record is a `Record`, and nothing it does not declare;
/// - `square`: a jar class with a JDK and a jar interface is both, and
///   nothing else.
#[test]
fn a_jar_object_casts_as_its_class_chain_says() {
    let fixture = javac_fixture::Fixture::build("jar-object-cast", SOURCES);
    let mut failures = Vec::new();
    for method in ["klass", "myList", "color", "point", "square"] {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let text = match answer {
            Ok(horizon_native_java::Value::Ref(Some(text))) => text,
            other => format!("{other:?}"),
        };
        let unknown = host.unknown_host_calls();
        if text != expected(method) || !unknown.is_empty() {
            failures.push(format!(
                "{method}\n  java:   {}\n  native: {text}\n  unknown: {unknown:?}",
                expected(method)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A jar class implementing a JDK interface Java 25's table holds no row
/// for (`Closeable`): whether it reaches the cast's type is not known here,
/// so the cast and `instanceof` are counted, never a silent answer. Java 25
/// answers `ok|ok|CCE(…)|TF`.
#[test]
fn a_chain_through_an_undescribed_jdk_interface_is_counted() {
    let fixture = javac_fixture::Fixture::build("jar-object-cast-res", SOURCES);
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "res",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    let text = match answer {
        Ok(horizon_native_java::Value::Ref(Some(text))) => text,
        other => format!("{other:?}"),
    };
    let unknown = host.unknown_host_calls();
    let counted = |call: &str| unknown.iter().any(|(counted, _)| counted.starts_with(call));
    assert!(
        text == expected("res")
            || (counted("java/lang/AutoCloseable#<checkcast>")
                && counted("java/lang/AutoCloseable#<instanceof>")
                && counted("java/lang/Runnable#<checkcast>")),
        "{text} {unknown:?}"
    );
}

/// A jar class whose superclass the jar leaves out (`missing.UnheldBase
/// extends Number`, #267 review): it may extend any JDK class that is not
/// final, so the casts to `Number` and `Runnable` and their `instanceof`s are
/// counted, never answered. Java 25 answers `ok|CCE(…)|CCE(…)|TFF`. `String`
/// is final, so no unheld class extends it: its cast is Java's CCE and its
/// `instanceof` false, uncounted.
#[test]
fn a_chain_through_an_unheld_superclass_is_counted() {
    let fixture = javac_fixture::Fixture::build("jar-object-cast-leaves", SOURCES);
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "leaves",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    let text = match answer {
        Ok(horizon_native_java::Value::Ref(Some(text))) => text,
        other => format!("{other:?}"),
    };
    let unknown = host.unknown_host_calls();
    let counted = |call: &str| unknown.iter().any(|(counted, _)| counted.starts_with(call));
    assert!(
        counted("java/lang/Number#<checkcast>")
            && counted("java/lang/Number#<instanceof>")
            && counted("java/lang/Runnable#<checkcast>")
            && counted("java/lang/Runnable#<instanceof>")
            && !counted("java/lang/String#"),
        "{text} {unknown:?}"
    );
    // The `String` parts are Java's: the third cast and the last flag.
    let java: Vec<&str> = expected("leaves").split('|').collect();
    let native: Vec<&str> = text.split('|').collect();
    assert_eq!(native.get(2), java.get(2), "{text}");
    assert_eq!(
        native.get(3).and_then(|flags| flags.chars().nth(2)),
        Some('F'),
        "{text}"
    );
}
