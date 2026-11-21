package demo;
import java.util.*;
import java.util.function.*;
import java.util.stream.*;
public class Probe {
  static final IdentityHashMap<String,Integer> shared = new IdentityHashMap<>();
  static String result(String s) { return s; }
  static void helper(IdentityHashMap<String,Integer> m, int v) { m.put(new String("x"), v); }
  static void computeHelper(IdentityHashMap<String,Integer> m, int v) { m.computeIfAbsent(new String("x"), k -> v); }
  static class A { static void add(IdentityHashMap<String,Integer> m) { m.put(new String("x"), 1); } }
  static class B { static void add(IdentityHashMap<String,Integer> m) { m.put(new String("x"), 2); } }
  static void rec(IdentityHashMap<String,Integer> m, int n) { m.put(new String("x"), n); if (n > 0) rec(m,n-1); }
  static void overloaded(IdentityHashMap<String,Integer> m, int v) { m.put(new String("x"), v); }
  static void overloaded(IdentityHashMap<String,Integer> m, long v) { m.put(new String("x"), (int)v); }
  static String fieldCase() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); String[] a = new String[1]; String x = new String("x"); m.put(x,1); a[0]=x; m.put(a[0],2); return m.size()+":"+m.get(x)+":"+m.get(a[0]); }
  static String listCase() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); List<String> l = new ArrayList<>(); String x = new String("x"); m.put(x,1); l.add(x); m.put(l.get(0),2); return m.size()+":"+m.get(x)+":"+m.get(l.get(0)); }
  static String identityCmpHelper() { String a = helperString(); String b = helperString(); return String.valueOf(a == b); }
  static String helperString() { return new String("x"); }
  static String identityCmpRec(int n) { String a = new String("x"); if (n == 0) return String.valueOf(a == identityCmpRecString()); return identityCmpRec(n-1); }
  static String identityCmpRecString() { return new String("x"); }
  static String lambdaCase() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); Consumer<Integer> c = v -> m.put(new String("x"),v); c.accept(1); c.accept(2); return m.size()+":"+m.getOrDefault("x",-1); }
  static IdentityHashMap<String,Integer> initMap = init();
  static IdentityHashMap<String,Integer> init() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); helper(m,1); helper(m,2); return m; }
  static String sameClassCalls() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); helper(m,1); helper(m,2); return m.size()+":"+m.getOrDefault("x",-1); }
  static String computeCalls() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); computeHelper(m,1); computeHelper(m,2); return m.size()+":"+m.getOrDefault("x",-1); }
  static String classCalls() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); A.add(m); B.add(m); return String.valueOf(m.size()); }
  static String recursion() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); rec(m,1); return m.size()+":"+m.getOrDefault("x",-1); }
  static String overloadCalls() { IdentityHashMap<String,Integer> m = new IdentityHashMap<>(); overloaded(m,1); overloaded(m,2L); return String.valueOf(m.size()); }
  static String staticInit() { return initMap.size()+":"+initMap.getOrDefault("x",-1); }
  static String recCmp(int n) { String a = new String("x"); if (n > 0) return recCmp(n-1); String b = new String("x"); return String.valueOf(a == b); }
  static String lambdaIdentityCmp() { Supplier<String> s = () -> new String("x"); String a=s.get(), b=s.get(); return String.valueOf(a==b); }
  static String loopAlloc() {
    IdentityHashMap<String,Integer> m = new IdentityHashMap<>();
    for (int i = 0; i < 2; i++) {
      m.put(new String("x"), i);
    }
    return m.size() + ":" + m.getOrDefault("x", -1);
  }
  static String stringValueOf() {
    String a = new String("x");
    IdentityHashMap<String,Integer> m = new IdentityHashMap<>();
    m.put(a, 1);
    return String.valueOf(m.get(String.valueOf(a)));
  }
  static String wholeSubstring() {
    String a = new String("x");
    String b = a.substring(0);
    IdentityHashMap<String,Integer> m = new IdentityHashMap<>();
    m.put(a, 1);
    return (a == b) + ":" + m.get(b);
  }
  static String callbackMutation() {
    String a = new String("x");
    String b = new String("x");
    IdentityHashMap<String,Integer> m = new IdentityHashMap<>();
    m.put("z", 1);
    m.put(a, 2);
    Integer n = m.computeIfAbsent(b, k -> {
      m.remove("z");
      return 3;
    });
    return n + ":" + m.get(a) + ":" + m.get(b) + ":" + m.size();
  }
  static String computeValueOverwrite() {
    String v1 = new String("v");
    IdentityHashMap<String,String> m = new IdentityHashMap<>();
    m.put("k", v1);
    m.compute("k", (k, old) -> new String("v"));
    return String.valueOf(m.remove("k", v1));
  }
  static class InternA {
    static final IdentityHashMap<String,Integer> M = new IdentityHashMap<>();
    static String S;
    static String make() {
      String s = new StringBuilder("xy").append("zq").toString();
      boolean self = (s.intern() == s);
      S = s;
      M.put(s, 1);
      return "A intern self=" + self;
    }
  }
  static class InternB {
    static String lookup() {
      Integer v = InternA.M.get("xyzq");
      boolean same = (InternA.S == "xyzq");
      return "B lookup=" + v + " same=" + same;
    }
  }
  static String dynamicIntern() {
    return InternA.make() + "|" + InternB.lookup();
  }
  static String integerAbove127Eq() {
    return String.valueOf(Integer.valueOf(200) == Integer.valueOf(200));
  }
  static String integerAbove127Map() {
    IdentityHashMap<Integer, String> m = new IdentityHashMap<>();
    m.put(Integer.valueOf(200), "a");
    m.put(Integer.valueOf(200), "b");
    return m.size() + ":" + m.get(Integer.valueOf(200));
  }
  static String entrySetValue() {
    String k = new String("k"), old = new String("v"), fresh = new String("v");
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    m.entrySet().iterator().next().setValue(fresh);
    return m.remove(k, old) + ":" + m.get(k);
  }
  static String entrySetValueOpaque() {
    String k = new String("k"), old = new String("v"), fresh = result(new String("v"));
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    m.entrySet().iterator().next().setValue(fresh);
    return m.remove(k, old) + ":" + m.get(k);
  }
  @SuppressWarnings("unchecked")
  static String entryToArraySetValue() {
    String k = new String("k"), old = new String("v"), fresh = new String("v");
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    ((Map.Entry<String, String>) m.entrySet().toArray()[0]).setValue(fresh);
    return m.remove(k, old) + ":" + m.get(k);
  }
  static String replaceAllValue() {
    String k = new String("k"), old = new String("v"), fresh = new String("v");
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    m.replaceAll((kk, vv) -> fresh);
    return m.remove(k, old) + ":" + m.get(k);
  }
  static String entryIterRemove() {
    String k = new String("k"), old = new String("v");
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    Iterator<Map.Entry<String, String>> it = m.entrySet().iterator();
    it.next();
    it.remove();
    return m.size() + ":" + m.get(k);
  }
  @SuppressWarnings("unchecked")
  static String unmodEntryToArrayFn() {
    String k = new String("k"), old = new String("v"), fresh = new String("v");
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    Map<String, String> u = Collections.unmodifiableMap(m);
    Map.Entry<String, String>[] arr = u.entrySet().toArray(Map.Entry[]::new);
    String threw = "no";
    try {
      arr[0].setValue(fresh);
    } catch (UnsupportedOperationException e) {
      threw = "uoe";
    }
    return threw + ":" + m.remove(k, old) + ":" + m.get(k);
  }
  @SuppressWarnings("unchecked")
  static String unmodEntryToArrayTyped() {
    String k = new String("k"), old = new String("v"), fresh = new String("v");
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    Map<String, String> u = Collections.unmodifiableMap(m);
    Map.Entry<String, String>[] arr = u.entrySet().toArray(new Map.Entry[0]);
    String threw = "no";
    try {
      arr[0].setValue(fresh);
    } catch (UnsupportedOperationException e) {
      threw = "uoe";
    }
    return threw + ":" + m.remove(k, old) + ":" + m.get(k);
  }
  @SuppressWarnings("unchecked")
  static String unmodSyncEntryToArrayFn() {
    String k = new String("k"), old = new String("v"), fresh = new String("v");
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    Map<String, String> u = Collections.unmodifiableMap(Collections.synchronizedMap(m));
    Map.Entry<String, String>[] arr = u.entrySet().toArray(Map.Entry[]::new);
    String threw = "no";
    try {
      arr[0].setValue(fresh);
    } catch (UnsupportedOperationException e) {
      threw = "uoe";
    }
    return threw + ":" + m.remove(k, old) + ":" + m.get(k);
  }
  @SuppressWarnings("unchecked")
  static String unmodUnmodEntryToArrayFn() {
    String k = new String("k"), old = new String("v"), fresh = new String("v");
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(k, old);
    Map<String, String> u = Collections.unmodifiableMap(Collections.unmodifiableMap(m));
    Map.Entry<String, String>[] arr = u.entrySet().toArray(Map.Entry[]::new);
    String threw = "no";
    try {
      arr[0].setValue(fresh);
    } catch (UnsupportedOperationException e) {
      threw = "uoe";
    }
    return threw + ":" + m.remove(k, old) + ":" + m.get(k);
  }
  static String reflectPutThenLookup() {
    try {
      String k = new String("k"), old = new String("v"), fresh = new String("v");
      IdentityHashMap<String, String> m = new IdentityHashMap<>();
      m.put(k, old);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, k, fresh);
      return m.remove(k, old) + ":" + m.get(k);
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutViaSyncView() {
    try {
      String k = new String("k"), old = new String("v"), fresh = new String("v");
      IdentityHashMap<String, String> m = new IdentityHashMap<>();
      m.put(k, old);
      Map<String, String> s = Collections.synchronizedMap(m);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(s, k, fresh);
      return m.remove(k, old) + ":" + m.get(k);
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectSetValueOnEntry() {
    try {
      String k = new String("k"), old = new String("v"), fresh = new String("v");
      IdentityHashMap<String, String> m = new IdentityHashMap<>();
      m.put(k, old);
      Map.Entry<String, String> e = m.entrySet().iterator().next();
      java.lang.reflect.Method setValue = Map.Entry.class.getMethod("setValue", Object.class);
      setValue.invoke(e, fresh);
      return m.remove(k, old) + ":" + m.get(k);
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutOnEmptyMap() {
    try {
      String k = new String("k"), v = new String("v");
      IdentityHashMap<String, String> m = new IdentityHashMap<>();
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, k, v);
      return m.size() + ":" + m.get(k);
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutThenReadViaUnmod() {
    try {
      String k = new String("k"), old = new String("v"), fresh = new String("v");
      IdentityHashMap<String, String> m = new IdentityHashMap<>();
      m.put(k, old);
      Map<String, String> u = Collections.unmodifiableMap(m);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, k, fresh);
      return u.get(k) + ":" + u.size();
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static final IdentityHashMap<String, Integer> TAINT_M = new IdentityHashMap<>();
  static final String TAINT_K = new String("k");
  static void taintMutate() { TAINT_M.put(TAINT_K, 2); }
  static String staticReach() {
    try {
      TAINT_M.clear();
      TAINT_M.put(TAINT_K, 1);
      java.lang.reflect.Method f = Probe.class.getDeclaredMethod("taintMutate");
      f.invoke(null);
      return String.valueOf(TAINT_M.get(TAINT_K));
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static final class Box { Object next; }
  static void deepMutate(Box b) {
    Object x = b;
    for (int i = 0; i < 601; i++) x = ((Box) x).next;
    @SuppressWarnings("unchecked")
    IdentityHashMap<String, Integer> m = (IdentityHashMap<String, Integer>) x;
    m.put(TAINT_K, 2);
  }
  static String deepReach() {
    try {
      TAINT_M.clear();
      TAINT_M.put(TAINT_K, 1);
      Box root = new Box(), p = root;
      for (int i = 0; i < 600; i++) { p.next = new Box(); p = (Box) p.next; }
      p.next = TAINT_M;
      java.lang.reflect.Method f = Probe.class.getDeclaredMethod("deepMutate", Box.class);
      f.invoke(null, root);
      return String.valueOf(TAINT_M.get(TAINT_K));
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static final class MutatingKey {
    final IdentityHashMap<String, Integer> target;
    final String key;
    MutatingKey(IdentityHashMap<String, Integer> target, String key) {
      this.target = target;
      this.key = key;
    }
    @Override
    public boolean equals(Object other) {
      target.put(key, 2);
      return false;
    }
  }
  static String mutatingKeyDeepEquals() {
    IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
    m.put("a", 1);
    boolean eq = Objects.deepEquals(
        new Object[] { new MutatingKey(m, "a") },
        new Object[] { "other" });
    return eq + ":" + m.get("a");
  }
  static String reflectStaticUncachedTaint() {
    try {
      TAINT_M.clear();
      TAINT_M.put("a", 1);
      java.lang.reflect.Method gc = System.class.getMethod("gc");
      gc.invoke(null);
      return String.valueOf(TAINT_M.get("a"));
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectDeepChain600Taint() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Box root = new Box(), p = root;
      for (int i = 0; i < 600; i++) { p.next = new Box(); p = (Box) p.next; }
      p.next = m;
      java.lang.reflect.Method hc = System.class.getMethod("identityHashCode", Object.class);
      hc.invoke(null, root);
      return String.valueOf(m.get("a"));
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static final class EqAtom {
    final String label;
    EqAtom(String label) { this.label = label; }
    @Override public boolean equals(Object o) { return o instanceof EqAtom a && label.equals(a.label); }
    @Override public int hashCode() { return label.hashCode(); }
  }
  static String idMapOneEntryDistinctEq() {
    IdentityHashMap<String, String> a = new IdentityHashMap<>();
    IdentityHashMap<String, String> b = new IdentityHashMap<>();
    a.put(new String("k"), new String("v"));
    b.put(new String("k"), new String("v"));
    return a.equals(b) + ":" + b.equals(a);
  }
  static String idMapOneEntrySameKDistinctVEq() {
    IdentityHashMap<String, String> a = new IdentityHashMap<>();
    IdentityHashMap<String, String> b = new IdentityHashMap<>();
    String k = new String("k");
    a.put(k, new String("v"));
    b.put(k, new String("v"));
    return String.valueOf(a.equals(b));
  }
  static String idMapOneEntrySameKVEq() {
    IdentityHashMap<String, String> a = new IdentityHashMap<>();
    IdentityHashMap<String, String> b = new IdentityHashMap<>();
    String k = new String("k"), v = new String("v");
    a.put(k, v);
    b.put(k, v);
    IdentityHashMap<String, String> e1 = new IdentityHashMap<>();
    IdentityHashMap<String, String> e2 = new IdentityHashMap<>();
    IdentityHashMap<String, String> n = new IdentityHashMap<>();
    n.put(null, null);
    return a.equals(b) + ":" + e1.equals(e2) + ":" + e1.hashCode() + ":" + n.hashCode();
  }
  static String idMapOneEntryHashCode() {
    IdentityHashMap<String, String> a = new IdentityHashMap<>();
    IdentityHashMap<String, String> b = new IdentityHashMap<>();
    a.put(new String("k"), new String("v"));
    b.put(new String("k"), new String("v"));
    return a.equals(b) + ":" + b.equals(a) + ":" + (a.hashCode() == b.hashCode());
  }
  static String idMapVsHashMapJarKeys() {
    EqAtom k1 = new EqAtom("k"), k2 = new EqAtom("k");
    EqAtom v1 = new EqAtom("v"), v2 = new EqAtom("v");
    IdentityHashMap<EqAtom, EqAtom> id = new IdentityHashMap<>();
    id.put(k1, v1);
    HashMap<EqAtom, EqAtom> sameKV = new HashMap<>(); sameKV.put(k1, v1);
    HashMap<EqAtom, EqAtom> sameKDiffV = new HashMap<>(); sameKDiffV.put(k1, v2);
    HashMap<EqAtom, EqAtom> diffKSameV = new HashMap<>(); diffKSameV.put(k2, v1);
    HashMap<EqAtom, EqAtom> diffKDiffV = new HashMap<>(); diffKDiffV.put(k2, v2);
    IdentityHashMap<EqAtom, EqAtom> idEmpty = new IdentityHashMap<>();
    HashMap<EqAtom, EqAtom> hmEmpty = new HashMap<>();
    return id.equals(sameKV) + ":" + sameKV.equals(id) + "|"
        + id.equals(sameKDiffV) + ":" + sameKDiffV.equals(id) + "|"
        + id.equals(diffKSameV) + ":" + diffKSameV.equals(id) + "|"
        + id.equals(diffKDiffV) + ":" + diffKDiffV.equals(id) + "|"
        + idEmpty.equals(hmEmpty) + ":" + hmEmpty.equals(idEmpty);
  }
  static String idMapVsHashMapStringKeys() {
    IdentityHashMap<String, String> id = new IdentityHashMap<>();
    id.put(new String("k"), new String("v"));
    HashMap<String, String> hm = new HashMap<>();
    hm.put(new String("k"), new String("v"));
    return id.equals(hm) + ":" + hm.equals(id);
  }
  static String idMapKeySetEntrySetEq() {
    String k1 = new String("k"), k2 = new String("k");
    String v1 = new String("v"), v2 = new String("v");
    IdentityHashMap<String, String> m1 = new IdentityHashMap<>(); m1.put(k1, v1);
    IdentityHashMap<String, String> m2 = new IdentityHashMap<>(); m2.put(k2, v2);
    IdentityHashMap<String, String> m3 = new IdentityHashMap<>(); m3.put(k1, v2);
    return m1.keySet().equals(m2.keySet()) + ":"
        + m1.keySet().equals(m3.keySet()) + ":"
        + m1.entrySet().equals(m2.entrySet()) + ":"
        + m1.entrySet().equals(m3.entrySet());
  }
  static String idMapKeySetHashCode() {
    IdentityHashMap<String, String> m = new IdentityHashMap<>();
    m.put(new String("k"), new String("v"));
    return String.valueOf(m.keySet().hashCode() != 0 || m.keySet().hashCode() == 0);
  }
  static String reflectPutThenIterator() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Iterator<String> it = m.keySet().iterator();
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "b", 2);
      try {
        int n = 0;
        while (it.hasNext()) { it.next(); n++; }
        return "n=" + n;
      } catch (ConcurrentModificationException e) {
        return "CME";
      }
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutThenStream() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Stream<String> st = m.keySet().stream();
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "b", 2);
      return "n=" + st.count();
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutThenFilteredStream() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Stream<String> st = m.keySet().stream().filter(s -> s != null);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "b", 2);
      return "n=" + st.toList().size();
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutThenSpliterator() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Spliterator<String> sp = Spliterators.spliterator(m.keySet(), 0);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "b", 2);
      int[] c = { 0 };
      sp.forEachRemaining(x -> c[0]++);
      return "n=" + c[0];
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutThenStreamSupport() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Spliterator<String> sp = Spliterators.spliterator(m.keySet(), 0);
      Stream<String> st = StreamSupport.stream(sp, false).map(s -> s + "!");
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "b", 2);
      return "n=" + st.count();
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutThenIterSpliterator() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Spliterator<String> sp = Spliterators.spliterator(m.keySet().iterator(), 1, 0);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "b", 2);
      try {
        int[] c = { 0 };
        sp.forEachRemaining(x -> c[0]++);
        return "n=" + c[0];
      } catch (ConcurrentModificationException e) {
        return "CME";
      }
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String forEachSavedEntryToString() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("k", 1);
      @SuppressWarnings("unchecked")
      Map.Entry<String, Integer>[] saved = new Map.Entry[1];
      m.entrySet().forEach(e -> saved[0] = e);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "k", 2);
      return saved[0].toString();
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String forEachSavedEntryEquals() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("k", 1);
      @SuppressWarnings("unchecked")
      Map.Entry<String, Integer>[] saved = new Map.Entry[1];
      m.entrySet().forEach(e -> saved[0] = e);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "k", 2);
      return saved[0].equals(Map.entry("k", 1))
          + ":" + saved[0].equals(Map.entry("k", 2))
          + ":" + Map.entry("k", 1).equals(saved[0])
          + ":" + Map.entry("k", 2).equals(saved[0]);
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String forEachSavedEntryConcat() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("k", 1);
      @SuppressWarnings("unchecked")
      Map.Entry<String, Integer>[] saved = new Map.Entry[1];
      m.entrySet().forEach(e -> saved[0] = e);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "k", 2);
      return "e=" + saved[0];
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String unmodForEachSavedEntryToString() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("k", 1);
      @SuppressWarnings("unchecked")
      Map.Entry<String, Integer>[] saved = new Map.Entry[1];
      Collections.unmodifiableMap(m).entrySet().forEach(e -> saved[0] = e);
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "k", 2);
      return saved[0].toString();
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutThenEnumeration() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Enumeration<String> en = Collections.enumeration(m.keySet());
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "b", 2);
      return String.valueOf(en.hasMoreElements());
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  static String reflectPutThenStreamIterator() {
    try {
      IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
      m.put("a", 1);
      Iterator<String> it = m.keySet().stream().iterator();
      java.lang.reflect.Method put = Map.class.getMethod("put", Object.class, Object.class);
      put.invoke(m, "b", 2);
      return String.valueOf(it.hasNext());
    } catch (Exception e) {
      return "ex:" + e.getClass().getSimpleName();
    }
  }
  public static String sameClassCallsOut(){return sameClassCalls();}
  public static String computeCallsOut(){return computeCalls();}
  public static String classCallsOut(){return classCalls();}
  public static String recursionOut(){return recursion();}
  public static String overloadCallsOut(){return overloadCalls();}
  public static String staticInitOut(){return staticInit();}
  public static String fieldCaseOut(){return fieldCase();}
  public static String listCaseOut(){return listCase();}
  public static String lambdaCaseOut(){return lambdaCase();}
  public static String identityCmpHelperOut(){return identityCmpHelper();}
  public static String lambdaIdentityCmpOut(){return lambdaIdentityCmp();}
  public static String recCmpOut(){return recCmp(1);}
  public static String loopAllocOut(){return loopAlloc();}
  public static String stringValueOfOut(){return stringValueOf();}
  public static String wholeSubstringOut(){return wholeSubstring();}
  public static String callbackMutationOut(){return callbackMutation();}
  public static String computeValueOverwriteOut(){return computeValueOverwrite();}
  public static String dynamicInternOut(){return dynamicIntern();}
  public static String integerAbove127EqOut(){return integerAbove127Eq();}
  public static String integerAbove127MapOut(){return integerAbove127Map();}
  public static String entrySetValueOut(){return entrySetValue();}
  public static String entrySetValueOpaqueOut(){return entrySetValueOpaque();}
  public static String entryToArraySetValueOut(){return entryToArraySetValue();}
  public static String replaceAllValueOut(){return replaceAllValue();}
  public static String entryIterRemoveOut(){return entryIterRemove();}
  public static String unmodEntryToArrayFnOut(){return unmodEntryToArrayFn();}
  public static String unmodEntryToArrayTypedOut(){return unmodEntryToArrayTyped();}
  public static String unmodSyncEntryToArrayFnOut(){return unmodSyncEntryToArrayFn();}
  public static String unmodUnmodEntryToArrayFnOut(){return unmodUnmodEntryToArrayFn();}
  public static String reflectPutThenLookupOut(){return reflectPutThenLookup();}
  public static String reflectPutViaSyncViewOut(){return reflectPutViaSyncView();}
  public static String reflectSetValueOnEntryOut(){return reflectSetValueOnEntry();}
  public static String reflectPutOnEmptyMapOut(){return reflectPutOnEmptyMap();}
  public static String reflectPutThenReadViaUnmodOut(){return reflectPutThenReadViaUnmod();}
  public static String staticReachOut(){return staticReach();}
  public static String deepReachOut(){return deepReach();}
  public static String mutatingKeyDeepEqualsOut(){return mutatingKeyDeepEquals();}
  public static String reflectStaticUncachedTaintOut(){return reflectStaticUncachedTaint();}
  public static String reflectDeepChain600TaintOut(){return reflectDeepChain600Taint();}
  public static String idMapOneEntryDistinctEqOut(){return idMapOneEntryDistinctEq();}
  public static String idMapOneEntrySameKDistinctVEqOut(){return idMapOneEntrySameKDistinctVEq();}
  public static String idMapOneEntrySameKVEqOut(){return idMapOneEntrySameKVEq();}
  public static String idMapOneEntryHashCodeOut(){return idMapOneEntryHashCode();}
  public static String idMapVsHashMapJarKeysOut(){return idMapVsHashMapJarKeys();}
  public static String idMapVsHashMapStringKeysOut(){return idMapVsHashMapStringKeys();}
  public static String idMapKeySetEntrySetEqOut(){return idMapKeySetEntrySetEq();}
  public static String idMapKeySetHashCodeOut(){return idMapKeySetHashCode();}
  public static String reflectPutThenIteratorOut(){return reflectPutThenIterator();}
  public static String reflectPutThenStreamOut(){return reflectPutThenStream();}
  public static String reflectPutThenFilteredStreamOut(){return reflectPutThenFilteredStream();}
  public static String reflectPutThenSpliteratorOut(){return reflectPutThenSpliterator();}
  public static String reflectPutThenStreamSupportOut(){return reflectPutThenStreamSupport();}
  public static String reflectPutThenIterSpliteratorOut(){return reflectPutThenIterSpliterator();}
  public static String forEachSavedEntryToStringOut(){return forEachSavedEntryToString();}
  public static String forEachSavedEntryEqualsOut(){return forEachSavedEntryEquals();}
  public static String forEachSavedEntryConcatOut(){return forEachSavedEntryConcat();}
  public static String unmodForEachSavedEntryToStringOut(){return unmodForEachSavedEntryToString();}
  public static String reflectPutThenEnumerationOut(){return reflectPutThenEnumeration();}
  public static String reflectPutThenStreamIteratorOut(){return reflectPutThenStreamIterator();}
}
