//! Integration tests for generic block-use (`Block.useItemOn`, `Block.useWithoutItem`,
//! `Block.getMenuProvider`), the MC 26.3 sealed `InteractionResult` hierarchy,
//! `BlockHitResult`, `SimpleMenuProvider`, `ContainerLevelAccess.create`, and
//! `ServerPlayer.openMenu(MenuProvider)` vs client-side `Player.openMenu`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use std::collections::BTreeMap;

use horizon_native_java::{
    BlockHitData, BlockUseContext, BlockUseOutcome, BlockUsePlayerData, BlockUseSide,
    ExecutionBudget, Host, InteractionItemContext, InteractionResultValue, MenuEffect,
    PlayerContainer, StdHost, SwingSource, Value,
};
use serde_json::json;

const MC_STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api; public interface ModInitializer { void onInitialize(); }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources;\n\
         public final class Identifier {\n\
           public static Identifier parse(String s) { return null; }\n\
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
        "net/minecraft/core/Direction.java",
        "package net.minecraft.core;\n\
         public enum Direction { DOWN, UP, NORTH, SOUTH, WEST, EAST }",
    ),
    (
        "net/minecraft/core/Vec3i.java",
        "package net.minecraft.core;\n\
         public class Vec3i {\n\
           public int getX() { return 0; }\n\
           public int getY() { return 0; }\n\
           public int getZ() { return 0; }\n\
         }",
    ),
    (
        "net/minecraft/core/BlockPos.java",
        "package net.minecraft.core;\n\
         public class BlockPos extends Vec3i {\n\
           public static final BlockPos ZERO = null;\n\
           public BlockPos(int x, int y, int z) {}\n\
         }",
    ),
    (
        "net/minecraft/world/phys/Vec3.java",
        "package net.minecraft.world.phys;\n\
         public class Vec3 {\n\
           public final double x = 0, y = 0, z = 0;\n\
           public Vec3(double x, double y, double z) {}\n\
           public double x() { return x; }\n\
           public double y() { return y; }\n\
           public double z() { return z; }\n\
         }",
    ),
    (
        "net/minecraft/world/phys/HitResult.java",
        "package net.minecraft.world.phys;\n\
         public abstract class HitResult {\n\
           public enum Type { MISS, BLOCK, ENTITY }\n\
           public Vec3 getLocation() { return null; }\n\
           public abstract Type getType();\n\
         }",
    ),
    (
        "net/minecraft/world/phys/BlockHitResult.java",
        "package net.minecraft.world.phys;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.core.Direction;\n\
         public class BlockHitResult extends HitResult {\n\
           public static BlockHitResult miss(Vec3 loc, Direction dir, BlockPos pos) { return null; }\n\
           public BlockHitResult(Vec3 loc, Direction dir, BlockPos pos, boolean inside) {}\n\
           public BlockHitResult(Vec3 loc, Direction dir, BlockPos pos, boolean inside, boolean border) {}\n\
           public BlockHitResult withDirection(Direction dir) { return null; }\n\
           public BlockHitResult withPosition(BlockPos pos) { return null; }\n\
           public BlockHitResult hitBorder() { return null; }\n\
           public BlockPos getBlockPos() { return null; }\n\
           public Direction getDirection() { return null; }\n\
           public Type getType() { return Type.BLOCK; }\n\
           public boolean isInside() { return false; }\n\
           public boolean isWorldBorderHit() { return false; }\n\
         }",
    ),
    (
        "net/minecraft/world/InteractionHand.java",
        "package net.minecraft.world;\n\
         public enum InteractionHand { MAIN_HAND, OFF_HAND }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item;\n\
         public final class ItemStack {\n\
           public static final ItemStack EMPTY = null;\n\
           public boolean isEmpty() { return true; }\n\
         }",
    ),
    (
        "net/minecraft/world/InteractionResult.java",
        "package net.minecraft.world;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public sealed interface InteractionResult\n\
             permits InteractionResult.Success, InteractionResult.Fail,\n\
                     InteractionResult.Pass, InteractionResult.TryEmptyHandInteraction {\n\
           Success SUCCESS = null;\n\
           Success SUCCESS_SERVER = null;\n\
           Success CONSUME = null;\n\
           Fail FAIL = null;\n\
           Pass PASS = null;\n\
           TryEmptyHandInteraction TRY_WITH_EMPTY_HAND = null;\n\
           default boolean consumesAction() { return false; }\n\
           enum SwingSource { NONE, PREDICTED, SERVER_ONLY }\n\
           record ItemContext(boolean wasItemInteraction, ItemStack heldItemTransformedTo) {\n\
             public static final ItemContext NONE = null;\n\
             public static final ItemContext DEFAULT = null;\n\
           }\n\
           record Success(SwingSource swingSource, ItemContext itemContext) implements InteractionResult {\n\
             @Override public boolean consumesAction() { return true; }\n\
             public boolean shouldSwing() { return swingSource != SwingSource.NONE; }\n\
             public Success heldItemTransformedTo(ItemStack stack) { return null; }\n\
             public Success withoutItem() { return null; }\n\
             public boolean wasItemInteraction() { return itemContext.wasItemInteraction(); }\n\
             public ItemStack heldItemTransformedTo() { return itemContext.heldItemTransformedTo(); }\n\
           }\n\
           record Fail() implements InteractionResult {}\n\
           record Pass() implements InteractionResult {}\n\
           record TryEmptyHandInteraction() implements InteractionResult {}\n\
         }",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat;\n\
         public interface Component {\n\
           static MutableComponent literal(String s) { return null; }\n\
           static MutableComponent translatable(String key) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat;\n\
         public class MutableComponent implements Component {}",
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
        "net/minecraft/world/Container.java",
        "package net.minecraft.world;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public interface Container {\n\
           int getContainerSize();\n\
           ItemStack getItem(int slot);\n\
           void setItem(int slot, ItemStack stack);\n\
         }",
    ),
    (
        "net/minecraft/world/entity/player/Inventory.java",
        "package net.minecraft.world.entity.player;\n\
         import net.minecraft.world.Container;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public class Inventory implements Container {\n\
           public final Player player = null;\n\
           public int getContainerSize() { return 0; }\n\
           public ItemStack getItem(int slot) { return ItemStack.EMPTY; }\n\
           public void setItem(int slot, ItemStack stack) {}\n\
         }",
    ),
    (
        "net/minecraft/world/level/Level.java",
        "package net.minecraft.world.level;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         public abstract class Level {\n\
           public boolean isClientSide() { return false; }\n\
           public BlockState getBlockState(BlockPos pos) { return null; }\n\
           public void playSound(Object entity, BlockPos pos, Object sound) {}\n\
         }",
    ),
    (
        "net/minecraft/world/entity/player/Player.java",
        "package net.minecraft.world.entity.player;\n\
         import java.util.OptionalInt;\n\
         import net.minecraft.world.MenuProvider;\n\
         import net.minecraft.world.level.Level;\n\
         public abstract class Player {\n\
           public Level level() { return null; }\n\
           public Inventory getInventory() { return null; }\n\
           public OptionalInt openMenu(MenuProvider provider) { return OptionalInt.empty(); }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/AbstractContainerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         public abstract class AbstractContainerMenu {\n\
           protected AbstractContainerMenu(MenuType<?> type, int id) {}\n\
           public MenuType<?> getType() { return null; }\n\
           public void removed(Player player) {}\n\
           public abstract boolean stillValid(Player player);\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ContainerLevelAccess.java",
        "package net.minecraft.world.inventory;\n\
         import java.util.Optional;\n\
         import java.util.function.BiConsumer;\n\
         import java.util.function.BiFunction;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.level.Level;\n\
         public interface ContainerLevelAccess {\n\
           ContainerLevelAccess NULL = null;\n\
           static ContainerLevelAccess create(Level level, BlockPos pos) { return null; }\n\
           <T> Optional<T> evaluate(BiFunction<Level, BlockPos, T> fn);\n\
           default <T> T evaluate(BiFunction<Level, BlockPos, T> fn, T fallback) { return fallback; }\n\
           default void execute(BiConsumer<Level, BlockPos> c) {}\n\
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
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         public abstract class ItemCombinerMenu extends AbstractContainerMenu {\n\
           protected final ContainerLevelAccess access;\n\
           protected ItemCombinerMenu(MenuType<?> type, int id, Inventory inv, ContainerLevelAccess access, ItemCombinerMenuSlotDefinition def) {\n\
             super(type, id);\n\
             this.access = access;\n\
           }\n\
           protected abstract boolean isValidBlock(BlockState state);\n\
           public abstract void createResult();\n\
           @Override public boolean stillValid(Player player) { return true; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/MenuType.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.flag.FeatureFlagSet;\n\
         public class MenuType<T extends AbstractContainerMenu> {\n\
           public interface MenuSupplier<T extends AbstractContainerMenu> {\n\
             T create(int containerId, Inventory inv);\n\
           }\n\
           public MenuType(MenuSupplier<T> supplier, FeatureFlagSet features) {}\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/MenuConstructor.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         @FunctionalInterface\n\
         public interface MenuConstructor {\n\
           AbstractContainerMenu createMenu(int containerId, Inventory inv, Player player);\n\
         }",
    ),
    (
        "net/minecraft/world/MenuProvider.java",
        "package net.minecraft.world;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.inventory.MenuConstructor;\n\
         public interface MenuProvider extends MenuConstructor {\n\
           Component getDisplayName();\n\
         }",
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
           @Override public Component getDisplayName() { return null; }\n\
           @Override public AbstractContainerMenu createMenu(int id, Inventory inv, Player p) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/state/BlockBehaviour.java",
        "package net.minecraft.world.level.block.state;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.InteractionHand;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.MenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public abstract class BlockBehaviour {\n\
           public static class Properties { public static Properties of() { return new Properties(); } }\n\
           protected BlockBehaviour(Properties props) {}\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             return InteractionResult.PASS;\n\
           }\n\
           protected InteractionResult useItemOn(ItemStack stack, BlockState state, Level level, BlockPos pos, Player player, InteractionHand hand, BlockHitResult hit) {\n\
             return InteractionResult.TRY_WITH_EMPTY_HAND;\n\
           }\n\
           protected MenuProvider getMenuProvider(BlockState state, Level level, BlockPos pos) { return null; }\n\
           public static abstract class BlockStateBase {\n\
             public MenuProvider getMenuProvider(Level level, BlockPos pos) { return null; }\n\
             public InteractionResult useWithoutItem(Level level, Player player, BlockHitResult hit) { return null; }\n\
             public InteractionResult useItemOn(ItemStack stack, Level level, Player player, InteractionHand hand, BlockHitResult hit) { return null; }\n\
           }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/state/BlockState.java",
        "package net.minecraft.world.level.block.state;\n\
         public class BlockState extends BlockBehaviour.BlockStateBase {}",
    ),
    (
        "net/minecraft/world/level/block/Block.java",
        "package net.minecraft.world.level.block;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Block extends BlockBehaviour {\n\
           public Block(Properties props) { super(props); }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/BaseEntityBlock.java",
        "package net.minecraft.world.level.block;\n\
         public abstract class BaseEntityBlock extends Block {\n\
           protected BaseEntityBlock(Properties props) { super(props); }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/AbstractFurnaceBlock.java",
        "package net.minecraft.world.level.block;\n\
         public abstract class AbstractFurnaceBlock extends BaseEntityBlock {\n\
           protected AbstractFurnaceBlock(Properties props) { super(props); }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/FurnaceBlock.java",
        "package net.minecraft.world.level.block;\n\
         public class FurnaceBlock extends AbstractFurnaceBlock {\n\
           public FurnaceBlock(Properties props) { super(props); }\n\
         }",
    ),
    (
        "net/minecraft/world/level/block/CraftingTableBlock.java",
        "package net.minecraft.world.level.block;\n\
         public class CraftingTableBlock extends Block {\n\
           public CraftingTableBlock(Properties props) { super(props); }\n\
         }",
    ),
];

fn default_context(block_id: &str, side: BlockUseSide, pos: [i32; 3]) -> BlockUseContext {
    BlockUseContext {
        side,
        dimension: "minecraft:overworld".to_owned(),
        block_id: block_id.to_owned(),
        state_properties: BTreeMap::new(),
        pos,
        hit: BlockHitData::block(
            pos,
            "UP",
            [
                pos[0] as f64 + 0.5,
                pos[1] as f64 + 1.0,
                pos[2] as f64 + 0.5,
            ],
            false,
        ),
        player: BlockUsePlayerData::default(),
        hand: "MAIN_HAND".to_owned(),
        stack: None,
    }
}

#[test]
fn block_use_opens_combiner_menu_via_simple_menu_provider() {
    let mut sources = MC_STUBS.to_vec();
    sources.push((
        "demo/DemoCombinerMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.inventory.ItemCombinerMenu;\n\
         import net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         public class DemoCombinerMenu extends ItemCombinerMenu {\n\
           public static final MenuType<DemoCombinerMenu> TYPE =\n\
             new MenuType<>(DemoCombinerMenu::new, FeatureFlags.VANILLA_SET);\n\
           public static int lastAccessX = -999;\n\
           public static int lastSyncId = -1;\n\
           public final Level world;\n\
           public DemoCombinerMenu(int id, Inventory inv) {\n\
             this(id, inv, ContainerLevelAccess.NULL);\n\
           }\n\
           public DemoCombinerMenu(int id, Inventory inv, ContainerLevelAccess access) {\n\
             super(TYPE, id, inv, access, createSlots());\n\
             this.world = inv.player.level();\n\
             lastSyncId = id;\n\
             lastAccessX = access.evaluate((lvl, pos) -> pos.getX(), -1);\n\
           }\n\
           private static ItemCombinerMenuSlotDefinition createSlots() {\n\
             return ItemCombinerMenuSlotDefinition.create()\n\
               .withSlot(0, 27, 47, s -> true)\n\
               .withSlot(1, 76, 47, s -> true)\n\
               .withResultSlot(2, 134, 47)\n\
               .build();\n\
           }\n\
           @Override protected boolean isValidBlock(BlockState state) { return true; }\n\
           @Override public void createResult() {}\n\
         }",
    ));
    sources.push((
        "demo/AltarBlock.java",
        "package demo;\n\
         import java.util.OptionalInt;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.MenuProvider;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.inventory.ContainerLevelAccess;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.CraftingTableBlock;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class AltarBlock extends CraftingTableBlock {\n\
           private static final Component TITLE = Component.translatable(\"gui.demo.combiner\");\n\
           public static int lastReturnedContainerId = -1;\n\
           public AltarBlock(Properties props) { super(props); }\n\
           @Override\n\
           protected MenuProvider getMenuProvider(BlockState state, Level level, BlockPos pos) {\n\
             return new SimpleMenuProvider(\n\
               (syncId, inv, p) -> new DemoCombinerMenu(syncId, inv, ContainerLevelAccess.create(level, pos)),\n\
               TITLE\n\
             );\n\
           }\n\
           @Override\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (level.isClientSide()) {\n\
               return InteractionResult.SUCCESS;\n\
             }\n\
             OptionalInt id = player.openMenu(state.getMenuProvider(level, pos));\n\
             lastReturnedContainerId = id.orElse(-1);\n\
             return InteractionResult.CONSUME;\n\
           }\n\
         }",
    ));
    sources.push((
        "demo/Probe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Probe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"combiner\"), DemoCombinerMenu.TYPE);\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"altar\"), new AltarBlock(BlockBehaviour.Properties.of()));\n\
           }\n\
         }",
    ));

    let fixture = javac_fixture::Fixture::build("block_use_combiner", &sources);
    let mut host = StdHost::new();
    fixture
        .jar
        .execute("demo.Probe", None, "main", &mut host)
        .unwrap();
    assert_eq!(host.unknown_host_call_count(), 0);
    // The player's `ServerPlayer` was just built: `containerCounter` is 0.
    host.server_player_created(1, 1, 0);

    // 1. Inherited BlockBehaviour.useItemOn -> TRY_WITH_EMPTY_HAND (0 unknowns).
    let ctx = default_context("demo:altar", BlockUseSide::Server, [10, 64, -5]);
    let item_on = fixture.jar.block_use_item_on(&mut host, &ctx).unwrap();
    assert_eq!(item_on.failure, None, "{:?}", item_on.unknown_host_calls);
    assert_eq!(item_on.unknown_host_call_count, 0);
    assert_eq!(
        item_on.result,
        Some(InteractionResultValue::TryEmptyHandInteraction)
    );
    assert!(item_on.menu_effects.is_empty());

    // 2. Server-side useWithoutItem -> CONSUME + typed OpenMenuEffect. `initMenu` (the
    //    player's slot listener and synchronizer) is the one counted call.
    let server = fixture.jar.block_use_without_item(&mut host, &ctx).unwrap();
    assert_eq!(server.failure, None, "{:?}", server.unknown_host_calls);
    assert_only_unknown(&server, &[INIT_MENU]);
    assert_eq!(
        server.result,
        Some(InteractionResultValue::Success {
            swing_source: SwingSource::None,
            item_context: InteractionItemContext {
                was_item_interaction: true,
                held_item_transformed_to: None,
            },
        })
    );
    assert_eq!(server.menu_effects.len(), 1);
    let effect = server.open_menus().next().unwrap();
    assert_eq!(effect.container_id, 1);
    assert_eq!(effect.menu_type_id.as_deref(), Some("demo:combiner"));
    assert_eq!(
        effect.title_json,
        Some(json!({ "translate": "gui.demo.combiner" }))
    );
    assert_eq!(effect.access_pos, Some([10, 64, -5]));
    assert!(effect.constructor.is_some());
    // Decaf holds the menu as the player's `containerMenu`, constructor and live menu included.
    let held = host.player_open_menu(1).expect("held menu");
    assert_eq!(held.container_id, 1);
    assert_eq!(held.menu, effect.menu);
    assert_eq!(held.constructor, effect.constructor);
    let statics = host.persistent_class_state().0;
    assert_eq!(
        statics.get("demo/AltarBlock#lastReturnedContainerIdI"),
        Some(&Value::Int(1))
    );
    assert_eq!(
        statics.get("demo/DemoCombinerMenu#lastAccessXI"),
        Some(&Value::Int(10))
    );
    assert_eq!(
        statics.get("demo/DemoCombinerMenu#lastSyncIdI"),
        Some(&Value::Int(1))
    );

    // A second click while the window shows menu 1: `openMenu` closes it first, then builds
    // menu 2. Its vanilla `ItemCombinerMenu.removed` runs exactly (`live_menu.rs`): the cursor
    // and both inputs are empty, so nothing goes back and nothing is counted.
    let mut showing = ctx.clone();
    showing.player.container = PlayerContainer::Open(1);
    let second = fixture
        .jar
        .block_use_without_item(&mut host, &showing)
        .unwrap();
    assert_eq!(second.failure, None, "{:?}", second.unknown_host_calls);
    assert!(
        !second
            .unknown_host_calls
            .keys()
            .any(|k| k.contains("#removed")),
        "the combiner's vanilla removed() runs exactly: {:?}",
        second.unknown_host_calls
    );
    assert!(
        !second
            .unknown_host_calls
            .keys()
            .any(|k| k.contains("closeContainer")),
        "Decaf held menu 1, so its close ran: {:?}",
        second.unknown_host_calls
    );
    assert!(matches!(
        second.menu_effects.as_slice(),
        [MenuEffect::Closed { container_id: 1 }, MenuEffect::Opened(open)] if open.container_id == 2
    ));
    assert_eq!(host.player_open_menu(1).map(|m| m.container_id), Some(2));

    // 3. Client-side useWithoutItem -> SUCCESS + 0 open-menu effects.
    let client_ctx = default_context("demo:altar", BlockUseSide::Client, [10, 64, -5]);
    let client = fixture
        .jar
        .block_use_without_item(&mut host, &client_ctx)
        .unwrap();
    assert_eq!(client.failure, None, "{:?}", client.unknown_host_calls);
    assert_eq!(client.unknown_host_call_count, 0);
    assert_eq!(
        client.result,
        Some(InteractionResultValue::Success {
            swing_source: SwingSource::Predicted,
            item_context: InteractionItemContext {
                was_item_interaction: true,
                held_item_transformed_to: None,
            },
        })
    );
    assert!(client.menu_effects.is_empty());
}

#[test]
fn block_use_returns_every_interaction_result_kind() {
    let mut sources = MC_STUBS.to_vec();
    sources.push((
        "demo/ResultBlock.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.core.Direction;\n\
         import net.minecraft.world.InteractionHand;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         import net.minecraft.world.phys.HitResult;\n\
         public class ResultBlock extends Block {\n\
           public static int checksPassed = 0;\n\
           public ResultBlock(Properties props) { super(props); }\n\
           private static String classify(InteractionResult r) {\n\
             return switch (r) {\n\
               case InteractionResult.Success s -> \"S:\" + s.shouldSwing() + \":\" + s.wasItemInteraction();\n\
               case InteractionResult.Fail f -> \"F:\" + f.consumesAction();\n\
               case InteractionResult.Pass p -> \"P:\" + p.consumesAction();\n\
               case InteractionResult.TryEmptyHandInteraction t -> \"T:\" + t.consumesAction();\n\
             };\n\
           }\n\
           @Override\n\
           protected InteractionResult useItemOn(ItemStack stack, BlockState state, Level level, BlockPos pos, Player player, InteractionHand hand, BlockHitResult hit) {\n\
             if (hit.getType() != HitResult.Type.BLOCK || hit.getDirection() != Direction.UP || hit.isInside() || hit.isWorldBorderHit()) {\n\
               throw new AssertionError(\"unexpected hit\");\n\
             }\n\
             BlockHitResult shifted = hit.withDirection(Direction.NORTH).hitBorder();\n\
             if (shifted.getDirection() != Direction.NORTH || !shifted.isWorldBorderHit()) {\n\
               throw new AssertionError(\"unexpected shifted hit\");\n\
             }\n\
             if (!classify(InteractionResult.SUCCESS).equals(\"S:true:true\")) throw new AssertionError();\n\
             if (!classify(InteractionResult.CONSUME).equals(\"S:false:true\")) throw new AssertionError();\n\
             if (!classify(InteractionResult.FAIL).equals(\"F:false\")) throw new AssertionError();\n\
             if (!classify(InteractionResult.PASS).equals(\"P:false\")) throw new AssertionError();\n\
             if (!classify(InteractionResult.TRY_WITH_EMPTY_HAND).equals(\"T:false\")) throw new AssertionError();\n\
             InteractionResult.Success fresh = new InteractionResult.Success(InteractionResult.SwingSource.NONE, InteractionResult.ItemContext.NONE);\n\
             if (!fresh.equals(InteractionResult.CONSUME.withoutItem())) throw new AssertionError();\n\
             if (fresh == InteractionResult.CONSUME.withoutItem()) throw new AssertionError();\n\
             if (!InteractionResult.FAIL.toString().equals(\"Fail[]\")) throw new AssertionError();\n\
             if (InteractionResult.PASS.hashCode() != 0) throw new AssertionError();\n\
             if (InteractionResult.ItemContext.DEFAULT.hashCode() != 31 * 1231) throw new AssertionError();\n\
             checksPassed++;\n\
             return switch (pos.getX()) {\n\
               case 0 -> InteractionResult.SUCCESS;\n\
               case 1 -> InteractionResult.SUCCESS_SERVER;\n\
               case 2 -> InteractionResult.CONSUME;\n\
               case 3 -> InteractionResult.FAIL;\n\
               case 4 -> InteractionResult.PASS;\n\
               case 5 -> InteractionResult.TRY_WITH_EMPTY_HAND;\n\
               case 6 -> InteractionResult.SUCCESS.withoutItem();\n\
               case 7 -> InteractionResult.SUCCESS_SERVER.heldItemTransformedTo(stack);\n\
               default -> fresh;\n\
             };\n\
           }\n\
         }",
    ));
    sources.push((
        "demo/Probe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Probe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"results\"), new ResultBlock(BlockBehaviour.Properties.of()));\n\
           }\n\
         }",
    ));

    let fixture = javac_fixture::Fixture::build("block_use_results", &sources);
    let mut host = StdHost::new();
    fixture
        .jar
        .execute("demo.Probe", None, "main", &mut host)
        .unwrap();
    assert_eq!(host.unknown_host_call_count(), 0);

    let cases: &[(i32, InteractionResultValue)] = &[
        (
            0,
            InteractionResultValue::Success {
                swing_source: SwingSource::Predicted,
                item_context: InteractionItemContext {
                    was_item_interaction: true,
                    held_item_transformed_to: None,
                },
            },
        ),
        (
            1,
            InteractionResultValue::Success {
                swing_source: SwingSource::ServerOnly,
                item_context: InteractionItemContext {
                    was_item_interaction: true,
                    held_item_transformed_to: None,
                },
            },
        ),
        (
            2,
            InteractionResultValue::Success {
                swing_source: SwingSource::None,
                item_context: InteractionItemContext {
                    was_item_interaction: true,
                    held_item_transformed_to: None,
                },
            },
        ),
        (3, InteractionResultValue::Fail),
        (4, InteractionResultValue::Pass),
        (5, InteractionResultValue::TryEmptyHandInteraction),
        (
            6,
            InteractionResultValue::Success {
                swing_source: SwingSource::Predicted,
                item_context: InteractionItemContext {
                    was_item_interaction: false,
                    held_item_transformed_to: None,
                },
            },
        ),
        (
            8,
            InteractionResultValue::Success {
                swing_source: SwingSource::None,
                item_context: InteractionItemContext {
                    was_item_interaction: false,
                    held_item_transformed_to: None,
                },
            },
        ),
    ];

    for (x, expected) in cases {
        let ctx = default_context("demo:results", BlockUseSide::Server, [*x, 64, 0]);
        let outcome = fixture.jar.block_use_item_on(&mut host, &ctx).unwrap();
        assert_eq!(outcome.failure, None, "x={x}");
        assert_eq!(outcome.unknown_host_call_count, 0, "x={x}");
        assert_eq!(outcome.result.as_ref(), Some(expected), "x={x}");
    }

    // Case 7: heldItemTransformedTo(stack) preserves the transformed ItemStack value.
    let ctx7 = default_context("demo:results", BlockUseSide::Server, [7, 64, 0]);
    let outcome7 = fixture.jar.block_use_item_on(&mut host, &ctx7).unwrap();
    assert_eq!(outcome7.failure, None, "{:?}", outcome7.unknown_host_calls);
    assert_eq!(outcome7.unknown_host_call_count, 0);
    match outcome7.result {
        Some(InteractionResultValue::Success {
            swing_source: SwingSource::ServerOnly,
            item_context:
                InteractionItemContext {
                    was_item_interaction: true,
                    held_item_transformed_to: Some(_),
                },
        }) => {}
        other => panic!("unexpected case 7 result: {other:?}"),
    }
    assert_eq!(
        host.persistent_class_state()
            .0
            .get("demo/ResultBlock#checksPassedI"),
        Some(&Value::Int(9))
    );
}

#[test]
fn block_use_client_side_only_and_client_open_menu_noop() {
    let mut sources = MC_STUBS.to_vec();
    sources.push((
        "demo/ClientOnlyBlock.java",
        "package demo;\n\
         import java.util.OptionalInt;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class ClientOnlyBlock extends Block {\n\
           public static int clientOpenEmpty = 0;\n\
           public ClientOnlyBlock(Properties props) { super(props); }\n\
           @Override\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (!level.isClientSide()) {\n\
               return InteractionResult.PASS;\n\
             }\n\
             OptionalInt res = player.openMenu(new SimpleMenuProvider((id, inv, p) -> null, Component.literal(\"x\")));\n\
             if (res.isEmpty()) clientOpenEmpty++;\n\
             return InteractionResult.SUCCESS;\n\
           }\n\
         }",
    ));
    sources.push((
        "demo/Probe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Probe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"client_only\"), new ClientOnlyBlock(BlockBehaviour.Properties.of()));\n\
           }\n\
         }",
    ));

    let fixture = javac_fixture::Fixture::build("block_use_client_only", &sources);
    let mut host = StdHost::new();
    fixture
        .jar
        .execute("demo.Probe", None, "main", &mut host)
        .unwrap();

    let server = fixture
        .jar
        .block_use_without_item(
            &mut host,
            &default_context("demo:client_only", BlockUseSide::Server, [1, 2, 3]),
        )
        .unwrap();
    assert_eq!(server.failure, None, "{:?}", server.unknown_host_calls);
    assert_eq!(server.unknown_host_call_count, 0);
    assert_eq!(server.result, Some(InteractionResultValue::Pass));
    assert!(server.menu_effects.is_empty());

    let client = fixture
        .jar
        .block_use_without_item(
            &mut host,
            &default_context("demo:client_only", BlockUseSide::Client, [1, 2, 3]),
        )
        .unwrap();
    assert_eq!(client.failure, None, "{:?}", client.unknown_host_calls);
    assert_eq!(client.unknown_host_call_count, 0);
    assert_eq!(
        client.result,
        Some(InteractionResultValue::Success {
            swing_source: SwingSource::Predicted,
            item_context: InteractionItemContext {
                was_item_interaction: true,
                held_item_transformed_to: None,
            },
        })
    );
    assert!(client.menu_effects.is_empty());
    assert_eq!(
        host.persistent_class_state()
            .0
            .get("demo/ClientOnlyBlock#clientOpenEmptyI"),
        Some(&Value::Int(1))
    );
}

#[test]
fn block_use_counts_unmodelled_calls_and_unmodelled_superclasses() {
    let mut sources = MC_STUBS.to_vec();
    sources.push((
        "demo/UnmodelledCallBlock.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class UnmodelledCallBlock extends Block {\n\
           public UnmodelledCallBlock(Properties props) { super(props); }\n\
           @Override\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             // Reading the clicked pos is modelled; reading a neighbour pos or playSound must COUNT.\n\
             if (level.getBlockState(pos) == null) throw new AssertionError();\n\
             level.getBlockState(BlockPos.ZERO);\n\
             level.playSound(player, pos, null);\n\
             return InteractionResult.CONSUME;\n\
           }\n\
         }",
    ));
    sources.push((
        "demo/UnoverriddenCraftingBlock.java",
        "package demo;\n\
         import net.minecraft.world.level.block.CraftingTableBlock;\n\
         public class UnoverriddenCraftingBlock extends CraftingTableBlock {\n\
           public UnoverriddenCraftingBlock(Properties props) { super(props); }\n\
         }",
    ));
    sources.push((
        "demo/PlainBlock.java",
        "package demo;\n\
         import net.minecraft.world.level.block.Block;\n\
         public class PlainBlock extends Block {\n\
           public PlainBlock(Properties props) { super(props); }\n\
         }",
    ));
    sources.push((
        "demo/NullMenuBlock.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class NullMenuBlock extends Block {\n\
           public NullMenuBlock(Properties props) { super(props); }\n\
           @Override\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             if (player.openMenu(null).isPresent()) throw new AssertionError();\n\
             if (player.openMenu(new SimpleMenuProvider((id, inv, p) -> null, Component.literal(\"x\"))).isPresent()) throw new AssertionError();\n\
             return InteractionResult.PASS;\n\
           }\n\
         }",
    ));
    sources.push((
        "demo/Probe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Probe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"unknown_call\"), new UnmodelledCallBlock(BlockBehaviour.Properties.of()));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"crafting_sub\"), new UnoverriddenCraftingBlock(BlockBehaviour.Properties.of()));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"plain\"), new PlainBlock(BlockBehaviour.Properties.of()));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"null_menu\"), new NullMenuBlock(BlockBehaviour.Properties.of()));\n\
           }\n\
         }",
    ));

    let fixture = javac_fixture::Fixture::build("block_use_unknowns", &sources);
    let mut host = StdHost::new();
    fixture
        .jar
        .execute("demo.Probe", None, "main", &mut host)
        .unwrap();
    host.server_player_created(1, 1, 0);

    // 1. Unmodelled calls inside useWithoutItem COUNT.
    let unk = fixture
        .jar
        .block_use_without_item(
            &mut host,
            &default_context("demo:unknown_call", BlockUseSide::Server, [5, 70, 5]),
        )
        .unwrap();
    assert_eq!(unk.failure, None, "{:?}", unk.unknown_host_calls);
    assert_eq!(unk.unknown_host_call_count, 2);
    assert!(
        unk.unknown_host_calls
            .keys()
            .any(|k| k.contains("getBlockState")),
        "expected getBlockState to be counted: {:?}",
        unk.unknown_host_calls
    );
    assert!(
        unk.unknown_host_calls
            .keys()
            .any(|k| k.contains("playSound")),
        "expected playSound to be counted: {:?}",
        unk.unknown_host_calls
    );

    // 2. CraftingTableBlock inherits BlockBehaviour.useItemOn (answered, 0 unknowns) but
    //    declares its own useWithoutItem, which opens a crafting menu -> COUNTED.
    let craft_item = fixture
        .jar
        .block_use_item_on(
            &mut host,
            &default_context("demo:crafting_sub", BlockUseSide::Server, [1, 2, 3]),
        )
        .unwrap();
    assert_eq!(
        craft_item.failure, None,
        "{:?}",
        craft_item.unknown_host_calls
    );
    assert_eq!(craft_item.unknown_host_call_count, 0);
    assert_eq!(
        craft_item.result,
        Some(InteractionResultValue::TryEmptyHandInteraction)
    );
    let craft_empty = fixture
        .jar
        .block_use_without_item(
            &mut host,
            &default_context("demo:crafting_sub", BlockUseSide::Server, [1, 2, 3]),
        )
        .unwrap();
    assert_eq!(
        craft_empty.failure, None,
        "{:?}",
        craft_empty.unknown_host_calls
    );
    assert_eq!(craft_empty.unknown_host_call_count, 1);
    assert!(
        craft_empty
            .unknown_host_calls
            .keys()
            .any(|k| k.contains("CraftingTableBlock#useWithoutItem")),
        "expected CraftingTableBlock#useWithoutItem to be counted: {:?}",
        craft_empty.unknown_host_calls
    );

    // 3. Plain Block subclass inherits BlockBehaviour defaults cleanly with 0 unknowns.
    let plain = fixture
        .jar
        .block_use_without_item(
            &mut host,
            &default_context("demo:plain", BlockUseSide::Server, [1, 2, 3]),
        )
        .unwrap();
    assert_eq!(plain.failure, None, "{:?}", plain.unknown_host_calls);
    assert_eq!(plain.unknown_host_call_count, 0);
    assert_eq!(plain.result, Some(InteractionResultValue::Pass));

    // 4. A null provider returns OptionalInt.empty() before anything (offsets 0-7); a
    //    `createMenu` returning null does too, after advancing the counter (offsets 23-73), which
    //    is its one effect (`CounterAdvanced`, for the embedder's counter). 0 unknowns.
    let null_menu = fixture
        .jar
        .block_use_without_item(
            &mut host,
            &default_context("demo:null_menu", BlockUseSide::Server, [1, 2, 3]),
        )
        .unwrap();
    assert_eq!(
        null_menu.failure, None,
        "{:?}",
        null_menu.unknown_host_calls
    );
    assert_eq!(null_menu.unknown_host_call_count, 0);
    assert!(
        matches!(
            null_menu.menu_effects.as_slice(),
            [MenuEffect::CounterAdvanced { .. }]
        ),
        "{:?}",
        null_menu.menu_effects
    );
    assert_eq!(null_menu.result, Some(InteractionResultValue::Pass));
}

/// `ServerPlayer.openMenu` offsets 102-104: the player's slot listener and synchronizer, which
/// no model runs for a Java menu yet.
const INIT_MENU: &str = "net/minecraft/server/level/ServerPlayer#initMenu(Lnet/minecraft/world/inventory/AbstractContainerMenu;)V";
const CLOSE_CONTAINER: &str = "net/minecraft/server/level/ServerPlayer#closeContainer()V";
const DO_CLOSE_CONTAINER: &str = "net/minecraft/server/level/ServerPlayer#doCloseContainer()V";
const NEXT_CONTAINER_COUNTER: &str =
    "net/minecraft/server/level/ServerPlayer#nextContainerCounter()V";

/// Assert the use counted exactly `expected` (each once) and nothing else.
fn assert_only_unknown(outcome: &BlockUseOutcome, expected: &[&str]) {
    let wanted: BTreeMap<String, usize> = expected.iter().map(|k| ((*k).to_owned(), 1)).collect();
    assert_eq!(
        outcome.unknown_host_calls, wanted,
        "unexpected unknown host calls"
    );
}

/// A menu that logs, as three decimal digits per event, each construction (`1xx`) and each
/// `removed` (`2xx`) with its container id, so a test reads the exact order.
const ORDER_SOURCES: &[(&str, &str)] = &[
    (
        "demo/OrderMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         public class OrderMenu extends AbstractContainerMenu {\n\
           public static final MenuType<OrderMenu> TYPE = new MenuType<>(OrderMenu::new, FeatureFlags.VANILLA_SET);\n\
           public static long log = 0;\n\
           private final int id;\n\
           public OrderMenu(int id, Inventory inv) {\n\
             super(TYPE, id);\n\
             this.id = id;\n\
             log = log * 1000 + 100 + id;\n\
           }\n\
           public static int lastRemoved = -1;\n\
           @Override public void removed(Player player) { log = log * 1000 + 200 + id; lastRemoved = id; }\n\
           @Override public boolean stillValid(Player player) { return true; }\n\
         }",
    ),
    (
        "demo/OrderBlock.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class OrderBlock extends Block {\n\
           public static int lastId = -1;\n\
           public OrderBlock(Properties props) { super(props); }\n\
           @Override\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             lastId = player.openMenu(new SimpleMenuProvider((id, inv, p) -> new OrderMenu(id, inv), Component.literal(\"Order\"))).orElse(-1);\n\
             return InteractionResult.SUCCESS;\n\
           }\n\
         }",
    ),
    (
        "demo/Probe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Probe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"order\"), OrderMenu.TYPE);\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"order\"), new OrderBlock(BlockBehaviour.Properties.of()));\n\
           }\n\
         }",
    ),
];

fn order_fixture(test: &str) -> (javac_fixture::Fixture, StdHost) {
    let mut sources = MC_STUBS.to_vec();
    sources.extend_from_slice(ORDER_SOURCES);
    let fixture = javac_fixture::Fixture::build(test, &sources);
    let mut host = StdHost::new();
    fixture
        .jar
        .execute("demo.Probe", None, "main", &mut host)
        .unwrap();
    assert_eq!(host.unknown_host_call_count(), 0);
    (fixture, host)
}

fn order_log(host: &StdHost) -> i64 {
    match host.persistent_class_state().0.get("demo/OrderMenu#logJ") {
        Some(Value::Long(log)) => *log,
        Some(Value::Int(log)) => i64::from(*log),
        other => panic!("OrderMenu.log is {other:?}"),
    }
}

/// One server right-click on `demo:order` by `player`.
fn click_order(
    fixture: &javac_fixture::Fixture,
    host: &mut StdHost,
    player: &BlockUsePlayerData,
) -> BlockUseOutcome {
    let mut ctx = default_context("demo:order", BlockUseSide::Server, [4, 64, 4]);
    ctx.player = player.clone();
    let outcome = fixture.jar.block_use_without_item(host, &ctx).unwrap();
    // Every order click returns: what it did to the menus is only the whole story then.
    assert_eq!(outcome.failure, None, "{:?}", outcome.unknown_host_calls);
    outcome
}

fn opened_ids(outcome: &BlockUseOutcome) -> Vec<i32> {
    outcome.open_menus().map(|open| open.container_id).collect()
}

/// #629 review P1 4: `ServerPlayer.openMenu` closes the open menu (offsets 8-20) before it
/// advances the counter (23) and builds the new menu (27-42), so the old menu's `removed` runs
/// before the new constructor; a menu Decaf can't close is counted.
#[test]
fn open_menu_closes_the_held_menu_before_building_the_next() {
    let (fixture, mut host) = order_fixture("block_use_close_order");
    host.server_player_created(1, 1, 0);
    let mut player = BlockUsePlayerData::default();

    // Nothing open: no close, menu 1 built.
    let first = click_order(&fixture, &mut host, &player);
    assert_only_unknown(&first, &[INIT_MENU]);
    assert_eq!(opened_ids(&first), [1]);
    assert_eq!(order_log(&host), 101);

    // The window shows menu 1: its `removed` runs, then menu 2's constructor.
    player.container = PlayerContainer::Open(1);
    let second = click_order(&fixture, &mut host, &player);
    assert_only_unknown(&second, &[INIT_MENU]);
    assert!(matches!(
        second.menu_effects.as_slice(),
        [MenuEffect::Closed { container_id: 1 }, MenuEffect::Opened(open)] if open.container_id == 2
    ));
    assert_eq!(order_log(&host), 101_201_102);

    // A container Decaf doesn't hold is open (a chest the embedder owns): its close is counted,
    // and what Decaf held is no longer the player's menu.
    player.container = PlayerContainer::Open(55);
    let foreign = click_order(&fixture, &mut host, &player);
    assert_only_unknown(&foreign, &[CLOSE_CONTAINER, INIT_MENU]);
    assert_eq!(opened_ids(&foreign), [3]);
    assert_eq!(order_log(&host), 101_201_102_103);

    // The embedder can't name the open menu: counted too.
    player.container = PlayerContainer::Unknown;
    let unknown = click_order(&fixture, &mut host, &player);
    assert_only_unknown(&unknown, &[CLOSE_CONTAINER, INIT_MENU]);
    assert_eq!(opened_ids(&unknown), [4]);

    // The embedder closed menu 4 without telling Decaf (no `close_player_menu`): the missed
    // `doCloseContainer` is counted, and nothing more is closed.
    player.container = PlayerContainer::Inventory;
    let missed = click_order(&fixture, &mut host, &player);
    assert_only_unknown(&missed, &[DO_CLOSE_CONTAINER, INIT_MENU]);
    assert_eq!(opened_ids(&missed), [5]);
    assert_eq!(order_log(&host), 101_201_102_103_104_105);
}

/// #629 review P1 1: Decaf holds at most one menu per player (the `containerMenu`), replaces
/// it on the next open and drops it on close, refusal, logout and reset, with every retain
/// hold released.
#[test]
fn open_menu_holds_one_menu_per_player_and_releases_it() {
    let (fixture, mut host) = order_fixture("block_use_menu_lifetime");
    let baseline = host.retained_count();
    host.server_player_created(1, 1, 0);
    let mut player = BlockUsePlayerData::default();

    let first = click_order(&fixture, &mut host, &player);
    assert_eq!(opened_ids(&first), [1]);
    let one_menu = host.retained_count() - baseline;
    assert!(one_menu > 0, "the held menu's values are retained");

    // N clicks while each window shows: one held menu, the last, and one menu's holds.
    for id in 2..=12 {
        player.container = PlayerContainer::Open(id - 1);
        let outcome = click_order(&fixture, &mut host, &player);
        assert_eq!(opened_ids(&outcome), [id]);
        assert_eq!(host.player_open_menu(1).map(|m| m.container_id), Some(id));
        assert_eq!(host.retained_count(), baseline + one_menu);
    }
    // The window closed: the menu's `removed` runs and its holds go.
    player.container = PlayerContainer::Open(12);
    let closed = fixture
        .jar
        .close_player_menu(
            &mut host,
            &player,
            "minecraft:overworld",
            12,
            &ExecutionBudget::UNBOUNDED,
        )
        .unwrap()
        .expect("menu 12 is held");
    assert_eq!(closed.failure, None);
    assert_eq!(closed.container_id, 12);
    assert_eq!(
        closed.unknown_host_call_count, 0,
        "{:?}",
        closed.unknown_host_calls
    );
    assert_eq!(
        host.persistent_class_state()
            .0
            .get("demo/OrderMenu#lastRemovedI"),
        Some(&Value::Int(12))
    );
    assert!(host.player_open_menu(1).is_none());
    assert_eq!(host.retained_count(), baseline);
    // A second close of the same window finds nothing.
    assert!(
        fixture
            .jar
            .close_player_menu(
                &mut host,
                &player,
                "minecraft:overworld",
                12,
                &ExecutionBudget::UNBOUNDED,
            )
            .unwrap()
            .is_none()
    );

    // Refused by the embedder: dropped, and the counter is no longer trusted.
    player.container = PlayerContainer::Inventory;
    assert_eq!(opened_ids(&click_order(&fixture, &mut host, &player)), [13]);
    assert_eq!(host.retained_count(), baseline + one_menu);
    assert!(host.refuse_open_menu(1, 13));
    assert!(host.player_open_menu(1).is_none());
    assert_eq!(host.retained_count(), baseline);
    let after_refusal = click_order(&fixture, &mut host, &player);
    assert_only_unknown(&after_refusal, &[NEXT_CONTAINER_COUNTER, INIT_MENU]);
    assert_eq!(host.retained_count(), baseline + one_menu);

    // Logout drops what the player held.
    host.server_player_removed(1);
    assert!(host.player_open_menu(1).is_none());
    assert_eq!(host.retained_count(), baseline);

    // Two players each hold their own menu; a new server drops both.
    host.server_player_created(1, 2, 0);
    player.incarnation = Some((2, 0));
    let other = BlockUsePlayerData {
        uuid: 2,
        name: "Other".to_owned(),
        entity_id: Some(2),
        ..BlockUsePlayerData::default()
    };
    host.server_player_created(2, 1, 0);
    assert_eq!(opened_ids(&click_order(&fixture, &mut host, &player)), [1]);
    assert_eq!(opened_ids(&click_order(&fixture, &mut host, &other)), [1]);
    assert!(host.player_open_menu(1).is_some() && host.player_open_menu(2).is_some());
    assert_ne!(
        host.player_open_menu(1).map(|m| &m.menu),
        host.player_open_menu(2).map(|m| &m.menu)
    );
    assert!(host.retained_count() > baseline + one_menu);
    host.reset_player_menus();
    assert!(host.player_open_menu(1).is_none() && host.player_open_menu(2).is_none());
    assert_eq!(host.retained_count(), baseline);
}

/// #629 review P1 2: `containerCounter` belongs to one `ServerPlayer` object. A login and
/// `PlayerList.respawn` build a new one (`restoreFrom` doesn't copy the counter,
/// `server-26.3.jar`), so the first id is 1 again; it wraps 100 -> 1; and Decaf counts it when
/// it doesn't know the object or the counter.
#[test]
fn container_counter_lives_as_long_as_the_server_player() {
    let (fixture, mut host) = order_fixture("block_use_container_counter");
    let mut player = BlockUsePlayerData::default();
    let click = |host: &mut StdHost, player: &mut BlockUsePlayerData| {
        let outcome = click_order(&fixture, host, player);
        let ids = opened_ids(&outcome);
        assert_eq!(ids.len(), 1, "{outcome:?}");
        player.container = PlayerContainer::Open(ids[0]);
        (ids[0], outcome)
    };

    host.server_player_created(1, 1, 0);
    assert_eq!(click(&mut host, &mut player).0, 1);
    assert_eq!(click(&mut host, &mut player).0, 2);

    // Reconnect: a new session's object starts at 0 again.
    host.server_player_removed(1);
    host.server_player_created(1, 2, 0);
    player.incarnation = Some((2, 0));
    player.container = PlayerContainer::Inventory;
    let (id, relog) = click(&mut host, &mut player);
    assert_eq!(id, 1);
    assert_only_unknown(&relog, &[INIT_MENU]);
    assert_eq!(click(&mut host, &mut player).0, 2);

    // Respawn: `PlayerList.respawn` builds a new object; `restoreFrom` leaves its counter 0.
    host.server_player_created(1, 2, 1);
    player.incarnation = Some((2, 1));
    player.container = PlayerContainer::Inventory;
    assert_eq!(click(&mut host, &mut player).0, 1);

    // Wraps at 100: `containerCounter % 100 + 1`.
    for expected in 2..=100 {
        assert_eq!(click(&mut host, &mut player).0, expected);
    }
    assert_eq!(click(&mut host, &mut player).0, 1);

    // Another opener (a chest, another jar) took id 41 on a new object: the counter follows.
    host.server_player_created(1, 3, 0);
    player.incarnation = Some((3, 0));
    player.container = PlayerContainer::Inventory;
    host.server_player_container_opened(1, Some(41));
    let (id, after_chest) = click(&mut host, &mut player);
    assert_eq!(id, 42);
    assert_only_unknown(&after_chest, &[INIT_MENU]);
    // An open whose id the embedder can't read: the counter is unknown, so it is counted.
    host.server_player_container_opened(1, None);
    let (_, unread) = click(&mut host, &mut player);
    assert!(
        unread
            .unknown_host_calls
            .contains_key(NEXT_CONTAINER_COUNTER)
    );

    // An object Decaf was not told was created, or one it can't name at all: counted.
    let mut untold_player = BlockUsePlayerData {
        uuid: 7,
        incarnation: Some((5, 0)),
        ..BlockUsePlayerData::default()
    };
    let (_, untold) = click(&mut host, &mut untold_player);
    assert!(
        untold
            .unknown_host_calls
            .contains_key(NEXT_CONTAINER_COUNTER)
    );
    let mut nameless = BlockUsePlayerData {
        uuid: 8,
        incarnation: None,
        ..BlockUsePlayerData::default()
    };
    let (_, unnamed) = click(&mut host, &mut nameless);
    assert!(
        unnamed
            .unknown_host_calls
            .contains_key(NEXT_CONTAINER_COUNTER)
    );
    assert!(
        host.player_open_menu(8).is_none(),
        "no object to hold it on"
    );
}

/// A block whose `useItemOn` reads its menu provider through the state and through `this`,
/// then defers to `super` for both hooks, so every inherited body is reached.
fn inherited_probe(class: &str, parent: &str) -> String {
    format!(
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.InteractionHand;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.MenuProvider;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.*;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class {class} extends {parent} {{\n\
           public static int providerNull = -1;\n\
           public {class}(Properties props) {{ super(props); }}\n\
           @Override\n\
           protected InteractionResult useItemOn(ItemStack stack, BlockState state, Level level, BlockPos pos, Player player, InteractionHand hand, BlockHitResult hit) {{\n\
             MenuProvider viaState = state.getMenuProvider(level, pos);\n\
             MenuProvider viaThis = this.getMenuProvider(state, level, pos);\n\
             providerNull = (viaState == null ? 1 : 0) + (viaThis == null ? 2 : 0);\n\
             return super.useItemOn(stack, state, level, pos, player, hand, hit);\n\
           }}\n\
           @Override\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {{\n\
             return super.useWithoutItem(state, level, pos, player, hit);\n\
           }}\n\
         }}"
    )
}

/// #629 review P1 3: an inherited block-use method runs the body of the nearest vanilla class
/// that declares it (`server-26.3.jar`): answered for `BlockBehaviour`'s, counted for any
/// other (`AbstractFurnaceBlock.useWithoutItem`, `BaseEntityBlock.getMenuProvider`,
/// `CraftingTableBlock`'s two).
#[test]
fn inherited_block_methods_follow_the_vanilla_declaring_class() {
    const FURNACE_USE: &str = "net/minecraft/world/level/block/AbstractFurnaceBlock#useWithoutItem(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;";
    const ENTITY_PROVIDER: &str = "net/minecraft/world/level/block/BaseEntityBlock#getMenuProvider(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/MenuProvider;";
    const CRAFTING_USE: &str = "net/minecraft/world/level/block/CraftingTableBlock#useWithoutItem(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;";
    const CRAFTING_PROVIDER: &str = "net/minecraft/world/level/block/CraftingTableBlock#getMenuProvider(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/MenuProvider;";

    let furnace_probe = inherited_probe("FurnaceProbe", "FurnaceBlock");
    let crafting_probe = inherited_probe("CraftingProbe", "CraftingTableBlock");
    let plain_probe = inherited_probe("PlainProbe", "Block");
    let mut sources = MC_STUBS.to_vec();
    sources.push(("demo/FurnaceProbe.java", &furnace_probe));
    sources.push(("demo/CraftingProbe.java", &crafting_probe));
    sources.push(("demo/PlainProbe.java", &plain_probe));
    sources.push((
        "demo/PlainFurnace.java",
        "package demo;\n\
         import net.minecraft.world.level.block.FurnaceBlock;\n\
         public class PlainFurnace extends FurnaceBlock {\n\
           public PlainFurnace(Properties props) { super(props); }\n\
         }",
    ));
    sources.push((
        "demo/Probe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Probe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"furnace_probe\"), new FurnaceProbe(BlockBehaviour.Properties.of()));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"crafting_probe\"), new CraftingProbe(BlockBehaviour.Properties.of()));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"plain_probe\"), new PlainProbe(BlockBehaviour.Properties.of()));\n\
             Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", \"plain_furnace\"), new PlainFurnace(BlockBehaviour.Properties.of()));\n\
           }\n\
         }",
    ));
    let fixture = javac_fixture::Fixture::build("block_use_inherited", &sources);
    let mut host = StdHost::new();
    fixture
        .jar
        .execute("demo.Probe", None, "main", &mut host)
        .unwrap();
    // Registration counts only `FurnaceBlock`'s own constructor and state definition (its
    // FACING and LIT properties), which no model builds; the uses below are measured alone.
    assert!(
        host.unknown_host_calls().iter().all(|(key, _)| {
            key.starts_with("net/minecraft/world/level/block/FurnaceBlock#<init>")
                || key.starts_with(
                    "net/minecraft/world/level/block/FurnaceBlock#createBlockStateDefinition",
                )
        }),
        "{:?}",
        host.unknown_host_calls()
    );
    let run = |host: &mut StdHost, id: &str, with_item: bool| {
        let ctx = default_context(id, BlockUseSide::Server, [3, 3, 3]);
        let outcome = if with_item {
            fixture.jar.block_use_item_on(host, &ctx).unwrap()
        } else {
            fixture.jar.block_use_without_item(host, &ctx).unwrap()
        };
        assert_eq!(
            outcome.failure, None,
            "{id}: {:?}",
            outcome.unknown_host_calls
        );
        outcome
    };

    // FurnaceBlock: `super.useItemOn` is BlockBehaviour's (answered); both getMenuProvider
    // calls reach BaseEntityBlock's (counted twice).
    let furnace_item = run(&mut host, "demo:furnace_probe", true);
    assert_eq!(
        furnace_item.result,
        Some(InteractionResultValue::TryEmptyHandInteraction)
    );
    assert_eq!(
        furnace_item.unknown_host_calls,
        BTreeMap::from([(ENTITY_PROVIDER.to_owned(), 2)])
    );
    // `super.useWithoutItem` is AbstractFurnaceBlock's, which opens the furnace: counted.
    assert_only_unknown(&run(&mut host, "demo:furnace_probe", false), &[FURNACE_USE]);
    // The same with no override at all (the entry point's own virtual dispatch).
    let plain_furnace_item = run(&mut host, "demo:plain_furnace", true);
    assert_eq!(plain_furnace_item.unknown_host_call_count, 0);
    assert_eq!(
        plain_furnace_item.result,
        Some(InteractionResultValue::TryEmptyHandInteraction)
    );
    assert_only_unknown(&run(&mut host, "demo:plain_furnace", false), &[FURNACE_USE]);

    // CraftingTableBlock declares useWithoutItem and getMenuProvider, not useItemOn.
    let crafting_item = run(&mut host, "demo:crafting_probe", true);
    assert_eq!(
        crafting_item.result,
        Some(InteractionResultValue::TryEmptyHandInteraction)
    );
    assert_eq!(
        crafting_item.unknown_host_calls,
        BTreeMap::from([(CRAFTING_PROVIDER.to_owned(), 2)])
    );
    assert_only_unknown(
        &run(&mut host, "demo:crafting_probe", false),
        &[CRAFTING_USE],
    );

    // A plain Block inherits all three from BlockBehaviour: every answer exact.
    let plain_item = run(&mut host, "demo:plain_probe", true);
    assert_eq!(
        plain_item.unknown_host_call_count, 0,
        "{:?}",
        plain_item.unknown_host_calls
    );
    assert_eq!(
        plain_item.result,
        Some(InteractionResultValue::TryEmptyHandInteraction)
    );
    assert_eq!(
        host.persistent_class_state()
            .0
            .get("demo/PlainProbe#providerNullI"),
        Some(&Value::Int(3))
    );
    let plain_empty = run(&mut host, "demo:plain_probe", false);
    assert_eq!(plain_empty.unknown_host_call_count, 0);
    assert_eq!(plain_empty.result, Some(InteractionResultValue::Pass));
}

/// Menus whose `removed` re-enters `openMenu` or throws, for the vanilla `closeContainer` /
/// `doCloseContainer` order (#629 review round 2). Every construction and `removed` appends
/// three digits to `Log.log`, which keeps the last five: `1xx`/`2xx` a plain menu's,
/// `4xx`/`3xx` a re-entrant one's, `5xx`/`6xx` a throwing one's, `xx` the container id.
const NESTED_SOURCES: &[(&str, &str)] = &[
    (
        "demo/Log.java",
        "package demo;\n\
         public final class Log {\n\
           public static long log = 0;\n\
           public static int nestedId = -1;\n\
           public static int caught = 0;\n\
           public static void add(int code) { log = (log % 1000000000000L) * 1000 + code; }\n\
         }",
    ),
    (
        "demo/PlainMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         public class PlainMenu extends AbstractContainerMenu {\n\
           public static final MenuType<PlainMenu> TYPE = new MenuType<>(PlainMenu::new, FeatureFlags.VANILLA_SET);\n\
           private final int id;\n\
           public PlainMenu(int id, Inventory inv) { super(TYPE, id); this.id = id; Log.add(100 + id); }\n\
           @Override public void removed(Player player) { Log.add(200 + id); }\n\
           @Override public boolean stillValid(Player player) { return true; }\n\
         }",
    ),
    (
        "demo/ReentrantMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         public class ReentrantMenu extends AbstractContainerMenu {\n\
           public static final MenuType<ReentrantMenu> TYPE = new MenuType<>(ReentrantMenu::new, FeatureFlags.VANILLA_SET);\n\
           private final int id;\n\
           private boolean opened;\n\
           public ReentrantMenu(int id, Inventory inv) { super(TYPE, id); this.id = id; Log.add(400 + id); }\n\
           @Override public void removed(Player player) {\n\
             Log.add(300 + id);\n\
             if (!opened) {\n\
               opened = true;\n\
               Log.nestedId = player.openMenu(Providers.plain()).orElse(-1);\n\
             }\n\
           }\n\
           @Override public boolean stillValid(Player player) { return true; }\n\
         }",
    ),
    (
        "demo/ThrowingMenu.java",
        "package demo;\n\
         import net.minecraft.world.entity.player.Inventory;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.flag.FeatureFlags;\n\
         import net.minecraft.world.inventory.AbstractContainerMenu;\n\
         import net.minecraft.world.inventory.MenuType;\n\
         public class ThrowingMenu extends AbstractContainerMenu {\n\
           public static final MenuType<ThrowingMenu> TYPE = new MenuType<>(ThrowingMenu::new, FeatureFlags.VANILLA_SET);\n\
           private final int id;\n\
           private int removedCalls;\n\
           public ThrowingMenu(int id, Inventory inv) { super(TYPE, id); this.id = id; Log.add(500 + id); }\n\
           @Override public void removed(Player player) {\n\
             Log.add(600 + id);\n\
             if (++removedCalls <= 2) throw new IllegalStateException(\"removed\");\n\
           }\n\
           @Override public boolean stillValid(Player player) { return true; }\n\
         }",
    ),
    (
        "demo/Providers.java",
        "package demo;\n\
         import net.minecraft.network.chat.Component;\n\
         import net.minecraft.world.MenuProvider;\n\
         import net.minecraft.world.SimpleMenuProvider;\n\
         public final class Providers {\n\
           public static MenuProvider plain() { return new SimpleMenuProvider((id, inv, p) -> new PlainMenu(id, inv), Component.literal(\"Plain\")); }\n\
           public static MenuProvider reentrant() { return new SimpleMenuProvider((id, inv, p) -> new ReentrantMenu(id, inv), Component.literal(\"Reentrant\")); }\n\
           public static MenuProvider throwing() { return new SimpleMenuProvider((id, inv, p) -> new ThrowingMenu(id, inv), Component.literal(\"Throwing\")); }\n\
         }",
    ),
    (
        "demo/MenuBlock.java",
        "package demo;\n\
         import net.minecraft.core.BlockPos;\n\
         import net.minecraft.world.InteractionResult;\n\
         import net.minecraft.world.entity.player.Player;\n\
         import net.minecraft.world.level.Level;\n\
         import net.minecraft.world.level.block.Block;\n\
         import net.minecraft.world.level.block.state.BlockState;\n\
         import net.minecraft.world.phys.BlockHitResult;\n\
         public class MenuBlock extends Block {\n\
           private final int kind;\n\
           public MenuBlock(Properties props, int kind) { super(props); this.kind = kind; }\n\
           @Override\n\
           protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {\n\
             switch (kind) {\n\
               case 0 -> player.openMenu(Providers.plain());\n\
               case 1 -> player.openMenu(Providers.reentrant());\n\
               case 2 -> player.openMenu(Providers.throwing());\n\
               default -> {\n\
                 try { player.openMenu(Providers.plain()); }\n\
                 catch (IllegalStateException e) { Log.caught++; }\n\
               }\n\
             }\n\
             return InteractionResult.SUCCESS;\n\
           }\n\
         }",
    ),
    (
        "demo/Probe.java",
        "package demo;\n\
         import net.fabricmc.api.ModInitializer;\n\
         import net.minecraft.core.Registry;\n\
         import net.minecraft.core.registries.BuiltInRegistries;\n\
         import net.minecraft.resources.Identifier;\n\
         import net.minecraft.world.level.block.state.BlockBehaviour;\n\
         public class Probe implements ModInitializer {\n\
           @Override public void onInitialize() {\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"plain\"), PlainMenu.TYPE);\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"reentrant\"), ReentrantMenu.TYPE);\n\
             Registry.register(BuiltInRegistries.MENU, Identifier.fromNamespaceAndPath(\"demo\", \"throwing\"), ThrowingMenu.TYPE);\n\
             String[] ids = { \"plain_block\", \"reentrant_block\", \"throwing_block\", \"catching_block\" };\n\
             for (int kind = 0; kind < ids.length; kind++) {\n\
               Registry.register(BuiltInRegistries.BLOCK, Identifier.fromNamespaceAndPath(\"demo\", ids[kind]), new MenuBlock(BlockBehaviour.Properties.of(), kind));\n\
             }\n\
           }\n\
         }",
    ),
];

fn nested_fixture(test: &str) -> (javac_fixture::Fixture, StdHost) {
    let mut sources = MC_STUBS.to_vec();
    sources.extend_from_slice(NESTED_SOURCES);
    let fixture = javac_fixture::Fixture::build(test, &sources);
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
    (fixture, host)
}

fn nested_static(host: &StdHost, field: &str) -> i64 {
    match host
        .persistent_class_state()
        .0
        .get(&format!("demo/Log#{field}"))
    {
        Some(Value::Long(value)) => *value,
        Some(Value::Int(value)) => i64::from(*value),
        other => panic!("Log.{field} is {other:?}"),
    }
}

fn use_block(
    fixture: &javac_fixture::Fixture,
    host: &mut StdHost,
    block: &str,
    player: &BlockUsePlayerData,
) -> BlockUseOutcome {
    let mut ctx = default_context(block, BlockUseSide::Server, [2, 70, 2]);
    ctx.player = player.clone();
    fixture.jar.block_use_without_item(host, &ctx).unwrap()
}

/// A short description of a use's menu effects: `C1` closed 1, `O2` opened 2, `X2` discarded 2.
fn effect_trace(effects: &[MenuEffect]) -> Vec<String> {
    effects
        .iter()
        .map(|effect| match effect {
            MenuEffect::Closed { container_id } => format!("C{container_id}"),
            MenuEffect::Opened(open) => format!("O{}", open.container_id),
            MenuEffect::Orphaned { container_id } => format!("X{container_id}"),
            MenuEffect::CounterAdvanced { container_id } => format!("A{container_id}"),
        })
        .collect()
}

/// A `removed` that opens a menu runs while its own menu is still installed (vanilla reaches
/// `containerMenu = inventoryMenu` only at `doCloseContainer` offset 19): the nested
/// `openMenu` closes that menu again first (`removed` re-entered), opens its menu, and the
/// outer close's offset-19 assignment then discards that nested menu without closing it.
#[test]
fn a_removed_that_opens_a_menu_follows_vanillas_nested_close() {
    let (fixture, mut host) = nested_fixture("block_use_nested_removed");
    let baseline = host.retained_count();
    host.server_player_created(1, 1, 0);
    let mut player = BlockUsePlayerData::default();

    let first = use_block(&fixture, &mut host, "demo:reentrant_block", &player);
    assert_eq!(first.failure, None, "{:?}", first.unknown_host_calls);
    assert_eq!(effect_trace(&first.menu_effects), ["O1"]);
    assert_eq!(nested_static(&host, "logJ"), 401);

    // Open a plain menu while the window shows the re-entrant one. Vanilla: close packet for
    // 1, removed(1) -> nested openMenu: close packet for 1 again, removed(1) re-entered (no
    // second open), container 2 built and installed; back in the outer doCloseContainer,
    // `containerMenu = inventoryMenu` discards 2; then 3 is built and installed.
    player.container = PlayerContainer::Open(1);
    let second = use_block(&fixture, &mut host, "demo:plain_block", &player);
    assert_eq!(second.failure, None, "{:?}", second.unknown_host_calls);
    assert_eq!(
        effect_trace(&second.menu_effects),
        ["C1", "C1", "O2", "X2", "O3"]
    );
    assert_eq!(nested_static(&host, "logJ"), 401_301_301_102_103);
    assert_eq!(nested_static(&host, "nestedIdI"), 2);
    // `initMenu` for the nested menu and the outer one.
    assert_eq!(
        second.unknown_host_calls,
        BTreeMap::from([(INIT_MENU.to_owned(), 2)])
    );
    assert_eq!(host.player_open_menu(1).map(|m| m.container_id), Some(3));

    // The same through a close the embedder reports (`handleContainerClose`): no outer close
    // packet, the nested open's own close and open, and its menu discarded at offset 19.
    player.container = PlayerContainer::Open(3);
    let third = use_block(&fixture, &mut host, "demo:reentrant_block", &player);
    assert_eq!(third.failure, None, "{:?}", third.unknown_host_calls);
    assert_eq!(effect_trace(&third.menu_effects), ["C3", "O4"]);
    assert_eq!(nested_static(&host, "logJ"), 301_102_103_203_404);
    let closed = fixture
        .jar
        .close_player_menu(
            &mut host,
            &player,
            "minecraft:overworld",
            4,
            &ExecutionBudget::UNBOUNDED,
        )
        .unwrap()
        .expect("menu 4 is held");
    assert!(closed.failure.is_none(), "{:?}", closed.failure);
    assert_eq!(effect_trace(&closed.menu_effects), ["C4", "O5", "X5"]);
    assert_eq!(nested_static(&host, "logJ"), 203_404_304_304_105);
    assert!(host.player_open_menu(1).is_none());
    // Every menu discarded or closed along the way released its holds.
    assert_eq!(host.retained_count(), baseline);
}

/// A `removed` that throws stops `doCloseContainer` before offset 19: the menu stays installed
/// (its close packet already went out), the exception reaches the Java caller of `openMenu`,
/// and the next `openMenu` closes it again.
#[test]
fn a_removed_that_throws_leaves_its_menu_installed() {
    let (fixture, mut host) = nested_fixture("block_use_throwing_removed");
    let baseline = host.retained_count();
    host.server_player_created(1, 1, 0);
    let mut player = BlockUsePlayerData::default();

    let first = use_block(&fixture, &mut host, "demo:throwing_block", &player);
    assert_eq!(first.failure, None, "{:?}", first.unknown_host_calls);
    assert_eq!(effect_trace(&first.menu_effects), ["O1"]);

    // The throw escapes `openMenu` and the use: the close packet went out, nothing else ran.
    player.container = PlayerContainer::Open(1);
    let failed = use_block(&fixture, &mut host, "demo:plain_block", &player);
    assert!(
        failed
            .failure
            .as_deref()
            .is_some_and(|f| f.contains("IllegalStateException")),
        "{:?}",
        failed.failure
    );
    assert_eq!(failed.result, None);
    assert_eq!(effect_trace(&failed.menu_effects), ["C1"]);
    assert_eq!(nested_static(&host, "logJ"), 501_601);
    assert_eq!(host.player_open_menu(1).map(|m| m.container_id), Some(1));
    // Sunrise closing the window that close packet closed is no close of vanilla's server.
    player.container = PlayerContainer::Inventory;
    assert!(
        fixture
            .jar
            .close_player_menu(
                &mut host,
                &player,
                "minecraft:overworld",
                1,
                &ExecutionBudget::UNBOUNDED,
            )
            .unwrap()
            .is_none()
    );

    // The next openMenu, with no window shown, still closes the installed menu: it throws
    // again, and this block catches it at its `openMenu` call.
    let caught = use_block(&fixture, &mut host, "demo:catching_block", &player);
    assert!(caught.failure.is_none(), "{:?}", caught.failure);
    assert_eq!(
        caught.unknown_host_call_count, 0,
        "{:?}",
        caught.unknown_host_calls
    );
    assert_eq!(effect_trace(&caught.menu_effects), ["C1"]);
    assert_eq!(nested_static(&host, "caughtI"), 1);
    assert_eq!(nested_static(&host, "logJ"), 501_601_601);
    assert_eq!(host.player_open_menu(1).map(|m| m.container_id), Some(1));

    // Third time its removed returns: closed, and the counter (never advanced by the throws)
    // gives 2.
    let opened = use_block(&fixture, &mut host, "demo:plain_block", &player);
    assert!(opened.failure.is_none());
    assert_eq!(effect_trace(&opened.menu_effects), ["C1", "O2"]);
    assert_eq!(nested_static(&host, "logJ"), 501_601_601_601_102);
    assert_eq!(host.player_open_menu(1).map(|m| m.container_id), Some(2));

    // A throwing removed on the embedder's close: the menu stays installed with no window,
    // and the next openMenu closes it again.
    player.container = PlayerContainer::Open(2);
    let throwing = use_block(&fixture, &mut host, "demo:throwing_block", &player);
    assert_eq!(throwing.failure, None, "{:?}", throwing.unknown_host_calls);
    assert_eq!(effect_trace(&throwing.menu_effects), ["C2", "O3"]);
    let closed = fixture
        .jar
        .close_player_menu(
            &mut host,
            &player,
            "minecraft:overworld",
            3,
            &ExecutionBudget::UNBOUNDED,
        )
        .unwrap()
        .expect("menu 3 is held");
    assert!(
        closed
            .failure
            .as_deref()
            .is_some_and(|f| f.contains("IllegalStateException")),
        "{:?}",
        closed.failure
    );
    assert!(closed.menu_effects.is_empty());
    assert_eq!(host.player_open_menu(1).map(|m| m.container_id), Some(3));
    player.container = PlayerContainer::Inventory;
    let again = use_block(&fixture, &mut host, "demo:catching_block", &player);
    assert_eq!(again.failure, None, "{:?}", again.unknown_host_calls);
    assert_eq!(effect_trace(&again.menu_effects), ["C3"]);
    assert_eq!(nested_static(&host, "caughtI"), 2);

    host.server_player_removed(1);
    assert_eq!(host.retained_count(), baseline);
}
