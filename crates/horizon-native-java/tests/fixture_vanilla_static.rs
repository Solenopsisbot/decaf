#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/network/chat/Component.java",
        r#"package net.minecraft.network.chat; public interface Component { static MutableComponent empty(){throw new AssertionError();} String getString(); }"#,
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        r#"package net.minecraft.network.chat; public interface MutableComponent extends Component {}"#,
    ),
    (
        "net/minecraft/network/chat/CommonComponents.java",
        r#"package net.minecraft.network.chat; public final class CommonComponents { public static final Component EMPTY=null; }"#,
    ),
    (
        "com/mojang/serialization/Codec.java",
        r#"package com.mojang.serialization; public interface Codec<T> { com.mojang.serialization.MapCodec<T> fieldOf(String s); com.mojang.serialization.DataResult<T> encodeStart(DynamicOps ops,Object o); com.mojang.serialization.DataResult<T> parse(DynamicOps ops,Object o); com.mojang.serialization.DataResult<com.mojang.datafixers.util.Pair<T,T>> decode(DynamicOps ops,Object o); }"#,
    ),
    (
        "com/mojang/serialization/DynamicOps.java",
        r#"package com.mojang.serialization; public interface DynamicOps {}"#,
    ),
    (
        "com/mojang/serialization/DataResult.java",
        r#"package com.mojang.serialization; public interface DataResult<T> {}"#,
    ),
    (
        "com/mojang/serialization/JsonOps.java",
        r#"package com.mojang.serialization; public final class JsonOps implements DynamicOps { public static final JsonOps INSTANCE=null; }"#,
    ),
    (
        "com/mojang/datafixers/util/Pair.java",
        r#"package com.mojang.datafixers.util; public final class Pair<A,B> {}"#,
    ),
    (
        "com/google/gson/JsonPrimitive.java",
        r#"package com.google.gson; public final class JsonPrimitive { public JsonPrimitive(String s){} }"#,
    ),
    (
        "com/mojang/serialization/MapCodec.java",
        r#"package com.mojang.serialization; public interface MapCodec<T> extends Codec<T> {}"#,
    ),
    (
        "net/minecraft/resources/Identifier.java",
        r#"package net.minecraft.resources; import com.mojang.serialization.Codec; public final class Identifier { public static final Codec<Identifier> CODEC=null; public static Identifier parse(String s){throw new AssertionError();} }"#,
    ),
];
const PROBE: &str = include_str!("fixtures/vanilla_static/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/vanilla_static/java25.txt");
#[test]
fn vanilla_static_rows_match_java25() {
    let mut f = STUBS.to_vec();
    f.push(("demo/Probe.java", PROBE));
    let x = Fixture::build("vanilla-static", &f);
    let mut h = x.jar.seed_host(StdHost::new());
    assert_eq!(x.text_on(&mut h, "rows"), ORACLE);
    assert!(h.unknown_host_calls().is_empty());
}
#[test]
fn vanilla_static_hash_is_counted_on_fresh_host() {
    let mut f = STUBS.to_vec();
    f.push(("demo/Probe.java", PROBE));
    let x = Fixture::build("vanilla-static-hash", &f);
    let mut h = x.jar.seed_host(StdHost::new());
    let _ = x.text_on(&mut h, "hash");
    assert_eq!(
        h.unknown_host_calls(),
        vec![(
            "net/minecraft/network/chat/Component#hashCode()I".to_owned(),
            1
        )]
    );
}
#[test]
fn vanilla_static_field_of_is_counted_on_fresh_host() {
    let mut f = STUBS.to_vec();
    f.push(("demo/Probe.java", PROBE));
    let x = Fixture::build("vanilla-static-counted", &f);
    let mut h = x.jar.seed_host(StdHost::new());
    assert_eq!(x.text_on(&mut h, "counted"), "true");
    assert_eq!(h.unknown_host_calls(),vec![("com/mojang/serialization/Codec#fieldOf(Ljava/lang/String;)Lcom/mojang/serialization/MapCodec;".to_owned(),1)]);
}

#[test]
fn vanilla_static_codec_encode_counted() {
    counted_codec(
        "codecEncode",
        "com/mojang/serialization/Codec#encodeStart(Lcom/mojang/serialization/DynamicOps;Ljava/lang/Object;)Lcom/mojang/serialization/DataResult;",
    );
}
#[test]
fn vanilla_static_codec_parse_counted() {
    counted_codec(
        "codecDecode",
        "com/mojang/serialization/Codec#parse(Lcom/mojang/serialization/DynamicOps;Ljava/lang/Object;)Lcom/mojang/serialization/DataResult;",
    );
}
#[test]
fn vanilla_static_codec_decode_counted() {
    counted_codec(
        "codecDecodePair",
        "com/mojang/serialization/Codec#decode(Lcom/mojang/serialization/DynamicOps;Ljava/lang/Object;)Lcom/mojang/serialization/DataResult;",
    );
}
fn counted_codec(method: &str, sig: &str) {
    let mut f = STUBS.to_vec();
    f.push(("demo/Probe.java", PROBE));
    let x = Fixture::build("vanilla-static-codec", &f);
    let mut h = x.jar.seed_host(StdHost::new());
    let _ = x.text_on(&mut h, method);
    assert_eq!(
        h.unknown_host_calls(),
        vec![
            (sig.to_owned(), 1),
            (
                "com/mojang/serialization/JsonOps#INSTANCELcom/mojang/serialization/JsonOps;"
                    .to_owned(),
                1
            )
        ]
    );
}
