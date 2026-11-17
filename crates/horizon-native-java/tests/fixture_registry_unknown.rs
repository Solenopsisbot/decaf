#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
const PROBE: &str = include_str!("fixtures/registry_unknown/demo/Probe.java");
const ID: &str = r#"package net.minecraft.resources; public class Identifier { public static Identifier parse(String s){return null;} }"#;
const REG: &str = r#"package net.minecraft.core.registries; import java.util.*; import net.minecraft.resources.Identifier; public class Registry<T> { public boolean containsKey(Identifier i){throw new AssertionError();} public Optional<T> get(Identifier i){throw new AssertionError();} }"#;
const BUILTIN: &str = r#"package net.minecraft.core.registries; public class BuiltInRegistries { public static final Registry<Object> BLOCK=null; public static final Registry<Object> ITEM=null; }"#;
#[test]
fn vanilla_lookups_are_counted() {
    let f = javac_fixture::Fixture::build(
        "registry-unknown",
        &[
            ("demo/Probe.java", PROBE),
            ("net/minecraft/resources/Identifier.java", ID),
            ("net/minecraft/core/registries/Registry.java", REG),
            (
                "net/minecraft/core/registries/BuiltInRegistries.java",
                BUILTIN,
            ),
        ],
    );
    let mut h = f.jar.seed_host(StdHost::new());
    let _ = f
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "rows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut h,
        )
        .unwrap();
    assert_eq!(h.unknown_host_calls(), vec![("net/minecraft/core/registries/Registry#containsKey(Lnet/minecraft/resources/Identifier;)Z".into(), 4), ("net/minecraft/core/registries/Registry#get(Lnet/minecraft/resources/Identifier;)Ljava/util/Optional;".into(), 4)]);
}
