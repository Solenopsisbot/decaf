//! Captured Java 25 / real 26.3 server-game oracle rows for entity registration.
#[test]
fn captured_real_263_java25_rows_are_stable() {
    let captured = include_str!("fixtures/entity_registration/java25.txt");
    let rows: Vec<&str> = captured.lines().collect();
    assert_eq!(rows.len(), 14);
    assert!(rows.contains(&"serial=true,false"));
    assert!(rows.contains(&"base=0"));
    assert!(rows.contains(&"sub=1"));
    assert!(rows.contains(&"base2=1"));
    assert!(rows.contains(&"sibling=0"));
    assert!(
        rows.iter()
            .any(|r| r.starts_with("overflow=Data value id is too big with 255!"))
    );
    assert!(rows.contains(&"cat=CREATURE,creature,C,10,true,true,128,32,CREATURE"));
    assert!(rows.contains(&"cat=MISC,misc,MI,-1,true,true,128,32,MISC"));
}

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{StdHost, Value};
#[test]
fn probe_rows_match_java25_capture() {
    let probe: &'static str = Box::leak(
        format!(
            "package demo;\n{}",
            include_str!("fixtures/entity_registration/Probe.java")
        )
        .into_boxed_str(),
    );
    let sources = [
        ("demo/Probe.java", probe),
        (
            "net/minecraft/SharedConstants.java",
            include_str!("fixtures/entity_registration/stubs/net/minecraft/SharedConstants.java"),
        ),
        (
            "net/minecraft/server/Bootstrap.java",
            include_str!("fixtures/entity_registration/stubs/net/minecraft/server/Bootstrap.java"),
        ),
        (
            "net/minecraft/network/RegistryFriendlyByteBuf.java",
            include_str!(
                "fixtures/entity_registration/stubs/net/minecraft/network/RegistryFriendlyByteBuf.java"
            ),
        ),
        (
            "net/minecraft/network/codec/StreamCodec.java",
            include_str!(
                "fixtures/entity_registration/stubs/net/minecraft/network/codec/StreamCodec.java"
            ),
        ),
        (
            "net/minecraft/network/syncher/EntityDataSerializer.java",
            include_str!(
                "fixtures/entity_registration/stubs/net/minecraft/network/syncher/EntityDataSerializer.java"
            ),
        ),
        (
            "net/minecraft/network/syncher/EntityDataAccessor.java",
            include_str!(
                "fixtures/entity_registration/stubs/net/minecraft/network/syncher/EntityDataAccessor.java"
            ),
        ),
        (
            "net/minecraft/network/syncher/SyncedDataHolder.java",
            include_str!(
                "fixtures/entity_registration/stubs/net/minecraft/network/syncher/SyncedDataHolder.java"
            ),
        ),
        (
            "net/minecraft/network/syncher/SynchedEntityData.java",
            include_str!(
                "fixtures/entity_registration/stubs/net/minecraft/network/syncher/SynchedEntityData.java"
            ),
        ),
        (
            "net/minecraft/network/syncher/EntityDataSerializers.java",
            include_str!(
                "fixtures/entity_registration/stubs/net/minecraft/network/syncher/EntityDataSerializers.java"
            ),
        ),
        (
            "net/minecraft/world/entity/MobCategory.java",
            include_str!(
                "fixtures/entity_registration/stubs/net/minecraft/world/entity/MobCategory.java"
            ),
        ),
    ];
    let f = javac_fixture::Fixture::build("entity-registration", &sources);
    let mut h = f.jar.seed_host(StdHost::new());
    let v = f
        .jar
        .invoke_method("demo/Probe", "rows", "()Ljava/lang/String;", None, &mut h)
        .unwrap_or_else(|e| {
            panic!(
                "{e:?}; unknown_count={}; unknown={:?}",
                h.unknown_host_call_count(),
                h.unknown_host_calls()
            )
        });
    eprintln!(
        "unknown_count={} unknown={:?}",
        h.unknown_host_call_count(),
        h.unknown_host_calls()
    );
    assert_eq!(h.unknown_host_call_count(), 1);
    assert_eq!(
        h.unknown_host_calls()[0].0,
        "net/minecraft/network/syncher/EntityDataSerializer#equals(Ljava/lang/Object;)Z"
    );
    let Value::Ref(Some(s)) = v else {
        panic!("{v:?}")
    };
    assert_eq!(s, include_str!("fixtures/entity_registration/java25.txt"));
}
