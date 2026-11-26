//! Seeded `java.util.Random`s made by `new` and by a constructor reference are distinct objects
//! with their own sequences (#667 review).
//!
//! The host's `Random` model numbered its objects with a counter of its own, the runtime's
//! allocator numbered the constructor reference's: the first `Random::new` got
//! `java/util/Random#0`, the seeded `new Random(1L)`'s key, and its `<init>(J)` reseeded that
//! object (`true|-1154715079|1260042744`, nothing counted). Both now take the heap's numbers.
//!
//! `fixtures/random_handle_keys/demo/Probe.java` runs unchanged on both sides.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;

/// What `java -cp . demo.Probe` printed on Java 25 (Temurin 25.0.5, `javac --release 21`), the
/// same on two runs.
const ORACLE: &str = include_str!("fixtures/random_handle_keys/java25.txt");

const PROBE: &str = include_str!("fixtures/random_handle_keys/demo/Probe.java");

#[test]
fn seeded_randoms_from_new_and_a_constructor_reference_never_share_a_key() {
    let fixture =
        javac_fixture::Fixture::build("random-handle-keys", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "run"), ORACLE.trim_end());
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}
