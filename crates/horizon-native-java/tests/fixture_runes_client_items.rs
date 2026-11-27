//! Real-jar client item parts: Runes 1.3.2's rune pouches on a client host. Bundle API (nested
//! `bundle-api-5.0.0+26.3-fabric.jar`) registers its contents component type at the end of
//! `DataComponents.<clinit>` and its predicate type at the end of `DataComponentPredicates`',
//! and, on the client, its item model property `bundleapi:custom_bundle/fullness` at the end of
//! `RangeSelectItemModelProperties.bootstrap()`. The client bootstrap pass runs all three in the
//! client's measured order (`bootstrap_clinit_order_26_3_client.txt`); then the pouch's fullness
//! property and tooltip component run on the client runtime (`client_item_parts.rs`).
//!
//! `RUNES_JAR` names the jar; the test is skipped without it (CI has no corpus).
//!
//! Expected values are derived from the jars' bytecode (`javap -c -p`, JDK 25):
//! - `CustomBundleFullnessProperty.get` is `CustomBundleItem.getAmountFilled(stack)`:
//!   `stack.getOrDefault(CUSTOM_BUNDLE_CONTENTS, DEFAULT).getOccupancy().floatValue()`. A stack's
//!   occupancy is `CustomBundleContentsComponent.calculateOccupancy`: per item
//!   `Fraction.getFraction(1, maxStackSize * size_multiplier)` (`getOccupancy(ItemInstance, int)`
//!   offsets 99-108) times its count. A rune stacks to 64 and the small pouch's multiplier is 4:
//!   no runes `0.0`, ten `10/256` (`0.0390625`), 256 `1.0`.
//! - `CustomBundleItem.getTooltipImage` (offsets 0-52): the stack's contents as
//!   `CustomBundleTooltipData`, which Bundle API's `TooltipComponentMixin` (a cancellable `HEAD`
//!   into `ClientTooltipComponent.create`) maps to `CustomBundleTooltipComponent`, whose
//!   `getWidth` is `96` and whose `getHeight` for contents of `n` stacks is
//!   `ceilDiv(min(12, n), 4) * 24 + 13 + 8` (`getHeightOfNonEmpty`, `getRows`,
//!   `getNumVisibleSlots`); an empty pouch's height wraps its description with `Font.split`, which
//!   the client surface does not model: counted.

use std::io::Read;
use std::path::Path;

use horizon_native_java::{
    ExecutionBudget, GAME_PROTOTYPE_COMPONENTS, ItemHook, ItemModelHolder, ItemStackRequest,
    NativeJar, Nbt, NbtCompound, RuntimeComponentValue, StdHost, Value,
};

const POUCH: &str = "runes:small_rune_pouch";
const RUNE: &str = "runes:fire_stone";
const CONTENTS: &str = "bundleapi:custom_bundle_contents";
const FULLNESS: &str = "bundleapi:custom_bundle/fullness";

#[test]
fn rune_pouch_fullness_and_tooltip_run_on_the_client() {
    let Some(jar_path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes client item test skipped");
        return;
    };
    std::thread::Builder::new()
        .name("runes-client-items".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || run(Path::new(&jar_path)))
        .expect("spawn test thread")
        .join()
        .unwrap();
}

fn item(host: &StdHost, id: &str) -> Value {
    host.registrations()
        .iter()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == id)
        .unwrap_or_else(|| panic!("{id} is registered"))
        .value
        .clone()
}

/// The `model` node of the pouch's item definition, as the jar ships it.
fn pouch_model_node(jar_path: &Path) -> serde_json::Value {
    let file = std::fs::File::open(jar_path).expect("open RUNES_JAR");
    let mut archive = zip::ZipArchive::new(file).expect("read RUNES_JAR");
    let mut text = String::new();
    archive
        .by_name("assets/runes/items/small_rune_pouch.json")
        .expect("the pouch's item definition")
        .read_to_string(&mut text)
        .unwrap();
    let definition: serde_json::Value = serde_json::from_str(&text).unwrap();
    definition["model"].clone()
}

/// The pouch's contents as the game stores them: `(item, count)` stacks.
fn contents(stacks: &[(&str, i32)]) -> Nbt {
    Nbt::Compound(NbtCompound::from([
        (
            "content".to_owned(),
            Nbt::List(
                stacks
                    .iter()
                    .map(|(id, count)| {
                        Nbt::Compound(NbtCompound::from([
                            ("id".to_owned(), Nbt::String((*id).to_owned())),
                            ("count".to_owned(), Nbt::Int(*count)),
                        ]))
                    })
                    .collect(),
            ),
        ),
        ("size_multiplier".to_owned(), Nbt::Int(4)),
    ]))
}

fn run(jar_path: &Path) {
    let jar = NativeJar::from_path(jar_path).expect("load RUNES_JAR");
    let mut host = jar
        .try_seed_host(
            StdHost::new()
                .with_game_version("26.3")
                .with_environment_type("CLIENT"),
        )
        .expect("seed Runes client host");
    let budget = ExecutionBudget::UNBOUNDED;

    // Part A: on a client host, Bundle API's three bootstrap callbacks run, in the client's own
    // order, and nothing counts.
    assert_eq!(
        jar.run_bootstrap_static_injectors(&mut host, &budget),
        Vec::<String>::new()
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
    assert!(
        host.jar_component_types()
            .iter()
            .any(|kind| kind.id == CONTENTS && kind.persistent),
        "the client registers the contents type: {:?}",
        host.jar_component_types()
    );
    let properties = host.client_range_select_properties();
    assert_eq!(
        properties
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        [FULLNESS]
    );
    for entry in jar.discover_entrypoints("main") {
        jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
            .expect("run Runes main");
    }

    let pouch_item = item(&host, POUCH);
    let defaults: Vec<(String, Option<Nbt>)> = GAME_PROTOTYPE_COMPONENTS
        .iter()
        .map(|id| ((*id).to_owned(), None))
        .chain([(CONTENTS.to_owned(), None)])
        .collect();
    let pouch = |host: &mut StdHost, stacks: Option<&[(&str, i32)]>| {
        let patch = stacks
            .map(|stacks| {
                vec![(
                    CONTENTS.to_owned(),
                    Some(RuntimeComponentValue::Nbt(contents(stacks))),
                )]
            })
            .unwrap_or_default();
        jar.make_item_stack(
            host,
            Some(pouch_item.clone()),
            POUCH,
            1,
            patch,
            defaults.clone(),
            false,
            Vec::new(),
        )
        .expect("pouch stack")
    };
    // The game's prototype values of the runes a pouch holds, as an embedder hands them for every
    // item a stack's contents name (`CustomBundleContentsComponent.calculateOccupancy` asks each
    // one for the contents type and its max stack size).
    for rune in [
        RUNE,
        "runes:frost_stone",
        "runes:arcane_stone",
        "runes:healing_stone",
        "runes:lightning_stone",
    ] {
        let rune_item = item(&host, rune);
        jar.make_item_stack(
            &mut host,
            Some(rune_item),
            rune,
            1,
            Vec::new(),
            defaults.clone(),
            false,
            Vec::new(),
        )
        .expect("rune stack");
    }
    let empty = pouch(&mut host, None);
    let partly = pouch(&mut host, Some(&[(RUNE, 10)]));
    let full = pouch(
        &mut host,
        Some(&[(RUNE, 64), (RUNE, 64), (RUNE, 64), (RUNE, 64)]),
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
    let mixed = pouch(
        &mut host,
        Some(&[
            (RUNE, 1),
            ("runes:frost_stone", 1),
            ("runes:arcane_stone", 1),
            ("runes:healing_stone", 1),
            ("runes:lightning_stone", 1),
        ]),
    );

    // Step 7: the item definition's own node decodes through the jar's codec, and its property
    // answers each pouch's fullness, in a screen (no level, no holder) and in a hand.
    let (_, codec) = &properties[0];
    let node = pouch_model_node(jar_path);
    assert_eq!(node["property"], FULLNESS);
    let decoded = jar.decode_range_select_property(&mut host, codec, &node, &budget);
    let property = decoded
        .clone()
        .exact()
        .and_then(Result::ok)
        .unwrap_or_else(|| panic!("the fullness node decodes: {:?}", decoded.report));
    for (stack, expected) in [(&empty, 0.0_f32), (&partly, 0.0390625), (&full, 1.0)] {
        for holder in [
            ItemModelHolder::default(),
            ItemModelHolder {
                level: true,
                owner: true,
            },
        ] {
            let value =
                jar.range_select_property_value(&mut host, &property, stack, holder, 0, &budget);
            assert_eq!(
                value.clone().exact(),
                Some(expected),
                "{holder:?}: {:?}",
                value.report
            );
        }
    }

    // Step 8: the pouch's own `getTooltipImage`, Bundle API's `create` mapping, and the
    // component's size.
    let overrides = jar.item_overrides(&pouch_item, &host);
    let owner = overrides.owners[&ItemHook::TooltipImage].clone();
    let make_component = |host: &mut StdHost, stack: &Value| {
        let image = jar.item_tooltip_image(host, &pouch_item, &owner, stack, &budget);
        let data = image
            .clone()
            .exact()
            .flatten()
            .unwrap_or_else(|| panic!("a pouch has a tooltip image: {:?}", image.report));
        let created = jar.client_tooltip_component(host, &data, &budget);
        created
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("Bundle API maps its data: {:?}", created.report))
    };
    // `getTooltipImage` and `create` run as the item part's hook does, outside a frame; the
    // component measures and draws inside one.
    let components: Vec<(Value, i32)> = [(&partly, 45), (&full, 45), (&mixed, 69)]
        .into_iter()
        .map(|(stack, height)| (make_component(&mut host, stack), height))
        .collect();
    let empty_component = make_component(&mut host, &empty);
    host.begin_client_frame(std::sync::Arc::default(), &[]);
    for (component, height) in &components {
        let size = jar.tooltip_component_size(&mut host, component, &budget);
        let size = size
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("the size is exact: {:?}", size.report));
        assert_eq!((size.width, size.height), (96, *height));
    }
    // An empty pouch wraps its (empty) description to measure itself: `Font.split` counts.
    let size = jar.tooltip_component_size(&mut host, &empty_component, &budget);
    assert!(!size.report.exact());
    assert!(
        size.report
            .unknown_host_calls
            .keys()
            .any(|key| key.starts_with("net/minecraft/client/gui/Font#split")),
        "{:?}",
        size.report
    );
    // Its image draws its slots with 26.3's `blitSprite` (the `renderpearl` pipeline overload),
    // which the client surface counts.
    let drawn =
        jar.draw_tooltip_component_image(&mut host, &components[0].0, 8, 8, 96, 45, &budget);
    assert!(!drawn.report.exact(), "{:?}", drawn.report);
    host.end_client_frame();

    // A pouch on screen, drawn as the client bridge draws it every frame (#609 round 4 bug 1):
    // its fullness on the stack the last draw of the same game stack used, in a transient scope.
    // Every answer is the bytecode's `10/256`, nothing counts, the contents are decoded once, and
    // the heap and the stack rows don't grow with the draws.
    let counted = host.unknown_host_call_count();
    let fields = host.resident_heap().instance_fields();
    let request = ItemStackRequest {
        item_override: Some(pouch_item.clone()),
        item_id: POUCH.to_owned(),
        count: 1,
        patch: vec![(
            CONTENTS.to_owned(),
            Some(RuntimeComponentValue::Nbt(contents(&[(RUNE, 10)]))),
        )],
        defaults: defaults.clone(),
        named_prototypes: vec![(RUNE.to_owned(), defaults.clone())],
        ..ItemStackRequest::default()
    };
    let start = std::time::Instant::now();
    for draw in 0..2_000 {
        let value = host.with_transient_rows(|host| {
            let stack = jar
                .reusable_item_stack(host, request.clone())
                .expect("pouch stack");
            jar.range_select_property_value(
                host,
                &property,
                &stack,
                ItemModelHolder::default(),
                0,
                &budget,
            )
        });
        assert_eq!(
            value.clone().exact(),
            Some(0.0390625),
            "draw {draw}: {:?}",
            value.report
        );
    }
    eprintln!(
        "2000 pouch fullness draws: {:?}; cache {:?}; rows {:?}; fields {fields} -> {}",
        start.elapsed(),
        host.item_stack_cache_counts(),
        host.recipe_row_counts(),
        host.resident_heap().instance_fields()
    );
    assert_eq!(
        host.unknown_host_call_count(),
        counted,
        "{:?}",
        host.unknown_host_calls()
    );
    assert_eq!(host.item_stack_cache_counts(), (1, 1_999, 1));
    assert!(host.resident_heap().instance_fields() <= 2 * fields + 2048);
}
