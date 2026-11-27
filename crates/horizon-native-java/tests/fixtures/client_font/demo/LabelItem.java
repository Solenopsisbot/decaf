package demo;

import java.util.Optional;
import net.minecraft.world.inventory.tooltip.TooltipComponent;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

/** An item whose tooltip shows a {@link Label} of the kind its count names. */
public final class LabelItem extends Item {
  public LabelItem(Item.Properties properties) {
    super(properties);
  }

  @Override
  public Optional<TooltipComponent> getTooltipImage(ItemStack stack) {
    return Optional.of(new Label(stack.getCount()));
  }
}
