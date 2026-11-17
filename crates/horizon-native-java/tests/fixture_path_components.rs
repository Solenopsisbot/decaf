#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const PROBE: &str = include_str!("fixtures/path_components/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/path_components/java25.txt");
#[test]
fn path_components() {
    let f = javac_fixture::Fixture::build("path-components", &[("demo/Probe.java", PROBE)]);
    let mut h = f.jar.seed_host(horizon_native_java::StdHost::new());
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
    let s = match v {
        horizon_native_java::Value::Ref(Some(x)) => x,
        _ => panic!("{v:?}"),
    };
    assert_eq!(
        (s, h.unknown_host_calls()),
        (ORACLE.to_string(), Vec::new())
    );
}
