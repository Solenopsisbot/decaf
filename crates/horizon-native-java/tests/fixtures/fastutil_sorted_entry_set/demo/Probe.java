package demo;

import it.unimi.dsi.fastutil.Hash;
import it.unimi.dsi.fastutil.objects.Object2BooleanAVLTreeMap;
import it.unimi.dsi.fastutil.objects.Object2BooleanLinkedOpenCustomHashMap;
import it.unimi.dsi.fastutil.objects.Object2BooleanLinkedOpenHashMap;
import it.unimi.dsi.fastutil.objects.Object2BooleanMap;
import it.unimi.dsi.fastutil.objects.Object2BooleanRBTreeMap;
import it.unimi.dsi.fastutil.objects.Object2BooleanSortedMap;
import it.unimi.dsi.fastutil.objects.ObjectBidirectionalIterator;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BiFunction;

/**
 * Oracle for fastutil 8.5.18 Object2Boolean*Map entry-set iteration with String keys.
 *
 * rows() is every case the runtime answers (`FastSortedEntrySet.iterator()` of the linked hash
 * map and the iterator it returns); countedRows() is the cases it leaves counted. Each prints one
 * line per case: <name><TAB><result>, an exception as "<exception-class-name>: <message>".
 *
 * Build/run (JDK 25, fastutil-8.5.18.jar on the class path):
 *   javac --release 21 -cp $FASTUTIL -d out demo/Probe.java
 *   java -cp $FASTUTIL:out demo.Probe          # rows()
 *   java -cp $FASTUTIL:out demo.Probe counted  # countedRows()
 */
public class Probe {

    interface Throwing<T> {
        T get() throws Throwable;
    }

    /** Run an action; on success return its string, on failure "<Class>: <msg>". */
    static String of(Throwing<String> f) {
        try {
            return f.get();
        } catch (Throwable t) {
            String msg = t.getMessage();
            return t.getClass().getName() + ": " + (msg == null ? "" : msg);
        }
    }

    /** The keys of each entry visited by an iterator, comma separated. */
    static String keys(Iterator<?> it) {
        StringBuilder sb = new StringBuilder();
        boolean first = true;
        while (it.hasNext()) {
            Object o = it.next();
            if (!first) {
                sb.append(',');
            }
            first = false;
            sb.append(String.valueOf(((Map.Entry<?, ?>) o).getKey()));
        }
        return sb.toString();
    }

    /** The keys of a list of entries, comma separated. */
    static String keys(List<Object2BooleanMap.Entry<String>> entries) {
        StringBuilder sb = new StringBuilder();
        for (Object2BooleanMap.Entry<String> e : entries) {
            if (sb.length() > 0) {
                sb.append(',');
            }
            sb.append(e.getKey());
        }
        return sb.toString();
    }

    static String keys(Object2BooleanLinkedOpenHashMap<String> map) {
        return keys(map.object2BooleanEntrySet().iterator());
    }

    /** The number of entries an iterator visits. */
    static int count(Iterator<?> it) {
        int n = 0;
        while (it.hasNext()) {
            it.next();
            n++;
        }
        return n;
    }

    /** Each entry as <char count>:<first char code>=<value>, so a lone surrogate shows. */
    static String surrogateText(Object2BooleanLinkedOpenHashMap<String> map) {
        StringBuilder sb = new StringBuilder();
        Iterator<Object2BooleanMap.Entry<String>> it = map.object2BooleanEntrySet().iterator();
        while (it.hasNext()) {
            Object2BooleanMap.Entry<String> e = it.next();
            String key = e.getKey();
            sb.append(key.length()).append(':').append(key.isEmpty() ? -1 : (int) key.charAt(0))
                    .append('=').append(e.getBooleanValue()).append(';');
        }
        return sb.toString();
    }

    static Object2BooleanLinkedOpenHashMap<String> linked() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
        m.put("banana", true);
        m.put("apple", false);
        m.put("cherry", true);
        return m;
    }

    /** A key whose equals is the class's own, not identity. */
    static final class K {
        final int id;

        K(int id) {
            this.id = id;
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof K k && k.id == id;
        }

        @Override
        public int hashCode() {
            return id;
        }
    }

    static StringBuilder out;

    static void line(String name, String result) {
        out.append(name).append('\t').append(result).append('\n');
    }

    public static void main(String[] args) {
        System.out.print(args.length > 0 ? countedRows() : rows());
    }

    public static String rows() {
        out = new StringBuilder();
        Object2BooleanLinkedOpenHashMap<String> linked = linked();

        // FastSortedEntrySet.iterator(): insertion order, read through java.util.Iterator.
        line("LINKED.iterator-order", of(() -> keys(linked.object2BooleanEntrySet().iterator())));
        // The same iterator, typed as the fastutil iterator (hasNext/next/getKey/getBooleanValue).
        line("LINKED.iterator-typed", of(() -> {
            ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it =
                    linked.object2BooleanEntrySet().iterator();
            StringBuilder sb = new StringBuilder();
            while (it.hasNext()) {
                Object2BooleanMap.Entry<String> e = it.next();
                sb.append(e.getKey()).append('=').append(e.getBooleanValue()).append(';');
            }
            return sb.toString();
        }));
        // Every iterator() call is a new iterator with its own cursor.
        line("LINKED.iterator-independent", of(() -> {
            ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> a =
                    linked.object2BooleanEntrySet().iterator();
            ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> b =
                    linked.object2BooleanEntrySet().iterator();
            String first = a.next().getKey() + "," + a.next().getKey();
            return first + "|" + b.next().getKey() + "|" + (a != b);
        }));
        // Each next() is a new entry; the entries outlive the iteration.
        line("LINKED.iterator-retained", of(() -> {
            List<Object2BooleanMap.Entry<String>> kept = new ArrayList<>();
            Iterator<Object2BooleanMap.Entry<String>> it = linked.object2BooleanEntrySet().iterator();
            while (it.hasNext()) {
                kept.add(it.next());
            }
            return keys(kept) + "|" + kept.get(0).getBooleanValue() + kept.get(1).getBooleanValue()
                    + kept.get(2).getBooleanValue();
        }));
        // Past the end, and remove() with no entry to remove.
        line("LINKED.iterator-past-end", of(() -> {
            ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it =
                    linked.object2BooleanEntrySet().iterator();
            it.next();
            it.next();
            it.next();
            boolean more = it.hasNext();
            it.next();
            return "no exception " + more;
        }));
        line("LINKED.iterator-remove-before-next", of(() -> {
            linked.object2BooleanEntrySet().iterator().remove();
            return "no exception";
        }));
        line("LINKED.iterator-remove-twice", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it =
                    m.object2BooleanEntrySet().iterator();
            it.next();
            it.remove();
            it.remove();
            return "no exception";
        }));
        // remove() through the iterator: the map shrinks and the iteration goes on. Nothing is
        // read from an entry that was returned before the removal.
        line("LINKED.iterator-remove-middle", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it =
                    m.object2BooleanEntrySet().iterator();
            int n = 0;
            while (it.hasNext()) {
                it.next();
                n++;
                if (n == 2) {
                    it.remove();
                }
            }
            return n + "|" + m.size() + "|" + keys(m) + "|" + m.containsKey("apple");
        }));
        line("LINKED.iterator-remove-last", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
            StringBuilder seen = new StringBuilder();
            while (it.hasNext()) {
                Object2BooleanMap.Entry<String> e = it.next();
                seen.append(e.getKey()).append(e.getBooleanValue()).append(';');
                if (!it.hasNext()) {
                    it.remove();
                }
            }
            return seen + "|" + m.size() + "|" + keys(m) + "|" + m.containsKey("cherry");
        }));
        line("LINKED.iterator-remove-all", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
            while (it.hasNext()) {
                it.next();
                it.remove();
            }
            return m.size() + "|" + m.isEmpty() + "|" + keys(m);
        }));
        // A key removed and put again moves to the end of the order.
        line("LINKED.iterator-reput", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            m.removeBoolean("banana");
            m.put("banana", false);
            m.put("apple", true);
            return keys(m);
        }));
        // The order of an empty-string key, and a re-put of a key already present: it keeps its
        // place and takes the new value.
        line("LINKED.iterator-order-empty-key", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
            m.put("b", true);
            m.put("a", true);
            m.put("c", true);
            m.put("aa", true);
            m.put("", true);
            return keys(m);
        }));
        line("LINKED.iterator-put-existing", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            m.put("banana", false);
            StringBuilder sb = new StringBuilder();
            Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
            while (it.hasNext()) {
                Object2BooleanMap.Entry<String> e = it.next();
                sb.append(e.getKey()).append(e.getBooleanValue()).append(';');
            }
            return sb + "|" + m.size();
        }));
        line("LINKED.iterator-empty", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
            ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it =
                    m.object2BooleanEntrySet().iterator();
            boolean more = it.hasNext();
            it.next();
            return "no exception " + more;
        }));
        // An entry set kept while the map does not change is the map's view: read twice, and a
        // fresh entry set per use sees a put.
        line("LINKED.entry-set-kept-unchanged", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
            boolean has = m.containsKey("apple");
            int size = m.size();
            boolean value = m.getBoolean("banana");
            return keys(es.iterator()) + "|" + size + "|" + has + "|" + value;
        }));
        line("LINKED.entry-set-fresh-per-use", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            String before = keys(m.object2BooleanEntrySet().iterator());
            m.put("date", true);
            return before + "|" + keys(m.object2BooleanEntrySet().iterator());
        }));
        // Keys of any length and code point, none a lone surrogate or U+FFFD.
        line("LINKED.iterator-unicode-keys", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
            m.put("", true);
            m.put("\u0000", false);
            m.put("\u00e9", true);
            m.put("\ud83d\ude00", false);
            m.put("a\u0000b", true);
            StringBuilder sb = new StringBuilder();
            Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
            while (it.hasNext()) {
                Object2BooleanMap.Entry<String> e = it.next();
                sb.append(e.getKey().length()).append(e.getBooleanValue()).append(';');
            }
            return sb + "|" + m.size();
        }));
        // Entries of one iteration are distinct objects (a new entry per next()), and an entry is
        // equal to itself and to an equal entry of another class; hashed, it is key ^ value.
        line("LINKED.entry-identity-consecutive", of(() -> {
            Iterator<Object2BooleanMap.Entry<String>> it = linked().object2BooleanEntrySet().iterator();
            Object a = it.next();
            Object b = it.next();
            return (a != b) + "|" + (a == a) + "|" + a.equals(a) + "|" + a.equals(b);
        }));
        // A second object2BooleanEntrySet() call makes new entries, equal to the first's.
        line("LINKED.entry-equals-fresh-set", of(() -> {
            Object2BooleanLinkedOpenHashMap<String> m = linked();
            Object a = m.object2BooleanEntrySet().iterator().next();
            Object b = m.object2BooleanEntrySet().iterator().next();
            return a.equals(b) + "|" + (a.hashCode() == b.hashCode());
        }));
        line("LINKED.entry-hash-and-sets", of(() -> {
            Iterator<Object2BooleanMap.Entry<String>> it = linked().object2BooleanEntrySet().iterator();
            java.util.Set<Object> hashed = new java.util.HashSet<>();
            Map<Object, Integer> identity = new java.util.IdentityHashMap<>();
            Map<Object, String> byEntry = new java.util.HashMap<>();
            Object first = null;
            StringBuilder sb = new StringBuilder();
            while (it.hasNext()) {
                Object2BooleanMap.Entry<String> e = it.next();
                first = first == null ? e : first;
                hashed.add(e);
                identity.put(e, 1);
                byEntry.put(e, e.getKey());
                sb.append(e.hashCode() == (e.getKey().hashCode() ^ (e.getBooleanValue() ? 1231 : 1237)))
                        .append(',');
            }
            return sb + "" + hashed.size() + "|" + identity.size() + "|" + hashed.contains(first)
                    + "|" + byEntry.get(first);
        }));
        line("LINKED.entry-equals-java-entries", of(() -> {
            Object e = linked().object2BooleanEntrySet().iterator().next();
            Object simple = new java.util.AbstractMap.SimpleEntry<>("banana", true);
            Object mapEntry = Map.entry("banana", true);
            Object other = Map.entry("banana", false);
            Object otherKey = Map.entry("apple", true);
            Object jarOtherKey = new JarEntry("apple", "x");
            Object jar = new JarEntry("banana", true);
            Object otherMap = linked().object2BooleanEntrySet().iterator().next();
            return e.equals(jarOtherKey) + "|" + e.equals(jar) + "|" + jar.equals(e) + "|"
                    + e.equals(otherMap) + "|" + otherMap.equals(e) + "|" + e.equals(simple) + "|" + simple.equals(e) + "|" + e.equals(mapEntry) + "|"
                    + mapEntry.equals(e) + "|" + e.equals(other) + "|" + e.equals(otherKey) + "|"
                    + e.equals(null) + "|" + e.equals("banana");
        }));
        return out.toString();
    }

    // --- The cases the runtime leaves counted. Each is its own method, so the test can run one
    // --- on its own host and see that it is counted; countedRows() prints them all.

    /** fastIterator() reuses one entry object for every next(). */
    static String fastIteratorOrderBody() {
        return keys(linked().object2BooleanEntrySet().fastIterator());
    }

    static String fastIteratorRetainedBody() {
        List<Object2BooleanMap.Entry<String>> kept = new ArrayList<>();
        Iterator<Object2BooleanMap.Entry<String>> it = linked().object2BooleanEntrySet().fastIterator();
        while (it.hasNext()) {
            kept.add(it.next());
        }
        return keys(kept);
    }

    /**
     * fastutil's iterator reads the map live and never fails fast: a key put under it is visited
     * when it lands after the cursor.
     */
    static String iteratorPutDuringBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
        StringBuilder seen = new StringBuilder();
        while (it.hasNext()) {
            String key = it.next().getKey();
            seen.append(key).append(';');
            if (key.equals("banana")) {
                m.put("date", true);
            }
        }
        return seen.toString();
    }

    static String iteratorRemoveDuringBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
        StringBuilder seen = new StringBuilder();
        while (it.hasNext()) {
            String key = it.next().getKey();
            seen.append(key).append(';');
            if (key.equals("banana")) {
                m.removeBoolean("cherry");
            }
        }
        return seen.toString();
    }

    /** An entry returned before a removal reads the map as it is after it. */
    static String iteratorReadAfterRemoveBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
        StringBuilder seen = new StringBuilder();
        while (it.hasNext()) {
            Object2BooleanMap.Entry<String> e = it.next();
            seen.append(e.getKey()).append(';');
            if (e.getKey().equals("apple")) {
                it.remove();
            }
        }
        return seen.toString();
    }

    /** The same changes under an iterator the program types as the fastutil iterator. */
    static String typedPutDuringBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it =
                m.object2BooleanEntrySet().iterator();
        int n = 0;
        while (it.hasNext()) {
            it.next();
            n++;
            if (n == 1) {
                m.put("zzz", true);
            }
        }
        return "" + n;
    }

    static String typedRemoveThenPutBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it =
                m.object2BooleanEntrySet().iterator();
        it.next();
        it.remove();
        m.put("zzz", true);
        int n = 0;
        while (it.hasNext()) {
            it.next();
            n++;
        }
        return n + "|" + m.size();
    }

    /** A put of an existing key under the iterator changes no order, but its entries read the map live. */
    static String iteratorPutExistingDuringBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
        Object2BooleanMap.Entry<String> first = it.next();
        m.put("banana", false);
        return first.getKey() + first.getBooleanValue();
    }

    /** A fastutil entry prints key=>value, not java.util's key=value. */
    static String entryToStringBody() {
        Object2BooleanMap.Entry<String> e = linked().object2BooleanEntrySet().iterator().next();
        return "" + e + "|" + String.valueOf(e) + "|" + e.toString();
    }

    /** setValue on an entry of the iterator writes through to the map (not modelled: counted). */
    static String entrySetValueBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanMap.Entry<String> e = m.object2BooleanEntrySet().iterator().next();
        boolean old = e.setValue(false);
        return old + "|" + m.getBoolean("banana") + "|" + e.getBooleanValue();
    }

    /** hasPrevious/previous of the bidirectional iterator (not modelled: counted). */
    static String iteratorPreviousBody() {
        ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it =
                linked().object2BooleanEntrySet().iterator();
        it.next();
        it.next();
        return it.hasPrevious() + "|" + it.previous().getKey();
    }

    /** An entry set kept across a change of the map is a live view. */
    static String entrySetKeptBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        m.put("date", true);
        return keys(es.iterator());
    }

    /** An entry set kept across a put is a live view; the host's is a copy made before it. */
    static String keptCountAfterPutBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        m.put("x", true);
        return "" + count(es.iterator());
    }

    static String keptCountAfterRemoveBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        m.removeBoolean("apple");
        return "" + count(es.iterator());
    }

    static String keptCountThenClearBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        m.clear();
        return "" + count(es.iterator());
    }

    /** The set made on an empty map sees the key put later. */
    static String keptEmptyThenPutBody() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        m.put("x", true);
        return "" + es.iterator().hasNext();
    }

    /** A set read, then the map changed, then read again. */
    static String keptTwoIterationsBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        int a = count(es.iterator());
        m.put("x", true);
        int b = count(es.iterator());
        return a + "|" + b;
    }

    /** The map changed through an iterator's remove(), then the set it came from is read. */
    static String keptAfterIteratorRemoveBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        ObjectBidirectionalIterator<Object2BooleanMap.Entry<String>> it = es.iterator();
        it.next();
        it.remove();
        return "" + count(es.iterator()) + m.size();
    }

    /** A key put and removed again: the same keys in the same order, but not the same map. */
    static String keptPutRemoveBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        m.put("x", true);
        m.removeBoolean("x");
        return "" + count(es.iterator());
    }

    /** A value changed through a put of a present key: the entries read the map's value. */
    static String keptPutExistingBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = m.object2BooleanEntrySet();
        m.put("banana", false);
        StringBuilder sb = new StringBuilder();
        Iterator<Object2BooleanMap.Entry<String>> it = es.iterator();
        while (it.hasNext()) {
            Object2BooleanMap.Entry<String> e = it.next();
            sb.append(e.getKey()).append(e.getBooleanValue()).append(';');
        }
        return sb.toString();
    }

    /**
     * Two iterators over one entry set: the host's copy of the set holds one entry object per key
     * and every iterator over it returns those, where each next() of fastutil's makes a new one.
     */
    static String keptTwoIteratorsBody() {
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = linked().object2BooleanEntrySet();
        return keys(es.iterator()) + "|" + keys(es.iterator());
    }

    static String entryIdentityTwoIteratorsBody() {
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = linked().object2BooleanEntrySet();
        Object a = es.iterator().next();
        Object b = es.iterator().next();
        return "" + (a == b);
    }

    static String entryIdentityLoopTwiceBody() {
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = linked().object2BooleanEntrySet();
        Map<Object, Integer> identity = new java.util.IdentityHashMap<>();
        for (Object2BooleanMap.Entry<String> e : es) {
            identity.put(e, 1);
        }
        for (Object2BooleanMap.Entry<String> e : es) {
            identity.put(e, 2);
        }
        return "" + identity.size();
    }

    /** The same entry set iterated through a `Set` reference and through its own type. */
    static String entryIdentityGenericThenFastBody() {
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = linked().object2BooleanEntrySet();
        java.util.Set<Object2BooleanMap.Entry<String>> generic = es;
        Object first = null;
        for (Object e : generic) {
            first = e;
            break;
        }
        Object second = es.iterator().next();
        return "" + (first == second);
    }

    static String entryIdentityFastThenGenericBody() {
        Object2BooleanSortedMap.FastSortedEntrySet<String> es = linked().object2BooleanEntrySet();
        Object first = es.iterator().next();
        java.util.Set<Object2BooleanMap.Entry<String>> generic = es;
        Object second = null;
        for (Object e : generic) {
            second = e;
            break;
        }
        return "" + (first == second);
    }

    /** Two keys that are lone surrogates: the host stores each as U+FFFD, so as one key. */
    static String surrogateKeysBody() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
        m.put("\ud800", true);
        m.put("\udc00", false);
        return m.size() + "|" + surrogateText(m);
    }

    static String fffdAndSurrogateKeysBody() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
        m.put("\ufffd", true);
        m.put("\ud800", false);
        return m.size() + "|" + surrogateText(m);
    }

    /** A lone surrogate key removed again leaves the other one behind. */
    static String surrogateRemoveBody() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
        m.put("x", true);
        m.put("\ud800", false);
        m.put("\udc00", true);
        m.removeBoolean("\ud800");
        return m.size() + "|" + surrogateText(m);
    }

    /** A genuine U+FFFD key: counted as well, the host cannot tell it from a lone surrogate. */
    static String fffdKeyBody() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>();
        m.put("\ufffd", true);
        m.put("a", false);
        return m.size() + "|" + surrogateText(m);
    }

    /** A null value put through the java.util.Map type: NullPointerException, the key is not added. */
    static String nullValuePutBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Map<String, Boolean> viewed = m;
        try {
            viewed.put("n", null);
        } catch (NullPointerException expected) {
            // The unboxing of the value.
        }
        return surrogateText(m) + "|" + m.size();
    }

    /** Writes the runtime does not follow: a bulk put, and a wrapper around the map. */
    static String putAllBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        LinkedHashMap<String, Boolean> more = new LinkedHashMap<>();
        more.put("date", true);
        more.put("apple", true);
        m.putAll(more);
        return surrogateText(m) + "|" + m.size();
    }

    static String wrapperBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Map<String, Boolean> wrapped = Collections.synchronizedMap(m);
        wrapped.put("date", true);
        return surrogateText(m) + "|" + m.size();
    }

    /** A method reference to the map's put, handed to library code: two lone surrogate keys. */
    static String methodRefPutBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Map<String, Boolean> viewed = m;
        BiFunction<String, Boolean, Boolean> put = viewed::put;
        List<String> keys = new ArrayList<>();
        keys.add("\ud800");
        keys.add("\udc00");
        keys.forEach(k -> put.apply(k, true));
        return m.size() + "|" + surrogateText(m);
    }

    static String computeBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Map<String, Boolean> viewed = m;
        viewed.computeIfAbsent("date", k -> true);
        return surrogateText(m) + "|" + m.size();
    }

    /** A Hash.Strategy replaces the key's own equals and hashCode. */
    static String customStrategyBody() {
        Object2BooleanLinkedOpenCustomHashMap<String> m =
                new Object2BooleanLinkedOpenCustomHashMap<>(new Hash.Strategy<String>() {
                    public int hashCode(String s) {
                        return s == null ? 0 : s.toLowerCase().hashCode();
                    }

                    public boolean equals(String a, String b) {
                        return a == null ? b == null : b != null && a.equalsIgnoreCase(b);
                    }
                });
        m.put("A", true);
        m.put("a", false);
        StringBuilder sb = new StringBuilder();
        Iterator<Object2BooleanMap.Entry<String>> it = m.object2BooleanEntrySet().iterator();
        while (it.hasNext()) {
            Object2BooleanMap.Entry<String> e = it.next();
            sb.append(e.getKey()).append(e.getBooleanValue()).append(';');
        }
        return sb + "|" + m.size();
    }

    /** A map built by a constructor with arguments. */
    static String copyMapCtorBody() {
        LinkedHashMap<String, Boolean> source = new LinkedHashMap<>();
        source.put("z", true);
        source.put("a", false);
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>(source);
        return m.size() + "|" + m.containsKey("z") + "|" + keys(m);
    }

    static String copyFastutilCtorBody() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>(linked());
        return m.size() + "|" + m.containsKey("banana") + "|" + keys(m);
    }

    static String copyArraysCtorBody() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>(
                new String[] {"x", "y"}, new boolean[] {true, false});
        return m.size() + "|" + m.containsKey("x") + "|" + keys(m);
    }

    static String capacityCtorBody() {
        Object2BooleanLinkedOpenHashMap<String> m = new Object2BooleanLinkedOpenHashMap<>(16);
        m.put("p", true);
        return m.size() + "|" + keys(m);
    }

    /** A jar class whose equals is its own (the host compares jar objects by identity). */
    static String jarClassKeysBody() {
        Object2BooleanLinkedOpenHashMap<K> m = new Object2BooleanLinkedOpenHashMap<>();
        m.put(new K(1), true);
        m.put(new K(1), false);
        StringBuilder sb = new StringBuilder();
        Iterator<Object2BooleanMap.Entry<K>> it = m.object2BooleanEntrySet().iterator();
        while (it.hasNext()) {
            Object2BooleanMap.Entry<K> e = it.next();
            sb.append(e.getKey().id).append(e.getBooleanValue()).append(';');
        }
        return sb + "|" + m.size();
    }

    /** Keys other than strings: boxes. */
    static String boxedKeysBody() {
        Object2BooleanLinkedOpenHashMap<Integer> m = new Object2BooleanLinkedOpenHashMap<>();
        m.put(Integer.valueOf(1), true);
        m.put(Integer.valueOf(2), false);
        StringBuilder sb = new StringBuilder();
        Iterator<Object2BooleanMap.Entry<Integer>> it = m.object2BooleanEntrySet().iterator();
        while (it.hasNext()) {
            sb.append(it.next().getKey()).append(';');
        }
        return sb.toString();
    }

    /** The tree maps' entry set is an ObjectSortedSet in natural key order, not a FastSortedEntrySet. */
    static String avlIteratorOrderBody() {
        Object2BooleanAVLTreeMap<String> avl = new Object2BooleanAVLTreeMap<>();
        avl.put("banana", true);
        avl.put("apple", false);
        avl.put("cherry", true);
        return keys(avl.object2BooleanEntrySet().iterator());
    }

    static String rbtIteratorOrderBody() {
        Object2BooleanRBTreeMap<String> rbt = new Object2BooleanRBTreeMap<>();
        rbt.put("banana", true);
        rbt.put("apple", false);
        rbt.put("cherry", true);
        return keys(rbt.object2BooleanEntrySet().iterator());
    }

    static String sortedIteratorOrderBody() {
        Object2BooleanAVLTreeMap<String> avl = new Object2BooleanAVLTreeMap<>();
        avl.put("banana", true);
        avl.put("apple", false);
        avl.put("cherry", true);
        Object2BooleanSortedMap<String> sorted = avl;
        return keys(sorted.object2BooleanEntrySet().iterator());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String fastIteratorOrder() {
        return of(Probe::fastIteratorOrderBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String fastIteratorRetained() {
        return of(Probe::fastIteratorRetainedBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String iteratorPutDuring() {
        return of(Probe::iteratorPutDuringBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String iteratorRemoveDuring() {
        return of(Probe::iteratorRemoveDuringBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String iteratorReadAfterRemove() {
        return of(Probe::iteratorReadAfterRemoveBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String typedPutDuring() {
        return of(Probe::typedPutDuringBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String typedRemoveThenPut() {
        return of(Probe::typedRemoveThenPutBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String iteratorPutExistingDuring() {
        return of(Probe::iteratorPutExistingDuringBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryToString() {
        return of(Probe::entryToStringBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entrySetValue() {
        return of(Probe::entrySetValueBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String iteratorPrevious() {
        return of(Probe::iteratorPreviousBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entrySetKept() {
        return of(Probe::entrySetKeptBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String customStrategy() {
        return of(Probe::customStrategyBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String copyMapCtor() {
        return of(Probe::copyMapCtorBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String copyFastutilCtor() {
        return of(Probe::copyFastutilCtorBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String copyArraysCtor() {
        return of(Probe::copyArraysCtorBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String capacityCtor() {
        return of(Probe::capacityCtorBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String jarClassKeys() {
        return of(Probe::jarClassKeysBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String boxedKeys() {
        return of(Probe::boxedKeysBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String avlIteratorOrder() {
        return of(Probe::avlIteratorOrderBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String rbtIteratorOrder() {
        return of(Probe::rbtIteratorOrderBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String sortedIteratorOrder() {
        return of(Probe::sortedIteratorOrderBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptCountAfterPut() {
        return of(Probe::keptCountAfterPutBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptCountAfterRemove() {
        return of(Probe::keptCountAfterRemoveBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptCountThenClear() {
        return of(Probe::keptCountThenClearBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptEmptyThenPut() {
        return of(Probe::keptEmptyThenPutBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptTwoIterations() {
        return of(Probe::keptTwoIterationsBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptAfterIteratorRemove() {
        return of(Probe::keptAfterIteratorRemoveBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptPutRemove() {
        return of(Probe::keptPutRemoveBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptPutExisting() {
        return of(Probe::keptPutExistingBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String keptTwoIterators() {
        return of(Probe::keptTwoIteratorsBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryIdentityTwoIterators() {
        return of(Probe::entryIdentityTwoIteratorsBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryIdentityLoopTwice() {
        return of(Probe::entryIdentityLoopTwiceBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryIdentityGenericThenFast() {
        return of(Probe::entryIdentityGenericThenFastBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryIdentityFastThenGeneric() {
        return of(Probe::entryIdentityFastThenGenericBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String surrogateKeys() {
        return of(Probe::surrogateKeysBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String fffdAndSurrogateKeys() {
        return of(Probe::fffdAndSurrogateKeysBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String surrogateRemove() {
        return of(Probe::surrogateRemoveBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String fffdKey() {
        return of(Probe::fffdKeyBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String nullValuePut() {
        return of(Probe::nullValuePutBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String putAll() {
        return of(Probe::putAllBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String wrapper() {
        return of(Probe::wrapperBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String compute() {
        return of(Probe::computeBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String methodRefPut() {
        return of(Probe::methodRefPutBody);
    }

    // --- Entries compared with entries the host does not model (a fastutil BasicEntry, a
    // --- java.util entry, a jar class): counted, never answered.

    static Object2BooleanMap.Entry<String> firstEntry() {
        return linked().object2BooleanEntrySet().iterator().next();
    }

    static Object2BooleanMap.Entry<String> basic() {
        return new it.unimi.dsi.fastutil.objects.AbstractObject2BooleanMap.BasicEntry<>("banana", true);
    }

    /** A jar Map.Entry that keeps Object's equals and hashCode. */
    static final class JarEntry implements Map.Entry<String, Object> {
        final String key;
        final Object value;

        JarEntry(String key, Object value) {
            this.key = key;
            this.value = value;
        }

        public String getKey() {
            return key;
        }

        public Object getValue() {
            return value;
        }

        public Object setValue(Object v) {
            throw new UnsupportedOperationException();
        }
    }

    static String entryEqualsBasicBody() {
        return "" + firstEntry().equals(basic());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryEqualsBasic() {
        return of(Probe::entryEqualsBasicBody);
    }

    static String basicEqualsEntryBody() {
        return "" + basic().equals(firstEntry());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String basicEqualsEntry() {
        return of(Probe::basicEqualsEntryBody);
    }

    static String entryEqualsBasicOtherValueBody() {
        return "" + firstEntry().equals(new it.unimi.dsi.fastutil.objects.AbstractObject2BooleanMap.BasicEntry<>("banana", false));
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryEqualsBasicOtherValue() {
        return of(Probe::entryEqualsBasicOtherValueBody);
    }

    static String entryHashCodeBasicBody() {
        return "" + (firstEntry().hashCode() == basic().hashCode());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryHashCodeBasic() {
        return of(Probe::entryHashCodeBasicBody);
    }



    static String entryListContainsBody() {
        List<Object> list = new ArrayList<>();
        list.add(basic());
        return list.contains(firstEntry()) + "|" + list.indexOf(firstEntry());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryListContains() {
        return of(Probe::entryListContainsBody);
    }

    static String basicListContainsBody() {
        List<Object> list = new ArrayList<>();
        list.add(firstEntry());
        return list.contains(basic()) + "|" + list.indexOf(basic());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String basicListContains() {
        return of(Probe::basicListContainsBody);
    }

    static String entryHashSetBody() {
        java.util.Set<Object> set = new java.util.HashSet<>();
        set.add(basic());
        return "" + set.contains(firstEntry());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryHashSet() {
        return of(Probe::entryHashSetBody);
    }

    static String basicHashSetBody() {
        java.util.Set<Object> set = new java.util.HashSet<>();
        set.add(firstEntry());
        return "" + set.contains(basic());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String basicHashSet() {
        return of(Probe::basicHashSetBody);
    }

    static String entryHashMapBody() {
        Map<Object, String> map = new java.util.HashMap<>();
        map.put(basic(), "found");
        return "" + map.get(firstEntry());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryHashMap() {
        return of(Probe::entryHashMapBody);
    }

    static String basicHashMapBody() {
        Map<Object, String> map = new java.util.HashMap<>();
        map.put(firstEntry(), "found");
        return "" + map.get(basic());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String basicHashMap() {
        return of(Probe::basicHashMapBody);
    }

















    static String entryEqualsNonBooleanValueBody() {
        Object e = firstEntry();
        return "" + e.equals(new JarEntry("banana", "x"));
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryEqualsNonBooleanValue() {
        return of(Probe::entryEqualsNonBooleanValueBody);
    }

    static String entryEqualsNullValueBody() {
        Object e = firstEntry();
        return "" + e.equals(new java.util.AbstractMap.SimpleEntry<>("banana", null));
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryEqualsNullValue() {
        return of(Probe::entryEqualsNullValueBody);
    }

    static String basicEqualsBasicBody() {
        return "" + basic().equals(basic());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String basicEqualsBasic() {
        return of(Probe::basicEqualsBasicBody);
    }

    static String objectBasicEqualsEntryBody() {
        Object o = basic();
        return "" + o.equals(firstEntry());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String objectBasicEqualsEntry() {
        return of(Probe::objectBasicEqualsEntryBody);
    }

    static String objectsEqualsBasicEntryBody() {
        return "" + java.util.Objects.equals(basic(), firstEntry());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String objectsEqualsBasicEntry() {
        return of(Probe::objectsEqualsBasicEntryBody);
    }

    static String objectsEqualsEntryBasicBody() {
        return "" + java.util.Objects.equals(firstEntry(), basic());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String objectsEqualsEntryBasic() {
        return of(Probe::objectsEqualsEntryBasicBody);
    }

    static String basicHashCodeBody() {
        Object o = basic();
        return "" + o.hashCode();
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String basicHashCode() {
        return of(Probe::basicHashCodeBody);
    }

    static String objectsHashCodeBasicBody() {
        return "" + java.util.Objects.hashCode(basic());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String objectsHashCodeBasic() {
        return of(Probe::objectsHashCodeBasicBody);
    }

    /** The first entries of two object2BooleanEntrySet() calls are two objects, equal. */
    static String entryIdentityFreshSetBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Object a = m.object2BooleanEntrySet().iterator().next();
        Object b = m.object2BooleanEntrySet().iterator().next();
        return "" + (a == b);
    }

    static String entryIdentityFreshSetMapBody() {
        Object2BooleanLinkedOpenHashMap<String> m = linked();
        Map<Object, Integer> identity = new java.util.IdentityHashMap<>();
        identity.put(m.object2BooleanEntrySet().iterator().next(), 1);
        identity.put(m.object2BooleanEntrySet().iterator().next(), 2);
        return "" + identity.size();
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryIdentityFreshSet() {
        return of(Probe::entryIdentityFreshSetBody);
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String entryIdentityFreshSetMap() {
        return of(Probe::entryIdentityFreshSetMapBody);
    }

    static String objectEntryEqualsBasicBody() {
        Object e = firstEntry();
        return "" + e.equals(basic());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String objectEntryEqualsBasic() {
        return of(Probe::objectEntryEqualsBasicBody);
    }

    static String objectEntryHashCodeBasicBody() {
        Object e = firstEntry();
        Object b = basic();
        return "" + (e.hashCode() == b.hashCode());
    }

    /** Run on its own: the exception a counted call's neutral answer causes is caught as a row's is. */
    public static String objectEntryHashCodeBasic() {
        return of(Probe::objectEntryHashCodeBasicBody);
    }



    public static String countedRows() {
        out = new StringBuilder();
        line("LINKED.fastIterator-order", fastIteratorOrder());
        line("LINKED.fastIterator-retained", fastIteratorRetained());
        line("LINKED.iterator-put-during", iteratorPutDuring());
        line("LINKED.iterator-remove-during", iteratorRemoveDuring());
        line("LINKED.iterator-read-after-remove", iteratorReadAfterRemove());
        line("LINKED.typed-put-during", typedPutDuring());
        line("LINKED.typed-remove-then-put", typedRemoveThenPut());
        line("LINKED.iterator-put-existing-during", iteratorPutExistingDuring());
        line("LINKED.entry-toString", entryToString());
        line("LINKED.entry-setValue", entrySetValue());
        line("LINKED.iterator-previous", iteratorPrevious());
        line("LINKED.entry-set-kept", entrySetKept());
        line("LINKED.kept-count-after-put", keptCountAfterPut());
        line("LINKED.kept-count-after-remove", keptCountAfterRemove());
        line("LINKED.kept-count-then-clear", keptCountThenClear());
        line("LINKED.kept-empty-then-put", keptEmptyThenPut());
        line("LINKED.kept-two-iterations", keptTwoIterations());
        line("LINKED.kept-after-iterator-remove", keptAfterIteratorRemove());
        line("LINKED.kept-put-remove", keptPutRemove());
        line("LINKED.kept-put-existing", keptPutExisting());
        line("LINKED.kept-two-iterators", keptTwoIterators());
        line("LINKED.entry-identity-two-iterators", entryIdentityTwoIterators());
        line("LINKED.entry-identity-loop-twice", entryIdentityLoopTwice());
        line("LINKED.entry-identity-generic-then-fast", entryIdentityGenericThenFast());
        line("LINKED.entry-identity-fast-then-generic", entryIdentityFastThenGeneric());
        line("LINKED.entry-equals-basic", entryEqualsBasic());
        line("LINKED.basic-equals-entry", basicEqualsEntry());
        line("LINKED.entry-equals-basic-other-value", entryEqualsBasicOtherValue());
        line("LINKED.entry-hashCode-basic", entryHashCodeBasic());
        line("LINKED.entry-list-contains-basic", entryListContains());
        line("LINKED.basic-list-contains-entry", basicListContains());
        line("LINKED.entry-hashset-basic", entryHashSet());
        line("LINKED.basic-hashset-entry", basicHashSet());
        line("LINKED.entry-hashmap-basic", entryHashMap());
        line("LINKED.basic-hashmap-entry", basicHashMap());
        line("LINKED.entry-identity-fresh-set", entryIdentityFreshSet());
        line("LINKED.entry-identity-fresh-set-map", entryIdentityFreshSetMap());
        line("LINKED.object-entry-equals-basic", objectEntryEqualsBasic());
        line("LINKED.object-entry-hashCode-basic", objectEntryHashCodeBasic());
        line("LINKED.entry-equals-non-boolean-value", entryEqualsNonBooleanValue());
        line("LINKED.entry-equals-null-value", entryEqualsNullValue());
        line("LINKED.basic-equals-basic", basicEqualsBasic());
        line("LINKED.object-basic-equals-entry", objectBasicEqualsEntry());
        line("LINKED.objects-equals-basic-entry", objectsEqualsBasicEntry());
        line("LINKED.objects-equals-entry-basic", objectsEqualsEntryBasic());
        line("LINKED.basic-hashCode", basicHashCode());
        line("LINKED.objects-hashCode-basic", objectsHashCodeBasic());
        line("KEYS.surrogate-pair-merged", surrogateKeys());
        line("KEYS.fffd-and-surrogate", fffdAndSurrogateKeys());
        line("KEYS.surrogate-removed", surrogateRemove());
        line("KEYS.fffd-genuine", fffdKey());
        line("KEYS.null-value-put", nullValuePut());
        line("MUTATE.put-all", putAll());
        line("MUTATE.wrapper", wrapper());
        line("MUTATE.compute-if-absent", compute());
        line("MUTATE.method-ref-put", methodRefPut());
        line("CUSTOM.iterator-strategy", customStrategy());
        line("COPY.map-ctor", copyMapCtor());
        line("COPY.fastutil-ctor", copyFastutilCtor());
        line("COPY.arrays-ctor", copyArraysCtor());
        line("COPY.capacity-ctor", capacityCtor());
        line("KEYS.jar-class", jarClassKeys());
        line("KEYS.boxed", boxedKeys());
        line("AVL.iterator-order", avlIteratorOrder());
        line("RBT.iterator-order", rbtIteratorOrder());
        line("AVL.sorted-iterator-order", sortedIteratorOrder());
        return out.toString();
    }
}
