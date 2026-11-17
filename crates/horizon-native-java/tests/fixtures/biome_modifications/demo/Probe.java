package demo;

import java.util.function.Consumer;
import java.util.function.Predicate;
import net.fabricmc.fabric.api.biome.v1.BiomeModification;
import net.fabricmc.fabric.api.biome.v1.BiomeModificationContext;
import net.fabricmc.fabric.api.biome.v1.BiomeModifications;
import net.fabricmc.fabric.api.biome.v1.BiomeSelectionContext;
import net.fabricmc.fabric.api.biome.v1.ModificationPhase;
import net.minecraft.resources.Identifier;

/** Fabric's `BiomeModifications.create` and `BiomeModification.add` (#476). */
public final class Probe {
    public static void main(String[] args) {
        System.out.print(rows());
        for (String m : new String[] {"nullSelector", "nullModifier", "nullContextModifier"}) {
            System.out.print("\n" + m + "=" + run(m));
        }
    }

    static String run(String method) {
        try {
            return (String) Probe.class.getMethod(method).invoke(null);
        } catch (ReflectiveOperationException e) {
            throw new RuntimeException(e);
        }
    }

    /** Answered: every value `create` and `add` give back during a mod's init. */
    public static String rows() {
        Identifier id = Identifier.parse("demo:test");
        BiomeModification a = BiomeModifications.create(id);
        BiomeModification b = BiomeModifications.create(id);
        // The context-sensitive overload, then the plain one, chained.
        BiomeModification c = a.add(ModificationPhase.ADDITIONS, s -> true, (s, ctx) -> {});
        BiomeModification d = c.add(ModificationPhase.REMOVALS, s -> false, ctx -> {});
        // `create` stores its id without checking it, and `add` never reads the phase.
        BiomeModification n = BiomeModifications.create(null);
        BiomeModification e = n.add(null, s -> true, ctx -> {});
        return (a != b)
                + "|" + (c == a)
                + "|" + (d == a)
                + "|" + (a instanceof BiomeModification)
                + "|" + a.equals(a)
                + "|" + a.equals(b)
                + "|" + (n != null)
                + "|" + (e == n);
    }

    /** Counted: `addModifier` throws on a null selector, which may be a counted call's answer. */
    public static String nullSelector() {
        BiomeModification a = BiomeModifications.create(Identifier.parse("demo:s"));
        try {
            a.add(ModificationPhase.ADDITIONS, (Predicate<BiomeSelectionContext>) null, ctx -> {});
            return "returned";
        } catch (NullPointerException e) {
            return "NullPointerException";
        }
    }

    /** Counted: the same for a null plain modifier. */
    public static String nullModifier() {
        BiomeModification a = BiomeModifications.create(Identifier.parse("demo:m"));
        try {
            a.add(ModificationPhase.ADDITIONS, s -> true, (Consumer<BiomeModificationContext>) null);
            return "returned";
        } catch (NullPointerException e) {
            return "NullPointerException";
        }
    }

    /** Counted: the same for a null context-sensitive modifier. */
    public static String nullContextModifier() {
        BiomeModification a = BiomeModifications.create(Identifier.parse("demo:c"));
        try {
            a.add(
                    ModificationPhase.ADDITIONS,
                    s -> true,
                    (java.util.function.BiConsumer<BiomeSelectionContext, BiomeModificationContext>) null);
            return "returned";
        } catch (NullPointerException e) {
            return "NullPointerException";
        }
    }
}
