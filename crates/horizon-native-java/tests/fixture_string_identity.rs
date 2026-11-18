//! `new String(...)` keeps ordinary text exact while identity-sensitive uses count ambiguity (#544).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/string_identity_c/demo/Probe.java");
const JAVA25: &str = include_str!("fixtures/string_identity_c/java25.txt");

fn run(method: &str) -> (String, Vec<(String, usize)>) {
    let fixture = Fixture::build(method, &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, method);
    (text, host.unknown_host_calls())
}

fn expected(method: &str) -> &'static str {
    JAVA25
        .lines()
        .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no Java 25 row for {method}"))
}

#[test]
fn normal_string_text_is_exact() {
    let (text, counted) = run("normal");
    assert_eq!(text, expected("normal"));
    assert!(counted.is_empty(), "normal: {counted:?}");
}

#[test]
fn direct_identity_is_counted() {
    let (_text, counted) = run("directIdentity");
    assert_eq!(
        counted,
        vec![(
            "java/lang/String#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z".into(),
            1,
        )]
    );
}

#[test]
fn identity_map_equal_text_is_counted() {
    let (_text, counted) = run("identityMap");
    assert_eq!(
        counted,
        vec![(
            "java/util/Map#get(Ljava/lang/Object;)Ljava/lang/Object;".into(),
            1,
        )]
    );
}

#[test]
fn literal_identity_lookup_is_counted() {
    let (_text, counted) = run("literalLookup");
    assert_eq!(
        counted,
        vec![(
            "java/util/Map#get(Ljava/lang/Object;)Ljava/lang/Object;".into(),
            1,
        )]
    );
}
