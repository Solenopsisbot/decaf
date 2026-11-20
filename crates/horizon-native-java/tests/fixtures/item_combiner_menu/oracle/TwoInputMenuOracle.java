import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.inventory.ContainerLevelAccess;
import net.minecraft.world.inventory.ItemCombinerMenu;
import net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition;
import net.minecraft.world.inventory.MenuType;
import net.minecraft.world.inventory.Slot;

/** Independent Java 25 oracle for a mod-owned two-input ItemCombinerMenu. */
public final class TwoInputMenuOracle {
  private static final class TwoInputMenu extends ItemCombinerMenu {
    boolean created;

    TwoInputMenu() {
      super((MenuType<?>) null, 17, new Inventory(null, null), ContainerLevelAccess.NULL, definition());
    }

    private static ItemCombinerMenuSlotDefinition definition() {
      return ItemCombinerMenuSlotDefinition.create()
          .withSlot(0, 17, 29, stack -> stack != net.minecraft.world.item.ItemStack.EMPTY)
          .withSlot(1, 73, 29, stack -> true)
          .withResultSlot(2, 121, 29)
          .build();
    }

    @Override public void createResult() { created = true; resultSlots.setItem(0, net.minecraft.world.item.ItemStack.EMPTY); }
    @Override protected void onTake(net.minecraft.world.entity.player.Player player,
                                    net.minecraft.world.item.ItemStack stack) { }
    @Override protected boolean isValidBlock(net.minecraft.world.level.block.state.BlockState state) { return true; }

    void trigger() { inputSlots.setItem(0, net.minecraft.world.item.ItemStack.EMPTY); }
    boolean resultSet() { return created; }
    boolean inputsEmpty() { return inputSlots.isEmpty(); }
    boolean resultEmpty() { return resultSlots.isEmpty(); }
  }

  private static void validation(String name, Runnable action) {
    try {
      action.run();
      System.out.println(name + "=ok");
    } catch (Throwable error) {
      System.out.println(name + "=" + error.getClass().getName() + ":" + error.getMessage());
    }
  }

  public static void main(String[] args) {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    TwoInputMenu menu = new TwoInputMenu();
    System.out.println("slotCount=" + menu.slots.size());
    for (int i = 0; i < menu.slots.size(); i++) {
      Slot slot = menu.slots.get(i);
      System.out.println("slot position=" + i + " index=" + slot.index + " x=" + slot.x + " y=" + slot.y
          + " containerSlot=" + slot.getContainerSlot());
    }
    int result = menu.getResultSlot();
    System.out.println("resultSlot=" + result);
    System.out.println("inventoryStart=" + (result + 1));
    System.out.println("inventoryEnd=" + (result + 1 + 27));
    System.out.println("useRowStart=" + (result + 1 + 27));
    System.out.println("useRowEnd=" + (result + 1 + 27 + 9));
    System.out.println("stillValid=" + menu.stillValid(null));
    System.out.println("inputEmpty=" + menu.inputsEmpty());
    System.out.println("resultEmpty=" + menu.resultEmpty());
    menu.trigger();
    System.out.println("resultSet=" + menu.resultSet());
    System.out.println("mayPlace=" + menu.getSlot(0).mayPlace(net.minecraft.world.item.ItemStack.EMPTY));
    validation("builderEmpty", () -> ItemCombinerMenuSlotDefinition.create().build());
    validation("builderUnsetResult", () -> ItemCombinerMenuSlotDefinition.create()
        .withSlot(0, 0, 0, stack -> true).build());
    validation("builderBadInputIndex", () -> ItemCombinerMenuSlotDefinition.create()
        .withSlot(1, 0, 0, stack -> true).withResultSlot(2, 0, 0).build());
    validation("builderBadResultIndex", () -> ItemCombinerMenuSlotDefinition.create()
        .withSlot(0, 0, 0, stack -> true).withResultSlot(2, 0, 0).build());
  }
}
