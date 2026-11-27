package demo;

import net.minecraft.world.entity.SlotAccess;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.ClickAction;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

/**
 * A jar item with its own click overrides, clicked inside the bench menu: vanilla's
 * `AbstractContainerMenu.tryItemClickBehaviourOverride` runs them before its plain pickup.
 */
public class GrabItem extends Item {
  public GrabItem(Properties properties) {
    super(properties);
  }

  /** A right click with a stack on the grab item takes one of the stack into it. */
  @Override
  public boolean overrideOtherStackedOnMe(
      ItemStack me, ItemStack other, Slot slot, ClickAction action, Player player, SlotAccess access) {
    if (action != ClickAction.SECONDARY || other.isEmpty()) {
      return false;
    }
    other.shrink(1);
    me.grow(1);
    return true;
  }

  /** The carried grab item right-clicked on a slot takes one from it (`Slot.safeTake`). */
  @Override
  public boolean overrideStackedOnOther(ItemStack me, Slot slot, ClickAction action, Player player) {
    ItemStack in = slot.getItem();
    if (action != ClickAction.SECONDARY || in.isEmpty()) {
      return false;
    }
    ItemStack taken = slot.safeTake(1, 1, player);
    me.grow(taken.getCount());
    return true;
  }
}
