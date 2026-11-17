package demo;

import com.google.common.collect.Maps;
import com.google.common.collect.Sets;
import java.util.IdentityHashMap;
import java.util.Map;
import java.util.Set;

/** Identity collections with equal-text strings that are different objects (#464). */
public final class Probe {
  /** A string equal to {@code text} that is a new object each call. */
  static String fresh(String text) {
    return new StringBuilder(text).toString();
  }

  /**
   * Distinct texts: no identity question, answered exactly. (Asking for "a" again would be one:
   * two equal literals are the same interned object in Java, which Decaf can't tell from two
   * equal strings, so that lookup is counted.)
   */
  public static String exact() {
    Map<String, Integer> m = new IdentityHashMap<>();
    m.put("a", 1);
    m.put("b", 2);
    return m.size() + ":" + m.containsKey("c") + ":" + m.get("c");
  }

  /** JDK IdentityHashMap: two equal-text keys, then reads. */
  public static String jdkMap() {
    Map<String, Integer> m = new IdentityHashMap<>();
    m.put(fresh("x"), 1);
    m.put(fresh("x"), 2);
    return m.size() + ":" + m.get(fresh("x"));
  }

  /** Guava's identity map, the same. */
  public static String guavaMap() {
    Map<String, Integer> m = Maps.newIdentityHashMap();
    m.put(fresh("x"), 1);
    m.put(fresh("x"), 2);
    return m.size() + ":" + m.get(fresh("x"));
  }

  /** Guava's identity set: two equal-text elements, then reads. */
  public static String guavaSet() {
    Set<String> s = Sets.newIdentityHashSet();
    s.add(fresh("x"));
    s.add(fresh("x"));
    return s.size() + ":" + s.contains(fresh("x"));
  }

  public static void main(String[] args) {
    System.out.print(
        "exact=" + exact() + "\njdkMap=" + jdkMap() + "\nguavaMap=" + guavaMap() + "\nguavaSet="
            + guavaSet() + "\n");
  }
}
