import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
public final class Probe {
  static final String ITEM = BuiltInRegistries.ITEM.getKey(Items.STONE).toString();
  static final String BLOCK = BuiltInRegistries.BLOCK.getKey(Blocks.STONE).toString();
  static final boolean SAME = Items.STONE == BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:stone"));
  public static String[] rows() { return new String[]{ITEM, BLOCK, Boolean.toString(SAME)}; }
}
