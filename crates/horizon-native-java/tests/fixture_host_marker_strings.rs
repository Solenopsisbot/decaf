//! Java 25 oracle fixture: a jar `String` prints as itself whatever it contains (#330).
//!
//! `fixtures/host_marker_strings/demo/Probe.java` is one source run two ways: on Java 25
//! (`java25.txt`, command in the README) and here. Its Strings are spelled like the runtime's
//! own callback and collection keys (`abc/hcb:x`, `list/hobj:x`), and each goes through every
//! print path: `String.valueOf`, concatenation, `StringBuilder.append`, and the `toString` of a
//! list, a map and an `Optional` holding it. `strings()` must equal the capture byte for byte.
//!
//! Three `toUpperCase` calls on a text spelling a runtime key stay counted (`CASE_COUNTED_ROWS`).
//!
//! Four calls stay counted, with the right text: `Optional.toString` of the four Strings
//! shaped `word/rest`. A text of that shape may still be one of the runtime's remaining text
//! markers (`value_class::is_plain_string`), so the optional can't be sure it holds a `String`.
//! That goes away when those markers become handles (#156 P17), not here.
//!
//! `collections()` prints collections that hold a lambda. Java prints the lambda's identity,
//! which differs every run, so those four `toString` calls stay counted, and the text that
//! stands in must not show a runtime key.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/host_marker_strings/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/host_marker_strings/java25.txt");

/// The rows whose `x.toUpperCase()` is counted: a text that spells one of the runtime's own
/// keys (`foo/hopt:hcb:@indy:x`, `list/hobj:x`, `abc/hcb:literal`) may be a marker, not text, so
/// the case mapping is never answered (the row prints `null` where the oracle has the text).
const CASE_COUNTED_ROWS: [&str; 3] = ["methods2", "methods3", "methods5"];

#[test]
fn marker_spelled_strings_print_as_java25() {
    let fixture = Fixture::build("host-marker-strings", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "strings");
    // Both at once, so a failure shows which calls were counted beside the wrong rows.
    let unknown = format!("{:?}", host.unknown_host_calls());
    let expected = r#"[("java/lang/Object#toString()Ljava/lang/String;", 4), ("java/lang/String#toUpperCase()Ljava/lang/String;", 3)]"#;
    // Every row is the oracle byte for byte, except the counted case mappings: their middle
    // field (`length|toUpperCase|substring`) is not answered.
    let oracle: String = ORACLE
        .lines()
        .map(|line| {
            let (label, value) = line.split_once('=').expect("oracle row");
            if !CASE_COUNTED_ROWS.contains(&label) {
                return format!("{line}\n");
            }
            let mut fields = value.split('|');
            let length = fields.next().expect("length");
            let _upper = fields.next().expect("toUpperCase");
            let head = fields.next().expect("substring");
            format!("{label}={length}|null|{head}\n")
        })
        .collect();
    assert_eq!(
        (rows.as_str(), unknown.as_str()),
        (oracle.as_str(), expected)
    );
}

#[test]
fn collections_holding_a_lambda_count_their_text_and_leak_no_key() {
    let fixture = Fixture::build("host-marker-lambdas", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let printed = fixture.text_on(&mut host, "collections");
    assert_eq!(
        host.unknown_host_calls(),
        vec![("java/lang/Object#toString()Ljava/lang/String;".into(), 4)]
    );
    for key in ["hcb:", "hobj:", "@indy"] {
        assert!(!printed.contains(key), "{key} leaked into {printed:?}");
    }
}
