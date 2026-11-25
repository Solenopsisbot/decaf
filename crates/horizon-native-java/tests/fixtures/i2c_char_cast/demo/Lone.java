package demo;

public class Lone {
    static int id(int v) { return v; }

    // Java answers "a" + U+D800: two chars, the second a lone surrogate half. The runtime keeps text as
    // Rust strings, which cannot hold one, so it counts the concatenation instead of answering it.
    public static String loneConcat() { return "a" + (char) id(0xd800); }

    // A char that Rust text can hold is answered (and matches Java 25, see Probe.charStrings).
    public static String plainConcat() { return "a" + (char) id(0x4e2d); }
}
