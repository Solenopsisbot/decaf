//! `Annotation.toString()`, `hashCode()` and `equals()` of a held annotation
//! proxy, answered as Java 25 answers them.
//!
//! The proxy printed `@PAnno(value="marked")` where Java prints
//! `@demo.PAnno("marked")`: the simple name instead of the canonical one,
//! `value=` on a lone `value` element, elements in declaration order instead of
//! the order the JDK's `AnnotationInvocationHandler` holds them (the defaults
//! in `HashMap` bucket order, then the explicit elements the annotation type
//! gives no default, in class-file order), non-ASCII text raw instead of
//! `\uXXXX`, and a `Class` element by its binary name instead of its canonical
//! one. Nothing was counted, so a jar that prints an annotation read a wrong
//! answer.
//!
//! `fixtures/annotation_tostring/demo/Probe.java` runs unchanged on both
//! sides: its own `main` on Java 25 (output is `java25.txt`), its static
//! probes in Horizon. Every row is answered exactly except the ones in
//! [`COUNTED`], which stay counted unknown host calls.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

/// What `demo.Probe`'s `main` printed on OpenJDK 25.0.5.1 (`javac --release
/// 21`), the same on two runs; see the README beside it.
const ORACLE: &str = include_str!("fixtures/annotation_tostring/java25.txt");
const PROBE: &str = include_str!("fixtures/annotation_tostring/demo/Probe.java");

/// Rows Java answers that Horizon must count instead:
///
/// - `t_many*`, `t_dual*`: two defaulted elements share a `HashMap` bucket in the
///   annotation type's `memberDefaults`, so the printed order is the order
///   `getDeclaredMethods()` returned them in, which HotSpot does not define
///   (it followed neither declaration order nor its reverse).
/// - `*_lone*`, `*_fffd`: a string constant with an unpaired surrogate is read
///   as U+FFFD (#447), so a real U+FFFD cannot be told from it.
/// - `jdk_*`: the annotation type is a JDK class the runtime holds no class
///   file for, so it has no element list or defaults to read.
/// - `t_valueof`, `t_concat`: `String.valueOf(annotation)` and `"" + annotation`
///   print through the host's handle model, which holds no class file, and
///   were counted before this change (a direct `toString()` is the exact path).
/// - `t_params`: `Method.getParameterAnnotations()` is not modelled (a separate
///   counted call; what it returns is not an annotation to print).
/// - `x_class_*`, `x_enum_*`: `equals` of a `Class` or enum element keeps the
///   earlier decision to count it (their hash codes are identity hash codes and
///   are not in the oracle).
const COUNTED: &[&str] = &[
    "t_many0",
    "t_many1",
    "t_many2",
    "t_many3",
    "t_dual0",
    "t_dual1",
    "t_dual2",
    "x_class_same",
    "x_class_diff",
    "x_enum_same",
    "x_enum_diff",
    "t_valueof",
    "t_concat",
    "t_params",
    "t_jdk_deprecated",
    "t_jdk_functional",
    "h_str_fffd",
    "h_str_lone",
    "h_str_lone2",
    "t_str_fffd",
    "t_str_lone",
    "t_str_lone2",
];

fn fixture() -> Fixture {
    Fixture::build("annotation-tostring", &[("demo/Probe.java", PROBE)])
}

#[test]
fn annotation_text_matches_java_25() {
    let fixture = fixture();
    let mut failures = Vec::new();
    for line in ORACLE.lines() {
        let (method, java) = line.split_once('=').unwrap();
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let unknown = host.unknown_host_calls();
        let counted = COUNTED.contains(&method);
        let text = match answer {
            Ok(horizon_native_java::Value::Ref(Some(text))) => text,
            other => format!("{other:?}"),
        };
        if counted {
            if unknown.is_empty() {
                failures.push(format!(
                    "{method} must stay counted\n  java:   {java}\n  native: {text}"
                ));
            }
        } else if text != java || !unknown.is_empty() {
            failures.push(format!(
                "{method}\n  java:   {java}\n  native: {text}\n  unknown: {unknown:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
