//! Java 25 oracle fixture for the vanilla `DamageTypes` `ResourceKey` constants
//! (`vanilla_registry_keys.rs`, table `vanilla_damage_types_capture.txt`; Spell Engine reads
//! `DamageTypes` keys during init).
//!
//! `fixtures/vanilla_damage_types_keys/demo/Probe.java` reads every one of the 51 constants and is
//! one source run two ways: on Java 25 against the real 26.3 game jar (`java25.txt`, command in
//! the README), and here, compiled against the stubs below. Only `demo/` ships, so every game
//! call reaches the host model; `rows()` must equal the capture byte for byte and nothing may be
//! counted.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

/// Every `DamageTypes` field, so the probe compiles; the host answers each read.
const DAMAGE_TYPES_STUB: &str = "package net.minecraft.world.damagesource; import net.minecraft.resources.ResourceKey; public interface DamageTypes { public static final ResourceKey IN_FIRE = null, CAMPFIRE = null, LIGHTNING_BOLT = null, ON_FIRE = null, LAVA = null, HOT_FLOOR = null, SULFUR_CUBE_HOT = null, IN_WALL = null, CRAMMING = null, DROWN = null, STARVE = null, CACTUS = null, FALL = null, ENDER_PEARL = null, FLY_INTO_WALL = null, FELL_OUT_OF_WORLD = null, GENERIC = null, MAGIC = null, WITHER = null, DRAGON_BREATH = null, DRY_OUT = null, SWEET_BERRY_BUSH = null, FREEZE = null, STALAGMITE = null, FALLING_BLOCK = null, FALLING_ANVIL = null, FALLING_STALACTITE = null, STING = null, MOB_ATTACK = null, MOB_ATTACK_NO_AGGRO = null, PLAYER_ATTACK = null, SPEAR = null, ARROW = null, TRIDENT = null, MOB_PROJECTILE = null, SPIT = null, WIND_CHARGE = null, FIREWORKS = null, FIREBALL = null, UNATTRIBUTED_FIREBALL = null, WITHER_SKULL = null, THROWN = null, INDIRECT_MAGIC = null, THORNS = null, EXPLOSION = null, PLAYER_EXPLOSION = null, SONIC_BOOM = null, BAD_RESPAWN_POINT = null, OUTSIDE_BORDER = null, GENERIC_KILL = null, MACE_SMASH = null; }";

const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources; public class ResourceKey<T> {}",
    ),
    (
        "net/minecraft/world/damagesource/DamageTypes.java",
        DAMAGE_TYPES_STUB,
    ),
];
const PROBE: &str = include_str!("fixtures/vanilla_damage_types_keys/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/vanilla_damage_types_keys/java25.txt");

#[test]
fn vanilla_damage_type_key_rows_match_java25() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("vanilla-damage-types-keys", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    let unknown = host.unknown_host_calls();
    assert_eq!(rows, ORACLE, "unknown calls: {unknown:?}");
    assert!(unknown.is_empty(), "{unknown:?}");
}
