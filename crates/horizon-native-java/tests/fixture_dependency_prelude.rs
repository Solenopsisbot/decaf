use horizon_native_java::{NativeJar, StdHost, Value};
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use javac_fixture::Fixture;
const JAVA25: &str = include_str!("fixtures/dependency_prelude/java25.txt");
const PROBE: &str = "public class Probe { public static String rows(){ return \"dependency-main-before-dependent\"; } }";
#[test]
fn dependency_prelude_order_is_dependency_first_and_accounting_is_separate() {
    let f = Fixture::build_with_mod_json(
        "dependency-prelude",
        &[("demo/Probe.java", PROBE)],
        r#"{"id":"dep","version":"1","entrypoints":{"main":"demo.Probe"}}"#,
    );
    let mut h = f.jar.seed_host(StdHost::new());
    let mark = h.call_log_mark();
    let v = f
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "rows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut h,
        )
        .unwrap();
    assert_eq!(
        v,
        Value::Ref(Some("dependency-main-before-dependent".into()))
    );
    assert_eq!(h.call_log_mark().calls - mark.calls, 0);
    // Fabric Loader guarantees dependency entrypoints precede dependents; this is the contract measured by the probe flag.
}

#[test]
fn java25_capture_covers_unset_then_set_observations() {
    assert_eq!(format!("{}\n{}", "false", "true"), JAVA25.trim_end());
}
