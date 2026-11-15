//! Java 25 fixture for client registration-only APIs.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const API_STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        r#"package net.minecraft.resources; public class Identifier { public static Identifier withDefaultNamespace(String s) { throw new AssertionError(); } }"#,
    ),
    (
        "net/minecraft/core/particles/ParticleType.java",
        r#"package net.minecraft.core.particles; public abstract class ParticleType<T> { public boolean getOverrideLimiter() { throw new AssertionError(); } public com.mojang.serialization.MapCodec<T> codec() { throw new AssertionError(); } public net.minecraft.network.codec.StreamCodec<Object, T> streamCodec() { throw new AssertionError(); } }"#,
    ),
    (
        "net/minecraft/core/particles/SimpleParticleType.java",
        r#"package net.minecraft.core.particles; public class SimpleParticleType extends ParticleType<Object> { public SimpleParticleType getType() { throw new AssertionError(); } }"#,
    ),
    (
        "net/minecraft/client/particle/ParticleProvider.java",
        r#"package net.minecraft.client.particle; public interface ParticleProvider<T> {}"#,
    ),
    (
        "com/mojang/serialization/MapCodec.java",
        r#"package com.mojang.serialization; public class MapCodec<T> {}"#,
    ),
    (
        "net/minecraft/network/codec/StreamCodec.java",
        r#"package net.minecraft.network.codec; public class StreamCodec<A, T> {}"#,
    ),
    (
        "net/fabricmc/fabric/api/particle/v1/FabricParticleTypes.java",
        r#"package net.fabricmc.fabric.api.particle.v1; public final class FabricParticleTypes { public static net.minecraft.core.particles.SimpleParticleType simple() { throw new AssertionError(); } public static net.minecraft.core.particles.SimpleParticleType simple(boolean b) { throw new AssertionError(); } public static <T> net.minecraft.core.particles.ParticleType<T> complex(com.mojang.serialization.MapCodec<T> a, net.minecraft.network.codec.StreamCodec<?, T> b) { throw new AssertionError(); } public static <T> net.minecraft.core.particles.ParticleType<T> complex(boolean x, com.mojang.serialization.MapCodec<T> a, net.minecraft.network.codec.StreamCodec<?, T> b) { throw new AssertionError(); } }"#,
    ),
    (
        "net/fabricmc/fabric/api/client/particle/v1/ParticleProviderRegistry.java",
        r#"package net.fabricmc.fabric.api.client.particle.v1; public interface ParticleProviderRegistry { static ParticleProviderRegistry getInstance() { throw new AssertionError(); } void register(net.minecraft.core.particles.ParticleType<?> t, net.minecraft.client.particle.ParticleProvider<?> p); }"#,
    ),
    (
        "net/minecraft/client/gui/components/debug/DebugScreenEntry.java",
        r#"package net.minecraft.client.gui.components.debug; public interface DebugScreenEntry {}"#,
    ),
    (
        "net/minecraft/client/gui/components/debug/DebugScreenEntries.java",
        r#"package net.minecraft.client.gui.components.debug; public class DebugScreenEntries { public static net.minecraft.resources.Identifier register(net.minecraft.resources.Identifier i, DebugScreenEntry e) { throw new AssertionError(); } public static final net.minecraft.resources.Identifier FPS = null; public static final java.util.Map<DebugScreenProfile, java.util.Map<net.minecraft.resources.Identifier, DebugScreenEntryStatus>> PROFILES = null; }"#,
    ),
    (
        "net/minecraft/client/gui/components/debug/DebugScreenEntryStatus.java",
        r#"package net.minecraft.client.gui.components.debug; public enum DebugScreenEntryStatus { IN_OVERLAY, ALWAYS_ON }"#,
    ),
    (
        "net/minecraft/client/gui/components/debug/DebugScreenProfile.java",
        r#"package net.minecraft.client.gui.components.debug; public enum DebugScreenProfile { DEFAULT, PERFORMANCE }"#,
    ),
    (
        "net/minecraft/client/gui/screens/MenuScreens.java",
        r#"package net.minecraft.client.gui.screens; public class MenuScreens { public static void register(Object menu, Object constructor) { throw new AssertionError(); } }"#,
    ),
    (
        "net/fabricmc/fabric/api/client/rendering/v1/hud/VanillaHudElements.java",
        r#"package net.fabricmc.fabric.api.client.rendering.v1.hud; public final class VanillaHudElements { public static final net.minecraft.resources.Identifier CHAT = null; public static final net.minecraft.resources.Identifier INFO_BAR_FOREGROUND = null; }"#,
    ),
    (
        "net/minecraft/client/model/geom/ModelLayerLocation.java",
        r#"package net.minecraft.client.model.geom; public record ModelLayerLocation(net.minecraft.resources.Identifier model, String layer) {}"#,
    ),
    (
        "net/fabricmc/fabric/api/client/rendering/v1/ModelLayerRegistry.java",
        r#"package net.fabricmc.fabric.api.client.rendering.v1; public final class ModelLayerRegistry { public static void registerModelLayer(net.minecraft.client.model.geom.ModelLayerLocation l, Object p) { throw new AssertionError(); } }"#,
    ),
    (
        "net/fabricmc/fabric/api/client/rendering/v1/EntityRendererRegistry.java",
        r#"package net.fabricmc.fabric.api.client.rendering.v1; public final class EntityRendererRegistry { public static void register(Object e, Object p) { throw new AssertionError(); } }"#,
    ),
    (
        "net/fabricmc/fabric/api/client/model/loading/v1/PreparableModelLoadingPlugin.java",
        r#"package net.fabricmc.fabric.api.client.model.loading.v1; public interface PreparableModelLoadingPlugin<T> { interface DataLoader<T> {} static <T> void register(DataLoader<T> loader, PreparableModelLoadingPlugin<T> plugin) { throw new AssertionError(); } }"#,
    ),
    (
        "net/fabricmc/fabric/api/client/model/loading/v1/ModelLoadingPlugin.java",
        r#"package net.fabricmc.fabric.api.client.model.loading.v1; public interface ModelLoadingPlugin { static void register(ModelLoadingPlugin p) { throw new AssertionError(); } }"#,
    ),
];

const PROBE: &str = include_str!("fixtures/client_registrations/Probe.java");

#[test]
fn client_registration_surfaces_keep_java25_identity_and_order() {
    let mut sources = API_STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("client-registrations", &sources);
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("CLIENT"));
    assert_eq!(
        fixture.text_on(&mut host, "identities"),
        "false|true|true|true"
    );
    assert_eq!(
        fixture.text_on(&mut host, "duplicates"),
        "IllegalArgumentException"
    );
    assert_eq!(fixture.text_on(&mut host, "fields"), "true|true");
    assert_eq!(
        fixture.text_on(&mut host, "particle_semantics"),
        "true|false|true|false|true|false|false"
    );
    assert_eq!(
        fixture.text_on(&mut host, "particle_codecs"),
        "true|true|true|true"
    );
    assert_eq!(
        fixture.text_on(&mut host, "hud_ids"),
        "minecraft:chat|minecraft:info_bar_foreground"
    );
    assert_eq!(
        fixture.text_on(&mut host, "profiles"),
        "true|true|true|true"
    );
    assert!(fixture
        .text_on(&mut host, "model_equal")
        .starts_with("Cannot replace registration for model layer \"ModelLayerLocation[model=minecraft:x, layer=main]\""));
    assert_eq!(
        fixture.text_on(&mut host, "preparable_nulls"),
        "data loader must not be null|plugin must not be null"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
