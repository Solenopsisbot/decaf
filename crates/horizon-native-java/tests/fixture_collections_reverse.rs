#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const ORACLE: &str = include_str!("fixtures/collections_reverse/java25.txt");
const PROBE: &str = include_str!("fixtures/collections_reverse/demo/Probe.java");
#[test]
fn collections_reverse_matches_java25() {
    let f = javac_fixture::Fixture::build("collections-reverse", &[("demo/Probe.java", PROBE)]);
    let mut h = f.jar.seed_host(horizon_native_java::StdHost::new());
    let got = f
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "all",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut h,
        )
        .unwrap();
    let got = match got {
        horizon_native_java::Value::Ref(Some(s)) => s,
        x => format!("{x:?}"),
    };
    assert!(ORACLE.contains("av=[3, 2, 1]") && ORACLE.contains("sub=[3, 2, 1]"));
    assert!(got.contains("array=[3, 2, 1] linked=[3, 2, 1] null=[3, null, 1] imm=ok imm=ok npe=NullPointerException imm2=UnsupportedOperationException"), "{got}");
    assert_eq!(
        h.unknown_host_calls().iter().map(|(_, n)| n).sum::<usize>(),
        2,
        "{:?}",
        h.unknown_host_calls()
    );
}
