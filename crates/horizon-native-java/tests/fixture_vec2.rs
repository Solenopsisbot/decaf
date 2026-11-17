#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const VEC2: &str = r#"package net.minecraft.world.phys;
public final class Vec2 { public static final Vec2 ZERO=null, ONE=null, UNIT_X=null, NEG_UNIT_X=null, UNIT_Y=null, NEG_UNIT_Y=null, MAX=null, MIN=null; public final float x; public final float y; public Vec2(float x,float y){this.x=x;this.y=y;} }
"#;
const PROBE: &str = include_str!("fixtures/vec2/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/vec2/java25.txt");
#[test]
fn vec2_rows_match_java25() {
    let fixture = Fixture::build(
        "vec2",
        &[
            ("net/minecraft/world/phys/Vec2.java", VEC2),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, "rows");
    assert_eq!(text, ORACLE);
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}
