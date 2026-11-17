#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{StdHost, Value};
const GSON: &str = r#"package com.google.gson; public class Gson { public Gson() {} public <T> T fromJson(java.io.Reader reader, Class<T> type) { return null; } }"#;
const PROBE: &str = include_str!("fixtures/gson_reader/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/gson_reader/java25.txt");
const COUNTED: &str = include_str!("fixtures/gson_reader/java25_counted.txt");
const RESOURCE: &[u8] = include_bytes!("fixtures/gson_reader/resource-config.json");
const SIG: &str =
    "com/google/gson/Gson#fromJson(Ljava/io/Reader;Ljava/lang/Class;)Ljava/lang/Object;";
fn call(f: &javac_fixture::Fixture, method: &str, host: &mut StdHost) -> String {
    match f
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            host,
        )
        .unwrap()
    {
        Value::Ref(Some(s)) => s,
        x => format!("{x:?}"),
    }
}
#[test]
fn gson_reader_matches_java25() {
    let fixture = javac_fixture::Fixture::build_with_lib(
        "gson-reader",
        &[
            ("com/google/gson/Gson.java", GSON),
            ("demo/Probe.java", PROBE),
        ],
        &[("resource-config.json", RESOURCE)],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(call(&fixture, "rows", &mut host), ORACLE);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
    let methods: Vec<_> = COUNTED.lines().filter(|x| !x.is_empty()).collect();
    assert_eq!(methods, ["trailing"]);
    for method in methods {
        let mut counted_host = fixture.jar.seed_host(StdHost::new());
        let _ = call(&fixture, method, &mut counted_host);
        assert_eq!(
            counted_host.unknown_host_calls(),
            vec![(SIG.to_owned(), 1)],
            "{method}: {:?}",
            counted_host.unknown_host_calls()
        );
    }
}
