package demo;

import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;

/**
 * String constants (constant pool) and an unpaired surrogate. A Rust String cannot hold one, so the
 * runtime counts every use of such a constant. Rows named ok.* hold none (a pair, a genuine U+FFFD)
 * and must be answered exactly; rows named lone.* use a constant with an unpaired half and must be
 * counted. Each row is "label=value".
 */
public class Probe {
    static int id(int v) { return v; }

    static final String LONE_FINAL = "\ud83d";

    /** Its own class, so the other rows do not run its initializer. */
    static class Holder {
        static String lonePlain = "\ude00";
    }
    static final String PAIR_FINAL = "😀";

    static String sw(String key) {
        switch (key) {
            case "\ud83d": return "high";
            case "\ude00": return "low";
            default: return "other";
        }
    }

    static String swPair(String key) {
        switch (key) {
            case "😀": return "pair";
            case "�": return "replacement";
            default: return "other";
        }
    }

    static String typeSw(String o) {
        return switch (o) {
            case "\ud83d" -> "high";
            case "\ude00" -> "low";
            case String s when s.length() > 3 -> "long";
            case String s -> "string";
        };
    }

    public static final int ROWS = 36;

    public static String row(int i) {
        try {
            switch (i) {
                // Exact: no unpaired surrogate in any constant.
                case 0: return "ok.pairHash=" + "😀".hashCode();
                case 1: return "ok.pairLength=" + "😀".length();
                case 2: return "ok.pairCodePoint=" + "😀".codePointAt(0);
                case 3: return "ok.replacementHash=" + "�".hashCode();
                case 4: return "ok.replacementChar=" + (int) "�".charAt(0);
                case 5: return "ok.mixedHash=" + "A\u0000ÿ世😀z".hashCode();
                case 6: return "ok.foldedPairHash=" + ("\ud83d" + "\ude00").hashCode();
                case 7: return "ok.concatPairHash=" + ("x😀" + id(1)).hashCode();
                case 8: return "ok.replacementEquals=" + "�".equals("�");
                case 9: return "ok.switchPair=" + swPair("😀") + "," + swPair("�") + "," + swPair("a");
                case 10: return "ok.finalPairHash=" + PAIR_FINAL.hashCode();
                // Counted: the constant holds an unpaired surrogate.
                case 11: return "lone.highHash=" + "\ud83d".hashCode();
                case 12: return "lone.lowHash=" + "\ude00".hashCode();
                case 13: return "lone.highChar=" + (int) "\ud83d".charAt(0);
                case 14: return "lone.highLength=" + "\ud83d".length();
                case 15: return "lone.aHighHash=" + "a\ud83d".hashCode();
                case 16: return "lone.highBHash=" + "\ud83db".hashCode();
                case 17: return "lone.highHighHash=" + "\ud83d\ud83d".hashCode();
                case 18: return "lone.lowHighHash=" + "\udc00\ud83d".hashCode();
                case 19: return "lone.highAHash=" + "\ud83dA".hashCode();
                case 20: return "lone.arrayHash=" + Arrays.hashCode(new Object[] { "\ud83d" });
                case 21: return "lone.equalsOther=" + "\ud83d".equals("\ude00");
                case 22: return "lone.equalsSelf=" + "\ud83d".equals("\ud83d");
                case 23: return "lone.concatRecipeHash=" + ("a\ud83d" + id(1)).hashCode();
                case 24: return "lone.concatTailHash=" + (id(1) + "\ud83d").hashCode();
                case 25: return "lone.switchHigh=" + sw("\ud83d");
                case 26: return "lone.switchLow=" + sw("\ude00");
                case 27: return "lone.finalHash=" + LONE_FINAL.hashCode();
                case 28: return "lone.fieldLength=" + Holder.lonePlain.length();
                case 29: {
                    Map<String, Integer> m = new HashMap<>();
                    m.put("\ud83d", 1);
                    return "lone.mapGet=" + m.get("\ud83d") + "," + m.get("\ude00");
                }
                case 30: {
                    String a = "\ud83d";
                    String b = "\ud83d";
                    return "lone.identity=" + (a == b);
                }
                case 31: return "lone.typeSwitch=" + typeSw("\ud83d") + "," + typeSw("\ude00") + "," + typeSw("x");
                case 32: return "lone.compareTo=" + "\ud83d".compareTo("\ude00");
                case 33: return "lone.reflectFinal=" + Probe.class.getDeclaredField("LONE_FINAL").get(null).toString().length();
                case 34: return "lone.toCharArray=" + "\ud83d".toCharArray().length;
                case 35: return "lone.stringValueOf=" + String.valueOf("\ud83d").hashCode();
            }
        } catch (Throwable t) {
            return "row" + i + "=EXC " + t.getClass().getName() + ": " + t.getMessage();
        }
        return "NOROW";
    }

    public static void main(String[] args) {
        for (int i = 0; i < ROWS; i++) {
            System.out.println(row(i));
        }
    }
}
