package demo;

import java.util.Set;
import java.util.function.Supplier;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.enchantment.Enchantments;

/**
 * The vanilla `Enchantments` ResourceKey constants (Spell Engine puts three of them in a
 * Set). The same source runs on Java 25 against the real 26.3 game jar after bootstrap
 * (java25.txt) and, compiled against stubs, through the interpreter
 * (fixture_vanilla_enchantment_keys.rs). Each row is computed on its own.
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
    row(out, "same", () -> Enchantments.KNOCKBACK == Enchantments.KNOCKBACK);
    row(out, "different", () -> Enchantments.KNOCKBACK != Enchantments.FIRE_ASPECT);
    row(out, "equals", () -> Enchantments.KNOCKBACK.equals(Enchantments.KNOCKBACK));
    row(out, "notEquals", () -> Enchantments.KNOCKBACK.equals(Enchantments.LOOTING));
    row(
        out,
        "set.size",
        () -> Set.of(Enchantments.KNOCKBACK, Enchantments.FIRE_ASPECT, Enchantments.LOOTING).size());
    row(
        out,
        "set.contains",
        () ->
            Set.of(Enchantments.KNOCKBACK, Enchantments.FIRE_ASPECT, Enchantments.LOOTING)
                .contains(Enchantments.LOOTING));
    row(
        out,
        "set.missing",
        () ->
            Set.of(Enchantments.KNOCKBACK, Enchantments.FIRE_ASPECT)
                .contains(Enchantments.PROTECTION));
    row(out, "set.duplicate", () -> Set.of(Enchantments.KNOCKBACK, Enchantments.KNOCKBACK).size());
    row(out, "text", () -> Enchantments.KNOCKBACK.toString());
    row(out, "identifier", () -> Enchantments.KNOCKBACK.identifier().toString());
    row(out, "registry", () -> Enchantments.KNOCKBACK.registry().toString());
    row(
        out,
        "created.default",
        () ->
            ResourceKey.create(Registries.ENCHANTMENT, Identifier.withDefaultNamespace("knockback"))
                == Enchantments.KNOCKBACK);
    row(
        out,
        "created.parsed",
        () ->
            ResourceKey.create(Registries.ENCHANTMENT, Identifier.parse("minecraft:looting"))
                == Enchantments.LOOTING);
    row(
        out,
        "created.other",
        () ->
            ResourceKey.create(Registries.ENCHANTMENT, Identifier.parse("minecraft:looting"))
                == Enchantments.KNOCKBACK);
    return out.toString();
  }
}
