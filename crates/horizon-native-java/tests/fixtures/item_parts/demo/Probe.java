package demo;

import net.fabricmc.api.ModInitializer;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;

/** Registers the tally component type and the tally item. */
public final class Probe implements ModInitializer {
  public static DataComponentType<Tally> TALLY;
  public static Item TALLY_ITEM;

  @Override
  public void onInitialize() {
    TALLY =
        Registry.register(
            BuiltInRegistries.DATA_COMPONENT_TYPE,
            Identifier.fromNamespaceAndPath("demo", "tally"),
            DataComponentType.<Tally>builder().persistent(Tally.CODEC).build());
    Identifier id = Identifier.fromNamespaceAndPath("demo", "tally_item");
    TALLY_ITEM =
        Registry.register(
            BuiltInRegistries.ITEM,
            id,
            new TallyItem(
                new Item.Properties().setId(ResourceKey.create(Registries.ITEM, id)).stacksTo(1)));
  }
}
