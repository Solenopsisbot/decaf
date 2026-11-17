#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const PROBE: &str = include_str!("fixtures/system_nanotime/demo/Probe.java");
const JAVA25: &str = include_str!("fixtures/system_nanotime/java25.txt");

#[test]
fn system_nanotime_properties_match_java_25() {
    let fixture = javac_fixture::Fixture::build("system-nanotime", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let answer = fixture
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
    let text = match answer {
        Value::Ref(Some(text)) => text,
        other => panic!("unexpected result: {other:?}"),
    };
    assert_eq!(text, JAVA25.trim_end());
    assert!(
        host.unknown_host_calls().is_empty(),
        "unknown calls: {:?}",
        host.unknown_host_calls()
    );
}
