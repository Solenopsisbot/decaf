//! BUGS-3d34-round2 #4: an item's `Item.Properties.component(DataComponents.LORE, new
//! ItemLore(..))` reaches its content declaration as the `components` patch, each lore encoded as
//! 26.3's `ItemLore.CODEC` writes it to JSON.
//!
//! Expected values are `fixtures/item_lore/java25.txt`, captured from the real Minecraft 26.3
//! server jar on OpenJDK 25.0.5+8 by `fixtures/item_lore/ItemLoreOracle.java` (see its header for
//! the command). The demo mod builds the same lores; the game classes are compile-only stubs, so
//! every call reaches the runtime's models as a real mod's does.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{NativeJar, StdHost, content::declarations};
use serde_json::Value as Json;
use std::collections::BTreeMap;

const JAVA25: &str = include_str!("fixtures/item_lore/java25.txt");

const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api; public interface ModInitializer { void onInitialize(); }",
    ),
    (
        "net/minecraft/ChatFormatting.java",
        "package net.minecraft; public enum ChatFormatting { BLACK, DARK_BLUE, DARK_GREEN, DARK_AQUA, DARK_RED, DARK_PURPLE, GOLD, GRAY, DARK_GRAY, BLUE, GREEN, AQUA, RED, LIGHT_PURPLE, YELLOW, WHITE, OBFUSCATED, BOLD, STRIKETHROUGH, UNDERLINE, ITALIC, RESET; }",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat; public interface Component { static MutableComponent translatable(String k) { return null; } static MutableComponent translatable(String k, Object... a) { return null; } static MutableComponent literal(String s) { return null; } }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat; import net.minecraft.ChatFormatting; public interface MutableComponent extends Component { MutableComponent withStyle(ChatFormatting f); MutableComponent withStyle(ChatFormatting... f); MutableComponent append(Component c); }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { return null; } }",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources; public final class ResourceKey<T> { public static <T> ResourceKey<T> create(ResourceKey<? extends net.minecraft.core.Registry<T>> r, Identifier id) { return null; } }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core; import net.minecraft.resources.Identifier; public interface Registry<T> { static <V, T extends V> T register(Registry<V> r, Identifier id, T v) { return v; } }",
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        "package net.minecraft.core; public interface DefaultedRegistry<T> extends Registry<T> {}",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.DefaultedRegistry; import net.minecraft.world.item.Item; public final class BuiltInRegistries { public static final DefaultedRegistry<Item> ITEM = null; }",
    ),
    (
        "net/minecraft/core/registries/Registries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.Registry; import net.minecraft.resources.ResourceKey; import net.minecraft.world.item.Item; public final class Registries { public static final ResourceKey<Registry<Item>> ITEM = null; }",
    ),
    (
        "net/minecraft/core/component/DataComponentType.java",
        "package net.minecraft.core.component; public interface DataComponentType<T> {}",
    ),
    (
        "net/minecraft/core/component/DataComponents.java",
        "package net.minecraft.core.component; import net.minecraft.world.item.component.ItemLore; public final class DataComponents { public static final DataComponentType<ItemLore> LORE = null; }",
    ),
    (
        "net/minecraft/world/item/component/ItemLore.java",
        "package net.minecraft.world.item.component; import java.util.List; import net.minecraft.network.chat.Component; public record ItemLore(List<Component> lines, List<Component> styledLines) { public ItemLore(List<Component> lines) { this(lines, lines); } }",
    ),
    (
        "net/minecraft/world/item/Item.java",
        "package net.minecraft.world.item; import net.minecraft.core.component.DataComponentType; import net.minecraft.resources.ResourceKey; public class Item { public static class Properties { public Properties setId(ResourceKey<Item> k) { return this; } public Properties stacksTo(int n) { return this; } public <T> Properties component(DataComponentType<T> t, T v) { return this; } } public Item(Properties p) {} }",
    ),
];

const DEMO: &str = r#"package demo;
import java.util.List;
import net.fabricmc.api.ModInitializer;
import net.minecraft.ChatFormatting;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.component.ItemLore;

public final class Probe implements ModInitializer {
  static void item(String path, Item.Properties properties) {
    Identifier id = Identifier.fromNamespaceAndPath("fixture", path);
    Registry.register(BuiltInRegistries.ITEM, id,
        new Item(properties.setId(ResourceKey.create(Registries.ITEM, id))));
  }

  @Override
  public void onInitialize() {
    // The same lores as ItemLoreOracle's cases, built the way Runes builds its pouches'.
    item("wand", new Item.Properties().stacksTo(1).component(DataComponents.LORE,
        new ItemLore(List.of(Component.translatable("item.fixture.wand.hint").withStyle(ChatFormatting.GRAY)))));
    item("rod", new Item.Properties().component(DataComponents.LORE,
        new ItemLore(List.of(Component.literal("First"), Component.literal("Second")))));
    item("staff", new Item.Properties().component(DataComponents.LORE, new ItemLore(List.of(
        Component.translatable("item.fixture.staff.uses", 3).withStyle(ChatFormatting.AQUA, ChatFormatting.BOLD),
        Component.literal("x").append(Component.literal("y").withStyle(ChatFormatting.RED))))));
    item("blank", new Item.Properties().component(DataComponents.LORE, new ItemLore(List.of())));
    // `component(type, null)` removes the component (`DataComponentMap.Builder.set`): no lore.
    item("removed", new Item.Properties()
        .component(DataComponents.LORE, new ItemLore(List.of(Component.literal("gone"))))
        .component(DataComponents.LORE, null));
    // No lore set: no component patch at all.
    item("plain", new Item.Properties());
  }
}
"#;

fn java25() -> BTreeMap<&'static str, Json> {
    JAVA25
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter_map(|(case, json)| Some((case, serde_json::from_str(json).ok()?)))
        .collect()
}

/// Each registered item's declaration file, by path.
fn item_files(host: &StdHost) -> BTreeMap<String, Json> {
    declarations(host.registrations())
        .iter()
        .filter(|declaration| !declaration.is_block())
        .map(|declaration| (declaration.path().to_owned(), declaration.file_json()))
        .collect()
}

#[test]
fn an_items_lore_is_its_declarations_component_patch_as_the_game_encodes_it() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", DEMO));
    let fixture = javac_fixture::Fixture::build("item_lore", &sources);
    let entry = fixture.jar.discover_entrypoint("main").unwrap();
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture
        .jar
        .execute(&entry.class, entry.method.as_deref(), "main", &mut host)
        .unwrap();
    let files = item_files(&host);
    let expected = java25();
    for case in ["wand", "rod", "staff", "blank"] {
        assert_eq!(
            files[case]["components"],
            serde_json::json!({ "minecraft:lore": expected[case] }),
            "{case}: {}",
            files[case]
        );
    }
    assert_eq!(files["wand"]["max_stack_size"], 1);
    for case in ["removed", "plain"] {
        assert!(
            files[case].get("components").is_none(),
            "{case}: {}",
            files[case]
        );
    }
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

/// The real Runes jar: each rune pouch's declaration carries the lore its `Item.Properties` set
/// (`RunePouches.entry`: `item.runes.rune_pouch.hint` in gray), and the mods the runtime loaded
/// from the jar are Runes and its nested Bundle API (`fabric.mod.json` `jars`), the set the bake
/// makes its packages from.
#[test]
fn runes_pouches_declare_their_lore_and_the_jar_loads_its_nested_bundle_api() {
    let Some(jar) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes test skipped");
        return;
    };
    // Unoptimised test builds need more than the default test thread's stack while Runes
    // initialises its nested Bundle API classes (as `fixture_bundle_api`'s Runes test).
    std::thread::Builder::new()
        .name("runes-item-lore".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let jar =
                NativeJar::from_path(std::path::Path::new(&jar)).expect("load real Runes jar");
            // `seed_host` links the nested jars `fabric.mod.json` `jars` selects, as the probe does.
            let mut host = jar.seed_host(StdHost::new().with_game_version("26.3"));
            for entry in jar.discover_entrypoints("main") {
                jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
                    .expect("Runes main entrypoint");
            }
            assert_eq!(
                host.unknown_host_call_count(),
                0,
                "{:?}",
                host.unknown_host_calls()
            );
            let files = item_files(&host);
            let lore = serde_json::json!({
                "minecraft:lore": [{"translate": "item.runes.rune_pouch.hint", "color": "gray"}]
            });
            for pouch in ["small_rune_pouch", "medium_rune_pouch", "large_rune_pouch"] {
                assert_eq!(
                    files[pouch]["components"], lore,
                    "{pouch}: {}",
                    files[pouch]
                );
                assert_eq!(files[pouch]["max_stack_size"], 1, "{pouch}");
            }
            assert_eq!(files["large_rune_pouch"]["rarity"], "uncommon");
            // The runes themselves set no component.
            assert!(files["fire_stone"].get("components").is_none());
            assert_eq!(
                host.loaded_jar_mods(jar.fabric_id.as_deref()),
                ["runes", "bundleapi"]
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
