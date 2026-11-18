//! Puzzles Lib's client constructor bridge, with Java 25 rows and explicit counted cases.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/client_mod_constructor/demo/Probe.java");
const JAVA25: &str = include_str!("fixtures/client_mod_constructor/java25.txt");
const COMMON_CONSTRUCT: &str = "fuzs/puzzleslib/common/api/core/v1/ModConstructor#construct(Ljava/lang/String;Ljava/util/function/Supplier;)V";

const STUBS: &[(&str, &str)] = &[
    (
        "fuzs/puzzleslib/common/api/client/core/v1/ClientModConstructor.java",
        r#"package fuzs.puzzleslib.common.api.client.core.v1; import java.util.function.Supplier; public interface ClientModConstructor { static void construct(String id, Supplier<ClientModConstructor> s) {} void onConstructMod(); void onClientSetup(); }"#,
    ),
    (
        "fuzs/puzzleslib/common/api/core/v1/ModConstructor.java",
        r#"package fuzs.puzzleslib.common.api.core.v1; import java.util.function.Supplier; public interface ModConstructor { static void construct(String id, Supplier<ModConstructor> s) {} }"#,
    ),
];

fn fixture() -> Fixture {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    Fixture::build("client-mod-constructor", &sources)
}

#[test]
fn normal_use_matches_java25() {
    let fixture = fixture();
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("CLIENT"));
    assert_eq!(fixture.text_on(&mut host, "normal"), JAVA25);
    assert_eq!(
        host.unknown_host_calls(),
        Vec::<(String, usize)>::new(),
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn common_constructor_remains_counted_by_exact_name() {
    let fixture = fixture();
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("CLIENT"));
    assert_eq!(fixture.text_on(&mut host, "counted"), "ok");
    assert_eq!(
        host.unknown_host_calls(),
        vec![(COMMON_CONSTRUCT.to_owned(), 1)]
    );
}
