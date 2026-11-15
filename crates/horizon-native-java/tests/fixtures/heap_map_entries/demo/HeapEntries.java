package demo;

import java.util.*;

/** Map entries and entry views held in statics across forced collections (#221, #226). */
public final class HeapEntries {
    static final class Key {
        final int id;
        Key(int id) { this.id = id; }
        public String toString() { return "K" + id; }
    }

    static Map<String, Integer> hm;
    static Map.Entry<String, Integer> node;
    static Set<Map.Entry<String, Integer>> view;
    static List<Map.Entry<String, Integer>> held;
    static Map.Entry<String, Integer> detached;
    static TreeMap<String, Integer> tm;
    static Map.Entry<String, Integer> treeNode;
    static IdentityHashMap<Object, Object> ihm;
    static Key idKey;
    static Key idValue;
    static Map.Entry<Object, Object> idEntry;
    static Map<String, Integer> sg;
    static Map.Entry<String, Integer> sgEntry;

    static <K, V> Map.Entry<K, V> first(Map<K, V> m) { return m.entrySet().iterator().next(); }

    public static String setup() {
        hm = new HashMap<>();
        hm.put("a", 1);
        hm.put("b", 2);
        hm.put("c", 3);
        view = hm.entrySet();
        node = first(hm);
        held = new ArrayList<>(hm.entrySet());
        hm.remove("c");
        detached = held.get(2);
        hm.put("a", 10);
        tm = new TreeMap<>();
        tm.put("x", 1);
        tm.put("y", 2);
        treeNode = first(tm);
        ihm = new IdentityHashMap<>();
        idKey = new Key(1);
        idValue = new Key(2);
        ihm.put(idKey, idValue);
        idEntry = first(ihm);
        sg = Collections.singletonMap("s", 5);
        sgEntry = first(sg);
        return "ok";
    }

    /** A fresh map per pass, walked: entries nothing keeps. */
    public static String garbage() {
        int s = 0;
        for (int i = 0; i < 300; i++) {
            Map<String, Integer> m = new HashMap<>();
            m.put("k" + i, i);
            m.put("j" + i, i);
            for (Map.Entry<String, Integer> e : m.entrySet()) s += e.getValue();
            Object o = new Object();
        }
        return "g" + (s & 1);
    }

    public static String check() {
        return "" + (node == first(hm)) + node + (view == hm.entrySet()) + view
            + view.contains(Map.entry("a", 10)) + detached + held
            + "|" + (treeNode == first(tm)) + treeNode
            + "|" + (idEntry.getKey() == idKey) + idEntry
            + idEntry.equals(new AbstractMap.SimpleEntry<Object, Object>(idKey, idValue))
            + "|" + (sgEntry == first(sg)) + sgEntry;
    }

    public static void main(String[] args) {
        System.out.println("setup=" + setup());
        for (int i = 0; i < 40; i++) garbage();
        System.out.println("check=" + check());
    }
}
