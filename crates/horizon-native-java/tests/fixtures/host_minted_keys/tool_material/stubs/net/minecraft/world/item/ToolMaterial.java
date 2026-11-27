package net.minecraft.world.item;
import net.minecraft.tags.TagKey;
public final class ToolMaterial {
 public static final ToolMaterial WOOD = null, STONE = null, COPPER = null, IRON = null,
     DIAMOND = null, GOLD = null, NETHERITE = null;
 public ToolMaterial(TagKey<net.minecraft.world.level.block.Block> incorrect, int durability, float speed,
     float attackDamageBonus, int enchantmentValue, TagKey<Item> repairItems) {}
 public int durability(){throw new AssertionError();}
}
