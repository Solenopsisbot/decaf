package demo;

import java.util.Optional;
import net.minecraft.world.inventory.tooltip.TooltipComponent;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

/** An item whose tooltip shows a {@link Meter} of its count, once it holds more than one. */
public final class MeterItem extends Item {
  public MeterItem(Item.Properties properties) {
    super(properties);
  }

  @Override
  public Optional<TooltipComponent> getTooltipImage(ItemStack stack) {
    return stack.getCount() > 1 ? Optional.of(new Meter(stack.getCount())) : Optional.empty();
  }
}
