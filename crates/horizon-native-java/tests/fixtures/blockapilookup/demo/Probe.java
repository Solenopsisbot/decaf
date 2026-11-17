package demo;

import java.util.function.Supplier;
import net.fabricmc.fabric.api.lookup.v1.block.BlockApiLookup;
import net.minecraft.core.Direction;
import net.minecraft.resources.Identifier;

/** Fabric's BlockApiLookup as Balm uses it (#412): one row per check, each in its own try/catch. */
public class Probe {
  interface Energy {}
  interface Fluid {}

  private static String row(Supplier<Object> body) {
    try {
      return String.valueOf(body.get());
    } catch (Throwable t) {
      return t.getClass().getName() + ":" + t.getMessage();
    }
  }

  public static String rows() {
    Identifier id = Identifier.fromNamespaceAndPath("demo", "energy");
    BlockApiLookup<Energy, Direction> first = BlockApiLookup.get(id, Energy.class, Direction.class);
    StringBuilder s = new StringBuilder();
    s.append("same=").append(row(() -> first == BlockApiLookup.get(id, Energy.class, Direction.class))).append('\n');
    s.append("id=").append(row(() -> first.getId())).append('\n');
    s.append("api=").append(row(() -> first.apiClass().getName())).append('\n');
    s.append("context=").append(row(() -> first.contextClass().getName())).append('\n');
    s.append("otherApi=").append(row(() -> BlockApiLookup.get(id, Fluid.class, Direction.class))).append('\n');
    s.append("otherContext=").append(row(() -> BlockApiLookup.get(id, Energy.class, Void.class))).append('\n');
    s.append("fallbackNull=").append(row(() -> { first.registerFallback(null); return "ok"; })).append('\n');
    s.append("fallback=").append(row(() -> { first.registerFallback((w, p, st, be, c) -> null); return "ok"; }));
    return s.toString();
  }

  public static void main(String[] args) {
    System.out.print(rows());
  }
}
