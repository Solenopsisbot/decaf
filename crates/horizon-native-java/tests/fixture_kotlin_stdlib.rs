//! Java 25 oracles for the bounded Kotlin 2.4.21 stdlib surface.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

const PROBE: &str = include_str!("fixtures/kotlin_stdlib/demo/Probe.java");
const ENUM_ENTRIES: &str = include_str!("fixtures/kotlin_stdlib/kotlin/enums/EnumEntries.java");
const ENUM_ENTRIES_KT: &str =
    include_str!("fixtures/kotlin_stdlib/kotlin/enums/EnumEntriesKt.java");
const FUNCTION0: &str = include_str!("fixtures/kotlin_stdlib/kotlin/jvm/functions/Function0.java");
const INTRINSICS: &str = include_str!("fixtures/kotlin_stdlib/kotlin/jvm/internal/Intrinsics.java");
const LAZY: &str = include_str!("fixtures/kotlin_stdlib/kotlin/Lazy.java");
const LAZY_KT: &str = include_str!("fixtures/kotlin_stdlib/kotlin/LazyKt.java");
const COLLECTIONS: &str =
    include_str!("fixtures/kotlin_stdlib/kotlin/collections/CollectionsKt.java");
const MAPS: &str = include_str!("fixtures/kotlin_stdlib/kotlin/collections/MapsKt.java");
const SETS: &str = include_str!("fixtures/kotlin_stdlib/kotlin/collections/SetsKt.java");
const EMPTY_LIST: &str = include_str!("fixtures/kotlin_stdlib/kotlin/collections/EmptyList.java");

fn fixture() -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(
        "kotlin-stdlib",
        &[
            ("demo/Probe.java", PROBE),
            ("kotlin/enums/EnumEntries.java", ENUM_ENTRIES),
            ("kotlin/enums/EnumEntriesKt.java", ENUM_ENTRIES_KT),
            ("kotlin/jvm/functions/Function0.java", FUNCTION0),
            ("kotlin/jvm/internal/Intrinsics.java", INTRINSICS),
            ("kotlin/Lazy.java", LAZY),
            ("kotlin/LazyKt.java", LAZY_KT),
            ("kotlin/collections/CollectionsKt.java", COLLECTIONS),
            ("kotlin/collections/MapsKt.java", MAPS),
            ("kotlin/collections/SetsKt.java", SETS),
            ("kotlin/collections/EmptyList.java", EMPTY_LIST),
        ],
    )
}

#[test]
fn kotlin_stdlib_241_oracle() {
    let fixture = fixture();
    let expected = [
        "enumEntries",
        "intrinsics",
        "lazy",
        "collections",
        "crossCollectionEquality",
        "lazyOf",
        "readResolveIdentity",
    ]
    .into_iter()
    .map(|method| {
        let value = include_str!("fixtures/kotlin_stdlib/java25.txt")
            .lines()
            .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
            .expect("oracle method");
        (method, value)
    });
    let mut failures = Vec::new();
    for (method, expected) in expected {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let actual = fixture.text_on(&mut host, method);
        if actual != expected || !host.unknown_host_calls().is_empty() {
            failures.push(format!(
                "{method}: java={expected:?}, native={actual:?}, unknown={:?}",
                host.unknown_host_calls()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    // A cached value on a discarded lazy holder is collected, while the
    // earlier callback fixture proves a reachable static lazy survives.
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "droppedLazy"), "dropped");
    assert!(host.resident_heap().instance_fields() > 0);
    assert_eq!(fixture.text_on(&mut host, "forceCollectionOnly"), "forced");
    assert_eq!(host.resident_heap().instance_fields(), 0);

    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "lazyAcrossCallbacks"),
        "Lazy value not initialized yet.|false"
    );
    assert_eq!(
        fixture.text_on(&mut host, "lazyAfterCollection"),
        "kept|1|true"
    );
    assert_eq!(
        fixture.text_on(&mut host, "lazyRetriesAfterThrow"),
        "java.lang.IllegalStateException|boom|retry|2|true"
    );
}
