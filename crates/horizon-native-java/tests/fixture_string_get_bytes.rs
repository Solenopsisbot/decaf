#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const PROBE: &str = include_str!("fixtures/string_get_bytes/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/string_get_bytes/java25.txt");
const NAME: &str = include_str!("fixtures/string_get_bytes/name.txt");

/// Rows whose string holds a lone surrogate (`u*`) or a genuine U+FFFD (`g8`). Decaf keeps strings
/// as UTF-8, so both reach `getBytes` as U+FFFD and it can't tell them apart (#447): each call is
/// counted, and the probe prints `<name>=null`. Java's answers are the capture's lines; these
/// rows become exact when strings keep their surrogates.
const COUNTED_ROWS: &[&str] = &["u8", "ua", "ul", "ube", "ule", "g8"];

#[test]
fn string_get_bytes_rows_match_java25() {
    let fixture = Fixture::build("string_get_bytes", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let got = fixture.text_on(&mut host, "rows");
    let got: Vec<&str> = got.lines().collect();
    let java: Vec<&str> = ORACLE.lines().collect();
    assert_eq!(got.len(), java.len(), "{got:?}");
    for (decaf, java) in got.iter().zip(&java) {
        let name = java.split('=').next().unwrap();
        if COUNTED_ROWS.contains(&name) {
            // Counted: no value came back (Java's answer here: `java`).
            assert_eq!(*decaf, format!("{name}=null"), "Java: {java}");
        } else {
            assert_eq!(decaf, java);
        }
    }
    // Each counted `getBytes` result then reaches the probe's `Arrays.toString`, which is counted
    // too: its argument is an unknown, not an array. The probe's `"a\uD800b"` constant is a
    // `String` constant with a lone surrogate, so loading it (once, into the local `u`) is counted
    // as `String#ldc` rather than read as `"a�b"` (#447).
    assert_eq!(
        host.unknown_host_calls(),
        vec![
            (
                "java/lang/String#getBytes(Ljava/nio/charset/Charset;)[B".into(),
                COUNTED_ROWS.len()
            ),
            ("java/lang/String#ldc()Ljava/lang/String;".into(), 1),
            (
                "java/util/Arrays#toString([B)Ljava/lang/String;".into(),
                COUNTED_ROWS.len()
            ),
        ]
    );

    // `getBytes(String charsetName)` is not modelled: both calls are counted and answer nothing
    // (Java's answers are `name.txt`: the bytes, then `UnsupportedEncodingException: NOPE`).
    let mut host = fixture.jar.seed_host(StdHost::new());
    let got = fixture.text_on(&mut host, "nameOverload");
    assert_eq!(NAME.lines().count(), 2);
    assert_eq!(got, "name=null\nbad=null");
    assert_eq!(
        host.unknown_host_calls(),
        vec![
            ("java/lang/String#getBytes(Ljava/lang/String;)[B".into(), 2),
            ("java/util/Arrays#toString([B)Ljava/lang/String;".into(), 2),
        ]
    );
}
