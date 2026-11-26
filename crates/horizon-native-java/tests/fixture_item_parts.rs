//! A jar item's own overrides on the item-part façades (`item_parts.rs`), against Java 25.
//!
//! `fixtures/item_parts/demo/` is a tally item with a data component of its own (`demo:tally`,
//! a record with a `RecordCodecBuilder` codec) and its own `overrideOtherStackedOnMe`,
//! `overrideStackedOnOther`, bar methods and `use`, reading the `ItemStack`, `Slot` and `Player`
//! surfaces an item part answers. `java25.txt` is `oracle/ItemPartsOracle.java` run on Java
//! 25.0.5+8 against the Minecraft 26.3 server jar (see the fixture's README); this test runs the
//! same script on the stub-compiled demo through `NativeJar`'s item-part calls and prints the
//! same lines.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{
    BlockUsePlayerData, ExecutionBudget, GAME_PROTOTYPE_COMPONENTS, ItemClick, ItemClickDirection,
    ItemDestroyed, ItemHook, ItemUse, LiveMenuEffect, MenuPlayerFacts, NativeJar, Nbt, NbtCompound,
    PlayerContainer, RuntimeComponentValue, StdHost, Value, level_sound::SoundRecord,
};

const UUID: u128 = 0x0000_7a11_0000_0000_0000_0000_0000_0001;
const TALLY: &str = "demo:tally";
const TALLY_ITEM: &str = "demo:tally_item";
const OWNER: &str = "demo/TallyItem";
const JAVA25: &str = include_str!("fixtures/item_parts/java25.txt");

const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api; public interface ModInitializer { void onInitialize(); }",
    ),
    (
        "com/mojang/datafixers/kinds/App.java",
        "package com.mojang.datafixers.kinds; public interface App<F, A> {}",
    ),
    (
        "com/mojang/datafixers/kinds/Applicative.java",
        "package com.mojang.datafixers.kinds; public interface Applicative<F, Mu> {}",
    ),
    (
        "com/mojang/datafixers/Products.java",
        "package com.mojang.datafixers;\n\
         import com.mojang.datafixers.kinds.*; import java.util.function.*;\n\
         public interface Products {\n\
           final class P2<F, T1, T2> {\n\
             public <R> App<F, R> apply(Applicative<F, ?> a, BiFunction<T1, T2, R> f) { throw new AssertionError(); }\n\
           }\n\
         }",
    ),
    (
        "com/mojang/serialization/MapCodec.java",
        "package com.mojang.serialization;\n\
         import com.mojang.serialization.codecs.RecordCodecBuilder; import java.util.function.Function;\n\
         public abstract class MapCodec<A> {\n\
           public <O> RecordCodecBuilder<O, A> forGetter(Function<O, A> getter) { throw new AssertionError(); }\n\
         }",
    ),
    (
        "com/mojang/serialization/Codec.java",
        "package com.mojang.serialization;\n\
         import com.mojang.serialization.codecs.PrimitiveCodec;\n\
         public interface Codec<A> {\n\
           PrimitiveCodec<Integer> INT = null;\n\
           PrimitiveCodec<String> STRING = null;\n\
           MapCodec<A> fieldOf(String name);\n\
           MapCodec<A> optionalFieldOf(String name, A defaultValue);\n\
         }",
    ),
    (
        "com/mojang/serialization/codecs/PrimitiveCodec.java",
        "package com.mojang.serialization.codecs;\n\
         import com.mojang.serialization.Codec;\n\
         public interface PrimitiveCodec<A> extends Codec<A> {}",
    ),
    (
        "com/mojang/serialization/codecs/RecordCodecBuilder.java",
        "package com.mojang.serialization.codecs;\n\
         import com.mojang.datafixers.Products; import com.mojang.datafixers.kinds.*;\n\
         import com.mojang.serialization.Codec; import java.util.function.Function;\n\
         public final class RecordCodecBuilder<O, F> implements App<RecordCodecBuilder.Mu<O>, F> {\n\
           public static final class Mu<O> {}\n\
           public static <O> Codec<O> create(Function<Instance<O>, ? extends App<Mu<O>, O>> builder) { throw new AssertionError(); }\n\
           public static final class Instance<O> implements Applicative<Mu<O>, Instance.Mu<O>> {\n\
             public static final class Mu<O> {}\n\
             public <T1, T2> Products.P2<RecordCodecBuilder.Mu<O>, T1, T2> group(App<RecordCodecBuilder.Mu<O>, T1> t1, App<RecordCodecBuilder.Mu<O>, T2> t2) { throw new AssertionError(); }\n\
           }\n\
         }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources;\n\
         public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources;\n\
         public final class ResourceKey<T> { public static <T> ResourceKey<T> create(ResourceKey<? extends net.minecraft.core.Registry<T>> r, Identifier id) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core;\n\
         import net.minecraft.resources.Identifier;\n\
         public interface Registry<T> { static <V, T extends V> T register(Registry<V> r, Identifier id, T value) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        "package net.minecraft.core; public interface DefaultedRegistry<T> extends Registry<T> {}",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries;\n\
         import net.minecraft.core.*; import net.minecraft.core.component.DataComponentType; import net.minecraft.world.item.Item;\n\
         public final class BuiltInRegistries {\n\
           public static final Registry<DataComponentType<?>> DATA_COMPONENT_TYPE = null;\n\
           public static final DefaultedRegistry<Item> ITEM = null;\n\
         }",
    ),
    (
        "net/minecraft/core/registries/Registries.java",
        "package net.minecraft.core.registries;\n\
         import net.minecraft.core.Registry; import net.minecraft.resources.ResourceKey; import net.minecraft.world.item.Item;\n\
         public final class Registries { public static final ResourceKey<Registry<Item>> ITEM = null; }",
    ),
    (
        "net/minecraft/core/component/DataComponentType.java",
        "package net.minecraft.core.component;\n\
         import com.mojang.serialization.Codec;\n\
         public interface DataComponentType<T> {\n\
           static <T> Builder<T> builder() { throw new AssertionError(); }\n\
           class Builder<T> { public Builder<T> persistent(Codec<T> c) { return this; } public DataComponentType<T> build() { return null; } }\n\
         }",
    ),
    (
        "net/minecraft/sounds/SoundEvent.java",
        "package net.minecraft.sounds; public final class SoundEvent {}",
    ),
    (
        "net/minecraft/sounds/SoundEvents.java",
        "package net.minecraft.sounds; public class SoundEvents { public static final SoundEvent BUNDLE_INSERT = null; }",
    ),
    (
        "net/minecraft/stats/Stat.java",
        "package net.minecraft.stats; public class Stat<T> {}",
    ),
    (
        "net/minecraft/stats/StatType.java",
        "package net.minecraft.stats; public class StatType<T> { public Stat<T> get(T value) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/stats/Stats.java",
        "package net.minecraft.stats;\n\
         import net.minecraft.world.item.Item;\n\
         public class Stats { public static final StatType<Item> ITEM_USED = null; }",
    ),
    (
        "net/minecraft/util/Prediction.java",
        "package net.minecraft.util; public enum Prediction { PREDICTED, SERVER_ONLY }",
    ),
    (
        "net/minecraft/world/InteractionHand.java",
        "package net.minecraft.world; public enum InteractionHand { MAIN_HAND, OFF_HAND }",
    ),
    (
        "net/minecraft/world/InteractionResult.java",
        "package net.minecraft.world;\n\
         public interface InteractionResult {\n\
           Success SUCCESS = null; Success SUCCESS_SERVER = null; Fail FAIL = null;\n\
           final class Success implements InteractionResult {}\n\
           final class Fail implements InteractionResult {}\n\
         }",
    ),
    (
        "net/minecraft/world/level/Level.java",
        "package net.minecraft.world.level; public abstract class Level { public boolean isClientSide() { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/world/entity/item/ItemEntity.java",
        "package net.minecraft.world.entity.item;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class ItemEntity { public ItemStack getItem() { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/world/item/ItemUtils.java",
        "package net.minecraft.world.item;\n\
         import java.util.stream.Stream; import net.minecraft.world.entity.item.ItemEntity;\n\
         public class ItemUtils { public static void onContainerDestroyed(ItemEntity e, Stream<ItemStack> s) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/world/entity/SlotAccess.java",
        "package net.minecraft.world.entity;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public interface SlotAccess { ItemStack get(); boolean set(ItemStack stack); }",
    ),
    (
        "net/minecraft/world/entity/player/Player.java",
        "package net.minecraft.world.entity.player;\n\
         import net.minecraft.sounds.SoundEvent; import net.minecraft.stats.Stat; import net.minecraft.util.Prediction;\n\
         import net.minecraft.world.InteractionHand; import net.minecraft.world.entity.item.ItemEntity; import net.minecraft.world.item.ItemStack;\n\
         public abstract class Player {\n\
           public void playSound(SoundEvent s, float v, float p) { throw new AssertionError(); }\n\
           public void awardStat(Stat<?> s) { throw new AssertionError(); }\n\
           public ItemEntity drop(ItemStack s, boolean retain, Prediction p) { throw new AssertionError(); }\n\
           public ItemStack getItemInHand(InteractionHand h) { throw new AssertionError(); }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ClickAction.java",
        "package net.minecraft.world.inventory; public enum ClickAction { PRIMARY, SECONDARY }",
    ),
    (
        "net/minecraft/world/inventory/Slot.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Player; import net.minecraft.world.item.ItemStack;\n\
         public class Slot {\n\
           public ItemStack getItem() { throw new AssertionError(); }\n\
           public boolean allowModification(Player p) { throw new AssertionError(); }\n\
           public boolean mayPickup(Player p) { throw new AssertionError(); }\n\
           public int getMaxStackSize(ItemStack s) { throw new AssertionError(); }\n\
           public ItemStack safeTake(int count, int limit, Player p) { throw new AssertionError(); }\n\
         }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item;\n\
         import net.minecraft.core.component.DataComponentType;\n\
         public final class ItemStack {\n\
           public static final ItemStack EMPTY = null;\n\
           public static boolean isSameItemSameComponents(ItemStack a, ItemStack b) { throw new AssertionError(); }\n\
           public boolean isEmpty() { throw new AssertionError(); }\n\
           public int getCount() { throw new AssertionError(); }\n\
           public Item getItem() { throw new AssertionError(); }\n\
           public ItemStack split(int n) { throw new AssertionError(); }\n\
           public ItemStack copyWithCount(int n) { throw new AssertionError(); }\n\
           public <T> T get(DataComponentType<? extends T> t) { throw new AssertionError(); }\n\
           public <T> T getOrDefault(DataComponentType<? extends T> t, T d) { throw new AssertionError(); }\n\
           public boolean has(DataComponentType<?> t) { throw new AssertionError(); }\n\
           public <T> T set(DataComponentType<T> t, T v) { throw new AssertionError(); }\n\
           public <T> T remove(DataComponentType<? extends T> t) { throw new AssertionError(); }\n\
         }",
    ),
    (
        "net/minecraft/world/item/Item.java",
        "package net.minecraft.world.item;\n\
         import net.minecraft.resources.ResourceKey; import net.minecraft.world.*; import net.minecraft.world.entity.SlotAccess;\n\
         import net.minecraft.world.entity.player.Player; import net.minecraft.world.inventory.*; import net.minecraft.world.level.Level;\n\
         public class Item {\n\
           public static class Properties {\n\
             public Properties setId(ResourceKey<Item> key) { return this; }\n\
             public Properties stacksTo(int n) { return this; }\n\
           }\n\
           public Item(Properties p) {}\n\
           public boolean overrideOtherStackedOnMe(ItemStack me, ItemStack other, Slot s, ClickAction a, Player p, SlotAccess c) { return false; }\n\
           public boolean overrideStackedOnOther(ItemStack me, Slot s, ClickAction a, Player p) { return false; }\n\
           public boolean isBarVisible(ItemStack s) { return false; }\n\
           public int getBarWidth(ItemStack s) { return 0; }\n\
           public int getBarColor(ItemStack s) { return 0; }\n\
           public InteractionResult use(Level l, Player p, InteractionHand h) { return null; }\n\
           public void onDestroyed(net.minecraft.world.entity.item.ItemEntity e) {}\n\
         }",
    ),
];

fn fixture() -> javac_fixture::Fixture {
    let mut sources = STUBS.to_vec();
    sources.push((
        "demo/Tally.java",
        include_str!("fixtures/item_parts/demo/Tally.java"),
    ));
    sources.push((
        "demo/Probe.java",
        include_str!("fixtures/item_parts/demo/Probe.java"),
    ));
    sources.push((
        "demo/TallyItem.java",
        include_str!("fixtures/item_parts/demo/TallyItem.java"),
    ));
    javac_fixture::Fixture::build("item_parts", &sources)
}

fn player(main_hand: Option<Value>) -> BlockUsePlayerData {
    BlockUsePlayerData {
        uuid: UUID,
        name: "Tallier".to_owned(),
        entity_id: Some(3),
        incarnation: Some((1, 0)),
        container: PlayerContainer::Inventory,
        sneaking: false,
        spectator: false,
        main_hand,
        off_hand: None,
    }
}

fn facts(inventory: &[Value]) -> MenuPlayerFacts {
    MenuPlayerFacts {
        inventory: Some(inventory.to_vec()),
        selected_slot: 0,
        instabuild: Some(false),
        position: Some([1.5, 64.0, -2.5]),
        ..MenuPlayerFacts::default()
    }
}

/// `ItemStack.toString()`: `count + " " + item`.
fn stack_text(count: i32, item: &str) -> String {
    if count <= 0 {
        "0 minecraft:air".to_owned()
    } else {
        format!("{count} {item}")
    }
}

/// A tag as `ItemPartsOracle.tally` prints it: `TagType.getName()`, then `Tag.toString()`.
fn tag_text(tag: &Nbt) -> String {
    match tag {
        Nbt::Int(n) => format!("INT:{n}"),
        Nbt::String(s) => format!("STRING:\"{s}\""),
        other => panic!("unexpected tag {other:?}"),
    }
}

fn tally_text(patch: &[(String, Option<RuntimeComponentValue>)]) -> String {
    match patch.iter().find(|(id, _)| id == TALLY) {
        Some((_, Some(RuntimeComponentValue::Nbt(Nbt::Compound(fields))))) => {
            let mut out = "tally={".to_owned();
            for (key, value) in fields {
                out.push_str(&format!(" {key}={}", tag_text(value)));
            }
            out + " }"
        }
        None | Some((_, None)) => "tally=none".to_owned(),
        Some(other) => panic!("unexpected tally entry {other:?}"),
    }
}

fn stack_line(jar: &NativeJar, host: &mut StdHost, stack: &Value) -> String {
    let parts = jar
        .item_stack_components(host, stack)
        .expect("encode")
        .expect("modelled stack");
    format!(
        "{} {}",
        stack_text(parts.count, &parts.item_id),
        tally_text(&parts.patch)
    )
}

/// `Float.toString` for the plain values the script plays.
fn java_float(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// `Double.toString` for the plain values the script uses.
fn java_double(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

fn sound_lines(host: &mut StdHost, out: &mut Vec<String>) {
    for record in host.take_sound_records() {
        let SoundRecord::Play(effect) = record else {
            panic!("a seed drawn without a sound: {record:?}");
        };
        out.push(format!(
            "sound {} {} {} {} except={}",
            effect.sound_id,
            format!("{:?}", effect.source).to_uppercase(),
            java_float(effect.volume),
            java_float(effect.pitch),
            effect.except_player == Some(UUID)
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn click(
    jar: &NativeJar,
    host: &mut StdHost,
    item: &Value,
    name: &str,
    direction: ItemClickDirection,
    slot: i32,
    carried: &Value,
    primary: bool,
    inventory: &mut Vec<Value>,
    out: &mut Vec<String>,
) -> Value {
    let outcome = jar
        .item_click_override(
            host,
            &player(None),
            "minecraft:overworld",
            &facts(inventory),
            item,
            OWNER,
            &ItemClick {
                direction,
                inventory_slot: slot,
                carried: carried.clone(),
                primary,
            },
            &ExecutionBudget::UNBOUNDED,
        )
        .expect("click");
    assert!(outcome.report.exact(), "{name}: {:?}", outcome.report);
    assert!(outcome.effects.is_empty(), "{:?}", outcome.effects);
    sound_lines(host, out);
    let slot_text = stack_line(jar, host, &outcome.slot);
    let carried_text = stack_line(jar, host, &outcome.carried);
    out.push(format!(
        "click {name} handled={} slot={slot_text} carried={carried_text} takes={}",
        outcome.handled, outcome.takes
    ));
    *inventory = outcome.inventory.clone().expect("inventory");
    outcome.carried
}

fn run_script() -> Vec<String> {
    let fixture = fixture();
    let jar = &fixture.jar;
    let mut host = StdHost::new();
    jar.execute("demo.Probe", None, "main", &mut host)
        .expect("demo onInitialize");
    host.server_player_created(UUID, 1, 0);
    host.set_host_deferred_server_sound_seeds(true);
    let item = host
        .registrations()
        .iter()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == TALLY_ITEM)
        .expect("the tally item is registered")
        .value
        .clone();
    let overrides = jar.item_overrides(&item, &host);
    for hook in [
        ItemHook::StackedOnOther,
        ItemHook::OtherStackedOnMe,
        ItemHook::Use,
        ItemHook::BarVisible,
        ItemHook::BarWidth,
        ItemHook::BarColor,
        ItemHook::Destroyed,
    ] {
        assert_eq!(overrides.owners.get(&hook).map(String::as_str), Some(OWNER));
    }
    assert!(!overrides.overrides(ItemHook::TooltipImage));
    let tally = tally_stack(jar, &mut host, &item, None);

    let mut out = Vec::new();
    let mut inventory = vec![StdHost::empty_item_stack(); 36];
    inventory[0] = tally;
    inventory[1] = host.make_item_stack_for_id("minecraft:diamond", 5, false);
    let stone = host.make_item_stack_for_id("minecraft:stone", 10, false);
    let carried = click(
        jar,
        &mut host,
        &item,
        "primary",
        ItemClickDirection::OtherStackedOnMe,
        0,
        &stone,
        true,
        &mut inventory,
        &mut out,
    );
    click(
        jar,
        &mut host,
        &item,
        "insert",
        ItemClickDirection::OtherStackedOnMe,
        0,
        &carried,
        false,
        &mut inventory,
        &mut out,
    );

    let tally = std::mem::replace(&mut inventory[0], StdHost::empty_item_stack());
    let tally = click(
        jar,
        &mut host,
        &item,
        "take",
        ItemClickDirection::CarriedOnOther,
        1,
        &tally,
        false,
        &mut inventory,
        &mut out,
    );

    let bar = jar
        .item_bar(
            &mut host,
            &item,
            &overrides,
            &tally,
            false,
            &ExecutionBudget::UNBOUNDED,
        )
        .expect("bar");
    assert!(bar.report.exact(), "{:?}", bar.report);
    out.push(format!(
        "bar visible={} width={} color={}",
        bar.visible.expect("visible"),
        bar.width.expect("width"),
        bar.color.expect("color")
    ));

    let used = jar
        .item_use(
            &mut host,
            &player(Some(tally.clone())),
            "minecraft:overworld",
            &facts(&inventory),
            &item,
            OWNER,
            &ItemUse {
                hand: "MAIN_HAND".to_owned(),
            },
            &ExecutionBudget::UNBOUNDED,
        )
        .expect("use");
    assert!(used.report.exact(), "{:?}", used.report);
    for effect in &used.effects {
        match effect {
            LiveMenuEffect::AwardStat {
                stat_type,
                value,
                amount,
            } => out.push(format!("stat {stat_type} {value} {amount}")),
            LiveMenuEffect::Drop {
                stack,
                retain_ownership,
                prediction,
            } => out.push(format!(
                "drop {} {} retain={retain_ownership} {}",
                stack_text(stack.count, &stack.item_id),
                tally_text(&stack.patch),
                format!("{prediction:?}").to_uppercase()
            )),
            other => panic!("unexpected effect {other:?}"),
        }
    }
    let result = match used.result {
        Some(horizon_native_java::InteractionResultValue::Success { swing_source, .. }) => {
            format!("SUCCESS_{swing_source:?}")
        }
        other => format!("{other:?}"),
    };
    let held = stack_line(jar, &mut host, used.held.as_ref().expect("held"));
    out.push(format!(
        "use result={} held={held}",
        if result == "SUCCESS_ServerOnly" {
            "SUCCESS_SERVER"
        } else {
            &result
        }
    ));

    // An item entity destroyed while holding a full tally, as the game stores it.
    let doomed = tally_stack(
        jar,
        &mut host,
        &item,
        Some(Nbt::Compound(NbtCompound::from([
            ("count".to_owned(), Nbt::Int(4)),
            ("last".to_owned(), Nbt::String("minecraft:stone".to_owned())),
        ]))),
    );
    let position = [4.5, 70.0, 8.5];
    let destroyed = jar
        .item_destroyed(
            &mut host,
            &item,
            OWNER,
            &ItemDestroyed {
                dimension: "minecraft:overworld".to_owned(),
                stack: doomed.clone(),
                position,
            },
            &ExecutionBudget::UNBOUNDED,
        )
        .expect("destroyed");
    assert!(destroyed.report.exact(), "{:?}", destroyed.report);
    for drop in &destroyed.drops {
        // `ItemUtils.onContainerDestroyed` adds each at the entity's position, where the
        // embedder spawns the drops it is handed.
        out.push(format!(
            "spawn {} at {} {} {}",
            stack_line(jar, &mut host, drop),
            java_double(position[0]),
            java_double(position[1]),
            java_double(position[2])
        ));
    }
    out.push(format!(
        "destroyed held={}",
        stack_line(jar, &mut host, &doomed)
    ));
    out
}

/// A tally item stack with `tally` in its patch (`None`: none), the item's prototype holding
/// no tally and none of the vanilla container components, as the game's does.
fn tally_stack(jar: &NativeJar, host: &mut StdHost, item: &Value, tally: Option<Nbt>) -> Value {
    let mut defaults: Vec<(String, Option<Nbt>)> = GAME_PROTOTYPE_COMPONENTS
        .iter()
        .map(|id| ((*id).to_owned(), None))
        .collect();
    defaults.push((TALLY.to_owned(), None));
    let patch = tally
        .map(|nbt| vec![(TALLY.to_owned(), Some(RuntimeComponentValue::Nbt(nbt)))])
        .unwrap_or_default();
    jar.make_item_stack(
        host,
        Some(item.clone()),
        TALLY_ITEM,
        1,
        patch,
        defaults,
        false,
        Vec::new(),
    )
    .expect("tally stack")
}

#[test]
fn tally_item_overrides_match_java25() {
    let lines = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(run_script)
        .expect("spawn")
        .join()
        .unwrap();
    let expected: Vec<&str> = JAVA25.lines().collect();
    for (index, (got, want)) in lines.iter().zip(&expected).enumerate() {
        assert_eq!(got, want, "line {}", index + 1);
    }
    assert_eq!(lines.len(), expected.len(), "{lines:#?}");
}
