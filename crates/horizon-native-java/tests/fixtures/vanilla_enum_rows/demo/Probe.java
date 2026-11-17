package demo;

import java.util.function.Supplier;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.entity.ai.attributes.AttributeModifier;

/**
 * Vanilla enum constants mods read (`SoundSource`, `AttributeModifier.Operation`,
 * `HumanoidArm`): `values()` order, `name()`, `ordinal()`, `valueOf` identity and
 * `SoundSource.getName()`. The same source runs on Java 25 against the real 26.3 game jar
 * after bootstrap (java25.txt) and, compiled against stubs, through the interpreter
 * (fixture_vanilla_enum_rows.rs). Each row is computed on its own.
 */
public final class Probe {
  static void row(StringBuilder out, String name, Supplier<Object> value) {
    out.append(name).append('=');
    try {
      out.append(value.get());
    } catch (RuntimeException e) {
      out.append(e.getClass().getName()).append(':').append(e.getMessage());
    }
    out.append('\n');
  }

  public static String rows() {
    StringBuilder out = new StringBuilder();
    row(out, "SoundSource.count", () -> SoundSource.values().length);
    for (int i = 0; i < 11; i++) {
      int at = i;
      row(
          out,
          "SoundSource." + at,
          () -> {
            SoundSource s = SoundSource.values()[at];
            return s.name() + "|" + s.ordinal() + "|" + s.getName();
          });
    }
    row(out, "SoundSource.valueOf", () -> SoundSource.valueOf("PLAYERS") == SoundSource.PLAYERS);
    row(out, "SoundSource.field", () -> SoundSource.RECORDS.getName());
    row(out, "SoundSource.valueOf.values", () -> SoundSource.valueOf("UI") == SoundSource.values()[10]);
    row(out, "SoundSource.valueOf.unknown", () -> SoundSource.valueOf("NOPE"));
    row(out, "SoundSource.valueOf.lowercase", () -> SoundSource.valueOf("players"));
    row(out, "SoundSource.valueOf.null", () -> SoundSource.valueOf(null));
    row(out, "SoundSource.fresh", () -> SoundSource.values() != SoundSource.values());
    row(out, "Operation.count", () -> AttributeModifier.Operation.values().length);
    for (int i = 0; i < 3; i++) {
      int at = i;
      row(
          out,
          "Operation." + at,
          () -> {
            AttributeModifier.Operation o = AttributeModifier.Operation.values()[at];
            return o.name() + "|" + o.ordinal();
          });
    }
    row(
        out,
        "Operation.valueOf",
        () ->
            AttributeModifier.Operation.valueOf("ADD_VALUE")
                == AttributeModifier.Operation.ADD_VALUE);
    row(out, "Operation.valueOf.unknown", () -> AttributeModifier.Operation.valueOf("NOPE"));
    row(out, "HumanoidArm.count", () -> HumanoidArm.values().length);
    for (int i = 0; i < 2; i++) {
      int at = i;
      row(
          out,
          "HumanoidArm." + at,
          () -> {
            HumanoidArm a = HumanoidArm.values()[at];
            return a.name() + "|" + a.ordinal();
          });
    }
    row(out, "HumanoidArm.valueOf", () -> HumanoidArm.valueOf("RIGHT") == HumanoidArm.RIGHT);
    return out.toString();
  }
}
