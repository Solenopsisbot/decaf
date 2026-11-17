//! `Gson.toJson(Object, Appendable)` into the targets mods save their configs through (#458).
//!
//! `fixtures/gson_appendable_targets/demo/Probe.java` runs on Java 25 with Gson 2.14.0 (`java25.txt`,
//! command in the README), and here with Gson stubbed for compiling only: only `demo/` ships, so
//! every Gson call reaches the host. A `StringBuilder` and a `Files.newBufferedWriter` file (read
//! back) must hold exactly Java's text, including pretty printing, `serializeNulls` and a null
//! source, with nothing counted. An `Appendable` Decaf doesn't model (a jar class) is counted.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const GSON: &str = r#"package com.google.gson; public final class Gson { public Gson() {} public void toJson(Object src, Appendable writer) {} }"#;
const GSON_BUILDER: &str = r#"package com.google.gson; public final class GsonBuilder { public GsonBuilder setPrettyPrinting() { return this; } public GsonBuilder serializeNulls() { return this; } public Gson create() { return null; } }"#;
const PROBE: &str = include_str!("fixtures/gson_appendable_targets/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/gson_appendable_targets/java25.txt");

fn fixture() -> Fixture {
    Fixture::build(
        "gson-appendable-targets",
        &[
            ("com/google/gson/Gson.java", GSON),
            ("com/google/gson/GsonBuilder.java", GSON_BUILDER),
            ("demo/Probe.java", PROBE),
        ],
    )
}

#[test]
fn modelled_targets_hold_java25_text() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows"), ORACLE);
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}

#[test]
fn a_jar_appendable_is_counted() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    // Java 25 writes the same JSON into it (`java25_counted.txt`); Decaf can't run Gson's writes
    // through a jar's own `append` methods, so it counts the call.
    fixture.text_on(&mut host, "unmodelled");
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "com/google/gson/Gson#toJson(Ljava/lang/Object;Ljava/lang/Appendable;)V".to_owned(),
            1
        )]
    );
}
