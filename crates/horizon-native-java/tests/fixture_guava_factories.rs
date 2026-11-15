//! Java 25 oracle fixture for Guava 33.6.0 plain and identity factories.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const MAPS: &str = r#"package com.google.common.collect; public final class Maps {
 public static <K,V> java.util.IdentityHashMap<K,V> newIdentityHashMap(){throw new AssertionError();}
 public static <K,V> java.util.HashMap<K,V> newHashMapWithExpectedSize(int n){throw new AssertionError();}
 public static <K,V> java.util.concurrent.ConcurrentMap<K,V> newConcurrentMap(){throw new AssertionError();}
}"#;
const LISTS: &str = r#"package com.google.common.collect; public final class Lists {
 public static <E> java.util.ArrayList<E> newArrayListWithCapacity(int n){throw new AssertionError();}
 public static <E> java.util.ArrayList<E> newArrayListWithExpectedSize(int n){throw new AssertionError();}
}"#;
const SETS: &str = r#"package com.google.common.collect; public final class Sets {
 public static <E> java.util.Set<E> newIdentityHashSet(){throw new AssertionError();}
}"#;
const PROBE: &str = include_str!("fixtures/guava_factories/Probe.java");
#[test]
fn guava_identity_concurrent_and_capacity_cases_match_java25() {
    let fixture = Fixture::build(
        "guava-factories",
        &[
            ("com/google/common/collect/Maps.java", MAPS),
            ("com/google/common/collect/Lists.java", LISTS),
            ("com/google/common/collect/Sets.java", SETS),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "identity"), "2:null:2:false");
    assert_eq!(fixture.text_on(&mut host, "concurrent"), "1:npe:1");
    assert_eq!(
        fixture.text_on(&mut host, "capacities"),
        "expectedSize cannot be negative but was: -1|initialArraySize cannot be negative but was: -1|arraySize cannot be negative but was: -1"
    );
}
