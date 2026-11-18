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
        Object value = registry.getValue(presentId);
        return Boolean.toString(registry.containsKey(presentId))
            + "|" + Boolean.toString(registry.get(presentId).isPresent())
            + "|" + registry.getKey(value);
    }

    public static String rows() {
        return row(BuiltInRegistries.BLOCK, "minecraft:stone")
            + "\n" + row(BuiltInRegistries.ITEM, "minecraft:stone");
    }

    static void unknown(Registry registry, Identifier id) {
        registry.containsKey(id);
        registry.get(id);
        registry.getValue(id);
    }

    public static String unknown() {
        unknown(BuiltInRegistries.BLOCK, id("minecraft:nope"));
        unknown(BuiltInRegistries.BLOCK, id("othermod:thing"));
        unknown(BuiltInRegistries.ITEM, id("minecraft:nope"));
        unknown(BuiltInRegistries.ITEM, id("othermod:thing"));
        return "counted";
    }
}
