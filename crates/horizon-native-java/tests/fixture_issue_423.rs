#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/issue_423/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/issue_423/java25.txt");

#[test]
fn string_constructors_match_java25() {
    let fixture = Fixture::build("issue-423", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let got = fixture.text_on(&mut host, "rows");
    assert_eq!(got, ORACLE);
}

/// `new String(bytes, UTF_16)` is a constructor Decaf counts. Its result must
/// count on every later use, so `s == s` records an `if_acmp` unknown call
/// rather than being answered silently (Java 25 prints `true`).
#[test]
fn counted_string_identity_is_counted() {
    let fixture = Fixture::build("issue-423-counted", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.text_on(&mut host, "countedSelf");
    let calls = host.unknown_host_calls();
    assert!(
        calls
            .iter()
            .any(|(call, _)| call
                == "java/lang/Object#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z"),
        "{calls:?}"
    );
}

/// Concatenating a counted constructor's String counts the concatenation
/// instead of joining the object's internal key into the text.
#[test]
fn counted_string_concat_is_counted() {
    let fixture = Fixture::build("issue-423-concat", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    // The counted concatenation answers the counted call's placeholder, never
    // a text joined from the counted String's internal key.
    let got = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "countedConcat",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .expect("countedConcat runs");
    assert!(
        !matches!(&got, horizon_native_java::Value::Ref(Some(text)) if text.contains("java/lang/String#")),
        "{got:?}"
    );
    let calls = host.unknown_host_calls();
    assert!(
        calls.iter().any(|(call, _)| call
            .starts_with("java/lang/invoke/StringConcatFactory#makeConcatWithConstants")),
        "{calls:?}"
    );
}

#[test]
fn counted_strings_never_become_marker_text() {
    let methods = [
        "countedObjectConcat",
        "countedBuilderAppend",
        "countedJoin",
        "countedFormat",
        "countedConcatLength",
    ];
    for method in methods {
        let tag = format!("issue-423-{method}");
        let fixture = Fixture::build(&tag, &[("demo/Probe.java", PROBE)]);
        let mut host = fixture.jar.seed_host(StdHost::new());
        let got = fixture
            .jar
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .expect("probe runs");
        assert!(
            !matches!(&got, horizon_native_java::Value::Ref(Some(text)) if text.contains("java/lang/String#")),
            "{method} leaked marker: {got:?}"
        );
        assert!(
            !host.unknown_host_calls().is_empty(),
            "{method} did not count: {:?}",
            host.unknown_host_calls()
        );
    }
}

/// `new String(counted)` builds a String whose text is as opaque as its
/// argument's, so `outer.toString()` counts and never answers text.
#[test]
fn copy_of_a_counted_string_is_counted() {
    let fixture = Fixture::build("issue-423-copy", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let got = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "countedCopy",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .expect("countedCopy runs");
    assert!(
        !matches!(&got, horizon_native_java::Value::Ref(Some(text)) if text == "null" || text.contains("java/lang/String#")),
        "{got:?}"
    );
    let calls = host.unknown_host_calls();
    assert!(
        calls
            .iter()
            .any(|(call, _)| call.starts_with("java/lang/String#toString")),
        "{calls:?}"
    );
}
