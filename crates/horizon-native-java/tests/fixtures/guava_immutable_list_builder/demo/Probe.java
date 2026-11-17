package demo;

import com.google.common.collect.ImmutableList;
import java.util.List;

/**
 * Guava 33.6.0 ImmutableList.builder probe. The same source runs twice: against the real
 * Guava jar on Java 25 (its output is java25.txt) and, compiled against stubs, through the
 * interpreter (fixture_guava_immutable_list_builder.rs), which compares rows() byte for byte.
 */
public class Probe {
  static void p(StringBuilder out, String k, Object v) {
    out.append(k).append('=').append(v).append('\n');
  }

  public static String rows() {
    StringBuilder out = new StringBuilder();
    ImmutableList.Builder<String> b = ImmutableList.builder();
    p(out, "empty", b.build().size());
    p(out, "emptyIdentity", b.build() == ImmutableList.<String>of());
    try {
      b.add((String) null);
      p(out, "null", "ok");
    } catch (Exception e) {
      p(out, "null", e.getClass().getName() + ":" + e.getMessage());
    }
    b.add("a").add("b");
    p(out, "order", b.build());
    ImmutableList<String> c = b.build();
    b.add("c");
    p(out, "reuse", c + "/" + b.build());
    ImmutableList.Builder<String> d = ImmutableList.builderWithExpectedSize(3);
    d.add("x", "y");
    d.addAll(List.of("z"));
    p(out, "varargsIterable", d.build());
    ImmutableList.Builder<String> e = ImmutableList.builder();
    e.addAll(List.of("i", "j").iterator());
    p(out, "iterator", e.build());
    return out.toString();
  }

  public static void main(String[] args) {
    System.out.print(rows());
  }
}
