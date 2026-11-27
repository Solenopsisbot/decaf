package demo;

import java.util.EnumSet;

/**
 * `EnumSet.allOf` as Java 25 answers it (#669): the universe is `values()`, called through
 * `Class.getEnumConstantsShared`, and a `values()` that throws (here a failing static
 * initialiser) makes that answer `null`, so `allOf` throws `ClassCastException("class X not an
 * enum")`, on the first call and on every later one. No element is ever made up.
 */
public final class EnumSetProbe {
  enum Color { RED, GREEN { @Override int weight() { return 2; } }, BLUE;
    static Color DEFAULT = BLUE;
    int weight() { return 1; } }

  enum Empty { ; static Empty NONE = null; }

  enum Broken { ONE, TWO;
    static { boom(); }
    static void boom() { throw new IllegalStateException("broken"); } }

  static String attempt(Class<? extends Enum> type) {
    try {
      @SuppressWarnings("unchecked")
      EnumSet<?> set = EnumSet.allOf((Class) type);
      return set.toString();
    } catch (Throwable t) {
      return t.getClass().getName() + ":" + t.getMessage();
    }
  }

  public static String run() {
    EnumSet<Color> colors = EnumSet.allOf(Color.class);
    StringBuilder out = new StringBuilder();
    out.append(colors).append('|').append(colors.size()).append('|')
        .append(colors.contains(Color.DEFAULT)).append('|')
        .append(colors.iterator().next() == Color.RED).append('|');
    int weights = 0;
    for (Color color : colors) weights = weights * 10 + color.weight();
    out.append(weights).append('|');
    out.append(EnumSet.allOf(Empty.class)).append('|');
    out.append(attempt(Broken.class)).append('|');
    out.append(attempt(Broken.class));
    return out.toString();
  }

  public static void main(String[] args) {
    System.out.println(run());
  }
}
