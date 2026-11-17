use horizon_native_java::StdHost;
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use javac_fixture::Fixture;
const STUB: &str = r#"package net.minecraft.util; public abstract class RandomSource { public static RandomSource create(){throw new AssertionError();} public static RandomSource create(long x){throw new AssertionError();} public abstract long nextLong(); public abstract int nextInt(); public abstract float nextFloat(); public abstract double nextDouble(); }"#;
const PROBE: &str = include_str!("fixtures/randomsource_create/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/randomsource_create/java25.txt");
#[test]
fn randomsource_create_rows_match_java25() {
    let f = Fixture::build(
        "randomsource-create",
        &[
            ("net/minecraft/util/RandomSource.java", STUB),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut h = f.jar.seed_host(StdHost::new());
    assert_eq!(f.text_on(&mut h, "rows"), ORACLE);
}
