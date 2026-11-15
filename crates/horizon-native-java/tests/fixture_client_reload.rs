//! Java 25 fixture for C2's registration-only resource reload surfaces.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::Value;

const PROBE: &str = include_str!("fixtures/client_reload/demo/Probe.java");
const IDENTIFIER: &str =
    include_str!("fixtures/client_reload/net/minecraft/resources/Identifier.java");
const PACK_TYPE: &str =
    include_str!("fixtures/client_reload/net/minecraft/server/packs/PackType.java");
const PREPARABLE: &str = include_str!(
    "fixtures/client_reload/net/minecraft/server/packs/resources/PreparableReloadListener.java"
);
const HELPER: &str = include_str!(
    "fixtures/client_reload/net/fabricmc/fabric/api/resource/ResourceManagerHelper.java"
);
const IDENTIFIABLE: &str = include_str!(
    "fixtures/client_reload/net/fabricmc/fabric/api/resource/IdentifiableResourceReloadListener.java"
);
const LOADER: &str =
    include_str!("fixtures/client_reload/net/fabricmc/fabric/api/resource/v1/ResourceLoader.java");
const KEYS: &str = include_str!(
    "fixtures/client_reload/net/fabricmc/fabric/api/resource/v1/reloader/ResourceReloaderKeys.java"
);
const CONVERTER: &str =
    include_str!("fixtures/client_reload/net/minecraft/resources/FileToIdConverter.java");

#[test]
fn registrations_and_file_converter_match_java_25() {
    let fixture = javac_fixture::Fixture::build(
        "client-reload",
        &[
            ("demo/Probe.java", PROBE),
            ("net/minecraft/resources/Identifier.java", IDENTIFIER),
            ("net/minecraft/server/packs/PackType.java", PACK_TYPE),
            (
                "net/minecraft/server/packs/resources/PreparableReloadListener.java",
                PREPARABLE,
            ),
            (
                "net/fabricmc/fabric/api/resource/ResourceManagerHelper.java",
                HELPER,
            ),
            (
                "net/fabricmc/fabric/api/resource/IdentifiableResourceReloadListener.java",
                IDENTIFIABLE,
            ),
            (
                "net/fabricmc/fabric/api/resource/v1/ResourceLoader.java",
                LOADER,
            ),
            (
                "net/fabricmc/fabric/api/resource/v1/reloader/ResourceReloaderKeys.java",
                KEYS,
            ),
            ("net/minecraft/resources/FileToIdConverter.java", CONVERTER),
        ],
    );
    for (method, expected) in [
        ("fabric", "true|duplicate"),
        ("v1", "true|minecraft:atlas"),
        (
            "converter",
            "demo:textures/foo/bar.json|demo:foo/bar|true|textures|.json",
        ),
    ] {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let text = match answer {
            Ok(Value::Ref(Some(text))) => text,
            other => panic!("{method}: {other:?}"),
        };
        assert_eq!(text, expected, "{method}");
        assert!(
            host.unknown_host_calls().is_empty(),
            "{method}: {:?}",
            host.unknown_host_calls()
        );
    }
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "builtin",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert_eq!(answer.unwrap(), Value::Ref(Some("false".into())));
    assert_eq!(
        host.unknown_host_calls().len(),
        1,
        "builtin pack remains counted"
    );

    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "malformed",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert_eq!(
        answer.unwrap(),
        Value::Ref(Some("Range [9, -4) out of bounds for length 1".into()))
    );
    assert!(host.unknown_host_calls().is_empty());
}
