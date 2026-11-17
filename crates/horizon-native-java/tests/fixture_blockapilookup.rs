#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;
const LOOKUP: &str = r#"package net.fabricmc.fabric.api.lookup.v1.block;
import net.minecraft.core.Direction; import net.minecraft.resources.Identifier;
public abstract class BlockApiLookup<A,C> { public static <A,C> BlockApiLookup<A,C> get(Identifier i, Class<A> a, Class<C> c){throw new AssertionError();} public Identifier getId(){throw new AssertionError();} public Class<A> apiClass(){throw new AssertionError();} public Class<C> contextClass(){throw new AssertionError();} public void registerFallback(BlockApiProvider<A,C> p){throw new AssertionError();} }

"#;
const IDENT: &str = r#"package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String n,String p){throw new AssertionError();} }"#;
const DIR: &str = r#"package net.minecraft.core; public enum Direction { NORTH }"#;
const PROBE: &str = include_str!("fixtures/blockapilookup/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/blockapilookup/java25.txt");
const ISOLATION: &str = r#"package demo; import net.fabricmc.fabric.api.lookup.v1.block.BlockApiLookup; import net.minecraft.resources.Identifier; public class Isolation { public static String run(){ BlockApiLookup.get(Identifier.fromNamespaceAndPath("demo","energy"), String.class, Void.class); return "ok"; } }"#;
#[test]
fn rows_match_java25() {
    let f = Fixture::build(
        "blockapilookup",
        &[
            (
                "net/fabricmc/fabric/api/lookup/v1/block/BlockApiLookup.java",
                LOOKUP,
            ),
            (
                "net/fabricmc/fabric/api/lookup/v1/block/BlockApiProvider.java",
                r#"package net.fabricmc.fabric.api.lookup.v1.block; public interface BlockApiProvider<A,C> { A find(Object w,Object p,Object st,Object be,C c); }"#,
            ),
            ("net/minecraft/resources/Identifier.java", IDENT),
            ("net/minecraft/core/Direction.java", DIR),
            ("demo/Probe.java", PROBE),
            ("demo/Isolation.java", ISOLATION),
        ],
    );
    let mut h = f.jar.seed_host(StdHost::new());
    let rows = f.text_on(&mut h, "rows");
    let unknown = h.unknown_host_calls().to_vec();
    assert_eq!((rows, unknown), (ORACLE.to_string(), Vec::new()));
}

#[test]
fn registrations_are_per_host() {
    let f = Fixture::build(
        "blockapilookup-isolation",
        &[
            (
                "net/fabricmc/fabric/api/lookup/v1/block/BlockApiLookup.java",
                LOOKUP,
            ),
            (
                "net/fabricmc/fabric/api/lookup/v1/block/BlockApiProvider.java",
                r#"package net.fabricmc.fabric.api.lookup.v1.block; public interface BlockApiProvider<A,C> { A find(Object w,Object p,Object st,Object be,C c); }"#,
            ),
            ("net/minecraft/resources/Identifier.java", IDENT),
            ("net/minecraft/core/Direction.java", DIR),
            ("demo/Probe.java", PROBE),
            ("demo/Isolation.java", ISOLATION),
        ],
    );
    let mut first = f.jar.seed_host(StdHost::new());
    let _ = f.text_on(&mut first, "rows");
    let mut second = f.jar.seed_host(StdHost::new());
    let value = f
        .jar
        .invoke_method_with_args(
            "demo/Isolation",
            "run",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut second,
        )
        .expect("run");
    assert!(matches!(value, Value::Ref(Some(ref text)) if text == "ok"));
}
