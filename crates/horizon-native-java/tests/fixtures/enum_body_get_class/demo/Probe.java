package demo;

import java.util.function.Supplier;

/**
 * getClass() and the class surface of enum constants with and without a body, and of a plain
 * anonymous class. The same source runs on Java 25 (java25.txt) and, compiled as it is (plain
 * JDK, no stubs), through the interpreter (fixture_enum_body_get_class.rs), which compares
 * rows() byte for byte. Each row is computed on its own, so one wrong answer cannot hide the
 * others.
 */
public class Probe {
  enum Body {
    A {},
    B {},
    C
  }

  enum Plain {
    X,
    Y
  }

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
    Object anon = new Object() {};
    row(out, "A.class", () -> Body.A.getClass().getName());
    row(out, "B.class", () -> Body.B.getClass().getName());
    row(out, "C.class", () -> Body.C.getClass().getName());
    row(out, "X.class", () -> Plain.X.getClass().getName());
    row(out, "A.isBody", () -> Body.A.getClass() == Body.class);
    row(out, "C.isBody", () -> Body.C.getClass() == Body.class);
    row(out, "A.declaring", () -> Body.A.getDeclaringClass().getName());
    row(out, "C.declaring", () -> Body.C.getDeclaringClass().getName());
    row(out, "A.super", () -> Body.A.getClass().getSuperclass().getName());
    row(out, "C.super", () -> Body.C.getClass().getSuperclass().getName());
    row(out, "A.isEnum", () -> Body.A.getClass().isEnum());
    row(out, "C.isEnum", () -> Body.C.getClass().isEnum());
    row(out, "A.instanceofString", () -> ((Object) Body.A) instanceof String);
    row(out, "A.instanceofBody", () -> ((Object) Body.A) instanceof Body);
    row(out, "A.name", () -> Body.A.name());
    row(out, "B.ordinal", () -> Body.B.ordinal());
    row(out, "valueOf.same", () -> Body.valueOf("A") == Body.A);
    row(out, "valueOf.class", () -> Body.valueOf("B").getClass().getName());
    row(out, "values.class", () -> Body.values()[0].getClass().getName());
    row(out, "anon.class", () -> anon.getClass().getName());
    return out.toString();
  }

  public static void main(String[] args) {
    System.out.print(rows());
  }
}
