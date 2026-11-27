//! Collections the host caches for their owner survive collections exactly
//! as long as their owner (#226 re-check 4). #233's cache
//! (`StdHost::cached_collection`: `System.getProperties()`, a map's views, a
//! mod container's root paths) was neither traced nor pruned by the
//! collector, so `System.getProperties()` lost what a jar put into it after
//! a collection, and every fresh map's views stayed cached forever.
//!
//! `fixtures/heap_host_tables/demo/Rc226d.java` and `SubListTick.java` run
//! unchanged on both sides (their `main`s on Java 25 are `java25.txt`);
//! `Roots.java` needs Fabric's
//! loader, so its answer is the one before any collection, which a
//! collection must not change.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

/// What `java -cp . demo.Rc226d` printed on Java 25 (Temurin 25.0.5,
/// `javac --release 21`), the same on two runs.
const ORACLE: &str = include_str!("fixtures/heap_host_tables/java25.txt");

const STUBS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/fabric_runtime/stubs"
);

fn expected(name: &str) -> &'static str {
    ORACLE
        .lines()
        .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no oracle line for {name}"))
}

fn fixture() -> javac_fixture::Fixture {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((name, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut sources = vec![
        (
            "demo/Rc226d.java".to_owned(),
            include_str!("fixtures/heap_host_tables/demo/Rc226d.java").to_owned(),
        ),
        (
            "demo/SubListTick.java".to_owned(),
            include_str!("fixtures/heap_host_tables/demo/SubListTick.java").to_owned(),
        ),
        (
            "demo/Roots.java".to_owned(),
            include_str!("fixtures/heap_host_tables/demo/Roots.java").to_owned(),
        ),
    ];
    walk(
        std::path::Path::new(STUBS),
        std::path::Path::new(STUBS),
        &mut sources,
    );
    let sources: Vec<(&str, &str)> = sources
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    javac_fixture::Fixture::build("heap-host-tables", &sources)
}

fn call(fixture: &javac_fixture::Fixture, host: &mut StdHost, class: &str, method: &str) -> String {
    match fixture.jar.invoke_method_with_args(
        class,
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

/// What a jar put into `System.getProperties()` (a string, an object, an
/// object with no row) and a mod container's cached root paths and license
/// stay through three rounds of 40 garbage-making calls, each round past a
/// collection, as do row-less objects held in a static, lambdas, an
/// `Optional`, an entry, an array, a map key, views, an `IdentityHashMap`,
/// and a `UUID`/`Locale`. Java 25's answer, with the opaque V0 license observation counted on each of the four calls.
#[test]
fn what_a_jar_keeps_in_cached_collections_survives_collections() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(
        StdHost::new()
            .with_environment_type("SERVER")
            .with_mod("moda", "1.0"),
    );
    assert_eq!(
        call(&fixture, &mut host, "demo/Rc226d", "setup"),
        expected("setup")
    );
    assert_eq!(call(&fixture, &mut host, "demo/Roots", "hold"), "ok");
    assert_eq!(
        call(&fixture, &mut host, "demo/Rc226d", "check"),
        expected("check")
    );
    let roots = call(&fixture, &mut host, "demo/Roots", "check");
    assert_eq!(roots, "true|/horizon/mods/moda|true|1|true");
    for round in 1..=3 {
        for _ in 0..40 {
            call(&fixture, &mut host, "demo/Rc226d", "garbage");
        }
        assert_eq!(
            call(&fixture, &mut host, "demo/Rc226d", "check"),
            expected("check"),
            "round {round}"
        );
        assert_eq!(
            call(&fixture, &mut host, "demo/Roots", "check"),
            roots,
            "round {round}"
        );
    }
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "net/fabricmc/loader/api/metadata/ModMetadata#getLicense()Ljava/util/Collection;"
                .to_owned(),
            5,
        )]
    );
}

/// A fresh map per call read through its three views keeps nothing cached
/// once the map is collected: the cache follows the live maps, not how many
/// were made (+247 MB over 300k calls before).
#[test]
fn a_fresh_maps_views_go_with_the_map() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        call(&fixture, &mut host, "demo/Rc226d", "mapTick"),
        expected("mapTick").split(' ').next().unwrap()
    );
    let mark = host.call_log_mark();
    let mut most = 0;
    for _ in 0..20_000 {
        assert_eq!(call(&fixture, &mut host, "demo/Rc226d", "mapTick"), "3");
        host.rewind_call_log(mark);
        most = most.max(host.cached_collection_count());
    }
    assert!(most < 10_000, "{most} cached collections");
    assert!(host.unknown_host_calls().is_empty());
}

/// A fresh sub-list per call keeps no class row once the list is collected: the host's
/// `getClass` answer for a sub-list (`sub_list_class_names`) follows the live lists, not how
/// many were made. Unpruned, a pouch tooltip's per-frame `subList` left one row a draw for good
/// (#609 round 5, about 116 bytes a draw). Every call answers Java 25's classes.
#[test]
fn a_fresh_sub_lists_class_goes_with_the_list() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    let oracle = expected("subListTick");
    let mark = host.call_log_mark();
    let collections = host.collection_count();
    let mut most = 0;
    for call_number in 0..20_000 {
        assert_eq!(
            call(&fixture, &mut host, "demo/SubListTick", "tick"),
            oracle,
            "call {call_number}"
        );
        host.rewind_call_log(mark);
        most = most.max(host.sub_list_class_count());
    }
    assert!(host.collection_count() > collections + 2, "collections ran");
    assert!(most < 5_000, "{most} sub-list classes kept");
    assert!(host.unknown_host_calls().is_empty());
}
