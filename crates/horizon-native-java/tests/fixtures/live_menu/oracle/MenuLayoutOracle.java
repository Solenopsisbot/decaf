import demo.BenchMenu;
import demo.Probe;
import java.lang.reflect.Field;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityEquipment;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.level.block.Block;
import sun.misc.Unsafe;

/**
 * Java 25 oracle for the menu-type declaration test (tests/fixture_live_menu_bridge.rs): the
 * layout of the menu the bench's MenuType supplier builds (`new BenchMenu(id, inventory)`, the
 * `MenuType.create` constructor), with the real 26.3 AbstractContainerMenu / ItemCombinerMenu /
 * Slot / SimpleContainer / ResultContainer / Inventory classes. One line per slot: its index,
 * its class, its container's class (`player` for the player's Inventory), its container slot
 * and `Slot.getMaxStackSize()`; then the menu's data slot count.
 */
public final class MenuLayoutOracle {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    Field theUnsafe = Unsafe.class.getDeclaredField("theUnsafe");
    theUnsafe.setAccessible(true);
    LiveMenuOracle.unsafe = (Unsafe) theUnsafe.get(null);
    Probe.BENCH = (Block) LiveMenuOracle.allocate(demo.Bench.class);
    Player player = (Player) LiveMenuOracle.allocate(LiveMenuOracle.FakePlayer.class);
    Inventory inventory = new Inventory(player, new EntityEquipment());
    LiveMenuOracle.set(Player.class, player, "inventory", inventory);

    BenchMenu menu = new BenchMenu(0, inventory);
    for (Slot slot : menu.slots) {
      String container =
          slot.container == inventory ? "player" : slot.container.getClass().getName();
      System.out.println(
          "slot "
              + slot.index
              + " "
              + slot.getClass().getName()
              + " "
              + container
              + " "
              + slot.getContainerSlot()
              + " "
              + slot.getMaxStackSize());
    }
    Field dataSlots = AbstractContainerMenu.class.getDeclaredField("dataSlots");
    dataSlots.setAccessible(true);
    System.out.println("data_slots " + ((List<?>) dataSlots.get(menu)).size());
  }
}
