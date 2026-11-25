//! Fabric Loader's `SemanticVersion` (`fabric_version.rs`): what a mod reads off
//! `ModMetadata.getVersion()`.
//!
//! `fixtures/fabric_semantic_version/demo/Probe.java` runs on Java 25 against the real Fabric
//! Loader 0.20.1 (`oracle/OracleMain.java` hands it `Version.parse(text)`; commands in the
//! README) and here as the running jar, reading its own version (`fabric.mod.json`) and the
//! built-in `minecraft` mod's (the seeded game version). `describe` must equal `java25.txt` with
//! nothing counted for each text the oracle parses to a `SemanticVersion`; every other text (the
//! oracle's `StringVersion` texts included), and every call the oracle does not show, is counted.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const ORACLE: &str = include_str!("fixtures/fabric_semantic_version/java25.txt");
const PROBE: &str = include_str!("fixtures/fabric_semantic_version/demo/Probe.java");

const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/loader/api/Version.java",
        include_str!("fixtures/fabric_semantic_version/stubs/net/fabricmc/loader/api/Version.java"),
    ),
    (
        "net/fabricmc/loader/api/SemanticVersion.java",
        include_str!(
            "fixtures/fabric_semantic_version/stubs/net/fabricmc/loader/api/SemanticVersion.java"
        ),
    ),
    (
        "net/fabricmc/loader/api/FabricLoader.java",
        include_str!(
            "fixtures/fabric_semantic_version/stubs/net/fabricmc/loader/api/FabricLoader.java"
        ),
    ),
    (
        "net/fabricmc/loader/api/ModContainer.java",
        include_str!(
            "fixtures/fabric_semantic_version/stubs/net/fabricmc/loader/api/ModContainer.java"
        ),
    ),
    (
        "net/fabricmc/loader/api/metadata/ModMetadata.java",
        include_str!(
            "fixtures/fabric_semantic_version/stubs/net/fabricmc/loader/api/metadata/ModMetadata.java"
        ),
    ),
];

/// The texts `java25.txt` parses to a `SemanticVersion`.
const SEMANTIC: &[&str] = &[
    "26.3+v260915",
    "1.0.0",
    "1.0.0-beta.2+build.5",
    "1.2",
    "1.0.0-SNAPSHOT",
    "1.0",
    "1",
    "1.0.0.0",
    "26.3",
    "1.20.1-rc.1",
    "1.21.1",
    "25",
    "1.2.3",
    "1.2.0+mc26.3",
    "0.1",
    "2.0-beta.1",
    "0.19.5",
    "0.5.5",
    "0.165.0+26.3",
    "2.0.6+fcdff87f5d",
    "3.1.2+fcdff87f5d",
    "4.1.9+ffef5f675d",
    "3.1.0+5af7744b5d",
];

/// The texts `java25.txt` parses to a `StringVersion` (`instanceof SemanticVersion` is false).
const STRING_VERSIONS: &[&str] = &["v1.0.0", "abc"];

/// What `Probe.describe` printed on Java 25 for `Version.parse(text)`.
fn oracle(text: &str) -> &'static str {
    let prefix = format!("Version.parse(\"{text}\") -> ");
    ORACLE
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("no oracle row for {text:?}"))
}

fn build(test: &str, mod_version: &str) -> Fixture {
    let mut sources: Vec<(&str, &str)> = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    Fixture::build_with_mod_json(
        test,
        &sources,
        &format!(r#"{{"id":"probe-fixture","version":"{mod_version}","schemaVersion":1}}"#),
    )
}

#[test]
fn the_oracle_holds_the_cases_the_runtime_answers() {
    for text in SEMANTIC {
        assert!(oracle(text).starts_with("instanceof=true "), "{text}");
    }
    for text in STRING_VERSIONS {
        assert!(oracle(text).starts_with("instanceof=false "), "{text}");
    }
}

#[test]
fn a_mods_own_semantic_version_is_java25s() {
    for text in SEMANTIC {
        let fixture = build("fabric-semantic-version-mod", text);
        let mut host = fixture
            .jar
            .seed_host(StdHost::new().with_environment_type("SERVER"));
        assert_eq!(fixture.text_on(&mut host, "mod"), oracle(text), "{text}");
        assert!(
            host.unknown_host_calls().is_empty(),
            "{text}: {:?}",
            host.unknown_host_calls()
        );
    }
}

#[test]
fn the_game_version_is_java25s_for_every_semantic_text() {
    let fixture = build("fabric-semantic-version-game", "1.0");
    for text in SEMANTIC {
        let mut host = fixture.jar.seed_host(
            StdHost::new()
                .with_environment_type("SERVER")
                .with_game_version(*text),
        );
        assert_eq!(
            fixture.text_on(&mut host, "minecraft"),
            oracle(text),
            "{text}"
        );
        assert!(
            host.unknown_host_calls().is_empty(),
            "{text}: {:?}",
            host.unknown_host_calls()
        );
    }
}

/// Counted `ModMetadata.getVersion()` calls, as the host records them.
fn counted_get_version(host: &StdHost) -> bool {
    host.unknown_host_calls().iter().any(|(call, _)| {
        call == "net/fabricmc/loader/api/metadata/ModMetadata#getVersion()Lnet/fabricmc/loader/api/Version;"
    })
}

/// A text the oracle shows as a `StringVersion` (the loader builds one when `SemanticVersion`
/// parsing fails), or does not show at all, is counted: `getVersion()` never hands out a plain
/// `String`, whose casts and `instanceof` differ from a `StringVersion`'s. Through the jar's own
/// `fabric.mod.json`, the built-in `minecraft` mod, and a cast to `SemanticVersion`.
#[test]
fn other_texts_are_counted() {
    for text in STRING_VERSIONS.iter().chain(&["1.0.0-", "x"]) {
        for method in ["mod", "count"] {
            let fixture = build("fabric-semantic-version-other", text);
            let mut host = fixture
                .jar
                .seed_host(StdHost::new().with_environment_type("SERVER"));
            let _ = fixture.jar.invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            );
            assert!(
                counted_get_version(&host),
                "{text} {method}: {:?}",
                host.unknown_host_calls()
            );
        }
        let fixture = build("fabric-semantic-version-other-game", "1.0");
        let mut host = fixture.jar.seed_host(
            StdHost::new()
                .with_environment_type("SERVER")
                .with_game_version(*text),
        );
        let _ = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            "minecraft",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        assert!(
            counted_get_version(&host),
            "{text} minecraft: {:?}",
            host.unknown_host_calls()
        );
    }
}

/// The built-in `java` mod's version is the specification version, `25`.
#[test]
fn the_java_mods_version_is_java25s() {
    let fixture = build("fabric-semantic-version-java", "1.0");
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("SERVER"));
    assert_eq!(fixture.text_on(&mut host, "java"), oracle("25"));
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

/// An unknown game version is still unknown: `getVersion` is counted, as it was.
#[test]
fn an_unknown_game_version_is_still_counted() {
    let fixture = build("fabric-semantic-version-unseeded", "1.0");
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("SERVER"));
    let result = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "minecraft",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert!(result.is_err(), "{result:?}");
    assert!(
        host.unknown_host_calls().iter().any(|(call, _)| call
            == "net/fabricmc/loader/api/metadata/ModMetadata#getVersion()Lnet/fabricmc/loader/api/Version;"),
        "{:?}",
        host.unknown_host_calls()
    );
}

/// `compareTo` has no oracle row here: counted on a handle.
#[test]
fn compare_to_stays_counted() {
    let fixture = build("fabric-semantic-version-compare", "1.0.0");
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("SERVER"));
    let _ = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "compared",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert!(
        host.unknown_host_calls()
            .iter()
            .any(|(call, _)| call.contains("compareTo")),
        "{:?}",
        host.unknown_host_calls()
    );
}
