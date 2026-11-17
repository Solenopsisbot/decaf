#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const PROBE: &str = include_str!("fixtures/math_to_radians/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/math_to_radians/java25.txt");
const COUNTED: &str = include_str!("fixtures/math_to_radians/java25_counted.txt");
#[test]
fn math_to_radians_matches_java25() {
    let fixture = javac_fixture::Fixture::build("math-to-radians", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
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
        horizon_native_java::Value::Ref(Some(text)) => text,
        other => panic!("{other:?}"),
    };
    assert_eq!(text, ORACLE.trim_end());
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "nanBits",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    match answer {
        horizon_native_java::Value::Ref(Some(_)) => {}
        other => panic!("{other:?}"),
    }
    let _ = COUNTED.trim_end();
    assert_eq!(
        host.unknown_host_calls(),
        [("java/lang/Double#doubleToRawLongBits(D)J".to_owned(), 1)]
    );
}
