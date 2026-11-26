package demo;

import java.util.*;

/**
 * An enum constant's text is its class's `toString()`, dispatched virtually (#670): a
 * constant-specific body's override (`GREEN`, compiled as a subclass), an enum-wide override
 * (`Shade`), or `Enum.toString()`, the name. Every way a jar turns one into text agrees.
 */
public final class EnumText {
  enum Color { RED, GREEN { @Override public String toString() { return "green!"; } }, BLUE }

  enum Shade { DARK, LIGHT; @Override public String toString() { return "s:" + name(); } }

  public static String run() {
    StringBuilder out = new StringBuilder();
    out.append(EnumSet.allOf(Color.class)).append('|');
    out.append(new ArrayList<>(List.of(Color.RED, Color.GREEN))).append('|');
    Map<String, Color> map = new HashMap<>();
    map.put("g", Color.GREEN);
    out.append(map).append('|').append(map.values()).append('|');
    out.append(List.of(Color.GREEN, Shade.LIGHT)).append('|');
    out.append(String.valueOf(Color.GREEN)).append('|').append("" + Color.GREEN).append('|');
    out.append(Color.GREEN.name()).append('|');
    out.append(EnumSet.allOf(Shade.class)).append('|');
    out.append(new StringBuilder().append((Object) Color.GREEN));
    return out.toString();
  }

  public static void main(String[] args) {
    System.out.println(run());
  }
}
