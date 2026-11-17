package demo;
import net.fabricmc.fabric.api.creativetab.v1.FabricCreativeModeTab;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.CreativeModeTab;
public final class Probe { public static String rows(){ CreativeModeTab.Builder b1=FabricCreativeModeTab.builder(); CreativeModeTab.Builder b2=FabricCreativeModeTab.builder(); b1.title(Component.literal("one")); b2.title(Component.literal("two")); CreativeModeTab t1=b1.build(); CreativeModeTab t2=b1.build(); CreativeModeTab t3=b2.build(); return (b1!=b2)+"|"+(t1!=t2)+"|"+t1.getDisplayName().getString()+"|"+t2.getDisplayName().getString()+"|"+t3.getDisplayName().getString(); } public static void main(String[]x){System.out.print(rows());}}
