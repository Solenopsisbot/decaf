//! Java 25 oracle fixture for the vanilla enums `SoundSource`, `AttributeModifier.Operation`
//! and `HumanoidArm`: `values()` order, `name()`, `ordinal()`, `valueOf` identity and
//! `SoundSource.getName()` (not the lowercased constant name: `RECORDS` is `record`).
//!
//! `fixtures/vanilla_enum_rows/demo/Probe.java` is one source run two ways: on Java 25
//! against the real 26.3 game jar after bootstrap (`java25.txt`, command in the README), and
//! here, compiled against the stubs below. Only `demo/` ships, so every enum call reaches the
//! host model; `rows()` must equal the capture byte for byte and nothing may be counted.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/sounds/SoundSource.java",
        "package net.minecraft.sounds;
public enum SoundSource {
 MASTER, MUSIC, RECORDS, WEATHER, BLOCKS, HOSTILE, NEUTRAL, PLAYERS, AMBIENT, VOICE, UI;
 public String getName() { throw new AssertionError(); }
}",
    ),
    (
        "net/minecraft/world/entity/HumanoidArm.java",
        "package net.minecraft.world.entity; public enum HumanoidArm { LEFT, RIGHT }",
    ),
    (
        "net/minecraft/world/entity/ai/attributes/AttributeModifier.java",
        "package net.minecraft.world.entity.ai.attributes;
public final class AttributeModifier {
 public enum Operation { ADD_VALUE, ADD_MULTIPLIED_BASE, ADD_MULTIPLIED_TOTAL }
}",
    ),
];
const PROBE: &str = include_str!("fixtures/vanilla_enum_rows/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/vanilla_enum_rows/java25.txt");

#[test]
fn vanilla_enum_rows_match_java25() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("vanilla-enum-rows", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    let unknown = host.unknown_host_calls();
    assert_eq!(rows, ORACLE, "unknown calls: {unknown:?}");
    assert!(unknown.is_empty(), "{unknown:?}");
}

// EnumMap ordering coverage is exercised by the host model above.
