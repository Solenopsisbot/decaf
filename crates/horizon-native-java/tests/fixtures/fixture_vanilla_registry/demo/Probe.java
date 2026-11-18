package demo;

import java.util.Optional;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;

public final class Probe {
    static Identifier id(String text) {
        return Identifier.parse(text);
    }

    static String row(Registry registry, String present) {
        Identifier presentId = id(present);
        Identifier missingId = id("minecraft:nope");
        Object value = registry.getValue(presentId);
        Object fallback = registry.getValue(missingId);
        return Boolean.toString(registry.containsKey(presentId))
            + "|" + Boolean.toString(registry.get(presentId).isPresent())
            + "|" + registry.getKey(value)
            + "|" + Boolean.toString(registry.containsKey(missingId))
            + "|" + Boolean.toString(registry.get(missingId).isPresent())
            + "|" + registry.getKey(fallback);
    }

    public static String rows() {
        return row(BuiltInRegistries.BLOCK, "minecraft:stone")
            + "\n" + row(BuiltInRegistries.ITEM, "minecraft:stone");
    }

    public static String modded() {
        Identifier id = id("demo:thing");
        return BuiltInRegistries.BLOCK.containsKey(id) + "|"
            + BuiltInRegistries.BLOCK.get(id).isPresent() + "|"
            + BuiltInRegistries.ITEM.containsKey(id) + "|"
            + BuiltInRegistries.ITEM.get(id).isPresent();
    }
}
