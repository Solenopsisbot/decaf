package demo;

import java.util.*;
import java.util.function.Predicate;

public class Probe {
  static final StringBuilder log = new StringBuilder();

  /// Equal by id, hashed by h; logs each equals as e<id>:<other id>.
  static final class K {
    final int id, h;
    K(int id, int h) { this.id = id; this.h = h; }
    @Override public boolean equals(Object o) {
      log.append('e').append(id).append(':').append(o instanceof K k ? String.valueOf(k.id) : String.valueOf(o)).append(' ');
      return o instanceof K k && k.id == id;
    }
    @Override public int hashCode() { return h; }
    @Override public String toString() { return "K" + id; }
  }

  static String it(Iterable<?> c) {
    StringBuilder sb = new StringBuilder("[");
    for (Object o : c) sb.append(o).append(',');
    return sb.append(']').toString();
  }

  static String mapText(Map<?, ?> m) {
    StringBuilder sb = new StringBuilder("{");
    for (Map.Entry<?, ?> e : m.entrySet()) sb.append(e.getKey()).append('=').append(e.getValue()).append(',');
    return sb.append('}').toString();
  }

  static Map<String, Integer> strings(Map<String, Integer> m, int n) {
    for (int i = 0; i < n; i++) m.put("k" + i, i);
    return m;
  }

  static String thrown(Throwable t) {
    return "threw:" + t.getClass().getName() + ":" + t.getMessage();
  }

  /// Run `body`; a Throwable becomes its class and message.
  interface Body { String run() throws Throwable; }
  static String run(Body body) {
    log.setLength(0);
    try { return body.run(); } catch (Throwable t) { return thrown(t) + "|log=" + log; }
  }

  static String some(Map<String, Integer> m) {
    Set<String> view = m.keySet();
    boolean r = view.removeIf(k -> { log.append(k).append(' '); return m.get(k) % 2 == 1; });
    return r + "|" + it(view) + "|" + mapText(m) + "|" + view.size() + "," + m.size() + "|log=" + log;
  }

  public static String rows() {
    List<String> rows = new ArrayList<>();
    // Plain removal, results, remaining order, predicate order.
    rows.add("hm_some\t" + run(() -> some(strings(new HashMap<>(), 10))));
    rows.add("hm_some_resized\t" + run(() -> some(strings(new HashMap<>(), 40))));
    rows.add("hm_all\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 6);
      boolean r = m.keySet().removeIf(k -> { log.append(k).append(' '); return true; });
      return r + "|" + mapText(m) + "|" + m.size() + "|" + m.isEmpty() + "|log=" + log;
    }));
    rows.add("hm_none\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 6);
      boolean r = m.keySet().removeIf(k -> { log.append(k).append(' '); return false; });
      return r + "|" + mapText(m) + "|log=" + log;
    }));
    rows.add("hm_empty\t" + run(() -> {
      Map<String, Integer> m = new HashMap<>();
      boolean r = m.keySet().removeIf(k -> { log.append(k).append(' '); return true; });
      return r + "|" + mapText(m) + "|log=" + log;
    }));
    // Null predicate, with and without entries.
    rows.add("hm_null_pred\t" + run(() -> String.valueOf(strings(new HashMap<>(), 3).keySet().removeIf(null))));
    rows.add("hm_null_pred_empty\t" + run(() -> String.valueOf(new HashMap<String, Integer>().keySet().removeIf(null))));
    rows.add("lhm_null_pred\t" + run(() -> String.valueOf(strings(new LinkedHashMap<>(), 3).keySet().removeIf(null))));
    // A throwing predicate: what it removed before stays removed.
    rows.add("hm_throw\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 10);
      int[] n = {0};
      try {
        m.keySet().removeIf(k -> {
          log.append(k).append(' ');
          if (++n[0] == 4) throw new IllegalStateException("boom " + k);
          return m.get(k) % 2 == 0;
        });
        return "no throw";
      } catch (IllegalStateException e) {
        return thrown(e) + "|" + mapText(m) + "|" + m.size() + "|log=" + log;
      }
    }));
    // The null key and Integer keys.
    rows.add("hm_null_key\t" + run(() -> {
      Map<String, Integer> m = new HashMap<>();
      m.put(null, 1); m.put("a", 2); m.put("b", 3);
      boolean r = m.keySet().removeIf(k -> { log.append(k).append(' '); return k == null || k.equals("b"); });
      return r + "|" + mapText(m) + "|" + m.containsKey(null) + "|log=" + log;
    }));
    rows.add("hm_int_keys\t" + run(() -> {
      Map<Integer, String> m = new HashMap<>();
      for (int i = 0; i < 30; i += 1) m.put(i * 7 - 5, "v" + i);
      boolean r = m.keySet().removeIf(k -> { log.append(k).append(' '); return k % 3 == 0; });
      return r + "|" + mapText(m) + "|log=" + log;
    }));
    // The view is live while the predicate runs; two views see one map.
    rows.add("hm_view_live\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 8);
      Set<String> view = m.keySet();
      Set<String> other = m.keySet();
      StringBuilder sizes = new StringBuilder();
      boolean r = view.removeIf(k -> { sizes.append(view.size()).append('/').append(m.size()).append('/').append(view.contains(k)).append(' '); return m.get(k) < 4; });
      return r + "|" + sizes + "|" + it(other) + "|" + other.size() + "|" + mapText(m);
    }));
    // The predicate reads the map and iterates it.
    rows.add("hm_pred_reads\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 6);
      boolean r = m.keySet().removeIf(k -> {
        int n = 0;
        for (String x : m.keySet()) n++;
        log.append(k).append(':').append(m.get(k)).append(':').append(m.containsKey(k)).append(':').append(n).append(' ');
        return m.get(k) >= 3;
      });
      return r + "|" + mapText(m) + "|log=" + log;
    }));
    // A predicate that changes the map.
    rows.add("hm_pred_put_new\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 5);
      try {
        m.keySet().removeIf(k -> { log.append(k).append(' '); m.put("zz", 9); return false; });
        return "no throw";
      } catch (ConcurrentModificationException e) {
        return thrown(e) + "|" + mapText(m) + "|log=" + log;
      }
    }));
    rows.add("hm_pred_put_new_removing\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 5);
      try {
        m.keySet().removeIf(k -> { log.append(k).append(' '); m.put("zz", 9); return true; });
        return "no throw";
      } catch (ConcurrentModificationException e) {
        return thrown(e) + "|" + mapText(m) + "|log=" + log;
      }
    }));
    rows.add("hm_pred_remove_other\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 5);
      try {
        boolean r = m.keySet().removeIf(k -> { log.append(k).append(' '); m.remove("k4"); return false; });
        return "no throw" + r + mapText(m);
      } catch (ConcurrentModificationException e) {
        return thrown(e) + "|" + mapText(m) + "|log=" + log;
      }
    }));
    rows.add("hm_pred_removes_self\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 5);
      try {
        m.keySet().removeIf(k -> { log.append(k).append(' '); m.remove(k); return true; });
        return "no throw";
      } catch (ConcurrentModificationException e) {
        return thrown(e) + "|" + mapText(m) + "|log=" + log;
      }
    }));
    // A change at the last element: `hasNext()` is false, so only a removal there throws.
    rows.add("hm_pred_put_at_last\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 5);
      boolean r = m.keySet().removeIf(k -> { log.append(k).append(' '); if (k.equals("k4")) m.put("zz", 9); return false; });
      return r + "|" + mapText(m) + "|log=" + log;
    }));
    rows.add("hm_pred_put_at_last_removing\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 5);
      try {
        m.keySet().removeIf(k -> { log.append(k).append(' '); if (k.equals("k4")) m.put("zz", 9); return k.equals("k4"); });
        return "no throw";
      } catch (ConcurrentModificationException e) {
        return thrown(e) + "|" + mapText(m) + "|log=" + log;
      }
    }));
    rows.add("hm_pred_replaces_value\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 6);
      boolean r = m.keySet().removeIf(k -> { log.append(k).append(' '); m.put("k0", 99); return m.get(k) % 2 == 0; });
      return r + "|" + mapText(m) + "|log=" + log;
    }));
    // The map keeps working after removeIf: lookups, re-insertion, order.
    rows.add("hm_after\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 12);
      m.keySet().removeIf(k -> m.get(k) % 3 == 0);
      m.put("k3", 33); m.put("new", 1);
      return mapText(m) + "|" + m.get("k1") + "|" + m.get("k3") + "|" + m.containsKey("k6") + "|" + m.remove("k4") + "|" + m.size() + "|" + m.values();
    }));
    // A loop over the map after a removal, and a loop broken by one.
    rows.add("hm_loop_after\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 6);
      Iterator<String> iter = m.keySet().iterator();
      iter.next();
      m.keySet().removeIf(k -> k.equals("k2"));
      try { iter.next(); return "no throw"; } catch (ConcurrentModificationException e) { return thrown(e); }
    }));
    // Nothing removed: the map's modCount did not move.
    rows.add("hm_loop_none\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 6);
      Iterator<String> iter = m.keySet().iterator();
      iter.next();
      boolean r = m.keySet().removeIf(k -> false);
      return r + "|" + iter.next() + "|" + iter.next();
    }));
    // Colliding jar keys: the iterator's remove re-walks the bin.
    rows.add("hm_jar_keys\t" + run(() -> {
      Map<K, String> m = new HashMap<>();
      for (int i = 0; i < 8; i++) m.put(new K(i, i % 3), "v" + i);
      log.setLength(0);
      boolean r = m.keySet().removeIf(k -> k.id % 4 == 0);
      return r + it(m.keySet()) + "|" + mapText(m) + "|" + log;
    }));
    // LinkedHashMap, insertion order.
    rows.add("lhm_some\t" + run(() -> some(strings(new LinkedHashMap<>(), 10))));
    rows.add("lhm_jar_keys\t" + run(() -> {
      Map<K, String> m = new LinkedHashMap<>();
      for (int i = 0; i < 6; i++) m.put(new K(i, 1), "v" + i);
      log.setLength(0);
      boolean r = m.keySet().removeIf(k -> k.id % 2 == 1);
      return r + it(m.keySet()) + "|" + mapText(m) + "|" + log;
    }));
    rows.add("lhm_pred_put_new\t" + run(() -> {
      Map<String, Integer> m = strings(new LinkedHashMap<>(), 5);
      try {
        m.keySet().removeIf(k -> { log.append(k).append(' '); m.put("zz", 9); return false; });
        return "no throw";
      } catch (ConcurrentModificationException e) {
        return thrown(e) + "|" + mapText(m) + "|log=" + log;
      }
    }));
    // The map's other views follow.
    rows.add("hm_values_entries\t" + run(() -> {
      Map<String, Integer> m = strings(new HashMap<>(), 8);
      Collection<Integer> values = m.values();
      Set<Map.Entry<String, Integer>> entries = m.entrySet();
      m.keySet().removeIf(k -> m.get(k) % 2 == 0);
      return values + "|" + entries + "|" + values.size() + "," + entries.size();
    }));
    StringBuilder out = new StringBuilder();
    for (String row : rows) {
      if (out.length() > 0) out.append('\n');
      out.append(row);
    }
    return out.toString();
  }

  public static void main(String[] args) { System.out.print(rows()); }
}
