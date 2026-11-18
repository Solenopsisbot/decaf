#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const ORACLE: &str = include_str!("fixtures/fastutil_boolean_entries/java25.txt");

const STUBS: &[(&str, &str)] = &[
    (
        "it/unimi/dsi/fastutil/objects/Object2BooleanMap.java",
        r#"package it.unimi.dsi.fastutil.objects; public interface Object2BooleanMap<K> { interface Entry<K> { K getKey(); boolean getBooleanValue(); } interface FastSortedEntrySet<K> extends java.lang.Iterable<Entry<K>> {} }"#,
    ),
    (
        "it/unimi/dsi/fastutil/objects/Object2BooleanLinkedOpenHashMap.java",
        r#"package it.unimi.dsi.fastutil.objects; import java.util.*; public class Object2BooleanLinkedOpenHashMap<K> { public Object2BooleanLinkedOpenHashMap() {} public boolean put(K k, boolean v) { throw new UnsupportedOperationException(); } public boolean removeBoolean(K k) { throw new UnsupportedOperationException(); } public java.lang.Iterable<Object2BooleanMap.Entry<K>> object2BooleanEntrySet() { throw new UnsupportedOperationException(); } }"#,
    ),
];
const PROBE: &str = include_str!("fixtures/fastutil_boolean_entries/demo/Probe.java");
#[test]
fn fastutil_boolean_entries_matches_java25() {
    let fixture = javac_fixture::Fixture::build(
        "fastutil_boolean_entries",
        &[("demo/Probe.java", PROBE), STUBS[0], STUBS[1]],
    );
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "rows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let text = match answer {
        horizon_native_java::Value::Ref(Some(text)) => text,
        other => panic!("{other:?}"),
    };
    assert_eq!(text, ORACLE);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
