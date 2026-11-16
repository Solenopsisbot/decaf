package demo;

import java.util.function.Consumer;
import org.apache.commons.lang3.StringUtils;
import org.apache.commons.lang3.Validate;
import org.apache.commons.lang3.function.Consumers;

public final class Probe {
    private static String q(String value) {
        return value == null ? "<null>" : value.replace("\u0000", "<nul>");
    }

    public static String capitalize() {
        return q(StringUtils.capitalize(null)) + "|" + q(StringUtils.capitalize("")) + "|"
            + StringUtils.capitalize("hello") + "|" + StringUtils.capitalize("éclair") + "|"
            + StringUtils.capitalize("😀x") + "|" + StringUtils.capitalize("ǆuro") + "|"
            + StringUtils.capitalize("ßeta");
    }

    public static String pad() {
        return q(StringUtils.rightPad(null, 4)) + "|" + StringUtils.rightPad("", 3) + "|"
            + StringUtils.rightPad("ab", 5) + "|" + StringUtils.rightPad("😀", 5) + "|"
            + StringUtils.rightPad("ab", 5, "xy") + "|"
            + StringUtils.leftPad("ab", 5, "xy") + "|" + StringUtils.repeat("é", 3)
            + "|" + q(StringUtils.rightPad("ab", 5, (String) null))
            + "|" + q(StringUtils.rightPad("ab", 5, ""))
            + "|" + q(StringUtils.leftPad("ab", 5, (String) null))
            + "|" + q(StringUtils.leftPad("ab", 5, ""))
            + "|" + q(StringUtils.rightPad("ab", 5, (char) 0x1f600))
            + "|" + q(StringUtils.leftPad("ab", 5, (char) 0x1f600));
    }

    public static String unicode() {
        return StringUtils.containsIgnoreCase("Straße", "STRASSE") + "|"
            + StringUtils.containsIgnoreCase("Éclair", "ÉC") + "|"
            + StringUtils.containsIgnoreCase("e\u0301clair", "İ") + "|"
            + StringUtils.containsIgnoreCase("İ", "i") + "|"
            + q(StringUtils.defaultIfBlank(null, "fallback")) + "|"
            + StringUtils.defaultIfBlank("\u2003", "fallback") + "|"
            + StringUtils.defaultIfBlank("\u00a0", "fallback");
    }

    public static String nullRules() {
        return (StringUtils.rightPad(null, 4) == null ? "right-null|" : "right-bad|")
            + (StringUtils.leftPad(null, 4) == null ? "left-null|" : "left-bad|")
            + (StringUtils.repeat((String) null, 2) == null ? "repeat-null|" : "repeat-bad|")
            + (!StringUtils.containsIgnoreCase(null, "x") ? "contains-null|" : "contains-bad|")
            + (!StringUtils.containsIgnoreCase("x", null) ? "contains-needle-null|" : "contains-needle-bad|")
            + ("fallback".equals(StringUtils.defaultIfBlank(null, "fallback")) ? "blank-null" : "blank-bad");
    }

    public static String validate() {
        String bare;
        try {
            Validate.isTrue(false);
            bare = "no";
        } catch (Exception error) {
            bare = error.getClass().getName() + ":" + error.getMessage();
        }
        String formatted;
        try {
            Validate.isTrue(false, "bad %s %d", "é", 7);
            formatted = "no";
        } catch (Exception error) {
            formatted = error.getClass().getName() + ":" + error.getMessage();
        }
        String trueCase;
        try {
            Validate.isTrue(true, "never %s", "x");
            trueCase = "ok";
        } catch (Exception error) {
            trueCase = "bad";
        }
        String nullMessage;
        try {
            Validate.isTrue(false, null);
            nullMessage = "no";
        } catch (Exception error) {
            nullMessage = error.getClass().getName() + ":" + error.getMessage();
        }
        return bare + "|" + formatted + "|" + nullMessage + "|" + trueCase;
    }

    public static String surrogate() {
        String value = StringUtils.rightPad("a", 2, (char) 0xd800);
        return value.chars().mapToObj(c -> String.format("%04x", c)).collect(java.util.stream.Collectors.joining(",")) + "|len=" + value.length();
    }

    public static String nopIdentity() {
        Consumer<Object> first = Consumers.nop();
        Consumer<Object> second = Consumers.nop();
        first.accept("ignored");
        second.accept("ignored");
        return Boolean.toString(first == second);
    }
}
