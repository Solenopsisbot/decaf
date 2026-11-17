//! Java 25 oracle fixture for the vanilla `Enchantments` `ResourceKey` constants
//! (`vanilla_registry_keys.rs`; Spell Engine's `RPGSeriesCore.init` puts three in a `Set`).
//!
//! `fixtures/vanilla_enchantment_keys/demo/Probe.java` is one source run two ways: on Java 25
//! against the real 26.3 game jar after bootstrap (`java25.txt`, command in the README), and
//! here, compiled against the stubs below. Only `demo/` ships, so every game call reaches the
//! host model; `rows()` must equal the capture byte for byte and nothing may be counted.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core; public interface Registry<T> {}",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier {
 public static Identifier parse(String id) { throw new AssertionError(); }
 public static Identifier withDefaultNamespace(String path) { throw new AssertionError(); }
}",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources; import net.minecraft.core.Registry;
public class ResourceKey<T> {
 public static <T> ResourceKey<T> create(ResourceKey<? extends Registry<T>> registry, Identifier id) { throw new AssertionError(); }
 public Identifier identifier() { throw new AssertionError(); }
 public Identifier registry() { throw new AssertionError(); }
}",
    ),
    (
        "net/minecraft/world/item/enchantment/Enchantment.java",
        "package net.minecraft.world.item.enchantment; public final class Enchantment {}",
    ),
    (
        "net/minecraft/core/registries/Registries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.Registry; import net.minecraft.resources.ResourceKey; import net.minecraft.world.item.enchantment.Enchantment;
public class Registries {
 public static final ResourceKey<Registry<Enchantment>> ENCHANTMENT = null;
}",
    ),
    (
        "net/minecraft/world/item/enchantment/Enchantments.java",
        "package net.minecraft.world.item.enchantment; import net.minecraft.resources.ResourceKey;
public class Enchantments {
 public static final ResourceKey<Enchantment> PROTECTION = null, KNOCKBACK = null, FIRE_ASPECT = null, LOOTING = null;
}",
    ),
];
const PROBE: &str = include_str!("fixtures/vanilla_enchantment_keys/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/vanilla_enchantment_keys/java25.txt");

#[test]
fn vanilla_enchantment_key_rows_match_java25() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("vanilla-enchantment-keys", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    let unknown = host.unknown_host_calls();
    assert_eq!(rows, ORACLE, "unknown calls: {unknown:?}");
    assert!(unknown.is_empty(), "{unknown:?}");
}
