package demo;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.Item;
import net.minecraft.core.registries.BuiltInRegistries;
public final class ItemsCapture {
  public static void main(String[] args) throws Exception {
    // Bootstrap routes System.out into the game's logger ("[main/INFO]: [STDOUT]: ..."):
    // keep the real stdout so the capture is just the rows.
    java.io.PrintStream out = System.out;
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    for (Field f : Items.class.getDeclaredFields())
      if (Modifier.isStatic(f.getModifiers()) && f.getType() == Item.class)
        out.println(f.getName()+"|"+BuiltInRegistries.ITEM.getKey((Item)f.get(null)));
  }
}
