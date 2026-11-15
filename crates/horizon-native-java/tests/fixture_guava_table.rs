//! Guava 33.6.0 Table semantics, checked against a Java 25 run.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const TABLE_STUB: &str = r#"package com.google.common.collect;
import java.util.*;
public interface Table<R,C,V> {
  V put(R r, C c, V v); V get(Object r, Object c); boolean contains(Object r,Object c);
  boolean containsRow(Object r); boolean containsColumn(Object c); boolean containsValue(Object v);
  V remove(Object r,Object c); Map<C,V> row(R r); Map<R,V> column(C c);
  Map<R,Map<C,V>> rowMap(); Map<C,Map<R,V>> columnMap();
  Set<Cell<R,C,V>> cellSet(); Set<R> rowKeySet(); Set<C> columnKeySet();
  Collection<V> values(); int size(); boolean isEmpty(); void clear();
}
"#;

const HASH_STUB: &str = r#"package com.google.common.collect;
public final class HashBasedTable<R,C,V> {
  public static <R,C,V> Table<R,C,V> create() { throw new AssertionError(); }
  public static <R,C,V> Table<R,C,V> create(int r, int c) { throw new AssertionError(); }
}
"#;

const TREE_STUB: &str = r#"package com.google.common.collect;
import java.util.*;
public final class TreeBasedTable<R,C,V> {
  public static <R extends Comparable<? super R>,C extends Comparable<? super C>,V> Table<R,C,V> create() { throw new AssertionError(); }
  public static <R,C,V> Table<R,C,V> create(Comparator<? super R> r, Comparator<? super C> c) { throw new AssertionError(); }
}
"#;

const IMMUTABLE_STUB: &str = r#"package com.google.common.collect;
public final class ImmutableTable<R,C,V> {
  public static <R,C,V> Table<R,C,V> of() { throw new AssertionError(); }
  public static <R,C,V> Table<R,C,V> of(R r,C c,V v) { throw new AssertionError(); }
  public static <R,C,V> Builder<R,C,V> builder() { throw new AssertionError(); }
  public static final class Builder<R,C,V> {
    public Builder<R,C,V> put(R r,C c,V v) { throw new AssertionError(); }
    public Table<R,C,V> build() { throw new AssertionError(); }
  }
}
"#;

const TABLES_STUB: &str = r#"package com.google.common.collect;
import java.util.*; import java.util.function.Supplier;
public final class Tables {
  public static <R,C,V> Table<R,C,V> newCustomTable(Map<R,Map<C,V>> m, Supplier<Map<C,V>> s) { throw new AssertionError(); }
  public static <R,C,V> Table<R,C,V> synchronizedTable(Table<R,C,V> t) { throw new AssertionError(); }
}
"#;

const CELL_STUB: &str = r#"package com.google.common.collect;
public interface Cell<R,C,V> { R getRowKey(); C getColumnKey(); V getValue(); }
"#;

const PROBE: &str = r#"package demo;
import java.util.*;
import com.google.common.collect.*;
public class Probe {
  public static String core() {
    Table<String,String,Integer> t = HashBasedTable.create(2, 3);
    t.put("b", "y", 2); t.put("a", "x", 1); t.put("b", "z", 3);
    int old = t.put("b", "y", 4);
    Map<String,Integer> row = t.row("a"); row.put("q", 7);
    Map<String,Integer> column = t.column("y"); column.put("c", 8);
    return old + "," + t.get("b", "y") + "," + t.containsRow("c") + ","
      + t.containsColumn("q") + "," + t.size() + "," + t.rowKeySet() + ","
      + t.columnKeySet() + "," + t.values() + "," + t;
  }
  public static String removeAndClear() {
    Table<String,String,Integer> t = HashBasedTable.create();
    t.put("r", "c", 1); t.put("r", "d", 2);
    int removed = t.remove("r", "c"); int before = t.size(); t.row("r").clear();
    return removed + "," + before + "," + t.isEmpty();
  }
  public static String immutable() {
    Table<String,String,Integer> t = ImmutableTable.of("r", "c", 1);
    try { t.put("r", "d", 2); return "no-throw"; }
    catch (UnsupportedOperationException e) { return t.get("r", "c") + "," + t.size(); }
  }
  public static String builderAndTree() {
    Table<String,String,Integer> i = ImmutableTable.<String,String,Integer>builder().put("r", "c", 1).build();
    Table<String,String,Integer> t = TreeBasedTable.create();
    t.put("z", "b", 2); t.put("a", "c", 3); t.put("a", "a", 4);
    return i + "|" + t.rowKeySet() + "|" + t.columnKeySet();
  }
  public static String wrappers() {
    Table<String,String,Integer> base = Tables.newCustomTable(new LinkedHashMap<>(), LinkedHashMap::new);
    Table<String,String,Integer> sync = Tables.synchronizedTable(base);
    sync.put("r", "c", 9);
    return sync.get("r", "c") + "," + base.get("r", "c") + "," + sync.equals(base);
  }
  public static String nulls() {
    Table<String,String,Integer> t = HashBasedTable.create();
    try { t.put(null, "c", 1); return "no-throw"; }
    catch (NullPointerException e) { return "npe"; }
  }
}
"#;

fn fixture() -> Fixture {
    Fixture::build(
        "guava-table",
        &[
            ("com/google/common/collect/Table.java", TABLE_STUB),
            ("com/google/common/collect/HashBasedTable.java", HASH_STUB),
            ("com/google/common/collect/TreeBasedTable.java", TREE_STUB),
            (
                "com/google/common/collect/ImmutableTable.java",
                IMMUTABLE_STUB,
            ),
            ("com/google/common/collect/Tables.java", TABLES_STUB),
            ("com/google/common/collect/Cell.java", CELL_STUB),
            ("demo/Probe.java", PROBE),
        ],
    )
}

#[test]
fn table_factories_views_and_nulls_match_guava() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "core"),
        "2,4,true,true,5,[b, a, c],[y, z, x, q],[4, 3, 1, 7, 8],{b={y=4, z=3}, a={x=1, q=7}, c={y=8}}"
    );
    assert_eq!(fixture.text_on(&mut host, "removeAndClear"), "1,1,true");
    assert_eq!(fixture.text_on(&mut host, "immutable"), "1,1");
    assert_eq!(
        fixture.text_on(&mut host, "builderAndTree"),
        "{r={c=1}}|[a, z]|[a, b, c]"
    );
    assert_eq!(fixture.text_on(&mut host, "wrappers"), "9,9,true");
    assert_eq!(fixture.text_on(&mut host, "nulls"), "npe");
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
