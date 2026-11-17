#[path = "support/javac_fixture.rs"]
mod javac_fixture;

const PROBE: &str = include_str!("fixtures/block_entity_types/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/block_entity_types/java25.txt");
const TYPES: &str = include_str!("fixtures/block_entity_types/BlockEntityTypes.java");
const TYPE: &str = include_str!("fixtures/block_entity_types/BlockEntityType.java");
const REG: &str = include_str!("fixtures/block_entity_types/BuiltInRegistries.java");
const REGISTRY: &str = include_str!("fixtures/block_entity_types/Registry.java");
const IDENT: &str = include_str!("fixtures/block_entity_types/Identifier.java");

#[test]
fn block_entity_types_rows_match_java25_oracle() {
    let fixture = javac_fixture::Fixture::build(
        "block-entity-types",
        &[
            ("demo/Probe.java", PROBE),
            (
                "net/minecraft/world/level/block/entity/BlockEntityTypes.java",
                TYPES,
            ),
            (
                "net/minecraft/world/level/block/entity/BlockEntityType.java",
                TYPE,
            ),
            ("net/minecraft/core/registries/BuiltInRegistries.java", REG),
            ("net/minecraft/core/Registry.java", REGISTRY),
            ("net/minecraft/resources/Identifier.java", IDENT),
        ],
    );
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "rows",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    let got = match answer {
        Ok(horizon_native_java::Value::Ref(Some(s))) => s,
        other => panic!("probe failed: {other:?}"),
    };
    eprintln!("UNKNOWN={:?}", host.unknown_host_calls());
    assert_eq!(got.trim(), ORACLE.trim());
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
