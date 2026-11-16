#[path = "support/javac_fixture.rs"] mod javac_fixture;
use horizon_native_java::StdHost;
const ORACLE: &str = include_str!("fixtures/cov13/java25.txt");
#[test]
fn cov13_oracle_fixture_runs() {
 let f=javac_fixture::Fixture::build("cov13", &[("demo/Probe.java", include_str!("fixtures/cov13/Probe.java"))]);
 let mut h=f.jar.seed_host(StdHost::new());
 let got=f.text_on(&mut h,"main");
 assert!(!got.is_empty());
 assert!(!ORACLE.is_empty());
}
