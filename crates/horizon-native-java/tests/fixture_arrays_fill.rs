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
    let expected = ORACLE
        .lines()
        .filter(|line| !line.starts_with("object-nonstr=") && !line.starts_with("char-surrogate="))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let actual = text
        .lines()
        .filter(|line| !line.starts_with("object-nonstr=") && !line.starts_with("char-surrogate="))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(actual, expected);
    // The two rows left out above are counted, each once: an `Object[]` holding a plain
    // `Object` (Java prints its identity hash) and a `char[]` holding a lone surrogate.
    assert_eq!(
        host.unknown_host_calls(),
        vec![
            ("java/util/Arrays#toString([C)Ljava/lang/String;".into(), 1),
            (
                "java/util/Arrays#toString([Ljava/lang/Object;)Ljava/lang/String;".into(),
                1
            ),
        ]
    );
}
