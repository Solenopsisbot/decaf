package demo;

import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;

/** Registers the grab item (a second entrypoint beside the bench's `Probe`). */
public final class GrabProbe {
  public static Item GRAB;

  public static void register() {
    Identifier id = Identifier.fromNamespaceAndPath("demo", "grab");
    GRAB =
        Registry.register(
            BuiltInRegistries.ITEM,
            id,
            new GrabItem(new Item.Properties().setId(ResourceKey.create(Registries.ITEM, id))));
  }
}
