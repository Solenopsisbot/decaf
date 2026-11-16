//! The creative-tab registration boundary, compared byte-for-byte with the Java 25 capture.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const ORACLE: &[u8] = include_bytes!("fixtures/spell_engine_itemgroup_java25.txt");

const PROBE: &str = include_str!("fixtures/cov15_clinit/Probe.java");

const STUBS: &[(&str, &str)] = &[
 ("net/minecraft/world/item/Item.java", "package net.minecraft.world.item; public class Item {}"),
 ("net/minecraft/world/level/block/Block.java", "package net.minecraft.world.level.block; public class Block {}"),
 ("net/minecraft/resources/Identifier.java", "package net.minecraft.resources; public final class Identifier { public static Identifier parse(String s){return null;} }"),
 ("net/minecraft/core/registries/BuiltInRegistries.java", "package net.minecraft.core.registries; public final class BuiltInRegistries { public static Registry ITEM; public static Registry BLOCK; }"),
 ("net/minecraft/core/registries/Registry.java", "package net.minecraft.core.registries; import net.minecraft.resources.Identifier; public class Registry { public Object getValue(Identifier i){return null;} public Object getKey(Object o){return null;} }"),
 ("net/minecraft/world/item/Items.java", "package net.minecraft.world.item; public final class Items { public static Item STONE; }"),
 ("net/minecraft/world/level/block/Blocks.java", "package net.minecraft.world.level.block; public final class Blocks { public static Block STONE; }"),
];

#[test]
fn static_initializer_matches_java25_capture() {
    let fixture = javac_fixture::Fixture::build(
        "cov15-clinit",
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
    assert_eq!(&actual[..actual.len()-5], &ORACLE[..ORACLE.len()-5]);
    assert_eq!(host.unknown_host_call_count(), 1, "Registry.getValue identity lookup must be counted");

}
