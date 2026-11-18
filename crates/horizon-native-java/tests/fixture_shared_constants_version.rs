#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{RunLoader, StdHost};
use javac_fixture::Fixture;
const WORLD: &str = r#"package net.minecraft; public interface WorldVersion { String name(); String id(); int protocolVersion(); boolean stable(); net.minecraft.world.level.storage.DataVersion dataVersion(); net.minecraft.server.packs.metadata.pack.PackFormat packVersion(net.minecraft.server.packs.PackType type); java.util.Date buildTime(); }"#;
const SHARED: &str = r#"package net.minecraft; public final class SharedConstants { public static void tryDetectVersion(){} public static WorldVersion getCurrentVersion(){throw new AssertionError();} }"#;
const DATA: &str = r#"package net.minecraft.world.level.storage; public final class DataVersion { public int version(){throw new AssertionError();} public String series(){throw new AssertionError();} }"#;
const PACK: &str = r#"package net.minecraft.server.packs.metadata.pack; public final class PackFormat { public int major(){throw new AssertionError();} public int minor(){throw new AssertionError();} }"#;
const TYPE: &str =
    r#"package net.minecraft.server.packs; public enum PackType { CLIENT_RESOURCES, SERVER_DATA }"#;
const PROBE: &str = include_str!("fixtures/shared_constants_version/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/shared_constants_version/java25.txt");
#[test]
fn rows_match_java25() {
    let f = Fixture::build(
        "shared-constants-version",
        &[
            ("net/minecraft/WorldVersion.java", WORLD),
            ("net/minecraft/SharedConstants.java", SHARED),
            ("net/minecraft/world/level/storage/DataVersion.java", DATA),
            (
                "net/minecraft/server/packs/metadata/pack/PackFormat.java",
                PACK,
            ),
            ("net/minecraft/server/packs/PackType.java", TYPE),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut h = f.jar.seed_host(
        StdHost::new()
            .with_run_loader(RunLoader::NeoForge)
            .with_game_version("26.3"),
    );
    assert_eq!(f.text_on(&mut h, "rows"), ORACLE);
    assert_eq!(h.unknown_host_calls(), vec![]);
}
#[test]
fn build_time_is_counted() {
    let f = Fixture::build(
        "shared-constants-version-counted",
        &[
            ("net/minecraft/WorldVersion.java", WORLD),
            ("net/minecraft/SharedConstants.java", SHARED),
            ("net/minecraft/world/level/storage/DataVersion.java", DATA),
            (
                "net/minecraft/server/packs/metadata/pack/PackFormat.java",
                PACK,
            ),
            ("net/minecraft/server/packs/PackType.java", TYPE),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut h = f.jar.seed_host(
        StdHost::new()
            .with_run_loader(RunLoader::NeoForge)
            .with_game_version("26.3"),
    );
    let _ = f.text_on(&mut h, "buildTime");
    assert_eq!(
        h.unknown_host_calls(),
        vec![(
            "net/minecraft/WorldVersion#buildTime()Ljava/util/Date;".into(),
            1
        )]
    );
}
