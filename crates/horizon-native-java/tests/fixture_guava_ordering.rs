//! Java 25 oracle fixture for Guava 33.6.0 `Ordering.natural()` and `reverse()` (jamlib).
//!
//! `fixtures/guava_ordering/demo/Probe.java` is one source run two ways: on Java 25 against
//! the real Guava jar (`java25.txt`, command in the README), and here, compiled against the
//! stub below. Only `demo/` ships, so every `Ordering` call reaches the host model, and
//! `rows()` must equal the capture byte for byte.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const ORDERING: &str = r#"package com.google.common.collect;
public abstract class Ordering<T> implements java.util.Comparator<T> {
 public static <C extends Comparable> Ordering<C> natural(){throw new AssertionError();}
 public <S extends T> Ordering<S> reverse(){throw new AssertionError();}
 public abstract int compare(T a, T b);
}"#;
const PROBE: &str = include_str!("fixtures/guava_ordering/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/guava_ordering/java25.txt");

#[test]
fn guava_ordering_rows_match_java25() {
    let fixture = Fixture::build(
        "guava-ordering",
        &[
            ("com/google/common/collect/Ordering.java", ORDERING),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows"), ORACLE);
    // Every Ordering call the probe makes is answered; nothing is counted.
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
