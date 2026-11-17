package demo;

import net.minecraft.world.item.ToolMaterial;

/**
 * Vanilla 26.3 ToolMaterial constants and accessors (Wizards, Paladins and Archers read them).
 * The same source runs on Java 25 against the real game jar and its server libraries, after
 * bootstrap (java25.txt), and, compiled against a stub, through the interpreter
 * (fixture_tool_material.rs), which compares rows() byte for byte.
 */
public final class Probe {
  static void row(StringBuilder out, String name, ToolMaterial m) {
    out.append(name).append('|').append(m.durability()).append('|').append(m.speed())
        .append('|').append(m.attackDamageBonus()).append('|').append(m.enchantmentValue())
        .append('\n');
  }

  public static String rows() {
    StringBuilder out = new StringBuilder();
    row(out, "WOOD", ToolMaterial.WOOD);
    row(out, "STONE", ToolMaterial.STONE);
    row(out, "COPPER", ToolMaterial.COPPER);
    row(out, "IRON", ToolMaterial.IRON);
    row(out, "DIAMOND", ToolMaterial.DIAMOND);
    row(out, "GOLD", ToolMaterial.GOLD);
    row(out, "NETHERITE", ToolMaterial.NETHERITE);
    out.append("same|").append(ToolMaterial.IRON == ToolMaterial.IRON).append('\n');
    return out.toString();
  }
}
