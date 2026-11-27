package demo;

import java.lang.reflect.Field;

/** Each `getDeclaredField` hands out a new `Field` copy (#669): never one object for two. */
public final class FieldCopies {
  public int x;

  public static String run() throws Exception {
    Field a = FieldCopies.class.getDeclaredField("x");
    Field b = FieldCopies.class.getDeclaredField("x");
    Field c = FieldCopies.class.getDeclaredField("x");
    return "" + (a == b) + "|" + (b == c) + "|" + a.equals(c);
  }

  public static void main(String[] args) throws Exception {
    System.out.println(run());
  }
}
