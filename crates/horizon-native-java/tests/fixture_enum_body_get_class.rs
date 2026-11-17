//! Java 25 oracle fixture for `getClass()` on enum constants with a body (`enum E { A {}, C }`:
//! `A.getClass()` is the anonymous subclass `E$1`, not `E`) and the class surface around it.
//!
//! `fixtures/enum_body_get_class/demo/Probe.java` is one plain-JDK source run two ways: on
//! Java 25 (`java25.txt`, command in the README) and here through the interpreter. `rows()`
//! must equal the capture byte for byte, and nothing it calls may be counted.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/enum_body_get_class/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/enum_body_get_class/java25.txt");

#[test]
fn enum_body_get_class_rows_match_java25() {
    let fixture = Fixture::build("enum-body-get-class", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    let unknown = host.unknown_host_calls();
    assert_eq!(rows, ORACLE, "unknown calls: {unknown:?}");
    assert!(unknown.is_empty(), "{unknown:?}");
}
