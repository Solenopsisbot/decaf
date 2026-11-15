//! Map entries and entry views held across collections (#221 on #226's collector).
//!
//! A held `HashMap` node and its map's `entrySet()` view, a detached node, a list of nodes, a
//! `TreeMap` node, an `IdentityHashMap` entry and a `singletonMap` entry stay the very objects
//! they were, reading what Java reads, through three rounds of calls that each make well past a
//! collection's worth of maps and walked entries. The entries the garbage made are freed.
//!
//! `fixtures/heap_map_entries/demo/HeapEntries.java` runs unchanged on both sides; its `main` on
//! Java 25 printed `java25.txt`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

/// `java -cp . demo.HeapEntries` on Java 25 (Temurin 25.0.5, `javac --release 21`), the same on
/// two runs.
const ORACLE: &str = include_str!("fixtures/heap_map_entries/java25.txt");

fn expected(name: &str) -> &'static str {
    ORACLE
        .lines()
        .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no oracle line for {name}"))
}

fn call(fixture: &javac_fixture::Fixture, host: &mut StdHost, method: &str) -> String {
    match fixture.jar.invoke_method_with_args(
        "demo/HeapEntries",
        method,
        "()Ljava/lang/String;",
        None,
        &[],
        host,
    ) {
        Ok(Value::Ref(Some(text))) => text,
        other => format!("{other:?}"),
    }
}

#[test]
fn held_entries_and_views_survive_collections() {
    let fixture = javac_fixture::Fixture::build(
        "heap-map-entries",
        &[(
            "demo/HeapEntries.java",
            include_str!("fixtures/heap_map_entries/demo/HeapEntries.java"),
        )],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(call(&fixture, &mut host, "setup"), expected("setup"));
    assert_eq!(call(&fixture, &mut host, "check"), expected("check"));
    let mark = host.call_log_mark();
    let mut freed = 0;
    for round in 1..=3 {
        for _ in 0..40 {
            call(&fixture, &mut host, "garbage");
            host.rewind_call_log(mark);
        }
        freed = freed.max(host.freed_object_ranges());
        assert_eq!(
            call(&fixture, &mut host, "check"),
            expected("check"),
            "round {round}"
        );
    }
    // The garbage was collected: 72,000 entries were walked, and the host keeps about the
    // live ones (the held ones and the last pass's). Nothing was counted.
    assert!(freed > 0, "no collection ran");
    assert!(
        host.map_entry_count() < 5_000,
        "{} entries kept",
        host.map_entry_count()
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
