#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const PROBE: &str = include_str!("fixtures/properties-load/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/properties-load/java25.txt");
#[test]
fn properties_load_matches_java25() {
    let extras: &[(&str, &[u8])] = &[];
    let f = javac_fixture::Fixture::build_with_lib(
        "properties-load",
        &[("demo/Probe.java", PROBE)],
        extras,
    );
    let mut h = f.jar.seed_host(horizon_native_java::StdHost::new());
    let got = match f.jar.invoke_method_with_args(
        "demo/Probe",
        "rows",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut h,
    ) {
        Ok(horizon_native_java::Value::Ref(Some(s))) => s,
        x => panic!("{x:?}"),
    };
    assert_eq!(got, ORACLE);
    assert!(
        h.unknown_host_calls().is_empty(),
        "{:?}",
        h.unknown_host_calls()
    );
}

/// `Properties.load` treats `\r\n` as one line terminator, so a `\` continuation carries
/// across it. Java 25 (`new Properties().load(new StringReader("a=b\\\r\n  c\r\nd=e\r\n"))`)
/// gives `{a=bc, d=e}`.
#[test]
fn crlf_continuation_matches_java25() {
    assert_eq!(
        horizon_native_java::properties_entries_for_tests("a=b\\\r\n  c\r\nd=e\r\n"),
        Ok(vec![
            ("a".to_owned(), "bc".to_owned()),
            ("d".to_owned(), "e".to_owned())
        ])
    );
}
