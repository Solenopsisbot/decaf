#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"
package demo;
public final class Probe {
  static String counted() { return new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16); }
  public static String stringWriter() { java.io.StringWriter w = new java.io.StringWriter(); w.append(counted()); return w.toString(); }
  public static String printWriter() { java.io.StringWriter w = new java.io.StringWriter(); new java.io.PrintWriter(w).print(counted()); return w.toString(); }
  public static String printStream() { java.io.ByteArrayOutputStream b = new java.io.ByteArrayOutputStream(); new java.io.PrintStream(b).print(counted()); return b.toString(); }
  public static String joiner() { java.util.StringJoiner j = new java.util.StringJoiner(","); j.add(counted()); return j.toString(); }
}
"#;

fn assert_counted(method: &str, read: &str) {
    let fixture = Fixture::build("review-601", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let got = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .expect("probe runs");
    assert!(
        !matches!(&got, horizon_native_java::Value::Ref(Some(text)) if text.contains("java/lang/String#")),
        "{method} leaked marker: {got:?}"
    );
    assert!(
        host.unknown_host_calls()
            .iter()
            .any(|(call, _)| call.starts_with(read)),
        "{method}: {:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn string_writer_counted_append_then_to_string() {
    assert_counted("stringWriter", "java/io/StringWriter#toString");
}
#[test]
fn print_writer_counted_print_then_underlying_to_string() {
    assert_counted("printWriter", "java/io/StringWriter#toString");
}
#[test]
fn print_stream_counted_print_then_byte_array_to_string() {
    assert_counted("printStream", "java/io/ByteArrayOutputStream#toString");
}
#[test]
fn string_joiner_counted_add_then_to_string() {
    assert_counted("joiner", "java/util/StringJoiner#toString");
}
