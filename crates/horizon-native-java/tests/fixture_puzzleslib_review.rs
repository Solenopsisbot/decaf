#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const STUBS: &[(&str, &str)] = &[
    (
        "com/google/common/collect/Queues.java",
        r#"package com.google.common.collect; public final class Queues { public static <E> java.util.concurrent.ConcurrentLinkedQueue<E> newConcurrentLinkedQueue(){ throw new AssertionError(); } }"#,
    ),
    (
        "com/google/common/collect/ImmutableMap.java",
        r#"package com.google.common.collect; public final class ImmutableMap { public static final class Builder<K,V> { public Builder<K,V> put(K k,V v){throw new AssertionError();} public java.util.Map<K,V> build(){throw new AssertionError();} } public static <K,V> Builder<K,V> builder(){throw new AssertionError();} }"#,
    ),
    (
        "com/google/common/collect/Maps.java",
        r#"package com.google.common.collect; public final class Maps { public static <K extends Enum<K>,V> java.util.Map<K,V> immutableEnumMap(java.util.Map<K,V> m){throw new AssertionError();} }"#,
    ),
    (
        "net/fabricmc/fabric/api/event/Event.java",
        r#"package net.fabricmc.fabric.api.event; public interface Event<T> { public static final net.minecraft.resources.Identifier DEFAULT_PHASE = null; }"#,
    ),
    (
        "net/fabricmc/fabric/api/biome/v1/ModificationPhase.java",
        r#"package net.fabricmc.fabric.api.biome.v1; public final class ModificationPhase { public static final ModificationPhase ADDITIONS=null, REMOVALS=null, REPLACEMENTS=null, POST_PROCESSING=null; }"#,
    ),
    (
        "net/minecraft/resources/Identifier.java",
        r#"package net.minecraft.resources; public final class Identifier { }"#,
    ),
];

const PROBE: &str = include_str!("fixtures/puzzleslib_review/Probe.java");

#[test]
fn review_semantics_match_java25() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("puzzleslib-review", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "queue"), "npe:true:x");
    assert_eq!(
        fixture.text_on(&mut host, "immutable"),
        "iae:Multiple entries with same key: a=2 and a=1|uoe:1"
    );
    assert_eq!(fixture.text_on(&mut host, "enumMap"), "ZAM");
    assert_eq!(fixture.text_on(&mut host, "phases"), "true|true|true");
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
