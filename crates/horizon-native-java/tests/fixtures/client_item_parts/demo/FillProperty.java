package demo;

import com.mojang.serialization.Codec;
import com.mojang.serialization.MapCodec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.item.properties.numeric.RangeSelectItemModelProperty;
import net.minecraft.world.entity.ItemOwner;
import net.minecraft.world.item.ItemStack;

/**
 * A numeric item model property (`demo:fill`): the stack's count over `divisor` (read from the
 * `range_dispatch` node, 64 by default), plus 100 with a level, 1000 with a holder, and half the
 * seed.
 */
public record FillProperty(int divisor) implements RangeSelectItemModelProperty {
  public static final MapCodec<FillProperty> CODEC =
      RecordCodecBuilder.mapCodec(
          i ->
              i.group(Codec.INT.optionalFieldOf("divisor", 64).forGetter(FillProperty::divisor))
                  .apply(i, FillProperty::new));

  @Override
  public float get(ItemStack stack, ClientLevel level, ItemOwner owner, int seed) {
    float fill = stack.getCount() / (float) divisor;
    if (level != null) {
      fill += 100.0f;
    }
    if (owner != null) {
      fill += 1000.0f;
    }
    return fill + seed * 0.5f;
  }

  @Override
  public MapCodec<FillProperty> type() {
    return CODEC;
  }
}
