//! Regression fixture for JVMS §6.5 `wide`: javac uses it for locals above
//! 255 and for `iinc` constants outside signed byte range.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

const ORACLE: &str = include_str!("fixtures/wide-opcode/java25.txt");
const PROBE: &str = include_str!("fixtures/wide-opcode/demo/Probe.java");

#[test]
fn wide_locals_and_iinc_match_java25() {
    let fixture = javac_fixture::Fixture::build("wide-opcode", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "rows",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    let actual = match answer {
        Ok(horizon_native_java::Value::Ref(Some(text))) => text,
        other => format!("{other:?}"),
    };
    assert_eq!(actual, ORACLE.trim_end());
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
