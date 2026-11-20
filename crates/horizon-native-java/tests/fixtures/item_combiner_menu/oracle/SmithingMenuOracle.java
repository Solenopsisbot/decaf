import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import net.minecraft.server.Bootstrap;
import net.minecraft.SharedConstants;
import net.minecraft.world.inventory.SmithingMenu;
import net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition;
import net.minecraft.world.item.crafting.RecipeAccess;
import net.minecraft.world.item.crafting.RecipePropertySet;

/** Independent Java 25 oracle for vanilla 26.3 smithing slot geometry. */
public final class SmithingMenuOracle {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    RecipeAccess access = (RecipeAccess) Proxy.newProxyInstance(
        SmithingMenuOracle.class.getClassLoader(), new Class<?>[] {RecipeAccess.class},
        (proxy, method, methodArgs) -> method.getName().equals("propertySet") ? RecipePropertySet.EMPTY : null);
    Method create = SmithingMenu.class.getDeclaredMethod("createInputSlotDefinitions", RecipeAccess.class);
    create.setAccessible(true);
    ItemCombinerMenuSlotDefinition definition = (ItemCombinerMenuSlotDefinition) create.invoke(null, access);
    System.out.println("inputCount=" + definition.getNumOfInputSlots());
    for (var slot : definition.getSlots()) System.out.println("slot index=" + slot.slotIndex() + " x=" + slot.x() + " y=" + slot.y());
    var result = definition.getResultSlot();
    System.out.println("result index=" + result.slotIndex() + " x=" + result.x() + " y=" + result.y());
    int resultSlot = definition.getResultSlotIndex();
    System.out.println("resultSlot=" + resultSlot);
    System.out.println("inventoryStart=" + (resultSlot + 1));
    System.out.println("inventoryEnd=" + (resultSlot + 1 + 27));
    System.out.println("useRowStart=" + (resultSlot + 1 + 27));
    System.out.println("useRowEnd=" + (resultSlot + 1 + 27 + 9));
  }
}
