package demo;

import java.nio.charset.Charset;
import java.nio.charset.StandardCharsets;

public final class Probe {
    interface Make { String make() throws Exception; }

    static String t(Make make) {
        try {
            String s = make.make();
            StringBuilder out = new StringBuilder();
            for (int i = 0; i < s.length(); i++) {
                char c = s.charAt(i);
                if (c < 0x20 || c > 0x7e) out.append("+").append((int) c); else out.append(c);
            }
            return out.toString();
        } catch (Exception e) {
            return e.getClass().getSimpleName() + ":" + e.getMessage();
        }
    }

    public static String run() {
        byte[] utf8 = {'c', 'a', 'f', (byte) 0xC3, (byte) 0xA9};
        byte[] latin = {'c', (byte) 0xE9};
        byte[] none = null;
        Charset noCharset = null;
        String noName = null;
        return t(() -> new String(utf8)) + "|" + t(() -> new String(utf8, 1, 4)) + "|"
            + t(() -> new String(latin, StandardCharsets.ISO_8859_1)) + "|"
            + t(() -> new String(latin, StandardCharsets.US_ASCII)) + "|"
            + t(() -> new String(utf8, 0, 3, "US-ASCII")) + "|" + t(() -> new String(latin, "latin1")) + "|"
            + t(() -> new String(utf8, 3, 5)) + "|" + t(() -> new String(none)) + "|"
            + t(() -> new String(none, 0, 1, StandardCharsets.UTF_8)) + "|"
            + t(() -> new String(utf8, noCharset)) + "|" + t(() -> new String(none, noName)) + "|"
            + t(() -> new String(utf8, 1, 9, noCharset));
    }

    public static String malformed() {
        byte[] surrogate = {(byte) 0xED, (byte) 0xA0, (byte) 0x80};
        byte[] overlong = {(byte) 0xE0, (byte) 0x80, (byte) 0x80};
        byte[] badTail = {(byte) 0xF0, (byte) 0x90, (byte) 0x80, 0x41};
        byte[] split = {(byte) 0xE2, (byte) 0x82, (byte) 0xE2, 0x28, (byte) 0xA1};
        byte[] badSecond = {(byte) 0xED, (byte) 0xC5, (byte) 0xE5, (byte) 0xEA};
        return t(() -> new String(surrogate, StandardCharsets.UTF_8)) + "|"
            + t(() -> new String(overlong, StandardCharsets.UTF_8)) + "|"
            + t(() -> new String(badTail, StandardCharsets.UTF_8)) + "|"
            + t(() -> new String(split, StandardCharsets.UTF_8)) + "|"
            + t(() -> new String(badSecond, StandardCharsets.UTF_8));
    }

    public static String unknownName() {
        byte[] bytes = {104, 105};
        return t(() -> new String(bytes, "default"));
    }
    // 220,000 deterministic arrays, compared with a Java 25 run. The rolling
    // hash makes every array participate without storing the whole output.
    public static String differential() {
        long state = 0x1234_5678_9ABCDEFL;
        long hash = 0;
        for (int n = 0; n < 220_000; n++) {
            state ^= state << 13;
            state ^= state >>> 7;
            state ^= state << 17;
            int length = (int) (state & 15);
            byte[] bytes = new byte[length];
            for (int i = 0; i < length; i++) {
                state ^= state << 13;
                state ^= state >>> 7;
                state ^= state << 17;
                bytes[i] = (byte) state;
            }
            hash = hash * 1_000_003L + new String(bytes, StandardCharsets.UTF_8).hashCode();
        }
        return 220_000 + "|" + hash;
    }

    public static void main(String[] args) { System.out.println(run()); }
}
