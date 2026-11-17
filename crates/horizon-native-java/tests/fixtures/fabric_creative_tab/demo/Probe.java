package demo;

import net.fabricmc.fabric.api.creativetab.v1.FabricCreativeModeTab;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.CreativeModeTab;

/** Fabric's `FabricCreativeModeTab.builder()` and vanilla's `CreativeModeTab.Builder` (#453). */
public final class Probe {
    public static void main(String[] args) {
        System.out.print(rows());
        System.out.print("\nnullTitle=" + nullTitle());
    }

    /** Answered: what a mod's init reads back from builders and the tabs they build. */
    public static String rows() {
        CreativeModeTab.Builder b1 = FabricCreativeModeTab.builder();
        CreativeModeTab.Builder b2 = FabricCreativeModeTab.builder();
        b1.title(Component.literal("one"));
        b2.title(Component.literal("two"));
        CreativeModeTab t1 = b1.build();
        CreativeModeTab t2 = b1.build();
        CreativeModeTab t3 = b2.build();
        // Every public setter stores its argument and returns the same builder; the last `title`
        // wins. (`displayItems` takes a lambda of the protected `CreativeModeTab.Output`, which only
        // compiles with an access widener.)
        CreativeModeTab.Builder b3 = FabricCreativeModeTab.builder();
        boolean fluent = b3.title(Component.literal("first")) == b3
                && b3.icon(() -> null) == b3
                && b3.alignedRight() == b3
                && b3.hideTitle() == b3
                && b3.noScrollBar() == b3
                && b3.title(Component.literal("last")) == b3;
        CreativeModeTab t4 = b3.build();
        // Fabric's builder refuses to build before `title` is called.
        String untitled;
        try {
            FabricCreativeModeTab.builder().icon(() -> null).build();
            untitled = "built";
        } catch (IllegalStateException e) {
            untitled = e.getClass().getName() + ": " + e.getMessage();
        }
        return (b1 != b2)
                + "|" + (t1 != t2)
                + "|" + t1.getDisplayName().getString()
                + "|" + t2.getDisplayName().getString()
                + "|" + t3.getDisplayName().getString()
                + "|" + fluent
                + "|" + t4.getDisplayName().getString()
                + "|" + untitled;
    }

    /**
     * Counted: `title(null)` builds, and the tab's display name is that null; but a null here may be
     * a counted call's answer, so `getDisplayName` is counted.
     */
    public static String nullTitle() {
        CreativeModeTab.Builder b = FabricCreativeModeTab.builder();
        b.title(null);
        return "" + (b.build().getDisplayName() == null);
    }
}
