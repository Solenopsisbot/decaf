package demo;
import net.minecraft.world.item.*; import net.minecraft.world.level.block.*; import net.minecraft.core.registries.*; import net.minecraft.resources.*;
public final class Probe { static final String I= "minecraft:stone"; static final String B="minecraft:stone"; static final boolean SAME = Items.STONE == (Item)BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:stone")); public static String rows(){ return I+"\n"+B+"\n"+Boolean.toString(SAME)+"\n"; } }
