#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;

const PROBE: &str = include_str!("fixtures/fixture_vanilla_registry/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/fixture_vanilla_registry/java25.txt");

const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier parse(String s) { return null; } }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core; import java.util.Optional; import net.minecraft.resources.Identifier; public interface Registry<T> { Optional<T> get(Identifier id); T getValue(Identifier id); Identifier getKey(T value); boolean containsKey(Identifier id); }",
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        "package net.minecraft.core; public interface DefaultedRegistry<T> extends Registry<T> {}",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.DefaultedRegistry; public final class BuiltInRegistries { public static final DefaultedRegistry<?> BLOCK = null; public static final DefaultedRegistry<?> ITEM = null; }",
    ),
];

fn fixture(name: &str) -> javac_fixture::Fixture {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    javac_fixture::Fixture::build(name, &sources)
}

#[test]
fn vanilla_rows_match_java25() {
    let fixture = fixture("vanilla-registry");
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, "rows");
    assert_eq!(text, ORACLE.trim());
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn unseen_modded_ids_stay_counted() {
    let fixture = fixture("vanilla-registry-modded");
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, "modded");
    assert_eq!(text, "false|false|false|false");
    assert_eq!(
        host.unknown_host_calls(),
        &[
            ("net/minecraft/core/DefaultedRegistry#containsKey(Lnet/minecraft/resources/Identifier;)Z".into(), 2),
            ("net/minecraft/core/DefaultedRegistry#get(Lnet/minecraft/resources/Identifier;)Ljava/util/Optional;".into(), 2),
        ]
    );
}
