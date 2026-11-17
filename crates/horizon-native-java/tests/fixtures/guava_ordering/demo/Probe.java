package demo;

import com.google.common.collect.Ordering;

/**
 * Guava 33.6.0 Ordering.natural()/reverse() probe. The same source runs on Java 25 against
 * the real Guava jar (java25.txt) and, compiled against stubs, through the interpreter
 * (fixture_guava_ordering.rs), which compares rows() byte for byte. The string pairs differ
 * by more than one UTF-16 unit, or in length, so a model answering only -1/0/1 fails.
 */
public final class Probe {
  static void p(StringBuilder out, String k, Object v) {
    out.append(k).append('=').append(v).append('\n');
  }

  @SuppressWarnings({"rawtypes", "unchecked"})
  public static String rows() {
    StringBuilder out = new StringBuilder();
    Ordering n = Ordering.natural();
    Ordering r = n.reverse();
    p(out, "natural.a.b", n.compare("a", "b"));
    p(out, "natural.a.c", n.compare("a", "c"));
    p(out, "natural.ab.abcd", n.compare("ab", "abcd"));
    p(out, "natural.same", n.compare("x", "x"));
    p(out, "natural.3.7", n.compare(3, 7));
    p(out, "reverse.a.c", r.compare("a", "c"));
    p(out, "reverse.ab.abcd", r.compare("ab", "abcd"));
    p(out, "naturalIdentity", Ordering.natural() == n);
    p(out, "reverseIdentity", n.reverse() == r);
    p(out, "reverseReverse", r.reverse() == n);
    try {
      n.compare(null, "a");
      p(out, "natural.null", "ok");
    } catch (Exception e) {
      p(out, "natural.null", e.getClass().getName() + ":" + e.getMessage());
    }
    try {
      r.compare("a", null);
      p(out, "reverse.null", "ok");
    } catch (Exception e) {
      p(out, "reverse.null", e.getClass().getName() + ":" + e.getMessage());
    }
    return out.toString();
  }

  public static void main(String[] args) {
    System.out.print(rows());
  }
}
