package demo;

import java.io.Serializable;
import java.util.AbstractList;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedList;
import java.util.List;
import java.util.RandomAccess;
import java.util.Set;

/**
 * A jar object cast to a JDK type (#242): its class chain decides, as the JVM's `checkcast` does, with Java's
 * message; `instanceof` agrees.
 */
public final class Probe {
    static class Klass {}

    static final class MyList extends ArrayList<String> {}

    enum Color { RED }

    record Point(int x) {}

    interface Shape {}

    static final class Square implements Shape, Comparable<Square> {
        @Override
        public int compareTo(Square other) {
            return 0;
        }
    }

    static final class Res implements java.io.Closeable {
        @Override
        public void close() {}
    }

    /** Its superclass is not in the jar (`missing.UnheldBase extends Number`). */
    static final class Leaves extends missing.UnheldBase {}

    interface Step {
        Object run();
    }

    static String c(Step step) {
        try {
            step.run();
            return "ok";
        } catch (ClassCastException e) {
            return "CCE(" + e.getMessage() + ")";
        }
    }

    static String b(boolean v) {
        return v ? "T" : "F";
    }

    /** A plain jar class: only `Object` (and itself). */
    public static String klass() {
        Object o = new Klass();
        return c(() -> (String) o) + "|" + c(() -> (Number) o) + "|" + c(() -> (Comparable<?>) o) + "|"
            + c(() -> (Runnable) o) + "|" + c(() -> (List<?>) o) + "|" + c(() -> (Object[]) o) + "|"
            + c(() -> (String[]) o) + "|" + c(() -> (int[]) o) + "|" + c(() -> (Object) o) + "|"
            + b(o instanceof String) + b(o instanceof Comparable) + b(o instanceof Runnable) + b(o instanceof List)
            + b(o instanceof Object[]) + b(o instanceof Object);
    }

    /** A jar subclass of `ArrayList`: the JDK's chain above it. */
    public static String myList() {
        Object o = new MyList();
        return c(() -> (List<?>) o) + "|" + c(() -> (Collection<?>) o) + "|" + c(() -> (RandomAccess) o) + "|"
            + c(() -> (ArrayList<?>) o) + "|" + c(() -> (AbstractList<?>) o) + "|" + c(() -> (Set<?>) o) + "|"
            + c(() -> (LinkedList<?>) o) + "|" + c(() -> (String) o) + "|" + b(o instanceof List)
            + b(o instanceof RandomAccess) + b(o instanceof Set) + b(o instanceof LinkedList) + b(o instanceof Cloneable)
            + b(o instanceof Serializable);
    }

    /** A jar enum: `Enum`'s chain. */
    public static String color() {
        Object o = Color.RED;
        return c(() -> (Enum<?>) o) + "|" + c(() -> (Comparable<?>) o) + "|" + c(() -> (Serializable) o) + "|"
            + c(() -> (String) o) + "|" + c(() -> (Runnable) o) + "|" + b(o instanceof Enum) + b(o instanceof Comparable)
            + b(o instanceof Serializable) + b(o instanceof String) + b(o instanceof Runnable);
    }

    /** A jar record: `Record`'s chain, nothing it does not declare. */
    public static String point() {
        Object o = new Point(1);
        return c(() -> (Record) o) + "|" + c(() -> (Serializable) o) + "|" + c(() -> (Comparable<?>) o) + "|"
            + c(() -> (String) o) + "|" + b(o instanceof Record) + b(o instanceof Serializable)
            + b(o instanceof Comparable);
    }

    /** A jar class implementing a JDK interface and a jar one. */
    public static String square() {
        Object o = new Square();
        return c(() -> (Comparable<?>) o) + "|" + c(() -> (Shape) o) + "|" + c(() -> (Runnable) o) + "|"
            + c(() -> (String) o) + "|" + b(o instanceof Comparable) + b(o instanceof Shape) + b(o instanceof Runnable);
    }

    /** A jar class implementing a JDK interface whose superinterface the cast names. */
    public static String res() {
        Object o = new Res();
        return c(() -> (AutoCloseable) o) + "|" + c(() -> (java.io.Closeable) o) + "|" + c(() -> (Runnable) o) + "|"
            + b(o instanceof AutoCloseable) + b(o instanceof Runnable);
    }

    /**
     * A jar class whose superclass the jar leaves out: `Number` (not final) and `Runnable` are not known here;
     * `String` is final, so no unheld class extends it.
     */
    public static String leaves() {
        Object o = new Leaves();
        return c(() -> (Number) o) + "|" + c(() -> (Runnable) o) + "|" + c(() -> (String) o) + "|"
            + b(o instanceof Number) + b(o instanceof Runnable) + b(o instanceof String);
    }

    public static void main(String[] args) {
        System.out.println("klass=" + klass());
        System.out.println("myList=" + myList());
        System.out.println("color=" + color());
        System.out.println("point=" + point());
        System.out.println("square=" + square());
        System.out.println("res=" + res());
        System.out.println("leaves=" + leaves());
    }
}
