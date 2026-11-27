package demo;

import net.fabricmc.api.ModInitializer;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;

/** Registers the meter item. */
public final class Probe implements ModInitializer {
  public static Item METER;

  @Override
  public void onInitialize() {
    Identifier id = Identifier.fromNamespaceAndPath("demo", "meter");
    METER =
        Registry.register(
            BuiltInRegistries.ITEM,
            id,
            new MeterItem(
                new Item.Properties().setId(ResourceKey.create(Registries.ITEM, id)).stacksTo(16)));
  }
}
