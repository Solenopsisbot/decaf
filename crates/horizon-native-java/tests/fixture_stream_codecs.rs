//! `ByteBufCodecs.map` / `collection` (#472, `stream_codec.rs`).
//!
//! `fixtures/stream_codecs/demo/Probe.java` runs on Java 25 against the 26.3 jar and its
//! libraries (`java25.txt`, command in the README), and here against compile-only stubs: only
//! `demo/` ships. Building a codec is exact and counts nothing (vanilla builds it lazily, so even
//! a null factory doesn't throw); using one (`encode`) is counted.
use horizon_native_java::StdHost;
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use javac_fixture::Fixture;
const PROBE: &str = include_str!("fixtures/stream_codecs/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/stream_codecs/java25.txt");
const STUB: &str = r#"package net.minecraft.network.codec; public interface StreamCodec<B,V>{ void encode(B b,V v); V decode(B b); }"#;
const CODECS: &str = r#"package net.minecraft.network.codec; import java.util.function.*; public final class ByteBufCodecs { public static final StreamCodec<Object,Integer> VAR_INT=null; public static final StreamCodec<Object,String> STRING_UTF8=null; public static <K,V> StreamCodec<Object,java.util.Map<K,V>> map(IntFunction<java.util.Map<K,V>> f,StreamCodec<Object,K> k,StreamCodec<Object,V> v){throw new AssertionError();} public static <E> StreamCodec<Object,java.util.Collection<E>> collection(IntFunction<java.util.Collection<E>> f,StreamCodec<Object,E> e){throw new AssertionError();} }"#;
fn fixture() -> Fixture {
    Fixture::build(
        "stream-codecs",
        &[
            ("net/minecraft/network/codec/StreamCodec.java", STUB),
            ("net/minecraft/network/codec/ByteBufCodecs.java", CODECS),
            ("demo/Probe.java", PROBE),
        ],
    )
}

#[test]
fn building_stream_codecs_matches_java25() {
    let f = fixture();
    let mut h = f.jar.seed_host(StdHost::new());
    assert_eq!(f.text_on(&mut h, "rows"), ORACLE);
    assert_eq!(h.unknown_host_calls(), Vec::<(String, usize)>::new());
}

#[test]
fn using_a_stream_codec_is_counted() {
    let f = fixture();
    let mut h = f.jar.seed_host(StdHost::new());
    f.text_on(&mut h, "counted");
    assert_eq!(
        h.unknown_host_calls(),
        vec![(
            "net/minecraft/network/codec/StreamCodec#encode(Ljava/lang/Object;Ljava/lang/Object;)V"
                .to_owned(),
            1
        )]
    );
}
