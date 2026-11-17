use horizon_native_java::{StdHost, Value};
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const GSON: &str = r#"package com.google.gson; public class Gson { public Gson() {} public <T> T fromJson(String s, Class<T> c) { return null; } }"#;
const PROBE: &str = include_str!("fixtures/gson_casts/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/gson_casts/java25.txt");
#[test]
fn gson_linked_tree_map_casts() {
    let f = javac_fixture::Fixture::build(
        "gson-casts",
        &[
            ("com/google/gson/Gson.java", GSON),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut h = f.jar.seed_host(StdHost::new());
    let out = f
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
    assert_eq!(out, Value::Ref(Some(ORACLE.trim_end().into())));
    assert!(
        h.unknown_host_calls().is_empty(),
        "{:?}",
        h.unknown_host_calls()
    );
}
