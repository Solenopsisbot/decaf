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
//!   `getNumVisibleSlots`).
//! - The empty, partly full and full pouches' tooltip images, measured and drawn in a tooltip's
//!   screen frame whose font is the advance table `fixture_client_font.rs` uses, are
//!   `fixtures/runes_client_items/java25.txt`: `oracle/PouchTooltipOracle.java` on Java 25.0.5+8,
//!   Bundle API's own `CustomBundleTooltipComponent` on the real 26.3 client's `Font`,
//!   `ClientLanguage` and `GuiGraphicsExtractor` (see the fixture's README). Every size and draw
//!   is exact, nothing counted: the description wraps with `Font.split`, the slots and the bar are
//!   26.3 `blitSprite`s on `GUI_TEXTURED`, the runes are `item`/`itemDecorations` of the
//!   contents' own stacks, and the labels are centred with `Font.width`.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use horizon_native_java::{
    ExecutionBudget, GAME_PROTOTYPE_COMPONENTS, ItemHook, ItemModelHolder, ItemStackRequest,
    NativeJar, Nbt, NbtCompound, RuntimeComponentValue, StdHost, Value,
    client_surface::{
        ClientSnapshot, ClientText, DrawOp, FontAnswers, FontQuestion, TextMetrics, WidthOrder,
    },
};

const JAVA25: &str = include_str!("fixtures/runes_client_items/java25.txt");

/// The translations the oracle's `ClientLanguage` holds: Runes 1.3.2's en_us for the pouch's
/// description, vanilla 26.3's en_us for the bundle labels Bundle API draws.
const TRANSLATIONS: &[(&str, &str)] = &[
    (
        "item.runes.rune_pouch.empty.description",
        "Can hold mixed stacks of runes",
    ),
    ("item.minecraft.bundle.empty", "Empty"),
    ("item.minecraft.bundle.full", "Full"),
];

/// The oracle's advance table (`PouchTooltipOracle.advance`, `fixture_client_font.rs`'s too).
fn advances() -> Vec<f32> {
    (0u8..128)
        .map(|c| {
            let c = c as char;
            match c {
                c if (c as u32) < 32 => 0.0,
                ' ' => 4.0,
                c if "i!.,:;'|".contains(c) => 2.0,
                'l' => 3.0,
                c if "tI[]".contains(c) => 4.0,
                c if "fk<>{}()\"*".contains(c) => 5.0,
                'q' => 5.5,
                '@' | '~' => 7.0,
                _ => 6.0,
            }
        })
        .collect()
}

/// The oracle's font: its advance table and translations.
fn oracle_font() -> TextMetrics {
    TextMetrics {
        advances: advances(),
        translations: TRANSLATIONS
            .iter()
            .map(|(key, text)| ((*key).to_owned(), (*text).to_owned()))
            .collect(),
        ..TextMetrics::default()
    }
}

/// A tooltip's screen frame (the embedder's is one) whose font is `metrics`.
fn screen_frame(metrics: TextMetrics) -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        screen_scope: true,
        metrics: Arc::new(metrics),
        ..ClientSnapshot::default()
    })
}

/// A tooltip frame whose font is the oracle's glyph table.
fn tooltip_frame() -> Arc<ClientSnapshot> {
    screen_frame(oracle_font())
}

/// A tooltip frame with no glyph table, as Sunrise's is, holding only the game font's answers.
fn answered_frame(answers: &FontAnswers) -> Arc<ClientSnapshot> {
    screen_frame(TextMetrics {
        answers: answers.clone(),
        ..TextMetrics::default()
    })
}

/// A fake embedder whose game font is the oracle's, asked text by text as Sunrise's
/// `text_width` / `text_split` are (`mod/src/client_item_parts.rs`).
fn game_font(questions: &[FontQuestion], answers: &mut FontAnswers) -> bool {
    let font = oracle_font();
    questions.iter().all(|question| match question {
        FontQuestion::Width(text) => font
            .exact_width(text, WidthOrder::Logical)
            .map(|width| answers.widths.insert(text.clone(), width))
            .is_some(),
        FontQuestion::Split(text, width) => font
            .exact_split(text, *width)
            .map(|lines| answers.splits.insert((text.clone(), *width), lines))
            .is_some(),
    })
}

/// A recorded draw as the oracle prints it.
fn print(op: &DrawOp) -> String {
    let translations: BTreeMap<&str, &str> = TRANSLATIONS.iter().copied().collect();
    match op {
        DrawOp::Text {
            text,
            at: [x, y],
            argb,
            shadow,
            scale: [1.0, 1.0],
        } => {
            let text = match text {
                ClientText::Literal(text) => text.as_str(),
                ClientText::Translatable(key) => translations[key.as_str()],
                ClientText::Json(json) => panic!("a styled text: {json}"),
            };
            format!(
                "  text \"{text}\" {} {} {} {shadow}",
                *x as i32, *y as i32, *argb as i32
            )
        }
        DrawOp::Sprite {
            sprite,
            rect: [x, y, w, h],
            argb,
        } => format!(
            "  sprite gui_textured minecraft:{sprite} {} {} {} {} {}",
            *x as i32, *y as i32, *w as i32, *h as i32, *argb as i32
        ),
        DrawOp::Stack {
            stack,
            at: [x, y],
            scale: [1.0, 1.0],
            model,
            seed,
            decorations,
        } => {
            assert!(
                stack.patch.is_empty() && !stack.opaque_components,
                "{stack:?}"
            );
            match (model, decorations) {
                (true, None) => format!(
                    "  item {} {} {} {} {seed}",
                    stack.item_id, stack.count, *x as i32, *y as i32
                ),
                (false, Some(None)) => format!(
                    "  decorations {} {} {} {}",
                    stack.item_id, stack.count, *x as i32, *y as i32
                ),
                other => panic!("unexpected stack draw {other:?}"),
            }
        }
        other => panic!("unexpected draw {other:?}"),
    }
}

/// A zero-sized sprite (the oracle's recorder prints it from `blitSprite`'s colour overload: the
/// empty pouch's bar fill is 0 wide) draws nothing in 26.3 in any `gui.scaling`
/// (`fixture_client_font.rs`'s `draws_something`), so the surface records none.
fn draws_something(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("  sprite ") else {
        return true;
    };
    let fields: Vec<&str> = rest.split(' ').collect();
    fields[4] != "0" && fields[5] != "0"
}

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
    // component measures and draws inside one, a tooltip's screen frame.
    let mixed_component = make_component(&mut host, &mixed);
    host.begin_client_frame(tooltip_frame(), &[]);
    let size = jar.tooltip_component_size(&mut host, &mixed_component, &budget);
    let size = size
        .clone()
        .exact()
        .unwrap_or_else(|| panic!("the size is exact: {:?}", size.report));
    assert_eq!((size.width, size.height), (96, 69));
    host.end_client_frame();

    // The oracle's three pouches: each answers an image, every size and draw exact.
    let mut printed = Vec::new();
    for (name, stack) in [("empty", &empty), ("partly", &partly), ("full", &full)] {
        let component = make_component(&mut host, stack);
        host.begin_client_frame(tooltip_frame(), &[]);
        let size = jar.tooltip_component_size(&mut host, &component, &budget);
        let size = size
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("{name}: the size is exact: {:?}", size.report));
        printed.push(format!(
            "pouch {name} width={} height={}",
            size.width, size.height
        ));
        printed.push(format!("  draw at 8,8 box {}x{}", size.width, size.height));
        let drawn = jar.draw_tooltip_component_image(
            &mut host,
            &component,
            8,
            8,
            size.width,
            size.height,
            &budget,
        );
        let draws = drawn
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("{name}: the image is exact: {:?}", drawn.report));
        let out = host.end_client_frame();
        assert_eq!(out.font_questions, Vec::new(), "{name}");
        printed.extend(draws.iter().map(print));
        host.release(&component);
    }
    let expected: Vec<&str> = JAVA25
        .lines()
        .filter(|line| draws_something(line))
        .collect();
    assert_eq!(printed, expected);

    // The path Sunrise runs: no glyph table, each text asked of the game's font between runs
    // (`NativeJar::tooltip_image_with_font`), the image drawn later with the answers. The same
    // sizes and draws, and the probes' counts rewound.
    let mut sunrise = Vec::new();
    for (name, stack) in [("empty", &empty), ("partly", &partly), ("full", &full)] {
        let component = make_component(&mut host, stack);
        let image = jar
            .tooltip_image_with_font(
                &mut host,
                &component,
                &answered_frame,
                8,
                &mut game_font,
                &budget,
            )
            .unwrap_or_else(|| panic!("{name}: no image: {:?}", host.unknown_host_calls()));
        let size = image.size;
        sunrise.push(format!(
            "pouch {name} width={} height={}",
            size.width, size.height
        ));
        sunrise.push(format!("  draw at 8,8 box {}x{}", size.width, size.height));
        host.begin_client_frame(answered_frame(&image.answers), &[]);
        let drawn = jar.draw_tooltip_component_image(
            &mut host,
            &component,
            8,
            8,
            size.width,
            size.height,
            &budget,
        );
        host.end_client_frame();
        let draws = drawn
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("{name}: {:?}", drawn.report));
        sunrise.extend(draws.iter().map(print));
        host.release(&component);
    }
    assert_eq!(sunrise, expected);
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
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
