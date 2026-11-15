package demo;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.util.*;
import java.util.concurrent.*;

/**
 * Entries read through {@code Collections.unmodifiableMap} (#221 review 6, B10). Each is an
 * {@code UnmodifiableEntry} wrapping the entry the source map hands out: a Gson node keeps its
 * own value once removed, a {@code ConcurrentHashMap.MapEntry} keeps the value it was made with,
 * a {@code ConcurrentSkipListMap} entry is a snapshot.
 */
public final class Probe {
    static <K, V> Map.Entry<K, V> ent(Map<K, V> m, K k) {
        for (var e : m.entrySet()) if (e.getKey().equals(k)) return e;
        return null;
    }

    static JsonPrimitive p(Number n) {
        return new JsonPrimitive(n);
    }

    static JsonObject obj() {
        JsonObject o = new JsonObject();
        o.addProperty("a", 1);
        o.addProperty("b", 2);
        return o;
    }

    public static String v01UnmodView() {
        JsonObject o = obj();
        Map<String, JsonElement> v = Collections.unmodifiableMap(o.asMap());
        var e = ent(v, "a");
        o.remove("a");
        o.addProperty("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String v09UnmodOfUnmod() {
        JsonObject o = obj();
        var v = Collections.unmodifiableMap(Collections.unmodifiableMap(o.asMap()));
        var e = ent(v, "a");
        o.remove("a");
        o.addProperty("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String v10NestedObjView() {
        JsonObject o = new JsonObject();
        o.add("x", obj());
        var v = Collections.unmodifiableMap(o.getAsJsonObject("x").asMap());
        var e = ent(v, "a");
        o.getAsJsonObject("x").remove("a");
        o.getAsJsonObject("x").addProperty("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String v11ArrayObjView() {
        JsonArray arr = new JsonArray();
        arr.add(obj());
        var v = Collections.unmodifiableMap(arr.get(0).getAsJsonObject().asMap());
        var e = ent(v, "a");
        arr.get(0).getAsJsonObject().remove("a");
        arr.get(0).getAsJsonObject().addProperty("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String v12ViewEntrySetHeld() {
        JsonObject o = obj();
        Set<Map.Entry<String, JsonElement>> es = Collections.unmodifiableMap(o.asMap()).entrySet();
        var e = es.iterator().next();
        o.remove("a");
        o.addProperty("a", 5);
        return e.getValue() + "|" + e + "|" + es.size();
    }

    public static String v13ViewEqualsHashSet() {
        JsonObject o = obj();
        var v = Collections.unmodifiableMap(o.asMap());
        var e1 = ent(v, "a");
        o.remove("a");
        o.addProperty("a", 5);
        var e2 = ent(v, "a");
        Set<Object> s = new HashSet<>();
        s.add(e1);
        s.add(e2);
        return e1.equals(e2) + "|" + s.size() + "|" + e1.hashCode();
    }

    public static String v14ViewAsMapWrites() {
        JsonObject o = obj();
        var v = Collections.unmodifiableMap(o.asMap());
        var e = ent(v, "a");
        o.asMap().remove("a");
        o.asMap().put("a", p(5));
        return e.getValue() + "|" + e;
    }

    public static String v16ViewClear() {
        JsonObject o = obj();
        var v = Collections.unmodifiableMap(o.asMap());
        var e = ent(v, "a");
        o.asMap().clear();
        o.addProperty("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String v17ViewOfParsed() {
        JsonObject o = JsonParser.parseString("{\"a\":1,\"b\":2}").getAsJsonObject();
        var v = Collections.unmodifiableMap(o.asMap());
        var e = ent(v, "a");
        o.remove("a");
        o.addProperty("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String g01ViewToArray() {
        JsonObject o = obj();
        Object[] a = Collections.unmodifiableMap(o.asMap()).entrySet().toArray();
        o.remove("a");
        o.addProperty("a", 5);
        return a[0] + "";
    }

    public static String g04SyncOfUnmodGson() {
        JsonObject o = obj();
        var e = ent(Collections.synchronizedMap(Collections.unmodifiableMap(o.asMap())), "a");
        o.remove("a");
        o.addProperty("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String v18ChmView() {
        var m = new ConcurrentHashMap<String, Integer>();
        m.put("a", 1);
        var e = ent(Collections.unmodifiableMap(m), "a");
        m.put("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String v19ChmViewRemovePut() {
        var m = new ConcurrentHashMap<String, Integer>();
        m.put("a", 1);
        var e = ent(Collections.unmodifiableMap(m), "a");
        m.remove("a");
        m.put("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String c02ChmViewOfView() {
        var m = new ConcurrentHashMap<String, Integer>();
        m.put("a", 1);
        var e = ent(Collections.unmodifiableMap(Collections.unmodifiableMap(m)), "a");
        m.put("a", 5);
        return e.getValue() + "|" + e;
    }

    public static String a02CslmView() {
        var m = new ConcurrentSkipListMap<String, Integer>();
        m.put("a", 1);
        var e = ent(Collections.unmodifiableMap(m), "a");
        m.put("a", 5);
        return e.getValue() + "|" + e;
    }

    /** A `HashMap` node behind a view reads live, as the node does; `setValue` throws. */
    public static String hashMapView() {
        var m = new HashMap<String, Integer>();
        m.put("a", 1);
        var e = ent(Collections.unmodifiableMap(m), "a");
        m.put("a", 5);
        String thrown;
        try {
            e.setValue(9);
            thrown = "no";
        } catch (UnsupportedOperationException x) {
            thrown = "UOE";
        }
        return e.getValue() + "|" + e + "|" + thrown + "|" + m;
    }

    // Review 7, B11: JDK 25's `UnmodifiableCollection.toArray(IntFunction)` is `c.toArray(f)`, so a
    // view's entry set hands out its source's own entries there (`UnmodifiableEntrySet` wraps only
    // in `toArray()` and `toArray(T[])`): `setValue` writes through, and `==` the direct entry.
    static <M extends Map<String, Integer>> M fill(M m) {
        m.put("a", 1);
        m.put("b", 2);
        return m;
    }

    @SuppressWarnings("unchecked")
    static <K, V> Map.Entry<K, V>[] arr(Set<Map.Entry<K, V>> s) {
        return s.toArray(Map.Entry[]::new);
    }

    public static String d06ToArrayFn() {
        var m = fill(new HashMap<String, Integer>());
        Map.Entry<String, Integer>[] a = arr(Collections.unmodifiableMap(m).entrySet());
        try {
            a[0].setValue(9);
            return "set|" + m;
        } catch (UnsupportedOperationException x) {
            return "UOE|" + m;
        }
    }

    public static String t01HashFnSetValue() {
        var m = fill(new HashMap<String, Integer>());
        var a = arr(Collections.unmodifiableMap(m).entrySet());
        a[0].setValue(9);
        return m + "";
    }

    public static String t04ChmFnSetValue() {
        var m = fill(new ConcurrentHashMap<String, Integer>());
        var a = arr(Collections.unmodifiableMap(m).entrySet());
        a[0].setValue(9);
        return m + "|" + a[0];
    }

    public static String t05GsonFnSetValue() {
        JsonObject o = obj();
        Map.Entry<String, JsonElement>[] a = arr(Collections.unmodifiableMap(o.asMap()).entrySet());
        a[0].setValue(new JsonPrimitive(9));
        return o + "";
    }

    public static String t06TreeFnSetValue() {
        var m = fill(new TreeMap<String, Integer>());
        var a = arr(Collections.unmodifiableMap(m).entrySet());
        a[0].setValue(9);
        return m + "";
    }

    public static String t07NestedFnSetValue() {
        var m = fill(new HashMap<String, Integer>());
        var a = arr(Collections.unmodifiableMap(Collections.unmodifiableMap(m)).entrySet());
        a[0].setValue(9);
        return m + "";
    }

    public static String t08FnSameAsDirect() {
        var m = fill(new HashMap<String, Integer>());
        var a = arr(Collections.unmodifiableMap(m).entrySet());
        var d = ent(m, "a");
        return (a[0] == d) + "";
    }

    // Review 7, B12: `Map.Entry.comparingByKey` over entries read before a Gson write, directly
    // or through a view.
    public static String e11ComparingGsonReadd() {
        JsonObject o = obj();
        var l = new ArrayList<>(Collections.unmodifiableMap(o.asMap()).entrySet());
        o.remove("a");
        o.addProperty("a", 0);
        l.sort(Map.Entry.comparingByKey());
        return l + "";
    }

    public static String s01DirectGsonSort() {
        JsonObject o = obj();
        var l = new ArrayList<>(o.entrySet());
        o.remove("a");
        o.addProperty("a", 0);
        l.sort(Map.Entry.comparingByKey());
        return l + "";
    }

    public static String s02DirectGsonInPlace() {
        JsonObject o = obj();
        var g = new ArrayList<>(o.entrySet());
        o.addProperty("a", 7);
        try {
            g.sort(Map.Entry.comparingByKey());
            return g + "";
        } catch (RuntimeException x) {
            return x.getClass().getSimpleName();
        }
    }

    public static String s03ViewGsonStream() {
        JsonObject o = obj();
        var l = new ArrayList<>(Collections.unmodifiableMap(o.asMap()).entrySet());
        o.remove("a");
        o.addProperty("a", 0);
        return l.stream().sorted(Map.Entry.comparingByKey()).map(Object::toString)
            .collect(java.util.stream.Collectors.joining(","));
    }

    public static String s05ViewGsonTreeSet() {
        JsonObject o = obj();
        var l = new ArrayList<>(Collections.unmodifiableMap(o.asMap()).entrySet());
        o.remove("a");
        o.addProperty("a", 0);
        var t = new TreeSet<Map.Entry<String, JsonElement>>(Map.Entry.comparingByKey());
        t.addAll(l);
        return t.size() + "";
    }

    public static String s06ViewGsonCompare() {
        JsonObject o = obj();
        var l = new ArrayList<>(Collections.unmodifiableMap(o.asMap()).entrySet());
        o.remove("a");
        o.addProperty("a", 0);
        return Map.Entry.<String, JsonElement>comparingByKey().compare(l.get(0), l.get(1)) + "";
    }

    public static String s07ViewGsonCatch() {
        JsonObject o = obj();
        var l = new ArrayList<>(Collections.unmodifiableMap(o.asMap()).entrySet());
        o.remove("a");
        o.addProperty("a", 0);
        try {
            l.sort(Map.Entry.comparingByKey());
            return "ok|" + l;
        } catch (Throwable x) {
            return "caught " + x.getClass().getSimpleName();
        }
    }

    public static void main(String[] args) throws Exception {
        for (var m : Probe.class.getDeclaredMethods()) {
            if (m.getParameterCount() == 0 && m.getReturnType() == String.class
                    && java.lang.reflect.Modifier.isPublic(m.getModifiers())) {
                System.out.println(m.getName() + "=" + m.invoke(null));
            }
        }
    }
}
