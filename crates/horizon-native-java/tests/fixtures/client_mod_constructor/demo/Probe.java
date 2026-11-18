package demo;
import java.util.function.Supplier;
import fuzs.puzzleslib.common.api.client.core.v1.ClientModConstructor;
import fuzs.puzzleslib.common.api.core.v1.ModConstructor;
public final class Probe {
  static final class Demo implements ClientModConstructor, ModConstructor {
    static int constructed, setup;
    public void onConstructMod() { constructed++; }
    public void onClientSetup() { setup++; }
  }
  public static String normal() {
    ClientModConstructor.construct("demo", Demo::new);
    return "constructed=" + Demo.constructed + ",setup=" + Demo.setup;
  }
  public static void main(String[] args) { System.out.println(normal()); }
  public static String counted() {
    ModConstructor.construct("demo", Demo::new);
    return "ok";
  }
}
