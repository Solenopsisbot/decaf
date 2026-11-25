package demo;

/**
 * i2c, i2b and i2s on values outside the target type's range. The inputs come from a static array (and
 * from fields and locals), so javac cannot fold the casts into constants: each cast and each compound
 * assignment on a char, byte or short is a real i2c / i2b / i2s in the class file.
 */
public final class Probe {
    static final int[] VALUES = {
        0, 65, 127, 128, 255, 256, 32767, 32768, 0x4e2d, 0xd7ff, 0xd800, 0xdfff, 0xffff, 0x10000, 0x10041,
        -1, -128, -129, -32768, -32769, Integer.MIN_VALUE, Integer.MAX_VALUE, 0x12345678, 0x7fff8000,
    };

    static String hex(int v) {
        return Integer.toHexString(v);
    }

    /** (int) (char) v and its hex string, per value. */
    public static String charCast() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            char c = (char) v;
            int back = c;
            out.append(back).append('/').append(hex(c)).append(' ');
        }
        return out.toString().trim();
    }

    public static String byteCast() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            byte b = (byte) v;
            int back = b;
            out.append(back).append('/').append(hex(b)).append(' ');
        }
        return out.toString().trim();
    }

    public static String shortCast() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            short s = (short) v;
            int back = s;
            out.append(back).append('/').append(hex(s)).append(' ');
        }
        return out.toString().trim();
    }

    /** The cast result used directly in an int expression, with no local in between. */
    public static String charCastInline() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            out.append(((char) v) + 1).append(' ');
        }
        return out.toString().trim();
    }

    /** (char) of a long, a float and a double: l2i / f2i / d2i, then i2c. */
    public static String charFromWide() {
        long[] longs = {0x1_0000_0041L, -1L, 0xffff_ffffL, Long.MIN_VALUE, Long.MAX_VALUE, 0x4e2dL};
        float[] floats = {65.9f, -1.5f, 65536.5f, 70000f, -70000f, Float.NaN, Float.MAX_VALUE};
        double[] doubles = {65.9, -1.5, 65536.5, 1e10, -1e10, Double.NaN, Double.NEGATIVE_INFINITY};
        StringBuilder out = new StringBuilder();
        for (long l : longs) {
            out.append((int) (char) l).append(' ');
        }
        out.append('|');
        for (float f : floats) {
            out.append(' ').append((int) (char) f);
        }
        out.append(" |");
        for (double d : doubles) {
            out.append(' ').append((int) (char) d);
        }
        return out.toString();
    }

    /** Narrowing through another narrow type, and back. */
    public static String chained() {
        StringBuilder out = new StringBuilder();
        byte neg = (byte) VALUES[15];
        out.append((int) (char) neg).append(' ');
        char max = (char) VALUES[12];
        out.append((int) (short) max).append(' ');
        out.append((int) (byte) (char) VALUES[13 + 1]).append(' ');
        int big = VALUES[14];
        out.append((int) (byte) (char) (short) big).append(' ');
        out.append((int) (char) (short) VALUES[16]).append(' ');
        out.append((int) (char) (byte) VALUES[3]);
        return out.toString();
    }

    /** Compound assignments and increments on a char local: each ends in an i2c. */
    public static String charArith() {
        int k = VALUES[1];
        char c = (char) k;
        StringBuilder out = new StringBuilder();
        c += 300;
        out.append((int) c).append(' ');
        c += 65536 * 3 + 7;
        out.append((int) c).append(' ');
        c -= 70000;
        out.append((int) c).append(' ');
        c *= 1000;
        out.append((int) c).append(' ');
        c <<= 12;
        out.append((int) c).append(' ');
        c >>= 3;
        out.append((int) c).append(' ');
        c >>>= 1;
        out.append((int) c).append(' ');
        c ^= -1;
        out.append((int) c).append(' ');
        c |= 0x10000;
        out.append((int) c).append(' ');
        c &= -1;
        out.append((int) c).append(' ');
        c = (char) VALUES[12];
        c++;
        out.append((int) c).append(' ');
        c--;
        out.append((int) c).append(' ');
        char d = (char) (c + 1);
        out.append((int) d).append(' ');
        char e = (char) (c * c);
        out.append((int) e).append(' ');
        char f = (char) -c;
        out.append((int) f).append(' ');
        char g = (char) ~c;
        out.append((int) g);
        return out.toString();
    }

    /** Compound assignments on a byte and a short local, for comparison. */
    public static String byteShortArith() {
        byte b = (byte) VALUES[1];
        short s = (short) VALUES[1];
        StringBuilder out = new StringBuilder();
        b += 200;
        out.append((int) b).append(' ');
        b *= 3;
        out.append((int) b).append(' ');
        b <<= 4;
        out.append((int) b).append(' ');
        b >>>= 1;
        out.append((int) b).append(' ');
        b--;
        out.append((int) b).append(' ');
        s += 40000;
        out.append((int) s).append(' ');
        s *= 1000;
        out.append((int) s).append(' ');
        s <<= 9;
        out.append((int) s).append(' ');
        s >>>= 1;
        out.append((int) s).append(' ');
        s++;
        out.append((int) s);
        return out.toString();
    }

    static char fc = 'x';
    char ic = 'y';

    /** Compound assignments on a char field, static and instance, and on a char behind an int expression. */
    public static String charFields() {
        Probe p = new Probe();
        StringBuilder out = new StringBuilder();
        fc += 400;
        out.append((int) fc).append(' ');
        p.ic += 65536 + 500;
        out.append((int) p.ic).append(' ');
        fc = (char) VALUES[21];
        out.append((int) fc).append(' ');
        p.ic = (char) VALUES[20];
        out.append((int) p.ic).append(' ');
        fc--;
        out.append((int) fc).append(' ');
        p.ic--;
        out.append((int) p.ic).append(' ');
        fc = (char) (VALUES[12] + 2);
        out.append((int) fc);
        return out.toString();
    }

    /** char[] stores: castore keeps the low 16 bits; compound assignment is caload, op, i2c, castore. */
    public static String charArrays() {
        char[] a = new char[6];
        StringBuilder out = new StringBuilder();
        a[0] = (char) VALUES[13];
        a[1] = (char) VALUES[8];
        a[2] = (char) VALUES[21];
        a[3] = (char) VALUES[15];
        out.append((int) a[0]).append(' ').append((int) a[1]).append(' ').append((int) a[2]).append(' ')
            .append((int) a[3]).append(" |");
        a[0] += 300;
        a[1] += 70000;
        a[2] -= 70000;
        a[3] *= 3;
        a[4]--;
        a[5] += VALUES[13] + 5;
        for (char c : a) {
            out.append(' ').append((int) c);
        }
        a[4]++;
        a[4]++;
        a[1] <<= 4;
        a[2] >>= 1;
        out.append(" |");
        for (char c : a) {
            out.append(' ').append((int) c);
        }
        int i = 0;
        a[i++] += 1;
        a[i++] -= 1;
        out.append(" | ").append(i).append(' ').append((int) a[0]).append(' ').append((int) a[1]);
        return out.toString();
    }

    /** byte[] and short[] stores and compound assignments, for comparison. */
    public static String narrowArrays() {
        byte[] b = new byte[3];
        short[] s = new short[3];
        StringBuilder out = new StringBuilder();
        b[0] = (byte) VALUES[4];
        b[1] = (byte) VALUES[21];
        b[2] = (byte) VALUES[13];
        s[0] = (short) VALUES[12];
        s[1] = (short) VALUES[21];
        s[2] = (short) VALUES[13];
        b[0] += 3;
        b[1] += 3;
        b[2] -= 1;
        s[0] += 3;
        s[1] += 3;
        s[2] -= 1;
        for (byte x : b) {
            out.append((int) x).append(' ');
        }
        out.append('|');
        for (short x : s) {
            out.append(' ').append((int) x);
        }
        return out.toString();
    }

    /**
     * A char survives string building and string indexing with its low 16 bits. A lone surrogate is left out:
     * the runtime's String is Rust text and counts an append of one, it does not answer it.
     */
    public static String charStrings() {
        StringBuilder sb = new StringBuilder();
        for (int v : VALUES) {
            char c = (char) v;
            if (c >= 0xd800 && c <= 0xdfff) {
                continue;
            }
            sb.append(c);
        }
        String s = sb.toString();
        StringBuilder out = new StringBuilder();
        out.append(s.length()).append(':');
        for (int i = 0; i < s.length(); i++) {
            out.append(hex(s.charAt(i))).append(' ');
        }
        char[] arr = new char[3];
        arr[0] = (char) VALUES[8];
        arr[1] = (char) VALUES[14];
        arr[2] = (char) VALUES[2];
        String t = new String(arr);
        out.append('|').append(t.length()).append(' ').append((int) t.charAt(0)).append(' ')
            .append((int) t.charAt(1)).append(' ').append((int) t.charAt(2));
        out.append(" | ").append(String.valueOf((char) VALUES[8]).length()).append(' ')
            .append((int) String.valueOf((char) VALUES[14]).charAt(0));
        return out.toString();
    }

    /** A char passed as an argument, returned from a method and compared. */
    static char widen(int v) {
        return (char) v;
    }

    static int unwrap(char c) {
        return c;
    }

    public static String charCalls() {
        StringBuilder out = new StringBuilder();
        for (int v : VALUES) {
            out.append(unwrap(widen(v))).append(' ');
        }
        out.append('|');
        char lo = (char) VALUES[5];
        char hi = (char) VALUES[13];
        out.append(lo == hi).append(' ').append(lo == 0).append(' ').append(hi == 0).append(' ')
            .append(widen(VALUES[12]) > widen(VALUES[13]));
        return out.toString();
    }
}
