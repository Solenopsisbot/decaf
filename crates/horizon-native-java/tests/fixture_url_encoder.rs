#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;
const PROBE: &str = include_str!("fixtures/url_encoder/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/url_encoder/java25.txt");
#[test]
fn url_encoder_utf8_matches_java25() {
    let fixture = Fixture::build("url-encoder", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
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
    assert_eq!(answer, Value::Ref(Some(ORACLE.to_owned())));
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
    let sig = "java/net/URLEncoder#encode(Ljava/lang/String;Ljava/nio/charset/Charset;)Ljava/lang/String;";
    for method in [
        "rowsOtherCharset",
        "loneSurrogate",
        "replacementChar",
        "nullString",
    ] {
        let mut counted_host = fixture.jar.seed_host(StdHost::new());
        let _ = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut counted_host,
        );
        // `"\uD800"` is a `String` constant with a lone surrogate: its load is counted too (#447).
        let mut expected = Vec::new();
        if method == "loneSurrogate" {
            expected.push(("java/lang/String#ldc()Ljava/lang/String;".into(), 1));
        }
        expected.push((sig.into(), 1));
        assert_eq!(counted_host.unknown_host_calls(), expected);
    }
}
