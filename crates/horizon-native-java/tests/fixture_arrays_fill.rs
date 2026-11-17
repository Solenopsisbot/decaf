#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const ORACLE: &str = include_str!("fixtures/arrays-fill/java25.txt");
const PROBE: &str = include_str!("fixtures/arrays-fill/demo/Probe.java");
#[test]
fn arrays_fill_matches_java25() {
    let fixture = javac_fixture::Fixture::build("arrays-fill", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture.jar.invoke_method(
        "demo/Probe",
        "rows",
        "()Ljava/lang/String;",
        None,
        &mut host,
    );
    let text = match answer {
        Ok(horizon_native_java::Value::Ref(Some(text))) => text,
        other => format!("{other:?}"),
    };
    assert_eq!(text, ORACLE);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
