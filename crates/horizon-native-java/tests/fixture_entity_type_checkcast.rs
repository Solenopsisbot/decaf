#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/entity_type_checkcast/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/entity_type_checkcast/java25.txt");
const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier parse(String s){ throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/world/entity/EntityType.java",
        "package net.minecraft.world.entity; public class EntityType<T>{ public static class Builder {} }",
    ),
    (
        "net/minecraft/world/level/block/Block.java",
        "package net.minecraft.world.level.block; public class Block {}",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core; import net.minecraft.resources.Identifier; import java.util.Optional; public interface Registry<T>{ Optional<T> getOptional(Identifier i); Optional<T> get(Identifier i); T getValue(Identifier i); boolean containsKey(Identifier i); }",
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        "package net.minecraft.core; public interface DefaultedRegistry<T> extends Registry<T>{}",
    ),
    (
        "demo/UnknownInterface.java",
        "package demo; public interface UnknownInterface {}",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.DefaultedRegistry; import net.minecraft.world.entity.EntityType; public final class BuiltInRegistries { public static final DefaultedRegistry<EntityType<?>> ENTITY_TYPE=null; public static final DefaultedRegistry<Object> BLOCK=null; }",
    ),
];

#[test]
fn rows_match_java25() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("entity-type-lookup", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let captured = format!(
        "{}\n{}",
        fixture.text_on(&mut host, "rows"),
        fixture.text_on(&mut host, "rejected")
    );
    assert_eq!(captured, ORACLE.trim());
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn decidable_rejected_cast_throws() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("entity-type-checkcast-rejected", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "rejected"),
        "java.lang.ClassCastException"
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn undecidable_instanceof_is_counted() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("entity-type-checkcast-counted", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "undecidable"), "false");
    assert_eq!(
        host.unknown_host_calls(),
        &[(
            "demo/UnknownInterface#<instanceof>(Ljava/lang/Object;)Z".into(),
            1
        )]
    );
}

#[test]
fn othermod_lookup_is_counted() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("entity-type-lookup-counted", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.text_on(&mut host, "othermod");
    assert_eq!(host.unknown_host_calls(), &[("net/minecraft/core/DefaultedRegistry#getValue(Lnet/minecraft/resources/Identifier;)Ljava/lang/Object;".into(), 1)]);
}
