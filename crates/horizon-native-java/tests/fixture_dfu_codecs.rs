#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
const PROBE: &str = include_str!("fixtures/dfu_codecs/demo/Probe.java");
const JAVA25: &str = include_str!("fixtures/dfu_codecs/java25.txt");
fn fixture() -> javac_fixture::Fixture {
    let sources = [
        (
            "com/mojang/datafixers/Products.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/datafixers/Products.java"),
        ),
        (
            "com/mojang/datafixers/kinds/App.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/datafixers/kinds/App.java"),
        ),
        (
            "com/mojang/datafixers/util/Either.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/datafixers/util/Either.java"),
        ),
        (
            "com/mojang/datafixers/util/Function3.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/datafixers/util/Function3.java"),
        ),
        (
            "com/mojang/datafixers/util/Function8.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/datafixers/util/Function8.java"),
        ),
        (
            "com/mojang/datafixers/util/Pair.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/datafixers/util/Pair.java"),
        ),
        (
            "com/mojang/serialization/Codec.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/serialization/Codec.java"),
        ),
        (
            "com/mojang/serialization/DataResult.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/serialization/DataResult.java"),
        ),
        (
            "com/mojang/serialization/DynamicOps.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/serialization/DynamicOps.java"),
        ),
        (
            "com/mojang/serialization/Lifecycle.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/serialization/Lifecycle.java"),
        ),
        (
            "com/mojang/serialization/MapCodec.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/serialization/MapCodec.java"),
        ),
        (
            "com/mojang/serialization/MapLike.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/serialization/MapLike.java"),
        ),
        (
            "com/mojang/serialization/RecordBuilder.java",
            include_str!("fixtures/host_codecs/stubs/com/mojang/serialization/RecordBuilder.java"),
        ),
        (
            "com/mojang/serialization/codecs/PrimitiveCodec.java",
            include_str!(
                "fixtures/host_codecs/stubs/com/mojang/serialization/codecs/PrimitiveCodec.java"
            ),
        ),
        (
            "com/mojang/serialization/codecs/RecordCodecBuilder.java",
            include_str!(
                "fixtures/host_codecs/stubs/com/mojang/serialization/codecs/RecordCodecBuilder.java"
            ),
        ),
        (
            "net/minecraft/network/codec/ByteBufCodecs.java",
            include_str!(
                "fixtures/host_codecs/stubs/net/minecraft/network/codec/ByteBufCodecs.java"
            ),
        ),
        (
            "net/minecraft/network/codec/StreamCodec.java",
            include_str!("fixtures/host_codecs/stubs/net/minecraft/network/codec/StreamCodec.java"),
        ),
        (
            "net/minecraft/resources/Identifier.java",
            include_str!("fixtures/host_codecs/stubs/net/minecraft/resources/Identifier.java"),
        ),
        (
            "net/minecraft/util/StringRepresentable.java",
            include_str!("fixtures/host_codecs/stubs/net/minecraft/util/StringRepresentable.java"),
        ),
        ("demo/Probe.java", PROBE),
    ];
    javac_fixture::Fixture::build("dfu_codecs", &sources)
}
#[test]
fn construction_matches_java25() {
    let f = fixture();
    let mut host = f.jar.seed_host(StdHost::new());
    assert_eq!(f.text_on(&mut host, "rows"), JAVA25);
    assert_eq!(host.unknown_host_calls(), vec![]);
}
