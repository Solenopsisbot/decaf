//! The creative-tab registration boundary, compared byte-for-byte with the Java 25 capture.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const ORACLE: &[u8] = include_bytes!("fixtures/spell_engine_itemgroup_java25.txt");

const PROBE: &str = include_str!("fixtures/spell_engine_itemgroup/Probe.java");

const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier parse(String s) { return null; } }",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources; public final class ResourceKey<T> { public static <T> ResourceKey<T> createRegistryKey(Identifier i) { return null; } public static <T> ResourceKey<T> create(ResourceKey<?> r, Identifier i) { return null; } }",
    ),
    (
        "net/minecraft/world/item/CreativeModeTab.java",
        "package net.minecraft.world.item; public final class CreativeModeTab {}",
    ),
    (
        "net/fabricmc/fabric/api/event/Event.java",
        "package net.fabricmc.fabric.api.event; public interface Event<T> { void register(T listener); }",
    ),
    (
        "net/fabricmc/fabric/api/creativetab/v1/CreativeModeTabEvents.java",
        "package net.fabricmc.fabric.api.creativetab.v1; import net.fabricmc.fabric.api.event.Event; import net.minecraft.resources.ResourceKey; import net.minecraft.world.item.CreativeModeTab; public final class CreativeModeTabEvents { public interface ModifyOutput { void modifyOutput(Object output); } public static Event<ModifyOutput> modifyOutputEvent(ResourceKey<CreativeModeTab> key) { return null; } }",
    ),
];

#[test]
fn creative_tab_registration_matches_java25_capture_and_is_unfired() {
    let fixture = javac_fixture::Fixture::build(
        "spell-engine-itemgroup",
        &[
            ("demo/Probe.java", PROBE),
            STUBS[0],
            STUBS[1],
            STUBS[2],
            STUBS[3],
            STUBS[4],
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let actual = fixture
        .jar
        .invoke_method(
            "demo/Probe",
            "rows",
            "()Ljava/lang/String;",
            None,
            &mut host,
        )
        .unwrap();
    let actual = match actual {
        Value::Ref(Some(text)) => text.into_bytes(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        actual, ORACLE,
        "fixture output diverged from captured Java 25 rows"
    );
    let listeners = host.game_listeners(&fixture.jar, true);
    assert!(listeners.listeners.is_empty());
    assert!(
        listeners.unsupported.values().sum::<usize>() >= 1,
        "registration must be reported unfired: {:?}",
        listeners.unsupported
    );
}
