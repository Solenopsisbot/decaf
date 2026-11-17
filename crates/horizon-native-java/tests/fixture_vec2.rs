#[path = "support/javac_fixture.rs"] mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const VEC2:&str=r#"package net.minecraft.world.phys;
public class Vec2 { public static final Vec2 ZERO=null,ONE=null,UNIT_X=null,NEG_UNIT_X=null,UNIT_Y=null,NEG_UNIT_Y=null,MAX=null,MIN=null; public final float x; public final float y; public Vec2(float x,float y){this.x=x;this.y=y;} public Vec2 scale(float x){throw new AssertionError();} public float dot(Vec2 x){throw new AssertionError();} public Vec2 add(Vec2 x){throw new AssertionError();} public Vec2 add(float x){throw new AssertionError();} public Vec2 normalized(){throw new AssertionError();} public float length(){throw new AssertionError();} public float lengthSquared(){throw new AssertionError();} public float distanceToSqr(Vec2 x){throw new AssertionError();} public Vec2 negated(){throw new AssertionError();} public Vec2 rotate(double x){throw new AssertionError();} }"#;
const PROBE:&str=include_str!("fixtures/vec2/demo/Probe.java"); const ORACLE:&str=include_str!("fixtures/vec2/java25.txt");
#[test] fn vec2_rows_match_java25(){let f=Fixture::build("vec2",&[("net/minecraft/world/phys/Vec2.java",VEC2),("demo/Probe.java",PROBE)]); let mut h=f.jar.seed_host(StdHost::new()); assert_eq!(f.text_on(&mut h,"main"),ORACLE); assert!(h.unknown_host_calls().is_empty(),"{:?}",h.unknown_host_calls());}
