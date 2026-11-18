package demo;

import java.util.function.Supplier;
import net.minecraft.core.Direction;
import net.minecraft.world.Difficulty;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.item.Rarity;

/** Game enums whose constants Decaf doesn't list: an enum to `isEnum`, counted beyond that. */
public final class Probe {
  private static String row(Supplier<Object> body) {
    try {
      return String.valueOf(body.get());
    } catch (Throwable t) {
      return t.getClass().getName() + ":" + t.getMessage();
    }
  }

  /** `Class.isEnum`: exact. */
  public static String exact() {
    return "rarity=" + row(() -> Rarity.class.isEnum()) + "\n"
        + "plane=" + row(() -> Direction.Plane.class.isEnum()) + "\n"
        + "mobCategory=" + row(() -> MobCategory.class.isEnum()) + "\n"
        + "dyeColor=" + row(() -> DyeColor.class.isEnum()) + "\n"
        + "difficulty=" + row(() -> Difficulty.class.isEnum()) + "\n";
  }

  /** The constants themselves: Java's answers here, counted calls in Decaf. */
  public static String counted() {
    return "values=" + row(() -> Rarity.values().length) + "\n"
        + "valueOf=" + row(() -> Rarity.valueOf("COMMON").name()) + "\n"
        + "constants=" + row(() -> Rarity.class.getEnumConstants().length) + "\n";
  }

  public static void main(String[] args) {
    System.out.print(exact() + counted());
  }
}
