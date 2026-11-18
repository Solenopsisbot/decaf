package demo;

import java.util.IdentityHashMap;
import java.util.Map;

/** Option c for #544: keep String text exact and count identity ambiguity. */
public final class Probe {
  /** A normal String constructor remains an ordinary text value. */
  public static String normal() {
    String value = new String(new char[] {'o', 'k'});
    return value + ":" + value.length();
  }

  /** A fresh String compared with an interned literal has unknown identity. */
  public static String directIdentity() {
    String value = new String(new char[] {'s', 'a', 'm', 'e'});
    return String.valueOf(value == "same");
  }

  /** Equal-text IdentityHashMap keys remain counted rather than guessed. */
  public static String identityMap() {
    Map<String, Integer> map = new IdentityHashMap<>();
    map.put(new String(new char[] {'k', 'e', 'y'}), 1);
    return String.valueOf(map.get(new String(new char[] {'k', 'e', 'y'})));
  }

  /** A literal lookup is also counted because the model has no String provenance. */
  public static String literalLookup() {
    Map<String, Integer> map = new IdentityHashMap<>();
    map.put("key", 1);
    return String.valueOf(map.get("key"));
  }

  public static void main(String[] args) {
    System.out.print("normal=" + normal() + "\n");
    System.out.print("directIdentity=" + directIdentity() + "\n");
    System.out.print("identityMap=" + identityMap() + "\n");
    System.out.print("literalLookup=" + literalLookup() + "\n");
  }
}
