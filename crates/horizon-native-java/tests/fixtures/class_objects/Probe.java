package demo;

import java.io.Serializable;
import java.util.AbstractList;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.EnumSet;
import java.util.EnumMap;
import java.util.RandomAccess;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import net.minecraft.core.Direction;

/**
 * {@code Class} objects are handles, one per name in a run (#156 P13b1): every producer ({@code
 * ldc}, {@code getClass}, {@code Class.forName}, {@code Integer.TYPE}, {@code getComponentType},
 * {@code getSuperclass}, {@code getInterfaces}) answers the same object for one class, and the
 * {@code Class} members answer from the class file or Java 25's table.
 */
public class Probe {
  public static class Foo {}

  public interface Shape {}

  public static class Bar extends Foo implements Shape {}

  public enum Color {
    RED,
    GREEN
  }

  /** The first constant has a class body, so its runtime class is Kind$1. */
  public enum Kind {
    A {
      @Override
      public String toString() {
        return "kind-a";
      }
    },
    B
  }

  public enum E {
    A {
      @Override
      public String toString() { return "a-body"; }
    },
    B
  }

  static String bits(boolean... values) {
    StringBuilder out = new StringBuilder();
    for (boolean value : values) {
      out.append(value ? '1' : '0');
    }
    return out.toString();
  }

  /** One class, one object, whichever producer made it. */
  public static String identity() throws Exception {
    Class<?> literal = Foo.class;
    Class<?> fromObject = new Foo().getClass();
    Class<?> byName = Class.forName("demo.Probe$Foo");
    Object text = "text";
    return bits(
        literal == fromObject,
        literal == byName,
        fromObject == byName,
        literal.equals(byName),
        text.getClass() == String.class,
        Class.forName("java.lang.String") == String.class,
        Integer.valueOf(3).getClass() == Integer.class,
        new ArrayList<String>().getClass() == ArrayList.class,
        (Object) Foo.class != (Object) Bar.class,
        !Foo.class.equals(Bar.class),
        Foo.class.hashCode() == byName.hashCode(),
        new Bar().getClass().getSuperclass() == Foo.class);
  }

  /** Primitive classes: their own objects, apart from the wrappers. */
  public static String primitives() {
    return bits(
            int.class == Integer.TYPE,
            (Object) int.class != (Object) Integer.class,
            void.class == Void.TYPE,
            boolean.class == Boolean.TYPE,
            int.class.isPrimitive(),
            !Integer.class.isPrimitive(),
            !int.class.isArray(),
            int.class.getSuperclass() == null,
            int.class.getInterfaces().length == 0)
        + ","
        + int.class.getName()
        + ","
        + void.class.getName()
        + ","
        + int.class
        + ","
        + long.class.getSimpleName()
        + ","
        + double.class.getTypeName();
  }

  /** Array classes: one per component, named by their descriptor. */
  public static String arrays() {
    return bits(
            int[].class == new int[0].getClass(),
            String[].class == new String[1].getClass(),
            int[][].class == new int[2][3].getClass(),
            String[][].class.getComponentType() == String[].class,
            int[].class.getComponentType() == int.class,
            String[].class.getComponentType() == String.class,
            int[].class.isArray(),
            !int[].class.isPrimitive(),
            String.class.getComponentType() == null,
            int[].class != (Object) long[].class)
        + ","
        + int[].class.getName()
        + ","
        + String[].class.getName()
        + ","
        + String[][].class.getName()
        + ","
        + String[][].class.getTypeName()
        + ","
        + int[].class.getSimpleName()
        + ","
        + int[].class
        + ","
        + Foo[].class.getName()
        + ","
        + int[].class.getSuperclass().getName();
  }

  /** A jar class's names and its {@code toString}. */
  public static String names() {
    return Foo.class.getName()
        + ","
        + Foo.class.getSimpleName()
        + ","
        + Foo.class.getTypeName()
        + ","
        + Foo.class.getPackageName()
        + ","
        + Foo.class
        + ","
        + Shape.class
        + ","
        + String.valueOf(Probe.class)
        + ","
        + Shape.class.toString()
        + ","
        + List.class
        + ","
        + String.class;
  }

  /** JDK classes from Java 25's supertype table. */
  public static String jdk() {
    return bits(
        ArrayList.class.getSuperclass() == AbstractList.class,
        List.class.isInterface(),
        !ArrayList.class.isInterface(),
        Collection.class.isAssignableFrom(ArrayList.class),
        !ArrayList.class.isAssignableFrom(Collection.class),
        RandomAccess.class.isAssignableFrom(ArrayList.class),
        Serializable.class.isAssignableFrom(String.class),
        List.class.isInstance(new ArrayList<String>()),
        !List.class.isInstance("text"),
        Runnable.class.getSuperclass() == null,
        Object.class.getSuperclass() == null,
        Object.class.isAssignableFrom(Foo.class),
        ThreadPoolExecutor.class.getInterfaces().length == 0,
        CharSequence.class.isInstance("text"));
  }

  /** A jar class's members from its class file. */
  public static String members() {
    Object bar = new Bar();
    Class<?>[] interfaces = Bar.class.getInterfaces();
    boolean castThrew;
    try {
      Bar.class.cast(new Foo());
      castThrew = false;
    } catch (ClassCastException expected) {
      castThrew = true;
    }
    return bits(
            Shape.class.isInterface(),
            !Foo.class.isInterface(),
            Foo.class.getSuperclass() == Object.class,
            Bar.class.getSuperclass() == Foo.class,
            interfaces.length == 1 && interfaces[0] == Shape.class,
            Foo.class.isAssignableFrom(Bar.class),
            !Bar.class.isAssignableFrom(Foo.class),
            Shape.class.isAssignableFrom(Bar.class),
            Foo.class.isInstance(bar),
            !Bar.class.isInstance(new Foo()),
            !Foo.class.isInstance(null),
            Foo.class.cast(bar) == bar,
            Foo.class.cast(null) == null,
            castThrew,
            Foo.class.getEnumConstants() == null,
            !Probe.class.desiredAssertionStatus())
        + ","
        + Color.class.getEnumConstants().length
        + ","
        + Color.class.getEnumConstants()[1]
        + ","
        + Foo.class.getModifiers()
        + ","
        + Shape.class.getModifiers()
        + ","
        + Probe.class.getModifiers();
  }

  /** Enum declaring classes differ from constant-specific runtime classes. */
  public static String enumReview() {
    StringBuilder out = new StringBuilder();
    try { Enum.valueOf((Class) E.A.getClass(), "A"); out.append("ok"); }
    catch (IllegalArgumentException expected) { out.append("iae"); }
    out.append(":");
    try { Enum.valueOf(E.class, "enum/demo/Probe$E/A"); out.append("ok"); }
    catch (IllegalArgumentException expected) { out.append("iae"); }
    out.append(":");
    try { Enum.valueOf(E.class, "NO_SUCH"); out.append("ok"); }
    catch (IllegalArgumentException expected) { out.append("iae"); }
    try {
      EnumSet<E> set = EnumSet.of(E.A, E.B);
      EnumMap<E, String> map = new EnumMap<>(E.class);
      map.put(E.A, "aye"); map.put(E.B, "bee");
      out.append(":").append(set.size()).append(":").append(E.A).append(":").append(E.B)
        .append(":").append(set.contains(E.A)).append(":").append(set)
        .append(":").append(map);
    } catch (NullPointerException counted) {
      out.append("counted");
    }
    Object[] states = Thread.State.class.getEnumConstants();
    Object[] statesAgain = Thread.State.class.getEnumConstants();
    out.append(":").append(states.length).append(":")
      .append(states[0]).append(":").append(statesAgain != states).append(":")
      .append(Thread.State.class.getEnumConstants()[0]).append(":")
      .append(E.A.getDeclaringClass().getName());
    return out.toString();
  }

  static String enumFailure(Class<?> type, String name) {
    try {
      Enum.valueOf((Class) type, name);
      return "ok";
    } catch (RuntimeException error) {
      return error.toString();
    }
  }

  static String enumNames(Object[] values) {
    StringBuilder out = new StringBuilder("[");
    for (int i = 0; i < values.length; i++) {
      if (i > 0) out.append(", ");
      out.append(values[i]);
    }
    return out.append("]").toString();
  }

  /** Null/non-enum Enum.valueOf and JDK/game enum reflection re-check. */
  public static String enumReflectionRecheck() throws Exception {
    Object[] direction = Direction.class.getEnumConstants();
    return enumFailure(E.class, null)
        + "|" + enumFailure(null, "A")
        + "|" + enumFailure(String.class, "A")
        + "|" + (E.A.getClass().isEnum()) + "," + E.class.isEnum()
        + "|" + enumNames(E.class.getEnumConstants())
        + "|" + (E.class.getEnumConstants() != E.class.getEnumConstants())
        + "|" + enumNames(TimeUnit.class.getEnumConstants())
        + "|" + enumNames(TimeUnit.values())
        + "|" + enumNames(Thread.State.class.getEnumConstants())
        + "|" + enumNames(Thread.State.values())
        + "|" + enumNames(direction)
        + "|" + (direction != Direction.class.getEnumConstants());
  }

  public static String enumClasses() {
    Object[] first = Kind.class.getEnumConstants();
    Object[] second = Kind.class.getEnumConstants();
    return (Kind.A.getClass() == Kind.class)
        + ","
        + (Kind.A.getDeclaringClass() == Kind.class)
        + ","
        + (Kind.A.getClass().getEnumConstants() == null)
        + ","
        + first.length
        + ","
        + first[0]
        + ","
        + first[1]
        + ","
        + (first != second)
        + ","
        + Kind.A.getClass().getName();
  }
}
