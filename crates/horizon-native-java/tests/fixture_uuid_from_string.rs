//! `UUID.fromString` answers what Java 25 answers, or throws its exact exception: the captured
//! rows in `fixtures/uuid_from_string/java25.txt` (#217). What the model cannot tell is counted.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const ORACLE: &str = include_str!("fixtures/uuid_from_string/java25.txt");
const PROBE: &str = include_str!("fixtures/uuid_from_string/demo/Probe.java");
const FROM_STRING: &str = "java/util/UUID#fromString(Ljava/lang/String;)Ljava/util/UUID;";

#[test]
fn uuid_from_string_matches_java25() {
    let fixture = javac_fixture::Fixture::build("uuid_from_string", &[("demo/Probe.java", PROBE)]);
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
    for (row, (ours, java)) in text.lines().zip(ORACLE.lines()).enumerate() {
        assert_eq!(ours, java, "row {row}");
    }
    assert_eq!(text, ORACLE);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

/// Past the length and dash checks Java reads each field with `Character.digit`, which takes
/// every Unicode decimal digit and the fullwidth letters (Java 25: `١-2-3-4-5` and
/// `１-2-3-4-5` are `00000001-0002-0003-0004-000000000005`, `é-2-3-4-5` throws), and a
/// null name throws a NullPointerException whose message names the JDK's code. The model knows
/// neither, so each call is counted instead of answered or thrown.
#[test]
fn what_the_model_cannot_tell_is_counted() {
    const SOURCE: &str = r#"package demo;
import java.util.UUID;
public class Counted {
  public static int run(String name) {
    try { UUID.fromString(name); return 0; } catch (RuntimeException e) { return 1; }
  }
}"#;
    let fixture =
        javac_fixture::Fixture::build("uuid_from_string_counted", &[("demo/Counted.java", SOURCE)]);
    let inputs = [
        Some("\u{e9}-2-3-4-5"),
        Some("\u{661}-2-3-4-5"),
        Some("\u{ff11}-2-3-4-5"),
        Some("\u{1f600}-2-3-4-5"),
        Some("123e4567-e89b-12d3-a456-4266141740\u{661}\u{661}"),
        None,
    ];
    for input in inputs {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let argument = horizon_native_java::Value::Ref(input.map(str::to_owned));
        let _ = fixture.jar.invoke_method_with_args(
            "demo/Counted",
            "run",
            "(Ljava/lang/String;)I",
            None,
            &[argument],
            &mut host,
        );
        assert!(
            host.unknown_host_calls()
                .iter()
                .any(|(call, _)| call == FROM_STRING),
            "{input:?}: {:?}",
            host.unknown_host_calls()
        );
    }
}
