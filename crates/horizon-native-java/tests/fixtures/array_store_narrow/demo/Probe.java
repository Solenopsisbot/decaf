package demo;

/**
 * castore, sastore and bastore with an int wider than the array's element. The inputs come from a static array.
 * demo/Raw has the stores; its narrowing instructions are patched out before it runs (README.md).
 */
public final class Probe {
    static final int[] VALUES = {
        0, 1, 2, 3, 127, 128, 255, 256, 257, 32767, 32768, 65535, 65536, 65537, 0x123456,
        -1, -2, -128, -129, -32768, -32769, Integer.MIN_VALUE, Integer.MAX_VALUE, 0x12345678, 0x7fff8000,
    };

    /** char[] store then caload: 16 bits, zero-extended. */
    public static String charStore() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            out.append(Raw.charStore(v)).append(' ');
        }
        return out.toString().trim();
    }

    /** short[] store then saload: 16 bits, sign-extended. */
    public static String shortStore() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            out.append(Raw.shortStore(v)).append(' ');
        }
        return out.toString().trim();
    }

    /** byte[] store then baload: 8 bits, sign-extended. */
    public static String byteStore() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            out.append(Raw.byteStore(v)).append(' ');
        }
        return out.toString().trim();
    }

    /** boolean[] store then baload: the low bit. */
    public static String boolStore() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            out.append(Raw.boolStore(v)).append(' ');
        }
        return out.toString().trim();
    }
}
