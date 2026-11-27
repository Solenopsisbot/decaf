package demo;

import com.mojang.serialization.Codec;
import java.util.List;

/**
 * The values {@code fixture_jar_component_types.rs} encodes with {@code NbtOps}, and
 * {@code NbtOracle} encodes on a real JDK 25 with Minecraft 26.3's {@code NbtOps.INSTANCE}.
 */
public final class Cases {
  public static final int COUNT = 9;

  public static String name(int n) {
    switch (n) {
      case 0: return "pouch_default_multiplier";
      case 1: return "pouch_multiplier_4";
      case 2: return "pouch_items";
      case 3: return "gauge";
      case 4: return "gauge_nan_and_negative_zero";
      case 5: return "int_list";
      case 6: return "bool";
      case 7: return "bounded_list_too_long";
      default: return "string";
    }
  }

  public static Codec<?> codec(int n) {
    switch (n) {
      case 0: case 1: case 2: return Pouch.CODEC;
      case 3: case 4: return Gauge.CODEC;
      case 5: return Codec.INT.listOf();
      case 6: return Codec.BOOL;
      case 7: return Codec.STRING.listOf(0, 1);
      default: return Codec.STRING;
    }
  }

  public static Object value(int n) {
    switch (n) {
      case 0: return new Pouch(new Pouch.Names(List.of()), 1);
      case 1: return new Pouch(new Pouch.Names(List.of()), 4);
      case 2: return new Pouch(new Pouch.Names(List.of("a", "b")), 12);
      case 3: return new Gauge(true, 5000000000L, 0.5, (byte) 7, (short) -3, 1.25f);
      case 4: return new Gauge(false, 1L, -0.0, (byte) 0, (short) 0, Float.NaN);
      case 5: return List.of(1, 2, 3);
      case 6: return Boolean.TRUE;
      case 7: return List.of("x", "y");
      default: return "text";
    }
  }
}
