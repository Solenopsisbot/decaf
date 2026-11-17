//! Identity collections whose keys are equal strings of unknown identity (#464).
//!
//! `fixtures/identity_uncertain/demo/Probe.java` runs on Java 25 against Guava 33.6.0
//! (`java25.txt`, command in the README), and here against compile-only stubs: only `demo/`
//! ships. Decaf keeps a string as its text, so it can't tell two equal strings apart as objects
//! (`immutable_collections.rs` `identity_unknown`). A call whose answer depends on that is
//! counted. It must not also run, and a counted change leaves the collection uncertain, so every
//! later call on it is counted rather than answered from contents the call may have changed.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/identity_uncertain/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/identity_uncertain/java25.txt");

const STUBS: &[(&str, &str)] = &[
    (
        "com/google/common/collect/Maps.java",
        "package com.google.common.collect; public final class Maps { public static <K,V> java.util.IdentityHashMap<K,V> newIdentityHashMap(){throw new AssertionError();} }",
    ),
    (
        "com/google/common/collect/Sets.java",
        "package com.google.common.collect; public final class Sets { public static <E> java.util.Set<E> newIdentityHashSet(){throw new AssertionError();} }",
    ),
];

const PUT: &str = "java/util/Map#put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;";
const MAP_SIZE: &str = "java/util/Map#size()I";
const MAP_GET: &str = "java/util/Map#get(Ljava/lang/Object;)Ljava/lang/Object;";
const ADD: &str = "java/util/Set#add(Ljava/lang/Object;)Z";
const SET_SIZE: &str = "java/util/Set#size()I";
const CONTAINS: &str = "java/util/Set#contains(Ljava/lang/Object;)Z";

fn java_row(name: &str) -> &'static str {
    ORACLE
        .lines()
        .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no {name} row in java25.txt"))
}

#[test]
fn identity_collections_count_what_identity_decides() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("identity-uncertain", &sources);

    // Distinct texts: exact, nothing counted.
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "exact"), java_row("exact"));
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());

    // Equal texts: the second change is counted, and so is every read after it. Java's answers
    // (the capture's rows) depend on object identity the runtime doesn't keep.
    for (method, counted) in [
        ("jdkMap", vec![(MAP_GET, 1), (PUT, 1), (MAP_SIZE, 1)]),
        ("guavaMap", vec![(MAP_GET, 1), (PUT, 1), (MAP_SIZE, 1)]),
        ("guavaSet", vec![(ADD, 1), (CONTAINS, 1), (SET_SIZE, 1)]),
    ] {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let decaf = fixture.text_on(&mut host, method);
        let expected: Vec<(String, usize)> = counted
            .iter()
            .map(|(call, n)| (call.to_string(), *n))
            .collect();
        assert_eq!(
            host.unknown_host_calls(),
            expected,
            "{method}: Decaf {decaf}, Java {}",
            java_row(method)
        );
    }
}
