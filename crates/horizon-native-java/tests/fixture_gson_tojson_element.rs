//! `Gson.toJson(JsonElement)` and its `Appendable`/`JsonWriter` forms against Java 25 (F7).
//!
//! A tree (`JsonObject`, `JsonArray`, `JsonPrimitive`, `JsonNull`) is written by
//! `JsonElement`'s own adapter: the `Gson`'s type adapters, naming and exclusion
//! strategies, `@Expose`, date format, version and number strategies never see it.
//! A mod that builds its `Gson` with any of them (Mod Menu registers an `Enum`
//! hierarchy adapter and a field naming policy) therefore still gets exactly the
//! default writer's text from `toJson(JsonElement)`, and the runtime used to count
//! the call and answer a neutral `null`, which the mod then read back as
//! "Not a JSON Object: null".
//!
//! `fixtures/gson_tojson_element/demo/Probe.java` runs on Java 25 with the real
//! gson-2.14.0 jar (`oracle/java25.txt`, command in the README). Here it is
//! compiled against signature-only copies of the Gson API it calls (`STUBS`) and
//! only `demo/` ships, so every Gson call reaches the runtime's model. Each
//! oracle row is one `Probe` method; the runtime's answer, with `\n`, `\r` and
//! `\t` escaped as the probe does, must equal the row byte for byte with nothing
//! counted. `COUNTED` rows are ones the runtime must keep counting.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;

const ORACLE: &str = include_str!("fixtures/gson_tojson_element/oracle/java25.txt");
const PROBE: &str = include_str!("fixtures/gson_tojson_element/demo/Probe.java");

/// Rows the runtime keeps counting, with what Java 25 answers in `java25.txt`:
/// - `nanDefault`/`infinityInArray`: a non-finite number. Java writes `NaN` and
///   `Infinity` for a default `Gson` (its writer is lenient) and throws for a
///   strict one; the runtime does not record that, so it counts the call.
/// - `objectWithAdapter`: `toJson(Object)`, not `toJson(JsonElement)`, with an
///   adapter registered. Java ignores the adapter for a tree here too, but that
///   is a different overload and is not claimed.
const COUNTED: &[&str] = &["nanDefault", "infinityInArray", "objectWithAdapter"];

/// Signature-only copies of the gson-2.14.0 members the probe calls. The bodies
/// never run: the classes stay out of the fixture jar.
const STUBS: &[(&str, &str)] = &[
    (
        "com/google/gson/JsonElement.java",
        r#"package com.google.gson;
public abstract class JsonElement {
    public JsonObject getAsJsonObject() { return null; }
    public JsonArray getAsJsonArray() { return null; }
    public boolean getAsBoolean() { return false; }
    public String getAsString() { return null; }
}
"#,
    ),
    (
        "com/google/gson/JsonObject.java",
        r#"package com.google.gson;
public final class JsonObject extends JsonElement {
    public JsonObject() {}
    public void add(String property, JsonElement value) {}
    public void addProperty(String property, String value) {}
    public void addProperty(String property, Number value) {}
    public void addProperty(String property, Boolean value) {}
    public void addProperty(String property, Character value) {}
    public boolean has(String memberName) { return false; }
    public JsonElement get(String memberName) { return null; }
    public JsonArray getAsJsonArray(String memberName) { return null; }
}
"#,
    ),
    (
        "com/google/gson/JsonArray.java",
        r#"package com.google.gson;
public final class JsonArray extends JsonElement {
    public JsonArray() {}
    public void add(Boolean bool) {}
    public void add(Character character) {}
    public void add(Number number) {}
    public void add(String string) {}
    public void add(JsonElement element) {}
    public int size() { return 0; }
}
"#,
    ),
    (
        "com/google/gson/JsonPrimitive.java",
        r#"package com.google.gson;
public final class JsonPrimitive extends JsonElement {
    public JsonPrimitive(Boolean bool) {}
    public JsonPrimitive(Number number) {}
    public JsonPrimitive(String string) {}
    public JsonPrimitive(Character c) {}
}
"#,
    ),
    (
        "com/google/gson/JsonNull.java",
        r#"package com.google.gson;
public final class JsonNull extends JsonElement {
    public static final JsonNull INSTANCE = null;
}
"#,
    ),
    (
        "com/google/gson/JsonParser.java",
        r#"package com.google.gson;
import java.io.Reader;
public final class JsonParser {
    public static JsonElement parseReader(Reader reader) { return null; }
}
"#,
    ),
    (
        "com/google/gson/JsonSerializer.java",
        r#"package com.google.gson;
import java.lang.reflect.Type;
public interface JsonSerializer<T> {
    JsonElement serialize(T src, Type typeOfSrc, JsonSerializationContext context);
}
"#,
    ),
    (
        "com/google/gson/JsonSerializationContext.java",
        "package com.google.gson;\npublic interface JsonSerializationContext {}\n",
    ),
    (
        "com/google/gson/ExclusionStrategy.java",
        r#"package com.google.gson;
public interface ExclusionStrategy {
    boolean shouldSkipField(FieldAttributes f);
    boolean shouldSkipClass(Class<?> clazz);
}
"#,
    ),
    (
        "com/google/gson/FieldAttributes.java",
        "package com.google.gson;\npublic final class FieldAttributes {}\n",
    ),
    (
        "com/google/gson/FieldNamingStrategy.java",
        r#"package com.google.gson;
import java.lang.reflect.Field;
public interface FieldNamingStrategy {
    String translateName(Field f);
}
"#,
    ),
    (
        "com/google/gson/FieldNamingPolicy.java",
        r#"package com.google.gson;
import java.lang.reflect.Field;
public enum FieldNamingPolicy implements FieldNamingStrategy {
    IDENTITY, UPPER_CAMEL_CASE, LOWER_CASE_WITH_UNDERSCORES;
    public String translateName(Field f) { return null; }
}
"#,
    ),
    (
        "com/google/gson/FormattingStyle.java",
        r#"package com.google.gson;
public class FormattingStyle {
    public static final FormattingStyle COMPACT = null;
    public static final FormattingStyle PRETTY = null;
    public FormattingStyle withNewline(String newline) { return null; }
    public FormattingStyle withIndent(String indent) { return null; }
    public FormattingStyle withSpaceAfterSeparators(boolean space) { return null; }
}
"#,
    ),
    (
        "com/google/gson/LongSerializationPolicy.java",
        "package com.google.gson;\npublic enum LongSerializationPolicy { DEFAULT, STRING }\n",
    ),
    (
        "com/google/gson/ToNumberStrategy.java",
        "package com.google.gson;\npublic interface ToNumberStrategy {}\n",
    ),
    (
        "com/google/gson/ToNumberPolicy.java",
        "package com.google.gson;\npublic enum ToNumberPolicy implements ToNumberStrategy { DOUBLE, BIG_DECIMAL }\n",
    ),
    (
        "com/google/gson/TypeAdapter.java",
        r#"package com.google.gson;
import com.google.gson.stream.JsonWriter;
public abstract class TypeAdapter<T> {
    public abstract void write(JsonWriter out, T value) throws java.io.IOException;
}
"#,
    ),
    (
        "com/google/gson/TypeAdapterFactory.java",
        r#"package com.google.gson;
import com.google.gson.reflect.TypeToken;
public interface TypeAdapterFactory {
    <T> TypeAdapter<T> create(Gson gson, TypeToken<T> type);
}
"#,
    ),
    (
        "com/google/gson/reflect/TypeToken.java",
        "package com.google.gson.reflect;\npublic class TypeToken<T> {}\n",
    ),
    (
        "com/google/gson/stream/JsonWriter.java",
        r#"package com.google.gson.stream;
import java.io.Writer;
public class JsonWriter implements java.io.Closeable, java.io.Flushable {
    public JsonWriter(Writer out) {}
    public void flush() throws java.io.IOException {}
    public void close() throws java.io.IOException {}
}
"#,
    ),
    (
        "com/google/gson/Gson.java",
        r#"package com.google.gson;
import com.google.gson.stream.JsonWriter;
public final class Gson {
    public Gson() {}
    public String toJson(Object src) { return null; }
    public String toJson(JsonElement jsonElement) { return null; }
    public void toJson(JsonElement jsonElement, Appendable writer) {}
    public void toJson(JsonElement jsonElement, JsonWriter writer) {}
}
"#,
    ),
    (
        "com/google/gson/GsonBuilder.java",
        r#"package com.google.gson;
import java.lang.reflect.Type;
public final class GsonBuilder {
    public GsonBuilder() {}
    public GsonBuilder setPrettyPrinting() { return this; }
    public GsonBuilder serializeNulls() { return this; }
    public GsonBuilder disableHtmlEscaping() { return this; }
    public GsonBuilder generateNonExecutableJson() { return this; }
    public GsonBuilder serializeSpecialFloatingPointValues() { return this; }
    public GsonBuilder setLongSerializationPolicy(LongSerializationPolicy serializationPolicy) { return this; }
    public GsonBuilder setFormattingStyle(FormattingStyle formattingStyle) { return this; }
    public GsonBuilder setFieldNamingPolicy(FieldNamingPolicy namingConvention) { return this; }
    public GsonBuilder setFieldNamingStrategy(FieldNamingStrategy fieldNamingStrategy) { return this; }
    public GsonBuilder setExclusionStrategies(ExclusionStrategy... strategies) { return this; }
    public GsonBuilder addSerializationExclusionStrategy(ExclusionStrategy strategy) { return this; }
    public GsonBuilder addDeserializationExclusionStrategy(ExclusionStrategy strategy) { return this; }
    public GsonBuilder excludeFieldsWithoutExposeAnnotation() { return this; }
    public GsonBuilder enableComplexMapKeySerialization() { return this; }
    public GsonBuilder setDateFormat(String pattern) { return this; }
    public GsonBuilder setVersion(double version) { return this; }
    public GsonBuilder setObjectToNumberStrategy(ToNumberStrategy objectToNumberStrategy) { return this; }
    public GsonBuilder setNumberToNumberStrategy(ToNumberStrategy numberToNumberStrategy) { return this; }
    public GsonBuilder disableInnerClassSerialization() { return this; }
    public GsonBuilder registerTypeAdapter(Type type, Object typeAdapter) { return this; }
    public GsonBuilder registerTypeHierarchyAdapter(Class<?> baseType, Object typeAdapter) { return this; }
    public GsonBuilder registerTypeAdapterFactory(TypeAdapterFactory factory) { return this; }
    public Gson create() { return null; }
}
"#,
    ),
];

fn fixture() -> Fixture {
    let mut sources: Vec<(&str, &str)> = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    Fixture::build("gson-tojson-element", &sources)
}

/// The oracle rows, `(label, value)`, in file order.
fn oracle() -> Vec<(&'static str, &'static str)> {
    ORACLE
        .lines()
        .map(|line| line.split_once('=').expect("an oracle row is label=value"))
        .collect()
}

/// What `Probe.row` prints for `text`.
fn escaped(text: &str) -> String {
    text.replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[test]
fn every_probe_row_is_java_25_text_or_counted() {
    let fixture = fixture();
    let rows = oracle();
    assert!(rows.len() >= 50, "the oracle lost rows: {}", rows.len());
    let mut failures = Vec::new();
    for (label, expected) in &rows {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let value = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            label,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        // Reading `ToNumberPolicy.DOUBLE` as the argument of `setObjectToNumberStrategy`
        // is its own counted field read (the runtime does not model the constant);
        // it is not the `toJson` and does not change its text.
        let unknown: Vec<_> = host
            .unknown_host_calls()
            .into_iter()
            .filter(|(call, _)| !call.starts_with("com/google/gson/ToNumberPolicy#"))
            .collect();
        if COUNTED.contains(label) {
            if unknown.is_empty() {
                failures.push(format!("{label}: expected a counted call, nothing counted"));
            }
            continue;
        }
        match value {
            Ok(Value::Ref(Some(text))) if escaped(&text) == *expected => {}
            Ok(Value::Ref(Some(text))) => failures.push(format!(
                "{label}\n  java:    {expected:?}\n  runtime: {:?}",
                escaped(&text)
            )),
            other => failures.push(format!("{label}: {other:?}")),
        }
        if !unknown.is_empty() {
            failures.push(format!("{label}: counted {unknown:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The counted rows count exactly the call that Java answers, and the neutral
/// answer is never one of the oracle's texts.
#[test]
fn counted_rows_name_the_gson_call() {
    let fixture = fixture();
    for label in COUNTED {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let _ = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            label,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let unknown = host.unknown_host_calls();
        assert!(
            unknown
                .iter()
                .all(|(call, _)| call.starts_with("com/google/gson/Gson#toJson(")),
            "{label}: {unknown:?}"
        );
    }
}
