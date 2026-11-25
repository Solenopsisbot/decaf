//! fastutil 8.5.18 `Object2BooleanLinkedOpenHashMap.object2BooleanEntrySet().iterator()`, compiled
//! against the real jar. `java25.txt` and `java25_counted.txt` are the raw output of `demo.Probe`
//! on JDK 25 with that jar on the class path (commands in the fixture README), never the native
//! implementation's.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use std::path::PathBuf;

const ORACLE: &str = include_str!("fixtures/fastutil_sorted_entry_set/java25.txt");
const COUNTED_ORACLE: &str = include_str!("fixtures/fastutil_sorted_entry_set/java25_counted.txt");
const PROBE: &str = include_str!("fixtures/fastutil_sorted_entry_set/demo/Probe.java");

fn fastutil_jar() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::var("FASTUTIL_JAR") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join("ava/fastutil-8.5.18.jar"));
        candidates.push(home.join(
            ".local/share/PrismLauncher/libraries/it/unimi/dsi/fastutil/8.5.18/fastutil-8.5.18.jar",
        ));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn build() -> Option<javac_fixture::Fixture> {
    let Some(jar) = fastutil_jar() else {
        eprintln!("FASTUTIL_JAR is unavailable; real-fastutil fixture needs fastutil-8.5.18.jar");
        return None;
    };
    Some(javac_fixture::Fixture::build_with_classpath(
        "fastutil_sorted_entry_set",
        &[("demo/Probe.java", PROBE)],
        &jar,
    ))
}

#[test]
fn fastutil_sorted_entry_set_matches_java25() {
    let Some(fixture) = build() else {
        return;
    };
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let text = fixture.text_on(&mut host, "rows");
    for (got, want) in text.lines().zip(ORACLE.lines()) {
        assert_eq!(got, want);
    }
    assert_eq!(text, ORACLE);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

/// What stays counted, each case on a host of its own: it must count at least one call, and the
/// call named for it. A counted case is never answered with a value the oracle does not show.
#[test]
fn fastutil_sorted_entry_set_leaves_the_rest_counted() {
    let Some(fixture) = build() else {
        return;
    };
    let objects = "it/unimi/dsi/fastutil/objects/";
    let fast_iterator = format!(
        "{objects}Object2BooleanSortedMap$FastSortedEntrySet#fastIterator()L{objects}ObjectBidirectionalIterator;"
    );
    let iterator = format!(
        "{objects}Object2BooleanSortedMap$FastSortedEntrySet#iterator()L{objects}ObjectBidirectionalIterator;"
    );
    let entry_equals = format!("{objects}Object2BooleanMap$Entry#equals(Ljava/lang/Object;)Z");
    let entry_hash = format!("{objects}Object2BooleanMap$Entry#hashCode()I");
    let object_equals = String::from("java/lang/Object#equals(Ljava/lang/Object;)Z");
    let object_hash = String::from("java/lang/Object#hashCode()I");
    let objects_hash = String::from("java/util/Objects#hashCode(Ljava/lang/Object;)I");
    let if_acmp = String::from("java/lang/Object#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z");
    let map_put =
        String::from("java/util/Map#put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;");
    let tree = |class: &str| {
        format!("{objects}{class}#object2BooleanEntrySet()L{objects}ObjectSortedSet;")
    };
    let cases: Vec<(&str, &str, Vec<String>)> = vec![
        // One entry object reused by every `next()`.
        (
            "fastIteratorOrder",
            "LINKED.fastIterator-order",
            vec![fast_iterator.clone()],
        ),
        (
            "fastIteratorRetained",
            "LINKED.fastIterator-retained",
            vec![fast_iterator],
        ),
        // A map changed under a live iterator, whatever type the program gives the iterator, and
        // an entry read after the iterator's `remove()` or a `put`.
        (
            "iteratorPutDuring",
            "LINKED.iterator-put-during",
            vec!["java/util/Iterator#hasNext()Z".into()],
        ),
        (
            "iteratorRemoveDuring",
            "LINKED.iterator-remove-during",
            vec!["java/util/Iterator#hasNext()Z".into()],
        ),
        (
            "iteratorReadAfterRemove",
            "LINKED.iterator-read-after-remove",
            vec!["java/util/Map$Entry#getKey()Ljava/lang/Object;".into()],
        ),
        (
            "typedPutDuring",
            "LINKED.typed-put-during",
            vec!["java/util/Iterator#hasNext()Z".into()],
        ),
        (
            "typedRemoveThenPut",
            "LINKED.typed-remove-then-put",
            vec!["java/util/Iterator#hasNext()Z".into()],
        ),
        (
            "iteratorPutExistingDuring",
            "LINKED.iterator-put-existing-during",
            vec![],
        ),
        // fastutil's entry prints `key=>value`.
        ("entryToString", "LINKED.entry-toString", vec![]),
        // `setValue` and the backward steps of the bidirectional iterator are not modelled.
        ("entrySetValue", "LINKED.entry-setValue", vec![]),
        ("iteratorPrevious", "LINKED.iterator-previous", vec![]),
        // The kept entry set is a live view; the host's is a copy.
        ("entrySetKept", "LINKED.entry-set-kept", vec![]),
        // The same, in every way the host's copy and the live view part: a put, a remove, a
        // clear, an iterator's remove, a put and remove that restore the keys, a changed value;
        // keys the host cannot tell apart (a lone surrogate is stored as U+FFFD); a null value
        // the host stores as `false`; writes the runtime does not follow.
        (
            "keptCountAfterPut",
            "LINKED.kept-count-after-put",
            vec![iterator.clone()],
        ),
        (
            "keptCountAfterRemove",
            "LINKED.kept-count-after-remove",
            vec![iterator.clone()],
        ),
        (
            "keptCountThenClear",
            "LINKED.kept-count-then-clear",
            vec![iterator.clone()],
        ),
        (
            "keptEmptyThenPut",
            "LINKED.kept-empty-then-put",
            vec![iterator.clone()],
        ),
        (
            "keptTwoIterations",
            "LINKED.kept-two-iterations",
            vec![iterator.clone()],
        ),
        (
            "keptAfterIteratorRemove",
            "LINKED.kept-after-iterator-remove",
            vec![iterator.clone()],
        ),
        (
            "keptPutRemove",
            "LINKED.kept-put-remove",
            vec![iterator.clone()],
        ),
        (
            "keptPutExisting",
            "LINKED.kept-put-existing",
            vec![iterator.clone()],
        ),
        // A second iterator over one entry set: the host's copy hands out the same entry objects.
        (
            "keptTwoIterators",
            "LINKED.kept-two-iterators",
            vec![iterator.clone()],
        ),
        (
            "entryIdentityTwoIterators",
            "LINKED.entry-identity-two-iterators",
            vec![iterator.clone()],
        ),
        (
            "entryIdentityLoopTwice",
            "LINKED.entry-identity-loop-twice",
            vec![iterator.clone()],
        ),
        (
            "entryIdentityGenericThenFast",
            "LINKED.entry-identity-generic-then-fast",
            vec![iterator.clone()],
        ),
        (
            "entryIdentityFastThenGeneric",
            "LINKED.entry-identity-fast-then-generic",
            vec!["java/util/Set#iterator()Ljava/util/Iterator;".into()],
        ),
        // An entry the iterator handed out compared or hashed against a fastutil `BasicEntry`
        // (or any entry the host cannot prove equal as Java does), in either direction, as a
        // key of a hash collection, or against an entry whose value fastutil's `equals` casts
        // to `Boolean` and throws on: counted.
        (
            "entryEqualsBasic",
            "LINKED.entry-equals-basic",
            vec![entry_equals.clone()],
        ),
        (
            "basicEqualsEntry",
            "LINKED.basic-equals-entry",
            vec![entry_equals.clone()],
        ),
        (
            "entryEqualsBasicOtherValue",
            "LINKED.entry-equals-basic-other-value",
            vec![entry_equals.clone()],
        ),
        (
            "entryHashCodeBasic",
            "LINKED.entry-hashCode-basic",
            vec![entry_hash.clone()],
        ),
        (
            "entryListContains",
            "LINKED.entry-list-contains-basic",
            vec![object_equals.clone()],
        ),
        (
            "basicListContains",
            "LINKED.basic-list-contains-entry",
            vec![object_equals.clone()],
        ),
        (
            "entryHashSet",
            "LINKED.entry-hashset-basic",
            vec![object_hash.clone()],
        ),
        (
            "basicHashSet",
            "LINKED.basic-hashset-entry",
            vec![object_hash.clone()],
        ),
        (
            "entryHashMap",
            "LINKED.entry-hashmap-basic",
            vec![object_hash.clone()],
        ),
        (
            "basicHashMap",
            "LINKED.basic-hashmap-entry",
            vec![object_hash.clone()],
        ),
        (
            "entryIdentityFreshSet",
            "LINKED.entry-identity-fresh-set",
            vec![if_acmp.clone()],
        ),
        (
            "entryIdentityFreshSetMap",
            "LINKED.entry-identity-fresh-set-map",
            vec![map_put.clone()],
        ),
        (
            "objectEntryEqualsBasic",
            "LINKED.object-entry-equals-basic",
            vec![object_equals.clone()],
        ),
        (
            "objectEntryHashCodeBasic",
            "LINKED.object-entry-hashCode-basic",
            vec![object_hash.clone()],
        ),
        (
            "entryEqualsNonBooleanValue",
            "LINKED.entry-equals-non-boolean-value",
            vec![object_equals.clone()],
        ),
        (
            "entryEqualsNullValue",
            "LINKED.entry-equals-null-value",
            vec![object_equals.clone()],
        ),
        (
            "basicEqualsBasic",
            "LINKED.basic-equals-basic",
            vec![entry_equals.clone()],
        ),
        (
            "objectBasicEqualsEntry",
            "LINKED.object-basic-equals-entry",
            vec![object_equals.clone()],
        ),
        (
            "objectsEqualsBasicEntry",
            "LINKED.objects-equals-basic-entry",
            vec![object_equals.clone()],
        ),
        (
            "objectsEqualsEntryBasic",
            "LINKED.objects-equals-entry-basic",
            vec![object_equals.clone()],
        ),
        (
            "basicHashCode",
            "LINKED.basic-hashCode",
            vec![object_hash.clone()],
        ),
        (
            "objectsHashCodeBasic",
            "LINKED.objects-hashCode-basic",
            vec![objects_hash.clone()],
        ),
        (
            "surrogateKeys",
            "KEYS.surrogate-pair-merged",
            vec![iterator.clone()],
        ),
        (
            "fffdAndSurrogateKeys",
            "KEYS.fffd-and-surrogate",
            vec![iterator.clone()],
        ),
        (
            "surrogateRemove",
            "KEYS.surrogate-removed",
            vec![iterator.clone()],
        ),
        ("fffdKey", "KEYS.fffd-genuine", vec![iterator.clone()]),
        (
            "nullValuePut",
            "KEYS.null-value-put",
            vec![iterator.clone()],
        ),
        ("putAll", "MUTATE.put-all", vec![iterator.clone()]),
        ("wrapper", "MUTATE.wrapper", vec![iterator.clone()]),
        (
            "compute",
            "MUTATE.compute-if-absent",
            vec![iterator.clone()],
        ),
        (
            "methodRefPut",
            "MUTATE.method-ref-put",
            vec![iterator.clone()],
        ),
        // A map the host table does not mirror: a strategy, constructors with arguments, keys the
        // host compares by identity or boxes.
        ("customStrategy", "CUSTOM.iterator-strategy", vec![]),
        ("copyMapCtor", "COPY.map-ctor", vec![iterator.clone()]),
        (
            "copyFastutilCtor",
            "COPY.fastutil-ctor",
            vec![iterator.clone()],
        ),
        ("copyArraysCtor", "COPY.arrays-ctor", vec![iterator.clone()]),
        ("capacityCtor", "COPY.capacity-ctor", vec![iterator.clone()]),
        ("jarClassKeys", "KEYS.jar-class", vec![iterator.clone()]),
        ("boxedKeys", "KEYS.boxed", vec![iterator]),
        // The tree maps' natural key order.
        (
            "avlIteratorOrder",
            "AVL.iterator-order",
            vec![tree("Object2BooleanAVLTreeMap")],
        ),
        (
            "rbtIteratorOrder",
            "RBT.iterator-order",
            vec![tree("Object2BooleanRBTreeMap")],
        ),
        (
            "sortedIteratorOrder",
            "AVL.sorted-iterator-order",
            vec![tree("Object2BooleanSortedMap")],
        ),
    ];
    // The method table is the oracle's: every row of `java25_counted.txt` has a case.
    let oracle_rows: Vec<&str> = COUNTED_ORACLE
        .lines()
        .map(|line| line.split('\t').next().unwrap_or_default())
        .collect();
    let case_rows: Vec<&str> = cases.iter().map(|(_, row, _)| *row).collect();
    assert_eq!(case_rows, oracle_rows);
    for (method, row, expected) in cases {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        fixture.text_on(&mut host, method);
        let counted: Vec<String> = host
            .unknown_host_calls()
            .into_iter()
            .map(|(call, _)| call)
            .collect();
        eprintln!("{row}: {counted:?}");
        assert!(
            !counted.is_empty(),
            "{row} ({method}) answered, none counted"
        );
        for call in expected {
            assert!(
                counted.contains(&call),
                "{row} ({method}): {call} not counted: {counted:?}"
            );
        }
    }
}
