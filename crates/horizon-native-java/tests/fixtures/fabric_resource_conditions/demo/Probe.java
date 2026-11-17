package demo;

import com.mojang.serialization.MapCodec;
import net.fabricmc.fabric.api.resource.conditions.v1.ResourceConditionType;
import net.fabricmc.fabric.api.resource.conditions.v1.ResourceConditions;
import net.minecraft.resources.Identifier;

/** Fabric's ResourceConditionType.create and ResourceConditions.register (#454). */
@SuppressWarnings({"rawtypes", "unchecked"})
public final class Probe {
  static Identifier id(String path) {
    return Identifier.fromNamespaceAndPath("probe", path);
  }

  /** A real MapCodec, raw so it fits create's `T extends ResourceCondition`. */
  static MapCodec unit() {
    return MapCodec.unit(Boolean.TRUE);
  }

  static String attempt(Runnable r) {
    try {
      r.run();
      return "ok";
    } catch (RuntimeException e) {
      return e.getClass().getName() + ": " + e.getMessage();
    }
  }

  /** Rows Decaf answers exactly. */
  public static String rows() {
    StringBuilder b = new StringBuilder();
    MapCodec codec = unit();
    Identifier aId = id("a");
    ResourceConditionType a = ResourceConditionType.create(aId, codec);
    ResourceConditionType b2 = ResourceConditionType.create(id("b"), codec);
    b.append("a.id=").append(a.id()).append('\n');
    b.append("a.id same object=").append(a.id() == aId).append('\n');
    b.append("a.codec same object=").append(a.codec() == codec).append('\n');
    b.append("a==b=").append(a == b2).append('\n');
    b.append("a.equals(a)=").append(a.equals(a)).append('\n');
    b.append("a.equals(b)=").append(a.equals(b2)).append('\n');
    b.append("a instanceof type=").append(a instanceof ResourceConditionType).append('\n');
    b.append("register a=").append(attempt(() -> ResourceConditions.register(a))).append('\n');
    b.append("register b=").append(attempt(() -> ResourceConditions.register(b2))).append('\n');
    ResourceConditionType again = ResourceConditionType.create(id("a"), codec);
    b.append("register a again=")
        .append(attempt(() -> ResourceConditions.register(again)))
        .append('\n');
    b.append("register again twice=")
        .append(attempt(() -> ResourceConditions.register(again)))
        .append('\n');
    return b.toString();
  }

  // Counted in Decaf (a null may be a counted call's answer; a fabric: id depends on load
  // order; the identity hash). Their Java 25 answers are in java25_counted.txt.
  public static String nullId() {
    return attempt(() -> ResourceConditionType.create(null, unit()));
  }

  public static String nullCodec() {
    return attempt(() -> ResourceConditionType.create(id("c"), null));
  }

  public static String registerNull() {
    return attempt(() -> ResourceConditions.register(null));
  }

  public static String fabricDefault() {
    ResourceConditionType t =
        ResourceConditionType.create(
            Identifier.fromNamespaceAndPath("fabric", "true"), unit());
    return attempt(() -> ResourceConditions.register(t));
  }

  public static String hash() {
    return attempt(() -> ResourceConditionType.create(id("h"), unit()).hashCode());
  }

  public static void main(String[] args) {
    if (args.length > 0 && args[0].equals("counted")) {
      System.out.print(
          "nullId=" + nullId() + "\nnullCodec=" + nullCodec() + "\nregisterNull=" + registerNull()
              + "\nfabricDefault=" + fabricDefault() + "\n");
    } else {
      System.out.print(rows());
    }
  }
}
