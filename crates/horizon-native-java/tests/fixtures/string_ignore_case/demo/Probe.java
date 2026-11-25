package demo;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.TreeMap;
import java.util.TreeSet;

/**
 * Case-insensitive `String` comparisons, run unchanged on both sides: its own
 * `main` on Java 25 (output is `java25.txt`), its static probes in Horizon.
 * Each probe answers one `<label>` row; `main` prints `<label>=<value>`, one
 * line per case. A call that throws prints `<exception class>:<message>`.
 *
 * Covered: `equalsIgnoreCase`, `compareToIgnoreCase` (exact value),
 * `regionMatches(boolean,int,String,int,int)` and the 4-argument
 * case-sensitive overload (offsets, lengths, overflow, null argument) and
 * `String.CASE_INSENSITIVE_ORDER` (compare, sort, TreeSet/TreeMap), on ASCII,
 * Latin-1, Greek, Turkish i, the Kelvin sign, Roman numerals, Cherokee,
 * Georgian, titlecase digraphs, supplementary letters (Deseret, Osage, Warang
 * Citi, Adlam, Medefaidrin) and unpaired surrogates.
 */
public final class Probe {

    private static String b(boolean z) {
        return z ? "true" : "false";
    }

    private static String i(int n) {
        return Integer.toString(n);
    }

    private static String ex(Throwable t) {
        return t.getClass().getName() + ":" + t.getMessage();
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static int rawCompare(Object a, Object b) {
        return ((java.util.Comparator) String.CASE_INSENSITIVE_ORDER).compare(a, b);
    }


    private static final String[] POOL = {"", "a", "A", "K", "\u212a", "i", "\u0130", "\u0131", "\u00e4", "\u00c4", "\u00df", "SS", "\u00b5", "\u039c", "\u00ff", "\u0178", "\u017f", "s", "\u0100", "\u0101", "\ue000", "\u03c2", "\u03a3", "\ud801\udc00", "\ud801\udc28", "\ud801\udc29", "\ud83d\ude00", "a\ud801\udc00", "A\ud801\udc28", "\u0100\ud801\udc00", "\u0101\ud801\udc28x", "\u00e4\ud801\udc00", "\u00c4\ud801\udc28"};

    private static final String[] REGION_POOL = {"aB", "\u00e4\u00c4", "x\ud801\udc00y", "X\ud801\udc28Y", "\ud801\udc00\u0100"};

    /** `equalsIgnoreCase` over every ordered pair of `POOL`, one digit each. */
    private static String gridEquals() {
        StringBuilder out = new StringBuilder();
        for (String a : POOL) {
            for (String b : POOL) {
                out.append(a.equalsIgnoreCase(b) ? '1' : '0');
            }
            out.append('/');
        }
        return out.toString();
    }

    /** `compareToIgnoreCase` over every ordered pair of `POOL`. */
    private static String gridCompare() {
        StringBuilder out = new StringBuilder();
        for (String a : POOL) {
            for (String b : POOL) {
                out.append(a.compareToIgnoreCase(b)).append(',');
            }
            out.append('/');
        }
        return out.toString();
    }

    /** `String.CASE_INSENSITIVE_ORDER.compare` over every ordered pair of `POOL`. */
    private static String gridOrder() {
        StringBuilder out = new StringBuilder();
        for (String a : POOL) {
            for (String b : POOL) {
                out.append(String.CASE_INSENSITIVE_ORDER.compare(a, b)).append(',');
            }
            out.append('/');
        }
        return out.toString();
    }

    /** `regionMatches(true, ...)` over every pair of `REGION_POOL` and every offset and length around it. */
    private static String gridRegion() {
        List<String> rows = new ArrayList<>();
        for (String a : REGION_POOL) {
            for (String b : REGION_POOL) {
                StringBuilder row = new StringBuilder();
                for (int t = -1; t <= a.length() + 1; t++) {
                    for (int o = -1; o <= b.length() + 1; o++) {
                        for (int n = -1; n <= 3; n++) {
                            row.append(a.regionMatches(true, t, b, o, n) ? '1' : '0');
                        }
                    }
                }
                rows.add(row.toString());
            }
        }
        return String.join("/", rows);
    }

    private static List<String> listOf(String... items) {
        List<String> list = new ArrayList<>();
        for (String item : items) {
            list.add(item);
        }
        return list;
    }

    private static List<String> words() {
        return listOf("banana", "Apple", "cherry", "apple", "\u00e4pfel", "Zebra", "\u00c4pfel", "zebra");
    }

    private static String sortedList() {
        List<String> list = words();
        Collections.sort(list, String.CASE_INSENSITIVE_ORDER);
        return String.join(",", list);
    }

    private static String sortedDeseret() {
        List<String> list = listOf("\ud801\udc28b", "\ud801\udc00a", "\ue000", "\uffff", "z", "\ud801\udc29");
        Collections.sort(list, String.CASE_INSENSITIVE_ORDER);
        StringBuilder out = new StringBuilder();
        for (String s : list) {
            for (int k = 0; k < s.length(); k++) {
                out.append(Integer.toHexString(s.charAt(k))).append(' ');
            }
            out.append('|');
        }
        return out.toString();
    }

    private static String listSort() {
        List<String> list = words();
        list.sort(String.CASE_INSENSITIVE_ORDER);
        return String.join(",", list);
    }

    private static String treeSet() {
        TreeSet<String> set = new TreeSet<>(String.CASE_INSENSITIVE_ORDER);
        set.addAll(words());
        return set.size() + ":" + set.contains("APPLE") + ":" + set.first() + ":" + set.last();
    }

    private static String treeMap() {
        TreeMap<String, Integer> map = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
        map.put("Key", 1);
        map.put("KEY", 2);
        map.put("\u00e4", 3);
        map.put("\u00c4", 4);
        return map.size() + ":" + map.get("kEy") + ":" + map.get("\u00e4") + ":" + map.firstKey();
    }

    private static String streamSorted() {
        return String.join(",", words().stream().sorted(String.CASE_INSENSITIVE_ORDER).toList());
    }

    public static String ee_kk() {
        try {
            return b("K".equalsIgnoreCase("k"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_ae() {
        try {
            return b("\u00e4".equalsIgnoreCase("\u00c4"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_oe() {
        try {
            return b("\u00d6".equalsIgnoreCase("\u00f6"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_kelvin() {
        try {
            return b("k".equalsIgnoreCase("\u212a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_kelvin_rev() {
        try {
            return b("\u212a".equalsIgnoreCase("k"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_kelvin_upper() {
        try {
            return b("K".equalsIgnoreCase("\u212a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_turk_i() {
        try {
            return b("\u0130".equalsIgnoreCase("i"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_turk_I() {
        try {
            return b("I".equalsIgnoreCase("\u0131"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_turk_dotless_i() {
        try {
            return b("\u0131".equalsIgnoreCase("i"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_turk_both() {
        try {
            return b("\u0130".equalsIgnoreCase("\u0131"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_sharp() {
        try {
            return b("\u00df".equalsIgnoreCase("SS"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_sharp_cap() {
        try {
            return b("\u00df".equalsIgnoreCase("\u1e9e"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_ydia() {
        try {
            return b("\u00ff".equalsIgnoreCase("\u0178"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_sigma() {
        try {
            return b("\u03c3".equalsIgnoreCase("\u03a3"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_sigma_final() {
        try {
            return b("\u03c2".equalsIgnoreCase("\u03a3"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_sigma_final_small() {
        try {
            return b("\u03c2".equalsIgnoreCase("\u03c3"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_roman() {
        try {
            return b("\u2160".equalsIgnoreCase("\u2170"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_long_s() {
        try {
            return b("\u017f".equalsIgnoreCase("s"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_long_s_cap() {
        try {
            return b("\u017f".equalsIgnoreCase("S"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_micro() {
        try {
            return b("\u00b5".equalsIgnoreCase("\u03bc"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_micro_cap() {
        try {
            return b("\u00b5".equalsIgnoreCase("\u039c"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_ohm() {
        try {
            return b("\u2126".equalsIgnoreCase("\u03c9"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_angstrom() {
        try {
            return b("\u212b".equalsIgnoreCase("\u00e5"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_cherokee() {
        try {
            return b("\u13a0".equalsIgnoreCase("\uab70"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_cherokee_cap() {
        try {
            return b("\u13a0".equalsIgnoreCase("\u13f0"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_georgian() {
        try {
            return b("\u10d0".equalsIgnoreCase("\u1c90"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_title_dz() {
        try {
            return b("\u01c5".equalsIgnoreCase("\u01c4"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_title_dz2() {
        try {
            return b("\u01c5".equalsIgnoreCase("\u01c6"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_beta_sym() {
        try {
            return b("\u03d0".equalsIgnoreCase("\u03b2"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_ypogegrammeni() {
        try {
            return b("\u1fbe".equalsIgnoreCase("\u0399"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_ypogegrammeni_small() {
        try {
            return b("\u1fbe".equalsIgnoreCase("\u03b9"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_jcaron() {
        try {
            return b("\u01f0".equalsIgnoreCase("J\u030c"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_sentence() {
        try {
            return b("Hello W\u00f6rld".equalsIgnoreCase("HELLO W\u00d6RLD"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_des_up_low() {
        try {
            return b("\ud801\udc00".equalsIgnoreCase("\ud801\udc28"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_des_low_up() {
        try {
            return b("\ud801\udc28".equalsIgnoreCase("\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_des_same() {
        try {
            return b("\ud801\udc00".equalsIgnoreCase("\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_des_mixed() {
        try {
            return b("a\ud801\udc00b".equalsIgnoreCase("A\ud801\udc28B"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_des_diff() {
        try {
            return b("\ud801\udc00".equalsIgnoreCase("\ud801\udc29"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_linearb() {
        try {
            return b("\ud800\udc00".equalsIgnoreCase("\ud800\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_osage() {
        try {
            return b("\ud801\udcb0".equalsIgnoreCase("\ud801\udcd8"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_warang() {
        try {
            return b("\ud806\udca0".equalsIgnoreCase("\ud806\udcc0"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_adlam() {
        try {
            return b("\ud83a\udd00".equalsIgnoreCase("\ud83a\udd22"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_medef() {
        try {
            return b("\ud81b\ude40".equalsIgnoreCase("\ud81b\ude60"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_supp_vs_two() {
        try {
            return b("\ud801\udc00".equalsIgnoreCase("ab"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_difflen() {
        try {
            return b("ab".equalsIgnoreCase("ABC"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_empty() {
        try {
            return b("".equalsIgnoreCase(""));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_empty_nonempty() {
        try {
            return b("".equalsIgnoreCase("a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String ee_null() {
        try {
            return b("a".equalsIgnoreCase(null));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_kk() {
        try {
            return i("K".compareToIgnoreCase("k"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_aeb() {
        try {
            return i("a".compareToIgnoreCase("b"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_ba() {
        try {
            return i("b".compareToIgnoreCase("a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_kelvin() {
        try {
            return i("k".compareToIgnoreCase("\u212a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_kelvin_l() {
        try {
            return i("\u212a".compareToIgnoreCase("l"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_kelvin_j() {
        try {
            return i("\u212a".compareToIgnoreCase("j"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_turk_i() {
        try {
            return i("i".compareToIgnoreCase("\u0130"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_turk_I() {
        try {
            return i("I".compareToIgnoreCase("\u0131"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_turk_dotless_z() {
        try {
            return i("\u0131".compareToIgnoreCase("z"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_sharp() {
        try {
            return i("\u00df".compareToIgnoreCase("SS"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_roman() {
        try {
            return i("\u2160".compareToIgnoreCase("\u2170"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_long_s_t() {
        try {
            return i("\u017f".compareToIgnoreCase("t"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_long_s_r() {
        try {
            return i("\u017f".compareToIgnoreCase("r"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_ae_z() {
        try {
            return i("\u00e4".compareToIgnoreCase("z"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_AE_aring() {
        try {
            return i("\u00c4".compareToIgnoreCase("\u00e5"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_sigma_final() {
        try {
            return i("\u03c2".compareToIgnoreCase("\u03a3"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_ohm_z() {
        try {
            return i("\u2126".compareToIgnoreCase("z"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_micro_cap() {
        try {
            return i("\u00b5".compareToIgnoreCase("\u039c"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_title_dz() {
        try {
            return i("\u01c5".compareToIgnoreCase("\u01c6"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_eq() {
        try {
            return i("\ud801\udc00".compareToIgnoreCase("\ud801\udc28"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_same() {
        try {
            return i("\ud801\udc00".compareToIgnoreCase("\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des0() {
        try {
            return i("\ud800\udc00".compareToIgnoreCase("\ud800\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_diff() {
        try {
            return i("\ud801\udc00".compareToIgnoreCase("\ud801\udc29"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_diff2() {
        try {
            return i("\ud801\udc29".compareToIgnoreCase("\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_longer() {
        try {
            return i("\ud801\udc00".compareToIgnoreCase("\ud801\udc28x"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_shorter() {
        try {
            return i("\ud801\udc28y".compareToIgnoreCase("\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_vs_bmp() {
        try {
            return i("\ud801\udc00".compareToIgnoreCase("\ue000"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_bmp_vs_des() {
        try {
            return i("\ue000".compareToIgnoreCase("\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_vs_ffff() {
        try {
            return i("\ud801\udc28".compareToIgnoreCase("\uffff"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_vs_ascii() {
        try {
            return i("\ud801\udc00".compareToIgnoreCase("a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_vs_d7ff() {
        try {
            return i("\ud801\udc00".compareToIgnoreCase("\ud7ff"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_adlam_ffff() {
        try {
            return i("\ud83a\udd22".compareToIgnoreCase("\uffff"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_len() {
        try {
            return i("ab".compareToIgnoreCase("abc"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_len2() {
        try {
            return i("abc".compareToIgnoreCase("ab"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_empty_a() {
        try {
            return i("".compareToIgnoreCase("a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_a_empty() {
        try {
            return i("a".compareToIgnoreCase(""));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_empty_empty() {
        try {
            return i("".compareToIgnoreCase(""));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_null() {
        try {
            return i("a".compareToIgnoreCase(null));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_sentence() {
        try {
            return i("Hello World".compareToIgnoreCase("hELLO wORLD!"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_basic() {
        try {
            return b("abc".regionMatches(true, 0, "ABC", 0, 3));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_off() {
        try {
            return b("abcdef".regionMatches(true, 2, "xxCDEFxx", 2, 4));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_umlaut_tail() {
        try {
            return b("ab\u00e4c".regionMatches(true, 2, "AB\u00c4C", 0, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_umlaut_mid() {
        try {
            return b("x\u00e4bc".regionMatches(true, 1, "X\u00c4BC", 1, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_cs() {
        try {
            return b("abc".regionMatches(false, 0, "ABC", 0, 3));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_oob() {
        try {
            return b("abc".regionMatches(true, 5, "ABC", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_neg_len() {
        try {
            return b("abc".regionMatches(true, 0, "ABC", 0, -1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_neg_len_end() {
        try {
            return b("abc".regionMatches(true, 3, "ABC", 3, -1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_neg_len_past() {
        try {
            return b("abc".regionMatches(true, 5, "ABC", 0, -1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_neg_len_past2() {
        try {
            return b("abc".regionMatches(true, 0, "ABC", 5, -1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_min_len() {
        try {
            return b("abc".regionMatches(true, 0, "ABC", 0, -2147483648));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_max_len() {
        try {
            return b("abc".regionMatches(true, 0, "ABC", 0, 2147483647));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_max_off() {
        try {
            return b("abc".regionMatches(true, 2147483647, "ABC", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_neg_toff() {
        try {
            return b("abc".regionMatches(true, -1, "ABC", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_neg_ooff() {
        try {
            return b("abc".regionMatches(true, 0, "ABC", -1, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_zero_len_end() {
        try {
            return b("abc".regionMatches(true, 3, "ABC", 3, 0));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_zero_len_past() {
        try {
            return b("abc".regionMatches(true, 4, "ABC", 0, 0));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_zero_len_past2() {
        try {
            return b("abc".regionMatches(true, 0, "ABC", 4, 0));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_len_too_long() {
        try {
            return b("abc".regionMatches(true, 1, "ABC", 0, 3));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_other_short() {
        try {
            return b("abcdef".regionMatches(true, 0, "AB", 0, 3));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_kelvin() {
        try {
            return b("k".regionMatches(true, 0, "\u212a", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_kelvin_cs() {
        try {
            return b("k".regionMatches(false, 0, "\u212a", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_turk() {
        try {
            return b("I".regionMatches(true, 0, "\u0131", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des() {
        try {
            return b("x\ud801\udc00y".regionMatches(true, 1, "X\ud801\udc28Y", 1, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_all() {
        try {
            return b("x\ud801\udc00y".regionMatches(true, 0, "X\ud801\udc28Y", 0, 4));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_cs() {
        try {
            return b("x\ud801\udc00y".regionMatches(false, 1, "X\ud801\udc28Y", 1, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_high_only() {
        try {
            return b("\ud801\udc00".regionMatches(true, 0, "\ud801\udc28", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_low_only() {
        try {
            return b("\ud801\udc00".regionMatches(true, 1, "\ud801\udc28", 1, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_low_start() {
        try {
            return b("a\ud801\udc00".regionMatches(true, 2, "a\ud801\udc28", 2, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_diff() {
        try {
            return b("\ud801\udc00".regionMatches(true, 0, "\ud801\udc29", 0, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_split_a() {
        try {
            return b("x\ud801\udc00".regionMatches(true, 2, "y\ud801\udc28", 2, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_split_b() {
        try {
            return b("\ud801\udc00x".regionMatches(true, 0, "\ud801\udc28y", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_null_ic() {
        try {
            return b("abc".regionMatches(true, 0, null, 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_null_cs() {
        try {
            return b("abc".regionMatches(false, 0, null, 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_null_oob() {
        try {
            return b("abc".regionMatches(true, 9, null, 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_null_neg() {
        try {
            return b("abc".regionMatches(true, -1, null, 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_null_len0() {
        try {
            return b("abc".regionMatches(true, 0, null, 0, 0));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_cs_umlaut() {
        try {
            return b("\u00e4".regionMatches(false, 0, "\u00c4", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_cs_same() {
        try {
            return b("\u00e4".regionMatches(false, 0, "\u00e4", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain() {
        try {
            return b("abc".regionMatches(0, "ABC", 0, 3));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_eq() {
        try {
            return b("abc".regionMatches(0, "abc", 0, 3));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_umlaut() {
        try {
            return b("ab\u00e4c".regionMatches(2, "AB\u00c4C", 0, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_umlaut_same() {
        try {
            return b("ab\u00e4c".regionMatches(2, "\u00e4c", 0, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_kelvin() {
        try {
            return b("k".regionMatches(0, "\u212a", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_oob() {
        try {
            return b("abc".regionMatches(5, "abc", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_neg_len() {
        try {
            return b("abc".regionMatches(0, "abc", 0, -1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_neg_len_past() {
        try {
            return b("abc".regionMatches(5, "abc", 0, -1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_neg_toff() {
        try {
            return b("abc".regionMatches(-1, "abc", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_neg_ooff() {
        try {
            return b("abc".regionMatches(0, "abc", -1, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_zero_end() {
        try {
            return b("abc".regionMatches(3, "abc", 3, 0));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_zero_past() {
        try {
            return b("abc".regionMatches(4, "abc", 0, 0));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_min_len() {
        try {
            return b("abc".regionMatches(0, "abc", 0, -2147483648));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_max_len() {
        try {
            return b("abc".regionMatches(0, "abc", 0, 2147483647));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_des() {
        try {
            return b("x\ud801\udc00y".regionMatches(1, "X\ud801\udc00Y", 1, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_des_case() {
        try {
            return b("x\ud801\udc00y".regionMatches(1, "x\ud801\udc28y", 1, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_null() {
        try {
            return b("abc".regionMatches(0, null, 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_null_oob() {
        try {
            return b("abc".regionMatches(9, null, 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_null_len0() {
        try {
            return b("abc".regionMatches(0, null, 0, 0));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_kk() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("k", "K"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_ae() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("\u00e4", "\u00c4"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_ab() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("a", "b"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_ba() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("b", "a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_kelvin() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("k", "\u212a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_sharp() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("\u00df", "SS"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_des_eq() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("\ud801\udc00", "\ud801\udc28"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_des_diff() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("\ud801\udc00", "\ud801\udc29"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_des_vs_bmp() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("\ud801\udc00", "\ue000"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_bmp_vs_des() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("\ue000", "\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_len() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("ab", "abc"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_null_first() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare(null, "a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_null_second() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare("a", null));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_null_both() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.compare(null, null));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_cp_prefix() {
        try {
            return i("\u0100\ud801\udc00".compareToIgnoreCase("\u0101"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_cp_prefix_rev() {
        try {
            return i("\u0101".compareToIgnoreCase("\u0100\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_latin_prefix_des() {
        try {
            return i("a".compareToIgnoreCase("A\ud801\udc00"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_des_vs_latin_prefix() {
        try {
            return i("A\ud801\udc28".compareToIgnoreCase("a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_mixed_des_first() {
        try {
            return i("\ud801\udc00".compareToIgnoreCase("\u00e4"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_mixed_micro() {
        try {
            return i("\u00b5".compareToIgnoreCase("\u039c\u0100"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cii_mixed_ydia() {
        try {
            return i("\u00ff".compareToIgnoreCase("\u0178"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_hi_end_diff() {
        try {
            return b("\ud801\udc00".regionMatches(true, 0, "\ud801\udc29", 0, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_lo_start_same() {
        try {
            return b("\ud801\udc00".regionMatches(true, 1, "\ud801\udc00", 1, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_lo_start_diff() {
        try {
            return b("\ud801\udc00".regionMatches(true, 1, "\ud801\udc29", 1, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_des_mid() {
        try {
            return b("x\ud801\udc00".regionMatches(true, 1, "x\ud801\udc28", 1, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_mixed_coding() {
        try {
            return b("\u00e4\u00c4".regionMatches(true, 0, "\u00c4\u00e4\u0100", 0, 2));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_null_neg_ooff() {
        try {
            return b("abc".regionMatches(true, 0, null, -1, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rmi_null_len_big() {
        try {
            return b("abc".regionMatches(true, 0, null, 0, 5));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String rm_plain_null_neg_ooff() {
        try {
            return b("abc".regionMatches(0, null, -1, 1));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_raw_int_first() {
        try {
            return i(rawCompare(Integer.valueOf(1), "a"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_raw_int_second() {
        try {
            return i(rawCompare("a", Integer.valueOf(1)));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_raw_null_int() {
        try {
            return i(rawCompare(null, Integer.valueOf(1)));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_raw_int_null() {
        try {
            return i(rawCompare(Integer.valueOf(1), null));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_same_object() {
        try {
            return b(String.CASE_INSENSITIVE_ORDER == String.CASE_INSENSITIVE_ORDER);
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_sorted_list() {
        try {
            return sortedList();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_sorted_des() {
        try {
            return sortedDeseret();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_list_sort() {
        try {
            return listSort();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_reversed() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.reversed().compare("a", "B"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_then() {
        try {
            return i(String.CASE_INSENSITIVE_ORDER.thenComparing(java.util.Comparator.naturalOrder()).compare("a", "A"));
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_treeset() {
        try {
            return treeSet();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_treemap() {
        try {
            return treeMap();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_stream() {
        try {
            return streamSorted();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String cmporder_tree_comparator() {
        try {
            return b(new java.util.TreeSet<String>(String.CASE_INSENSITIVE_ORDER).comparator() == String.CASE_INSENSITIVE_ORDER);
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String grid_equals() {
        try {
            return gridEquals();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String grid_compare() {
        try {
            return gridCompare();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String grid_order() {
        try {
            return gridOrder();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static String grid_region_ic() {
        try {
            return gridRegion();
        } catch (Throwable t) {
            return ex(t);
        }
    }

    public static void main(String[] args) {
        System.out.println("ee_kk=" + ee_kk());
        System.out.println("ee_ae=" + ee_ae());
        System.out.println("ee_oe=" + ee_oe());
        System.out.println("ee_kelvin=" + ee_kelvin());
        System.out.println("ee_kelvin_rev=" + ee_kelvin_rev());
        System.out.println("ee_kelvin_upper=" + ee_kelvin_upper());
        System.out.println("ee_turk_i=" + ee_turk_i());
        System.out.println("ee_turk_I=" + ee_turk_I());
        System.out.println("ee_turk_dotless_i=" + ee_turk_dotless_i());
        System.out.println("ee_turk_both=" + ee_turk_both());
        System.out.println("ee_sharp=" + ee_sharp());
        System.out.println("ee_sharp_cap=" + ee_sharp_cap());
        System.out.println("ee_ydia=" + ee_ydia());
        System.out.println("ee_sigma=" + ee_sigma());
        System.out.println("ee_sigma_final=" + ee_sigma_final());
        System.out.println("ee_sigma_final_small=" + ee_sigma_final_small());
        System.out.println("ee_roman=" + ee_roman());
        System.out.println("ee_long_s=" + ee_long_s());
        System.out.println("ee_long_s_cap=" + ee_long_s_cap());
        System.out.println("ee_micro=" + ee_micro());
        System.out.println("ee_micro_cap=" + ee_micro_cap());
        System.out.println("ee_ohm=" + ee_ohm());
        System.out.println("ee_angstrom=" + ee_angstrom());
        System.out.println("ee_cherokee=" + ee_cherokee());
        System.out.println("ee_cherokee_cap=" + ee_cherokee_cap());
        System.out.println("ee_georgian=" + ee_georgian());
        System.out.println("ee_title_dz=" + ee_title_dz());
        System.out.println("ee_title_dz2=" + ee_title_dz2());
        System.out.println("ee_beta_sym=" + ee_beta_sym());
        System.out.println("ee_ypogegrammeni=" + ee_ypogegrammeni());
        System.out.println("ee_ypogegrammeni_small=" + ee_ypogegrammeni_small());
        System.out.println("ee_jcaron=" + ee_jcaron());
        System.out.println("ee_sentence=" + ee_sentence());
        System.out.println("ee_des_up_low=" + ee_des_up_low());
        System.out.println("ee_des_low_up=" + ee_des_low_up());
        System.out.println("ee_des_same=" + ee_des_same());
        System.out.println("ee_des_mixed=" + ee_des_mixed());
        System.out.println("ee_des_diff=" + ee_des_diff());
        System.out.println("ee_linearb=" + ee_linearb());
        System.out.println("ee_osage=" + ee_osage());
        System.out.println("ee_warang=" + ee_warang());
        System.out.println("ee_adlam=" + ee_adlam());
        System.out.println("ee_medef=" + ee_medef());
        System.out.println("ee_supp_vs_two=" + ee_supp_vs_two());
        System.out.println("ee_difflen=" + ee_difflen());
        System.out.println("ee_empty=" + ee_empty());
        System.out.println("ee_empty_nonempty=" + ee_empty_nonempty());
        System.out.println("ee_null=" + ee_null());
        System.out.println("cii_kk=" + cii_kk());
        System.out.println("cii_aeb=" + cii_aeb());
        System.out.println("cii_ba=" + cii_ba());
        System.out.println("cii_kelvin=" + cii_kelvin());
        System.out.println("cii_kelvin_l=" + cii_kelvin_l());
        System.out.println("cii_kelvin_j=" + cii_kelvin_j());
        System.out.println("cii_turk_i=" + cii_turk_i());
        System.out.println("cii_turk_I=" + cii_turk_I());
        System.out.println("cii_turk_dotless_z=" + cii_turk_dotless_z());
        System.out.println("cii_sharp=" + cii_sharp());
        System.out.println("cii_roman=" + cii_roman());
        System.out.println("cii_long_s_t=" + cii_long_s_t());
        System.out.println("cii_long_s_r=" + cii_long_s_r());
        System.out.println("cii_ae_z=" + cii_ae_z());
        System.out.println("cii_AE_aring=" + cii_AE_aring());
        System.out.println("cii_sigma_final=" + cii_sigma_final());
        System.out.println("cii_ohm_z=" + cii_ohm_z());
        System.out.println("cii_micro_cap=" + cii_micro_cap());
        System.out.println("cii_title_dz=" + cii_title_dz());
        System.out.println("cii_des_eq=" + cii_des_eq());
        System.out.println("cii_des_same=" + cii_des_same());
        System.out.println("cii_des0=" + cii_des0());
        System.out.println("cii_des_diff=" + cii_des_diff());
        System.out.println("cii_des_diff2=" + cii_des_diff2());
        System.out.println("cii_des_longer=" + cii_des_longer());
        System.out.println("cii_des_shorter=" + cii_des_shorter());
        System.out.println("cii_des_vs_bmp=" + cii_des_vs_bmp());
        System.out.println("cii_bmp_vs_des=" + cii_bmp_vs_des());
        System.out.println("cii_des_vs_ffff=" + cii_des_vs_ffff());
        System.out.println("cii_des_vs_ascii=" + cii_des_vs_ascii());
        System.out.println("cii_des_vs_d7ff=" + cii_des_vs_d7ff());
        System.out.println("cii_adlam_ffff=" + cii_adlam_ffff());
        System.out.println("cii_len=" + cii_len());
        System.out.println("cii_len2=" + cii_len2());
        System.out.println("cii_empty_a=" + cii_empty_a());
        System.out.println("cii_a_empty=" + cii_a_empty());
        System.out.println("cii_empty_empty=" + cii_empty_empty());
        System.out.println("cii_null=" + cii_null());
        System.out.println("cii_sentence=" + cii_sentence());
        System.out.println("rmi_basic=" + rmi_basic());
        System.out.println("rmi_off=" + rmi_off());
        System.out.println("rmi_umlaut_tail=" + rmi_umlaut_tail());
        System.out.println("rmi_umlaut_mid=" + rmi_umlaut_mid());
        System.out.println("rmi_cs=" + rmi_cs());
        System.out.println("rmi_oob=" + rmi_oob());
        System.out.println("rmi_neg_len=" + rmi_neg_len());
        System.out.println("rmi_neg_len_end=" + rmi_neg_len_end());
        System.out.println("rmi_neg_len_past=" + rmi_neg_len_past());
        System.out.println("rmi_neg_len_past2=" + rmi_neg_len_past2());
        System.out.println("rmi_min_len=" + rmi_min_len());
        System.out.println("rmi_max_len=" + rmi_max_len());
        System.out.println("rmi_max_off=" + rmi_max_off());
        System.out.println("rmi_neg_toff=" + rmi_neg_toff());
        System.out.println("rmi_neg_ooff=" + rmi_neg_ooff());
        System.out.println("rmi_zero_len_end=" + rmi_zero_len_end());
        System.out.println("rmi_zero_len_past=" + rmi_zero_len_past());
        System.out.println("rmi_zero_len_past2=" + rmi_zero_len_past2());
        System.out.println("rmi_len_too_long=" + rmi_len_too_long());
        System.out.println("rmi_other_short=" + rmi_other_short());
        System.out.println("rmi_kelvin=" + rmi_kelvin());
        System.out.println("rmi_kelvin_cs=" + rmi_kelvin_cs());
        System.out.println("rmi_turk=" + rmi_turk());
        System.out.println("rmi_des=" + rmi_des());
        System.out.println("rmi_des_all=" + rmi_des_all());
        System.out.println("rmi_des_cs=" + rmi_des_cs());
        System.out.println("rmi_des_high_only=" + rmi_des_high_only());
        System.out.println("rmi_des_low_only=" + rmi_des_low_only());
        System.out.println("rmi_des_low_start=" + rmi_des_low_start());
        System.out.println("rmi_des_diff=" + rmi_des_diff());
        System.out.println("rmi_des_split_a=" + rmi_des_split_a());
        System.out.println("rmi_des_split_b=" + rmi_des_split_b());
        System.out.println("rmi_null_ic=" + rmi_null_ic());
        System.out.println("rmi_null_cs=" + rmi_null_cs());
        System.out.println("rmi_null_oob=" + rmi_null_oob());
        System.out.println("rmi_null_neg=" + rmi_null_neg());
        System.out.println("rmi_null_len0=" + rmi_null_len0());
        System.out.println("rmi_cs_umlaut=" + rmi_cs_umlaut());
        System.out.println("rmi_cs_same=" + rmi_cs_same());
        System.out.println("rm_plain=" + rm_plain());
        System.out.println("rm_plain_eq=" + rm_plain_eq());
        System.out.println("rm_plain_umlaut=" + rm_plain_umlaut());
        System.out.println("rm_plain_umlaut_same=" + rm_plain_umlaut_same());
        System.out.println("rm_plain_kelvin=" + rm_plain_kelvin());
        System.out.println("rm_plain_oob=" + rm_plain_oob());
        System.out.println("rm_plain_neg_len=" + rm_plain_neg_len());
        System.out.println("rm_plain_neg_len_past=" + rm_plain_neg_len_past());
        System.out.println("rm_plain_neg_toff=" + rm_plain_neg_toff());
        System.out.println("rm_plain_neg_ooff=" + rm_plain_neg_ooff());
        System.out.println("rm_plain_zero_end=" + rm_plain_zero_end());
        System.out.println("rm_plain_zero_past=" + rm_plain_zero_past());
        System.out.println("rm_plain_min_len=" + rm_plain_min_len());
        System.out.println("rm_plain_max_len=" + rm_plain_max_len());
        System.out.println("rm_plain_des=" + rm_plain_des());
        System.out.println("rm_plain_des_case=" + rm_plain_des_case());
        System.out.println("rm_plain_null=" + rm_plain_null());
        System.out.println("rm_plain_null_oob=" + rm_plain_null_oob());
        System.out.println("rm_plain_null_len0=" + rm_plain_null_len0());
        System.out.println("cmporder_kk=" + cmporder_kk());
        System.out.println("cmporder_ae=" + cmporder_ae());
        System.out.println("cmporder_ab=" + cmporder_ab());
        System.out.println("cmporder_ba=" + cmporder_ba());
        System.out.println("cmporder_kelvin=" + cmporder_kelvin());
        System.out.println("cmporder_sharp=" + cmporder_sharp());
        System.out.println("cmporder_des_eq=" + cmporder_des_eq());
        System.out.println("cmporder_des_diff=" + cmporder_des_diff());
        System.out.println("cmporder_des_vs_bmp=" + cmporder_des_vs_bmp());
        System.out.println("cmporder_bmp_vs_des=" + cmporder_bmp_vs_des());
        System.out.println("cmporder_len=" + cmporder_len());
        System.out.println("cmporder_null_first=" + cmporder_null_first());
        System.out.println("cmporder_null_second=" + cmporder_null_second());
        System.out.println("cmporder_null_both=" + cmporder_null_both());
        System.out.println("cii_cp_prefix=" + cii_cp_prefix());
        System.out.println("cii_cp_prefix_rev=" + cii_cp_prefix_rev());
        System.out.println("cii_latin_prefix_des=" + cii_latin_prefix_des());
        System.out.println("cii_des_vs_latin_prefix=" + cii_des_vs_latin_prefix());
        System.out.println("cii_mixed_des_first=" + cii_mixed_des_first());
        System.out.println("cii_mixed_micro=" + cii_mixed_micro());
        System.out.println("cii_mixed_ydia=" + cii_mixed_ydia());
        System.out.println("rmi_des_hi_end_diff=" + rmi_des_hi_end_diff());
        System.out.println("rmi_des_lo_start_same=" + rmi_des_lo_start_same());
        System.out.println("rmi_des_lo_start_diff=" + rmi_des_lo_start_diff());
        System.out.println("rmi_des_mid=" + rmi_des_mid());
        System.out.println("rmi_mixed_coding=" + rmi_mixed_coding());
        System.out.println("rmi_null_neg_ooff=" + rmi_null_neg_ooff());
        System.out.println("rmi_null_len_big=" + rmi_null_len_big());
        System.out.println("rm_plain_null_neg_ooff=" + rm_plain_null_neg_ooff());
        System.out.println("cmporder_raw_int_first=" + cmporder_raw_int_first());
        System.out.println("cmporder_raw_int_second=" + cmporder_raw_int_second());
        System.out.println("cmporder_raw_null_int=" + cmporder_raw_null_int());
        System.out.println("cmporder_raw_int_null=" + cmporder_raw_int_null());
        System.out.println("cmporder_same_object=" + cmporder_same_object());
        System.out.println("cmporder_sorted_list=" + cmporder_sorted_list());
        System.out.println("cmporder_sorted_des=" + cmporder_sorted_des());
        System.out.println("cmporder_list_sort=" + cmporder_list_sort());
        System.out.println("cmporder_reversed=" + cmporder_reversed());
        System.out.println("cmporder_then=" + cmporder_then());
        System.out.println("cmporder_treeset=" + cmporder_treeset());
        System.out.println("cmporder_treemap=" + cmporder_treemap());
        System.out.println("cmporder_stream=" + cmporder_stream());
        System.out.println("cmporder_tree_comparator=" + cmporder_tree_comparator());
        System.out.println("grid_equals=" + grid_equals());
        System.out.println("grid_compare=" + grid_compare());
        System.out.println("grid_order=" + grid_order());
        System.out.println("grid_region_ic=" + grid_region_ic());
    }
}
