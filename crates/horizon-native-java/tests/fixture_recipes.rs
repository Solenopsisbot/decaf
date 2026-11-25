//! General vanilla 26.3 recipe decoding, matching, assembly, TransmuteRecipe, Ingredient,
//! ItemStackTemplate, DataComponentPatch, SimpleContainer, and ItemCombinerMenu.createResult
//! fixture.
//!
//! Expected outputs in `fixtures/recipes/mc263.txt` were captured from a real Java 25 run
//! against Minecraft 26.3 server classes (`tests/oracles/recipes/GeneralRecipesOracle.java`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{
    ExecutionBudget, Host, StdHost,
    events::{EventContext, EventFacade, EventFiring, FacadeLevel, FacadePlayer, GameEvent},
};
use std::collections::BTreeMap;
use std::sync::Arc;

const STUBS: &[(&str, &str)] = &[
    (
        "com/mojang/datafixers/kinds/App.java",
        "package com.mojang.datafixers.kinds; public interface App<F, A> {}",
    ),
    (
        "com/mojang/datafixers/kinds/Applicative.java",
        "package com.mojang.datafixers.kinds; public interface Applicative<F, Mu> {}",
    ),
    (
        "com/mojang/datafixers/util/Function6.java",
        "package com.mojang.datafixers.util;\n\
         @FunctionalInterface public interface Function6<T1,T2,T3,T4,T5,T6,R> { R apply(T1 a, T2 b, T3 c, T4 d, T5 e, T6 f); }",
    ),
    (
        "com/mojang/datafixers/util/Function7.java",
        "package com.mojang.datafixers.util;\n\
         @FunctionalInterface public interface Function7<T1,T2,T3,T4,T5,T6,T7,R> { R apply(T1 a, T2 b, T3 c, T4 d, T5 e, T6 f, T7 g); }",
    ),
    (
        "com/mojang/datafixers/Products.java",
        "package com.mojang.datafixers;\n\
         import com.mojang.datafixers.kinds.*; import com.mojang.datafixers.util.*;\n\
         public final class Products {\n\
           public static final class P1<F,T1> {\n\
             public <R> App<F,R> apply(Applicative<F,?> app, java.util.function.Function<T1,R> f) { return null; }\n\
           }\n\
           public static final class P2<F,T1,T2> {\n\
             public <R> App<F,R> apply(Applicative<F,?> app, java.util.function.BiFunction<T1,T2,R> f) { return null; }\n\
           }\n\
           public static final class P6<F,T1,T2,T3,T4,T5,T6> {\n\
             public <R> App<F,R> apply(Applicative<F,?> app, Function6<T1,T2,T3,T4,T5,T6,R> f) { return null; }\n\
           }\n\
           public static final class P7<F,T1,T2,T3,T4,T5,T6,T7> {\n\
             public <R> App<F,R> apply(Applicative<F,?> app, Function7<T1,T2,T3,T4,T5,T6,T7,R> f) { return null; }\n\
           }\n\
         }",
    ),
    (
        "com/mojang/serialization/Codec.java",
        "package com.mojang.serialization;\n\
         import java.util.Optional; import java.util.function.Function;\n\
         public interface Codec<A> {\n\
           Codec<Integer> INT = null;\n\
           Codec<String> STRING = null;\n\
           MapCodec<A> fieldOf(String name);\n\
           MapCodec<Optional<A>> optionalFieldOf(String name);\n\
           MapCodec<A> optionalFieldOf(String name, A defaultValue);\n\
           <S> Codec<S> xmap(Function<? super A, ? extends S> to, Function<? super S, ? extends A> from);\n\
         }",
    ),
    (
        "com/mojang/serialization/DynamicOps.java",
        "package com.mojang.serialization; public interface DynamicOps<T> {}",
    ),
    (
        "com/mojang/serialization/MapLike.java",
        "package com.mojang.serialization; public interface MapLike<T> {}",
    ),
    (
        "com/mojang/serialization/DataResult.java",
        "package com.mojang.serialization;\n\
         import java.util.Optional;\n\
         public interface DataResult<R> {\n\
           Optional<R> result();\n\
           boolean isSuccess();\n\
         }",
    ),
    (
        "com/mojang/serialization/MapCodec.java",
        "package com.mojang.serialization;\n\
         import com.mojang.serialization.codecs.RecordCodecBuilder;\n\
         import java.util.function.Function; import java.util.function.Supplier;\n\
         public abstract class MapCodec<A> {\n\
           public static <A> MapCodec<A> unit(A defaultValue) { return null; }\n\
           public static <A> MapCodec<A> unit(Supplier<A> defaultValue) { return null; }\n\
           public <O> RecordCodecBuilder<O, A> forGetter(Function<O, A> getter) { return null; }\n\
           public <S> MapCodec<S> xmap(Function<? super A, ? extends S> to, Function<? super S, ? extends A> from) { return null; }\n\
           public Codec<A> codec() { return null; }\n\
           public abstract <T> DataResult<A> decode(DynamicOps<T> ops, MapLike<T> input);\n\
         }",
    ),
    (
        "com/mojang/serialization/codecs/RecordCodecBuilder.java",
        "package com.mojang.serialization.codecs;\n\
         import com.mojang.datafixers.Products; import com.mojang.datafixers.kinds.*;\n\
         import com.mojang.serialization.*; import java.util.function.Function;\n\
         public final class RecordCodecBuilder<O, P> implements App<RecordCodecBuilder.Mu<O>, P> {\n\
           public static final class Mu<O> {}\n\
           public static final class Instance<O> implements Applicative<Mu<O>, Object> {\n\
             public <T1> Products.P1<Mu<O>,T1> group(App<Mu<O>,T1> t1) { return null; }\n\
             public <T1,T2> Products.P2<Mu<O>,T1,T2> group(App<Mu<O>,T1> t1, App<Mu<O>,T2> t2) { return null; }\n\
             public <T1,T2,T3,T4,T5,T6> Products.P6<Mu<O>,T1,T2,T3,T4,T5,T6> group(\n\
               App<Mu<O>,T1> t1, App<Mu<O>,T2> t2, App<Mu<O>,T3> t3,\n\
               App<Mu<O>,T4> t4, App<Mu<O>,T5> t5, App<Mu<O>,T6> t6) { return null; }\n\
             public <T1,T2,T3,T4,T5,T6,T7> Products.P7<Mu<O>,T1,T2,T3,T4,T5,T6,T7> group(\n\
               App<Mu<O>,T1> t1, App<Mu<O>,T2> t2, App<Mu<O>,T3> t3,\n\
               App<Mu<O>,T4> t4, App<Mu<O>,T5> t5, App<Mu<O>,T6> t6, App<Mu<O>,T7> t7) { return null; }\n\
           }\n\
           public static <O> Codec<O> create(Function<Instance<O>, ? extends App<Mu<O>, O>> builder) { return null; }\n\
           public static <O> MapCodec<O> mapCodec(Function<Instance<O>, ? extends App<Mu<O>, O>> builder) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/util/ExtraCodecs.java",
        "package net.minecraft.util;\n\
         import com.mojang.serialization.Codec;\n\
         public final class ExtraCodecs {\n\
           public static Codec<Integer> intRange(int min, int max) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/network/codec/StreamCodec.java",
        "package net.minecraft.network.codec; public interface StreamCodec<B, V> {}",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources;\n\
         public final class Identifier {\n\
           public static Identifier fromNamespaceAndPath(String ns, String path) { return null; }\n\
           public static Identifier parse(String id) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources;\n\
         public class ResourceKey<T> {\n\
           public static <T> ResourceKey<T> create(ResourceKey<? extends net.minecraft.core.Registry<T>> reg, Identifier id) { return null; }\n\
           public Identifier identifier() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core;\n\
         import net.minecraft.resources.Identifier;\n\
         public interface Registry<T> {\n\
           static <V, T extends V> T register(Registry<V> reg, Identifier id, T value) { return value; }\n\
         }",
    ),
    (
        "net/minecraft/core/registries/Registries.java",
        "package net.minecraft.core.registries;\n\
         import net.minecraft.resources.ResourceKey; import net.minecraft.core.Registry;\n\
         public final class Registries {\n\
           public static final ResourceKey<Registry<net.minecraft.world.item.Item>> ITEM = null;\n\
           public static final ResourceKey<Registry<net.minecraft.world.item.crafting.Recipe<?>>> RECIPE = null;\n\
         }",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries;\n\
         import net.minecraft.core.Registry;\n\
         public final class BuiltInRegistries {\n\
           public static final Registry<net.minecraft.world.item.Item> ITEM = null;\n\
           public static final Registry<net.minecraft.world.item.crafting.RecipeType<?>> RECIPE_TYPE = null;\n\
           public static final Registry<net.minecraft.world.item.crafting.RecipeSerializer<?>> RECIPE_SERIALIZER = null;\n\
         }",
    ),
    (
        "net/minecraft/core/Holder.java",
        "package net.minecraft.core;\n\
         public interface Holder<T> { T value(); String getRegisteredName(); }",
    ),
    (
        "net/minecraft/core/component/DataComponentType.java",
        "package net.minecraft.core.component; public interface DataComponentType<T> {}",
    ),
    (
        "net/minecraft/core/component/DataComponentPatch.java",
        "package net.minecraft.core.component;\n\
         public final class DataComponentPatch {\n\
           public static final DataComponentPatch EMPTY = null;\n\
           public static Builder builder() { return null; }\n\
           public boolean isEmpty() { return true; }\n\
           public int size() { return 0; }\n\
           public static final class Builder {\n\
             public <T> Builder set(DataComponentType<T> t, T v) { return this; }\n\
             public <T> Builder remove(DataComponentType<T> t) { return this; }\n\
             public DataComponentPatch build() { return null; }\n\
           }\n\
         }",
    ),
    (
        "net/minecraft/core/component/DataComponents.java",
        "package net.minecraft.core.component;\n\
         import net.minecraft.network.chat.Component;\n\
         public final class DataComponents {\n\
           public static final DataComponentType<Integer> REPAIR_COST = null;\n\
           public static final DataComponentType<Integer> MAX_DAMAGE = null;\n\
           public static final DataComponentType<Integer> MAX_STACK_SIZE = null;\n\
           public static final DataComponentType<Component> CUSTOM_NAME = null;\n\
           public static final DataComponentType<Boolean> ENCHANTMENT_GLINT_OVERRIDE = null;\n\
           public static final DataComponentType<Object> RARITY = null;\n\
           public static final DataComponentType<Object> CONTAINER = null;\n\
         }",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat;\n\
         public interface Component {\n\
           static MutableComponent literal(String text) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat;\n\
         public class MutableComponent implements Component {}",
    ),
    (
        "net/minecraft/world/level/ItemLike.java",
        "package net.minecraft.world.level;\n\
         import net.minecraft.world.item.Item;\n\
         public interface ItemLike { Item asItem(); }",
    ),
    (
        "net/minecraft/world/item/Item.java",
        "package net.minecraft.world.item;\n\
         import net.minecraft.resources.ResourceKey; import net.minecraft.world.level.ItemLike;\n\
         public class Item implements ItemLike {\n\
           public Item(Properties props) {}\n\
           public Item asItem() { return this; }\n\
           public static class Properties {\n\
             public Properties() {}\n\
             public Properties stacksTo(int n) { return this; }\n\
             public Properties setId(ResourceKey<Item> key) { return this; }\n\
           }\n\
         }",
    ),
    (
        "net/minecraft/world/item/Items.java",
        "package net.minecraft.world.item;\n\
         public final class Items {\n\
           public static final Item AIR = null;\n\
           public static final Item STONE = null;\n\
           public static final Item COBBLESTONE = null;\n\
           public static final Item BLACKSTONE = null;\n\
           public static final Item COBBLED_DEEPSLATE = null;\n\
           public static final Item DIRT = null;\n\
           public static final Item IRON_INGOT = null;\n\
           public static final Item GOLD_INGOT = null;\n\
           public static final Item COPPER_INGOT = null;\n\
           public static final Item REDSTONE = null;\n\
           public static final Item AMETHYST_SHARD = null;\n\
           public static final Item COAL = null;\n\
           public static final Item DIAMOND = null;\n\
           public static final Item EMERALD = null;\n\
           public static final Item QUARTZ = null;\n\
           public static final Item GOLD_NUGGET = null;\n\
           public static final Item STICK = null;\n\
           public static final Item ENDER_PEARL = null;\n\
           public static final Item DIAMOND_SWORD = null;\n\
         }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item;\n\
         import net.minecraft.core.component.*; import net.minecraft.world.level.ItemLike;\n\
         public final class ItemStack {\n\
           public static final ItemStack EMPTY = null;\n\
           public ItemStack(ItemLike item) {}\n\
           public ItemStack(ItemLike item, int count) {}\n\
           public boolean isEmpty() { return true; }\n\
           public int getCount() { return 0; }\n\
           public int getMaxStackSize() { return 1; }\n\
           public Item getItem() { return null; }\n\
           public boolean is(Item item) { return false; }\n\
           public DataComponentPatch getComponentsPatch() { return null; }\n\
           public <T> T set(DataComponentType<T> type, T value) { return null; }\n\
           public <T> T get(DataComponentType<? extends T> type) { return null; }\n\
           public <T> T remove(DataComponentType<? extends T> type) { return null; }\n\
           public void applyComponents(DataComponentPatch patch) {}\n\
         }",
    ),
    (
        "net/minecraft/world/item/ItemStackTemplate.java",
        "package net.minecraft.world.item;\n\
         import com.mojang.serialization.Codec; import net.minecraft.core.Holder;\n\
         import net.minecraft.core.component.DataComponentPatch;\n\
         public final class ItemStackTemplate {\n\
           public static final Codec<ItemStackTemplate> CODEC = null;\n\
           public ItemStackTemplate(Item item, int count) {}\n\
           public ItemStackTemplate(Item item, DataComponentPatch patch) {}\n\
           public static ItemStackTemplate fromNonEmptyStack(ItemStack stack) { return null; }\n\
           public Holder<Item> item() { return null; }\n\
           public int count() { return 0; }\n\
           public DataComponentPatch components() { return null; }\n\
           public ItemStack create() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/item/crafting/Ingredient.java",
        "package net.minecraft.world.item.crafting;\n\
         import com.mojang.serialization.Codec; import java.util.Optional;\n\
         import net.minecraft.world.item.ItemStack; import net.minecraft.world.level.ItemLike;\n\
         public final class Ingredient {\n\
           public static final Codec<Ingredient> CODEC = null;\n\
           public static Ingredient of(ItemLike item) { return null; }\n\
           public static boolean testOptionalIngredient(Optional<Ingredient> opt, ItemStack stack) { return false; }\n\
           public boolean test(ItemStack stack) { return false; }\n\
         }",
    ),
    (
        "net/minecraft/world/item/crafting/RecipeInput.java",
        "package net.minecraft.world.item.crafting;\n\
         import net.minecraft.world.item.ItemStack;\n\
         public interface RecipeInput {\n\
           ItemStack getItem(int index);\n\
           int size();\n\
           default boolean isEmpty() {\n\
             for (int i = 0; i < size(); i++) {\n\
               if (!getItem(i).isEmpty()) return false;\n\
             }\n\
             return true;\n\
           }\n\
         }",
    ),
    (
        "net/minecraft/world/item/crafting/RecipeType.java",
        "package net.minecraft.world.item.crafting;\n\
         public interface RecipeType<T extends Recipe<?>> {\n\
           RecipeType<Recipe<RecipeInput>> CRAFTING = null;\n\
         }",
    ),
    (
        "net/minecraft/world/item/crafting/RecipeSerializer.java",
        "package net.minecraft.world.item.crafting;\n\
         import com.mojang.serialization.MapCodec; import net.minecraft.network.codec.StreamCodec;\n\
         public final class RecipeSerializer<T extends Recipe<?>> {\n\
           public RecipeSerializer(MapCodec<T> codec, StreamCodec<?, T> streamCodec) {}\n\
           public MapCodec<T> codec() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/item/crafting/Recipe.java",
        "package net.minecraft.world.item.crafting;\n\
         import com.mojang.serialization.MapCodec;\n\
         import net.minecraft.world.item.ItemStack; import net.minecraft.world.level.Level;\n\
         public interface Recipe<I extends RecipeInput> {\n\
           boolean matches(I input, Level level);\n\
           ItemStack assemble(I input);\n\
           RecipeSerializer<? extends Recipe<I>> getSerializer();\n\
           RecipeType<? extends Recipe<I>> getType();\n\
           final class CommonInfo {\n\
             public static final MapCodec<CommonInfo> MAP_CODEC = null;\n\
             public CommonInfo(boolean showNotification) {}\n\
             public boolean showNotification() { return true; }\n\
           }\n\
         }",
    ),
    (
        "net/minecraft/world/item/crafting/RecipeHolder.java",
        "package net.minecraft.world.item.crafting;\n\
         import net.minecraft.resources.ResourceKey;\n\
         public final class RecipeHolder<T extends Recipe<?>> {\n\
           public RecipeHolder(ResourceKey<Recipe<?>> id, T value) {}\n\
           public ResourceKey<Recipe<?>> id() { return null; }\n\
           public T value() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/item/crafting/RecipeManager.java",
        "package net.minecraft.world.item.crafting;\n\
         import java.util.Optional; import net.minecraft.resources.ResourceKey;\n\
         import net.minecraft.world.level.Level;\n\
         public class RecipeManager {\n\
           public RecipeManager() {}\n\
           public Object getRecipeFor() { return null; }\n\
           public <I extends RecipeInput, T extends Recipe<I>> Optional<RecipeHolder<T>> getRecipeFor(RecipeType<T> type, I input, Level level) { return Optional.empty(); }\n\
           public <I extends RecipeInput, T extends Recipe<I>> Optional<RecipeHolder<T>> getRecipeFor(RecipeType<T> type, I input, Level level, ResourceKey<Recipe<?>> last) { return Optional.empty(); }\n\
           public <I extends RecipeInput, T extends Recipe<I>> Optional<RecipeHolder<T>> getRecipeFor(RecipeType<T> type, I input, Level level, RecipeHolder<T> last) { return Optional.empty(); }\n\
           public Optional<RecipeHolder<?>> byKey(ResourceKey<Recipe<?>> key) { return Optional.empty(); }\n\
         }",
    ),
    (
        "net/minecraft/world/item/crafting/TransmuteRecipe.java",
        "package net.minecraft.world.item.crafting;\n\
         import net.minecraft.world.item.*;\n\
         public final class TransmuteRecipe {\n\
           public static ItemStack createWithOriginalComponents(ItemStackTemplate tpl, ItemStack base) { return null; }\n\
           public static ItemStack createWithOriginalComponents(ItemStackTemplate tpl, ItemStack base, int extra) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/server/MinecraftServer.java",
        "package net.minecraft.server;\n\
         import net.minecraft.world.item.crafting.RecipeManager;\n\
         public class MinecraftServer {\n\
           public RecipeManager getRecipeManager() { return null; }\n\
           public RecipeManager recipeAccess() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/level/Level.java",
        "package net.minecraft.world.level; public abstract class Level {}",
    ),
    (
        "net/minecraft/server/level/ServerLevel.java",
        "package net.minecraft.server.level;\n\
         import net.minecraft.server.MinecraftServer;\n\
         import net.minecraft.world.item.crafting.RecipeManager; import net.minecraft.world.level.Level;\n\
         public class ServerLevel extends Level {\n\
           public RecipeManager recipeAccess() { return null; }\n\
           public MinecraftServer getServer() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/entity/player/Player.java",
        "package net.minecraft.world.entity.player;\n\
         import net.minecraft.world.level.Level;\n\
         public class Player { public Level level() { return null; } }",
    ),
    (
        "net/minecraft/world/entity/EntityEquipment.java",
        "package net.minecraft.world.entity; public class EntityEquipment {}",
    ),
    (
        "net/minecraft/world/entity/player/Inventory.java",
        "package net.minecraft.world.entity.player;\n\
         import net.minecraft.world.entity.EntityEquipment;\n\
         public class Inventory { public final Player player; public Inventory(Player p, EntityEquipment e) { player = p; } }",
    ),
    (
        "net/minecraft/core/NonNullList.java",
        "package net.minecraft.core; import java.util.ArrayList; public class NonNullList<T> extends ArrayList<T> {}",
    ),
    (
        "net/minecraft/world/Container.java",
        "package net.minecraft.world; import net.minecraft.world.item.ItemStack;\n\
         public interface Container {\n\
           int getContainerSize(); ItemStack getItem(int i); void setItem(int i, ItemStack s); void setChanged(); boolean isEmpty();\n\
           default int getMaxStackSize() { return 99; }\n\
           default int getMaxStackSize(ItemStack stack) { return Math.min(getMaxStackSize(), stack.getMaxStackSize()); }\n\
         }",
    ),
    (
        "net/minecraft/world/SimpleContainer.java",
        "package net.minecraft.world; import net.minecraft.world.item.ItemStack;\n\
         public class SimpleContainer implements Container {\n\
           public SimpleContainer(int size) {}\n\
           public int getContainerSize() { return 0; }\n\
           public ItemStack getItem(int i) { return null; }\n\
           public void setItem(int i, ItemStack s) {}\n\
           public void setChanged() {}\n\
           public boolean isEmpty() { return true; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ResultContainer.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container; import net.minecraft.world.item.ItemStack;\n\
         import net.minecraft.world.item.crafting.RecipeHolder;\n\
         public class ResultContainer implements Container {\n\
           public ResultContainer() {}\n\
           public int getContainerSize() { return 1; }\n\
           public ItemStack getItem(int i) { return null; }\n\
           public void setItem(int i, ItemStack s) {}\n\
           public void setChanged() {}\n\
           public boolean isEmpty() { return true; }\n\
           public void setRecipeUsed(RecipeHolder<?> h) {}\n\
           public RecipeHolder<?> getRecipeUsed() { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/MenuType.java",
        "package net.minecraft.world.inventory; public class MenuType<T> {}",
    ),
    (
        "net/minecraft/world/inventory/ContainerLevelAccess.java",
        "package net.minecraft.world.inventory;\n\
         public class ContainerLevelAccess { public static final ContainerLevelAccess NULL = null; }",
    ),
    (
        "net/minecraft/world/inventory/Slot.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container;\n\
         public class Slot { public final Container container; public Slot(Container c, int i, int x, int y) { container = c; } }",
    ),
    (
        "net/minecraft/world/inventory/AbstractContainerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.core.NonNullList;\n\
         public abstract class AbstractContainerMenu {\n\
           public final NonNullList<Slot> slots = new NonNullList<>();\n\
           protected AbstractContainerMenu(MenuType<?> t, int id) {}\n\
           public Slot getSlot(int index) { return null; }\n\
         }",
    ),
    (
        "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition.java",
        "package net.minecraft.world.inventory;\n\
         import java.util.function.Predicate; import net.minecraft.world.item.ItemStack;\n\
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
        "net/minecraft/world/level/block/state/BlockState.java",
        "package net.minecraft.world.level.block.state; public class BlockState {}",
    ),
    (
        "net/minecraft/world/inventory/ItemCombinerMenu.java",
        "package net.minecraft.world.inventory;\n\
         import net.minecraft.world.Container; import net.minecraft.world.entity.player.*;\n\
         import net.minecraft.world.item.ItemStack; import net.minecraft.world.level.block.state.BlockState;\n\
         public abstract class ItemCombinerMenu extends AbstractContainerMenu {\n\
           protected final Container inputSlots;\n\
           protected final ResultContainer resultSlots;\n\
           protected ItemCombinerMenu(MenuType<?> t, int id, Inventory inv, ContainerLevelAccess acc, ItemCombinerMenuSlotDefinition def) {\n\
             super(t, id); inputSlots = null; resultSlots = null;\n\
           }\n\
           public abstract void createResult();\n\
           protected abstract void onTake(Player p, ItemStack s);\n\
           protected abstract boolean isValidBlock(BlockState s);\n\
         }",
    ),
    (
        "net/minecraft/world/InteractionHand.java",
        "package net.minecraft.world; public enum InteractionHand { MAIN_HAND, OFF_HAND }",
    ),
    (
        "net/minecraft/world/InteractionResult.java",
        "package net.minecraft.world; public final class InteractionResult { public static final InteractionResult PASS = new InteractionResult(); }",
    ),
    (
        "net/fabricmc/fabric/api/event/Event.java",
        "package net.fabricmc.fabric.api.event; public abstract class Event<T> { public void register(T listener) {} }",
    ),
    (
        "net/fabricmc/fabric/api/event/player/UseItemCallback.java",
        "package net.fabricmc.fabric.api.event.player;\n\
         import net.fabricmc.fabric.api.event.Event;\n\
         import net.minecraft.world.*; import net.minecraft.world.entity.player.Player; import net.minecraft.world.level.Level;\n\
         public interface UseItemCallback {\n\
           Event<UseItemCallback> EVENT = null;\n\
           InteractionResult interact(Player player, Level level, InteractionHand hand);\n\
         }",
    ),
];

const DEMO_RECIPE_INPUT: &str = r#"
package demo;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeInput;

public final class AlloyInput implements RecipeInput {
  public final ItemStack base;
  public final ItemStack catalyst;
  public AlloyInput(ItemStack base, ItemStack catalyst) {
    this.base = base;
    this.catalyst = catalyst;
  }
  public ItemStack getItem(int index) { return index == 0 ? base : catalyst; }
  public int size() { return 2; }
}
"#;

const DEMO_RECIPE: &str = r#"
package demo;
import com.mojang.serialization.Codec;
import com.mojang.serialization.MapCodec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import java.util.Optional;
import net.minecraft.util.ExtraCodecs;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.Level;

public final class AlloyRecipe implements Recipe<AlloyInput> {
  public static int labelXmapCalls = 0;
  public static int ctorCalls = 0;

  public final Recipe.CommonInfo commonInfo;
  public final Ingredient base;
  public final Ingredient catalyst;
  public final ItemStackTemplate result;
  public final int tier;
  public final Optional<String> label;
  public final int weight;

  public static final RecipeType<AlloyRecipe> TYPE = new RecipeType<>() {};
  public static final MapCodec<AlloyRecipe> MAP_CODEC = RecordCodecBuilder.mapCodec(i -> i.group(
      Recipe.CommonInfo.MAP_CODEC.forGetter(r -> r.commonInfo),
      Ingredient.CODEC.fieldOf("base").forGetter(r -> r.base),
      Ingredient.CODEC.fieldOf("catalyst").forGetter(r -> r.catalyst),
      ItemStackTemplate.CODEC.fieldOf("result").forGetter(r -> r.result),
      Codec.INT.optionalFieldOf("tier", 1).forGetter(r -> r.tier),
      Codec.STRING.optionalFieldOf("label").xmap(
          opt -> {
            if (opt.isPresent()) labelXmapCalls++;
            // `Locale.ROOT`: the no-argument forms use the default locale, which the
            // runtime does not know and counts when `i`/`I` could map differently.
            return opt.map(s -> s.toUpperCase(java.util.Locale.ROOT));
          },
          opt -> opt.map(s -> s.toLowerCase(java.util.Locale.ROOT))).forGetter(r -> r.label),
      ExtraCodecs.intRange(1, 99).optionalFieldOf("weight", 1).forGetter(r -> r.weight)
  ).apply(i, (ci, b, c, r, t, l, w) -> {
    ctorCalls++;
    return new AlloyRecipe(ci, b, c, r, t, l, w);
  }));
  public static final RecipeSerializer<AlloyRecipe> SERIALIZER = new RecipeSerializer<>(MAP_CODEC, null);

  public AlloyRecipe(
      Recipe.CommonInfo commonInfo,
      Ingredient base,
      Ingredient catalyst,
      ItemStackTemplate result,
      int tier,
      Optional<String> label,
      int weight) {
    this.commonInfo = commonInfo;
    this.base = base;
    this.catalyst = catalyst;
    this.result = result;
    this.tier = tier;
    this.label = label;
    this.weight = weight;
  }

  public boolean matches(AlloyInput input, Level level) {
    return base.test(input.base) && catalyst.test(input.catalyst);
  }

  public ItemStack assemble(AlloyInput input) {
    return TransmuteRecipe.createWithOriginalComponents(result, input.base);
  }

  public RecipeSerializer<AlloyRecipe> getSerializer() { return SERIALIZER; }
  public RecipeType<AlloyRecipe> getType() { return TYPE; }
}
"#;

const DEMO_ALWAYS_RECIPE: &str = r#"
package demo;
import com.mojang.serialization.MapCodec;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.Level;

public final class AlwaysRecipe implements Recipe<RecipeInput> {
  public static int matchesCalls = 0;
  public static final RecipeType<AlwaysRecipe> TYPE = new RecipeType<>() {};
  public static final AlwaysRecipe INSTANCE = new AlwaysRecipe();
  public static final MapCodec<AlwaysRecipe> MAP_CODEC = MapCodec.unit(INSTANCE);
  public static final RecipeSerializer<AlwaysRecipe> SERIALIZER = new RecipeSerializer<>(MAP_CODEC, null);

  public boolean matches(RecipeInput input, Level level) {
    matchesCalls++;
    return true;
  }
  public ItemStack assemble(RecipeInput input) { return new ItemStack(Items.STICK); }
  public RecipeSerializer<AlwaysRecipe> getSerializer() { return SERIALIZER; }
  public RecipeType<AlwaysRecipe> getType() { return TYPE; }
}
"#;

const DEMO_MENU: &str = r#"
package demo;
import java.util.Optional;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;

public final class AlloyMenu extends ItemCombinerMenu {
  private final Level world;
  public int createResultCalls;

  public AlloyMenu(int id, Inventory inv) {
    super(null, id, inv, ContainerLevelAccess.NULL, ItemCombinerMenuSlotDefinition.create()
        .withSlot(0, 27, 47, s -> true)
        .withSlot(1, 76, 47, s -> true)
        .withResultSlot(2, 134, 47)
        .build());
    this.world = inv.player.level();
  }

  public void createResult() {
    createResultCalls++;
    AlloyInput input = new AlloyInput(inputSlots.getItem(0), inputSlots.getItem(1));
    Optional<RecipeHolder<AlloyRecipe>> match = (world instanceof ServerLevel sl)
        ? sl.recipeAccess().getRecipeFor(AlloyRecipe.TYPE, input, sl)
        : Optional.empty();
    if (match.isPresent()) {
      RecipeHolder<AlloyRecipe> holder = match.get();
      resultSlots.setRecipeUsed(holder);
      resultSlots.setItem(0, holder.value().assemble(input));
    } else {
      resultSlots.setRecipeUsed(null);
      resultSlots.setItem(0, ItemStack.EMPTY);
    }
  }

  public void setInputs(ItemStack a, ItemStack b) {
    inputSlots.setItem(0, a);
    inputSlots.setItem(1, b);
  }

  public ItemStack resultItem() { return resultSlots.getItem(0); }
  public RecipeHolder<?> usedRecipe() { return resultSlots.getRecipeUsed(); }
  public boolean inputsEmpty() { return inputSlots.isEmpty(); }
  public boolean resultEmpty() { return resultSlots.isEmpty(); }
  protected void onTake(Player p, ItemStack s) {}
  protected boolean isValidBlock(BlockState s) { return true; }
}
"#;

const DEMO_PROBE: &str = r#"
package demo;
import java.util.Optional;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.*;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.Level;

public final class Probe {
  public static Item ALLOY_ITEM;
  public static Item SMALL_ITEM;
  public static String MENU_SUMMARY;
  public static String OVERLOAD_SUMMARY;
  public static String SERVER_SUMMARY;
  public static String VIRTUAL_IS_EMPTY_SUMMARY;

  private static void capture(String name, Runnable action) {
    try {
      action.run();
      System.out.println(name + "=ok");
    } catch (Throwable t) {
      System.out.println(name + "=" + t.getClass().getName() + ":" + t.getMessage());
    }
  }

  public static void onInitialize() {
    Identifier alloyId = Identifier.fromNamespaceAndPath("demo", "alloy_ingot");
    ALLOY_ITEM = Registry.register(
        BuiltInRegistries.ITEM,
        alloyId,
        new Item(new Item.Properties().setId(ResourceKey.create(Registries.ITEM, alloyId))));
    Identifier smallId = Identifier.fromNamespaceAndPath("demo", "small_stack_item");
    SMALL_ITEM = Registry.register(
        BuiltInRegistries.ITEM,
        smallId,
        new Item(new Item.Properties().stacksTo(16).setId(ResourceKey.create(Registries.ITEM, smallId))));
    Identifier typeId = Identifier.fromNamespaceAndPath("demo", "alloy");
    Registry.register(BuiltInRegistries.RECIPE_TYPE, typeId, AlloyRecipe.TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, typeId, AlloyRecipe.SERIALIZER);
    Identifier alwaysId = Identifier.fromNamespaceAndPath("demo", "always");
    Registry.register(BuiltInRegistries.RECIPE_TYPE, alwaysId, AlwaysRecipe.TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, alwaysId, AlwaysRecipe.SERIALIZER);

    UseItemCallback.EVENT.register((player, level, hand) -> {
      runAll(player, (ServerLevel) level);
      return InteractionResult.PASS;
    });
  }

  private static void runAll(Player player, ServerLevel level) {
    RecipeManager mgr = level.recipeAccess();
    String[] validIds = {
        "demo:iron_alloy", "demo:precious_alloy", "demo:stone_alloy",
        "demo:small_stack", "demo:dfu_coercion"
    };
    for (String id : validIds) {
      ResourceKey<Recipe<?>> key = ResourceKey.create(Registries.RECIPE, Identifier.parse(id));
      AlloyRecipe rec = (AlloyRecipe) mgr.byKey(key).get().value();
      System.out.println("decoded:" + id
          + "|notify=" + rec.commonInfo.showNotification()
          + "|tier=" + rec.tier
          + "|label=" + rec.label.orElse("none")
          + "|weight=" + rec.weight
          + "|count=" + rec.result.count());
    }

    String[] invalidIds = {
        "demo:bad_air", "demo:bad_empty_list", "demo:bad_missing_tag",
        "demo:bad_count", "demo:bad_result_air", "demo:bad_leading_space",
        "demo:bad_trailing_space", "demo:bad_null_base", "demo:bad_count_float_hi",
        "demo:bad_count_float_lo"
    };
    int invalidErrors = 0;
    for (String id : invalidIds) {
      ResourceKey<Recipe<?>> key = ResourceKey.create(Registries.RECIPE, Identifier.parse(id));
      if (mgr.byKey(key).isEmpty()) {
        invalidErrors++;
      }
    }
    System.out.println("invalidErrorCount=" + invalidErrors);

    // RecordCodecBuilder multi-error accumulation: bad_multi ran label xmap (total=2 vs 1 from iron_alloy)
    // and never called the AlloyRecipe constructor (5 valid + 3 partial-result bad_count* recipes = 8,
    // matching real MC 26.3 where ItemStackTemplate.CODEC returns a count=1 partial on invalid count).
    boolean multiIsErr = mgr.byKey(ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:bad_multi"))).isEmpty();
    System.out.println("multiErr:isErr=" + multiIsErr
        + "|xmapDelta=" + (AlloyRecipe.labelXmapCalls - 1)
        + "|ctorDelta=" + (AlloyRecipe.ctorCalls - 8));

    ItemStack taggedBase = new ItemStack(Items.COBBLESTONE, 3);
    taggedBase.set(DataComponents.REPAIR_COST, 5);
    taggedBase.set(DataComponents.CUSTOM_NAME, Component.literal("Ancient"));

    ItemStack ironWithComp = new ItemStack(Items.IRON_INGOT, 1);
    ironWithComp.set(DataComponents.REPAIR_COST, 9);

    String[] names = {
        "iron+redstone", "gold+amethyst", "copper+amethyst",
        "cobble+coal", "blackstone+coal", "deepslate+coal",
        "diamond+nugget", "emerald+quartz", "dirt+coal", "empty+empty"
    };
    ItemStack[] left = {
        ironWithComp, new ItemStack(Items.GOLD_INGOT), new ItemStack(Items.COPPER_INGOT),
        taggedBase, new ItemStack(Items.BLACKSTONE), new ItemStack(Items.COBBLED_DEEPSLATE),
        new ItemStack(Items.DIAMOND), new ItemStack(Items.EMERALD), new ItemStack(Items.DIRT), ItemStack.EMPTY
    };
    ItemStack[] right = {
        new ItemStack(Items.REDSTONE), new ItemStack(Items.AMETHYST_SHARD), new ItemStack(Items.AMETHYST_SHARD),
        new ItemStack(Items.COAL), new ItemStack(Items.COAL), new ItemStack(Items.COAL),
        new ItemStack(Items.GOLD_NUGGET), new ItemStack(Items.QUARTZ), new ItemStack(Items.COAL), ItemStack.EMPTY
    };

    RecipeHolder<AlloyRecipe> firstHolder = null;
    for (int i = 0; i < names.length; i++) {
      AlloyInput input = new AlloyInput(left[i], right[i]);
      Optional<RecipeHolder<AlloyRecipe>> found = mgr.getRecipeFor(AlloyRecipe.TYPE, input, level);
      String foundId = "empty";
      ItemStack out = ItemStack.EMPTY;
      if (found.isPresent()) {
        RecipeHolder<AlloyRecipe> h = found.get();
        if (firstHolder == null) firstHolder = h;
        foundId = h.id().identifier().toString();
        out = h.value().assemble(input);
      }
      System.out.println("query:" + names[i]
          + "|id=" + foundId
          + "|out=" + out.toString()
          + "|empty=" + out.isEmpty()
          + "|repairCost=" + out.get(DataComponents.REPAIR_COST)
          + "|hasCustomName=" + (out.get(DataComponents.CUSTOM_NAME) != null));
    }

    // 4-arg cached key and cached holder overloads + MinecraftServer.recipeAccess() / getRecipeManager()
    AlloyInput ironInput = new AlloyInput(ironWithComp, new ItemStack(Items.REDSTONE));
    Optional<RecipeHolder<AlloyRecipe>> byHintKey =
        mgr.getRecipeFor(AlloyRecipe.TYPE, ironInput, level, firstHolder.id());
    Optional<RecipeHolder<AlloyRecipe>> byHintHolder =
        mgr.getRecipeFor(AlloyRecipe.TYPE, ironInput, level, firstHolder);
    OVERLOAD_SUMMARY = byHintKey.get().id().identifier().toString()
        + "|" + byHintHolder.get().id().identifier().toString();

    RecipeManager srvAccess = level.getServer().recipeAccess();
    RecipeManager srvMgr = level.getServer().getRecipeManager();
    SERVER_SUMMARY = srvAccess.byKey(firstHolder.id()).isPresent()
        + "|" + srvMgr.byKey(firstHolder.id()).isPresent()
        + "|" + srvAccess.getRecipeFor(AlloyRecipe.TYPE, ironInput, level).get().id().identifier().toString();

    ItemStackTemplate pearl20 = new ItemStackTemplate(Items.ENDER_PEARL, 20);
    ItemStack pearlOut = TransmuteRecipe.createWithOriginalComponents(pearl20, new ItemStack(Items.STONE));
    System.out.println("pearl20=" + pearlOut.toString() + "|empty=" + pearlOut.isEmpty() + "|sameEmpty=" + (pearlOut == ItemStack.EMPTY));

    ItemStackTemplate small20 = new ItemStackTemplate(SMALL_ITEM, 20);
    ItemStack smallOut20 = TransmuteRecipe.createWithOriginalComponents(small20, new ItemStack(Items.STONE));
    System.out.println("small20=" + smallOut20.toString() + "|empty=" + smallOut20.isEmpty() + "|sameEmpty=" + (smallOut20 == ItemStack.EMPTY));

    ItemStackTemplate stick2 = new ItemStackTemplate(Items.STICK, 2);
    ItemStack stickExtra = TransmuteRecipe.createWithOriginalComponents(stick2, ironWithComp, 3);
    System.out.println("stickExtra=" + stickExtra.toString() + "|repairCost=" + stickExtra.get(DataComponents.REPAIR_COST));

    ItemStackTemplate negTpl = new ItemStackTemplate(Items.STONE, -3);
    ItemStack negOut = negTpl.create();
    System.out.println("negTpl=" + negTpl.count()
        + "|sameEmpty=" + (negOut == ItemStack.EMPTY)
        + "|empty=" + negOut.isEmpty()
        + "|count=" + negOut.getCount()
        + "|max=" + negOut.getMaxStackSize()
        + "|str=" + negOut.toString());

    ItemStack dmgStoneOut = new ItemStackTemplate(
        Items.STONE, DataComponentPatch.builder().set(DataComponents.MAX_DAMAGE, 10).build()).create();
    ItemStack stackSwordOut = new ItemStackTemplate(
        Items.DIAMOND_SWORD, DataComponentPatch.builder().set(DataComponents.MAX_STACK_SIZE, 16).build()).create();
    ItemStack validDmgStoneOut = new ItemStackTemplate(
        Items.STONE, DataComponentPatch.builder().set(DataComponents.MAX_DAMAGE, 10).set(DataComponents.MAX_STACK_SIZE, 1).build()).create();
    System.out.println("validateStrict="
        + (dmgStoneOut == ItemStack.EMPTY) + "|"
        + (stackSwordOut == ItemStack.EMPTY) + "|"
        + (validDmgStoneOut == ItemStack.EMPTY) + "|"
        + validDmgStoneOut.toString());

    // PatchedDataComponentMap exact semantics
    ItemStack pst = new ItemStack(Items.STONE, 1);
    Integer pr1 = pst.set(DataComponents.REPAIR_COST, 9);
    boolean pp1 = pst.getComponentsPatch().isEmpty();
    Integer pr2 = pst.set(DataComponents.REPAIR_COST, 0);
    boolean pp2 = pst.getComponentsPatch().isEmpty();
    Integer pr3 = pst.remove(DataComponents.REPAIR_COST);
    Integer pg3 = pst.get(DataComponents.REPAIR_COST);
    int psz3 = pst.getComponentsPatch().size();
    Integer pr4 = pst.remove(DataComponents.REPAIR_COST);
    Integer pr5 = pst.set(DataComponents.REPAIR_COST, 5);
    Component pr6 = pst.remove(DataComponents.CUSTOM_NAME);
    int pszAfterCn = pst.getComponentsPatch().size();
    pst.applyComponents(DataComponentPatch.builder().set(DataComponents.REPAIR_COST, 0).build());
    boolean ppCanonical = pst.getComponentsPatch().isEmpty();
    System.out.println("patchMap=" + pr1 + "|" + pp1 + "|" + pr2 + "|" + pp2
        + "|" + pr3 + "|" + pg3 + "|" + psz3 + "|" + pr4 + "|" + pr5
        + "|" + pr6 + "|" + pszAfterCn + "|" + ppCanonical);

    Recipe.CommonInfo ciTrue = new Recipe.CommonInfo(true);
    Recipe.CommonInfo ciFalse = new Recipe.CommonInfo(false);
    System.out.println("ci=" + ciTrue + "|" + ciFalse
        + "|eq=" + ciTrue.equals(new Recipe.CommonInfo(true)) + "|" + ciTrue.equals(ciFalse)
        + "|hash=" + ciTrue.hashCode() + "|" + ciFalse.hashCode());
    System.out.println("emptyStack=" + ItemStack.EMPTY.toString()
        + "|isAir=" + ItemStack.EMPTY.is(Items.AIR)
        + "|isStick=" + ItemStack.EMPTY.is(Items.STICK)
        + "|maxEmpty=" + ItemStack.EMPTY.getMaxStackSize()
        + "|maxZero=" + new ItemStack(Items.STONE, 0).getMaxStackSize()
        + "|maxAir=" + new ItemStack(Items.AIR, 1).getMaxStackSize()
        + "|patchEmpty=" + ItemStack.EMPTY.getComponentsPatch().isEmpty()
        + "|patchStr=" + ItemStack.EMPTY.getComponentsPatch().toString());
    System.out.println("optIngr="
        + Ingredient.testOptionalIngredient(Optional.empty(), ItemStack.EMPTY) + "|"
        + Ingredient.testOptionalIngredient(Optional.empty(), new ItemStack(Items.STONE)) + "|"
        + Ingredient.testOptionalIngredient(Optional.of(Ingredient.of(Items.STONE)), new ItemStack(Items.STONE)));

    SimpleContainer box = new SimpleContainer(2);
    ItemStack pearls = new ItemStack(Items.ENDER_PEARL, 30);
    box.setItem(0, pearls);
    ItemStack big = new ItemStack(Items.STONE, 80);
    big.set(DataComponents.MAX_STACK_SIZE, 99);
    box.setItem(1, big);
    SimpleContainer limitedBox = new SimpleContainer(2) {
      public int getMaxStackSize() { return 12; }
    };
    limitedBox.setItem(0, new ItemStack(Items.STONE, 30));
    limitedBox.setItem(1, new ItemStack(Items.ENDER_PEARL, 30));
    System.out.println("boxClamp=" + box.getItem(0).toString() + "|" + box.getItem(1).toString()
        + "|boxMax=" + box.getMaxStackSize() + "|boxEmpty=" + box.isEmpty()
        + "|lim0=" + limitedBox.getItem(0).toString() + "|lim1=" + limitedBox.getItem(1).toString()
        + "|limMax=" + limitedBox.getMaxStackSize());

    capture("ingrAir", () -> Ingredient.of(Items.AIR));
    capture("tplZero", () -> new ItemStackTemplate(Items.STONE, 0));
    capture("tplAir", () -> new ItemStackTemplate(Items.AIR, 1));
    capture("fromEmpty", () -> ItemStackTemplate.fromNonEmptyStack(ItemStack.EMPTY));

    ResourceKey<Recipe<?>> rk1 = ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:one"));
    ResourceKey<Recipe<?>> rk2 = ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:one"));
    ResourceKey<Recipe<?>> rk3 = ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:two"));
    RecipeHolder<?> h1 = new RecipeHolder<>(rk1, null);
    RecipeHolder<?> h2 = new RecipeHolder<>(rk2, null);
    RecipeHolder<?> h3 = new RecipeHolder<>(rk3, null);
    System.out.println("holder=" + h1.toString() + "|eqSame=" + h1.equals(h2) + "|eqDiff=" + h1.equals(h3));

    // RecipeInput.isEmpty() precheck + ResourceKey registry validation
    ResourceKey<Recipe<?>> alwaysKey = ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:always"));
    ResourceKey<Item> wrongKey = ResourceKey.create(Registries.ITEM, Identifier.parse("demo:always"));
    RecipeInput emptyIn = new RecipeInput() {
      public ItemStack getItem(int idx) { return ItemStack.EMPTY; }
      public int size() { return 1; }
    };
    boolean r3 = mgr.getRecipeFor(AlwaysRecipe.TYPE, emptyIn, level).isPresent();
    int c3 = AlwaysRecipe.matchesCalls;
    boolean r4key = mgr.getRecipeFor(AlwaysRecipe.TYPE, emptyIn, level, alwaysKey).isPresent();
    int c4key = AlwaysRecipe.matchesCalls;
    @SuppressWarnings("unchecked")
    boolean r4wrong = mgr.getRecipeFor(AlwaysRecipe.TYPE, emptyIn, level, (ResourceKey<Recipe<?>>) (ResourceKey<?>) wrongKey).isPresent();
    int c4wrong = AlwaysRecipe.matchesCalls;
    @SuppressWarnings("unchecked")
    boolean byWrong = mgr.byKey((ResourceKey<Recipe<?>>) (ResourceKey<?>) wrongKey).isPresent();
    boolean byRight = mgr.byKey(alwaysKey).isPresent();
    System.out.println("mgrChecks=" + r3 + "|" + c3 + "|" + r4key + "|" + c4key + "|" + r4wrong + "|" + c4wrong + "|" + byWrong + "|" + byRight);

    // Virtual RecipeInput.isEmpty() override runs even when slot 0 is non-empty
    int callsBeforeVirt = AlwaysRecipe.matchesCalls;
    RecipeInput overrideEmptyIn = new RecipeInput() {
      public ItemStack getItem(int idx) { return new ItemStack(Items.STONE, 1); }
      public int size() { return 1; }
      public boolean isEmpty() { return true; }
    };
    boolean virtFound = mgr.getRecipeFor(AlwaysRecipe.TYPE, overrideEmptyIn, level).isPresent();
    VIRTUAL_IS_EMPTY_SUMMARY = virtFound + "|" + (AlwaysRecipe.matchesCalls - callsBeforeVirt);

    // Drive mod ItemCombinerMenu.createResult via inputSlots.setItem
    AlloyMenu menu = new AlloyMenu(1, new Inventory(player, null));
    menu.setInputs(new ItemStack(Items.COBBLESTONE, 1), new ItemStack(Items.COAL, 1));
    String hitRes = menu.resultItem().toString()
        + "|used=" + menu.usedRecipe().id().identifier().toString()
        + "|inputsEmpty=" + menu.inputsEmpty()
        + "|resultEmpty=" + menu.resultEmpty()
        + "|calls=" + menu.createResultCalls;
    menu.setInputs(new ItemStack(Items.DIRT, 1), new ItemStack(Items.COAL, 1));
    String missRes = menu.resultItem().toString()
        + "|used=" + menu.usedRecipe()
        + "|resultEmpty=" + menu.resultEmpty();
    MENU_SUMMARY = hitRes + "||" + missRes;

    // Bare RecipeManager remains counted.
    new RecipeManager().getRecipeFor();
  }
}
"#;

fn fire_use_item(jar: &horizon_native_java::NativeJar, host: &mut StdHost) {
    let listeners = host.game_listeners(jar, true);
    assert_eq!(listeners.listeners.len(), 1);
    let player_uuid = 0x1111_2222_3333_4444_5555_6666_7777_8888;
    let facade = Arc::new(EventFacade {
        players: vec![FacadePlayer {
            uuid: player_uuid,
            name: "Tester".into(),
            level: Some("minecraft:overworld".into()),
            entity_id: Some(42),
            session: 1,
            life: 0,
        }],
        levels: vec![FacadeLevel {
            id: "minecraft:overworld".into(),
            ..FacadeLevel::default()
        }],
        ..EventFacade::default()
    });
    let mut firing = EventFiring::new(
        GameEvent::UseItem,
        facade,
        EventContext {
            level: Some("minecraft:overworld".into()),
            player: Some(player_uuid),
            off_hand: Some(false),
            ..EventContext::default()
        },
    );
    jar.run_game_listener(
        &listeners.listeners[0],
        &mut firing,
        host,
        &ExecutionBudget::UNBOUNDED,
    )
    .unwrap_or_else(|err| panic!("listener failed: {err:?}"));
}

#[test]
fn general_recipe_codec_matching_transmute_and_menu_match_mc263_oracle() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/AlloyInput.java", DEMO_RECIPE_INPUT));
    sources.push(("demo/AlloyRecipe.java", DEMO_RECIPE));
    sources.push(("demo/AlwaysRecipe.java", DEMO_ALWAYS_RECIPE));
    sources.push(("demo/AlloyMenu.java", DEMO_MENU));
    sources.push(("demo/Probe.java", DEMO_PROBE));

    let extra: &[(&str, &[u8])] = &[
        (
            "data/demo/tags/item/reagents.json",
            br##"{"values":["#minecraft:stone_tool_materials"]}"##,
        ),
        (
            "data/demo/recipe/always.json",
            br#"{"type":"demo:always"}"#,
        ),
        (
            "data/demo/recipe/iron_alloy.json",
            br#"{"type":"demo:alloy","base":"minecraft:iron_ingot","catalyst":"minecraft:redstone","result":{"id":"demo:alloy_ingot","count":4},"tier":2,"label":"basic"}"#,
        ),
        (
            "data/demo/recipe/precious_alloy.json",
            br#"{"type":"demo:alloy","show_notification":false,"base":["minecraft:gold_ingot","minecraft:copper_ingot"],"catalyst":"minecraft:amethyst_shard","result":"demo:alloy_ingot"}"#,
        ),
        (
            "data/demo/recipe/stone_alloy.json",
            br##"{"type":"demo:alloy","base":"#demo:reagents","catalyst":"minecraft:coal","result":{"id":"minecraft:stick","count":2,"components":{"minecraft:repair_cost":7}}}"##,
        ),
        (
            "data/demo/recipe/small_stack.json",
            br#"{"type":"demo:alloy","base":"minecraft:diamond","catalyst":"minecraft:gold_nugget","result":{"id":"demo:small_stack_item","count":16}}"#,
        ),
        (
            "data/demo/recipe/dfu_coercion.json",
            br#"{"type":"demo:alloy","show_notification":null,"base":"minecraft:emerald","catalyst":"minecraft:quartz","result":{"id":"demo:alloy_ingot","count":3.7,"components":null},"tier":2.9,"label":null,"weight":10.8}"#,
        ),
        (
            "data/demo/recipe/bad_air.json",
            br#"{"type":"demo:alloy","base":"minecraft:air","catalyst":"minecraft:coal","result":"demo:alloy_ingot"}"#,
        ),
        (
            "data/demo/recipe/bad_empty_list.json",
            br#"{"type":"demo:alloy","base":[],"catalyst":"minecraft:coal","result":"demo:alloy_ingot"}"#,
        ),
        (
            "data/demo/recipe/bad_missing_tag.json",
            br##"{"type":"demo:alloy","base":"#demo:no_such_tag","catalyst":"minecraft:coal","result":"demo:alloy_ingot"}"##,
        ),
        (
            "data/demo/recipe/bad_count.json",
            br#"{"type":"demo:alloy","base":"minecraft:iron_ingot","catalyst":"minecraft:coal","result":{"id":"demo:alloy_ingot","count":100}}"#,
        ),
        (
            "data/demo/recipe/bad_result_air.json",
            br#"{"type":"demo:alloy","base":"minecraft:iron_ingot","catalyst":"minecraft:coal","result":{"id":"minecraft:air","count":1}}"#,
        ),
        (
            "data/demo/recipe/bad_leading_space.json",
            br#"{"type":"demo:alloy","base":" minecraft:iron_ingot","catalyst":"minecraft:coal","result":"demo:alloy_ingot"}"#,
        ),
        (
            "data/demo/recipe/bad_trailing_space.json",
            br#"{"type":"demo:alloy","base":"minecraft:iron_ingot","catalyst":"minecraft:coal ","result":"demo:alloy_ingot"}"#,
        ),
        (
            "data/demo/recipe/bad_null_base.json",
            br#"{"type":"demo:alloy","base":null,"catalyst":"minecraft:coal","result":"demo:alloy_ingot"}"#,
        ),
        (
            "data/demo/recipe/bad_count_float_hi.json",
            br#"{"type":"demo:alloy","base":"minecraft:iron_ingot","catalyst":"minecraft:coal","result":{"id":"demo:alloy_ingot","count":100.2}}"#,
        ),
        (
            "data/demo/recipe/bad_count_float_lo.json",
            br#"{"type":"demo:alloy","base":"minecraft:iron_ingot","catalyst":"minecraft:coal","result":{"id":"demo:alloy_ingot","count":0.9}}"#,
        ),
        (
            "data/demo/recipe/bad_multi.json",
            br#"{"type":"demo:alloy","base":"minecraft:air","catalyst":"minecraft:coal","result":"demo:alloy_ingot","tier":"bad_int","label":"still_runs","weight":0.5}"#,
        ),
        (
            "data/demo/recipe/vanilla_shaped.json",
            br#"{"type":"minecraft:crafting_shaped","pattern":["X"],"key":{"X":"minecraft:stick"},"result":{"id":"minecraft:stick"}}"#,
        ),
    ];

    let fixture = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "recipes",
        &sources,
        r#"{"id":"demo","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        extra,
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture
        .jar
        .execute("demo.Probe", None, "main", &mut host)
        .unwrap_or_else(|err| panic!("onInitialize failed: {err:?}"));
    fire_use_item(&fixture.jar, &mut host);

    assert_eq!(
        host.take_stdout(),
        include_str!("fixtures/recipes/mc263.txt")
    );
    assert_eq!(
        host.loaded_recipe_ids(),
        vec![
            "demo:always",
            "demo:dfu_coercion",
            "demo:iron_alloy",
            "demo:precious_alloy",
            "demo:small_stack",
            "demo:stone_alloy",
        ]
    );
    assert_eq!(
        host.recipe_decode_errors().len(),
        11,
        "errors={:?}",
        host.recipe_decode_errors()
    );
    // RecordCodecBuilder accumulated all 3 errors on demo:bad_multi in field order.
    let multi_err = host
        .recipe_decode_errors()
        .iter()
        .find(|(id, _)| id == "demo:bad_multi")
        .map(|(_, msg)| msg.as_str())
        .expect("demo:bad_multi decode error");
    assert!(
        multi_err.contains("air") && multi_err.contains("bad_int") && multi_err.contains("[1;99]"),
        "expected all 3 accumulated field errors in demo:bad_multi, got: {multi_err}"
    );

    let statics = host.persistent_class_state().0;
    assert_eq!(
        statics.get("demo/Probe#OVERLOAD_SUMMARYLjava/lang/String;"),
        Some(&horizon_native_java::Value::Ref(Some(
            "demo:iron_alloy|demo:iron_alloy".into()
        )))
    );
    assert_eq!(
        statics.get("demo/Probe#SERVER_SUMMARYLjava/lang/String;"),
        Some(&horizon_native_java::Value::Ref(Some(
            "true|true|demo:iron_alloy".into()
        )))
    );
    assert_eq!(
        statics.get("demo/Probe#VIRTUAL_IS_EMPTY_SUMMARYLjava/lang/String;"),
        Some(&horizon_native_java::Value::Ref(Some("false|0".into())))
    );
    assert_eq!(
        statics.get("demo/Probe#MENU_SUMMARYLjava/lang/String;"),
        Some(&horizon_native_java::Value::Ref(Some(
            "2 minecraft:stick|used=demo:stone_alloy|inputsEmpty=false|resultEmpty=false|calls=2||0 minecraft:air|used=null|resultEmpty=true"
                .into()
        )))
    );
    // Only the intentional bare `new RecipeManager().getRecipeFor()` was counted.
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "net/minecraft/world/item/crafting/RecipeManager#getRecipeFor()Ljava/lang/Object;"
                .to_owned(),
            1
        )]
    );
}

#[test]
fn counted_inner_calls_reject_decode_matches_and_container_limit() {
    // 1. Counted inner call inside an xmap callback rejects decode, counts decodeRecipe, and keeps byKey counted.
    {
        let mut sources = STUBS.to_vec();
        sources.push((
            "demo/XmapCountedProbe.java",
            r#"
package demo;
import com.mojang.serialization.*;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.Level;

public final class XmapCountedProbe {
  static final class XmapRecipe implements Recipe<RecipeInput> {
    static final RecipeType<XmapRecipe> TYPE = new RecipeType<>() {};
    static final MapCodec<XmapRecipe> MAP_CODEC = RecordCodecBuilder.mapCodec(i -> i.group(
        Codec.STRING.fieldOf("tag").xmap(s -> {
          new RecipeManager().getRecipeFor();
          return s;
        }, s -> s).forGetter(r -> r.tag)
    ).apply(i, XmapRecipe::new));
    static final RecipeSerializer<XmapRecipe> SERIALIZER = new RecipeSerializer<>(MAP_CODEC, null);
    final String tag;
    XmapRecipe(String tag) { this.tag = tag; }
    public boolean matches(RecipeInput in, Level lvl) { return true; }
    public ItemStack assemble(RecipeInput in) { return new ItemStack(Items.STICK); }
    public RecipeSerializer<XmapRecipe> getSerializer() { return SERIALIZER; }
    public RecipeType<XmapRecipe> getType() { return TYPE; }
  }

  public static void onInitialize() {
    Registry.register(BuiltInRegistries.RECIPE_TYPE, Identifier.parse("demo:xmap_dirty"), XmapRecipe.TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, Identifier.parse("demo:xmap_dirty"), XmapRecipe.SERIALIZER);
    UseItemCallback.EVENT.register((player, level, hand) -> {
      RecipeManager mgr = ((ServerLevel) level).recipeAccess();
      mgr.byKey(ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:bad_xmap")));
      return InteractionResult.PASS;
    });
  }
}
"#,
        ));
        let fixture = javac_fixture::Fixture::build_with_mod_json_and_extra(
            "recipes-counted-xmap",
            &sources,
            r#"{"id":"demo","version":"1.0","entrypoints":{"main":"demo.XmapCountedProbe"}}"#,
            &[(
                "data/demo/recipe/bad_xmap.json",
                br#"{"type":"demo:xmap_dirty","tag":"hello"}"#,
            )],
        );
        let mut host = fixture.jar.seed_host(StdHost::new());
        fixture
            .jar
            .execute("demo.XmapCountedProbe", None, "main", &mut host)
            .unwrap();
        fire_use_item(&fixture.jar, &mut host);
        assert!(host.loaded_recipe_ids().is_empty());
        let unknowns: BTreeMap<String, usize> = host.unknown_host_calls().into_iter().collect();
        assert_eq!(
            unknowns.get(
                "net/minecraft/world/item/crafting/RecipeManager#decodeRecipe(demo:xmap_dirty)V"
            ),
            Some(&1),
            "unknowns={unknowns:?}"
        );
        assert_eq!(
            unknowns.get(
                "net/minecraft/world/item/crafting/RecipeManager#byKey(Lnet/minecraft/resources/ResourceKey;)Ljava/util/Optional;"
            ),
            Some(&1),
            "unknowns={unknowns:?}"
        );
    }

    // 2. Counted inner call inside Recipe.matches() rejects match & counts getRecipeFor,
    //    and counted inner call inside SimpleContainer.getMaxStackSize() override counts setItem.
    {
        let mut sources = STUBS.to_vec();
        sources.push((
            "demo/MatchAndBoxCountedProbe.java",
            r#"
package demo;
import com.mojang.serialization.MapCodec;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.Level;

public final class MatchAndBoxCountedProbe {
  public static int matchesEntered = 0;
  public static int boxMaxEntered = 0;

  static final class DirtyMatchRecipe implements Recipe<RecipeInput> {
    static final RecipeType<DirtyMatchRecipe> TYPE = new RecipeType<>() {};
    static final MapCodec<DirtyMatchRecipe> MAP_CODEC = MapCodec.unit(new DirtyMatchRecipe());
    static final RecipeSerializer<DirtyMatchRecipe> SERIALIZER = new RecipeSerializer<>(MAP_CODEC, null);
    public boolean matches(RecipeInput in, Level lvl) {
      matchesEntered++;
      new RecipeManager().getRecipeFor();
      return true;
    }
    public ItemStack assemble(RecipeInput in) { return new ItemStack(Items.STICK); }
    public RecipeSerializer<DirtyMatchRecipe> getSerializer() { return SERIALIZER; }
    public RecipeType<DirtyMatchRecipe> getType() { return TYPE; }
  }

  public static void onInitialize() {
    Registry.register(BuiltInRegistries.RECIPE_TYPE, Identifier.parse("demo:match_dirty"), DirtyMatchRecipe.TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, Identifier.parse("demo:match_dirty"), DirtyMatchRecipe.SERIALIZER);
    UseItemCallback.EVENT.register((player, level, hand) -> {
      RecipeManager mgr = ((ServerLevel) level).recipeAccess();
      RecipeInput nonEmpty = new RecipeInput() {
        public ItemStack getItem(int i) { return new ItemStack(Items.STONE); }
        public int size() { return 1; }
      };
      mgr.getRecipeFor(DirtyMatchRecipe.TYPE, nonEmpty, level);
      SimpleContainer dirtyBox = new SimpleContainer(1) {
        public int getMaxStackSize() {
          boxMaxEntered++;
          new RecipeManager().getRecipeFor();
          return 12;
        }
      };
      dirtyBox.setItem(0, new ItemStack(Items.STONE, 20));
      return InteractionResult.PASS;
    });
  }
}
"#,
        ));
        let fixture = javac_fixture::Fixture::build_with_mod_json_and_extra(
            "recipes-counted-match-box",
            &sources,
            r#"{"id":"demo","version":"1.0","entrypoints":{"main":"demo.MatchAndBoxCountedProbe"}}"#,
            &[(
                "data/demo/recipe/dirty_match.json",
                br#"{"type":"demo:match_dirty"}"#,
            )],
        );
        let mut host = fixture.jar.seed_host(StdHost::new());
        fixture
            .jar
            .execute("demo.MatchAndBoxCountedProbe", None, "main", &mut host)
            .unwrap();
        fire_use_item(&fixture.jar, &mut host);

        assert_eq!(host.loaded_recipe_ids(), vec!["demo:dirty_match"]);
        let statics = host.persistent_class_state().0;
        assert_eq!(
            statics.get("demo/MatchAndBoxCountedProbe#matchesEnteredI"),
            Some(&horizon_native_java::Value::Int(1)),
            "DirtyMatchRecipe.matches must actually execute before being rejected"
        );
        assert_eq!(
            statics.get("demo/MatchAndBoxCountedProbe#boxMaxEnteredI"),
            Some(&horizon_native_java::Value::Int(1)),
            "SimpleContainer.getMaxStackSize override must actually execute before being rejected"
        );
        let unknowns: BTreeMap<String, usize> = host.unknown_host_calls().into_iter().collect();
        assert_eq!(
            unknowns.get(
                "net/minecraft/world/item/crafting/RecipeManager#getRecipeFor(Lnet/minecraft/world/item/crafting/RecipeType;Lnet/minecraft/world/item/crafting/RecipeInput;Lnet/minecraft/world/level/Level;)Ljava/util/Optional;"
            ),
            Some(&1),
            "expected getRecipeFor with counted inner matches to be counted, got {unknowns:?}"
        );
        assert_eq!(
            unknowns.get(
                "net/minecraft/world/SimpleContainer#setItem(ILnet/minecraft/world/item/ItemStack;)V"
            ),
            Some(&1),
            "expected SimpleContainer.setItem with counted getMaxStackSize override to be counted, got {unknowns:?}"
        );
    }
}

#[test]
fn vanilla_and_unregistered_recipe_types_and_skipped_keys_stay_counted() {
    let mut sources = STUBS.to_vec();
    sources.push((
        "demo/VanillaCountedProbe.java",
        r#"
package demo;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;

public final class VanillaCountedProbe {
  static final RecipeType<Recipe<RecipeInput>> UNREGISTERED = new RecipeType<>() {};

  public static void onInitialize() {
    UseItemCallback.EVENT.register((player, level, hand) -> {
      RecipeManager mgr = ((ServerLevel) level).recipeAccess();
      RecipeInput in = new RecipeInput() {
        public ItemStack getItem(int i) { return new ItemStack(Items.STICK); }
        public int size() { return 1; }
      };
      // 1. Vanilla RecipeType.CRAFTING -> counted (never uncounted empty).
      mgr.getRecipeFor(RecipeType.CRAFTING, in, level);
      // 2. Unregistered RecipeType -> counted.
      mgr.getRecipeFor(UNREGISTERED, in, level);
      // 3. Vanilla minecraft:* recipe id in byKey -> counted.
      mgr.byKey(ResourceKey.create(Registries.RECIPE, Identifier.parse("minecraft:stick")));
      // 4. Mod recipe using skipped vanilla serializer minecraft:crafting_shaped -> counted.
      mgr.byKey(ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:vanilla_shaped")));
      return InteractionResult.PASS;
    });
  }
}
"#,
    ));

    let extra: &[(&str, &[u8])] = &[(
        "data/demo/recipe/vanilla_shaped.json",
        br#"{"type":"minecraft:crafting_shaped","pattern":["X"],"key":{"X":"minecraft:stick"},"result":{"id":"minecraft:stick"}}"#,
    )];

    let fixture = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "recipes-vanilla-counted",
        &sources,
        r#"{"id":"demo","version":"1.0","entrypoints":{"main":"demo.VanillaCountedProbe"}}"#,
        extra,
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture
        .jar
        .execute("demo.VanillaCountedProbe", None, "main", &mut host)
        .unwrap();
    fire_use_item(&fixture.jar, &mut host);

    let unknowns: BTreeMap<String, usize> = host.unknown_host_calls().into_iter().collect();
    assert_eq!(
        unknowns.get(
            "net/minecraft/world/item/crafting/RecipeManager#getRecipeFor(Lnet/minecraft/world/item/crafting/RecipeType;Lnet/minecraft/world/item/crafting/RecipeInput;Lnet/minecraft/world/level/Level;)Ljava/util/Optional;"
        ),
        Some(&2),
        "expected both vanilla and unregistered getRecipeFor to be counted, got {unknowns:?}"
    );
    assert_eq!(
        unknowns.get(
            "net/minecraft/world/item/crafting/RecipeManager#byKey(Lnet/minecraft/resources/ResourceKey;)Ljava/util/Optional;"
        ),
        Some(&2),
        "expected both minecraft:stick and skipped demo:vanilla_shaped byKey to be counted, got {unknowns:?}"
    );
}

#[test]
fn multi_jar_and_datapack_layers_override_recipes_and_tags_in_pack_order() {
    let mut base_sources = STUBS.to_vec();
    base_sources.push(("demo/AlloyInput.java", DEMO_RECIPE_INPUT));
    base_sources.push(("demo/AlloyRecipe.java", DEMO_RECIPE));
    base_sources.push((
        "demo/LayerProbe.java",
        r#"
package demo;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;

public final class LayerProbe {
  public static String SUMMARY;
  public static void onInitialize() {
    Identifier id = Identifier.parse("demo:alloy");
    Registry.register(BuiltInRegistries.RECIPE_TYPE, id, AlloyRecipe.TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, id, AlloyRecipe.SERIALIZER);
    UseItemCallback.EVENT.register((player, level, hand) -> {
      RecipeManager mgr = ((ServerLevel) level).recipeAccess();
      AlloyRecipe layered = (AlloyRecipe) mgr.byKey(
          ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:layered"))).get().value();
      boolean iron = mgr.getRecipeFor(AlloyRecipe.TYPE,
          new AlloyInput(new ItemStack(Items.IRON_INGOT), new ItemStack(Items.COAL)), level).isPresent();
      boolean gold = mgr.getRecipeFor(AlloyRecipe.TYPE,
          new AlloyInput(new ItemStack(Items.GOLD_INGOT), new ItemStack(Items.COAL)), level).isPresent();
      boolean diamond = mgr.getRecipeFor(AlloyRecipe.TYPE,
          new AlloyInput(new ItemStack(Items.DIAMOND), new ItemStack(Items.COAL)), level).isPresent();
      SUMMARY = "tier=" + layered.tier + "|iron=" + iron + "|gold=" + gold + "|diamond=" + diamond;
      return InteractionResult.PASS;
    });
  }
}
"#,
    ));

    let base_extra: &[(&str, &[u8])] = &[
        (
            "data/demo/tags/item/layer_tag.json",
            br#"{"values":["minecraft:iron_ingot"]}"#,
        ),
        (
            "data/demo/recipe/layered.json",
            br##"{"type":"demo:alloy","base":"#demo:layer_tag","catalyst":"minecraft:coal","result":"minecraft:stick","tier":1}"##,
        ),
        (
            "data/demo/recipe/base_only.json",
            br#"{"type":"demo:alloy","base":"minecraft:emerald","catalyst":"minecraft:coal","result":"minecraft:stick","tier":1}"#,
        ),
    ];
    let base_fixture = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "recipes-layer-base",
        &base_sources,
        r#"{"id":"base_mod","version":"1.0","entrypoints":{"main":"demo.LayerProbe"}}"#,
        base_extra,
    );

    // Second linked jar appends gold_ingot to tag and overrides demo:layered to tier 2.
    let override_extra: &[(&str, &[u8])] = &[
        (
            "data/demo/tags/item/layer_tag.json",
            br#"{"values":["minecraft:gold_ingot"]}"#,
        ),
        (
            "data/demo/recipe/layered.json",
            br##"{"type":"demo:alloy","base":"#demo:layer_tag","catalyst":"minecraft:coal","result":"minecraft:stick","tier":2}"##,
        ),
    ];
    let override_fixture = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "recipes-layer-override",
        &[("dummy/Noop.java", "package dummy; public class Noop {}")],
        r#"{"id":"override_mod","version":"1.0"}"#,
        override_extra,
    );

    // Link BOTH jars without calling seed_host: recipes and tags load from linked jar layers in order.
    let mut host = StdHost::new();
    host.link_jar("base_mod", &base_fixture.jar).unwrap();
    host.link_jar("override_mod", &override_fixture.jar)
        .unwrap();
    base_fixture
        .jar
        .execute("demo.LayerProbe", None, "main", &mut host)
        .unwrap();
    fire_use_item(&base_fixture.jar, &mut host);

    let statics = host.persistent_class_state().0;
    assert_eq!(
        statics.get("demo/LayerProbe#SUMMARYLjava/lang/String;"),
        Some(&horizon_native_java::Value::Ref(Some(
            "tier=2|iron=true|gold=true|diamond=false".into()
        )))
    );
    assert_eq!(
        host.loaded_recipe_sources(),
        vec![
            ("demo:base_only".to_owned(), "base_mod".to_owned()),
            ("demo:layered".to_owned(), "override_mod".to_owned()),
        ]
    );

    // Adding a higher-priority datapack layer with `replace: true` + recipe override invalidates cache.
    let mut dp = BTreeMap::new();
    dp.insert(
        "data/demo/tags/item/layer_tag.json".to_owned(),
        br#"{"replace":true,"values":["minecraft:diamond"]}"#.to_vec(),
    );
    dp.insert(
        "data/demo/recipe/layered.json".to_owned(),
        br##"{"type":"demo:alloy","base":"#demo:layer_tag","catalyst":"minecraft:coal","result":"minecraft:stick","tier":3}"##
            .to_vec(),
    );
    dp.insert(
        "data/demo/recipe/dp_only.json".to_owned(),
        br#"{"type":"demo:alloy","base":"minecraft:quartz","catalyst":"minecraft:coal","result":"minecraft:stick","tier":4}"#
            .to_vec(),
    );
    host.add_data_pack_layer("world/datapacks/override", dp);
    fire_use_item(&base_fixture.jar, &mut host);

    let statics = host.persistent_class_state().0;
    assert_eq!(
        statics.get("demo/LayerProbe#SUMMARYLjava/lang/String;"),
        Some(&horizon_native_java::Value::Ref(Some(
            "tier=3|iron=false|gold=false|diamond=true".into()
        )))
    );
    assert_eq!(
        host.loaded_recipe_sources(),
        vec![
            ("demo:base_only".to_owned(), "base_mod".to_owned()),
            (
                "demo:dp_only".to_owned(),
                "world/datapacks/override".to_owned()
            ),
            (
                "demo:layered".to_owned(),
                "world/datapacks/override".to_owned()
            ),
        ]
    );
    assert!(host.unknown_host_calls().is_empty());
}

#[test]
fn unmodelled_component_prototypes_and_container_validate_strict_stay_counted() {
    let mut sources = STUBS.to_vec();
    sources.push((
        "demo/ComponentCountedProbe.java",
        r#"
package demo;
import net.minecraft.core.component.*;
import net.minecraft.world.item.*;

public final class ComponentCountedProbe {
  public static void onInitialize() {
    ItemStack stack = new ItemStack(Items.STONE, 1);
    // Unmodelled prototype components (enchantment_glint_override is non-null on some vanilla items,
    // rarity has non-null prototypes on every item): must stay counted, never assumed null.
    stack.get(DataComponents.ENCHANTMENT_GLINT_OVERRIDE);
    stack.set(DataComponents.ENCHANTMENT_GLINT_OVERRIDE, Boolean.TRUE);
    stack.remove(DataComponents.RARITY);
    stack.applyComponents(DataComponentPatch.builder().remove(DataComponents.RARITY).build());
    // ItemStack.validateStrict inspects container/bundle_contents/charged_projectiles:
    // patching minecraft:container on an ItemStackTemplate must keep create() counted.
    new ItemStackTemplate(
        Items.STONE,
        DataComponentPatch.builder().set(DataComponents.CONTAINER, new Object()).build()
    ).create();
  }
}
"#,
    ));

    let fixture = javac_fixture::Fixture::build_with_mod_json(
        "recipes-component-counted",
        &sources,
        r#"{"id":"demo","version":"1.0","entrypoints":{"main":"demo.ComponentCountedProbe"}}"#,
    );
    let mut host = StdHost::new();
    fixture
        .jar
        .execute("demo.ComponentCountedProbe", None, "main", &mut host)
        .unwrap();

    let unknowns: BTreeMap<String, usize> = host.unknown_host_calls().into_iter().collect();
    assert_eq!(
        unknowns.get("net/minecraft/world/item/ItemStack#get(Lnet/minecraft/core/component/DataComponentType;)Ljava/lang/Object;"),
        Some(&1),
        "unknowns={unknowns:?}"
    );
    assert_eq!(
        unknowns.get("net/minecraft/world/item/ItemStack#set(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Ljava/lang/Object;"),
        Some(&1),
        "unknowns={unknowns:?}"
    );
    assert_eq!(
        unknowns.get("net/minecraft/world/item/ItemStack#remove(Lnet/minecraft/core/component/DataComponentType;)Ljava/lang/Object;"),
        Some(&1),
        "unknowns={unknowns:?}"
    );
    assert_eq!(
        unknowns.get("net/minecraft/world/item/ItemStack#applyComponents(Lnet/minecraft/core/component/DataComponentPatch;)V"),
        Some(&1),
        "unknowns={unknowns:?}"
    );
    assert_eq!(
        unknowns.get("net/minecraft/world/item/ItemStackTemplate#create()Lnet/minecraft/world/item/ItemStack;"),
        Some(&1),
        "unknowns={unknowns:?}"
    );
}

fn pack_in_memory_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buf.into_inner()
}

/// Three-mod + nested-JiJ + world-datapack fixture where link order, primary-jar status,
/// alphabetical mod-id order, and `fabric:resource_load_order` constraints all disagree.
/// Verified against the real Fabric 26.3 (`fabric-resource-loader-v1` 3.1.0 + `server-26.3.jar`)
/// server capture in `tests/fixtures/recipes/fabric263_pack_order.txt`.
#[test]
fn fabric_pack_order_matches_real_fabric_263_oracle() {
    const FABRIC_ORACLE: &str = include_str!("fixtures/recipes/fabric263_pack_order.txt");
    let oracle: BTreeMap<&str, &str> = FABRIC_ORACLE
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();

    let mut probe_sources = STUBS.to_vec();
    probe_sources.push(("demo/AlloyInput.java", DEMO_RECIPE_INPUT));
    probe_sources.push(("demo/AlloyRecipe.java", DEMO_RECIPE));
    probe_sources.push((
        "demo/PackOrderProbe.java",
        r#"
package demo;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;

public final class PackOrderProbe {
  public static String SUMMARY;
  private static int recipeTier(RecipeManager mgr, String id) {
    AlloyRecipe r = (AlloyRecipe) mgr.byKey(
        ResourceKey.create(Registries.RECIPE, Identifier.parse(id))).get().value();
    return r.tier;
  }
  private static boolean tagMatches(RecipeManager mgr, String id, Item item) {
    AlloyRecipe r = (AlloyRecipe) mgr.byKey(
        ResourceKey.create(Registries.RECIPE, Identifier.parse(id))).get().value();
    return r.matches(new AlloyInput(new ItemStack(item), new ItemStack(Items.COAL)), null);
  }
  public static void onInitialize() {
    Identifier id = Identifier.parse("demo:alloy");
    Registry.register(BuiltInRegistries.RECIPE_TYPE, id, AlloyRecipe.TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, id, AlloyRecipe.SERIALIZER);
    UseItemCallback.EVENT.register((player, level, hand) -> {
      RecipeManager mgr = ((ServerLevel) level).recipeAccess();
      // For shared_before: mod_a adds iron_ingot, mod_b replaces with gold_ingot, mod_c appends diamond.
      // Because real Fabric 26.3 applies [mod_a, mod_b, mod_c], mod_b's replace:true wipes iron_ingot
      // and mod_c appends diamond -> iron=false, gold=true, diamond=true!
      boolean beforeIron = tagMatches(mgr, "demo:shared_before", Items.IRON_INGOT);
      boolean beforeGold = tagMatches(mgr, "demo:shared_before", Items.GOLD_INGOT);
      boolean beforeDiamond = tagMatches(mgr, "demo:shared_before", Items.DIAMOND);
      // For shared_after: mod_x adds iron, mod_y replaces with gold, mod_z appends diamond.
      boolean afterIron = tagMatches(mgr, "demo:shared_after", Items.IRON_INGOT);
      boolean afterGold = tagMatches(mgr, "demo:shared_after", Items.GOLD_INGOT);
      boolean afterDiamond = tagMatches(mgr, "demo:shared_after", Items.DIAMOND);
      // For shared_nested: mod_a adds iron, mod_c adds diamond, nested_aaa replaces with quartz,
      // nested_zzz appends emerald -> iron=false, diamond=false, quartz=true, emerald=true!
      boolean nestedIron = tagMatches(mgr, "demo:shared_nested", Items.IRON_INGOT);
      boolean nestedQuartz = tagMatches(mgr, "demo:shared_nested", Items.QUARTZ);
      boolean nestedEmerald = tagMatches(mgr, "demo:shared_nested", Items.EMERALD);
      // For shared_dp: file/dp_override replaces with coal -> iron=false, diamond=false, coal=true!
      boolean dpDiamond = tagMatches(mgr, "demo:shared_dp", Items.DIAMOND);
      boolean dpCoal = tagMatches(mgr, "demo:shared_dp", Items.COAL);
      SUMMARY = "before=" + recipeTier(mgr, "demo:shared_before")
          + "(" + beforeIron + "," + beforeGold + "," + beforeDiamond + ")"
          + "|after=" + recipeTier(mgr, "demo:shared_after")
          + "(" + afterIron + "," + afterGold + "," + afterDiamond + ")"
          + "|nested=" + recipeTier(mgr, "demo:shared_nested")
          + "(" + nestedIron + "," + nestedQuartz + "," + nestedEmerald + ")"
          + "|dp=" + recipeTier(mgr, "demo:shared_dp")
          + "(" + dpDiamond + "," + dpCoal + ")";
      return InteractionResult.PASS;
    });
  }
}
"#,
    ));

    let nested_zzz_bytes = pack_in_memory_zip(&[
        (
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"nested_zzz","version":"1.0.0"}"#,
        ),
        (
            "data/demo/recipe/shared_nested.json",
            br##"{"type":"demo:alloy","base":"#demo:shared_nested","catalyst":"minecraft:coal","result":"minecraft:stick","tier":40}"##,
        ),
        (
            "data/demo/tags/item/shared_nested.json",
            br#"{"values":["minecraft:emerald"]}"#,
        ),
    ]);
    let nested_aaa_bytes = pack_in_memory_zip(&[
        (
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"nested_aaa","version":"1.0.0"}"#,
        ),
        (
            "data/demo/recipe/shared_nested.json",
            br##"{"type":"demo:alloy","base":"#demo:shared_nested","catalyst":"minecraft:coal","result":"minecraft:stick","tier":30}"##,
        ),
        (
            "data/demo/tags/item/shared_nested.json",
            br#"{"replace":true,"values":["minecraft:quartz"]}"#,
        ),
    ]);

    // Primary seeded jar is `mod_b` (so naive "primary jar wins" would wrongly pick mod_b!).
    let mod_b = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "pack-order-mod-b",
        &probe_sources,
        r#"{"schemaVersion":1,"id":"mod_b","version":"1.0.0","entrypoints":{"main":"demo.PackOrderProbe"}}"#,
        &[
            (
                "data/demo/recipe/shared_before.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_before","catalyst":"minecraft:coal","result":"minecraft:stick","tier":2}"##,
            ),
            (
                "data/demo/tags/item/shared_before.json",
                br#"{"replace":true,"values":["minecraft:gold_ingot"]}"#,
            ),
        ],
    );

    // `mod_a` declares `before: ["mod_c"]` (so Stage-1 ModPackResourcesSorter puts mod_a AFTER mod_c)
    // and bundles `nested_zzz` (which sits ABOVE mod_a and mod_c in real Fabric 26.3!).
    let mod_a = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "pack-order-mod-a",
        &[("dummy/A.java", "package dummy; public class A {}")],
        r#"{"schemaVersion":1,"id":"mod_a","version":"1.0.0","jars":[{"file":"META-INF/jars/nested_zzz.jar"}],"custom":{"fabric:resource_load_order":{"before":["mod_c"]}}}"#,
        &[
            ("META-INF/jars/nested_zzz.jar", nested_zzz_bytes.as_slice()),
            (
                "data/demo/recipe/shared_before.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_before","catalyst":"minecraft:coal","result":"minecraft:stick","tier":1}"##,
            ),
            (
                "data/demo/tags/item/shared_before.json",
                br#"{"values":["minecraft:iron_ingot"]}"#,
            ),
            (
                "data/demo/recipe/shared_nested.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_nested","catalyst":"minecraft:coal","result":"minecraft:stick","tier":10}"##,
            ),
            (
                "data/demo/tags/item/shared_nested.json",
                br#"{"values":["minecraft:iron_ingot"]}"#,
            ),
            (
                "data/demo/recipe/shared_dp.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_dp","catalyst":"minecraft:coal","result":"minecraft:stick","tier":100}"##,
            ),
            (
                "data/demo/tags/item/shared_dp.json",
                br#"{"values":["minecraft:iron_ingot"]}"#,
            ),
        ],
    );

    // `mod_c` is linked FIRST (disagreeing with id order!) and bundles `nested_aaa`.
    let mod_c = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "pack-order-mod-c",
        &[("dummy/C.java", "package dummy; public class C {}")],
        r#"{"schemaVersion":1,"id":"mod_c","version":"1.0.0","jars":[{"file":"META-INF/jars/nested_aaa.jar"}]}"#,
        &[
            ("META-INF/jars/nested_aaa.jar", nested_aaa_bytes.as_slice()),
            (
                "data/demo/recipe/shared_before.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_before","catalyst":"minecraft:coal","result":"minecraft:stick","tier":3}"##,
            ),
            (
                "data/demo/tags/item/shared_before.json",
                br#"{"values":["minecraft:diamond"]}"#,
            ),
            (
                "data/demo/recipe/shared_nested.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_nested","catalyst":"minecraft:coal","result":"minecraft:stick","tier":20}"##,
            ),
            (
                "data/demo/tags/item/shared_nested.json",
                br#"{"values":["minecraft:diamond"]}"#,
            ),
            (
                "data/demo/recipe/shared_dp.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_dp","catalyst":"minecraft:coal","result":"minecraft:stick","tier":200}"##,
            ),
            (
                "data/demo/tags/item/shared_dp.json",
                br#"{"values":["minecraft:diamond"]}"#,
            ),
        ],
    );

    // Three more mods for `after`: `mod_z` declares `after: ["mod_x"]`, linked in order z, x, y.
    let mod_x = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "pack-order-mod-x",
        &[("dummy/X.java", "package dummy; public class X {}")],
        r#"{"schemaVersion":1,"id":"mod_x","version":"1.0.0"}"#,
        &[
            (
                "data/demo/recipe/shared_after.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_after","catalyst":"minecraft:coal","result":"minecraft:stick","tier":11}"##,
            ),
            (
                "data/demo/tags/item/shared_after.json",
                br#"{"values":["minecraft:iron_ingot"]}"#,
            ),
        ],
    );
    let mod_y = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "pack-order-mod-y",
        &[("dummy/Y.java", "package dummy; public class Y {}")],
        r#"{"schemaVersion":1,"id":"mod_y","version":"1.0.0"}"#,
        &[
            (
                "data/demo/recipe/shared_after.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_after","catalyst":"minecraft:coal","result":"minecraft:stick","tier":12}"##,
            ),
            (
                "data/demo/tags/item/shared_after.json",
                br#"{"replace":true,"values":["minecraft:gold_ingot"]}"#,
            ),
        ],
    );
    let mod_z = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "pack-order-mod-z",
        &[("dummy/Z.java", "package dummy; public class Z {}")],
        r#"{"schemaVersion":1,"id":"mod_z","version":"1.0.0","custom":{"fabric:resource_load_order":{"after":["mod_x"]}}}"#,
        &[
            (
                "data/demo/recipe/shared_after.json",
                br##"{"type":"demo:alloy","base":"#demo:shared_after","catalyst":"minecraft:coal","result":"minecraft:stick","tier":13}"##,
            ),
            (
                "data/demo/tags/item/shared_after.json",
                br#"{"values":["minecraft:diamond"]}"#,
            ),
        ],
    );

    // Seed `mod_b` as primary jar, then link in deliberately scrambled order: mod_c, mod_a, mod_z, mod_x, mod_y.
    let mut host = mod_b.jar.seed_host(StdHost::new());
    host.link_jar("link-first-c", &mod_c.jar).unwrap();
    host.link_jar("link-second-a", &mod_a.jar).unwrap();
    host.link_jar("link-third-z", &mod_z.jar).unwrap();
    host.link_jar("link-fourth-x", &mod_x.jar).unwrap();
    host.link_jar("link-fifth-y", &mod_y.jar).unwrap();

    // Add world datapack above all mods.
    let mut dp = BTreeMap::new();
    dp.insert(
        "data/demo/recipe/shared_dp.json".to_owned(),
        br##"{"type":"demo:alloy","base":"#demo:shared_dp","catalyst":"minecraft:coal","result":"minecraft:stick","tier":300}"##
            .to_vec(),
    );
    dp.insert(
        "data/demo/tags/item/shared_dp.json".to_owned(),
        br#"{"replace":true,"values":["minecraft:coal"]}"#.to_vec(),
    );
    host.add_data_pack_layer("file/dp_override", dp);

    // Stage 1 ModPackResourcesSorter order matches `SORTER_MOD_ORDER` from real Fabric 26.3:
    // mod_a (before: ["mod_c"]) sits after mod_c; mod_x sits after mod_z (after: ["mod_x"]).
    assert_eq!(
        host.mod_pack_sorter_order(),
        vec![
            "mod_b",
            "mod_c",
            "mod_a",
            "mod_y",
            "mod_z",
            "mod_x",
            "nested_aaa",
            "nested_zzz",
        ]
    );
    assert!(
        oracle["SORTER_MOD_ORDER"]
            .contains("mod_b, mod_c, mod_a, mod_y, mod_z, mod_x, nested_aaa, nested_zzz"),
        "oracle SORTER_MOD_ORDER={}",
        oracle["SORTER_MOD_ORDER"]
    );

    mod_b
        .jar
        .execute("demo.PackOrderProbe", None, "main", &mut host)
        .unwrap();
    fire_use_item(&mod_b.jar, &mut host);

    let sources: BTreeMap<String, String> = host.loaded_recipe_sources().into_iter().collect();
    assert_eq!(
        sources["demo:shared_before"],
        oracle["BEFORE_RECIPE_WINNER"]
    );
    assert_eq!(sources["demo:shared_after"], oracle["AFTER_RECIPE_WINNER"]);
    assert_eq!(
        sources["demo:shared_nested"],
        oracle["NESTED_RECIPE_WINNER"]
    );
    assert_eq!(sources["demo:shared_dp"], oracle["DP_RECIPE_WINNER"]);

    let statics = host.persistent_class_state().0;
    assert_eq!(
        statics.get("demo/PackOrderProbe#SUMMARYLjava/lang/String;"),
        Some(&horizon_native_java::Value::Ref(Some(
            "before=3(false,true,true)|after=13(false,true,true)|nested=40(false,true,true)|dp=300(false,true)".into()
        )))
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "expected zero unknown calls, got {:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn empty_data_namespace_directory_participates_in_pack_order_matching_oracle() {
    const FABRIC_ORACLE: &str = include_str!("fixtures/recipes/fabric263_pack_order.txt");
    let oracle: BTreeMap<&str, &str> = FABRIC_ORACLE
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();

    let emp_a = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "empty-ns-emp-a",
        &[("dummy/EA.java", "package dummy; public class EA {}")],
        r#"{"schemaVersion":1,"id":"emp_a","version":"1.0.0"}"#,
        &[(
            "data/orderprobe/recipe/emp_only.json",
            br#"{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:stone"],"result":{"id":"minecraft:stick"}}"#,
        )],
    );
    let emp_z = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "empty-ns-emp-z",
        &[("dummy/EZ.java", "package dummy; public class EZ {}")],
        r#"{"schemaVersion":1,"id":"emp_z","version":"1.0.0"}"#,
        &[(
            "data/orderprobe/recipe/emp_only.json",
            br#"{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:stone"],"result":{"id":"minecraft:stick"}}"#,
        )],
    );
    // Jar with ONLY an empty `data/empty_ns/` directory (zero files under `data/`) plus
    // before/after constraints flipping `emp_z` ahead of `emp_a`.
    let empty_bridge = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "empty-ns-bridge",
        &[("dummy/EB.java", "package dummy; public class EB {}")],
        r#"{"schemaVersion":1,"id":"empty_bridge","version":"1.0.0","custom":{"fabric:resource_load_order":{"before":["emp_z"],"after":["emp_a"]}}}"#,
        &[("data/", b""), ("data/empty_ns/", b"")],
    );
    let inv_a = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "empty-ns-inv-a",
        &[("dummy/IA.java", "package dummy; public class IA {}")],
        r#"{"schemaVersion":1,"id":"inv_a","version":"1.0.0"}"#,
        &[(
            "data/orderprobe/recipe/inv_only.json",
            br#"{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:stone"],"result":{"id":"minecraft:stick"}}"#,
        )],
    );
    let inv_z = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "empty-ns-inv-z",
        &[("dummy/IZ.java", "package dummy; public class IZ {}")],
        r#"{"schemaVersion":1,"id":"inv_z","version":"1.0.0"}"#,
        &[(
            "data/orderprobe/recipe/inv_only.json",
            br#"{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:stone"],"result":{"id":"minecraft:stick"}}"#,
        )],
    );
    // Jar with ONLY an invalid uppercase namespace directory `data/Bad_NS/` and a bare file
    // `data/plain_file.txt`: ModNioPackResources.readNamespaces rejects both, so `empty_invalid`
    // is excluded along with its `before`/`after` edges (`inv_a` stays before `inv_z`).
    let empty_invalid = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "empty-ns-invalid",
        &[("dummy/EI.java", "package dummy; public class EI {}")],
        r#"{"schemaVersion":1,"id":"empty_invalid","version":"1.0.0","custom":{"fabric:resource_load_order":{"before":["inv_z"],"after":["inv_a"]}}}"#,
        &[
            ("data/", b""),
            ("data/Bad_NS/", b""),
            ("data/plain_file.txt", b"not a directory"),
        ],
    );

    // Case 1: `empty_bridge` and `empty_invalid` as linked jars.
    let mut host = emp_a.jar.seed_host(StdHost::new());
    host.link_jar("emp_z", &emp_z.jar).unwrap();
    host.link_jar("empty_bridge", &empty_bridge.jar).unwrap();
    host.link_jar("inv_a", &inv_a.jar).unwrap();
    host.link_jar("inv_z", &inv_z.jar).unwrap();
    host.link_jar("empty_invalid", &empty_invalid.jar).unwrap();

    let expected_subseq = vec!["emp_z", "empty_bridge", "emp_a", "inv_a", "inv_z"];
    assert_eq!(host.mod_pack_sorter_order(), expected_subseq);
    assert!(
        oracle["SORTER_MOD_ORDER"].contains("emp_z, empty_bridge, emp_a, inv_a, inv_z"),
        "oracle SORTER_MOD_ORDER={}",
        oracle["SORTER_MOD_ORDER"]
    );
    assert_eq!(oracle["EMPTY_BRIDGE_NAMESPACES"], "[empty_ns]");
    assert!(
        !oracle["SORTER_MOD_ORDER"].contains("empty_invalid")
            && !oracle["RM_PACK_ORDER"].contains("empty_invalid"),
        "oracle must exclude empty_invalid: {oracle:?}"
    );

    // Case 2: `empty_bridge` as the primary seeded jar itself (same exact sorter order).
    let mut host_primary = empty_bridge.jar.seed_host(StdHost::new());
    host_primary.link_jar("emp_a", &emp_a.jar).unwrap();
    host_primary.link_jar("emp_z", &emp_z.jar).unwrap();
    host_primary.link_jar("inv_a", &inv_a.jar).unwrap();
    host_primary.link_jar("inv_z", &inv_z.jar).unwrap();
    host_primary
        .link_jar("empty_invalid", &empty_invalid.jar)
        .unwrap();
    assert_eq!(host_primary.mod_pack_sorter_order(), expected_subseq);
}

#[test]
fn malformed_and_conflicting_zip_data_entries_are_counted_and_excluded_matching_jdk25_probe() {
    const JDK25_ZIPFS_ORACLE: &str = include_str!("fixtures/recipes/jdk25_zipfs_namespaces.txt");
    let oracle: BTreeMap<&str, &str> = JDK25_ZIPFS_ORACLE
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();

    // Verify expected JDK 25 ZipFS probe facts we are matching against.
    assert_eq!(
        oracle["data_file_then_child"],
        "open=OK|dataIsDir=false|namespaces=[]"
    );
    assert_eq!(
        oracle["child_then_data_file"],
        "open=OK|dataIsDir=false|namespaces=[]"
    );
    assert_eq!(
        oracle["ns_file_then_child"],
        "open=OK|dataIsDir=true|namespaces=[]"
    );
    assert_eq!(
        oracle["child_then_ns_file"],
        "open=OK|dataIsDir=true|namespaces=[]"
    );
    assert_eq!(
        oracle["ns_file_plus_other_ns"],
        "open=OK|dataIsDir=true|namespaces=[other]"
    );
    assert_eq!(oracle["dot_in_data_empty_dir"], "open=THROW|ZipException");
    assert_eq!(oracle["dot_in_data_file"], "open=THROW|ZipException");
    assert_eq!(oracle["dot_outside_data"], "open=THROW|ZipException");
    assert_eq!(oracle["dotdot_in_data"], "open=THROW|ZipException");
    assert_eq!(
        oracle["bare_data_file_only"],
        "open=OK|dataIsDir=false|namespaces=[]"
    );
    assert_eq!(
        oracle["valid_dotted_ns"],
        "open=OK|dataIsDir=true|namespaces=[a.b, c..d]"
    );

    let mut probe_sources = STUBS.to_vec();
    probe_sources.push(("demo/AlloyInput.java", DEMO_RECIPE_INPUT));
    probe_sources.push(("demo/AlloyRecipe.java", DEMO_RECIPE));
    probe_sources.push((
        "demo/ZipNsProbe.java",
        r#"
package demo;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.item.crafting.*;

public final class ZipNsProbe {
  public static void onInitialize() {
    Identifier id = Identifier.parse("demo:alloy");
    Registry.register(BuiltInRegistries.RECIPE_TYPE, id, AlloyRecipe.TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, id, AlloyRecipe.SERIALIZER);
    UseItemCallback.EVENT.register((player, level, hand) -> {
      RecipeManager mgr = ((ServerLevel) level).recipeAccess();
      mgr.byKey(ResourceKey.create(Registries.RECIPE, Identifier.parse("ns:r")));
      return InteractionResult.PASS;
    });
  }
}
"#,
    ));

    const ALLOY_JSON: &[u8] = br#"{"type":"demo:alloy","base":"minecraft:iron_ingot","catalyst":"minecraft:coal","result":"minecraft:stick","tier":1}"#;
    const READ_NS_KEY: &str = "net/fabricmc/fabric/impl/resource/pack/ModNioPackResources#readNamespaces(Ljava/util/List;Ljava/lang/String;)Ljava/util/Map;";

    let emp_a = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "zipfs-emp-a",
        &probe_sources,
        r#"{"schemaVersion":1,"id":"emp_a","version":"1.0.0","entrypoints":{"main":"demo.ZipNsProbe"}}"#,
        &[("data/demo/recipe/emp_a_only.json", ALLOY_JSON)],
    );
    let emp_z = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "zipfs-emp-z",
        &[("dummy/EZ.java", "package dummy; public class EZ {}")],
        r#"{"schemaVersion":1,"id":"emp_z","version":"1.0.0"}"#,
        &[("data/demo/recipe/emp_z_only.json", ALLOY_JSON)],
    );

    let malformed_cases: &[(&str, &[(&str, &[u8])])] = &[
        (
            "data_file_then_child",
            &[("data", b"file"), ("data/ns/recipe/r.json", ALLOY_JSON)],
        ),
        (
            "child_then_data_file",
            &[("data/ns/recipe/r.json", ALLOY_JSON), ("data", b"file")],
        ),
        (
            "ns_file_then_child",
            &[("data/ns", b"file"), ("data/ns/recipe/r.json", ALLOY_JSON)],
        ),
        (
            "ns_file_plus_other_ns",
            &[
                ("data/ns", b"file"),
                ("data/ns/recipe/r.json", ALLOY_JSON),
                ("data/other/recipe/o.json", ALLOY_JSON),
            ],
        ),
        ("dot_in_data_empty_dir", &[("data/./empty_ns/", b"")]),
        (
            "dotdot_in_data",
            &[("data/ns/../other/recipe/r.json", ALLOY_JSON)],
        ),
        (
            "dot_outside_data",
            &[
                ("foo/./bar.txt", b"x"),
                ("data/ns/recipe/r.json", ALLOY_JSON),
            ],
        ),
    ];

    for (case_name, entries) in malformed_cases {
        let mod_json = format!(
            r#"{{"schemaVersion":1,"id":"{case_name}","version":"1.0.0","custom":{{"fabric:resource_load_order":{{"before":["emp_z"],"after":["emp_a"]}}}}}}"#
        );
        let bad_jar = javac_fixture::Fixture::build_with_mod_json_and_extra(
            &format!("zipfs-{case_name}"),
            &[("dummy/B.java", "package dummy; public class B {}")],
            &mod_json,
            entries,
        );

        let mut host = emp_a.jar.seed_host(StdHost::new());
        host.link_jar("emp_z", &emp_z.jar).unwrap();
        host.link_jar(*case_name, &bad_jar.jar).unwrap();

        // Must NOT be included as a pack and must NOT flip `emp_z` ahead of `emp_a`.
        assert_eq!(
            host.mod_pack_sorter_order(),
            vec!["emp_a", "emp_z"],
            "case {case_name}: malformed jar must be excluded from sorter"
        );

        emp_a
            .jar
            .execute("demo.ZipNsProbe", None, "main", &mut host)
            .unwrap();
        fire_use_item(&emp_a.jar, &mut host);

        let unknowns: BTreeMap<String, usize> = host.unknown_host_calls().into_iter().collect();
        assert_eq!(
            unknowns.get(READ_NS_KEY),
            Some(&1),
            "case {case_name}: expected ModNioPackResources#readNamespaces to be counted once, got {unknowns:?}"
        );
        assert!(
            !host.loaded_recipe_ids().iter().any(|id| id == "ns:r"),
            "case {case_name}: recipe from malformed jar must not be decoded: {:?}",
            host.loaded_recipe_ids()
        );
    }

    // Primary-seeded malformed jars (both regular-file `data` collision and `.` entry segment)
    // must also be excluded and counted when seeded directly.
    for (case_name, entries) in [
        (
            "primary_data_file",
            &[
                ("data", b"file" as &[u8]),
                ("data/ns/recipe/r.json", ALLOY_JSON),
            ][..],
        ),
        ("primary_dot_dir", &[("data/./empty_ns/", b"" as &[u8])][..]),
    ] {
        let mod_json = format!(
            r#"{{"schemaVersion":1,"id":"{case_name}","version":"1.0.0","entrypoints":{{"main":"demo.ZipNsProbe"}},"custom":{{"fabric:resource_load_order":{{"before":["emp_z"],"after":["emp_a"]}}}}}}"#
        );
        let primary_bad = javac_fixture::Fixture::build_with_mod_json_and_extra(
            &format!("zipfs-{case_name}"),
            &probe_sources,
            &mod_json,
            entries,
        );
        let mut host = primary_bad.jar.seed_host(StdHost::new());
        host.link_jar("emp_a", &emp_a.jar).unwrap();
        host.link_jar("emp_z", &emp_z.jar).unwrap();
        assert_eq!(
            host.mod_pack_sorter_order(),
            vec!["emp_a", "emp_z"],
            "primary case {case_name}: malformed jar must be excluded"
        );
        primary_bad
            .jar
            .execute("demo.ZipNsProbe", None, "main", &mut host)
            .unwrap();
        fire_use_item(&primary_bad.jar, &mut host);
        let unknowns: BTreeMap<String, usize> = host.unknown_host_calls().into_iter().collect();
        assert_eq!(
            unknowns.get(READ_NS_KEY),
            Some(&1),
            "primary case {case_name}: expected ModNioPackResources#readNamespaces to be counted, got {unknowns:?}"
        );
    }

    // Clean controls matching JDK 25 ZipFS with ZERO unknown calls:
    // 1. Bare regular file `data` with no children under `data/` -> excluded cleanly, not counted.
    // 2. Valid dotted namespaces `data/a.b/` + `data/c..d/` -> included cleanly, not counted.
    let bare_data_only = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "zipfs-bare-data-only",
        &[("dummy/D.java", "package dummy; public class D {}")],
        r#"{"schemaVersion":1,"id":"bare_data","version":"1.0.0","custom":{"fabric:resource_load_order":{"before":["emp_z"],"after":["emp_a"]}}}"#,
        &[("data", b"not a directory")],
    );
    let valid_dotted = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "zipfs-valid-dotted",
        &[("dummy/V.java", "package dummy; public class V {}")],
        r#"{"schemaVersion":1,"id":"dotted_ns","version":"1.0.0","custom":{"fabric:resource_load_order":{"before":["emp_z"],"after":["emp_a"]}}}"#,
        &[("data/a.b/recipe/ok.json", ALLOY_JSON), ("data/c..d/", b"")],
    );
    let mut clean_host = emp_a.jar.seed_host(StdHost::new());
    clean_host.link_jar("emp_z", &emp_z.jar).unwrap();
    clean_host
        .link_jar("bare_data", &bare_data_only.jar)
        .unwrap();
    clean_host.link_jar("dotted_ns", &valid_dotted.jar).unwrap();
    assert_eq!(
        clean_host.mod_pack_sorter_order(),
        vec!["emp_z", "dotted_ns", "emp_a"]
    );
    emp_a
        .jar
        .execute("demo.ZipNsProbe", None, "main", &mut clean_host)
        .unwrap();
    fire_use_item(&emp_a.jar, &mut clean_host);
    assert!(
        clean_host.unknown_host_calls().is_empty(),
        "expected zero unknown calls for clean bare `data` and dotted namespaces, got {:?}",
        clean_host.unknown_host_calls()
    );
    assert!(
        clean_host
            .loaded_recipe_ids()
            .iter()
            .any(|id| id == "a.b:ok"),
        "expected recipe from valid dotted namespace to load: {:?}",
        clean_host.loaded_recipe_ids()
    );
}

#[test]
fn unsupported_resource_load_order_constraints_are_counted() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/AlloyInput.java", DEMO_RECIPE_INPUT));
    sources.push(("demo/AlloyRecipe.java", DEMO_RECIPE));
    sources.push((
        "demo/BadOrderProbe.java",
        r#"
package demo;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;

public final class BadOrderProbe {
  public static void onInitialize() {
    Identifier id = Identifier.parse("demo:alloy");
    Registry.register(BuiltInRegistries.RECIPE_TYPE, id, AlloyRecipe.TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, id, AlloyRecipe.SERIALIZER);
    UseItemCallback.EVENT.register((player, level, hand) -> {
      RecipeManager mgr = ((ServerLevel) level).recipeAccess();
      mgr.byKey(ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:r")));
      return InteractionResult.PASS;
    });
  }
}
"#,
    ));

    let bad_obj = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "bad-load-order-obj",
        &sources,
        r#"{"schemaVersion":1,"id":"bad_obj","version":"1.0.0","entrypoints":{"main":"demo.BadOrderProbe"},"custom":{"fabric:resource_load_order":"not_an_object"}}"#,
        &[(
            "data/demo/recipe/r.json",
            br#"{"type":"demo:alloy","base":"minecraft:iron_ingot","catalyst":"minecraft:coal","result":"minecraft:stick","tier":1}"#,
        )],
    );
    let self_dep = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "bad-load-order-self",
        &[("dummy/S.java", "package dummy; public class S {}")],
        r#"{"schemaVersion":1,"id":"self_mod","version":"1.0.0","custom":{"fabric:resource_load_order":{"before":["self_mod"],"after":123,"priority":1}}}"#,
        &[(
            "data/demo/recipe/s.json",
            br#"{"type":"demo:alloy","base":"minecraft:gold_ingot","catalyst":"minecraft:coal","result":"minecraft:stick","tier":2}"#,
        )],
    );

    let mut host = bad_obj.jar.seed_host(StdHost::new());
    host.link_jar("self_mod", &self_dep.jar).unwrap();
    bad_obj
        .jar
        .execute("demo.BadOrderProbe", None, "main", &mut host)
        .unwrap();
    fire_use_item(&bad_obj.jar, &mut host);

    let unknowns: BTreeMap<String, usize> = host.unknown_host_calls().into_iter().collect();
    assert_eq!(
        unknowns.get(
            "net/fabricmc/fabric/impl/resource/pack/ModPackResourcesUtil#addLoadOrdering(Lnet/fabricmc/loader/api/metadata/CustomValue$CvObject;Ljava/util/List;Lnet/fabricmc/fabric/impl/resource/pack/ModPackResourcesSorter;Lnet/fabricmc/fabric/impl/resource/pack/ModPackResourcesUtil$Order;Ljava/lang/String;)V"
        ),
        Some(&3),
        "expected non-object + unknown key + non-string/array target to be counted (3 total), got {unknowns:?}"
    );
    assert_eq!(
        unknowns.get(
            "net/fabricmc/fabric/impl/resource/pack/ModPackResourcesSorter#addLoadOrdering(Ljava/lang/String;Ljava/lang/String;Lnet/fabricmc/fabric/impl/resource/pack/ModPackResourcesUtil$Order;)V"
        ),
        Some(&1),
        "expected self-dependency to be counted, got {unknowns:?}"
    );
}

#[test]
fn record_codec_builder_and_unit_callbacks_with_counted_calls_refuse_and_count() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/AlloyInput.java", DEMO_RECIPE_INPUT));
    sources.push(("demo/AlloyRecipe.java", DEMO_RECIPE));
    sources.push((
        "com/example/ExternalUnmodelled.java",
        "package com.example; public final class ExternalUnmodelled { public static int sideEffect() { return 7; } }",
    ));
    sources.push((
        "demo/CountedCodecCallbacks.java",
        r#"
package demo;
import com.example.ExternalUnmodelled;
import com.mojang.serialization.*;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;

public final class CountedCodecCallbacks {
  public static final RecipeType<BadBuilderRecipe> TYPE = new RecipeType<>() {};
  public static final MapCodec<BadBuilderRecipe> MAP_CODEC = RecordCodecBuilder.mapCodec(i -> {
    // Calls an unmodelled method yet still returns a valid-looking App!
    ExternalUnmodelled.sideEffect();
    return i.group(
        Codec.INT.fieldOf("tier").forGetter(r -> r.tier)
    ).apply(i, BadBuilderRecipe::new);
  });
  public static final Codec<BadBuilderRecipe> VALUE_CODEC = RecordCodecBuilder.create(i -> {
    ExternalUnmodelled.sideEffect();
    return i.group(
        Codec.INT.fieldOf("tier").forGetter(r -> r.tier)
    ).apply(i, BadBuilderRecipe::new);
  });
  public static final RecipeSerializer<BadBuilderRecipe> SERIALIZER =
      new RecipeSerializer<>(MAP_CODEC, null);
  public static String UNIT_SUMMARY;

  public static final class BadBuilderRecipe implements Recipe<AlloyInput> {
    public final int tier;
    public BadBuilderRecipe(int tier) { this.tier = tier; }
    public boolean matches(AlloyInput in, net.minecraft.world.level.Level lvl) { return true; }
    public ItemStack assemble(AlloyInput in) { return new ItemStack(Items.STICK); }
    public RecipeSerializer<? extends Recipe<AlloyInput>> getSerializer() { return SERIALIZER; }
    public RecipeType<? extends Recipe<AlloyInput>> getType() { return TYPE; }
  }

  public static void onInitialize() {
    Identifier id = Identifier.parse("demo:bad_builder");
    Registry.register(BuiltInRegistries.RECIPE_TYPE, id, TYPE);
    Registry.register(BuiltInRegistries.RECIPE_SERIALIZER, id, SERIALIZER);

    // Chaining on the refused VALUE_CODEC must stay counted (opaque).
    VALUE_CODEC.fieldOf("wrapped");

    // MapCodec.unit(Supplier) whose supplier invokes an unmodelled call and still returns "ok":
    // both decode() and toString() (on MapCodec and its .codec()) must be counted.
    MapCodec<String> badUnit = MapCodec.unit(() -> {
      ExternalUnmodelled.sideEffect();
      return "ok";
    });
    DataResult<String> dr = badUnit.decode(null, null);
    String s1 = badUnit.toString();
    String s2 = badUnit.codec().toString();
    UNIT_SUMMARY = (MAP_CODEC != null) + "|" + (VALUE_CODEC != null) + "|" + (dr == null) + "|" + (s1 == null) + "|" + (s2 == null);

    UseItemCallback.EVENT.register((player, level, hand) -> {
      RecipeManager mgr = ((ServerLevel) level).recipeAccess();
      mgr.byKey(ResourceKey.create(Registries.RECIPE, Identifier.parse("demo:from_bad_builder")));
      return InteractionResult.PASS;
    });
  }
}
"#,
    ));

    let fixture = javac_fixture::Fixture::build_with_mod_json_and_extra(
        "recipes-counted-codec-callbacks",
        &sources,
        r#"{"schemaVersion":1,"id":"demo","version":"1.0.0","entrypoints":{"main":"demo.CountedCodecCallbacks"}}"#,
        &[(
            "data/demo/recipe/from_bad_builder.json",
            br#"{"type":"demo:bad_builder","tier":5}"#,
        )],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture
        .jar
        .execute("demo.CountedCodecCallbacks", None, "main", &mut host)
        .unwrap();
    fire_use_item(&fixture.jar, &mut host);

    let unknowns: BTreeMap<String, usize> = host.unknown_host_calls().into_iter().collect();
    // Both RecordCodecBuilder.mapCodec and RecordCodecBuilder.create must be counted despite returning valid-looking Apps!
    assert_eq!(
        unknowns.get(
            "com/mojang/serialization/codecs/RecordCodecBuilder#mapCodec(Ljava/util/function/Function;)Lcom/mojang/serialization/MapCodec;"
        ),
        Some(&1),
        "expected RecordCodecBuilder.mapCodec to be counted, got {unknowns:?}"
    );
    assert_eq!(
        unknowns.get(
            "com/mojang/serialization/codecs/RecordCodecBuilder#create(Ljava/util/function/Function;)Lcom/mojang/serialization/Codec;"
        ),
        Some(&1),
        "expected RecordCodecBuilder.create to be counted, got {unknowns:?}"
    );
    // Refused VALUE_CODEC is opaque -> fieldOf is counted too.
    assert_eq!(
        unknowns.get(
            "com/mojang/serialization/Codec#fieldOf(Ljava/lang/String;)Lcom/mojang/serialization/MapCodec;"
        ),
        Some(&1),
        "expected fieldOf on refused opaque Codec to be counted, got {unknowns:?}"
    );
    // Refused MAP_CODEC is unusable for recipe decode -> decodeRecipe + byKey counted!
    assert_eq!(
        unknowns
            .get("net/minecraft/world/item/crafting/RecipeManager#decodeRecipe(demo:bad_builder)V"),
        Some(&1),
        "expected decodeRecipe for refused MapCodec to be counted, got {unknowns:?}"
    );
    assert_eq!(
        unknowns.get(
            "net/minecraft/world/item/crafting/RecipeManager#byKey(Lnet/minecraft/resources/ResourceKey;)Ljava/util/Optional;"
        ),
        Some(&1),
        "expected byKey for skipped recipe to be counted, got {unknowns:?}"
    );
    // MapCodec.unit(Supplier) with counted inner call -> decode + both toString calls counted!
    assert_eq!(
        unknowns.get(
            "com/mojang/serialization/MapCodec#decode(Lcom/mojang/serialization/DynamicOps;Lcom/mojang/serialization/MapLike;)Lcom/mojang/serialization/DataResult;"
        ),
        Some(&1),
        "expected MapCodec.unit.decode to be counted, got {unknowns:?}"
    );
    assert_eq!(
        unknowns.get("java/lang/Object#toString()Ljava/lang/String;"),
        Some(&1),
        "expected MapCodec.unit.toString to be counted, got {unknowns:?}"
    );
    assert_eq!(
        unknowns.get("com/mojang/serialization/Codec#toString()Ljava/lang/String;"),
        Some(&1),
        "expected MapCodec.unit.codec().toString to be counted, got {unknowns:?}"
    );
    assert!(
        host.dfu_codec_refusals()
            .iter()
            .any(|msg| msg.contains("RecordCodecBuilder callback invoked a counted unknown call")),
        "expected DFU refusal record, got {:?}",
        host.dfu_codec_refusals()
    );
    // Refused codecs stayed non-null, while counted unit operations did not invent exact results.
    let statics = host.persistent_class_state().0;
    assert_eq!(
        statics.get("demo/CountedCodecCallbacks#UNIT_SUMMARYLjava/lang/String;"),
        Some(&horizon_native_java::Value::Ref(Some(
            "true|true|true|true|true".into()
        )))
    );
}
