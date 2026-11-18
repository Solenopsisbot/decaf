//! Java 25 oracle fixture: a game enum whose constants the host doesn't list is still an enum.
//!
//! `fixtures/game_enum_class/demo/Probe.java` is one source run two ways: on Java 25 against
//! the real 26.3 jar (`java25.txt`, command in the README) and here, compiled against the stubs
//! below. Only `demo/` ships.
//!
//! `exact()` is `Class.isEnum` for five such enums and must equal the capture with nothing
//! counted. `counted()` asks for `Rarity`'s constants, which Java answers (`4`, `COMMON`, `4`)
//! and the host does not model: each call must be counted, and `valueOf` must not throw Java's
//! "No enum constant" for a constant that exists. (Listing these enums with an empty constant
//! list did both silently: nothing counted, and that exception.) The neutral value the host
//! hands back beside a counted call is not Java's and is not compared.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

// Each is `public final class … extends java.lang.Enum<…>` in `javap` on the 26.3 server jar.
// Only what javac needs here: the enum-ness, and `Rarity`'s first constant.
const SOURCES: [(&str, &str); 5] = [
    (
        "net/minecraft/world/item/Rarity.java",
        "package net.minecraft.world.item; public enum Rarity { COMMON, UNCOMMON, RARE, EPIC }",
    ),
    (
        "net/minecraft/core/Direction.java",
        "package net.minecraft.core; public enum Direction { DOWN; public enum Plane { HORIZONTAL, VERTICAL } }",
    ),
    (
        "net/minecraft/world/entity/MobCategory.java",
        "package net.minecraft.world.entity; public enum MobCategory { MONSTER }",
    ),
    (
        "net/minecraft/world/item/DyeColor.java",
        "package net.minecraft.world.item; public enum DyeColor { WHITE }",
    ),
    (
        "net/minecraft/world/Difficulty.java",
        "package net.minecraft.world; public enum Difficulty { PEACEFUL }",
    ),
];
const PROBE: &str = include_str!("fixtures/game_enum_class/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/game_enum_class/java25.txt");

fn fixture(name: &str) -> Fixture {
    let mut sources = SOURCES.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    Fixture::build(name, &sources)
}

#[test]
fn is_enum_matches_java25() {
    let fixture = fixture("game-enum-class");
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "exact");
    let java: String = ORACLE
        .lines()
        .take(5)
        .map(|line| format!("{line}\n"))
        .collect();
    let unknown = format!("{:?}", host.unknown_host_calls());
    assert_eq!((rows.as_str(), unknown.as_str()), (java.as_str(), "[]"));
}

#[test]
fn unlisted_constants_are_counted() {
    let fixture = fixture("game-enum-constants");
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "counted");
    let counted: Vec<String> = host
        .unknown_host_calls()
        .into_iter()
        .map(|(call, _)| call)
        .collect();
    for call in [
        "net/minecraft/world/item/Rarity#values()[Lnet/minecraft/world/item/Rarity;",
        "net/minecraft/world/item/Rarity#valueOf(Ljava/lang/String;)Lnet/minecraft/world/item/Rarity;",
        "java/lang/Class#getEnumConstants()[Ljava/lang/Object;",
    ] {
        assert!(
            counted.iter().any(|name| name == call),
            "{call} not in {counted:?}"
        );
    }
    assert!(!rows.contains("No enum constant"), "{rows:?}");
}
