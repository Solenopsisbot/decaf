package demo;

/**
 * Four array stores of an int that javac narrowed with i2c / i2s / i2b. A test and Run.java turn each narrowing
 * into a nop (see README.md), so castore, sastore and bastore receive the wide int itself, as a hand-written
 * class file (or generated code) can pass it. No branches, so no StackMapTable is involved.
 * boolStore is a byte[] store turned into a boolean[] store by changing its newarray operand from 8 to 4.
 */
public final class Raw {
    public static int charStore(int v) {
        char[] a = new char[1];
        a[0] = (char) v;
        return a[0];
    }

    public static int shortStore(int v) {
        short[] a = new short[1];
        a[0] = (short) v;
        return a[0];
    }

    public static int byteStore(int v) {
        byte[] a = new byte[1];
        a[0] = (byte) v;
        return a[0];
    }

    public static int boolStore(int v) {
        byte[] a = new byte[1];
        a[0] = (byte) v;
        return a[0];
    }
}
