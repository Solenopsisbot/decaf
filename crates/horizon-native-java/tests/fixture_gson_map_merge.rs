//! Java 25 oracle fixture for `Map.merge` on maps Gson reads (#337).
//!
//! `fixtures/gson_map_merge/demo/Probe.java` is one source run two ways: on Java 25 against
//! the real Gson 2.14.0 (`java25.txt`, command in the README), and here, compiled against the
//! stubs below. Only `demo/` ships, so `fromJson` reaches the host's Gson model and `merge`
//! reaches the map it returns. `rows()` must equal the capture byte for byte, and the run must
//! leave no call counted: every row has an exact answer.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

// Signatures from `javap -public` on gson-2.14.0.jar; `throws JsonSyntaxException` dropped
// (it is unchecked).
const GSON: &str = r#"package com.google.gson;
public final class Gson {
 public Gson(){throw new AssertionError();}
 public <T> T fromJson(String json, Class<T> type){throw new AssertionError();}
 public <T> T fromJson(String json, java.lang.reflect.Type type){throw new AssertionError();}
}"#;
const TYPE_TOKEN: &str = r#"package com.google.gson.reflect;
public class TypeToken<T> {
 protected TypeToken(){throw new AssertionError();}
 public final java.lang.reflect.Type getType(){throw new AssertionError();}
}"#;
const PROBE: &str = include_str!("fixtures/gson_map_merge/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/gson_map_merge/java25.txt");

#[test]
fn merge_on_gson_maps_matches_java25() {
    let fixture = Fixture::build(
        "gson-map-merge",
        &[
            ("com/google/gson/Gson.java", GSON),
            ("com/google/gson/reflect/TypeToken.java", TYPE_TOKEN),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    // Both at once, so a failure shows which calls were counted beside the wrong rows.
    let unknown = format!("{:?}", host.unknown_host_calls());
    assert_eq!((rows.as_str(), unknown.as_str()), (ORACLE, "[]"));
}
