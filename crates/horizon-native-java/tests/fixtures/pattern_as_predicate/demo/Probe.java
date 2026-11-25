package demo;

import java.io.File;
import java.math.BigInteger;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedList;
import java.util.List;
import java.util.Random;
import java.util.StringJoiner;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Predicate;
import java.util.regex.Pattern;
import java.util.stream.Collectors;
import java.util.stream.Stream;

public final class Probe {
    private static void row(StringBuilder b, String name, boolean value) {
        b.append(name).append(" = ").append(value).append('\n');
    }

    /** A `Predicate<String>` passed in as an argument: the call site is not the one that made it. */
    private static boolean apply(Predicate<String> predicate, String input) {
        return predicate.test(input);
    }

    /** A jar class implementing Predicate itself. */
    private static final class StartsWithA implements Predicate<String> {
        @Override
        public boolean test(String s) {
            return s.startsWith("a");
        }
    }

    public static String rows() {
        StringBuilder b = new StringBuilder();

        // Anchored pattern (the shape Jankson uses for unquoted keys).
        Predicate<String> key = Pattern.compile("^[a-zA-Z0-9]+$").asPredicate();
        row(b, "key validKey", key.test("validKey"));
        row(b, "key abc123", key.test("abc123"));
        row(b, "key invalid-key", key.test("invalid-key"));
        row(b, "key abc_def", key.test("abc_def"));
        row(b, "key empty", key.test(""));
        row(b, "key underscore", key.test("_"));
        row(b, "key dollar", key.test("$"));
        // `$` also matches before a final line terminator.
        row(b, "key abc LF", key.test("abc\n"));
        row(b, "key abc CRLF", key.test("abc\r\n"));
        row(b, "key abc LF def", key.test("abc\ndef"));
        row(b, "key LF abc", key.test("\nabc"));

        // Unanchored: find semantics, not matches.
        Predicate<String> digits = Pattern.compile("[0-9]+").asPredicate();
        row(b, "digits abc123def", digits.test("abc123def"));
        row(b, "digits abc", digits.test("abc"));
        row(b, "digits empty", digits.test(""));
        row(b, "digits 123", digits.test("123"));
        Predicate<String> a = Pattern.compile("a").asPredicate();
        row(b, "a bab", a.test("bab"));
        row(b, "a xyz", a.test("xyz"));

        // The empty pattern occurs everywhere, including in the empty string.
        Predicate<String> empty = Pattern.compile("").asPredicate();
        row(b, "empty-pattern empty", empty.test(""));
        row(b, "empty-pattern x", empty.test("x"));
        Predicate<String> bothAnchors = Pattern.compile("^$").asPredicate();
        row(b, "anchors empty", bothAnchors.test(""));
        row(b, "anchors LF", bothAnchors.test("\n"));
        row(b, "anchors x", bothAnchors.test("x"));

        // Word boundary, lookbehind, backtracking alternation.
        Predicate<String> word = Pattern.compile("\\bfoo\\b").asPredicate();
        row(b, "word a foo b", word.test("a foo b"));
        row(b, "word afoo", word.test("afoo"));
        row(b, "word foo", word.test("foo"));
        Predicate<String> behind = Pattern.compile("(?<=x)y").asPredicate();
        row(b, "behind xy", behind.test("xy"));
        row(b, "behind y", behind.test("y"));
        Predicate<String> alt = Pattern.compile("(a|ab)(c|bcd)").asPredicate();
        row(b, "alt abcd", alt.test("abcd"));
        row(b, "alt abd", alt.test("abd"));

        // Non-ASCII text in the Basic Multilingual Plane.
        Predicate<String> cafe = Pattern.compile("caf\u00e9").asPredicate();
        row(b, "cafe cafe-acute", cafe.test("un caf\u00e9 noir"));
        row(b, "cafe CAFE-acute", cafe.test("CAF\u00c9"));

        // A predicate keeps no state between calls, and each call to asPredicate is a new object.
        row(b, "key again validKey", key.test("validKey"));
        row(b, "key again invalid-key", key.test("invalid-key"));
        Pattern shared = Pattern.compile("[0-9]");
        Predicate<String> first = shared.asPredicate();
        Predicate<String> second = shared.asPredicate();
        row(b, "first-test-1", first.test("1"));
        row(b, "second-test-x", second.test("x"));
        row(b, "first-test-1-again", first.test("1"));
        row(b, "same-object", first == second);
        row(b, "equals-self", first.equals(first));
        row(b, "equals-other", first.equals(second));

        // The call site that holds it as an `Object`, and as the interface it is.
        Object asObject = first;
        row(b, "instanceof-Predicate", asObject instanceof Predicate);
        row(b, "instanceof-Runnable", asObject instanceof Runnable);
        row(b, "instanceof-Object", asObject instanceof Object);
        row(b, "instanceof-Serializable", asObject instanceof java.io.Serializable);
        row(b, "instanceof-Comparable", asObject instanceof Comparable);
        row(b, "instanceof-Function", asObject instanceof java.util.function.Function);
        row(b, "instanceof-CharSequence", asObject instanceof CharSequence);
        row(b, "instanceof-Pattern", asObject instanceof Pattern);
        row(b, "instanceof-String", asObject instanceof String);

        // Reached through a helper that takes any Predicate<String>: a predicate
        // the pattern made, a lambda, a method reference and a jar class all take
        // the same call.
        row(b, "apply pattern yes", apply(digits, "x9"));
        row(b, "apply pattern no", apply(digits, "x"));
        row(b, "apply lambda yes", apply(s -> s.length() == 3, "abc"));
        row(b, "apply lambda no", apply(s -> s.length() == 3, "ab"));
        row(b, "apply methodref yes", apply(String::isEmpty, ""));
        row(b, "apply methodref no", apply(String::isEmpty, "x"));
        row(b, "apply jar-class yes", apply(new StartsWithA(), "abc"));
        row(b, "apply jar-class no", apply(new StartsWithA(), "bca"));
        Predicate<String> anonymous = new Predicate<String>() {
            @Override
            public boolean test(String s) {
                return s.equals("anon");
            }
        };
        row(b, "apply anonymous yes", apply(anonymous, "anon"));
        row(b, "apply anonymous no", apply(anonymous, "other"));

        // The predicate handed to the JDK's own consumers of one, and combined.
        b.append("stream filter = ")
            .append(Stream.of("a1", "b", "c2").filter(digits).collect(Collectors.toList()))
            .append('\n');
        List<String> removable = new ArrayList<>(List.of("a1", "b", "c2"));
        row(b, "removeIf changed", removable.removeIf(digits));
        b.append("removeIf left = ").append(removable).append('\n');
        List<String> untouched = new ArrayList<>(List.of("b", "c"));
        row(b, "removeIf none changed", untouched.removeIf(digits));
        b.append("removeIf none left = ").append(untouched).append('\n');
        Predicate<String> letters = Pattern.compile("[a-z]").asPredicate();
        row(b, "negate 1", digits.negate().test("1"));
        row(b, "negate x", digits.negate().test("x"));
        row(b, "and 1a", digits.and(letters).test("1a"));
        row(b, "and 1", digits.and(letters).test("1"));
        row(b, "or x", digits.or(letters).test("x"));
        row(b, "or ?", digits.or(letters).test("?"));
        // The other operations that take a Predicate: terminals of a stream, an Optional.
        b.append("stream count = ").append(Stream.of("a1", "b", "c2").filter(digits).count()).append('\n');
        row(b, "anyMatch", Stream.of("a", "b1").anyMatch(digits));
        row(b, "anyMatch none", Stream.of("a", "b").anyMatch(digits));
        row(b, "allMatch", Stream.of("a1", "b2").allMatch(digits));
        row(b, "allMatch not", Stream.of("a1", "b").allMatch(digits));
        row(b, "noneMatch", Stream.of("a", "b").noneMatch(digits));
        row(b, "noneMatch not", Stream.of("a", "b1").noneMatch(digits));
        row(b, "optional filter kept", java.util.Optional.of("a1").filter(digits).isPresent());
        row(b, "optional filter dropped", java.util.Optional.of("a").filter(digits).isPresent());
        return b.toString();
    }

    /** Pattern flags other than 0: the runtime does not model them, so these are counted. */
    public static String flagged() {
        Predicate<String> p = Pattern.compile("abc", Pattern.CASE_INSENSITIVE).asPredicate();
        return "flagged ABC = " + p.test("ABC");
    }

    /**
     * An input with a supplementary character (`.` is one code point there): the
     * engine reads the input as UTF-16 units it cannot build, so the runtime counts it.
     */
    public static String supplementary() {
        String face = new String(Character.toChars(0x1F600));
        StringBuilder b = new StringBuilder();
        b.append("one face = ").append(Pattern.compile("^.$").asPredicate().test(face)).append('\n');
        b.append("two face = ").append(Pattern.compile("^..$").asPredicate().test(face)).append('\n');
        List<String> faces = new ArrayList<>(List.of(face, "b"));
        faces.removeIf(Pattern.compile("^.$").asPredicate());
        b.append("removeIf left = ").append(faces.size()).append('\n');
        return b.toString();
    }

    /** `test` on an argument that is not a String: the JDK throws, the runtime counts it. */
    @SuppressWarnings({"unchecked", "rawtypes"})
    public static String nonString() {
        Predicate raw = Pattern.compile("a").asPredicate();
        StringBuilder b = new StringBuilder();
        String builder;
        try {
            builder = String.valueOf(raw.test(new StringBuilder("a")));
        } catch (ClassCastException e) {
            builder = "ClassCastException";
        }
        b.append("builder = ").append(builder).append('\n');
        String none;
        try {
            none = String.valueOf(raw.test(null));
        } catch (NullPointerException e) {
            none = "NullPointerException";
        }
        b.append("null = ").append(none).append('\n');
        String integer;
        try {
            integer = String.valueOf(raw.test(Integer.valueOf(1)));
        } catch (ClassCastException e) {
            integer = "ClassCastException";
        }
        b.append("integer = ").append(integer).append('\n');
        return b.toString();
    }

    /** A jar object that is not a String and does not implement Predicate. */
    private static final class Thing {}

    /**
     * `test` on an object that is not a String, through the raw `Predicate`: the JDK throws a
     * ClassCastException whatever the pattern says. The runtime counts these calls; a row prints
     * what the JDK does. Each row runs on a fresh host in the test.
     */
    @SuppressWarnings({"unchecked", "rawtypes"})
    public static String objectArgument(String which) {
        Predicate matchA = Pattern.compile("a").asPredicate();
        Predicate matchNone = Pattern.compile("^zzz").asPredicate();
        Predicate matchOne = Pattern.compile("1").asPredicate();
        Predicate matchKeyText = Pattern.compile("^java/lang/Object#").asPredicate();
        String out;
        try {
            switch (which) {
                case "object" -> out = String.valueOf(matchA.test(new Object()));
                case "object-nomatch" -> out = String.valueOf(matchNone.test(new Object()));
                case "object-keytext" -> out = String.valueOf(matchKeyText.test(new Object()));
                case "jar-object" -> out = String.valueOf(matchA.test(new Thing()));
                case "arraylist" -> out = String.valueOf(matchA.test(new ArrayList<String>()));
                case "hashmap" -> out = String.valueOf(matchA.test(new HashMap<String, String>()));
                case "hashset" -> out = String.valueOf(matchA.test(new HashSet<String>()));
                case "list-of" -> out = String.valueOf(matchA.test(List.of()));
                case "linkedlist" -> out = String.valueOf(matchA.test(new LinkedList<String>()));
                case "file" -> out = String.valueOf(matchA.test(new File("a")));
                case "atomic-integer" -> out = String.valueOf(matchOne.test(new AtomicInteger(1)));
                case "biginteger" -> out = String.valueOf(matchOne.test(new BigInteger("1")));
                case "runtime-exception" -> out = String.valueOf(matchA.test(new RuntimeException("a")));
                case "random" -> out = String.valueOf(matchA.test(new Random(1)));
                case "string-joiner" -> out = String.valueOf(matchA.test(new StringJoiner(",")));
                case "removeIf-object" -> {
                    List<Object> list = new ArrayList<>(Arrays.asList(new Object(), "b"));
                    boolean changed = list.removeIf(matchA);
                    out = changed + " " + list.size();
                }
                case "filter-object" ->
                    out = String.valueOf(Stream.<Object>of(new Object()).filter(matchA).count());
                case "filter-object-nomatch" ->
                    out = String.valueOf(Stream.<Object>of(new Object()).filter(matchNone).count());
                case "anyMatch-object" ->
                    out = String.valueOf(Stream.<Object>of(new Object()).anyMatch(matchA));
                // A String whose text spells an object's key is still a String on the JDK; the
                // runtime cannot tell it from the object and counts it.
                case "string-key-text" -> out = String.valueOf(matchKeyText.test("java/lang/Object#1"));
                default -> out = "unknown row " + which;
            }
        } catch (ClassCastException e) {
            out = "ClassCastException";
        }
        return which + " = " + out + "\n";
    }

    static final String[] OBJECT_ARGUMENT_ROWS = {
        "object", "object-nomatch", "object-keytext", "jar-object", "arraylist", "hashmap",
        "hashset", "list-of", "linkedlist", "file", "atomic-integer", "biginteger",
        "runtime-exception", "random", "string-joiner", "removeIf-object", "filter-object",
        "filter-object-nomatch", "anyMatch-object", "string-key-text",
    };

    /**
     * `test` called by a JDK operation (`removeIf`, `Stream.filter` ended by `collect`/`count`,
     * `anyMatch`, `allMatch`, `noneMatch`, `Optional.filter`, `Predicate.negate`) on an element
     * the runtime cannot answer: a non-String (the JDK throws ClassCastException) or a String
     * with a supplementary character (the engine refuses it). The runtime counts the call and
     * the enclosing operation stops there, never a definite `removeIf`/count/match result. A row
     * prints what the JDK does. Each row runs on a fresh host in the test.
     */
    @SuppressWarnings({"unchecked", "rawtypes"})
    public static String enclosing(String which) {
        Predicate matchA = Pattern.compile("a").asPredicate();
        Predicate matchNone = Pattern.compile("^zzz").asPredicate();
        // U+1F600 followed by `a`: a supplementary character in the input.
        String face = "\uD83D\uDE00a";
        String out;
        try {
            switch (which) {
                case "removeIf-array-list-object" -> {
                    List<Object> list = new ArrayList<>(Arrays.asList("a", new Object(), "ba"));
                    out = list.removeIf(matchA) + " " + list.size();
                }
                case "removeIf-linked-list-object" -> {
                    List<Object> list = new LinkedList<>(Arrays.asList("a", new Object(), "ba"));
                    out = list.removeIf(matchA) + " " + list.size();
                }
                case "removeIf-hash-set-object" -> {
                    java.util.Set<Object> set = new HashSet<>(Arrays.asList(new Object()));
                    out = set.removeIf(matchNone) + " " + set.size();
                }
                case "removeIf-supplementary" -> {
                    List<String> list = new ArrayList<>(Arrays.asList("b", face, "a"));
                    out = list.removeIf(matchA) + " " + list.size();
                }
                case "filter-collect-object" ->
                    out = String.valueOf(Stream.<Object>of("a", new Object()).filter(matchA).collect(Collectors.toList()));
                case "filter-count-object" ->
                    out = String.valueOf(Stream.<Object>of(new Object()).filter(matchNone).count());
                case "filter-collect-supplementary" ->
                    out = String.valueOf(Stream.of("a", face).filter(matchA).collect(Collectors.toList()));
                case "filter-count-supplementary" ->
                    out = String.valueOf(Stream.of("b", face).filter(matchA).count());
                case "anyMatch-supplementary" -> out = String.valueOf(Stream.of("b", face).anyMatch(matchA));
                case "allMatch-object" ->
                    out = String.valueOf(Stream.<Object>of(new Object()).allMatch(matchA));
                case "allMatch-supplementary" -> out = String.valueOf(Stream.of("a", face).allMatch(matchA));
                case "noneMatch-object" ->
                    out = String.valueOf(Stream.<Object>of(new Object()).noneMatch(matchNone));
                case "noneMatch-supplementary" -> out = String.valueOf(Stream.of("b", face).noneMatch(matchA));
                case "negate-removeIf-object" -> {
                    List<Object> list = new ArrayList<>(Arrays.asList(new Object()));
                    out = list.removeIf(matchA.negate()) + " " + list.size();
                }
                case "optional-filter-object" ->
                    out = String.valueOf(java.util.Optional.<Object>of(new Object()).filter(matchA).isPresent());
                default -> out = "unknown row " + which;
            }
        } catch (ClassCastException e) {
            out = "ClassCastException";
        }
        return which + " = " + out + "\n";
    }

    static final String[] ENCLOSING_ROWS = {
        "removeIf-array-list-object", "removeIf-linked-list-object", "removeIf-hash-set-object",
        "removeIf-supplementary", "filter-collect-object", "filter-count-object",
        "filter-collect-supplementary", "filter-count-supplementary",
        "anyMatch-supplementary", "allMatch-object", "allMatch-supplementary",
        "noneMatch-object", "noneMatch-supplementary", "negate-removeIf-object",
        "optional-filter-object",
    };

    /**
     * Reflection on the predicate: its class is a hidden lambda class, not the interface. Only
     * stable facts are printed (the class name has an address in it). Counted by the runtime.
     */
    @SuppressWarnings({"unchecked", "rawtypes"})
    public static String reflection(String which) {
        Predicate p = Pattern.compile("a").asPredicate();
        Class<?> c = p.getClass();
        String out = switch (which) {
            case "name-is-Predicate" -> String.valueOf(c.getName().equals("java.util.function.Predicate"));
            case "isInterface" -> String.valueOf(c.isInterface());
            case "class-eq-Predicate" -> String.valueOf(c == Predicate.class);
            case "simpleName-is-Predicate" -> String.valueOf(c.getSimpleName().equals("Predicate"));
            case "packageName" -> c.getPackageName();
            case "modifiers" -> String.valueOf(c.getModifiers());
            case "name-starts-with-Pattern-Lambda" ->
                String.valueOf(c.getName().startsWith("java.util.regex.Pattern$$Lambda"));
            case "toString-starts-with-class-Pattern-Lambda" ->
                String.valueOf(c.toString().startsWith("class java.util.regex.Pattern$$Lambda"));
            case "isHidden" -> String.valueOf(c.isHidden());
            case "superclass" -> c.getSuperclass().getName();
            case "interfaces" -> Arrays.toString(c.getInterfaces());
            default -> "unknown row " + which;
        };
        return which + " = " + out + "\n";
    }

    static final String[] REFLECTION_ROWS = {
        "name-is-Predicate", "isInterface", "class-eq-Predicate", "simpleName-is-Predicate",
        "packageName", "modifiers", "name-starts-with-Pattern-Lambda",
        "toString-starts-with-class-Pattern-Lambda", "isHidden", "superclass", "interfaces",
    };

    /**
     * A pattern the JDK rejects (or null): the JDK throws at `compile`, so `asPredicate` is never
     * reached. The runtime counts these. Each row runs on a fresh host in the test.
     */
    public static String badPattern(String which) {
        String out;
        try {
            Pattern p = switch (which) {
                case "unclosed-group" -> Pattern.compile("(");
                case "unclosed-class" -> Pattern.compile("[");
                case "dangling-star" -> Pattern.compile("*");
                case "bad-repeat" -> Pattern.compile("a{2,1}");
                case "trailing-backslash" -> Pattern.compile("\\");
                case "null" -> Pattern.compile(null);
                default -> throw new IllegalArgumentException(which);
            };
            out = String.valueOf(p.asPredicate().test("a"));
        } catch (java.util.regex.PatternSyntaxException e) {
            out = "PatternSyntaxException";
        } catch (NullPointerException e) {
            out = "NullPointerException";
        }
        return which + " = " + out + "\n";
    }

    static final String[] BAD_PATTERN_ROWS = {
        "unclosed-group", "unclosed-class", "dangling-star", "bad-repeat", "trailing-backslash",
        "null",
    };

    /** `java demo.Probe` prints the oracle rows; `java demo.Probe counted` the cases the runtime counts. */
    public static void main(String[] args) {
        if (args.length > 0 && args[0].equals("counted")) {
            System.out.println(flagged());
            System.out.print(supplementary());
            System.out.print(nonString());
            for (String row : OBJECT_ARGUMENT_ROWS) {
                System.out.print(objectArgument(row));
            }
            for (String row : ENCLOSING_ROWS) {
                System.out.print(enclosing(row));
            }
            for (String row : REFLECTION_ROWS) {
                System.out.print(reflection(row));
            }
            for (String row : BAD_PATTERN_ROWS) {
                System.out.print(badPattern(row));
            }
        } else {
            System.out.print(rows());
        }
    }
}
