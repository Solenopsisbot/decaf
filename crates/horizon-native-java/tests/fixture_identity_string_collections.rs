//! Java 25 identity-map behavior for ordinary `new String` copies and literals.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/identity_string_collections/Probe.java");
const ORACLE: &str = include_str!("fixtures/identity_string_collections/java25.txt");

#[test]
fn identity_map_distinguishes_literals_and_string_copies() {
    let fixture = Fixture::build("identity-string-collections", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = [
        "rows",
        "sameReference",
        "callbackMutation",
        "stringValueOfIdentity",
        "entrySetValue",
        "entryToArraySetValue",
        "unmodEntryToArrayFn",
        "unmodSyncEntryToArrayFn",
    ]
    .iter()
    .map(|method| fixture.text_on(&mut host, method))
    .collect::<Vec<_>>();
    assert_eq!(rows.join("\n"), ORACLE.trim_end());
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "exact rows should not be counted: {:?}",
        host.unknown_host_calls()
    );
}
