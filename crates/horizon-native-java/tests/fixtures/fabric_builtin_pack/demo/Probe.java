package demo;

import net.fabricmc.fabric.api.resource.v1.ResourceLoader;
import net.fabricmc.fabric.api.resource.v1.pack.PackActivationType;
import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.ModContainer;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;

/**
 * Fabric's `ResourceLoader.registerBuiltinPack` (#478). It answers whether a pack was made for the
 * client or the server: the container's root must hold `resourcepacks/<id path>` (or the given
 * sub-path), with at least one valid namespace folder (`[a-z0-9-_.]+`) under its `assets/` or
 * `data/`. The fixture's `resources/` holds one folder per case.
 */
public final class Probe {
    /** In Decaf: the running jar's own container. */
    public static String rows() {
        return rows(FabricLoader.getInstance().getModContainer("demo").orElseThrow());
    }

    /** The same rows for any container whose root holds this fixture's `resources/`. */
    public static String rows(ModContainer c) {
        Component name = Component.literal("Demo pack");
        PackActivationType normal = PackActivationType.NORMAL;
        return "present=" + ResourceLoader.registerBuiltinPack(id("present"), c, name, normal)
                // The id's namespace is not part of the folder: `resourcepacks/demo/nested` is not read.
                + "|nested=" + ResourceLoader.registerBuiltinPack(id("nested"), c, name, normal)
                // A pack.mcmeta but no namespace folder: no pack.
                + "|metaOnly=" + ResourceLoader.registerBuiltinPack(id("meta_only"), c, name, normal)
                // Server data only.
                + "|dataOnly=" + ResourceLoader.registerBuiltinPack(id("dataonly"), c, name, PackActivationType.DEFAULT_ENABLED)
                // `assets/BadNS/` is an invalid namespace and `assets/marker.txt` a file: neither counts.
                + "|badNamespace=" + ResourceLoader.registerBuiltinPack(id("badns"), c, name, normal)
                // `resourcepacks/fileonly` is a file, not a folder.
                + "|fileOnly=" + ResourceLoader.registerBuiltinPack(id("fileonly"), c, name, normal)
                + "|absent=" + ResourceLoader.registerBuiltinPack(id("absent"), c, name, normal)
                // Registering the same id again answers the same.
                + "|again=" + ResourceLoader.registerBuiltinPack(id("present"), c, name, normal)
                + "|subPath=" + ResourceLoader.registerBuiltinPack(id("custom"), "packs/custom", c, name, PackActivationType.ALWAYS_ENABLED)
                + "|subPathAbsent=" + ResourceLoader.registerBuiltinPack(id("custom"), "packs/missing", c, name, normal);
    }

    static Identifier id(String path) {
        return Identifier.fromNamespaceAndPath("demo", path);
    }
}
