//! Java 25 fixture for the small Guava/Commons calls used by puzzleslib.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const STRING_UTILS_STUB: &str = r#"package org.apache.commons.lang3;
public final class StringUtils {
    public static boolean isNotEmpty(CharSequence value) { throw new AssertionError(); }
}
"#;

const QUEUES_STUB: &str = r#"package com.google.common.collect;
public final class Queues {
    public static <E> java.util.concurrent.ConcurrentLinkedQueue<E> newConcurrentLinkedQueue() {
        throw new AssertionError();
    }
}
"#;

const MAPS_STUB: &str = r#"package com.google.common.collect;
public final class Maps {
    public static <K extends Enum<K>, V> ImmutableMap<K, V> immutableEnumMap(java.util.Map<K, V> source) {
        throw new AssertionError();
    }
}
"#;

const IMMUTABLE_MAP_STUB: &str = r#"package com.google.common.collect;
public class ImmutableMap<K, V> extends java.util.AbstractMap<K, V> {
    public java.util.Set<java.util.Map.Entry<K, V>> entrySet() { throw new AssertionError(); }
}
"#;

const PROBE: &str = include_str!("fixtures/puzzleslib/Probe.java");

#[test]
fn puzzleslib_dependency_calls_match_java25() {
    let fixture = Fixture::build(
        "puzzleslib",
        &[
            (
                "org/apache/commons/lang3/StringUtils.java",
                STRING_UTILS_STUB,
            ),
            ("com/google/common/collect/Queues.java", QUEUES_STUB),
            ("com/google/common/collect/Maps.java", MAPS_STUB),
            (
                "com/google/common/collect/ImmutableMap.java",
                IMMUTABLE_MAP_STUB,
            ),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "modId"), "true|false|false");
    assert_eq!(fixture.text_on(&mut host, "queue"), "true|one|true");
    assert_eq!(fixture.text_on(&mut host, "phase"), "add|post");
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
