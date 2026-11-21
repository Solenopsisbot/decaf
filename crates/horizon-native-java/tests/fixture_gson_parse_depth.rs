//! Gson's nesting limit (#262): a document nested to 255 levels parses, and the 256th opening
//! bracket throws gson-2.14.0's `JsonSyntaxException` with the reader's `MalformedJsonException`
//! as its cause, line, column and path included: the captured rows in
//! `fixtures/gson_parse_depth/java25.txt`. What the model cannot place stays counted.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const ORACLE: &str = include_str!("fixtures/gson_parse_depth/java25.txt");
const PROBE: &str = include_str!("fixtures/gson_parse_depth/demo/Depth.java");

/// Gson's API, compiled so `javac` accepts the probe; only `demo/` ships, so every call reaches
/// the host's Gson model as a real jar's does.
const STUBS: [(&str, &str); 5] = [
    (
        "com/google/gson/Gson.java",
        "package com.google.gson; public class Gson { public Gson() {} \
         public <T> T fromJson(String json, Class<T> type) { return null; } }",
    ),
    (
        "com/google/gson/JsonElement.java",
        "package com.google.gson; public abstract class JsonElement {}",
    ),
    (
        "com/google/gson/JsonParser.java",
        "package com.google.gson; public final class JsonParser { \
         public static JsonElement parseString(String json) { return null; } \
         public static JsonElement parseReader(java.io.Reader reader) { return null; } }",
    ),
    (
        "com/google/gson/JsonParseException.java",
        "package com.google.gson; public class JsonParseException extends RuntimeException { \
         public JsonParseException(String message) { super(message); } }",
    ),
    (
        "com/google/gson/JsonSyntaxException.java",
        "package com.google.gson; public final class JsonSyntaxException extends JsonParseException { \
         public JsonSyntaxException(String message) { super(message); } }",
    ),
];

fn fixture(name: &str, path: &'static str, source: &'static str) -> javac_fixture::Fixture {
    let mut files = vec![(path, source)];
    files.extend(STUBS);
    javac_fixture::Fixture::build(name, &files)
}

#[test]
fn nesting_matches_gson_2_14_on_java25() {
    let fixture = fixture("gson_parse_depth", "demo/Depth.java", PROBE);
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Depth",
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
    for (row, (ours, java)) in text.lines().zip(ORACLE.lines()).enumerate() {
        assert_eq!(ours, java, "row {row}");
    }
    assert_eq!(text, ORACLE);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

/// Past the limit, only a read whose outcome is the reader's is thrown. A document Gson rejects
/// earlier for another reason (`[1 2,`: "Unterminated array") and a typed target whose adapter
/// may refuse a shape first (`List`) keep being counted.
#[test]
fn what_the_model_cannot_place_is_counted() {
    const SOURCE: &str = r#"package demo;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
public class Counted {
  static String deep() { return "[".repeat(300) + "]".repeat(300); }
  public static int malformed() {
    try { JsonParser.parseString("[1 2," + deep() + "]"); return 0; } catch (RuntimeException e) { return 1; }
  }
  public static int typed() {
    try { new Gson().fromJson(deep(), java.util.List.class); return 0; } catch (RuntimeException e) { return 1; }
  }
}"#;
    let fixture = fixture("gson_parse_depth_counted", "demo/Counted.java", SOURCE);
    for (method, call) in [
        (
            "malformed",
            "com/google/gson/JsonParser#parseString(Ljava/lang/String;)Lcom/google/gson/JsonElement;",
        ),
        (
            "typed",
            "com/google/gson/Gson#fromJson(Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/Object;",
        ),
    ] {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Counted",
            method,
            "()I",
            None,
            &[],
            &mut host,
        );
        assert!(
            matches!(answer, Ok(horizon_native_java::Value::Int(0))),
            "{method}: {answer:?}"
        );
        assert!(
            host.unknown_host_calls()
                .iter()
                .any(|(name, _)| name == call),
            "{method}: {:?}",
            host.unknown_host_calls()
        );
    }
}
