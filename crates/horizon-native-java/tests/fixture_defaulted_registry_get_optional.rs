#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const API_STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        r#"package net.minecraft.resources; public final class Identifier { public static Identifier parse(String s){throw new AssertionError();} }"#,
    ),
    (
        "net/minecraft/world/entity/EntityType.java",
        r#"package net.minecraft.world.entity; public class EntityType<T>{}"#,
    ),
    (
        "net/minecraft/core/Registry.java",
        r#"package net.minecraft.core; import net.minecraft.resources.Identifier; import java.util.Optional; public interface Registry<T>{ Optional<T> getOptional(Identifier i); T getValue(Identifier i); }"#,
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        r#"package net.minecraft.core; public interface DefaultedRegistry<T> extends Registry<T>{}"#,
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        r#"package net.minecraft.core.registries; import net.minecraft.core.DefaultedRegistry; import net.minecraft.world.entity.EntityType; public final class BuiltInRegistries { public static final DefaultedRegistry<EntityType<?>> ENTITY_TYPE=null; }"#,
    ),
];
const PROBE: &str = include_str!("fixtures/defaulted_registry_get_optional/demo/Probe.java");
#[test]
fn defaulted_registry_get_optional_java25() {
    let mut s = API_STUBS.to_vec();
    s.push(("demo/Probe.java", PROBE));
    let f = Fixture::build("defaulted-registry-get-optional", &s);
    let mut h = f
        .jar
        .seed_host(StdHost::new().with_environment_type("CLIENT"));
    assert_eq!(
        f.text_on(&mut h, "rows"),
        include_str!("fixtures/defaulted_registry_get_optional/java25.txt").trim()
    );
    assert_eq!(
        h.unknown_host_call_count(),
        0,
        "{:?}",
        h.unknown_host_calls()
    );
}
