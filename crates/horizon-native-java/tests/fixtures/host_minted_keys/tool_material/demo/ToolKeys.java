package demo;

import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.ToolMaterial;

/**
 * A jar's own `new ToolMaterial(..)` made after reading `ToolMaterial.WOOD` is another object
 * (#669): the constant and the new record never share a key. Run against the real 26.3 server
 * jar (`command.txt`) and, compiled against the stubs beside it, through the interpreter.
 */
public final class ToolKeys {
  public static String run() {
    ToolMaterial wood = ToolMaterial.WOOD;
    ToolMaterial made = new ToolMaterial(null, 7, 1.5f, 0.5f, 3, null);
    return (made == wood) + "|" + (made == ToolMaterial.WOOD) + "|" + wood.durability() + "|"
        + made.durability();
  }

  public static void main(String[] args) {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    System.out.println("ROW " + run());
  }
}
