//! Gson TypeAdapter dispatch parity fixture, compiled with Java 25's javac.
//! Expected writer text and exception messages were captured from the real
//! gson-2.14.0 jar on Java 25.

use horizon_native_java::{NativeError, NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const GSON: &str = r#"package com.google.gson;
public class Gson {
  public Gson() {}
  public String toJson(Object src) { return null; }
  public void toJson(Object src, Appendable out) {}
  public void toJson(JsonElement src, Appendable out) {}
  public void toJson(JsonElement src, com.google.gson.stream.JsonWriter out) {}
  public void toJson(Object src, com.google.gson.stream.JsonWriter out) {}
  public com.google.gson.stream.JsonWriter newJsonWriter(java.io.Writer out) { return null; }
}"#;
const BUILDER: &str = r#"package com.google.gson;
public class GsonBuilder {
  public GsonBuilder() {}
  public GsonBuilder setPrettyPrinting() { return this; }
  public GsonBuilder serializeNulls() { return this; }
  public GsonBuilder disableHtmlEscaping() { return this; }
  public Gson create() { return new Gson(); }
}"#;
const ELEMENT: &str = "package com.google.gson; public class JsonElement {}\n";
const OBJECT: &str = r#"package com.google.gson;
public final class JsonObject extends JsonElement {
  public JsonObject() {}
  public void addProperty(String key, String value) {}
  public void addProperty(String key, Number value) {}
  public void addProperty(String key, Boolean value) {}
  public void addProperty(String key, Character value) {}
}"#;
const STRICTNESS: &str = r#"package com.google.gson;
public enum Strictness { LEGACY_STRICT, STRICT, LENIENT }
"#;
const FORMATTING: &str = r#"package com.google.gson;
public final class FormattingStyle {
  public static final FormattingStyle COMPACT = new FormattingStyle("", "", false);
  public static final FormattingStyle PRETTY = new FormattingStyle("\n", "  ", true);
  private final String newline, indent; private final boolean spaces;
  private FormattingStyle(String n, String i, boolean s) { newline=n; indent=i; spaces=s; }
  public FormattingStyle withIndent(String i) { return this; }
  public FormattingStyle withNewline(String n) { return this; }
  public FormattingStyle withSpaceAfterSeparators(boolean s) { return this; }
  public String getIndent() { return indent; }
  public boolean usesSpaceAfterSeparators() { return spaces; }
}"#;
const WRITER: &str = r#"package com.google.gson.stream;
import com.google.gson.*;
public class JsonWriter {
  public JsonWriter(java.io.Writer out) {}
  public JsonWriter beginObject() { return this; }
  public JsonWriter endObject() { return this; }
  public JsonWriter beginArray() { return this; }
  public JsonWriter endArray() { return this; }
  public JsonWriter name(String name) { return this; }
  public JsonWriter value(String value) { return this; }
  public JsonWriter value(boolean value) { return this; }
  public JsonWriter value(Boolean value) { return this; }
  public JsonWriter value(double value) { return this; }
  public JsonWriter value(float value) { return this; }
  public JsonWriter value(long value) { return this; }
  public JsonWriter value(Number value) { return this; }
  public JsonWriter nullValue() { return this; }
  public JsonWriter jsonValue(String value) { return this; }
  public void setIndent(String indent) {}
  public void setLenient(boolean lenient) {}
  public void setStrictness(Strictness strictness) {}
  public Strictness getStrictness() { return null; }
  public boolean isLenient() { return false; }
  public void setSerializeNulls(boolean value) {}
  public boolean getSerializeNulls() { return false; }
  public void setHtmlSafe(boolean value) {}
  public boolean isHtmlSafe() { return false; }
  public void setFormattingStyle(FormattingStyle style) {}
  public FormattingStyle getFormattingStyle() { return null; }
  public void flush() {}
  public void close() throws java.io.IOException {}
}
"#;
const PROBE: &str = r#"package demo;
import com.google.gson.*; import com.google.gson.stream.JsonWriter; import java.io.*;
public final class Probe {
  static String special() { return "<&>='" + (char)0x2028 + (char)0x2029; }
  static String q(String s) { return s.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t"); }
  public static String writers() {
    Gson gson = new GsonBuilder().setPrettyPrinting().serializeNulls().disableHtmlEscaping().create();
    JsonObject o = new JsonObject();
    o.addProperty("text", "<&>"); o.addProperty("bool", Boolean.TRUE);
    o.addProperty("num", Double.valueOf(2.5)); o.addProperty("char", Character.valueOf((char)120));
    o.addProperty("nil", (String) null);
    StringBuilder b = new StringBuilder(); gson.toJson(o, b);
    StringWriter s = new StringWriter(); gson.toJson(o, s);
    String compact = new Gson().toJson(o);
    String omitted = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(o);
    return b.toString().replace("\n", "\\n") + "|S=" + s.toString().replace("\n", "\\n")
      + "|C=" + compact + "|O=" + omitted.replace("\n", "\\n");
  }
  public static String writerExact() throws Exception {
    StringWriter out = new StringWriter(); JsonWriter w = new JsonWriter(out);
    w.setIndent("  "); w.setHtmlSafe(true); w.setSerializeNulls(false);
    w.beginObject().name("text").value(special()).name("omit").nullValue()
      .name("neg").value(-0.0).name("raw").jsonValue("[1,2]").endObject(); w.flush();
    return q(out.toString());
  }
  public static String writerMisuse() {
    StringBuilder out = new StringBuilder();
    try { new JsonWriter(new StringWriter()).name("x"); } catch (Exception e) { out.append("name=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    try { new JsonWriter(new StringWriter()).beginObject().name("x").name("y"); } catch (Exception e) { out.append("|double=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    try { new JsonWriter(new StringWriter()).endObject(); } catch (Exception e) { out.append("|end=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    try { new JsonWriter(new StringWriter()).beginObject().name("x").endObject(); } catch (Exception e) { out.append("|dangling=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    try { JsonWriter w = new JsonWriter(new StringWriter()); w.value("x"); w.close(); w.flush(); } catch (Exception e) { out.append("|flushClosed=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    try { JsonWriter w = new JsonWriter(new StringWriter()); w.beginArray(); w.close(); } catch (Exception e) { out.append("|incomplete=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    try { new JsonWriter(new StringWriter()).value(Double.NaN); } catch (Exception e) { out.append("|nan=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    try { new JsonWriter(new StringWriter()).value(Double.POSITIVE_INFINITY); } catch (Exception e) { out.append("|inf=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    StringWriter lenientOut = new StringWriter(); JsonWriter lenient = new JsonWriter(lenientOut); lenient.setLenient(true); lenient.value(Double.NaN).value(Double.POSITIVE_INFINITY);
    return out + "|lenient=" + lenientOut.toString();
  }
  public static String writerReviewCases() {
    StringBuilder out = new StringBuilder();
    try { new JsonWriter(new StringWriter()).setIndent("x"); } catch (Exception e) { out.append("indent=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    JsonWriter w = new JsonWriter(new StringWriter()); w.beginObject().name("x");
    try { w.value(Double.NaN); } catch (Exception e) { out.append("|first=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    try { w.name("y"); } catch (Exception e) { out.append("|after=").append(e.getClass().getName()).append(':').append(e.getMessage()); }
    return out.toString();
  }
  public static String writerValues() throws Exception {
    StringWriter out = new StringWriter(); JsonWriter w = new JsonWriter(out);
    w.beginArray().value(true).value(Boolean.FALSE).value(1.25f).value(2.5).value(3L).value(Integer.valueOf(4)).nullValue().endArray(); w.close();
    return out.toString();
  }
  public static String newWriter() throws Exception {
    Gson gson = new GsonBuilder().setPrettyPrinting().serializeNulls().disableHtmlEscaping().create(); JsonObject o = new JsonObject();
    o.addProperty("s", "<&>"); o.addProperty("n", (String)null); StringWriter out = new StringWriter();
    JsonWriter w = gson.newJsonWriter(out); gson.toJson(o, w); w.close(); return q(out.toString());
  }
  public static String writerOptions() {
    JsonWriter w = new JsonWriter(new StringWriter()); FormattingStyle style = FormattingStyle.PRETTY;
    if (style == null) return "NULLSTYLE";
    w.setFormattingStyle(style); w.setStrictness(Strictness.LENIENT); w.setSerializeNulls(false); w.setHtmlSafe(true);
    return w.getFormattingStyle().getIndent().replace("\t", "T") + "|" + w.getFormattingStyle().usesSpaceAfterSeparators()
      + "|" + w.getStrictness() + "|" + w.isLenient() + "|" + w.getSerializeNulls() + "|" + w.isHtmlSafe();
  }
  public static String gsonWriter() {
    Gson gson = new GsonBuilder().setPrettyPrinting().serializeNulls().create(); JsonObject o = new JsonObject();
    o.addProperty("s", special()); o.addProperty("b", Boolean.FALSE); o.addProperty("n", Double.valueOf(-0.0));
    o.addProperty("c", Character.valueOf('x')); o.addProperty("null", (String)null);
    StringWriter out = new StringWriter(); JsonWriter w = new JsonWriter(out); w.setIndent("\t"); w.setHtmlSafe(false); w.setSerializeNulls(true); gson.toJson(o, w);
    return q(out.toString()) + "|after=" + w.isHtmlSafe() + "," + w.getSerializeNulls();
  }
  static final class NestedConfig {
    String label; int amount;
    NestedConfig(String label, int amount) { this.label = label; this.amount = amount; }
  }
  static final class Config {
    String name; int count; boolean enabled; double ratio; String nullable;
    java.util.List<String> tags; java.util.Map<String,Integer> values; NestedConfig nested;
    Config() {
      name = "<cfg>"; count = -9; enabled = true; ratio = -98.0; nullable = null;
      tags = new java.util.ArrayList<>(); tags.add("<a>"); tags.add("b");
      values = new java.util.LinkedHashMap<>(); values.put("x", 96); values.put("y", 17);
      nested = new NestedConfig("n0", 47);
    }
  }
  public static String objectToJson() {
    Config value = new Config();
    Gson compact = new Gson();
    Gson pretty = new GsonBuilder().setPrettyPrinting().create();
    Gson nulls = new GsonBuilder().setPrettyPrinting().serializeNulls().create();
    Gson safe = new GsonBuilder().setPrettyPrinting().serializeNulls().disableHtmlEscaping().create();
    return compact.toJson(value) + "|P=" + pretty.toJson(value).replace("\n", "\\n")
      + "|N=" + nulls.toJson(value).replace("\n", "\\n")
      + "|H=" + safe.toJson(value).replace("\n", "\\n");
  }

  static final class FakeWriter extends Writer {
    public void write(char[] value, int off, int len) {}
    public void flush() {}
    public void close() {}
  }
  static final class FakeJsonWriter extends JsonWriter {
    FakeJsonWriter() { super(new StringWriter()); }
  }
  public static String sinkKinds() throws Exception {
    Gson gson = new Gson(); JsonObject o = new JsonObject();
    StringBuilder builder = new StringBuilder(); gson.toJson(o, builder);
    StringWriter writer = new StringWriter(); gson.toJson(o, writer);
    gson.toJson(o, new FakeWriter());
    gson.toJson(o, new FakeJsonWriter());
    return builder.toString() + "|" + writer.toString();
  }
  public static String addProperties() {
    Gson gson = new GsonBuilder().setPrettyPrinting().serializeNulls().create(); JsonObject x = new JsonObject();
    x.addProperty("str", "s"); x.addProperty("bool", Boolean.TRUE); x.addProperty("num", Double.valueOf(2.5)); x.addProperty("char", Character.valueOf('x'));
    x.addProperty("ns", (String)null); x.addProperty("nb", (Boolean)null); x.addProperty("nn", (Double)null); x.addProperty("nc", (Character)null);
    return gson.toJson(x);
  }
  public static String nullKey() { try { new JsonObject().addProperty((String)null, "x"); return "no"; } catch (NullPointerException e) { return e.getMessage(); } }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|h| PathBuf::from(h).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}
fn files(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|e| {
            let p = e.unwrap().path();
            if p.is_dir() { files(&p) } else { vec![p] }
        })
        .collect()
}
struct Fixture {
    root: PathBuf,
    jar: NativeJar,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn new() -> Self {
        // The counter keeps two parallel tests in this process apart: the
        // clock alone repeats on a coarse timer (macOS), and then one test's
        // `Drop` deleted the jar the other was reading (#255 round 4, S3).
        static BUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "horizon-gson-writer-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let classes = root.join("classes");
        let srcs = [
            ("com/google/gson/Gson.java", GSON),
            ("com/google/gson/GsonBuilder.java", BUILDER),
            ("com/google/gson/JsonElement.java", ELEMENT),
            ("com/google/gson/JsonObject.java", OBJECT),
            ("com/google/gson/Strictness.java", STRICTNESS),
            ("com/google/gson/FormattingStyle.java", FORMATTING),
            ("com/google/gson/stream/JsonWriter.java", WRITER),
            ("demo/Probe.java", PROBE),
        ];
        let mut cmd = Command::new(javac());
        cmd.args(["--release", "25", "-d"]).arg(&classes);
        for (name, text) in srcs {
            let p = root.join("src").join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, text).unwrap();
            cmd.arg(p);
        }
        assert!(cmd.status().unwrap().success());
        let jar_path = root.join("fixture.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"gson-writer-fixture","version":"1","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for p in files(&classes) {
            let n = p
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(n, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(p).unwrap()).unwrap();
        }
        zip.finish().unwrap();
        Self {
            root,
            jar: NativeJar::from_path(&jar_path).unwrap(),
        }
    }
    fn call_with_host(&self, name: &str, host: &mut StdHost) -> Result<Value, NativeError> {
        self.jar.invoke_method_with_args(
            "demo/Probe",
            name,
            "()Ljava/lang/String;",
            None,
            &[],
            host,
        )
    }
    fn text(&self, name: &str) -> String {
        match self.call_with_host(name, &mut StdHost::new()).unwrap() {
            Value::Ref(Some(s)) => s,
            v => panic!("{name}: {v:?}"),
        }
    }
}

#[test]
fn writers_options_and_tree_properties_match_real_gson() {
    let f = Fixture::new();
    assert_eq!(
        f.text("writers"),
        r#"{\n  "text": "<&>",\n  "bool": true,\n  "num": 2.5,\n  "char": "x",\n  "nil": null\n}|S={\n  "text": "<&>",\n  "bool": true,\n  "num": 2.5,\n  "char": "x",\n  "nil": null\n}|C={"text":"\u003c\u0026\u003e","bool":true,"num":2.5,"char":"x"}|O={\n  "text": "<&>",\n  "bool": true,\n  "num": 2.5,\n  "char": "x"\n}"#
    );
}

#[test]
fn exact_writer_output_and_options_match_real_gson() {
    let f = Fixture::new();
    assert_eq!(
        f.text("writerExact"),
        r#"{\n  "text": "\\u003c\\u0026\\u003e\\u003d\\u0027\\u2028\\u2029",\n  "neg": -0.0,\n  "raw": [1,2]\n}"#
    );
    assert_eq!(f.text("writerOptions"), "  |true|LENIENT|true|false|true");
    assert_eq!(f.text("writerValues"), "[true,false,1.25,2.5,3,4,null]");
    assert_eq!(f.text("newWriter"), r#"{\n  "s": "<&>",\n  "n": null\n}"#);
    assert_eq!(
        f.text("gsonWriter"),
        r#"{\n\t"s": "\\u003c\\u0026\\u003e\\u003d\\u0027\\u2028\\u2029",\n\t"b": false,\n\t"n": -0.0,\n\t"c": "x",\n\t"null": null\n}|after=false,true"#
    );
}

#[test]
fn review_writer_order_and_indent_validation_match_gson_214() {
    assert_eq!(
        Fixture::new().text("writerReviewCases"),
        "indent=java.lang.IllegalArgumentException:Only combinations of spaces and tabs are allowed in indent.|first=java.lang.IllegalArgumentException:Numeric values must be finite, but was NaN|after=java.lang.IllegalStateException:Please begin an object before writing a name."
    );
}

#[test]
fn exact_writer_state_and_nonfinite_messages_match_real_gson() {
    let f = Fixture::new();
    assert_eq!(
        f.text("writerMisuse"),
        "name=java.lang.IllegalStateException:Please begin an object before writing a name.|double=java.lang.IllegalStateException:Already wrote a name, expecting a value.|end=java.lang.IllegalStateException:Nesting problem.|dangling=java.lang.IllegalStateException:Dangling name: x|flushClosed=java.lang.IllegalStateException:JsonWriter is closed.|incomplete=java.io.IOException:Incomplete document|nan=java.lang.IllegalArgumentException:Numeric values must be finite, but was NaN|inf=java.lang.IllegalArgumentException:Numeric values must be finite, but was Infinity|lenient=NaNInfinity"
    );
}

#[test]
fn json_object_add_property_overloads_and_nulls_match_real_gson() {
    let f = Fixture::new();
    assert_eq!(
        f.text("addProperties"),
        "{\n  \"str\": \"s\",\n  \"bool\": true,\n  \"num\": 2.5,\n  \"char\": \"x\",\n  \"ns\": null,\n  \"nb\": null,\n  \"nn\": null,\n  \"nc\": null\n}"
    );
    assert_eq!(f.text("nullKey"), "key == null");
}

#[test]
fn only_canonical_writer_and_appendable_handles_are_modelled() {
    let f = Fixture::new();
    let mut host = StdHost::new();
    assert_eq!(
        f.call_with_host("sinkKinds", &mut host).unwrap(),
        Value::Ref(Some("{}|{}".into()))
    );
    let calls = host.unknown_host_calls();
    assert!(
        calls
            .iter()
            .any(|(call, _)| call.starts_with("com/google/gson/Gson#toJson")
                && call.contains("Appendable"))
    );
    assert!(calls.iter().any(|(call, _)| call.contains("JsonWriter")));
}

#[test]
fn unsupported_appendable_is_counted() {
    let f = Fixture::new();
    let mut host = StdHost::new();
    let _ = f.call_with_host("writers", &mut host);
    assert!(
        host.unknown_host_calls()
            .iter()
            .all(|(call, _)| !call.contains("JsonWriter#setIndent"))
    );
}

#[test]
fn object_to_json_matches_captured_java25_gson_oracle_byte_for_byte() {
    let f = Fixture::new();
    let actual = format!("{}\n", f.text("objectToJson"));
    assert_eq!(
        actual.as_bytes(),
        include_bytes!("fixtures/gson_tojson_object/oracle/java25.txt")
    );
}

#[test]
fn reads_java25_capture() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gson_type_adapter/java25.txt");
    let text = fs::read_to_string(path).expect("java25 capture");
    let oracle = include_str!("fixtures/gson_type_adapter/oracle/java25.txt");
    assert_eq!(text, oracle);
    assert!(text.contains("COUNTED"));
}
