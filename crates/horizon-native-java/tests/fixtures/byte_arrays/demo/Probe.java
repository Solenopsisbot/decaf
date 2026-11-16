package demo;

import java.util.Arrays;
import java.lang.reflect.Array;

/**
 * {@code byte[]} end to end (#274): the runtime keeps one byte per element. Loads sign-extend,
 * stores keep the low 8 bits, and the paths the runtime models on a byte array (`baload`,
 * `bastore`, `arraylength`, `System.arraycopy`, `Arrays.copyOf`/`copyOfRange`, `clone`) answer as
 * Java 25. Arrays are printed, compared, hashed and filled by loops here: `Arrays.toString`/
 * `equals`/`hashCode`/`fill` on a {@code byte[]} are counted on main and on this branch alike.
 */
public final class Probe {
    static String t(java.util.function.Supplier<Object> s) {
        try {
            return String.valueOf(s.get());
        } catch (RuntimeException e) {
            return e.getClass().getSimpleName() + ":" + e.getMessage();
        }
    }

    static String str(byte[] b) {
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < b.length; i++) {
            if (i > 0) {
                sb.append(", ");
            }
            sb.append(b[i]);
        }
        return sb.append(']').toString();
    }

    static int hash(byte[] b) {
        int h = 1;
        for (int i = 0; i < b.length; i++) {
            h = 31 * h + b[i];
        }
        return h;
    }

    static boolean same(byte[] a, byte[] b) {
        if (a.length != b.length) {
            return false;
        }
        for (int i = 0; i < a.length; i++) {
            if (a[i] != b[i]) {
                return false;
            }
        }
        return true;
    }

    /** `baload` sign-extends; `arraylength`; a bounds error. */
    public static String basics() {
        byte[] b = new byte[5];
        b[0] = (byte) 200;
        b[1] = (byte) -1;
        b[2] = 127;
        b[3] += 300;
        int x = 0x1234;
        b[4] = (byte) x;
        return str(b) + "|" + b.length + "|" + (b[0] & 0xff) + "|" + t(() -> b[5]);
    }

    /** `System.arraycopy` within one array, both directions (as if through a temporary). */
    public static String overlap() {
        byte[] a = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
        System.arraycopy(a, 0, a, 2, 5);
        String forward = str(a);
        byte[] c = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
        System.arraycopy(c, 3, c, 1, 4);
        return forward + "|" + str(c);
    }

    /** `System.arraycopy` refusals: kinds, `null`, bounds. */
    public static String copyErrors() {
        byte[] b = {1, 2, 3};
        int[] i = new int[3];
        Object[] o = {1};
        return t(() -> { System.arraycopy(b, 0, i, 0, 1); return "ok"; })
            + "|" + t(() -> { System.arraycopy(o, 0, b, 0, 1); return "ok"; })
            + "|" + t(() -> { System.arraycopy(null, 0, b, 0, 1); return "ok"; })
            + "|" + t(() -> { System.arraycopy(b, 2, b, 0, 2); return "ok"; })
            + "|" + t(() -> { System.arraycopy(b, 0, b, -1, 1); return "ok"; })
            + "|" + t(() -> { System.arraycopy(b, 0, new byte[2], 0, 2); return "ok"; });
    }

    /** `Arrays.copyOf`/`copyOfRange` and `clone`: new arrays of their own. */
    public static String copies() {
        byte[] b = {(byte) 200, 2, 3};
        byte[] padded = Arrays.copyOf(b, 5);
        byte[] range = Arrays.copyOfRange(b, 1, 3);
        byte[] c = b.clone();
        c[0] = 7;
        padded[1] = 9;
        return str(padded) + "|" + str(range) + "|" + str(b) + "|" + str(c) + "|" + same(b, b.clone())
            + same(b, c) + "|" + hash(b);
    }

    /** EntityCulling's `ArrayOcclusionCache`: a 4 MiB `byte[]`, written and read by index, copied. */
    public static String big() {
        byte[] cache = new byte[4 << 20];
        for (int i = 0; i < cache.length; i = i + 4096) {
            cache[i] = (byte) (i >>> 12);
        }
        long sum = 0;
        for (int i = 0; i < cache.length; i = i + 4096) {
            sum += cache[i];
        }
        byte[] half = new byte[2 << 20];
        System.arraycopy(cache, 0, half, 0, half.length);
        byte[] copy = Arrays.copyOf(cache, 8);
        return cache.length + "|" + sum + "|" + half[4096 * 200] + "|" + str(copy) + "|"
            + cache.clone().length;
    }

    /** A `byte[][]`: `multianewarray`'s leaves are byte arrays too. */
    public static String grid() {
        byte[][] g = new byte[3][4];
        g[1][2] = (byte) 255;
        g[2] = new byte[] {5};
        return g.length + "|" + g[0].length + "|" + g[1][2] + "|" + str(g[2]) + "|" + str(g[1]);
    }

    /** Reflection only permits widening reads and exact primitive writes. */
    public static String reflection() {
        byte[] b = {1};
        boolean[] z = {false};
        String[] strings = new String[1];
        Object[] objects = new Object[1];
        return t(() -> { Array.setByte(b, 0, (byte) 200); return b[0]; })
            + "|" + t(() -> { Array.setInt(b, 0, 300); return "ok"; })
            + "|" + t(() -> { Array.setShort(b, 0, (short) 2); return "ok"; })
            + "|" + t(() -> { Array.setLong(b, 0, 2L); return "ok"; })
            + "|" + t(() -> { Array.setFloat(b, 0, 2f); return "ok"; })
            + "|" + t(() -> { Array.setDouble(b, 0, 2d); return "ok"; })
            + "|" + t(() -> { Array.setBoolean(b, 0, true); return "ok"; })
            + "|" + t(() -> { Array.set(b, 0, Integer.valueOf(3)); return "ok"; })
            + "|" + t(() -> { Array.set(b, 0, Byte.valueOf((byte) 4)); return b[0]; })
            + "|" + Array.getByte(b, 0) + ":" + Array.getShort(b, 0) + ":" + Array.getInt(b, 0)
            + ":" + Array.getLong(b, 0) + ":" + (Array.getFloat(b, 0) == 4f)
            + ":" + (Array.getDouble(b, 0) == 4d)
            + "|" + t(() -> { Array.setBoolean(z, 0, true); return z[0]; })
            + "|" + t(() -> { Array.setByte(z, 0, (byte) 1); return "ok"; })
            + "|" + t(() -> { Array.setInt(z, 0, 1); return "ok"; })
            + "|" + t(() -> { Array.set(z, 0, Integer.valueOf(1)); return "ok"; })
            + "|" + Array.getBoolean(z, 0)
            + "|" + t(() -> Array.getByte(z, 0)) + "|" + t(() -> Array.getInt(z, 0))
            + "|" + t(() -> { Array.set(strings, 0, "ok"); return strings[0]; })
            + "|" + t(() -> { Array.set(strings, 0, Integer.valueOf(1)); return "bad"; })
            + "|" + t(() -> { Array.set(strings, 0, null); return "null"; })
            + "|" + t(() -> { Array.set(objects, 0, Integer.valueOf(2)); return objects[0]; });
    }

    /** The shared `bastore`/`baload` opcodes on a boolean[] mask to one bit. */
    public static String rawBoolean() {
        return String.valueOf(RawBoolean.raw(300));
    }

    private static Object reflect(String operation, Object array) {
        switch (operation) {
            case "setByte": Array.setByte(array, 0, (byte) 1); return "ok";
            case "setShort": Array.setShort(array, 0, (short) 1); return "ok";
            case "setInt": Array.setInt(array, 0, 1); return "ok";
            case "setLong": Array.setLong(array, 0, 1L); return "ok";
            case "setFloat": Array.setFloat(array, 0, 1F); return "ok";
            case "setDouble": Array.setDouble(array, 0, 1D); return "ok";
            case "setChar": Array.setChar(array, 0, 'a'); return "ok";
            case "getByte": return Array.getByte(array, 0);
            case "getShort": return Array.getShort(array, 0);
            case "getInt": return Array.getInt(array, 0);
            case "getLong": return Array.getLong(array, 0);
            case "getFloat": return Array.getFloat(array, 0);
            case "getDouble": return Array.getDouble(array, 0);
            case "getChar": return Array.getChar(array, 0);
            default: return "unknown";
        }
    }

    /** Java 25's catchable mismatch table for every non-boolean typed accessor. */
    public static String reflectionMatrix() {
        Object[] arrays = {new int[1], new long[1], new Object[1], new String[1], new byte[1], new boolean[1]};
        String[] names = {"setByte", "setShort", "setInt", "setLong", "setFloat", "setDouble", "setChar",
            "getByte", "getShort", "getInt", "getLong", "getFloat", "getDouble", "getChar"};
        StringBuilder out = new StringBuilder();
        for (String name : names) {
            for (Object array : arrays) {
                out.append(t(() -> reflect(name, array))).append('|');
            }
        }
        return out.toString();
    }

    public static void main(String[] args) throws Exception {
        for (String m : new String[] {"basics", "overlap", "copyErrors", "copies", "big", "grid", "reflection", "rawBoolean", "reflectionMatrix"}) {
            System.out.println(m + "=" + Probe.class.getMethod(m).invoke(null));
        }
    }
}
