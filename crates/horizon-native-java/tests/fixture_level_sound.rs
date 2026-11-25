//! javac fixture tests for Minecraft 26.3 `Level.playSound`, `playSeededSound`,
//! `playLocalSound`, `playPlayerSound`, `Entity.playSound`, `Player.playSound`,
//! `LocalPlayer.playSound`, and `SoundEvent` range / `Holder` semantics.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use horizon_native_java::{
    EntitySoundFacts, ExecutionBudget, GameHookContext, GameHookEffect, GameObject, HostKind,
    HostRef, LevelSoundEffect, NativeError, NativeJar, SoundHolderKind, SoundRecord, SoundSeed,
    SoundSide, SoundSource, SoundTarget, StdHost, Value,
    client_surface::{ClientCamera, ClientLevel, ClientPlayer, ClientSnapshot},
    events::{EventContext, EventFacade, EventFiring, FacadeLevel, FacadePlayer, GameEvent},
};
use javac_fixture::Fixture;

fn test_budget() -> ExecutionBudget {
    ExecutionBudget::within(Duration::from_secs(30))
}

// Compile-only MC 26.3 stubs (only `demo/` classes ship in the fixture jar).
const MC_STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        r#"package net.minecraft.resources;
public final class Identifier {
    public static Identifier fromNamespaceAndPath(String ns, String path) { return null; }
    public static Identifier withDefaultNamespace(String path) { return null; }
    public static Identifier parse(String id) { return null; }
    public String getNamespace() { return null; }
    public String getPath() { return null; }
}"#,
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        r#"package net.minecraft.resources;
public class ResourceKey<T> {
    public static <T> ResourceKey<T> create(ResourceKey<? extends net.minecraft.core.Registry<T>> registry, Identifier id) { return null; }
}"#,
    ),
    (
        "net/minecraft/core/HolderOwner.java",
        r#"package net.minecraft.core;
public interface HolderOwner<T> {}"#,
    ),
    (
        "net/minecraft/core/Holder.java",
        r#"package net.minecraft.core;
import net.minecraft.resources.ResourceKey;
public interface Holder<T> {
    T value();
    boolean isBound();
    static <T> Holder<T> direct(T value) { return null; }
    class Reference<T> implements Holder<T> {
        @Override public T value() { return null; }
        @Override public boolean isBound() { return false; }
        public static <T> Reference<T> createStandAlone(HolderOwner<T> owner, ResourceKey<T> key) { return null; }
    }
}"#,
    ),
    (
        "net/minecraft/core/Registry.java",
        r#"package net.minecraft.core;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
public interface Registry<T> extends HolderOwner<T> {
    static <V, T extends V> T register(Registry<V> reg, Identifier id, T value) { return value; }
    static <T> Holder.Reference<T> registerForHolder(Registry<T> reg, Identifier id, T value) { return null; }
    Holder<T> wrapAsHolder(T value);
    ResourceKey<? extends Registry<T>> key();
}"#,
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        r#"package net.minecraft.core.registries;
import net.minecraft.core.Registry;
import net.minecraft.sounds.SoundEvent;
public class BuiltInRegistries {
    public static Registry<SoundEvent> SOUND_EVENT;
}"#,
    ),
    (
        "net/minecraft/core/registries/Registries.java",
        r#"package net.minecraft.core.registries;
import net.minecraft.core.Registry;
import net.minecraft.resources.ResourceKey;
import net.minecraft.sounds.SoundEvent;
public class Registries {
    public static ResourceKey<Registry<SoundEvent>> SOUND_EVENT;
}"#,
    ),
    (
        "net/minecraft/core/Vec3i.java",
        r#"package net.minecraft.core;
public class Vec3i {
    private int x, y, z;
    public Vec3i(int x, int y, int z) { this.x = x; this.y = y; this.z = z; }
    public int getX() { return x; }
    public int getY() { return y; }
    public int getZ() { return z; }
    protected Vec3i setX(int x) { this.x = x; return this; }
    protected Vec3i setY(int y) { this.y = y; return this; }
    protected Vec3i setZ(int z) { this.z = z; return this; }
}"#,
    ),
    (
        "net/minecraft/core/BlockPos.java",
        r#"package net.minecraft.core;
public class BlockPos extends Vec3i {
    public BlockPos(int x, int y, int z) { super(x, y, z); }
    public static class MutableBlockPos extends BlockPos {
        public MutableBlockPos(int x, int y, int z) { super(x, y, z); }
        public MutableBlockPos set(int x, int y, int z) { return this; }
    }
}"#,
    ),
    (
        "net/minecraft/util/RandomSource.java",
        r#"package net.minecraft.util;
public interface RandomSource {
    long nextLong();
    float nextFloat();
    int nextInt(int bound);
}"#,
    ),
    (
        "net/minecraft/sounds/SoundSource.java",
        r#"package net.minecraft.sounds;
public enum SoundSource {
    MASTER, MUSIC, RECORDS, WEATHER, BLOCKS, HOSTILE, NEUTRAL, PLAYERS, AMBIENT, VOICE, UI;
    public String getName() { return name().toLowerCase(); }
}"#,
    ),
    (
        "net/minecraft/sounds/SoundEvent.java",
        r#"package net.minecraft.sounds;
import java.util.Optional;
import net.minecraft.resources.Identifier;
public record SoundEvent(Identifier location, Optional<Float> fixedRange) {
    public static SoundEvent createVariableRangeEvent(Identifier id) { return new SoundEvent(id, Optional.empty()); }
    public static SoundEvent createFixedRangeEvent(Identifier id, float range) { return new SoundEvent(id, Optional.of(range)); }
    public float getRange(float volume) { return fixedRange.orElse(volume > 1.0f ? 16.0f * volume : 16.0f); }
}"#,
    ),
    (
        "net/minecraft/world/entity/Entity.java",
        r#"package net.minecraft.world.entity;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.level.Level;
public class Entity {
    public double getX() { return 0; }
    public double getY() { return 0; }
    public double getZ() { return 0; }
    public boolean isSilent() { return false; }
    public SoundSource getSoundSource() { return SoundSource.NEUTRAL; }
    public void playSound(SoundEvent sound) {}
    public void playSound(SoundEvent sound, float volume, float pitch) {}
    public Level level() { return null; }
}"#,
    ),
    (
        "net/minecraft/world/entity/LivingEntity.java",
        r#"package net.minecraft.world.entity;
public class LivingEntity extends Entity {}"#,
    ),
    (
        "net/minecraft/world/entity/player/Player.java",
        r#"package net.minecraft.world.entity.player;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.entity.LivingEntity;
public class Player extends LivingEntity {
    @Override
    public void playSound(SoundEvent sound, float volume, float pitch) {}
}"#,
    ),
    (
        "net/minecraft/server/level/ServerPlayer.java",
        r#"package net.minecraft.server.level;
import net.minecraft.world.entity.player.Player;
public class ServerPlayer extends Player {}"#,
    ),
    (
        "net/minecraft/client/player/LocalPlayer.java",
        r#"package net.minecraft.client.player;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.entity.player.Player;
public class LocalPlayer extends Player {
    @Override
    public void playSound(SoundEvent sound, float volume, float pitch) {}
}"#,
    ),
    (
        "net/minecraft/world/level/LevelAccessor.java",
        r#"package net.minecraft.world.level;
import net.minecraft.core.BlockPos;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.Entity;
public interface LevelAccessor {
    default void playSound(Entity except, BlockPos pos, SoundEvent sound, SoundSource source) {}
    void playSound(Entity except, BlockPos pos, SoundEvent sound, SoundSource source, float volume, float pitch);
    net.minecraft.util.RandomSource getRandom();
}"#,
    ),
    (
        "net/minecraft/world/level/Level.java",
        r#"package net.minecraft.world.level;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
public abstract class Level implements LevelAccessor {
    public boolean isClientSide() { return false; }
    @Override public RandomSource getRandom() { return null; }
    @Override public void playSound(Entity except, BlockPos pos, SoundEvent sound, SoundSource source) {}
    @Override public void playSound(Entity except, BlockPos pos, SoundEvent sound, SoundSource source, float volume, float pitch) {}
    public void playSound(Entity except, double x, double y, double z, SoundEvent sound, SoundSource source) {}
    public void playSound(Entity except, double x, double y, double z, SoundEvent sound, SoundSource source, float volume, float pitch) {}
    public void playSound(Entity except, double x, double y, double z, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch) {}
    public void playSound(Entity except, Entity target, SoundEvent sound, SoundSource source, float volume, float pitch) {}
    public void playSound(Entity except, Entity target, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch) {}
    public void playSeededSound(Entity except, double x, double y, double z, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch, long seed) {}
    public void playSeededSound(Entity except, double x, double y, double z, SoundEvent sound, SoundSource source, float volume, float pitch, long seed) {}
    public void playSeededSound(Entity except, Entity target, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch, long seed) {}
    public void playLocalSound(BlockPos pos, SoundEvent sound, SoundSource source, float volume, float pitch, boolean distanceDelay) {}
    public void playLocalSound(double x, double y, double z, SoundEvent sound, SoundSource source, float volume, float pitch, boolean distanceDelay) {}
    public void playLocalSound(Entity target, SoundEvent sound, SoundSource source, float volume, float pitch) {}
    public void playPlayerSound(SoundEvent sound, SoundSource source, float volume, float pitch) {}
    public void playSound(double x, double y, double z, SoundEvent sound) {}
}"#,
    ),
    (
        "net/minecraft/server/level/ServerLevel.java",
        r#"package net.minecraft.server.level;
import net.minecraft.world.level.Level;
public class ServerLevel extends Level {}"#,
    ),
    (
        "net/minecraft/client/multiplayer/ClientLevel.java",
        r#"package net.minecraft.client.multiplayer;
import net.minecraft.world.level.Level;
public class ClientLevel extends Level {}"#,
    ),
];

fn with_stubs<'a>(extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    MC_STUBS
        .iter()
        .copied()
        .chain(extra.iter().copied())
        .collect()
}

fn player_handle(uuid: u128, entity_id: i32) -> Value {
    let mut facade = EventFacade::default();
    facade.players.push(FacadePlayer {
        uuid,
        name: "Alex".into(),
        level: Some("minecraft:overworld".into()),
        entity_id: Some(entity_id),
        session: 1,
        life: 0,
    });
    // Use the exact player marker format the runtime assigns (`game/player/<hex>/<session>.<life>/<id>`).
    let marker = format!("game/player/{uuid:032x}/1.0/{entity_id}");
    Value::Host(HostRef::new(HostKind::GameEntity, marker))
}

fn entity_handle(entity_id: i32) -> Value {
    Value::Host(HostRef::new(
        HostKind::GameEntity,
        format!("game/entity/{entity_id}"),
    ))
}

fn block_pos_handle(x: i32, y: i32, z: i32) -> Value {
    Value::Host(HostRef::new(
        HostKind::GameValue,
        format!("block-pos/{x},{y},{z}"),
    ))
}

// Java 25 oracle values verified with JDK 25 `java.util.Random` (same 48-bit LCG as MC 26.3
// `LegacyRandomSource` / `ThreadSafeLegacyRandomSource`):
const SEED_42_LONGS: [i64; 3] = [
    -5_025_562_857_975_149_833,
    -5_843_495_416_241_995_736,
    5_694_868_678_511_409_995,
];
const SEED_123456789_LONG_0: i64 = -6_197_403_153_606_331_135;
const SEED_987654321_LONGS: [i64; 3] = [
    5_643_591_630_821_615_871,
    -3_342_970_523_933_462_541,
    842_562_190_649_680_514,
];

#[test]
fn server_level_all_play_sound_overloads_and_seed_order() {
    let sources = with_stubs(&[(
        "demoprobe/Probe.java",
        r#"package demoprobe;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelAccessor;

public final class Probe {
    public void onInitialize() {}

    public static long exerciseAll(Level level, Entity player, Entity mob, BlockPos hostPos) {
        SoundEvent regSound = Registry.register(
            BuiltInRegistries.SOUND_EVENT,
            Identifier.fromNamespaceAndPath("demo", "registered"),
            SoundEvent.createVariableRangeEvent(Identifier.fromNamespaceAndPath("demo", "registered"))
        );
        SoundEvent directSound = SoundEvent.createVariableRangeEvent(
            Identifier.fromNamespaceAndPath("demo", "unregistered")
        );
        Holder<SoundEvent> regHolder = BuiltInRegistries.SOUND_EVENT.wrapAsHolder(regSound);
        Holder<SoundEvent> directHolder = Holder.direct(directSound);

        // 0: LevelAccessor 4-arg BlockPos (jar BlockPos -> +0.5 centre), draw #0 from soundSeedGenerator
        ((LevelAccessor) level).playSound(null, new BlockPos(3, 64, -7), regSound, SoundSource.BLOCKS);

        // 1: Level 6-arg BlockPos with MutableBlockPos and except=player, draw #1
        BlockPos.MutableBlockPos mut = new BlockPos.MutableBlockPos(0, 0, 0);
        mut.set(10, 70, 20);
        level.playSound(player, mut, directSound, SoundSource.PLAYERS, 0.75f, 1.25f);

        // 2: Level 6-arg BlockPos with host handle and except=mob (non-player -> except None), draw #2
        level.playSound(mob, hostPos, regSound, SoundSource.HOSTILE, 1.5f, 0.8f);

        // Null BlockPos must throw NPE BEFORE drawing soundSeedGenerator!
        boolean threwNpe = false;
        try {
            level.playSound(null, (BlockPos) null, regSound, SoundSource.BLOCKS, 1.0f, 1.0f);
        } catch (NullPointerException expected) {
            threwNpe = true;
        }
        if (!threwNpe) {
            throw new IllegalStateException("expected NPE on null BlockPos");
        }

        // 3: Level 6-arg DDD SoundEvent (defaults vol=1, pitch=1), draw #3
        level.playSound(null, 1.25, 2.5, 3.75, regSound, SoundSource.AMBIENT);

        // 4: Level 8-arg DDD SoundEvent, draw #4
        level.playSound(player, 4.0, 5.0, 6.0, directSound, SoundSource.WEATHER, 2.0f, 0.5f);

        // 5: Level 8-arg DDD Holder (registered Reference), draw #5
        level.playSound(null, 7.0, 8.0, 9.0, regHolder, SoundSource.RECORDS, 4.0f, 1.0f);

        // 6: Level 6-arg Entity SoundEvent, draw #6
        level.playSound(null, mob, regSound, SoundSource.NEUTRAL, 0.6f, 1.1f);

        // 7: Level 6-arg Entity Holder (direct), draw #7
        level.playSound(player, player, directHolder, SoundSource.VOICE, 0.9f, 1.4f);

        // 8-10: playSeededSound overloads -> explicit seeds, NO RNG draw!
        level.playSeededSound(player, 11.0, 12.0, 13.0, regHolder, SoundSource.MUSIC, 0.5f, 1.0f, 777L);
        level.playSeededSound(mob, 14.0, 15.0, 16.0, directSound, SoundSource.MASTER, 1.2f, 0.9f, 888L);
        level.playSeededSound(null, mob, regHolder, SoundSource.UI, 0.8f, 1.3f, 999L);

        // ServerLevel playLocalSound / playPlayerSound are no-ops and draw NO RNG!
        level.playLocalSound(new BlockPos(1, 2, 3), regSound, SoundSource.BLOCKS, 1.0f, 1.0f, true);
        level.playLocalSound(1.0, 2.0, 3.0, regSound, SoundSource.BLOCKS, 1.0f, 1.0f, false);
        level.playLocalSound(mob, regSound, SoundSource.NEUTRAL, 1.0f, 1.0f);
        level.playPlayerSound(regSound, SoundSource.PLAYERS, 1.0f, 1.0f);

        // playLocalSound with null BlockPos still throws NPE on ServerLevel (reads pos.getX())
        boolean localNpe = false;
        try {
            level.playLocalSound((BlockPos) null, regSound, SoundSource.BLOCKS, 1.0f, 1.0f, false);
        } catch (NullPointerException expected) {
            localNpe = true;
        }
        if (!localNpe) {
            throw new IllegalStateException("expected NPE on ServerLevel.playLocalSound(null, ...)");
        }

        // Level.getRandom() must remain completely untouched by every sound call above!
        return level.getRandom().nextLong();
    }
}"#,
    )]);
    let fixture = Fixture::build("level_sound_server_overloads", &sources);
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    host.seed_server_level_randoms("minecraft:overworld", 123_456_789, 42);
    let uuid = 0x1122_3344_5566_7788_99aa_bbcc_ddee_ff00_u128;
    let level = StdHost::server_level_handle("minecraft:overworld");
    let player = player_handle(uuid, 101);
    let mob = entity_handle(202);
    let host_pos = block_pos_handle(-5, 12, 9);

    let level_random_first = fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "exerciseAll",
            "(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/BlockPos;)J",
            None,
            &[level, player, mob, host_pos],
            &mut host,
        )
        .expect("all server overloads succeed");

    assert_eq!(level_random_first, Value::Long(SEED_123456789_LONG_0));
    assert_eq!(host.unknown_host_call_count(), 0);

    let sounds = host.played_sounds();
    assert_eq!(
        sounds.len(),
        11,
        "11 server sound effects expected: {sounds:#?}"
    );

    // 0: 4-arg BlockPos (3, 64, -7) -> +0.5 centre, seed #0
    assert_eq!(sounds[0].side, SoundSide::Server);
    assert_eq!(sounds[0].dimension.as_deref(), Some("minecraft:overworld"));
    assert_eq!(sounds[0].sound_id, "demo:registered");
    assert_eq!(sounds[0].fixed_range, None);
    assert_eq!(
        sounds[0].holder,
        Some(SoundHolderKind::Reference {
            registry_id: "demo:registered".into()
        })
    );
    assert_eq!(sounds[0].target, SoundTarget::Position([3.5, 64.5, -6.5]));
    assert_eq!(sounds[0].source, SoundSource::Blocks);
    assert_eq!(sounds[0].volume, 1.0);
    assert_eq!(sounds[0].pitch, 1.0);
    assert_eq!(sounds[0].except_player, None);
    assert_eq!(
        sounds[0].seed,
        SoundSeed::LevelSeedGenerator(SEED_42_LONGS[0])
    );

    // 1: MutableBlockPos.set(10, 70, 20) -> [10.5, 70.5, 20.5], unregistered -> Direct, except=player, seed #1
    assert_eq!(sounds[1].sound_id, "demo:unregistered");
    assert_eq!(sounds[1].holder, Some(SoundHolderKind::Direct));
    assert_eq!(sounds[1].target, SoundTarget::Position([10.5, 70.5, 20.5]));
    assert_eq!(sounds[1].source, SoundSource::Players);
    assert_eq!(sounds[1].volume, 0.75);
    assert_eq!(sounds[1].pitch, 1.25);
    assert_eq!(sounds[1].except_player, Some(uuid));
    assert_eq!(
        sounds[1].seed,
        SoundSeed::LevelSeedGenerator(SEED_42_LONGS[1])
    );

    // 2: host BlockPos (-5, 12, 9) -> [-4.5, 12.5, 9.5], except=mob -> None, seed #2
    assert_eq!(sounds[2].target, SoundTarget::Position([-4.5, 12.5, 9.5]));
    assert_eq!(sounds[2].source, SoundSource::Hostile);
    assert_eq!(sounds[2].volume, 1.5);
    assert_eq!(sounds[2].pitch, 0.8);
    assert_eq!(sounds[2].except_player, None);
    assert_eq!(
        sounds[2].seed,
        SoundSeed::LevelSeedGenerator(SEED_42_LONGS[2])
    );

    // 3: 6-arg DDD defaults vol=1, pitch=1 (and NPE did NOT burn a seed!)
    assert_eq!(sounds[3].target, SoundTarget::Position([1.25, 2.5, 3.75]));
    assert_eq!(sounds[3].source, SoundSource::Ambient);
    assert_eq!(sounds[3].volume, 1.0);
    assert_eq!(sounds[3].pitch, 1.0);
    assert!(matches!(sounds[3].seed, SoundSeed::LevelSeedGenerator(_)));

    // 4: 8-arg DDD SoundEvent
    assert_eq!(sounds[4].target, SoundTarget::Position([4.0, 5.0, 6.0]));
    assert_eq!(sounds[4].source, SoundSource::Weather);
    assert_eq!(sounds[4].except_player, Some(uuid));
    assert_eq!(sounds[4].volume, 2.0);
    assert_eq!(sounds[4].pitch, 0.5);

    // 5: 8-arg DDD Holder
    assert_eq!(sounds[5].target, SoundTarget::Position([7.0, 8.0, 9.0]));
    assert_eq!(sounds[5].source, SoundSource::Records);
    assert_eq!(
        sounds[5].holder,
        Some(SoundHolderKind::Reference {
            registry_id: "demo:registered".into()
        })
    );
    assert_eq!(sounds[5].volume, 4.0);

    // 6: 6-arg Entity SoundEvent
    assert_eq!(
        sounds[6].target,
        SoundTarget::Entity {
            entity_id: 202,
            position: None
        }
    );
    assert_eq!(sounds[6].source, SoundSource::Neutral);
    assert_eq!(sounds[6].except_player, None);

    // 7: 6-arg Entity Holder (direct) with except=player, target=player
    assert_eq!(
        sounds[7].target,
        SoundTarget::Entity {
            entity_id: 101,
            position: None
        }
    );
    assert_eq!(sounds[7].holder, Some(SoundHolderKind::Direct));
    assert_eq!(sounds[7].source, SoundSource::Voice);
    assert_eq!(sounds[7].except_player, Some(uuid));

    // 8-10: playSeededSound -> exact explicit seeds
    assert_eq!(sounds[8].seed, SoundSeed::Explicit(777));
    assert_eq!(sounds[8].source, SoundSource::Music);
    assert_eq!(sounds[8].except_player, Some(uuid));
    assert_eq!(sounds[9].seed, SoundSeed::Explicit(888));
    assert_eq!(sounds[9].source, SoundSource::Master);
    assert_eq!(sounds[9].except_player, None);
    assert_eq!(sounds[10].seed, SoundSeed::Explicit(999));
    assert_eq!(sounds[10].source, SoundSource::Ui);
    assert_eq!(
        sounds[10].target,
        SoundTarget::Entity {
            entity_id: 202,
            position: None
        }
    );
}

#[test]
fn client_level_dispatch_except_distance_delay_and_rng_streams() {
    let sources = with_stubs(&[(
        "demoprobe/Probe.java",
        r#"package demoprobe;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.Entity;

public final class Probe {
    public void onInitialize() {}

    public static void exerciseClient(ClientLevel level, Entity localPlayer, Entity other) {
        SoundEvent ping = SoundEvent.createVariableRangeEvent(
            Identifier.fromNamespaceAndPath("demo", "ping")
        );
        Holder<SoundEvent> pingHolder = Holder.direct(ping);

        // 1. playSound with except=null (not localPlayer): no-op, YET STILL advances soundSeedGenerator (#0)!
        level.playSound(null, 1.0, 2.0, 3.0, ping, SoundSource.BLOCKS, 1.0f, 1.0f);

        // 2. playSound with except=other (not localPlayer): no-op, STILL advances soundSeedGenerator (#1)!
        level.playSound(other, new BlockPos(4, 5, 6), ping, SoundSource.BLOCKS, 1.0f, 1.0f);

        // 3. playSound with except=localPlayer: PLAYS with soundSeedGenerator (#2)!
        level.playSound(localPlayer, new BlockPos(10, 20, 30), ping, SoundSource.BLOCKS, 0.8f, 1.2f);

        // 4. playSeededSound with except=other: no-op, advances NEITHER RNG!
        level.playSeededSound(other, 1.0, 2.0, 3.0, pingHolder, SoundSource.AMBIENT, 1.0f, 1.0f, 555L);

        // 5. playSeededSound with except=localPlayer: PLAYS with explicit seed 666L!
        level.playSeededSound(localPlayer, localPlayer, pingHolder, SoundSource.PLAYERS, 0.7f, 0.9f, 666L);

        // 6. playLocalSound at distance 10 (d2=100.0 <= 100.0 -> delay 0), draws Level.random (#0)!
        level.playLocalSound(10.0, 0.0, 0.0, ping, SoundSource.WEATHER, 1.0f, 1.0f, true);

        // 7. playLocalSound at distance 40 (d2=1600.0 -> delay = (int)((40/40)*20) = 20), draws Level.random (#1)!
        level.playLocalSound(40.0, 0.0, 0.0, ping, SoundSource.WEATHER, 2.0f, 0.5f, true);

        // 8. playLocalSound with distanceDelay=false at distance 40 -> delay 0, draws Level.random (#2)!
        level.playLocalSound(40.0, 0.0, 0.0, ping, SoundSource.BLOCKS, 1.0f, 1.0f, false);

        // 9. playLocalSound(BlockPos) -> +0.5 centre, d2 = (19.5)^2 + (0.5)^2 + (0.5)^2 = 380.75 -> delay = (int)((19.5128/40)*20)=9, Level.random (#3)
        level.playLocalSound(new BlockPos(19, 0, 0), ping, SoundSource.BLOCKS, 1.0f, 1.0f, true);

        // 10. playLocalSound(Entity) -> EntityBoundSoundInstance, draws Level.random (#4)!
        level.playLocalSound(localPlayer, ping, SoundSource.NEUTRAL, 0.4f, 1.3f);

        // 11. playPlayerSound -> EntityBoundSoundInstance on localPlayer, draws Level.random (#5)!
        level.playPlayerSound(ping, SoundSource.PLAYERS, 0.5f, 1.5f);
    }

    public static long exerciseNullPlayer(ClientLevel level) {
        SoundEvent ping = SoundEvent.createVariableRangeEvent(
            Identifier.fromNamespaceAndPath("demo", "ping")
        );
        // With minecraft.player == null: playPlayerSound returns early WITHOUT drawing Level.random!
        level.playPlayerSound(ping, SoundSource.PLAYERS, 1.0f, 1.0f);
        // With minecraft.player == null: playSeededSound(null, ...) matches `null == null` and plays!
        level.playSeededSound(null, 5.0, 6.0, 7.0, ping, SoundSource.BLOCKS, 1.0f, 1.0f, 321L);
        return level.getRandom().nextLong();
    }
}"#,
    )]);
    let fixture = Fixture::build("level_sound_client_dispatch", &sources);
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    host.seed_client_level_randoms(987_654_321, 42);

    let snapshot = Arc::new(ClientSnapshot {
        level: Some(ClientLevel {
            dimension: "minecraft:overworld".into(),
            ..ClientLevel::default()
        }),
        player: Some(ClientPlayer {
            uuid: 1,
            name: "Local".into(),
            entity_id: 55,
            pos: [3.0, 64.0, -2.0],
            silent: Some(false),
            ..ClientPlayer::default()
        }),
        camera: Some(ClientCamera {
            pos: [0.0, 0.0, 0.0],
            ..ClientCamera::default()
        }),
        ..ClientSnapshot::default()
    });
    host.begin_client_frame(snapshot, &[]);

    let client_level = StdHost::client_level_handle();
    let local_player = StdHost::client_player_handle();
    let other_entity = entity_handle(999);

    fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "exerciseClient",
            "(Lnet/minecraft/client/multiplayer/ClientLevel;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity;)V",
            None,
            &[client_level.clone(), local_player, other_entity],
            &mut host,
        )
        .expect("client sound dispatch succeeds");

    let frame_out = host.end_client_frame();
    assert_eq!(host.unknown_host_call_count(), 0);
    assert_eq!(
        frame_out.sounds.len(),
        8,
        "8 client sounds expected: {:#?}",
        frame_out.sounds
    );

    // Sound 0: playSound(localPlayer, BlockPos(10,20,30)) -> seed #2 from soundSeedGenerator(42)!
    assert_eq!(frame_out.sounds[0].side, SoundSide::Client);
    assert_eq!(
        frame_out.sounds[0].target,
        SoundTarget::Position([10.5, 20.5, 30.5])
    );
    assert_eq!(
        frame_out.sounds[0].seed,
        SoundSeed::LevelSeedGenerator(SEED_42_LONGS[2])
    );

    // Sound 1: playSeededSound(localPlayer, localPlayer, ..., 666L)
    assert_eq!(
        frame_out.sounds[1].target,
        SoundTarget::Entity {
            entity_id: 55,
            position: Some([3.0, 64.0, -2.0]),
        }
    );
    assert_eq!(frame_out.sounds[1].seed, SoundSeed::Explicit(666));

    // Sound 2: playLocalSound at d2=100.0 -> delay 0, Level.random #0
    assert!(frame_out.sounds[2].distance_delay);
    assert_eq!(frame_out.sounds[2].delay_ticks, 0);
    assert_eq!(
        frame_out.sounds[2].seed,
        SoundSeed::LevelRandom(SEED_987654321_LONGS[0])
    );

    // Sound 3: playLocalSound at d2=1600.0 -> delay 20, Level.random #1
    assert!(frame_out.sounds[3].distance_delay);
    assert_eq!(frame_out.sounds[3].delay_ticks, 20);
    assert_eq!(
        frame_out.sounds[3].seed,
        SoundSeed::LevelRandom(SEED_987654321_LONGS[1])
    );

    // Sound 4: playLocalSound distanceDelay=false -> delay 0, Level.random #2
    assert!(!frame_out.sounds[4].distance_delay);
    assert_eq!(frame_out.sounds[4].delay_ticks, 0);
    assert_eq!(
        frame_out.sounds[4].seed,
        SoundSeed::LevelRandom(SEED_987654321_LONGS[2])
    );

    // Sound 5: playLocalSound(BlockPos(19,0,0), ..., true) -> centred + delay 9
    assert_eq!(
        frame_out.sounds[5].target,
        SoundTarget::Position([19.5, 0.5, 0.5])
    );
    assert!(frame_out.sounds[5].distance_delay);
    assert_eq!(frame_out.sounds[5].delay_ticks, 9);
    assert!(matches!(
        frame_out.sounds[5].seed,
        SoundSeed::LevelRandom(_)
    ));

    // Sound 6: playLocalSound(localPlayer, ...) -> entity-bound
    assert_eq!(
        frame_out.sounds[6].target,
        SoundTarget::Entity {
            entity_id: 55,
            position: Some([3.0, 64.0, -2.0]),
        }
    );
    assert_eq!(frame_out.sounds[6].source, SoundSource::Neutral);
    assert_eq!(frame_out.sounds[6].volume, 0.4);
    assert_eq!(frame_out.sounds[6].pitch, 1.3);
    assert!(matches!(
        frame_out.sounds[6].seed,
        SoundSeed::LevelRandom(_)
    ));

    // Sound 7: playPlayerSound -> EntityBoundSoundInstance on localPlayer
    assert_eq!(
        frame_out.sounds[7].target,
        SoundTarget::Entity {
            entity_id: 55,
            position: Some([3.0, 64.0, -2.0]),
        }
    );
    assert_eq!(frame_out.sounds[7].source, SoundSource::Players);
    assert!(matches!(
        frame_out.sounds[7].seed,
        SoundSeed::LevelRandom(_)
    ));

    // Now test minecraft.player == null frame:
    host.seed_client_level_randoms(123_456_789, 42);
    let null_player_frame = Arc::new(ClientSnapshot {
        level: Some(ClientLevel {
            dimension: "minecraft:overworld".into(),
            ..ClientLevel::default()
        }),
        player: None,
        ..ClientSnapshot::default()
    });
    host.begin_client_frame(null_player_frame, &[]);
    let next_long = fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "exerciseNullPlayer",
            "(Lnet/minecraft/client/multiplayer/ClientLevel;)J",
            None,
            &[client_level],
            &mut host,
        )
        .expect("null-player client frame succeeds");
    let null_out = host.end_client_frame();
    // playPlayerSound did NOT draw Level.random when player == null!
    assert_eq!(next_long, Value::Long(SEED_123456789_LONG_0));
    // playSeededSound(null, ...) matched `null == null` and emitted 1 sound!
    assert_eq!(null_out.sounds.len(), 1);
    assert_eq!(null_out.sounds[0].seed, SoundSeed::Explicit(321));
    assert_eq!(
        null_out.sounds[0].target,
        SoundTarget::Position([5.0, 6.0, 7.0])
    );
}

#[test]
fn entity_player_and_local_player_play_sound_and_sound_source() {
    let sources = with_stubs(&[(
        "demoprobe/Probe.java",
        r#"package demoprobe;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.entity.Entity;

public final class Probe {
    public void onInitialize() {}

    public static void exerciseServerEntities(
        Entity loudMob,
        Entity silentMob,
        ServerPlayer loudPlayer,
        ServerPlayer silentPlayer
    ) {
        SoundEvent step = SoundEvent.createVariableRangeEvent(
            Identifier.fromNamespaceAndPath("demo", "step")
        );
        // Loud mob: both 1-arg and 3-arg play
        loudMob.playSound(step);
        loudMob.playSound(step, 0.5f, 1.5f);
        // Silent mob: both 1-arg and 3-arg are silent no-ops (no seed draw)
        silentMob.playSound(step);
        silentMob.playSound(step, 0.5f, 1.5f);
        // Loud player: both 1-arg and 3-arg play with except = player
        loudPlayer.playSound(step);
        loudPlayer.playSound(step, 0.7f, 1.1f);
        // Silent player: 1-arg checks !isSilent() -> NO-OP, 3-arg ignores isSilent() -> PLAYS!
        silentPlayer.playSound(step);
        silentPlayer.playSound(step, 0.8f, 0.9f);
    }

    public static void exerciseLocalPlayer(LocalPlayer silentLocal) {
        SoundEvent click = SoundEvent.createVariableRangeEvent(
            Identifier.fromNamespaceAndPath("demo", "click")
        );
        // Inherited Entity.playSound(SoundEvent) checks !isSilent() -> NO-OP
        silentLocal.playSound(click);
        // LocalPlayer.playSound(SoundEvent, float, float) ignores isSilent() -> PLAYS via playLocalSound!
        silentLocal.playSound(click, 0.6f, 1.4f);
    }

    public static String checkRabbitSource(Entity rabbit) {
        return String.valueOf(rabbit.getSoundSource());
    }
}"#,
    )]);
    let fixture = Fixture::build("level_sound_entities", &sources);
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);

    let loud_mob = entity_handle(10);
    let silent_mob = entity_handle(11);
    let loud_uuid = 0xaa_u128;
    let silent_uuid = 0xbb_u128;
    let loud_player = player_handle(loud_uuid, 20);
    let silent_player = player_handle(silent_uuid, 21);

    host.set_entity_sound_facts(
        "game/entity/10",
        EntitySoundFacts {
            entity_id: Some(10),
            level: Some("minecraft:overworld".into()),
            position: Some([1.0, 2.0, 3.0]),
            silent: Some(false),
            sound_source: Some(SoundSource::Hostile),
            is_player: false,
            player_uuid: None,
        },
    );
    host.set_entity_sound_facts(
        "game/entity/11",
        EntitySoundFacts {
            entity_id: Some(11),
            level: Some("minecraft:overworld".into()),
            position: Some([4.0, 5.0, 6.0]),
            silent: Some(true),
            sound_source: Some(SoundSource::Neutral),
            is_player: false,
            player_uuid: None,
        },
    );
    host.set_entity_sound_facts(
        format!("game/player/{loud_uuid:032x}/1.0/20"),
        EntitySoundFacts {
            entity_id: Some(20),
            level: Some("minecraft:overworld".into()),
            position: Some([7.0, 8.0, 9.0]),
            silent: Some(false),
            sound_source: Some(SoundSource::Players),
            is_player: true,
            player_uuid: Some(loud_uuid),
        },
    );
    host.set_entity_sound_facts(
        format!("game/player/{silent_uuid:032x}/1.0/21"),
        EntitySoundFacts {
            entity_id: Some(21),
            level: Some("minecraft:overworld".into()),
            position: Some([10.0, 11.0, 12.0]),
            silent: Some(true),
            sound_source: Some(SoundSource::Players),
            is_player: true,
            player_uuid: Some(silent_uuid),
        },
    );

    fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "exerciseServerEntities",
            "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/server/level/ServerPlayer;Lnet/minecraft/server/level/ServerPlayer;)V",
            None,
            &[loud_mob, silent_mob, loud_player, silent_player],
            &mut host,
        )
        .expect("server entity playSound succeeds");

    assert_eq!(host.unknown_host_call_count(), 0);
    let sounds = host.take_played_sounds();
    assert_eq!(
        sounds.len(),
        5,
        "loudMob(2) + loudPlayer(2) + silentPlayer 3-arg(1)"
    );

    assert_eq!(sounds[0].source, SoundSource::Hostile);
    assert_eq!(sounds[0].except_player, None);
    assert_eq!(sounds[0].volume, 1.0);
    assert_eq!(
        sounds[0].seed,
        SoundSeed::LevelSeedGenerator(SEED_42_LONGS[0])
    );

    assert_eq!(sounds[1].source, SoundSource::Hostile);
    assert_eq!(sounds[1].volume, 0.5);
    assert_eq!(sounds[1].pitch, 1.5);
    assert_eq!(
        sounds[1].seed,
        SoundSeed::LevelSeedGenerator(SEED_42_LONGS[1])
    );

    assert_eq!(sounds[2].source, SoundSource::Players);
    assert_eq!(sounds[2].except_player, Some(loud_uuid));
    assert_eq!(
        sounds[2].seed,
        SoundSeed::LevelSeedGenerator(SEED_42_LONGS[2])
    );

    assert_eq!(sounds[3].source, SoundSource::Players);
    assert_eq!(sounds[3].except_player, Some(loud_uuid));
    assert_eq!(sounds[3].volume, 0.7);

    // Silent ServerPlayer 3-arg played; 1-arg was skipped!
    assert_eq!(sounds[4].source, SoundSource::Players);
    assert_eq!(sounds[4].except_player, Some(silent_uuid));
    assert_eq!(sounds[4].target, SoundTarget::Position([10.0, 11.0, 12.0]));
    assert_eq!(sounds[4].volume, 0.8);
    assert_eq!(sounds[4].pitch, 0.9);

    // Silent LocalPlayer inside a client frame: 1-arg silent no-op, 3-arg plays using Level.random!
    host.seed_client_level_random(987_654_321);
    host.begin_client_frame(
        Arc::new(ClientSnapshot {
            level: Some(ClientLevel {
                dimension: "minecraft:overworld".into(),
                ..ClientLevel::default()
            }),
            player: Some(ClientPlayer {
                uuid: 1,
                name: "Local".into(),
                entity_id: 30,
                pos: [2.5, 65.0, -4.5],
                silent: Some(true),
                ..ClientPlayer::default()
            }),
            ..ClientSnapshot::default()
        }),
        &[],
    );
    fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "exerciseLocalPlayer",
            "(Lnet/minecraft/client/player/LocalPlayer;)V",
            None,
            &[StdHost::client_player_handle()],
            &mut host,
        )
        .expect("local player playSound succeeds");
    let out = host.end_client_frame();
    assert_eq!(out.sounds.len(), 1);
    assert_eq!(out.sounds[0].side, SoundSide::Client);
    assert_eq!(out.sounds[0].source, SoundSource::Players);
    assert_eq!(
        out.sounds[0].target,
        SoundTarget::Position([2.5, 65.0, -4.5])
    );
    assert_eq!(out.sounds[0].volume, 0.6);
    assert_eq!(out.sounds[0].pitch, 1.4);
    assert_eq!(
        out.sounds[0].seed,
        SoundSeed::LevelRandom(SEED_987654321_LONGS[0])
    );

    // Rabbit without explicit sound_source must COUNT (vanilla depends on getVariant() == EVIL).
    let mut lenient_host = StdHost::new();
    let rabbit = entity_handle(77);
    let _ = fixture.jar.invoke_method_with_args(
        "demoprobe/Probe",
        "checkRabbitSource",
        "(Lnet/minecraft/world/entity/Entity;)Ljava/lang/String;",
        None,
        &[rabbit],
        &mut lenient_host,
    );
    assert!(
        lenient_host.unknown_host_call_count() > 0,
        "rabbit without variant/sound_source must be counted as unknown"
    );
}

#[test]
fn sound_event_variable_vs_fixed_range_and_holders() {
    let sources = with_stubs(&[(
        "demoprobe/Probe.java",
        r#"package demoprobe;
import java.util.Optional;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.level.Level;

public final class Probe {
    public void onInitialize() {}

    public static String exerciseRanges(Level level) {
        Identifier varId = Identifier.fromNamespaceAndPath("demo", "var");
        Identifier fixId = Identifier.fromNamespaceAndPath("demo", "fix");
        Identifier recId = Identifier.fromNamespaceAndPath("demo", "rec");

        SoundEvent varEvent = SoundEvent.createVariableRangeEvent(varId);
        SoundEvent fixEvent = SoundEvent.createFixedRangeEvent(fixId, 32.0f);
        SoundEvent recFixed = new SoundEvent(recId, Optional.of(48.0f));
        SoundEvent recVar = new SoundEvent(varId, Optional.empty());

        Holder.Reference<SoundEvent> regRef = Registry.registerForHolder(
            BuiltInRegistries.SOUND_EVENT,
            fixId,
            fixEvent
        );
        Holder.Reference<SoundEvent> unbound = Holder.Reference.createStandAlone(
            BuiltInRegistries.SOUND_EVENT,
            ResourceKey.create(Registries.SOUND_EVENT, Identifier.fromNamespaceAndPath("demo", "unbound"))
        );

        level.playSound(null, 1.0, 2.0, 3.0, varEvent, SoundSource.BLOCKS, 2.0f, 1.0f);
        level.playSound(null, 4.0, 5.0, 6.0, regRef, SoundSource.BLOCKS, 0.5f, 1.0f);
        level.playSound(null, 7.0, 8.0, 9.0, recFixed, SoundSource.BLOCKS, 1.0f, 1.0f);

        boolean unboundSeededThrew = false;
        try {
            level.playSeededSound(null, 0.0, 0.0, 0.0, unbound, SoundSource.BLOCKS, 1.0f, 1.0f, 99L);
        } catch (IllegalStateException expected) {
            unboundSeededThrew = true;
        }
        boolean unboundPlayThrew = false;
        try {
            level.playSound(null, 0.0, 0.0, 0.0, unbound, SoundSource.BLOCKS, 1.0f, 1.0f);
        } catch (IllegalStateException expected) {
            unboundPlayThrew = true;
        }

        // Follow-up sound proves unboundPlayThrew drew soundSeedGenerator (#3) before throwing,
        // while unboundSeededThrew drew NOTHING!
        level.playSound(null, 10.0, 11.0, 12.0, recVar, SoundSource.BLOCKS, 1.0f, 1.0f);

        return varEvent.getRange(0.5f) + "," + varEvent.getRange(2.0f) + ","
            + fixEvent.getRange(0.5f) + "," + fixEvent.getRange(3.0f) + ","
            + recFixed.getRange(1.0f) + "," + recVar.equals(varEvent) + ","
            + unboundSeededThrew + "," + unboundPlayThrew;
    }
}"#,
    )]);
    let fixture = Fixture::build("level_sound_ranges", &sources);
    let mut host = StdHost::new().with_strict_unknown_host_calls();
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);
    let level = StdHost::server_level_handle("minecraft:overworld");

    let summary = fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "exerciseRanges",
            "(Lnet/minecraft/world/level/Level;)Ljava/lang/String;",
            None,
            &[level],
            &mut host,
        )
        .expect("range tests succeed");

    assert_eq!(
        summary,
        Value::Ref(Some("16.0,32.0,32.0,32.0,48.0,true,true,true".into()))
    );
    assert_eq!(host.unknown_host_call_count(), 0);

    let sounds = host.played_sounds();
    assert_eq!(sounds.len(), 4);
    assert_eq!(sounds[0].sound_id, "demo:var");
    assert_eq!(sounds[0].fixed_range, None);
    assert_eq!(sounds[0].range(), 32.0); // 16.0 * 2.0
    assert_eq!(sounds[0].holder, Some(SoundHolderKind::Direct));

    assert_eq!(sounds[1].sound_id, "demo:fix");
    assert_eq!(sounds[1].fixed_range, Some(32.0));
    assert_eq!(sounds[1].range(), 32.0);
    assert_eq!(
        sounds[1].holder,
        Some(SoundHolderKind::Reference {
            registry_id: "demo:fix".into()
        })
    );

    assert_eq!(sounds[2].sound_id, "demo:rec");
    assert_eq!(sounds[2].fixed_range, Some(48.0));
    assert_eq!(sounds[2].range(), 48.0);
    assert_eq!(
        sounds[2].seed,
        SoundSeed::LevelSeedGenerator(SEED_42_LONGS[2])
    );

    // sounds[3] was after unboundPlayThrew (which consumed draw #3), so it must NOT equal SEED_42_LONGS[2]
    assert_eq!(sounds[3].sound_id, "demo:var");
    assert_eq!(sounds[3].fixed_range, None);
    assert!(matches!(sounds[3].seed, SoundSeed::LevelSeedGenerator(_)));
    assert_ne!(sounds[3].seed, sounds[2].seed);
}

#[test]
fn unmodelled_sound_calls_and_missing_facts_are_counted() {
    let sources = with_stubs(&[
        (
            "demoprobe/DynamicBlockPos.java",
            r#"package demoprobe;
import net.minecraft.core.BlockPos;
public final class DynamicBlockPos extends BlockPos {
    public DynamicBlockPos(int x, int y, int z) { super(x, y, z); }
    @Override
    public int getX() { return super.getX() + 100; }
}"#,
        ),
        (
            "demoprobe/Probe.java",
            r#"package demoprobe;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.level.Level;

public final class Probe {
    public void onInitialize() {}

    public static void unseededServer(Level level) {
        SoundEvent s = SoundEvent.createVariableRangeEvent(Identifier.fromNamespaceAndPath("demo", "a"));
        level.playSound(null, 1.0, 2.0, 3.0, s, SoundSource.BLOCKS, 1.0f, 1.0f);
    }

    public static void unmodelledOverload(Level level) {
        SoundEvent s = SoundEvent.createVariableRangeEvent(Identifier.fromNamespaceAndPath("demo", "a"));
        level.playSound(1.0, 2.0, 3.0, s);
    }

    public static void overriddenBlockPos(Level level) {
        SoundEvent s = SoundEvent.createVariableRangeEvent(Identifier.fromNamespaceAndPath("demo", "a"));
        level.playSound(null, new DynamicBlockPos(1, 2, 3), s, SoundSource.BLOCKS, 1.0f, 1.0f);
    }

    public static void clientMissingCamera(ClientLevel level) {
        SoundEvent s = SoundEvent.createVariableRangeEvent(Identifier.fromNamespaceAndPath("demo", "a"));
        level.playLocalSound(1.0, 2.0, 3.0, s, SoundSource.BLOCKS, 1.0f, 1.0f, true);
    }

    public static void clientUnseededLocal(ClientLevel level) {
        SoundEvent s = SoundEvent.createVariableRangeEvent(Identifier.fromNamespaceAndPath("demo", "a"));
        level.playLocalSound(1.0, 2.0, 3.0, s, SoundSource.BLOCKS, 1.0f, 1.0f, false);
    }
}"#,
        ),
    ]);
    let fixture = Fixture::build("level_sound_counted", &sources);
    let level = StdHost::server_level_handle("minecraft:overworld");
    let client_level = StdHost::client_level_handle();

    // 1. Unseeded server soundSeedGenerator -> counted, no sound recorded
    let mut host = StdHost::new();
    fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "unseededServer",
            "(Lnet/minecraft/world/level/Level;)V",
            None,
            std::slice::from_ref(&level),
            &mut host,
        )
        .unwrap();
    assert_eq!(host.played_sounds().len(), 0);
    assert_eq!(host.unknown_host_call_count(), 1);

    // 2. Unmodelled overload signature -> counted
    let mut host = StdHost::new();
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);
    fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "unmodelledOverload",
            "(Lnet/minecraft/world/level/Level;)V",
            None,
            std::slice::from_ref(&level),
            &mut host,
        )
        .unwrap();
    assert_eq!(host.played_sounds().len(), 0);
    assert_eq!(host.unknown_host_call_count(), 1);

    // 3. Overridden BlockPos.getX() -> counted, never reads raw field
    let mut host = StdHost::new();
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);
    fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "overriddenBlockPos",
            "(Lnet/minecraft/world/level/Level;)V",
            None,
            std::slice::from_ref(&level),
            &mut host,
        )
        .unwrap();
    assert_eq!(host.played_sounds().len(), 0);
    assert_eq!(host.unknown_host_call_count(), 1);

    // 4. ClientLevel.playLocalSound with distanceDelay=true and camera == None -> counted
    let mut host = StdHost::new();
    host.seed_client_level_random(42);
    host.begin_client_frame(
        Arc::new(ClientSnapshot {
            level: Some(ClientLevel {
                dimension: "minecraft:overworld".into(),
                ..ClientLevel::default()
            }),
            camera: None,
            ..ClientSnapshot::default()
        }),
        &[],
    );
    let err = fixture.jar.invoke_method_with_args(
        "demoprobe/Probe",
        "clientMissingCamera",
        "(Lnet/minecraft/client/multiplayer/ClientLevel;)V",
        None,
        std::slice::from_ref(&client_level),
        &mut host,
    );
    assert!(matches!(err, Err(NativeError::UnknownHostCall(_))));
    let out = host.end_client_frame();
    assert!(out.sounds.is_empty());
    assert_eq!(host.unknown_host_call_count(), 1);

    // 5. ClientLevel.playLocalSound without seeded Level.random -> counted
    let mut host = StdHost::new();
    fixture
        .jar
        .invoke_method_with_args(
            "demoprobe/Probe",
            "clientUnseededLocal",
            "(Lnet/minecraft/client/multiplayer/ClientLevel;)V",
            None,
            &[client_level],
            &mut host,
        )
        .unwrap();
    assert_eq!(host.played_sounds().len(), 0);
    assert_eq!(host.unknown_host_call_count(), 1);
}

#[test]
fn server_event_firing_and_game_hook_record_sounds() {
    let sources = with_stubs(&[
        (
            "net/fabricmc/fabric/api/event/lifecycle/v1/ServerTickEvents.java",
            r#"package net.fabricmc.fabric.api.event.lifecycle.v1;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.server.level.ServerLevel;
public final class ServerTickEvents {
    public static Event<StartLevelTick> START_LEVEL_TICK;
    public interface StartLevelTick { void onStartTick(ServerLevel level); }
}"#,
        ),
        (
            "net/fabricmc/fabric/api/event/Event.java",
            r#"package net.fabricmc.fabric.api.event;
public abstract class Event<T> { public abstract void register(T listener); }"#,
        ),
        (
            "org/spongepowered/asm/mixin/Mixin.java",
            r#"package org.spongepowered.asm.mixin;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
public @interface Mixin { Class<?>[] value(); }"#,
        ),
        (
            "org/spongepowered/asm/mixin/injection/Inject.java",
            r#"package org.spongepowered.asm.mixin.injection;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
public @interface Inject { String[] method(); At[] at(); boolean cancellable() default false; }"#,
        ),
        (
            "org/spongepowered/asm/mixin/injection/At.java",
            r#"package org.spongepowered.asm.mixin.injection;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
public @interface At { String value(); }"#,
        ),
        (
            "org/spongepowered/asm/mixin/injection/callback/CallbackInfo.java",
            r#"package org.spongepowered.asm.mixin.injection.callback;
public class CallbackInfo { public void cancel() {} }"#,
        ),
        (
            "demoprobe/Probe.java",
            r#"package demoprobe;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;

public final class Probe {
    public void onInitialize() {
        SoundEvent tickSound = SoundEvent.createVariableRangeEvent(
            Identifier.fromNamespaceAndPath("demo", "tick")
        );
        ServerTickEvents.START_LEVEL_TICK.register(level -> {
            level.playSound(null, 1.0, 2.0, 3.0, tickSound, SoundSource.BLOCKS, 1.0f, 1.0f);
        });
    }
}"#,
        ),
        (
            "demo/PlayerSoundMixin.java",
            r#"package demo;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.entity.player.Player;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Player.class)
public class PlayerSoundMixin {
    @Inject(method = "tick", at = @At("HEAD"))
    private void onTick(CallbackInfo ci) {
        SoundEvent chime = SoundEvent.createVariableRangeEvent(
            Identifier.fromNamespaceAndPath("demo", "chime")
        );
        ((Player) (Object) this).playSound(chime, 0.75f, 1.25f);
    }
}"#,
        ),
    ]);
    let mod_json = r#"{"id":"demo","version":"1","entrypoints":{"main":["demoprobe.Probe"]},"mixins":["demo.mixins.json"]}"#;
    let mixins_json = br#"{"package":"demo","mixins":["PlayerSoundMixin"]}"#;
    let fixture = Fixture::build_with_mod_json_and_extra(
        "level_sound_event_and_hook",
        &sources,
        mod_json,
        &[("demo.mixins.json", mixins_json)],
    );
    let mut host = StdHost::new();
    for ep in fixture.jar.discover_entrypoints("main") {
        fixture
            .jar
            .execute(&ep.class, ep.method.as_deref(), "main", &mut host)
            .expect("entrypoint registers tick listener");
    }
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);

    // 1. EventFiring path
    let listeners = host.game_listeners(&fixture.jar, false);
    assert_eq!(listeners.listeners.len(), 1);
    let facade = Arc::new(EventFacade {
        levels: vec![FacadeLevel {
            id: "minecraft:overworld".into(),
            game_time: 10,
            day_time: 10,
            world_border: None,
        }],
        ..EventFacade::default()
    });
    let mut firing = EventFiring::new(
        GameEvent::LevelTickStart,
        facade,
        EventContext {
            level: Some("minecraft:overworld".into()),
            has_time: Some(true),
            entity_sound_facts: BTreeMap::new(),
            ..EventContext::default()
        },
    );
    fixture
        .jar
        .run_game_listener(
            &listeners.listeners[0],
            &mut firing,
            &mut host,
            &test_budget(),
        )
        .expect("tick listener runs");
    assert_eq!(firing.sounds.len(), 1);
    let tick = firing.sounds[0].effect().expect("a played sound");
    assert_eq!(tick.sound_id, "demo:tick");
    assert_eq!(tick.seed, SoundSeed::LevelSeedGenerator(SEED_42_LONGS[0]));

    // 2. GameHookContext path
    let weave = fixture
        .jar
        .plan_load_time_weave_with_game_hooks(horizon_native_java::MixinSide::Server);
    let hooks = fixture
        .jar
        .register_game_hooks(&weave)
        .expect("register hooks");
    let mut hook_state = horizon_native_java::GameHookState::default();
    let mut context = GameHookContext::new();
    let uuid = 0x55_u128;
    let receiver = context.insert(
        GameObject::player(uuid, "Alex")
            .with_entity_id(7)
            .with_incarnation(1, 0)
            .with_dimension("minecraft:overworld")
            .with_position([5.0, 64.0, -3.0])
            .with_silent(false),
    );
    fixture
        .jar
        .run_game_hook_in(
            &hooks,
            &mut hook_state,
            &mut context,
            "java_base_player_tick_head",
            Some(receiver),
            &[],
            &mut host,
            &test_budget(),
        )
        .expect("player tick hook runs");
    let effects = context.take_effects();
    assert_eq!(
        effects,
        vec![GameHookEffect::PlaySound(LevelSoundEffect {
            side: SoundSide::Server,
            dimension: Some("minecraft:overworld".into()),
            sound_id: "demo:chime".into(),
            fixed_range: None,
            holder: Some(SoundHolderKind::Direct),
            target: SoundTarget::Position([5.0, 64.0, -3.0]),
            source: SoundSource::Players,
            volume: 0.75,
            pitch: 1.25,
            except_player: Some(uuid),
            seed: SoundSeed::LevelSeedGenerator(SEED_42_LONGS[1]),
            distance_delay: false,
            delay_ticks: 0,
        })]
    );
}

#[test]
fn runes_jar_crafting_sound_event_resolves_and_plays() {
    let Some(jar_path) = std::env::var_os("RUNES_JAR").map(PathBuf::from) else {
        eprintln!("skipping runes_jar_crafting_sound_event_resolves_and_plays: RUNES_JAR unset");
        return;
    };
    std::thread::Builder::new()
        .name("runes-level-sound".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || run_runes_crafting_sound_test(jar_path))
        .expect("spawn runes sound thread")
        .join()
        .expect("runes sound test thread panicked");
}

fn run_runes_crafting_sound_test(jar_path: PathBuf) {
    let runes = NativeJar::from_path(&jar_path).expect("parse RUNES_JAR");

    // Compile a probe linking against the real Runes jar that reads `RuneCrafting.SOUND`,
    // verifies its registered range semantics, and replays the exact `Level.playSound` call
    // shape from `RuneCraftingScreenHandler.onTake` (live `onTake` itself casts `player` to
    // the `RuneCrafter` interface mixin from `PlayerEntityMixin`, which awaits interface
    // mixins + the menu bridge).
    let probe_sources = with_stubs(&[
        (
            "net/runes/crafting/RuneCrafting.java",
            r#"package net.runes.crafting;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
public class RuneCrafting {
    public static Identifier ID;
    public static SoundEvent SOUND;
}"#,
        ),
        (
            "demoprobe/Probe.java",
            r#"package demoprobe;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.runes.crafting.RuneCrafting;

public final class Probe {
    public void onInitialize() {}

    public static String playRunesCrafting(Level world, Player player) {
        SoundEvent sound = RuneCrafting.SOUND;
        Holder<SoundEvent> holder = BuiltInRegistries.SOUND_EVENT.wrapAsHolder(sound);
        if (!world.isClientSide()) {
            world.playSound(
                null,
                player.getX(),
                player.getY(),
                player.getZ(),
                sound,
                SoundSource.BLOCKS,
                world.getRandom().nextFloat() * 0.1f + 0.9f,
                1.0f
            );
        }
        return sound.location() + "|" + sound.fixedRange().isPresent() + "|"
            + sound.getRange(1.0f) + "|" + sound.getRange(2.0f) + "|" + holder.isBound();
    }
}"#,
        ),
    ]);
    let probe = Fixture::build("level_sound_runes_probe", &probe_sources);

    // Seed the host with Runes (and its nested jars), execute Runes's `main` entrypoint
    // (`RunesMod.registerSounds()` registers `runes:crafting`), then link `probe` so we can
    // exercise `RuneCrafting.SOUND` directly through the same initialized host.
    let mut host = runes.seed_host(StdHost::new().with_game_version("26.3"));
    let entrypoints = runes.discover_entrypoints("main");
    assert!(
        !entrypoints.is_empty(),
        "expected main entrypoints in RUNES_JAR"
    );
    for ep in &entrypoints {
        runes
            .execute(&ep.class, ep.method.as_deref(), "main", &mut host)
            .unwrap_or_else(|e| panic!("entrypoint {} failed: {e:?}", ep.class));
    }

    let entry = host
        .registrations()
        .iter()
        .find(|entry| entry.registry == "minecraft:sound_event" && entry.id == "runes:crafting")
        .expect("Runes registers runes:crafting in minecraft:sound_event")
        .clone();

    host.link_jar("probe.jar", &probe.jar)
        .expect("link probe jar");
    host.seed_server_level_randoms("minecraft:overworld", 42, 42);

    let uuid = 0xfeed_face_u128;
    let player = player_handle(uuid, 42);
    host.set_entity_sound_facts(
        format!("game/player/{uuid:032x}/1.0/42"),
        EntitySoundFacts {
            entity_id: Some(42),
            level: Some("minecraft:overworld".into()),
            position: Some([10.5, 64.0, -3.25]),
            silent: Some(false),
            sound_source: Some(SoundSource::Players),
            is_player: true,
            player_uuid: Some(uuid),
        },
    );
    let level = StdHost::server_level_handle("minecraft:overworld");

    let before_unknowns = host.unknown_host_call_count();
    let result = runes
        .invoke_method_with_args(
            "demoprobe/Probe",
            "playRunesCrafting",
            "(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;)Ljava/lang/String;",
            None,
            &[level, player],
            &mut host,
        )
        .expect("Runes crafting sound probe succeeds");

    assert_eq!(
        host.unknown_host_call_count(),
        before_unknowns,
        "playing runes:crafting must introduce zero unknown host calls"
    );
    assert_eq!(
        result,
        Value::Ref(Some("runes:crafting|false|16.0|32.0|true".into()))
    );

    let sounds = host.played_sounds();
    assert_eq!(sounds.len(), 1);
    let played = &sounds[0];
    assert_eq!(played.side, SoundSide::Server);
    assert_eq!(played.dimension.as_deref(), Some("minecraft:overworld"));
    assert_eq!(played.sound_id, "runes:crafting");
    assert_eq!(played.fixed_range, None);
    assert_eq!(played.range(), 16.0);
    assert_eq!(
        played.holder,
        Some(SoundHolderKind::Reference {
            registry_id: "runes:crafting".into()
        })
    );
    assert_eq!(played.target, SoundTarget::Position([10.5, 64.0, -3.25]));
    assert_eq!(played.source, SoundSource::Blocks);
    // Exact Java 25 `new Random(42L).nextFloat() * 0.1f + 0.9f` == 0x3f79_068f
    assert_eq!(played.volume.to_bits(), 0x3f79_068f);
    assert_eq!(played.pitch, 1.0);
    assert_eq!(played.except_player, None);
    assert_eq!(played.seed, SoundSeed::LevelSeedGenerator(SEED_42_LONGS[0]));
    // Sanity-check the registered entry matched `RuneCrafting.SOUND`
    assert_eq!(entry.id, "runes:crafting");
}

// Java 25 oracle, drawn with the real 26.3 classes from `server-26.3.jar`:
// `new ThreadSafeLegacyRandomSource(42)` (a `Level.soundSeedGenerator`) and
// `RandomSource.create(987654321)` (a `Level.random`), six `nextLong()` each.
const SOUND_SEEDS_42: [i64; 6] = [
    -5_025_562_857_975_149_833,
    -5_843_495_416_241_995_736,
    5_694_868_678_511_409_995,
    5_111_195_811_822_994_797,
    -6_169_532_649_852_302_182,
    -1_782_466_964_123_969_572,
];
const LEVEL_RANDOM_987654321: [i64; 6] = [
    5_643_591_630_821_615_871,
    -3_342_970_523_933_462_541,
    842_562_190_649_680_514,
    -5_427_006_930_497_216_603,
    179_179_319_810_873_421,
    -5_983_975_103_086_941_246,
];

const ODD_BLOCK_POS: &str = r#"package demoprobe;
import net.minecraft.core.BlockPos;
public final class OddBlockPos extends BlockPos {
    public OddBlockPos(int x, int y, int z) { super(x, y, z); }
    @Override
    public int getX() { return super.getX() + 100; }
}"#;

// Every call below that passes `null` throws a NullPointerException in the real game AFTER its
// seed draw (`Level.playSound` draws `soundSeedGenerator.nextLong()` before `playSeededSound`
// reads the sound, holder or target; `ClientLevel.playLocalSound` / `playPlayerSound` draw
// `Level.random.nextLong()` before the sound instance reads the sound or the entity). Decaf
// counts those calls instead of modelling the throw; either way the seed is gone.
const DRAW_ORDER_PROBE: &str = r#"package demoprobe;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;

public final class DrawOrder {
    public void onInitialize() {}

    static SoundEvent ok() {
        return SoundEvent.createVariableRangeEvent(Identifier.fromNamespaceAndPath("demo", "ok"));
    }

    // One call per unseeded overload: BlockPos, (DDD, SoundEvent), (DDD, Holder),
    // (Entity, SoundEvent), (Entity, Holder). Then a valid one.
    public static void unresolvableThenValid(Level level, Entity mob) {
        SoundEvent ok = ok();
        try { level.playSound(null, new BlockPos(1, 2, 3), (SoundEvent) null, SoundSource.BLOCKS, 1.0f, 1.0f); } catch (NullPointerException e) {}
        try { level.playSound(null, 1.0, 2.0, 3.0, (SoundEvent) null, SoundSource.BLOCKS, 1.0f, 1.0f); } catch (NullPointerException e) {}
        try { level.playSound(null, 1.0, 2.0, 3.0, (Holder<SoundEvent>) null, SoundSource.BLOCKS, 1.0f, 1.0f); } catch (NullPointerException e) {}
        try { level.playSound(null, (Entity) null, ok, SoundSource.NEUTRAL, 1.0f, 1.0f); } catch (NullPointerException e) {}
        try { level.playSound(null, mob, (Holder<SoundEvent>) null, SoundSource.NEUTRAL, 1.0f, 1.0f); } catch (NullPointerException e) {}
        level.playSound(null, 4.0, 5.0, 6.0, ok, SoundSource.BLOCKS, 1.0f, 1.0f);
    }

    public static void valid(Level level) {
        level.playSound(null, 4.0, 5.0, 6.0, ok(), SoundSource.BLOCKS, 1.0f, 1.0f);
    }

    public static void nullSound(Level level) {
        try { level.playSound(null, 1.0, 2.0, 3.0, (SoundEvent) null, SoundSource.BLOCKS, 1.0f, 1.0f); } catch (NullPointerException e) {}
    }

    public static void oddBlockPos(Level level) {
        level.playSound(null, new OddBlockPos(1, 2, 3), ok(), SoundSource.BLOCKS, 1.0f, 1.0f);
    }

    public static void entitySound(Entity entity) {
        entity.playSound(ok(), 1.0f, 1.0f);
    }

    public static void clientNullSound(ClientLevel level, Entity localPlayer) {
        level.playSound(localPlayer, 1.0, 2.0, 3.0, (SoundEvent) null, SoundSource.BLOCKS, 1.0f, 1.0f);
    }

    // `except` is not the local player: `ClientLevel.playSeededSound` returns before it reads
    // the sound, so this one does not throw in Java and is not counted here.
    public static void clientOtherNullSound(ClientLevel level, Entity other) {
        level.playSound(other, 1.0, 2.0, 3.0, (SoundEvent) null, SoundSource.BLOCKS, 1.0f, 1.0f);
    }

    public static void clientValid(ClientLevel level, Entity localPlayer) {
        level.playSound(localPlayer, 4.0, 5.0, 6.0, ok(), SoundSource.BLOCKS, 1.0f, 1.0f);
    }

    public static void clientNullLocal(ClientLevel level) {
        level.playLocalSound(1.0, 2.0, 3.0, (SoundEvent) null, SoundSource.BLOCKS, 1.0f, 1.0f, false);
    }

    public static void clientNullLocalEntity(ClientLevel level, Entity localPlayer) {
        level.playLocalSound(localPlayer, (SoundEvent) null, SoundSource.NEUTRAL, 1.0f, 1.0f);
    }

    public static void clientNullPlayerSound(ClientLevel level) {
        level.playPlayerSound((SoundEvent) null, SoundSource.PLAYERS, 1.0f, 1.0f);
    }

    public static void localPlayerNull(LocalPlayer player) {
        player.playSound((SoundEvent) null, 1.0f, 1.0f);
    }

    public static void clientValidLocal(ClientLevel level) {
        level.playLocalSound(7.0, 8.0, 9.0, ok(), SoundSource.BLOCKS, 1.0f, 1.0f, false);
    }

    public static void clientOddLocal(ClientLevel level) {
        level.playLocalSound(new OddBlockPos(1, 2, 3), ok(), SoundSource.BLOCKS, 1.0f, 1.0f, false);
    }

    public static long clientRandom(ClientLevel level) {
        return level.getRandom().nextLong();
    }
}"#;

const LEVEL_DESC: &str = "(Lnet/minecraft/world/level/Level;)V";
const CLIENT_LEVEL_DESC: &str = "(Lnet/minecraft/client/multiplayer/ClientLevel;)V";
const CLIENT_LEVEL_ENTITY_DESC: &str =
    "(Lnet/minecraft/client/multiplayer/ClientLevel;Lnet/minecraft/world/entity/Entity;)V";

fn draw_order_fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &with_stubs(&[
            ("demoprobe/OddBlockPos.java", ODD_BLOCK_POS),
            ("demoprobe/DrawOrder.java", DRAW_ORDER_PROBE),
        ]),
    )
}

fn draw_order_call(
    fixture: &Fixture,
    host: &mut StdHost,
    method: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Value, NativeError> {
    fixture
        .jar
        .invoke_method_with_args("demoprobe/DrawOrder", method, descriptor, None, args, host)
}

fn draw_order_client_frame() -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        level: Some(ClientLevel {
            dimension: "minecraft:overworld".into(),
            ..ClientLevel::default()
        }),
        player: Some(ClientPlayer {
            uuid: 1,
            name: "Local".into(),
            entity_id: 55,
            pos: [3.0, 64.0, -2.0],
            silent: Some(false),
            ..ClientPlayer::default()
        }),
        camera: Some(ClientCamera {
            pos: [0.0, 0.0, 0.0],
            ..ClientCamera::default()
        }),
        ..ClientSnapshot::default()
    })
}

#[test]
fn server_unseeded_overloads_draw_before_resolving_their_arguments() {
    let fixture = draw_order_fixture("level_sound_server_draw_order");
    let level = StdHost::server_level_handle("minecraft:overworld");
    // Lenient: a counted call returns and the probe goes on, as Java's `catch` does.
    let mut host = StdHost::new();
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);
    draw_order_call(
        &fixture,
        &mut host,
        "unresolvableThenValid",
        "(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/Entity;)V",
        &[level, entity_handle(202)],
    )
    .unwrap();
    // One counted call per unseeded overload, each AFTER its draw: the valid sound gets Java's
    // sixth `soundSeedGenerator` seed, not its first.
    assert_eq!(host.unknown_host_call_count(), 5);
    let sounds = host.take_played_sounds();
    assert_eq!(sounds.len(), 1, "{sounds:#?}");
    assert_eq!(sounds[0].sound_id, "demo:ok");
    assert_eq!(sounds[0].target, SoundTarget::Position([4.0, 5.0, 6.0]));
    assert_eq!(
        sounds[0].seed,
        SoundSeed::LevelSeedGenerator(SOUND_SEEDS_42[5])
    );
    // Every draw was Decaf's own: the stream's position is still known.
    assert!(host.sound_stream_known("game/level/minecraft:overworld"));
}

#[test]
fn client_unseeded_calls_draw_before_resolving_their_arguments() {
    let fixture = draw_order_fixture("level_sound_client_draw_order");
    let level = StdHost::client_level_handle();
    let me = StdHost::client_player_handle();
    let mut host = StdHost::new();
    host.seed_client_level_randoms(987_654_321, 42);
    host.begin_client_frame(draw_order_client_frame(), &[]);

    // `soundSeedGenerator` #0: a call that counts after its draw (a client frame refuses it).
    let counted = draw_order_call(
        &fixture,
        &mut host,
        "clientNullSound",
        CLIENT_LEVEL_ENTITY_DESC,
        &[level.clone(), me.clone()],
    );
    assert!(matches!(counted, Err(NativeError::UnknownHostCall(_))));
    // #1: `except` is another entity, so nothing reads the null sound: not counted, still drawn.
    draw_order_call(
        &fixture,
        &mut host,
        "clientOtherNullSound",
        CLIENT_LEVEL_ENTITY_DESC,
        &[level.clone(), entity_handle(999)],
    )
    .unwrap();
    // #2: the valid sound.
    draw_order_call(
        &fixture,
        &mut host,
        "clientValid",
        CLIENT_LEVEL_ENTITY_DESC,
        &[level.clone(), me.clone()],
    )
    .unwrap();

    // `Level.random` #0..#3: `playLocalSound(DDD)`, `playLocalSound(Entity)`,
    // `playPlayerSound` and `LocalPlayer.playSound`, each counted after its draw.
    for (method, descriptor, args) in [
        ("clientNullLocal", CLIENT_LEVEL_DESC, vec![level.clone()]),
        (
            "clientNullLocalEntity",
            CLIENT_LEVEL_ENTITY_DESC,
            vec![level.clone(), me.clone()],
        ),
        (
            "clientNullPlayerSound",
            CLIENT_LEVEL_DESC,
            vec![level.clone()],
        ),
        (
            "localPlayerNull",
            "(Lnet/minecraft/client/player/LocalPlayer;)V",
            vec![me.clone()],
        ),
    ] {
        let counted = draw_order_call(&fixture, &mut host, method, descriptor, &args);
        assert!(
            matches!(counted, Err(NativeError::UnknownHostCall(_))),
            "{method}: {counted:?}"
        );
    }
    // #4: the valid one.
    draw_order_call(
        &fixture,
        &mut host,
        "clientValidLocal",
        CLIENT_LEVEL_DESC,
        &[level],
    )
    .unwrap();

    let out = host.end_client_frame();
    assert_eq!(host.unknown_host_call_count(), 5);
    assert_eq!(out.sounds.len(), 2, "{:#?}", out.sounds);
    assert_eq!(
        out.sounds[0].seed,
        SoundSeed::LevelSeedGenerator(SOUND_SEEDS_42[2])
    );
    assert_eq!(out.sounds[0].target, SoundTarget::Position([4.0, 5.0, 6.0]));
    assert_eq!(
        out.sounds[1].seed,
        SoundSeed::LevelRandom(LEVEL_RANDOM_987654321[4])
    );
    assert_eq!(out.sounds[1].target, SoundTarget::Position([7.0, 8.0, 9.0]));
}

#[test]
fn unknown_sound_streams_count_every_later_draw_until_seeded_again() {
    let fixture = draw_order_fixture("level_sound_unknown_streams");
    let overworld = StdHost::server_level_handle("minecraft:overworld");
    let nether = StdHost::server_level_handle("minecraft:the_nether");
    let overworld_stream = "game/level/minecraft:overworld";
    let nether_stream = "game/level/minecraft:the_nether";

    // 1. A `BlockPos` Decaf cannot read (a jar override of `getX`) may or may not throw before
    //    the draw: the stream's position is unknown, and the next sound counts too.
    let mut host = StdHost::new();
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);
    draw_order_call(
        &fixture,
        &mut host,
        "oddBlockPos",
        LEVEL_DESC,
        std::slice::from_ref(&overworld),
    )
    .unwrap();
    assert!(!host.sound_stream_known(overworld_stream));
    draw_order_call(
        &fixture,
        &mut host,
        "valid",
        LEVEL_DESC,
        std::slice::from_ref(&overworld),
    )
    .unwrap();
    assert_eq!(host.unknown_host_call_count(), 2);
    assert!(host.played_sounds().is_empty());
    // Seeding the stream again makes it known again.
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);
    draw_order_call(
        &fixture,
        &mut host,
        "valid",
        LEVEL_DESC,
        std::slice::from_ref(&overworld),
    )
    .unwrap();
    assert_eq!(host.unknown_host_call_count(), 2);
    assert_eq!(
        host.take_played_sounds()[0].seed,
        SoundSeed::LevelSeedGenerator(SOUND_SEEDS_42[0])
    );

    // 2. A seed deferred to a live host whose call then counts after the draw is logged as a
    //    draw that played nothing (`SoundRecord::SeedDrawn`), in call order: the host bridge
    //    draws the host's stream there too (Sunrise's `LevelRef::next_sound_seed`), so the
    //    stream stays in step and later sounds play.
    let mut host = StdHost::new();
    host.set_host_deferred_server_sound_seeds(true);
    draw_order_call(
        &fixture,
        &mut host,
        "nullSound",
        LEVEL_DESC,
        std::slice::from_ref(&overworld),
    )
    .unwrap();
    assert_eq!(host.unknown_host_call_count(), 1);
    assert!(host.sound_stream_known(overworld_stream));
    for level in [&overworld, &nether] {
        draw_order_call(
            &fixture,
            &mut host,
            "valid",
            LEVEL_DESC,
            std::slice::from_ref(level),
        )
        .unwrap();
    }
    assert_eq!(host.unknown_host_call_count(), 1);
    let records = host.take_sound_records();
    assert_eq!(records.len(), 3, "{records:#?}");
    assert_eq!(
        records[0],
        SoundRecord::SeedDrawn {
            dimension: "minecraft:overworld".into()
        }
    );
    for (record, dimension) in records[1..]
        .iter()
        .zip(["minecraft:overworld", "minecraft:the_nether"])
    {
        let sound = record.effect().expect("a played sound");
        assert_eq!(sound.dimension.as_deref(), Some(dimension));
        assert_eq!(sound.sound_id, "demo:ok");
        assert_eq!(sound.seed, SoundSeed::HostDeferred);
    }

    // 2b. A call that may or may not reach its draw (a `BlockPos` Decaf cannot read) is still
    //     unknown: no draw can be logged for it. Sunrise draws every level's seeds from one
    //     stream, so while seeds are deferred the nether's position is unknown too.
    draw_order_call(
        &fixture,
        &mut host,
        "oddBlockPos",
        LEVEL_DESC,
        std::slice::from_ref(&overworld),
    )
    .unwrap();
    assert_eq!(host.unknown_host_call_count(), 2);
    assert!(!host.sound_stream_known(overworld_stream));
    assert!(!host.sound_stream_known(nether_stream));
    draw_order_call(
        &fixture,
        &mut host,
        "valid",
        LEVEL_DESC,
        std::slice::from_ref(&nether),
    )
    .unwrap();
    assert_eq!(host.unknown_host_call_count(), 3);
    assert!(host.sound_records().is_empty());
    // A new server's levels start fresh streams.
    host.forget_unknown_server_sound_streams();
    for level in [&overworld, &nether] {
        draw_order_call(
            &fixture,
            &mut host,
            "valid",
            LEVEL_DESC,
            std::slice::from_ref(level),
        )
        .unwrap();
    }
    assert_eq!(host.unknown_host_call_count(), 3);
    let sounds = host.take_played_sounds();
    assert_eq!(sounds.len(), 2);
    assert!(
        sounds
            .iter()
            .all(|sound| sound.seed == SoundSeed::HostDeferred)
    );

    // 3. An entity whose `isSilent()` Decaf does not know may or may not reach the draw.
    let mut host = StdHost::new();
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);
    host.set_entity_sound_facts(
        "game/entity/10",
        EntitySoundFacts {
            entity_id: Some(10),
            level: Some("minecraft:overworld".into()),
            position: Some([1.0, 2.0, 3.0]),
            silent: None,
            sound_source: Some(SoundSource::Hostile),
            is_player: false,
            player_uuid: None,
        },
    );
    let _ = draw_order_call(
        &fixture,
        &mut host,
        "entitySound",
        "(Lnet/minecraft/world/entity/Entity;)V",
        &[entity_handle(10)],
    );
    assert_eq!(host.unknown_host_call_count(), 1);
    assert!(!host.sound_stream_known(overworld_stream));

    // 4. An entity on a level Decaf cannot name drew on some server level: every server
    //    level's stream is unknown.
    let mut host = StdHost::new();
    host.seed_server_level_sound_seeds("minecraft:overworld", 42);
    host.seed_server_level_sound_seeds("minecraft:the_nether", 42);
    host.set_entity_sound_facts(
        "game/entity/11",
        EntitySoundFacts {
            entity_id: Some(11),
            level: None,
            position: Some([1.0, 2.0, 3.0]),
            silent: Some(false),
            sound_source: Some(SoundSource::Hostile),
            is_player: false,
            player_uuid: None,
        },
    );
    let _ = draw_order_call(
        &fixture,
        &mut host,
        "entitySound",
        "(Lnet/minecraft/world/entity/Entity;)V",
        &[entity_handle(11)],
    );
    assert_eq!(host.unknown_host_call_count(), 1);
    assert!(!host.sound_stream_known(overworld_stream));
    assert!(!host.sound_stream_known(nether_stream));
    for level in [&overworld, &nether] {
        draw_order_call(
            &fixture,
            &mut host,
            "valid",
            LEVEL_DESC,
            std::slice::from_ref(level),
        )
        .unwrap();
    }
    assert_eq!(host.unknown_host_call_count(), 3);
    assert!(host.played_sounds().is_empty());

    // 5. Client `Level.random`: a `BlockPos` Decaf cannot read before `playLocalSound` draws
    //    leaves it unknown, so the seeded handle the jar holds through `getRandom()` stops
    //    answering, and later local sounds count.
    let client_level = StdHost::client_level_handle();
    let mut host = StdHost::new();
    host.seed_client_level_randoms(987_654_321, 42);
    host.begin_client_frame(draw_order_client_frame(), &[]);
    let counted = draw_order_call(
        &fixture,
        &mut host,
        "clientOddLocal",
        CLIENT_LEVEL_DESC,
        std::slice::from_ref(&client_level),
    );
    assert!(matches!(counted, Err(NativeError::UnknownHostCall(_))));
    assert!(!host.sound_stream_known("client/level#random"));
    let before = host.unknown_host_call_count();
    let drawn = draw_order_call(
        &fixture,
        &mut host,
        "clientRandom",
        "(Lnet/minecraft/client/multiplayer/ClientLevel;)J",
        std::slice::from_ref(&client_level),
    );
    assert_ne!(
        drawn.ok(),
        Some(Value::Long(LEVEL_RANDOM_987654321[0])),
        "an unknown Level.random must not answer"
    );
    assert!(host.unknown_host_call_count() > before);
    let counted = draw_order_call(
        &fixture,
        &mut host,
        "clientValidLocal",
        CLIENT_LEVEL_DESC,
        std::slice::from_ref(&client_level),
    );
    assert!(matches!(counted, Err(NativeError::UnknownHostCall(_))));
    // Seeded again: known again.
    host.seed_client_level_random(987_654_321);
    draw_order_call(
        &fixture,
        &mut host,
        "clientValidLocal",
        CLIENT_LEVEL_DESC,
        std::slice::from_ref(&client_level),
    )
    .unwrap();
    let out = host.end_client_frame();
    assert_eq!(out.sounds.len(), 1);
    assert_eq!(
        out.sounds[0].seed,
        SoundSeed::LevelRandom(LEVEL_RANDOM_987654321[0])
    );
}
