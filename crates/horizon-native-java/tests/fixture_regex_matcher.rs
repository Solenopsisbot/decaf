#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const PROBE: &str = include_str!("fixtures/regex_matcher/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/regex_matcher/java25.txt");
#[test]
fn regex_matcher_rows_match_java25() {
    let fixture = Fixture::build("regex-matcher", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows"), ORACLE);
}
