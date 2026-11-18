#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const PROBE: &str = include_str!("fixtures/non_utf8_readers/demo/Probe.java");
const JAVA25: &str = include_str!("fixtures/non_utf8_readers/java25.txt");
#[test]
fn readers_match_java_25_capture() {
    let fixture = Fixture::build("non-utf8-readers", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows"), JAVA25);
    assert_eq!(host.unknown_host_calls(), &[]);
}
#[test]
fn counted_calls_are_explicit() {
    let fixture = Fixture::build("non-utf8-readers-counted", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "counted"), "ok");
    assert_eq!(
        host.unknown_host_calls(),
        [
            ("java/nio/file/Files#newBufferedReader(Ljava/nio/file/Path;Ljava/nio/charset/Charset;)Ljava/io/BufferedReader;".into(), 1),
            ("java/nio/file/Files#readAllLines(Ljava/nio/file/Path;Ljava/nio/charset/Charset;)Ljava/util/List;".into(), 1),
            ("java/nio/file/Files#readString(Ljava/nio/file/Path;Ljava/nio/charset/Charset;)Ljava/lang/String;".into(), 1),
        ]
    );
}
