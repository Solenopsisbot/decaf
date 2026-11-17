package demo;

import net.neoforged.fml.IExtensionPoint;
import net.neoforged.fml.ModLoadingContext;

/** NeoForge's registerExtensionPoint as mods call it from their constructor (#468). */
public final class Probe {
  /** A mod's extension point (as IConfigScreenFactory is). */
  public static final class Ext implements IExtensionPoint {}

  static boolean called;

  static String attempt(Runnable r) {
    try {
      r.run();
      return "ok";
    } catch (RuntimeException e) {
      return e.getClass().getName();
    }
  }

  public static String rows() {
    called = false;
    ModLoadingContext ctx = ModLoadingContext.get();
    StringBuilder b = new StringBuilder();
    b.append("register=")
        .append(
            attempt(
                () ->
                    ctx.registerExtensionPoint(
                        Ext.class,
                        () -> {
                          called = true;
                          return new Ext();
                        })))
        .append('\n');
    b.append("supplierCalled=").append(called).append('\n');
    b.append("nullPoint=")
        .append(attempt(() -> ctx.registerExtensionPoint(null, Ext::new)))
        .append('\n');
    b.append("nullSupplier=")
        .append(attempt(() -> ctx.registerExtensionPoint(Ext.class, null)))
        .append('\n');
    return b.toString();
  }
}
