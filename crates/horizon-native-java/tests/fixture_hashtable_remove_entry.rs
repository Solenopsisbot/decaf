#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
const PROBE: &str = include_str!("fixtures/hashtable_remove_entry/demo/Probe.java");
const JAVA25: &str = include_str!("fixtures/hashtable_remove_entry/java25.txt");
#[test]
fn hashtable_remove_entry_matches_java25() {
 let f=javac_fixture::Fixture::build("hashtable-remove-entry", &[("demo/Probe.java", PROBE)]);
 let mut h=f.jar.seed_host(StdHost::new());
 assert_eq!(f.text_on(&mut h,"rows"), JAVA25.trim_end());
 assert!(h.unknown_host_calls().is_empty(), "{:?}", h.unknown_host_calls());
}
