//! Live Java menus (`live_menu.rs`): a javac-built two-input combiner
//! (`fixtures/live_menu/demo/*.java`) opened by a block use, then driven step by step through
//! `NativeJar::click_player_menu` / `tick_player_menu` / `close_player_menu_live`, the way an
//! embedder delivers its player's clicks.
//!
//! Every line is compared with `fixtures/live_menu/java25.txt`, captured on Java 25 from the
//! same demo sources and the same `script.txt`, run against the real 26.3 server jar's
//! `AbstractContainerMenu` / `ItemCombinerMenu` / `Slot` / `SimpleContainer` / `Inventory` /
//! `ItemStack` (`oracle/LiveMenuOracle.java`). The mod's own prints come from the interpreter's
//! stdout; the effect lines from the typed `LiveMenuEffect` stream.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use std::collections::BTreeMap;

use horizon_native_java::{
    BlockHitData, BlockUseContext, BlockUsePlayerData, BlockUseSide, ContainerInput,
    ExecutionBudget, LiveMenuEffect, MenuClick, MenuEffect, MenuPlayerFacts, PlayerContainer,
    StackComponents, StdHost, Value,
};

const UUID: u128 = 0x5eed_0000_0000_0000_0000_0000_0000_0001;
const POS: [i32; 3] = [3, 64, 5];

const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api; public interface ModInitializer { void onInitialize(); }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources;\n\
         public final class Identifier {\n\
           public static Identifier fromNamespaceAndPath(String ns, String p) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core;\n\
         import net.minecraft.resources.Identifier;\n\
         public interface Registry<T> {\n\
           static <V, T extends V> T register(Registry<V> reg, Identifier id, T val) { return val; }\n\
         }",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.level.block.Block;\n\
         public final class BuiltInRegistries {\n\
           public static final Registry<Block> BLOCK = null;\n\
           public static final Registry<MenuType<?>> MENU = null;\n\
         }",
    ),
    (
        "net/minecraft/core/BlockPos.java",
        "package net.minecraft.core; public class BlockPos {}",
    ),
    (
        "net/minecraft/world/phys/BlockHitResult.java",
        "package net.minecraft.world.phys; public class BlockHitResult {}",
    ),
    (
        "net/minecraft/world/InteractionResult.java",
        "package net.minecraft.world;\n\
         public interface InteractionResult {\n\
           InteractionResult SUCCESS = null;\n\
           InteractionResult CONSUME = null;\n\
           InteractionResult PASS = null;\n\
         }",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat;\n\
         public interface Component { static MutableComponent translatable(String key) { return null; } }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat; public class MutableComponent implements Component {}",
    ),
    (
        "net/minecraft/world/flag/FeatureFlagSet.java",
        "package net.minecraft.world.flag; public class FeatureFlagSet {}",
    ),
    (
        "net/minecraft/world/flag/FeatureFlags.java",
        "package net.minecraft.world.flag;\n\
         public final class FeatureFlags { public static final FeatureFlagSet VANILLA_SET = null; }",
    ),
    (
        "net/minecraft/world/level/ItemLike.java",
        "package net.minecraft.world.level; public interface ItemLike {}",
    ),
    (
        "net/minecraft/world/item/Item.java",
        "package net.minecraft.world.item; public class Item implements net.minecraft.world.level.ItemLike {}",
    ),
    (
        "net/minecraft/world/item/Items.java",
        "package net.minecraft.world.item;\n\
         public final class Items {\n\
           public static final Item COBBLESTONE = null;\n\
           public static final Item AMETHYST_SHARD = null;\n\
           public static final Item STONE_BRICKS = null;\n\
           public static final Item DIRT = null;\n\
         }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.ItemLike;\n\
         public final class ItemStack {\n\
           public static final ItemStack EMPTY = null;\n\
           public ItemStack(ItemLike item, int count) {}\n\
           public boolean isEmpty() { return true; }\n\
           public int getCount() { return 0; }\n\
           public void shrink(int amount) {}\n\
           public boolean is(Item item) { return false; }\n\
           public void onCraftedBy(Player player, int amount) {}\n\
         }",
    ),
    (
        "net/minecraft/world/Container.java",
        "package net.minecraft.world;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public interface Container {\n\
           ItemStack getItem(int slot);\n\
           void setItem(int slot, ItemStack stack);\n\
         }",
    ),
    (
        "net/minecraft/world/entity/player/Player.java",
        "package net.minecraft.world.entity.player;\n\
         import java.util.OptionalInt;\n\
         import net.minecraft.world.MenuProvider;\n\
         public abstract class Player {\n\
           public int tickCount;\n\
           public final double getX() { return 0; }\n\
           public final double getY() { return 0; }\n\
           public final double getZ() { return 0; }\n\
           public OptionalInt openMenu(MenuProvider provider) { return OptionalInt.empty(); }\n\
         }",
    ),
    (
        "net/minecraft/world/entity/player/Inventory.java",
        "package net.minecraft.world.entity.player; public class Inventory {}",
    ),
    (
        "net/minecraft/world/level/Level.java",
        "package net.minecraft.world.level;\n\
         public abstract class Level { public boolean isClientSide() { return false; } }",
    ),
    (
        "net/minecraft/world/inventory/Slot.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class Slot {\n\
           public final Container container = null;\n\
           public ItemStack getItem() { return null; }\n\
           public void onTake(Player player, ItemStack stack) {}\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ResultContainer.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class ResultContainer implements Container {\n\
           public ItemStack getItem(int slot) { return null; }\n\
           public void setItem(int slot, ItemStack stack) {}\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/AbstractContainerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public abstract class AbstractContainerMenu {\n\
           protected AbstractContainerMenu(MenuType<?> type, int id) {}\n\
           public void removed(Player player) {}\n\
           public Slot getSlot(int index) { return null; }\n\
           public ItemStack getCarried() { return null; }\n\
           protected DataSlot addDataSlot(DataSlot slot) { return slot; }\n\
           public boolean canTakeItemForPickAll(ItemStack stack, Slot slot) { return true; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/DataSlot.java",
        "package net.minecraft.world.inventory;\n\
         public abstract class DataSlot {\n\
           public static DataSlot standalone() { return null; }\n\
           public abstract int get();\n\
           public abstract void set(int value);\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ContainerLevelAccess.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.level.Level;\n\
         public interface ContainerLevelAccess {\n\
           ContainerLevelAccess NULL = null;\n\
           static ContainerLevelAccess create(Level level, BlockPos pos) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition.java",
        "package net.minecraft.world.inventory;\n\
         import java.util.function.Predicate;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class ItemCombinerMenuSlotDefinition {\n\
           public static Builder create() { return null; }\n\
           public static class Builder {\n\
             public Builder withSlot(int i, int x, int y, Predicate<ItemStack> p) { return this; }\n\
             public Builder withResultSlot(int i, int x, int y) { return this; }\n\
             public ItemCombinerMenuSlotDefinition build() { return null; }\n\
           }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ItemCombinerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         public abstract class ItemCombinerMenu extends AbstractContainerMenu {\n\
           protected final Container inputSlots = null;\n\
           protected final ResultContainer resultSlots = null;\n\
           public ItemCombinerMenu(MenuType<?> type, int id, Inventory inv, ContainerLevelAccess access, ItemCombinerMenuSlotDefinition def) {\n\
             super(type, id);\n\
           }\n\
           protected abstract void onTake(Player player, ItemStack stack);\n\
           public ItemStack quickMoveStack(Player player, int index) { return null; }\n\
           protected abstract boolean isValidBlock(BlockState state);\n\
           public abstract void createResult();\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/MenuType.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.flag.FeatureFlagSet;\n\
         public class MenuType<T extends AbstractContainerMenu> {\n\
           public interface MenuSupplier<T extends AbstractContainerMenu> { T create(int id, Inventory inv); }\n\
           public MenuType(MenuSupplier<T> supplier, FeatureFlagSet features) {}\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/MenuConstructor.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         public interface MenuConstructor {\n\
           AbstractContainerMenu createMenu(int id, Inventory inv, Player player);\n\
         }",
    ),
    (
        "net/minecraft/world/MenuProvider.java",
        "package net.minecraft.world;\n\
         import net.minecraft.world.inventory.MenuConstructor;\n\
         public interface MenuProvider extends MenuConstructor {}",
    ),
    (
        "net/minecraft/world/SimpleMenuProvider.java",
        "package net.minecraft.world;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuConstructor;\n\
         public final class SimpleMenuProvider implements MenuProvider {\n\
           public SimpleMenuProvider(MenuConstructor ctor, Component title) {}\n\
           public AbstractContainerMenu createMenu(int id, Inventory inv, Player p) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/state/BlockBehaviour.java",
        "package net.minecraft.world.level.block.state;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public abstract class BlockBehaviour {\n\
           public static class Properties { public static Properties of() { return new Properties(); } }\n\
           protected BlockBehaviour(Properties props) {}\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             return InteractionResult.PASS;\n\
           }\n\
           public static abstract class BlockStateBase {}\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/state/BlockState.java",
        "package net.minecraft.world.level.block.state;\n\
         public class BlockState extends BlockBehaviour.BlockStateBase {\n\
           public boolean is(Object block) { return false; }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/Block.java",
        "package net.minecraft.world.level.block;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Block extends BlockBehaviour { public Block(Properties props) { super(props); } }",
    ),
];

fn sources() -> Vec<(&'static str, &'static str)> {
    let mut sources = STUBS.to_vec();
    sources.push((
        "demo/Probe.java",
        include_str!("fixtures/live_menu/demo/Probe.java"),
    ));
    sources.push((
        "demo/Bench.java",
        include_str!("fixtures/live_menu/demo/Bench.java"),
    ));
    sources.push((
        "demo/BenchMenu.java",
        include_str!("fixtures/live_menu/demo/BenchMenu.java"),
    ));
    sources.push((
        "demo/Types.java",
        include_str!("fixtures/live_menu/types/Types.java"),
    ));
    sources
}

/// `ItemStack.toString()`: `count + " " + item`, `0 minecraft:air` for an empty stack.
fn components_text(stack: &StackComponents) -> String {
    if stack.count <= 0 || stack.item_id == "minecraft:air" {
        "0 minecraft:air".to_owned()
    } else {
        format!("{} {}", stack.count, stack.item_id)
    }
}

fn stack_text(host: &StdHost, stack: &Value) -> String {
    components_text(
        &host
            .item_stack_components(stack)
            .unwrap_or_else(|| panic!("unmodelled stack {stack:?}")),
    )
}

/// The oracle's `effect:` lines (`LiveMenuOracle`'s recorders) for Decaf's effects.
fn effect_lines(effects: &[LiveMenuEffect], out: &mut Vec<String>) {
    for effect in effects {
        let line = match effect {
            LiveMenuEffect::InitialData {
                container_id,
                state_id,
                items,
                carried,
            } => {
                let mut line = format!(
                    "initial {container_id} state={} [",
                    state_id.map_or("?".to_owned(), |id| id.to_string())
                );
                for (index, item) in items.iter().enumerate() {
                    if item.count > 0 && item.item_id != "minecraft:air" {
                        line.push_str(&format!(" {index}:{}", components_text(item)));
                    }
                }
                format!("{line} ] carried={}", components_text(carried))
            }
            LiveMenuEffect::SlotChanged {
                container_id,
                slot,
                stack,
                ..
            } => format!("slot {container_id} {slot} {}", components_text(stack)),
            LiveMenuEffect::CarriedChanged { stack } => {
                format!("carried {}", components_text(stack))
            }
            LiveMenuEffect::DataChanged {
                container_id,
                index,
                value,
            } => format!("data {container_id} {index}={value}"),
            LiveMenuEffect::InventoryChangedCriterion { stack } => {
                format!("criterion {}", components_text(stack))
            }
            LiveMenuEffect::Drop {
                stack,
                retain_ownership,
                prediction,
            } => format!(
                "drop {} retain={retain_ownership} {}",
                components_text(stack),
                match prediction {
                    horizon_native_java::Prediction::Predicted => "PREDICTED",
                    horizon_native_java::Prediction::ServerOnly => "SERVER_ONLY",
                }
            ),
            LiveMenuEffect::AwardStat {
                stat_type,
                value,
                amount,
            } => format!("stat {stat_type} {value} {amount}"),
            LiveMenuEffect::RecipeCrafted { recipe, .. } => format!("recipe_crafted {recipe}"),
            LiveMenuEffect::AwardRecipes { recipes } => format!("award_recipes {recipes:?}"),
            LiveMenuEffect::InventorySlotSent { slot, stack } => {
                format!("inventory_sent {slot} {}", components_text(stack))
            }
            LiveMenuEffect::Closed { container_id } => format!("closed {container_id}"),
            // A notice for the embedder's own take hook; the oracle sends no packet for it.
            LiveMenuEffect::SlotTaken { .. } => continue,
        };
        out.push(format!("effect: {line}"));
    }
}

fn state_lines(host: &StdHost, carried: &Value, slots: &[Value], out: &mut Vec<String>) {
    out.push(format!("carried={}", stack_text(host, carried)));
    for (index, slot) in slots.iter().enumerate() {
        if host.item_stack_is_empty(slot) == Some(false) {
            out.push(format!("slot {index}={}", stack_text(host, slot)));
        }
    }
}

fn inventory_line(host: &StdHost, inventory: &[Value], out: &mut Vec<String>) {
    let mut line = "inventory:".to_owned();
    for (index, stack) in inventory.iter().enumerate() {
        if host.item_stack_is_empty(stack) == Some(false) {
            line.push_str(&format!(" {index}={}", stack_text(host, stack)));
        }
    }
    out.push(line);
}

fn player() -> BlockUsePlayerData {
    BlockUsePlayerData {
        uuid: UUID,
        name: "Benchwright".to_owned(),
        entity_id: Some(7),
        incarnation: Some((1, 0)),
        container: PlayerContainer::Inventory,
        sneaking: false,
        spectator: false,
        main_hand: None,
        off_hand: None,
    }
}

/// The player's facts: `LiveMenuOracle`'s fake `ServerPlayer` at (3.5, 65, z) with eye height
/// `1.62f` and a 4.5 block interaction range, facing the bench at `POS`.
fn facts(inventory: Vec<Value>, z: f64, creative: bool) -> MenuPlayerFacts {
    MenuPlayerFacts {
        inventory: Some(inventory),
        selected_slot: 0,
        instabuild: Some(creative),
        eye_position: Some([3.5, 65.0 + f64::from(1.62f32), z]),
        block_interaction_range: Some(4.5),
        access_block: Some(("demo:bench".to_owned(), BTreeMap::new())),
        ..MenuPlayerFacts::default()
    }
}

fn input(name: &str) -> ContainerInput {
    match name {
        "PICKUP" => ContainerInput::Pickup,
        "QUICK_MOVE" => ContainerInput::QuickMove,
        "SWAP" => ContainerInput::Swap,
        "CLONE" => ContainerInput::Clone,
        "THROW" => ContainerInput::Throw,
        "QUICK_CRAFT" => ContainerInput::QuickCraft,
        "PICKUP_ALL" => ContainerInput::PickupAll,
        other => panic!("unknown input {other}"),
    }
}

fn run_script() -> Vec<String> {
    let fixture = javac_fixture::Fixture::build("live_menu", &sources());
    let mut host = StdHost::new();
    fixture
        .jar
        .execute("demo.Probe", None, "main", &mut host)
        .unwrap();
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
    host.server_player_created(UUID, 1, 0);

    let empty = StdHost::empty_item_stack();
    let mut inventory = vec![empty; 43];
    inventory[0] = host.make_item_stack_for_id("minecraft:cobblestone", 10, false);
    inventory[1] = host.make_item_stack_for_id("minecraft:amethyst_shard", 3, false);
    inventory[2] = host.make_item_stack_for_id("minecraft:dirt", 5, false);
    let mut z = 7.5;
    let mut creative = false;
    let mut window = 0;
    let budget = ExecutionBudget::UNBOUNDED;
    let mut out = Vec::new();

    for raw in include_str!("fixtures/live_menu/script.txt").lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.push(format!("== {line}"));
        let parts: Vec<&str> = line.split(' ').collect();
        let before = host.unknown_host_call_count();
        let state: Option<(Value, Vec<Value>)>;
        match parts[0] {
            "open" => {
                host.set_player_menu_facts(UUID, Some(facts(inventory.clone(), z, creative)));
                let context = BlockUseContext {
                    side: BlockUseSide::Server,
                    dimension: "minecraft:overworld".to_owned(),
                    block_id: "demo:bench".to_owned(),
                    state_properties: BTreeMap::new(),
                    pos: POS,
                    hit: BlockHitData::block(POS, "UP", [3.5, 65.0, 5.5], false),
                    player: player(),
                    hand: "MAIN_HAND".to_owned(),
                    stack: None,
                };
                let outcome = fixture
                    .jar
                    .block_use_without_item(&mut host, &context)
                    .unwrap();
                assert_eq!(outcome.failure, None);
                let opened = outcome
                    .open_menus()
                    .next()
                    .expect("the bench opened its menu");
                window = opened.container_id;
                out.extend(host.take_stdout().lines().map(str::to_owned));
                effect_lines(&outcome.live_effects, &mut out);
                inventory = outcome
                    .inventory
                    .clone()
                    .expect("the facts gave the inventory");
                state = host.held_menu_contents(UUID);
            }
            "click" | "mismatch" => {
                let click = MenuClick {
                    container_id: window,
                    slot: parts[1].parse().unwrap(),
                    button: parts[2].parse().unwrap(),
                    input: input(parts[3]),
                    state_id_matches: parts[0] == "click",
                };
                let mut player = player();
                player.container = PlayerContainer::Open(window);
                let outcome = fixture
                    .jar
                    .click_player_menu(
                        &mut host,
                        &player,
                        "minecraft:overworld",
                        &facts(inventory.clone(), z, creative),
                        &click,
                        &budget,
                    )
                    .unwrap()
                    .expect("the menu is held");
                assert_eq!(outcome.failure, None, "{line}");
                assert!(outcome.handled, "{line}");
                out.extend(host.take_stdout().lines().map(str::to_owned));
                effect_lines(&outcome.effects, &mut out);
                inventory = outcome.inventory.clone().unwrap();
                state = outcome.carried.clone().zip(outcome.slots.clone());
            }
            "tick" | "close" => {
                let mut player = player();
                player.container = PlayerContainer::Open(window);
                let facts = facts(inventory.clone(), z, creative);
                let outcome = if parts[0] == "tick" {
                    fixture.jar.tick_player_menu(
                        &mut host,
                        &player,
                        "minecraft:overworld",
                        &facts,
                        window,
                        &budget,
                    )
                } else {
                    fixture.jar.close_player_menu_live(
                        &mut host,
                        &player,
                        "minecraft:overworld",
                        &facts,
                        window,
                        &budget,
                    )
                }
                .unwrap()
                .expect("the menu is held");
                assert_eq!(outcome.failure, None, "{line}");
                out.extend(host.take_stdout().lines().map(str::to_owned));
                effect_lines(&outcome.effects, &mut out);
                inventory = outcome.inventory.clone().unwrap();
                state = outcome.carried.clone().zip(outcome.slots.clone());
            }
            "far" => {
                z = 15.5;
                state = host.held_menu_contents(UUID);
            }
            "creative" => {
                creative = parts[1] == "true";
                state = host.held_menu_contents(UUID);
            }
            other => panic!("unknown step {other}"),
        }
        assert_eq!(
            host.unknown_host_call_count(),
            before,
            "{line} counted {:?}",
            host.unknown_host_calls()
        );
        if let Some((carried, slots)) = state {
            state_lines(&host, &carried, &slots, &mut out);
        }
        inventory_line(&host, &inventory, &mut out);
    }
    out
}

#[test]
fn live_combiner_menu_matches_java25() {
    let lines = std::thread::Builder::new()
        .name("live-menu".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run_script)
        .unwrap()
        .join()
        .unwrap();
    let expected: Vec<&str> = include_str!("fixtures/live_menu/java25.txt")
        .lines()
        .collect();
    for (index, (got, want)) in lines.iter().zip(&expected).enumerate() {
        assert_eq!(
            got,
            want,
            "line {} differs; Decaf so far:\n{}",
            index + 1,
            lines[..=index].join("\n")
        );
    }
    assert_eq!(lines.len(), expected.len(), "Decaf:\n{}", lines.join("\n"));
}

/// A drag over both combiner inputs: Java places it in `quickcraftSlots`' `HashSet` order,
/// which follows identity hashes no one can reproduce, and each placement runs the mod's
/// `createResult` (which sees the other input before or after its own). Decaf counts the click
/// instead of picking an order. A drag over plain inventory slots (the script above) is exact.
#[test]
fn drag_over_both_inputs_counts_the_unknowable_order() {
    std::thread::Builder::new()
        .name("live-menu-drag".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let fixture = javac_fixture::Fixture::build("live_menu_drag", &sources());
            let mut host = StdHost::new();
            fixture
                .jar
                .execute("demo.Probe", None, "main", &mut host)
                .unwrap();
            host.server_player_created(UUID, 1, 0);
            let mut inventory = vec![StdHost::empty_item_stack(); 43];
            inventory[0] = host.make_item_stack_for_id("minecraft:cobblestone", 10, false);
            host.set_player_menu_facts(UUID, Some(facts(inventory.clone(), 7.5, false)));
            let context = BlockUseContext {
                side: BlockUseSide::Server,
                dimension: "minecraft:overworld".to_owned(),
                block_id: "demo:bench".to_owned(),
                state_properties: BTreeMap::new(),
                pos: POS,
                hit: BlockHitData::block(POS, "UP", [3.5, 65.0, 5.5], false),
                player: player(),
                hand: "MAIN_HAND".to_owned(),
                stack: None,
            };
            let opened = fixture
                .jar
                .block_use_without_item(&mut host, &context)
                .unwrap();
            let mut inventory = opened.inventory.unwrap();
            let mut player = player();
            player.container = PlayerContainer::Open(1);
            let steps: &[(i32, i32, ContainerInput)] = &[
                (30, 0, ContainerInput::Pickup),
                (-999, 0, ContainerInput::QuickCraft),
                (0, 1, ContainerInput::QuickCraft),
                (1, 1, ContainerInput::QuickCraft),
                (-999, 2, ContainerInput::QuickCraft),
            ];
            let mut last = None;
            for (slot, button, input) in steps {
                let outcome = fixture
                    .jar
                    .click_player_menu(
                        &mut host,
                        &player,
                        "minecraft:overworld",
                        &facts(inventory.clone(), 7.5, false),
                        &MenuClick {
                            container_id: 1,
                            slot: *slot,
                            button: *button,
                            input: *input,
                            state_id_matches: true,
                        },
                        &ExecutionBudget::UNBOUNDED,
                    )
                    .unwrap()
                    .unwrap();
                inventory = outcome.inventory.clone().unwrap();
                last = Some(outcome);
            }
            let end = last.unwrap();
            assert_eq!(
                end.unknown_host_calls,
                BTreeMap::from([(
                    "net/minecraft/server/network/ServerGamePacketListenerImpl#handleContainerClick(Lnet/minecraft/network/protocol/game/ServerboundContainerClickPacket;)V"
                        .to_owned(),
                    1
                )]),
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

// ---------------------------------------------------------------------------
// The Sunrise bridge's runtime seams (`mod/src/live_menu.rs`)
// ---------------------------------------------------------------------------

/// Blocks and menus beside the bench for the bridge's seams: a bench whose menu prints the
/// player's `tickCount` and position as it is taken from, one whose provider builds no menu,
/// one whose menu overrides `removed` to read the cursor first, and a `MenuType` whose supplier
/// builds nothing.
const BRIDGE_SOURCES: &[(&str, &str)] = &[
    (
        "demo/BridgeProbe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public final class BridgeProbe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"nothing\"), new MenuType<BenchMenu>((id, inventory) -> null, FeatureFlags.VANILLA_SET));\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"data\"), new MenuType<DataMenu>(DataMenu::new, FeatureFlags.VANILLA_SET));\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"branchy\"), new MenuType<BranchMenu>(BranchMenu::new, FeatureFlags.VANILLA_SET));\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"late_slot\"), new MenuType<LateSlotMenu>(LateSlotMenu::new, FeatureFlags.VANILLA_SET));\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"static_def\"), StaticDefMenu.TYPE);\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"take_bench\"), new MenuBench(BlockBehaviour.Properties.of(), 4));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"tick_bench\"), new MenuBench(BlockBehaviour.Properties.of(), 0));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"null_bench\"), new MenuBench(BlockBehaviour.Properties.of(), 1));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"removed_bench\"), new MenuBench(BlockBehaviour.Properties.of(), 2));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"data_bench\"), new MenuBench(BlockBehaviour.Properties.of(), 3));\n\
           }\n\
         }",
    ),
    (
        "demo/MenuBench.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class MenuBench extends Block {\n\
           private final int kind;\n\
           public MenuBench(Properties properties, int kind) { super(properties); this.kind = kind; }\n\
           @Override protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (level.isClientSide()) return InteractionResult.SUCCESS;\n\
             int kind = this.kind;\n\
             player.openMenu(new SimpleMenuProvider((id, inventory, p) -> kind == 0 ? new TickMenu(id, inventory, ContainerLevelAccess.create(level, pos)) : kind == 1 ? null : kind == 2 ? new RemovedMenu(id, inventory, ContainerLevelAccess.create(level, pos)) : kind == 3 ? new DataMenu(id, inventory, ContainerLevelAccess.create(level, pos)) : new TakeMenu(id, inventory, ContainerLevelAccess.create(level, pos)), Component.translatable(\"container.demo.bench\")));\n\
             return InteractionResult.CONSUME;\n\
           }\n\
         }",
    ),
    (
        "demo/DataMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.inventory.DataSlot;\n\
         public class DataMenu extends BenchMenu {\n\
           public DataMenu(int id, Inventory inventory) { this(id, inventory, ContainerLevelAccess.NULL); }\n\
           public DataMenu(int id, Inventory inventory, ContainerLevelAccess access) {\n\
             super(id, inventory, access);\n\
             addDataSlot(DataSlot.standalone()).set(7);\n\
           }\n\
         }",
    ),
    (
        "demo/BranchMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.inventory.DataSlot;\n\
         public class BranchMenu extends BenchMenu {\n\
           public BranchMenu(int id, Inventory inventory) {\n\
             super(id, inventory, ContainerLevelAccess.NULL);\n\
             if (id > 0) addDataSlot(DataSlot.standalone());\n\
           }\n\
         }",
    ),
    (
        "demo/LateSlotMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         public class LateSlotMenu extends BenchMenu {\n\
           public LateSlotMenu(int id, Inventory inventory) {\n\
             super(id, inventory, ContainerLevelAccess.NULL);\n\
             extra();\n\
           }\n\
           private void extra() {}\n\
         }",
    ),
    (
        "demo/TakeMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.inventory.Slot;\n\
         import net.minecraft.world.item.ItemStack;\n\
         /** Its own shift-click: takes the slot's stack through `slot.onTake` and moves nothing. */\n\
         public class TakeMenu extends BenchMenu {\n\
           public TakeMenu(int id, Inventory inventory, ContainerLevelAccess access) { super(id, inventory, access); }\n\
           @Override public ItemStack quickMoveStack(Player player, int index) {\n\
             Slot slot = getSlot(index);\n\
             ItemStack stack = slot.getItem();\n\
             slot.onTake(player, stack);\n\
             return ItemStack.EMPTY;\n\
           }\n\
         }",
    ),
    (
        "demo/StaticDefMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.inventory.ItemCombinerMenu;\n\
         import net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         /** Its slot definition is a static field a helper returns: static state could set it. */\n\
         public class StaticDefMenu extends ItemCombinerMenu {\n\
           public static final MenuType<StaticDefMenu> TYPE = new MenuType<>(StaticDefMenu::new, FeatureFlags.VANILLA_SET);\n\
           static final ItemCombinerMenuSlotDefinition DEF = ItemCombinerMenuSlotDefinition.create().withSlot(0, 27, 47, stack -> true).withResultSlot(1, 134, 47).build();\n\
           public StaticDefMenu(int id, Inventory inventory) { super(TYPE, id, inventory, ContainerLevelAccess.NULL, def()); }\n\
           static ItemCombinerMenuSlotDefinition def() { return DEF; }\n\
           @Override protected boolean isValidBlock(BlockState state) { return true; }\n\
           @Override public void createResult() {}\n\
           @Override protected void onTake(Player player, ItemStack stack) {}\n\
         }",
    ),
    (
        "demo/TickMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class TickMenu extends BenchMenu {\n\
           public TickMenu(int id, Inventory inventory, ContainerLevelAccess access) { super(id, inventory, access); }\n\
           @Override protected void onTake(Player player, ItemStack stack) {\n\
             System.out.println(\"mod: tick \" + player.tickCount + \" at \" + player.getX() + \",\" + player.getY() + \",\" + player.getZ());\n\
             super.onTake(player, stack);\n\
           }\n\
         }",
    ),
    (
        "demo/RemovedMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         public class RemovedMenu extends BenchMenu {\n\
           public RemovedMenu(int id, Inventory inventory, ContainerLevelAccess access) { super(id, inventory, access); }\n\
           @Override public void removed(Player player) {\n\
             System.out.println(\"mod: removed with \" + getCarried());\n\
             super.removed(player);\n\
           }\n\
         }",
    ),
];

/// The bench jar with the bridge's blocks, both entrypoints run, the player logged in.
fn bridge_host(name: &str) -> (javac_fixture::Fixture, StdHost) {
    let mut sources = sources();
    sources.extend_from_slice(BRIDGE_SOURCES);
    let fixture = javac_fixture::Fixture::build(name, &sources);
    let mut host = StdHost::new();
    for entry in ["demo.Probe", "demo.BridgeProbe"] {
        fixture.jar.execute(entry, None, "main", &mut host).unwrap();
    }
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
    host.server_player_created(UUID, 1, 0);
    (fixture, host)
}

fn use_block(
    fixture: &javac_fixture::Fixture,
    host: &mut StdHost,
    block: &str,
    facts: MenuPlayerFacts,
) -> horizon_native_java::BlockUseOutcome {
    host.set_player_menu_facts(UUID, Some(facts));
    let outcome = fixture
        .jar
        .block_use_without_item(
            host,
            &BlockUseContext {
                side: BlockUseSide::Server,
                dimension: "minecraft:overworld".to_owned(),
                block_id: block.to_owned(),
                state_properties: BTreeMap::new(),
                pos: POS,
                hit: BlockHitData::block(POS, "UP", [3.5, 65.0, 5.5], false),
                player: player(),
                hand: "MAIN_HAND".to_owned(),
                stack: None,
            },
        )
        .unwrap();
    assert_eq!(outcome.failure, None);
    outcome
}

fn click_menu(
    fixture: &javac_fixture::Fixture,
    host: &mut StdHost,
    window: i32,
    facts: &MenuPlayerFacts,
    slot: i32,
    button: i32,
    input: ContainerInput,
) -> horizon_native_java::LiveMenuOutcome {
    let mut player = player();
    player.container = PlayerContainer::Open(window);
    let outcome = fixture
        .jar
        .click_player_menu(
            host,
            &player,
            "minecraft:overworld",
            facts,
            &MenuClick {
                container_id: window,
                slot,
                button,
                input,
                state_id_matches: true,
            },
            &ExecutionBudget::UNBOUNDED,
        )
        .unwrap()
        .expect("the menu is held");
    assert_eq!(outcome.failure, None);
    outcome
}

fn close_menu(
    fixture: &javac_fixture::Fixture,
    host: &mut StdHost,
    window: i32,
    facts: &MenuPlayerFacts,
) -> horizon_native_java::LiveMenuOutcome {
    let mut player = player();
    player.container = PlayerContainer::Inventory;
    fixture
        .jar
        .close_player_menu_live(
            host,
            &player,
            "minecraft:overworld",
            facts,
            window,
            &ExecutionBudget::UNBOUNDED,
        )
        .unwrap()
        .expect("the menu is held")
}

fn on_big_stack(name: &str, run: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name(name.into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}

fn held(host: &StdHost, inventory: &[Value]) -> Vec<String> {
    let mut held: Vec<String> = inventory
        .iter()
        .filter(|stack| host.item_stack_is_empty(stack) == Some(false))
        .map(|stack| stack_text(host, stack))
        .collect();
    held.sort();
    held
}

/// `NativeJar::menu_type_declarations` declares the bench's `MenuType` as the layout Java 25
/// builds (`fixtures/live_menu/layout_java25.txt`, `oracle/MenuLayoutOracle.java` against the
/// real 26.3 server jar): every container slot an owned slot holding `Slot.getMaxStackSize()`,
/// the combiner's input slots (`ItemCombinerMenu$2`) its inputs, and the player's slots, in
/// `addStandardInventorySlots` order, the standard player group. A supplier that builds nothing
/// gets a reason instead, and building the declarations counts nothing.
#[test]
fn menu_type_declaration_is_the_java25_layout() {
    on_big_stack("live-menu-declaration", || {
        let (fixture, mut host) = bridge_host("live_menu_declaration");
        let before = host.unknown_host_call_count();
        let declarations = fixture.jar.menu_type_declarations(&mut host);
        assert_eq!(host.unknown_host_call_count(), before);

        // The declaration the Java layout gives.
        let mut owned = Vec::new();
        let mut inputs = Vec::new();
        let mut player_slots = Vec::new();
        let mut data_slots = 0;
        for line in include_str!("fixtures/live_menu/layout_java25.txt").lines() {
            let parts: Vec<&str> = line.split(' ').collect();
            match parts[0] {
                "slot" => {
                    let index: usize = parts[1].parse().unwrap();
                    let container_slot: i32 = parts[4].parse().unwrap();
                    let max: i64 = parts[5].parse().unwrap();
                    if parts[3] == "player" {
                        player_slots.push(container_slot);
                        continue;
                    }
                    assert!(player_slots.is_empty(), "container slots come first");
                    if parts[2] == "net.minecraft.world.inventory.ItemCombinerMenu$2" {
                        inputs.push(index);
                    }
                    owned.push(max);
                }
                "data_slots" => data_slots = parts[1].parse::<usize>().unwrap(),
                other => panic!("unknown oracle line {other}"),
            }
        }
        assert_eq!(player_slots, (9..36).chain(0..9).collect::<Vec<i32>>());
        assert_eq!(data_slots, 0);
        let expected = serde_json::json!({
            "slots": owned.len(),
            "owned_slots": owned,
            "inputs": inputs,
            "player_inventory": true,
        });

        let found = |id: &str| {
            declarations
                .iter()
                .find(|declaration| declaration.id == id)
                .unwrap_or_else(|| panic!("no declaration for {id}: {declarations:?}"))
                .layout
                .clone()
                .map(|layout| layout.declaration())
        };
        assert_eq!(found("demo:bench"), Ok(expected.clone()));
        // One straight-line `addDataSlot` after the combiner's constructor: one data slot.
        let mut with_data = expected;
        with_data["data_slots"] = 1.into();
        assert_eq!(found("demo:data"), Ok(with_data));
        // Refused from the bytecode, before anything runs: a supplier that builds nothing
        // isn't a constructor reference, a branch could change the layout, and so could a call
        // into the menu's own code after its slots are laid out.
        assert_eq!(
            found("demo:nothing"),
            Err("its MenuSupplier lambda does not construct exactly one menu".to_owned())
        );
        // The reviewer's probe: a helper returning a static field passes no builder chain.
        assert_eq!(
            found("demo:static_def"),
            Err("demo/StaticDefMenu.def is not one slot-definition builder chain".to_owned())
        );
        assert_eq!(
            found("demo:branchy"),
            Err("demo/BranchMenu's constructor branches or throws".to_owned())
        );
        assert_eq!(
            found("demo:late_slot"),
            Err(
                "demo/LateSlotMenu's constructor calls demo/LateSlotMenu.extra after its slots are laid out"
                    .to_owned()
            )
        );
    });
}

/// The player's `tickCount` and position come from the embedder's facts: a menu's `onTake`
/// reading them (as Runes' `RuneCrafter` cooldown does) gets them, and nothing counts; without
/// the facts each read counts.
#[test]
fn facts_answer_the_live_players_tick_count_and_position() {
    on_big_stack("live-menu-tick", || {
        for known in [true, false] {
            let (fixture, mut host) = bridge_host("live_menu_tick");
            let mut inventory = vec![StdHost::empty_item_stack(); 43];
            inventory[0] = host.make_item_stack_for_id("minecraft:cobblestone", 10, false);
            inventory[1] = host.make_item_stack_for_id("minecraft:amethyst_shard", 3, false);
            let facts = |inventory: &[Value]| MenuPlayerFacts {
                tick_count: known.then_some(1234),
                position: known.then_some([3.5, 65.0, 7.5]),
                ..facts(inventory.to_vec(), 7.5, false)
            };
            let opened = use_block(&fixture, &mut host, "demo:tick_bench", facts(&inventory));
            let window = opened.open_menus().next().unwrap().container_id;
            inventory = opened.inventory.unwrap();
            for (slot, button, input) in [
                (30, 0, ContainerInput::Pickup),
                (0, 1, ContainerInput::Pickup),
                (30, 0, ContainerInput::Pickup),
                (31, 0, ContainerInput::QuickMove),
            ] {
                let outcome = click_menu(
                    &fixture,
                    &mut host,
                    window,
                    &facts(&inventory),
                    slot,
                    button,
                    input,
                );
                assert_eq!(outcome.unknown_host_call_count, 0);
                inventory = outcome.inventory.unwrap();
            }
            host.take_stdout();
            let taken = click_menu(
                &fixture,
                &mut host,
                window,
                &facts(&inventory),
                2,
                0,
                ContainerInput::Pickup,
            );
            let stdout = host.take_stdout();
            if known {
                assert!(
                    stdout.contains("mod: tick 1234 at 3.5,65.0,7.5"),
                    "{stdout}"
                );
                assert_eq!(
                    taken.unknown_host_call_count, 0,
                    "{:?}",
                    taken.unknown_host_calls
                );
            } else {
                assert_eq!(
                    taken.unknown_host_calls.keys().cloned().collect::<Vec<_>>(),
                    [
                        "net/minecraft/server/network/ServerGamePacketListenerImpl#handleContainerClick(Lnet/minecraft/network/protocol/game/ServerboundContainerClickPacket;)V",
                        "net/minecraft/world/entity/player/Player#getX()D",
                        "net/minecraft/world/entity/player/Player#getY()D",
                        "net/minecraft/world/entity/player/Player#getZ()D",
                        "net/minecraft/world/entity/player/Player#tickCountI",
                    ]
                );
            }
        }
    });
}

/// `ServerPlayer.openMenu` advances `containerCounter` before it asks the provider
/// (`nextContainerCounter`, offsets 23-24), so a `createMenu` that answers `null` still moves it
/// (offsets 43-73): the use reports `CounterAdvanced`, and the next menu takes the id after it.
#[test]
fn a_null_create_menu_still_advances_the_container_counter() {
    on_big_stack("live-menu-counter", || {
        let (fixture, mut host) = bridge_host("live_menu_counter");
        let inventory = vec![StdHost::empty_item_stack(); 43];
        // The embedder's counter: 41 windows opened before.
        host.server_player_container_opened(UUID, Some(41));
        let none = use_block(
            &fixture,
            &mut host,
            "demo:null_bench",
            facts(inventory.clone(), 7.5, false),
        );
        assert!(matches!(
            none.menu_effects.as_slice(),
            [MenuEffect::CounterAdvanced { container_id: 42 }]
        ));
        assert_eq!(none.unknown_host_call_count, 0);
        assert!(host.player_open_menu(UUID).is_none());
        let bench = use_block(
            &fixture,
            &mut host,
            "demo:bench",
            facts(inventory, 7.5, false),
        );
        assert_eq!(bench.open_menus().next().unwrap().container_id, 43);
    });
}

/// A close the embedder already put the cursor back for (Sunrise's window close does, before
/// `menu_closed`): the call starts from its empty cursor (`MenuPlayerFacts::carried`), so the
/// menu's `removed` hands back only the inputs, and the cursor's stack isn't handed back twice.
/// A jar `removed` override could read the cursor before `super.removed`, where Java's would
/// still see the stack: that close counts. A player removed for a reason other than a dimension
/// change (killed) or disconnected gets the inputs dropped (`dropOrPlaceInInventory` offsets
/// 16-59: `drop(stack, false, SERVER_ONLY)`); a dimension change places them back.
#[test]
fn a_close_starts_from_the_embedders_cursor() {
    #[derive(Clone, Copy, PartialEq)]
    enum Removal {
        Live,
        ChangedDimension,
        Killed,
        Disconnected,
    }
    on_big_stack("live-menu-close", || {
        for (block, removal) in [
            ("demo:bench", Removal::Live),
            ("demo:removed_bench", Removal::Live),
            ("demo:bench", Removal::ChangedDimension),
            ("demo:bench", Removal::Killed),
            ("demo:bench", Removal::Disconnected),
        ] {
            let (fixture, mut host) = bridge_host("live_menu_close");
            let mut inventory = vec![StdHost::empty_item_stack(); 43];
            inventory[0] = host.make_item_stack_for_id("minecraft:cobblestone", 10, false);
            let opened = use_block(
                &fixture,
                &mut host,
                block,
                facts(inventory.clone(), 7.5, false),
            );
            let window = opened.open_menus().next().unwrap().container_id;
            inventory = opened.inventory.unwrap();
            // Pick up the ten, put one in the base: nine on the cursor.
            for (slot, button) in [(30, 0), (0, 1)] {
                let outcome = click_menu(
                    &fixture,
                    &mut host,
                    window,
                    &facts(inventory.clone(), 7.5, false),
                    slot,
                    button,
                    ContainerInput::Pickup,
                );
                inventory = outcome.inventory.unwrap();
            }
            // The embedder closed the window and put the nine back into slot 0 (or dropped
            // them, for a player who is gone; either way not on the cursor any more).
            inventory[0] = host.make_item_stack_for_id("minecraft:cobblestone", 9, false);
            host.take_stdout();
            let closed = close_menu(
                &fixture,
                &mut host,
                window,
                &MenuPlayerFacts {
                    carried: Some(StdHost::empty_item_stack()),
                    removed: matches!(
                        removal,
                        Removal::ChangedDimension | Removal::Killed | Removal::Disconnected
                    ),
                    removed_for_dimension_change: removal == Removal::ChangedDimension,
                    disconnected: removal == Removal::Disconnected,
                    ..facts(inventory.clone(), 7.5, false)
                },
            );
            assert_eq!(closed.failure, None);
            assert!(host.player_open_menu(UUID).is_none());
            let drops: Vec<String> = closed
                .effects
                .iter()
                .filter_map(|effect| match effect {
                    LiveMenuEffect::Drop {
                        stack,
                        retain_ownership,
                        prediction,
                    } => Some(format!(
                        "{} retain={retain_ownership} {prediction:?}",
                        components_text(stack)
                    )),
                    _ => None,
                })
                .collect();
            let held = held(&host, &closed.inventory.clone().unwrap());
            if block == "demo:removed_bench" {
                assert!(
                    host.take_stdout()
                        .contains("mod: removed with 0 minecraft:air")
                );
                assert_eq!(
                    closed
                        .unknown_host_calls
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>(),
                    ["net/minecraft/server/level/ServerPlayer#doCloseContainer()V"]
                );
            } else {
                assert_eq!(
                    closed.unknown_host_call_count, 0,
                    "{:?}",
                    closed.unknown_host_calls
                );
            }
            match removal {
                Removal::Live | Removal::ChangedDimension => {
                    assert!(drops.is_empty(), "{drops:?}");
                    assert_eq!(held, ["10 minecraft:cobblestone"]);
                }
                Removal::Killed | Removal::Disconnected => {
                    assert_eq!(drops, ["1 minecraft:cobblestone retain=false ServerOnly"]);
                    assert_eq!(held, ["9 minecraft:cobblestone"]);
                }
            }
        }
    });
}

/// A menu with a data slot opens with its initial value sent (`ServerPlayer$1.sendInitialData`,
/// one `DataChanged` per data slot after the contents), which is what an embedder opens its
/// window with, and the held menu's layout is the one its type declares.
#[test]
fn a_data_slot_menu_opens_with_its_initial_value_and_its_declared_layout() {
    on_big_stack("live-menu-data", || {
        let (fixture, mut host) = bridge_host("live_menu_data");
        let inventory = vec![StdHost::empty_item_stack(); 43];
        let opened = use_block(
            &fixture,
            &mut host,
            "demo:data_bench",
            facts(inventory, 7.5, false),
        );
        assert_eq!(
            opened.unknown_host_call_count, 0,
            "{:?}",
            opened.unknown_host_calls
        );
        let window = opened.open_menus().next().unwrap().container_id;
        let after_initial: Vec<&LiveMenuEffect> = opened
            .live_effects
            .iter()
            .skip_while(|effect| !matches!(effect, LiveMenuEffect::InitialData { .. }))
            .skip(1)
            .collect();
        assert_eq!(
            after_initial.first(),
            Some(&&LiveMenuEffect::DataChanged {
                container_id: window,
                index: 0,
                value: 7
            })
        );
        let declared = fixture
            .jar
            .menu_type_declarations(&mut host.clone())
            .into_iter()
            .find(|declaration| declaration.id == "demo:data")
            .unwrap()
            .layout
            .unwrap();
        assert_eq!(host.held_menu_layout(UUID), Ok(declared));
    });
}

/// `Slot.onTake` is told once per call, wherever it is called from: a vanilla body's take (a
/// pickup from the result) and a jar method's `slot.onTake(...)` on a vanilla slot (a menu's own
/// `quickMoveStack`, as many combiners write it) each give one `SlotTaken`.
#[test]
fn every_slot_on_take_is_told_once() {
    on_big_stack("live-menu-takes", || {
        for (block, slot, button, input) in [
            ("demo:bench", 2, 0, ContainerInput::Pickup),
            ("demo:take_bench", 2, 0, ContainerInput::QuickMove),
        ] {
            let (fixture, mut host) = bridge_host("live_menu_takes");
            let mut inventory = vec![StdHost::empty_item_stack(); 43];
            inventory[0] = host.make_item_stack_for_id("minecraft:cobblestone", 10, false);
            inventory[1] = host.make_item_stack_for_id("minecraft:amethyst_shard", 3, false);
            let opened = use_block(
                &fixture,
                &mut host,
                block,
                facts(inventory.clone(), 7.5, false),
            );
            let window = opened.open_menus().next().unwrap().container_id;
            inventory = opened.inventory.unwrap();
            for (slot, button) in [(30, 0), (0, 1), (30, 0), (31, 0)] {
                let input = if slot == 31 {
                    ContainerInput::QuickMove
                } else {
                    ContainerInput::Pickup
                };
                let outcome = click_menu(
                    &fixture,
                    &mut host,
                    window,
                    &facts(inventory.clone(), 7.5, false),
                    slot,
                    button,
                    input,
                );
                inventory = outcome.inventory.unwrap();
            }
            host.take_stdout();
            let taken = click_menu(
                &fixture,
                &mut host,
                window,
                &facts(inventory.clone(), 7.5, false),
                slot,
                button,
                input,
            );
            assert!(
                host.take_stdout().contains("mod: onTake"),
                "{block}: the menu's onTake ran"
            );
            let takes: Vec<&LiveMenuEffect> = taken
                .effects
                .iter()
                .filter(|effect| matches!(effect, LiveMenuEffect::SlotTaken { .. }))
                .collect();
            assert_eq!(
                takes,
                [&LiveMenuEffect::SlotTaken { slot: Some(2) }],
                "{block}"
            );
        }
    });
}
