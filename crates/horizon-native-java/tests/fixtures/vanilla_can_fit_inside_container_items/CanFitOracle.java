import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.Item;

/** Every 26.3 item whose Item.canFitInsideContainerItems() is false, and the total count. */
public class CanFitOracle {
    public static void main(String[] args) {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        int total = 0;
        for (Item item : BuiltInRegistries.ITEM) {
            total++;
            if (!item.canFitInsideContainerItems()) {
                System.out.println("FALSE\t" + BuiltInRegistries.ITEM.getKey(item) + "\t" + item.getClass().getName());
            }
        }
        System.out.println("TOTAL\t" + total);
    }
}
