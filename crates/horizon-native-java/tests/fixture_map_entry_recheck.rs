//! The re-checks of #221 (#156 P14e, reviews 5905494234 and 5906897628): a library map's stored
//! entry is one object across readings while the map is unchanged, and counted across a change
//! (B4, B6), two equal
//! `entrySet()` views are equal inside other collections (B5), an `IdentityHashMap` entry
//! compares another entry's key and value by `==` (B1), `remove(key)` after a walk detaches only
//! that key's node (B3), and a hash map keyed by equal `JsonObject`s keeps one key (B7).
//!
//! The stubs exist only for `javac`: the host models Guava's `ImmutableMap` and Gson's
//! `JsonObject` itself.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;

/// OpenJDK 25.0.5 running this `Probe` (its `main`, left out here, prints each method), compiled
/// against the real guava-33.6.0-jre and gson-2.14.0 jars of the 26.3 libraries; the same on two
/// runs; pasted verbatim.
const EXPECTED: &str = "\
storedEntries=truetrue1|truetrue2|true2\n\
wrappedEntries=falsetrue\n\
viewsInCollections=true111true1true|java.lang.IllegalArgumentException:duplicate element: [a=1, b=2]|false\n\
identityEntries=truefalsetruefalsetruefalsetruefalsetruefalse\n\
removeAfterWalk=[a=7, b=2, c=3]|falsetrue|2b=8{a=7, b=5, c=3}\n\
unknownReadings=false\n\
gsonRemoveReadd=falsetruefalse\n\
equalJsonKeys=2|32|32|{\"a\":\"b\"}{\"c\":\"d\"}[3, 2]true3\n\
gsonHashKeys=221truetrue|1|1072693248|96|1072693345\n\
gsonRemovedEntry=1|5|2\n\
x01ReplaceRemoveReadd=2|a=2|false\n\
d02ReplaceRemoveReadd=3|false|98|2|false\n\
h17ListKeys=2|true|2\n\
c02ArrayListKeys=2\n\
c03EntryKeys=2\n\
c08LinkedHashMap=2|[3, 2]\n\
";

const IMMUTABLE_MAP_STUB: &str = "package com.google.common.collect;\n\
public abstract class ImmutableMap<K, V> implements java.util.Map<K, V> {\n\
public static <K, V> ImmutableMap<K, V> of(K k, V v) { throw new AssertionError(); }\n\
public static <K, V> ImmutableMap<K, V> of(K k, V v, K k2, V v2) { throw new AssertionError(); }\n}\n";

const JSON_ELEMENT_STUB: &str = "package com.google.gson;\npublic abstract class JsonElement {}\n";

const JSON_PRIMITIVE_STUB: &str = "package com.google.gson;\n\
public final class JsonPrimitive extends JsonElement {\n\
public JsonPrimitive(Number n) { throw new AssertionError(); }\n}\n";

const JSON_NULL_STUB: &str = "package com.google.gson;\n\
public final class JsonNull extends JsonElement { public static final JsonNull INSTANCE = null; }\n";

const JSON_OBJECT_STUB: &str = "package com.google.gson;\n\
public final class JsonObject extends JsonElement {\n\
public void add(String p, JsonElement v) { throw new AssertionError(); }\n\
public void addProperty(String p, Number v) { throw new AssertionError(); }\n\
public void addProperty(String p, String v) { throw new AssertionError(); }\n\
public JsonElement remove(String p) { throw new AssertionError(); }\n\
public java.util.Set<java.util.Map.Entry<String, JsonElement>> entrySet() { throw new AssertionError(); }\n}\n";

const PROBE: &str = r##"package demo;

import com.google.common.collect.ImmutableMap;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import java.util.*;
import java.util.function.Supplier;

@SuppressWarnings({"rawtypes", "unchecked"})
public final class Probe {
    record P(int x) {}

    static final class JE implements Map.Entry<Object, Object> {
        final Object k, v;
        JE(Object k, Object v) { this.k = k; this.v = v; }
        public Object getKey() { return k; }
        public Object getValue() { return v; }
        public Object setValue(Object x) { throw new UnsupportedOperationException(); }
        public boolean equals(Object o) { return o instanceof Map.Entry<?, ?> e && Objects.equals(k, e.getKey()) && Objects.equals(v, e.getValue()); }
        public int hashCode() { return Objects.hashCode(k) ^ Objects.hashCode(v); }
    }

    static String t(Supplier<Object> s) {
        try { return String.valueOf(s.get()); }
        catch (RuntimeException e) { return e.getClass().getName() + ":" + e.getMessage(); }
    }
    static Map<String, Integer> fill(Map<String, Integer> m) { m.put("a", 1); m.put("b", 2); return m; }
    static Map.Entry first(Map m) { return (Map.Entry) m.entrySet().iterator().next(); }
    static int distinct(Iterable<?> a, Iterable<?> b) {
        Map<Object, Boolean> ids = new IdentityHashMap<>();
        for (Object e : a) ids.put(e, true);
        for (Object e : b) ids.put(e, true);
        return ids.size();
    }

    // B4: a stored library entry is one object across readings.
    public static String storedEntries() {
        Map<String, Integer> sg = Collections.singletonMap("a", 1);
        Object x = null, y = null;
        for (var e : sg.entrySet()) x = e;
        for (var e : sg.entrySet()) y = e;
        Map<String, Integer> g1 = ImmutableMap.of("a", 1);
        Map<String, Integer> g2 = ImmutableMap.of("a", 1, "b", 2);
        JsonObject o = new JsonObject();
        o.addProperty("a", 1);
        o.addProperty("b", 2);
        return "" + (first(sg) == first(sg)) + (x == y) + distinct(sg.entrySet(), sg.entrySet())
            + "|" + (first(g1) == first(g1)) + (first(g2) == first(g2)) + distinct(g2.entrySet(), g2.entrySet())
            + "|" + (o.entrySet().iterator().next() == o.entrySet().iterator().next()) + distinct(o.entrySet(), o.entrySet());
    }

    // B4: a map whose readings this model does not know (Java 25's `Properties` hands out a
    // new `ConcurrentHashMap.MapEntry` per `next()`; `false`): counted.
    public static String unknownReadings() {
        Properties p = new Properties();
        p.put("a", "1");
        return "" + (first(p) == first(p));
    }

    // B6: Gson's `LinkedTreeMap.remove` unlinks the node and `put` makes a new one: the old
    // reading is another object than the readings after (counted), which are one object.
    public static String gsonRemoveReadd() {
        JsonObject o = new JsonObject();
        o.addProperty("a", 1);
        Object e1 = o.entrySet().iterator().next();
        o.remove("a");
        o.addProperty("a", 1);
        Object e2 = o.entrySet().iterator().next();
        Object e3 = o.entrySet().iterator().next();
        return "" + (e1 == e2) + (e2 == e3) + (e1 == e3);
    }

    // B7: a hash map keyed by equal `JsonObject`s (`JsonElement.equals` by contents) keeps one key.
    static JsonObject j(String k, String v) { JsonObject o = new JsonObject(); o.addProperty(k, v); return o; }

    public static String equalJsonKeys() {
        Map<JsonObject, Integer> m = new LinkedHashMap<>();
        m.put(j("a", "b"), 1);
        m.put(j("c", "d"), 2);
        m.put(j("a", "b"), 3);
        StringBuilder walked = new StringBuilder();
        for (var e : m.entrySet()) walked.append(e.getValue());
        StringBuilder each = new StringBuilder();
        m.forEach((k, v) -> each.append(v));
        StringBuilder keys = new StringBuilder();
        for (var k : m.keySet()) keys.append(k);
        return m.size() + "|" + walked + "|" + each + "|" + keys + m.values() + m.containsKey(j("c", "d")) + m.get(j("a", "b"));
    }

    // B9: `JsonPrimitive.hashCode` hashes an integral number by its `long` and any other by
    // `doubleToLongBits`, so `1` and `1.0` are equal keys in two buckets: two keys.
    public static String gsonHashKeys() {
        Map<Object, Integer> m = new HashMap<>();
        m.put(new JsonPrimitive(1), 1);
        m.put(new JsonPrimitive(1.0), 2);
        JsonObject a = new JsonObject();
        a.addProperty("a", 1);
        JsonObject b = new JsonObject();
        b.addProperty("a", 1.0);
        Map<Object, Integer> n = new HashMap<>();
        n.put(a, 1);
        n.put(b, 2);
        Map<Object, Integer> same = new HashMap<>();
        same.put(new JsonPrimitive(3), 1);
        same.put(new JsonPrimitive(3L), 2);
        return "" + m.size() + n.size() + same.size() + new JsonPrimitive(1).equals(new JsonPrimitive(1.0))
            + a.equals(b) + "|" + new JsonPrimitive(1).hashCode() + "|" + new JsonPrimitive(1.0).hashCode()
            + "|" + a.hashCode() + "|" + b.hashCode();
    }

    // B8: Gson's `remove` unlinks the node, which keeps its fields; a re-add is a new node.
    public static String gsonRemovedEntry() {
        JsonObject o = new JsonObject();
        o.addProperty("a", 1);
        Map.Entry e1 = o.entrySet().iterator().next();
        o.remove("a");
        o.addProperty("a", 5);
        Map.Entry e2 = o.entrySet().iterator().next();
        return e1.getValue() + "|" + e2.getValue() + "|" + new HashSet<Object>(List.of(e1, e2)).size();
    }

    static Map.Entry<String, JsonElement> ent(JsonObject o, String k) {
        for (var e : o.entrySet()) if (e.getKey().equals(k)) return e;
        return null;
    }

    static JsonPrimitive p(Number n) {
        return new JsonPrimitive(n);
    }

    // Review 5, B8 (`x01`): replaced in place, removed (the node keeps `2`), the first value
    // object re-added as a new node.
    public static String x01ReplaceRemoveReadd() {
        JsonObject o = new JsonObject();
        o.addProperty("a", 1);
        o.addProperty("b", 9);
        var e1 = ent(o, "a");
        JsonElement first = e1.getValue();
        o.addProperty("a", 2);
        o.remove("a");
        o.add("a", first);
        return e1.getValue() + "|" + e1 + "|" + (e1.getValue() == first);
    }

    // Review 5, B8 (`d02`): the same with `add`, then a new reading.
    public static String d02ReplaceRemoveReadd() {
        JsonObject o = new JsonObject();
        o.addProperty("a", 1);
        var e1 = ent(o, "a");
        JsonElement first = e1.getValue();
        o.add("a", p(3));
        o.remove("a");
        o.add("a", first);
        var e2 = ent(o, "a");
        return e1.getValue() + "|" + e1.equals(e2) + "|" + e1.hashCode() + "|"
            + new HashSet<Object>(List.of(e1, e2)).size() + "|" + List.of(e1).contains(e2);
    }

    // Review 5, B9: containers of Gson elements with an identity-hashed member are equal with
    // different hashes, as `1` and `1.0` are.
    public static String h17ListKeys() {
        Object o = new Object();
        Set<Object> s = new HashSet<>();
        s.add(List.of(p(1), o));
        s.add(List.of(p(1.0), o));
        Set<Object> t = new HashSet<>();
        t.add(List.of(JsonNull.INSTANCE, p(1)));
        t.add(List.of(JsonNull.INSTANCE, p(1.0)));
        return s.size() + "|" + s.contains(List.of(p(1), o)) + "|" + t.size();
    }

    public static String c02ArrayListKeys() {
        Set<Object> s = new HashSet<>();
        s.add(new ArrayList<>(List.of(JsonNull.INSTANCE, p(1))));
        s.add(new ArrayList<>(List.of(JsonNull.INSTANCE, p(1.0))));
        return "" + s.size();
    }

    public static String c03EntryKeys() {
        Set<Object> s = new HashSet<>();
        s.add(Map.entry(JsonNull.INSTANCE, p(1)));
        s.add(Map.entry(JsonNull.INSTANCE, p(1.0)));
        return "" + s.size();
    }

    public static String c08LinkedHashMap() {
        Map<Object, Integer> m = new LinkedHashMap<>();
        m.put(List.of(JsonNull.INSTANCE, p(1)), 1);
        m.put(List.of(JsonNull.INSTANCE, p(1.0)), 2);
        m.put(List.of(JsonNull.INSTANCE, p(1)), 3);
        return m.size() + "|" + m.values();
    }

    // B4: `unmodifiableMap` wraps a new `UnmodifiableEntry` per `next()`.
    public static String wrappedEntries() {
        Map<String, Integer> u = Collections.unmodifiableMap(fill(new HashMap<>()));
        return "" + (first(u) == first(u)) + first(u).equals(first(u));
    }

    // B5: two equal `entrySet()` views inside collections.
    public static String viewsInCollections() {
        Map<String, Integer> m = fill(new HashMap<>()), n = fill(new HashMap<>());
        Set<Object> holding = new HashSet<>();
        holding.add(m.entrySet());
        Map<Object, Integer> keyed = new HashMap<>();
        keyed.put(m.entrySet(), 1);
        return "" + List.of(m.entrySet()).contains(n.entrySet())
            + new HashSet<Object>(List.of(m.entrySet(), n.entrySet())).size()
            + new HashSet<Object>(List.of(fill(new LinkedHashMap<>()).entrySet(), fill(new LinkedHashMap<>()).entrySet())).size()
            + new HashSet<Object>(List.of(fill(new TreeMap<>()).entrySet(), fill(new TreeMap<>()).entrySet())).size()
            + holding.contains(n.entrySet()) + keyed.get(n.entrySet()) + keyed.containsKey(n.entrySet())
            + "|" + t(() -> Set.of(m.entrySet(), n.entrySet()))
            + "|" + List.of(m.entrySet()).contains(fill(new HashMap<>()).keySet());
    }

    // B1: an `IdentityHashMap` entry compares the other entry's key and value by `==`.
    public static String identityEntries() {
        P p = new P(1), q = new P(2);
        IdentityHashMap<Object, Object> m = new IdentityHashMap<>();
        m.put(p, q);
        var e = first(m);
        IdentityHashMap<Object, Object> nk = new IdentityHashMap<>();
        nk.put(null, q);
        IdentityHashMap<Object, Object> nv = new IdentityHashMap<>();
        nv.put(p, null);
        return "" + e.equals(new AbstractMap.SimpleEntry<>(p, q)) + e.equals(new AbstractMap.SimpleEntry<>(p, new P(2)))
            + e.equals(new AbstractMap.SimpleImmutableEntry<>(p, q)) + e.equals(new AbstractMap.SimpleImmutableEntry<>(new P(1), q))
            + e.equals(new JE(p, q)) + e.equals(new JE(new P(1), q))
            + first(nk).equals(new AbstractMap.SimpleEntry<>(null, q)) + first(nk).equals(new AbstractMap.SimpleEntry<>(null, new P(2)))
            + first(nv).equals(new AbstractMap.SimpleEntry<>(p, null)) + first(nv).equals(new AbstractMap.SimpleEntry<>(new P(1), null));
    }

    // B3: `remove(key)` after a walk detaches only that key's node.
    public static String removeAfterWalk() {
        Map<String, Integer> m = fill(new HashMap<>());
        m.put("c", 3);
        List<Map.Entry<String, Integer>> held = new ArrayList<>(m.entrySet());
        m.remove("b");
        m.remove("zz");
        m.remove("c", 9);
        m.put("a", 7);
        m.put("b", 5);
        Map.Entry<String, Integer> b2 = null;
        for (var e : m.entrySet()) if (e.getKey().equals("b")) b2 = e;
        return held + "|" + (held.get(1) == b2) + (held.get(0) == first(m)) + "|" + held.get(1).setValue(8) + held.get(1) + m;
    }
}
"##;

fn fixture() -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(
        "map-entry-recheck",
        &[
            (
                "com/google/common/collect/ImmutableMap.java",
                IMMUTABLE_MAP_STUB,
            ),
            ("com/google/gson/JsonElement.java", JSON_ELEMENT_STUB),
            ("com/google/gson/JsonNull.java", JSON_NULL_STUB),
            ("com/google/gson/JsonObject.java", JSON_OBJECT_STUB),
            ("com/google/gson/JsonPrimitive.java", JSON_PRIMITIVE_STUB),
            ("demo/Probe.java", PROBE),
        ],
    )
}

fn expected(method: &str) -> &'static str {
    EXPECTED
        .lines()
        .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no expected line for {method}"))
}

fn run(fixture: &javac_fixture::Fixture, method: &str) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, method);
    (text, host.unknown_host_calls())
}

/// Every case answers as Java 25 does, with nothing counted.
#[test]
fn entries_and_entry_views_answer_as_java_25() {
    let fixture = fixture();
    for method in [
        "storedEntries",
        "wrappedEntries",
        "viewsInCollections",
        "identityEntries",
        "removeAfterWalk",
        "equalJsonKeys",
        "gsonHashKeys",
    ] {
        let (text, unknown) = run(&fixture, method);
        assert_eq!(text, expected(method), "{method}: {unknown:?}");
        assert!(unknown.is_empty(), "{method}: {unknown:?}");
    }
}

/// Two readings of one mapping of a map whose entry class this model does not know are one key
/// with two origins: `==` between them is counted, never answered (#221 review B4).
#[test]
fn readings_of_an_unknown_maps_entry_are_counted() {
    let fixture = fixture();
    let (_, unknown) = run(&fixture, "unknownReadings");
    assert!(
        unknown.iter().any(|(call, _)| call.contains("if_acmp")),
        "{unknown:?}"
    );
}

/// Gson's `LinkedTreeMap.remove` unlinks the node and a later `put` makes a new one (Java 25:
/// `falsetruefalse`). Any write to the members map is a change (its row's generation moves, a
/// removal through Gson's own paths included), and this model cannot tell a replaced node from
/// a kept one: `==` between the reading before it and each reading after is counted, and the
/// readings after it are one object (#221 re-check 3, B6). All three were the old entry,
/// silently.
#[test]
fn readings_across_a_gson_remove_and_re_put_are_counted() {
    let fixture = fixture();
    let (_, unknown) = run(&fixture, "gsonRemoveReadd");
    assert_eq!(
        unknown,
        vec![(
            "java/lang/Object#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z".to_owned(),
            2
        )],
        "{unknown:?}"
    );
}

/// A Gson entry read before its key was removed and put again keeps the state it read: it is
/// never rewritten by a later reading (#221 re-check 4, B8). Whether it was removed (Java keeps
/// the removed node's own value, `1`) or kept with a new value cannot be told after a write by
/// Gson's own paths, so its value and the `HashSet` of both entries are counted, never the
/// re-added node's `5`.
///
/// A node replaced in place before it is removed (Java: the removed node holds the replacement)
/// while the first value object is re-added holds exactly what the reading saw, so the reading
/// answers only while its map's row is untouched since: after any write it is counted (#221
/// review 5, B8: `x01`, `d02`).
#[test]
fn an_entry_read_before_a_gson_remove_keeps_its_own_state() {
    let fixture = fixture();
    let (text, unknown) = run(&fixture, "gsonRemovedEntry");
    assert!(!text.starts_with("5|"), "{text}");
    assert!(
        unknown
            .iter()
            .any(|(call, _)| call.starts_with("java/util/Map$Entry#getValue")),
        "{unknown:?}"
    );
    // Each reading after the write counts the read that departs from Java.
    let get_value = "java/util/Map$Entry#getValue()Ljava/lang/Object;";
    for (method, calls) in [
        ("x01ReplaceRemoveReadd", &[get_value][..]),
        (
            "d02ReplaceRemoveReadd",
            &[
                get_value,
                "java/util/Map$Entry#equals(Ljava/lang/Object;)Z",
                "java/util/Map$Entry#hashCode()I",
            ][..],
        ),
    ] {
        let (text, unknown) = run(&fixture, method);
        assert!(
            !text.starts_with("1|"),
            "{method}: the re-added value, {text}"
        );
        for call in calls {
            assert!(
                unknown.iter().any(|(counted, _)| counted == call),
                "{method}: {call} in {unknown:?} (Java {}, here {text})",
                expected(method)
            );
        }
    }
}

/// An equal key whose hash is unknown is found only where Java's hashes surely agree (the same
/// object, or a kind whose equal values always hash alike). A list, entry or map key holding a
/// Gson element beside an identity-hashed member is equal to another with `1.0` for `1` and
/// hashes differently in Java (two keys): answered as Java or counted, never merged silently
/// (#221 review 5, B9: `h17`, `c02`, `c03`, `c08`).
#[test]
fn equal_keys_with_unknown_hashes_are_never_merged_silently() {
    let fixture = fixture();
    // Each counts the lookup that could not be placed: the set's `add`/`contains`, the map's
    // `put`.
    let add = "java/util/Set#add(Ljava/lang/Object;)Z";
    for (method, calls) in [
        (
            "h17ListKeys",
            &[add, "java/util/Set#contains(Ljava/lang/Object;)Z"][..],
        ),
        ("c02ArrayListKeys", &[add][..]),
        ("c03EntryKeys", &[add][..]),
        (
            "c08LinkedHashMap",
            &["java/util/Map#put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"][..],
        ),
    ] {
        let (text, unknown) = run(&fixture, method);
        for call in calls {
            assert!(
                unknown.iter().any(|(counted, _)| counted == call),
                "{method}: {call} in {unknown:?} (Java {}, here {text})",
                expected(method)
            );
        }
    }
}
