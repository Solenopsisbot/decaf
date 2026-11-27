package demo;

import net.fabricmc.api.ModInitializer;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;

/** Registers the label item. */
public final class Probe implements ModInitializer {
  public static Item LABEL;

  @Override
  public void onInitialize() {
    Identifier id = Identifier.fromNamespaceAndPath("demo", "label");
    LABEL =
        Registry.register(
            BuiltInRegistries.ITEM,
            id,
            new LabelItem(
                new Item.Properties().setId(ResourceKey.create(Registries.ITEM, id)).stacksTo(16)));
  }
}
