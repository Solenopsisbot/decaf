//! Java 25 oracle fixture for Guava 33.6.0 `ImmutableList.builder()` (Archers' bundle
//! component and shulkerboxtooltip build their lists this way).
//!
//! `fixtures/guava_immutable_list_builder/demo/Probe.java` is one source run two ways:
//! on Java 25 against the real Guava jar (`java25.txt`; command and jar SHA-256 in the
//! README), and here, compiled against the stubs below. Only `demo/` ships, so every
//! Guava call reaches the host model, and `rows()` must equal the capture byte for byte.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const IMMUTABLE_LIST: &str = r#"package com.google.common.collect;
public abstract class ImmutableList<E> extends java.util.AbstractList<E> {
 public static <E> ImmutableList<E> of(){throw new AssertionError();}
 public static <E> Builder<E> builder(){throw new AssertionError();}
 public static <E> Builder<E> builderWithExpectedSize(int n){throw new AssertionError();}
 public static final class Builder<E> {
  public Builder<E> add(E e){throw new AssertionError();}
  @SafeVarargs public final Builder<E> add(E... es){throw new AssertionError();}
  public Builder<E> addAll(Iterable<? extends E> es){throw new AssertionError();}
  public Builder<E> addAll(java.util.Iterator<? extends E> es){throw new AssertionError();}
  public ImmutableList<E> build(){throw new AssertionError();}
 }
}"#;
const PROBE: &str = include_str!("fixtures/guava_immutable_list_builder/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/guava_immutable_list_builder/java25.txt");

#[test]
fn immutable_list_builder_rows_match_java25() {
    let fixture = Fixture::build(
        "guava-immutable-list-builder",
        &[
            (
                "com/google/common/collect/ImmutableList.java",
                IMMUTABLE_LIST,
            ),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows"), ORACLE);
}
