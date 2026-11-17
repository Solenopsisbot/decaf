#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const PROBE: &str = include_str!("fixtures/mixin_service_probe/demo/Probe.java");
const JAVA25: &str = include_str!("fixtures/mixin_service_probe/java25.txt");
const REFLECT_NEW: &str =
    "java/lang/reflect/Constructor#newInstance([Ljava/lang/Object;)Ljava/lang/Object;";
#[test]
fn mixin_service_rows_match_java25_capture() {
    let fixture = Fixture::build("mixin-service-probe", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows").trim(), JAVA25.trim());
    assert_eq!(host.unknown_host_calls(), &[(REFLECT_NEW.into(), 2)]);
}
#[test]
fn get_bytecode_provider_remains_counted() {
    let fixture = Fixture::build("mixin-service-probe-counted", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.text_on(&mut host, "provider");
    assert_eq!(host.unknown_host_calls(), &[(REFLECT_NEW.into(), 1)]);
}
