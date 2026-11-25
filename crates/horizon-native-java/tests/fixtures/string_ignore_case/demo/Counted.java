package demo;

/**
 * Entry points the Rust test calls with operands Java cannot hold: a
 * `java/lang/String#n` jar-object marker (an unmodelled constructor's result)
 * or a host handle in place of a `String`. They have no `java25.txt` row (no
 * Java 25 program can make such a value); the test asserts each call is
 * counted, and that a real `String` in the same position is answered and
 * counts nothing.
 */
public final class Counted {

    public static boolean equalsIgnoreCase(String a, String b) {
        return a.equalsIgnoreCase(b);
    }

    public static int compareToIgnoreCase(String a, String b) {
        return a.compareToIgnoreCase(b);
    }

    public static boolean regionMatchesIgnoreCase(String a, String b) {
        return a.regionMatches(true, 0, b, 0, 1);
    }

    public static boolean regionMatchesExact(String a, String b) {
        return a.regionMatches(0, b, 0, 1);
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    public static int caseInsensitiveOrder(Object a, Object b) {
        return ((java.util.Comparator) String.CASE_INSENSITIVE_ORDER).compare(a, b);
    }
}
