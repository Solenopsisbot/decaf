//! `new String()`, `new String(String)` and the `byte[]` constructors
//! (`string_constructors.rs`).
//!
//! `fixtures/string_bytes/Probe.java` printed the string below on JDK 25
//! (OpenJDK 25, `java -cp out demo.Probe`): UTF-8 by default, ISO-8859-1
//! and US-ASCII by `Charset` or by name, the range check, and the null checks
//! in JDK 25's order (the charset before the array).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/string_bytes/Probe.java");

#[test]
fn byte_constructors_decode_like_jdk_25() {
    let fixture = Fixture::build("string-bytes", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "caf+233|af+233|c+233|c+65533|caf|c+233|\
         StringIndexOutOfBoundsException:Range [3, 3 + 5) out of bounds for length 5|\
         NullPointerException:Cannot read the array length because \"bytes\" is null|\
         NullPointerException:Cannot read the array length because \"bytes\" is null|\
         NullPointerException:null|NullPointerException:null|NullPointerException:null"
    );
    assert_eq!(
        fixture.text_on(&mut host, "malformed"),
        "+65533|+65533+65533+65533|+65533A|+65533+65533(+65533|+65533+65533+65533+65533"
    );
    assert_eq!(
        fixture.text_on(&mut host, "unknownName"),
        "UnsupportedEncodingException:default"
    );
    assert_eq!(
        fixture.text_on(&mut host, "differential"),
        "220000|8940964167532627793"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

const COUNTED: &str = r#"package demo;

public final class Probe {
    public static String run() throws Exception {
        byte[] bytes = {104, 105};
        String out = new String(bytes, "UTF-16");
        try {
            out += new String(bytes, "default");
        } catch (java.io.UnsupportedEncodingException e) {
            out += "!";
        }
        return out;
    }
}
"#;

/// A known charset the host does not decode exactly (UTF-16) stays counted.
#[test]
fn other_charsets_stay_counted() {
    let fixture = Fixture::build("string-bytes-counted", &[("demo/Probe.java", COUNTED)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    // `out` comes from a counted constructor, so `out += ...` is a counted
    // concatenation too (#423): `run` answers the counted placeholder, not text.
    fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "run",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .expect("run executes");
    let counted = host.unknown_host_calls();
    assert!(
        counted.iter().any(|(call, count)| call
            == "java/lang/String#<init>([BLjava/lang/String;)V"
            && *count == 1),
        "{counted:?}"
    );
    assert!(
        counted
            .iter()
            .any(|(call, _)| call.starts_with("java/lang/invoke/StringConcatFactory#")),
        "{counted:?}"
    );
}
