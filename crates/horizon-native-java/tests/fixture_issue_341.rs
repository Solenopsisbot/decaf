#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const ORACLE: &str = include_str!("fixtures/issue_341/java25.txt");
const PROBE: &str = include_str!("fixtures/issue_341/demo/Probe.java");
#[test]
fn byte_array_input_stream_matches_java25() {
    let fixture = javac_fixture::Fixture::build("issue-341", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let value = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "rows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let text = match value {
        horizon_native_java::Value::Ref(Some(s)) => s,
        other => panic!("{other:?}"),
    };
    assert_eq!(text, ORACLE);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
