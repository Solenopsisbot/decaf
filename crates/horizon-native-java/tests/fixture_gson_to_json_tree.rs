//! Java 25 Gson fixture, using real Gson 2.14.0 when the corpus jar is available.
//!
//! In a source-only CI checkout the fallback signature stubs keep the same
//! probe bytecode shape; the real-corpus run is selected by HORIZON_GSON_JAR.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const PROBE: &str = r#"package demo;
import com.google.gson.*;
import com.google.gson.annotations.*;
import java.util.*;

public final class Probe {
  static class Base {
    int base = 1;
    transient int transientValue = 2;
    static int staticValue = 3;
    @SerializedName("renamed") String label = "x";
    @Expose String exposed = "e";
  }
  static class Child extends Base {
    Integer boxed = 2;
    String nullable = null;
    List<String> list = List.of("a", "b");
    Map<String, Integer> map = Map.of("k", 3);
    int[] array = {4, 5};
    Kind kind = Kind.B;
    String someValue = "z";
    double one = 1.0;
    float fone = 1.0f;
  }
  enum Kind { A, @SerializedName("bee") B }
  record Rec(int x, String s) {}

  public static String plain() {
    return new Gson().toJsonTree(new Child()).toString();
  }
  public static String nulls() {
    return new GsonBuilder().serializeNulls().create().toJsonTree(new Child()).toString();
  }
  public static String naming() {
    return new GsonBuilder().setFieldNamingPolicy(FieldNamingPolicy.LOWER_CASE_WITH_UNDERSCORES)
        .create().toJsonTree(new Child()).toString();
  }
  public static String counted() {
    Gson gson = new GsonBuilder().registerTypeAdapter(Child.class,
        (JsonSerializer<Child>) (value, type, context) -> new JsonPrimitive("custom")).create();
    return String.valueOf(gson.toJsonTree(new Child()));
  }
  public static String special() {
    StringBuilder out = new StringBuilder();
    try { out.append(new Gson().toJsonTree(Double.NaN)); }
    catch (RuntimeException e) { out.append(e.getClass().getSimpleName()).append('|').append(e.getMessage()); }
    out.append('|');
    out.append(new GsonBuilder().serializeSpecialFloatingPointValues().create()
        .toJsonTree(Double.NaN).toString());
    return out.toString();
  }
  public static String record() {
    return String.valueOf(new Gson().toJsonTree(new Rec(1, "s")));
  }
}
"#;

const STUBS: &[(&str, &str)] = &[
    (
        "com/google/gson/Gson.java",
        r#"package com.google.gson;
public final class Gson {
  public Gson() {}
  public JsonElement toJsonTree(Object value) { return null; }
}
"#,
    ),
    (
        "com/google/gson/GsonBuilder.java",
        r#"package com.google.gson;
public final class GsonBuilder {
  public GsonBuilder() {}
  public GsonBuilder serializeNulls() { return this; }
  public GsonBuilder serializeSpecialFloatingPointValues() { return this; }
  public GsonBuilder setFieldNamingPolicy(FieldNamingPolicy policy) { return this; }
  public GsonBuilder registerTypeAdapter(Class<?> type, Object adapter) { return this; }
  public Gson create() { return null; }
}
"#,
    ),
    (
        "com/google/gson/JsonElement.java",
        r#"package com.google.gson;
public abstract class JsonElement {
  @Override public String toString() { return null; }
}
"#,
    ),
    (
        "com/google/gson/JsonPrimitive.java",
        r#"package com.google.gson;
public final class JsonPrimitive extends JsonElement {
  public JsonPrimitive(String value) {}
}
"#,
    ),
    (
        "com/google/gson/JsonSerializer.java",
        r#"package com.google.gson;
import java.lang.reflect.Type;
public interface JsonSerializer<T> {
  JsonElement serialize(T value, Type type, JsonSerializationContext context);
}
"#,
    ),
    (
        "com/google/gson/JsonSerializationContext.java",
        r#"package com.google.gson;
public interface JsonSerializationContext {}
"#,
    ),
    (
        "com/google/gson/FieldNamingPolicy.java",
        r#"package com.google.gson;
public enum FieldNamingPolicy { LOWER_CASE_WITH_UNDERSCORES }
"#,
    ),
    (
        "com/google/gson/annotations/SerializedName.java",
        r#"package com.google.gson.annotations;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface SerializedName { String value(); }
"#,
    ),
    (
        "com/google/gson/annotations/Expose.java",
        r#"package com.google.gson.annotations;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.FIELD)
public @interface Expose {}
"#,
    ),
];

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}
fn gson_jar() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HORIZON_GSON_JAR").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    [
        PathBuf::from("/home/infer/ava/mc-local/minecraft/26.3/libraries/client/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar"),
        PathBuf::from(".local/minecraft/26.3/libraries/client/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}
fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}
fn write_sources(root: &Path, sources: &[(&str, &str)]) -> Vec<PathBuf> {
    sources
        .iter()
        .map(|(path, source)| {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            file
        })
        .collect()
}

#[test]
fn gson_to_json_tree_matches_real_gson_or_signature_fallback() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "horizon-gson-to-json-tree-{}-{unique}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let classes = root.join("classes");
    let src_root = root.join("src");
    fs::create_dir_all(&classes).unwrap();
    let src = write_sources(&src_root, &[("demo/Probe.java", PROBE)]);
    let gson = gson_jar();
    let mut compile = if let Some(gson) = gson.as_ref() {
        let mut command = Command::new(javac());
        command.args(["--release", "25", "-cp"]).arg(gson).args([
            "-d",
            &classes.to_string_lossy(),
            src[0].to_str().unwrap(),
        ]);
        command
    } else {
        let stub_root = root.join("stubs");
        let stub_classes = root.join("stub-classes");
        let stub_files = write_sources(&stub_root, STUBS);
        let mut stubs = Command::new(javac());
        stubs
            .args(["--release", "25", "-d"])
            .arg(&stub_classes)
            .args(&stub_files);
        assert!(
            stubs.status().unwrap().success(),
            "javac failed for Gson signature stubs"
        );
        let mut command = Command::new(javac());
        command
            .args(["--release", "25", "-cp"])
            .arg(&stub_classes)
            .args(["-d", &classes.to_string_lossy(), src[0].to_str().unwrap()]);
        command
    };
    assert!(
        compile.status().unwrap().success(),
        "javac failed for Gson fixture"
    );
    let jar_path = root.join("fixture.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    zip.start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(
        br#"{"id":"gson-to-json-tree","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
    )
    .unwrap();
    for class in files_under(&classes) {
        let entry = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !entry.starts_with("demo/") {
            continue;
        }
        zip.start_file(entry, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    let jar = NativeJar::from_path(&jar_path).unwrap();
    for (method, expected) in [
        (
            "plain",
            r#"{"boxed":2,"list":["a","b"],"map":{"k":3},"array":[4,5],"kind":"bee","someValue":"z","one":1.0,"fone":1.0,"base":1,"renamed":"x","exposed":"e"}"#,
        ),
        (
            "nulls",
            r#"{"boxed":2,"nullable":null,"list":["a","b"],"map":{"k":3},"array":[4,5],"kind":"bee","someValue":"z","one":1.0,"fone":1.0,"base":1,"renamed":"x","exposed":"e"}"#,
        ),
        (
            "naming",
            r#"{"boxed":2,"list":["a","b"],"map":{"k":3},"array":[4,5],"kind":"bee","some_value":"z","one":1.0,"fone":1.0,"base":1,"renamed":"x","exposed":"e"}"#,
        ),
    ] {
        let mut host = jar.seed_host(StdHost::new());
        let result = jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        assert_eq!(
            result.unwrap(),
            Value::Ref(Some(expected.into())),
            "{method}: {:?}",
            host.unknown_host_calls()
        );
        assert!(host.unknown_host_calls().is_empty());
    }
    let mut host = jar.seed_host(StdHost::new());
    let special = jar
        .invoke_method_with_args(
            "demo/Probe",
            "special",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let expected_message = "NaN is not a valid double value as per JSON specification. To override this behavior, use GsonBuilder.serializeSpecialFloatingPointValues() method.";
    assert_eq!(
        special,
        Value::Ref(Some(format!(
            "IllegalArgumentException|{expected_message}|NaN"
        ),))
    );
    assert!(host.unknown_host_calls().is_empty());

    let mut host = jar.seed_host(StdHost::new());
    let record = jar
        .invoke_method_with_args(
            "demo/Probe",
            "record",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    assert_eq!(record, Value::Ref(Some("null".into())));
    assert!(
        host.unknown_host_calls()
            .iter()
            .any(|(call, _)| call.contains("com/google/gson/Gson#toJsonTree"))
    );

    let mut host = jar.seed_host(StdHost::new());
    let value = jar
        .invoke_method_with_args(
            "demo/Probe",
            "counted",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    assert_eq!(value, Value::Ref(Some("null".into())));
    assert_eq!(host.unknown_host_calls().len(), 1);
    let _ = fs::remove_dir_all(root);
}
