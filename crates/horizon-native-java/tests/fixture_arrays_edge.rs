#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const ORACLE: &str = include_str!("fixtures/arrays_edge/java25.txt");
const PROBE: &str = include_str!("fixtures/arrays_edge/demo/Probe.java");
#[test]
fn arrays_edge_exceptions_match_java25() {
    let fixture = javac_fixture::Fixture::build("arrays-edge", &[("demo/Probe.java", PROBE)]);
    for method in ["getLength", "copySrc", "copyDst"] {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let v = fixture
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
        let got = match v {
            horizon_native_java::Value::Ref(Some(s)) => s,
            x => format!("{x:?}"),
        };
        let exp = ORACLE
            .lines()
            .find_map(|l| l.strip_prefix(method)?.strip_prefix('='))
            .unwrap();
        assert_eq!(got, exp, "{method}");
        assert!(host.unknown_host_calls().is_empty());
    }
}
