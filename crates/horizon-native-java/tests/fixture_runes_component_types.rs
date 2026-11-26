//! The real Runes 1.3.2 jar's rune pouches (docs/java-item-parts.md steps 1 and 2), opt-in through
//! `RUNES_JAR` (CI has no corpus).
//!
//! What the jars hold, read with JDK 25's `javap -c -p`:
//!
//! * the nested `META-INF/jars/bundle-api-5.0.0+26.3-fabric.jar`'s `bundleapi.mixins.json` lists
//!   `registry.DataComponentTypesMixin`, `@Mixin(DataComponents.class)` with
//!   `@Inject(method = "<clinit>", at = @At("TAIL")) static_init_TAIL_BundleAPI(CallbackInfo)`,
//!   which does `Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE,
//!   BundleAPI.identifier("custom_bundle_contents"), BundleAPI.CUSTOM_BUNDLE_CONTENTS_COMPONENT)`,
//!   and `registry.ComponentPredicateTypesMixin` into `DataComponentPredicates`, which registers
//!   `BundleAPI.CUSTOM_BUNDLE_CONTENTS_ITEM_SUB_PREDICATE` under the same id;
//! * `BundleAPI.<clinit>` builds the type with `DataComponentType.builder()
//!   .persistent(CustomBundleContentsComponent.CODEC).networkSynchronized(..).cacheEncoding()`;
//!   the codec is `RecordCodecBuilder.create(i -> i.group(Content.CODEC.fieldOf("content"),
//!   Codec.INT.optionalFieldOf("size_multiplier", 1)))`, `Content.CODEC` being
//!   `ItemStackTemplate.CODEC.listOf().xmap(..)`;
//! * `RunePouches.<clinit>` makes `small_rune_pouch` (`iconst_4`), `medium_rune_pouch`
//!   (`bipush 8`) and `large_rune_pouch` (`bipush 12`), each `Item.Properties` with
//!   `component(BundleAPI.CUSTOM_BUNDLE_CONTENTS_COMPONENT, CustomBundleContentsComponent.builder()
//!   .size_multiplier(n).build())`: no contents (`Content.DEFAULT` is `List.of()`).
//!
//! So the type is registered by bundle-api's Mixin before Runes' entrypoint, and each pouch's
//! default is `{"content": [], "size_multiplier": n}`: the list encodes as an empty `ListTag`, the
//! multiplier as an `IntTag` (`fixture_jar_component_types.rs` checks those shapes against JDK 25's
//! `NbtOps`).

use horizon_native_java::{
    ExecutionBudget, JarComponentType, NativeJar, StdHost, content::declarations,
};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[test]
fn runes_pouches_declare_bundle_apis_component_type_and_their_defaults() {
    let Some(path) = std::env::var_os("RUNES_JAR").map(PathBuf::from) else {
        eprintln!("RUNES_JAR is unset; real Runes test skipped");
        return;
    };
    let jar = NativeJar::from_bytes(&fs::read(path).unwrap()).unwrap();
    let mut host = jar
        .try_seed_host(StdHost::new().with_environment_type("SERVER"))
        .unwrap();
    let budget = ExecutionBudget::UNBOUNDED;
    assert_eq!(
        jar.run_bootstrap_static_injectors(&mut host, &budget),
        Vec::<String>::new()
    );
    // Bundle API's Mixins ran before any entrypoint: the type is registered, its predicate type
    // after it (`DataComponents` finishes initializing before `DataComponentPredicates`).
    let rows: Vec<(String, String)> = host
        .registrations()
        .iter()
        .map(|entry| (entry.registry.clone(), entry.id.clone()))
        .collect();
    assert_eq!(
        rows,
        vec![
            (
                "minecraft:data_component_type".to_owned(),
                "bundleapi:custom_bundle_contents".to_owned()
            ),
            (
                "minecraft:data_component_predicate_type".to_owned(),
                "bundleapi:custom_bundle_contents".to_owned()
            ),
        ]
    );
    for entry in jar.discover_entrypoints("main") {
        jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
            .unwrap_or_else(|error| panic!("{}: {error}", entry.class));
    }
    jar.declare_jar_component_defaults(&mut host);
    assert_eq!(
        host.jar_component_types(),
        vec![JarComponentType {
            id: "bundleapi:custom_bundle_contents".into(),
            persistent: true
        }]
    );
    let pouches: BTreeMap<String, serde_json::Value> = declarations(host.registrations())
        .iter()
        .filter(|declaration| declaration.id().ends_with("_rune_pouch"))
        .map(|declaration| {
            let file = declaration.file_json();
            (
                declaration.id().to_owned(),
                file["components"]["bundleapi:custom_bundle_contents"].clone(),
            )
        })
        .collect();
    assert_eq!(
        pouches,
        BTreeMap::from([
            (
                "runes:small_rune_pouch".to_owned(),
                json!({"content": [], "size_multiplier": 4})
            ),
            (
                "runes:medium_rune_pouch".to_owned(),
                json!({"content": [], "size_multiplier": 8})
            ),
            (
                "runes:large_rune_pouch".to_owned(),
                json!({"content": [], "size_multiplier": 12})
            ),
        ])
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
