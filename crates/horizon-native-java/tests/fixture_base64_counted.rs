//! What the `java.util.Base64` model does not answer stays a counted unknown: never an
//! exception the JDK would not throw, and never bytes or text that are not the JDK's.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

const PROBE: &str = r#"
package demo;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
public class Probe {
  public static Object encode_buffer() { return Base64.getEncoder().encode(ByteBuffer.wrap(new byte[3])); }
  public static Object decode_buffer() { return Base64.getDecoder().decode(ByteBuffer.wrap(new byte[4])); }
  public static Object wrap_output() { return Base64.getEncoder().wrap(new ByteArrayOutputStream()); }
  public static Object wrap_input() { return Base64.getDecoder().wrap(new ByteArrayInputStream(new byte[0])); }
  public static Object decode_into() { return Base64.getDecoder().decode("AAAA".getBytes(StandardCharsets.ISO_8859_1), new byte[3]); }
  public static Object encoder_hash_code() { return Base64.getEncoder().hashCode(); }
  public static Object decoder_hash_code() { return Base64.getDecoder().hashCode(); }
  public static Object encoder_text() { return "" + Base64.getEncoder(); }
  public static Object decoder_text() { return "" + Base64.getMimeDecoder(); }
  public static Object encoder_class() { return Base64.getEncoder().getClass(); }
  public static Object custom_encoder_class() { return Base64.getMimeEncoder(8, new byte[] {'|'}).getClass(); }
  public static Object encoder_is_char_sequence() { Object o = Base64.getEncoder(); return o instanceof CharSequence; }
  public static Object encoder_cast() { Object o = Base64.getEncoder(); return (Base64.Decoder) o; }
  public static Object custom_encode_buffer() { return Base64.getMimeEncoder(8, new byte[] {'|'}).encode(ByteBuffer.wrap(new byte[3])); }
  public static Object modelled() {
    Base64.Encoder e = Base64.getUrlEncoder().withoutPadding();
    byte[] bytes = Base64.getDecoder().decode("SGVsbG8=");
    return e.encodeToString(bytes) + Base64.getMimeEncoder(4, new byte[] {'.'}).encodeToString(bytes)
        + Base64.getMimeDecoder().decode("SGVs\r\nbG8=").length;
  }
  // A mod that builds an encoder per value never runs the heap out (two fields each, past the
  // 65,536 the resident heap keeps).
  public static Object many() {
    int total = 0;
    for (int i = 0; i < 40000; i++) {
      Base64.Encoder e = Base64.getMimeEncoder(4 + (i & 3), new byte[] {'|'});
      total += e.withoutPadding().encodeToString(new byte[] {(byte) i}).length();
    }
    return total;
  }
  // An encoder a static holds keeps its configuration, and its separator array, between calls.
  static final byte[] SEP = {'|'};
  static final Base64.Encoder HELD = Base64.getMimeEncoder(8, SEP).withoutPadding();
  public static Object held() { return HELD.encodeToString(new byte[10]); }
  public static Object held_after_change() { SEP[0] = '!'; return HELD.encodeToString(new byte[10]); }
}
"#;

/// Run `Probe.<name>()` on a fresh host: the answer and the unknown calls it counted.
fn run(name: &str) -> (Result<horizon_native_java::Value, String>, Vec<String>) {
    let fixture = javac_fixture::Fixture::build("base64_counted", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            name,
            "()Ljava/lang/Object;",
            None,
            &[],
            &mut host,
        )
        .map_err(|error| format!("{error:?}"));
    let counted = host
        .unknown_host_calls()
        .into_iter()
        .map(|(call, _)| call)
        .collect();
    (answer, counted)
}

#[test]
fn what_the_model_does_not_cover_is_counted() {
    let cases: &[(&str, &str)] = &[
        (
            "encode_buffer",
            "java/util/Base64$Encoder#encode(Ljava/nio/ByteBuffer;)Ljava/nio/ByteBuffer;",
        ),
        (
            "custom_encode_buffer",
            "java/util/Base64$Encoder#encode(Ljava/nio/ByteBuffer;)Ljava/nio/ByteBuffer;",
        ),
        (
            "decode_buffer",
            "java/util/Base64$Decoder#decode(Ljava/nio/ByteBuffer;)Ljava/nio/ByteBuffer;",
        ),
        (
            "wrap_output",
            "java/util/Base64$Encoder#wrap(Ljava/io/OutputStream;)Ljava/io/OutputStream;",
        ),
        (
            "wrap_input",
            "java/util/Base64$Decoder#wrap(Ljava/io/InputStream;)Ljava/io/InputStream;",
        ),
        ("decode_into", "java/util/Base64$Decoder#decode([B[B)I"),
        ("encoder_hash_code", "java/lang/Object#hashCode()I"),
        ("decoder_hash_code", "java/lang/Object#hashCode()I"),
    ];
    for (name, expected) in cases {
        let (answer, counted) = run(name);
        assert!(answer.is_ok(), "{name}: {answer:?}");
        assert!(
            counted.iter().any(|call| call == expected),
            "{name} counted {counted:?}"
        );
    }
}

#[test]
fn an_objects_text_and_class_are_counted_not_guessed() {
    for name in [
        "encoder_text",
        "decoder_text",
        "encoder_class",
        "custom_encoder_class",
        "encoder_is_char_sequence",
        "encoder_cast",
    ] {
        let (answer, counted) = run(name);
        assert!(answer.is_ok(), "{name}: {answer:?}");
        assert!(
            !counted.is_empty(),
            "{name}: the object's text, class or type must not be invented"
        );
    }
}

#[test]
fn modelled_calls_count_nothing() {
    let (answer, counted) = run("modelled");
    assert_eq!(
        answer,
        Ok(horizon_native_java::Value::Ref(Some(
            "SGVsbG8SGVs.bG8=5".into()
        )))
    );
    assert!(counted.is_empty(), "{counted:?}");
}

#[test]
fn an_encoder_per_value_never_exhausts_the_heap() {
    let (answer, counted) = run("many");
    assert_eq!(answer, Ok(horizon_native_java::Value::Int(80000)));
    assert!(counted.is_empty(), "{counted:?}");
}

#[test]
fn a_held_encoder_keeps_its_separator_array() {
    let (answer, counted) = run("held");
    assert_eq!(
        answer,
        Ok(horizon_native_java::Value::Ref(Some(
            "AAAAAAAA|AAAAAA".into()
        )))
    );
    assert!(counted.is_empty(), "{counted:?}");
    let (answer, counted) = run("held_after_change");
    assert_eq!(
        answer,
        Ok(horizon_native_java::Value::Ref(Some(
            "AAAAAAAA!AAAAAA".into()
        )))
    );
    assert!(counted.is_empty(), "{counted:?}");
}
