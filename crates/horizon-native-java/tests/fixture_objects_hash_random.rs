#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;
const PROBE: &str = include_str!("fixtures/objects_hash_random/demo/Probe.java");
const JAVA25: &str = include_str!("fixtures/objects_hash_random/java25.txt");
fn fixture() -> Fixture {
    Fixture::build("objects-hash-random", &[("demo/Probe.java", PROBE)])
}
fn invoke(f: &Fixture, m: &str) -> (String, Vec<(String, usize)>) {
    let mut h = f.jar.seed_host(StdHost::new());
    let v = f
        .jar
        .invoke_method_with_args("demo/Probe", m, "()Ljava/lang/String;", None, &[], &mut h)
        .unwrap();
    let t = match v {
        Value::Ref(Some(t)) => t,
        x => panic!("{x:?}"),
    };
    (t, h.unknown_host_calls())
}
fn row(m: &str) -> &str {
    JAVA25
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{m}=")))
        .unwrap()
}
#[test]
fn normal_rows() {
    for m in ["objectsRows", "seededRandomRows", "badBound"] {
        let (a, u) = invoke(&fixture(), m);
        assert_eq!(a, row(m));
        assert!(u.is_empty(), "{u:?}");
    }
}
#[test]
fn unseeded_is_counted() {
    let (a, u) = invoke(&fixture(), "unseededRandom");
    assert_eq!(a, "0");
    assert_eq!(u, vec![("java/util/Random#nextInt(I)I".into(), 1)]);
}
#[test]
fn ranged_random_is_counted() {
    // Java 25 answers 3: RandomGenerator's nextInt(origin, bound) draws
    // differently from Minecraft's RandomSource for power-of-two ranges, so the
    // call is counted, not answered with the Minecraft algorithm.
    let (a, u) = invoke(&fixture(), "rangedRandom");
    assert_eq!(a, "0");
    assert_eq!(u, vec![("java/util/Random#nextInt(II)I".into(), 1)]);
}
#[test]
fn unknown_hash_is_counted() {
    let (a, u) = invoke(&fixture(), "unknownObjectHash");
    assert_eq!(a, "0");
    assert_eq!(
        u,
        vec![("java/util/Objects#hash([Ljava/lang/Object;)I".into(), 1)]
    );
}
