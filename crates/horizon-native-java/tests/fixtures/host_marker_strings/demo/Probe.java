package demo;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.function.Supplier;

/**
 * A jar String prints as itself whatever it contains (#330): the Strings here are spelled like
 * the runtime's own callback and collection keys. One row per String and print path.
 */
public final class Probe {
  private static String row(Supplier<Object> body) {
    try {
      return String.valueOf(body.get());
    } catch (Throwable t) {
      return t.getClass().getName() + ":" + t.getMessage();
    }
  }

  private static String built(String head, String tail) {
    return new StringBuilder().append(head).append(tail).toString();
  }

  public static String strings() {
    String[] texts = {
      built("abc/", "hcb:x"),
      built("a/b,", "hcb:c"),
      built("foo/hopt:", "hcb:@indy:x"),
      built("list/", "hobj:x"),
      built("h", "cb:"),
      "abc/hcb:literal",
    };
    StringBuilder out = new StringBuilder();
    for (int i = 0; i < texts.length; i++) {
      String x = texts[i];
      Object o = x;
      out.append("valueOf").append(i).append('=').append(row(() -> String.valueOf(o))).append('\n');
      out.append("concat").append(i).append('=').append(row(() -> "<" + x + ">")).append('\n');
      out.append("append").append(i).append('=')
          .append(row(() -> new StringBuilder().append(o).toString())).append('\n');
      out.append("list").append(i).append('=').append(row(() -> List.of(x).toString())).append('\n');
      out.append("arrayList").append(i).append('=')
          .append(row(() -> new ArrayList<>(List.of(x)).toString())).append('\n');
      out.append("map").append(i).append('=').append(row(() -> Map.of(x, x).toString())).append('\n');
      out.append("optional").append(i).append('=').append(row(() -> Optional.of(x).toString())).append('\n');
      out.append("methods").append(i).append('=')
          .append(row(() -> x.length() + "|" + x.toUpperCase() + "|" + x.substring(0, 3))).append('\n');
    }
    return out.toString();
  }

  /** Java prints each lambda's identity here, which differs every run: the test counts these. */
  public static String collections() {
    Runnable r = () -> {};
    Object[] holders = {List.of(r), new ArrayList<>(List.of(r)), Map.of("k", r), Optional.of(r)};
    StringBuilder out = new StringBuilder();
    for (Object holder : holders) {
      out.append(String.valueOf(holder)).append('\n');
    }
    return out.toString();
  }

  public static void main(String[] args) {
    System.out.print(strings());
  }
}
