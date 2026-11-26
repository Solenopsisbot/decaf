package demo;

import net.minecraft.sounds.SoundEvents;
import net.minecraft.stats.Stats;
import net.minecraft.util.Prediction;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import java.util.stream.Stream;
import net.minecraft.world.entity.SlotAccess;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.ClickAction;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemUtils;
import net.minecraft.world.level.Level;

/**
 * An item with its own click, use and bar overrides over a jar component, reading the
 * ItemStack, Slot and Player surfaces an item part answers.
 */
public class TallyItem extends Item {
  public TallyItem(Properties properties) {
    super(properties);
  }

  /** A right click with a stack on the tally takes up to three of it in. */
  @Override
  public boolean overrideOtherStackedOnMe(
      ItemStack me, ItemStack other, Slot slot, ClickAction action, Player player, SlotAccess access) {
    if (action != ClickAction.SECONDARY || other.isEmpty() || !slot.allowModification(player)) {
      return false;
    }
    Tally tally = me.getOrDefault(Probe.TALLY, new Tally(0, ""));
    int room = Math.min(3, slot.getMaxStackSize(other));
    ItemStack part = other.split(room);
    if (!ItemStack.isSameItemSameComponents(part, other.copyWithCount(1)) && !other.isEmpty()) {
      return false;
    }
    me.set(Probe.TALLY, new Tally(tally.count() + part.getCount(), part.getItem().toString()));
    access.set(other.copyWithCount(other.getCount()));
    player.playSound(SoundEvents.BUNDLE_INSERT, 0.8f, 1.0f);
    return true;
  }

  /** The carried tally right-clicked on a slot takes up to two of the slot's stack. */
  @Override
  public boolean overrideStackedOnOther(ItemStack me, Slot slot, ClickAction action, Player player) {
    if (action != ClickAction.SECONDARY) {
      return false;
    }
    Tally tally = me.get(Probe.TALLY);
    ItemStack in = slot.getItem();
    if (tally == null || in.isEmpty() || !slot.mayPickup(player)) {
      return false;
    }
    ItemStack taken = slot.safeTake(in.getCount(), 2, player);
    me.set(Probe.TALLY, new Tally(tally.count() + taken.getCount(), taken.getItem().toString()));
    return true;
  }

  @Override
  public boolean isBarVisible(ItemStack stack) {
    return stack.has(Probe.TALLY);
  }

  @Override
  public int getBarWidth(ItemStack stack) {
    return Math.min(13, stack.getOrDefault(Probe.TALLY, new Tally(0, "")).count());
  }

  @Override
  public int getBarColor(ItemStack stack) {
    Tally tally = stack.get(Probe.TALLY);
    return tally != null && tally.last().isEmpty() ? 0xFF0000 : 0x33CC66;
  }

  /** Using a full tally drops one copy of it, empties it and counts the use. */
  @Override
  public InteractionResult use(Level level, Player player, InteractionHand hand) {
    ItemStack held = player.getItemInHand(hand);
    player.awardStat(Stats.ITEM_USED.get(this));
    Tally tally = held.get(Probe.TALLY);
    if (tally == null) {
      return InteractionResult.FAIL;
    }
    player.drop(held.copyWithCount(1), true, Prediction.PREDICTED);
    held.remove(Probe.TALLY);
    return level.isClientSide() ? InteractionResult.SUCCESS : InteractionResult.SUCCESS_SERVER;
  }

  /** A destroyed tally spills one plain copy of itself per item it took in, in two drops. */
  @Override
  public void onDestroyed(ItemEntity entity) {
    ItemStack stack = entity.getItem();
    Tally tally = stack.get(Probe.TALLY);
    if (tally == null) {
      return;
    }
    stack.remove(Probe.TALLY);
    ItemUtils.onContainerDestroyed(
        entity, Stream.of(stack.copyWithCount(1), stack.copyWithCount(tally.count() - 1)));
  }
}
