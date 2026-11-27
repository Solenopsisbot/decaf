//! Real-jar memory check: a client draws a pouch every frame, and nothing a draw leaves behind
//! may outlive it (#609 round 5, bug C).
//!
//! In game, client memory grew about 30 KB per draw of a pouch with contents (1.3 GB to 3.9 GB in
//! 15 minutes) and frame hitches grew with it. The cause was the host's call log (`calls` and
//! `invocations`): decoding the pouch's contents component logged every host call the codec made
//! and never rewound them, so each draw added 13-23 KB, and every collection walked the whole
//! log for its read-back calls. A `subList` a tooltip makes also left its class row behind
//! (`sub_list_class_names`) for good.
//!
//! This test draws a pouch's fullness property, its item bar and its tooltip image (made and
//! drawn) as the client bridge does (`mod/src/client_item_parts.rs`, `mod/src/client.rs`): each
//! in a transient scope over a stack built fresh from the game's form. (The tooltip text and the
//! creative tab build ran outside a scope until #609 round 6 bug G, so each hovered frame and each
//! tab build rooted its rows for good; `mod/src/client_dispatch_tests.rs` pins those bridges.)
//! It checks two things:
//! - **Every host and jar table.** Each top-level field of `StdHost` and `NativeJar` is measured
//!   by its `Debug` text (both derive it, so no hand-kept list can miss a table). This is checked
//!   right after a collection, so garbage waiting for the next one doesn't count. No field may
//!   grow across the draws.
//! - **Every byte.** This binary's global allocator counts live heap bytes. That is everything the
//!   runtime holds, including what no `Debug` shows (capacities, caches behind locks).
//!
//! Every draw's answer must equal the first draw's, and nothing may count.
//!
//! `RUNES_JAR` names the jar; the test is skipped without it (CI has no corpus). The ignored
//! `pouch_draw_rss_probe` reports process RSS over 10,000 frames in a release build, and each
//! collection's time, live set and the draws between collections.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};

use horizon_native_java::{
    ExecutionBudget, GAME_PROTOTYPE_COMPONENTS, ItemHook, ItemModelHolder, ItemOverrides,
    ItemStackRequest, NativeJar, Nbt, NbtCompound, RuntimeComponentValue, StdHost, Value,
    client_surface::{ClientSnapshot, FontAnswers, FontQuestion, TextMetrics, WidthOrder},
};

/// The system allocator, counting the bytes live in this test binary.
struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

// SAFETY: every call forwards to `System` with the caller's own arguments; the counter is
// only arithmetic on the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: the caller's contract for `GlobalAlloc::alloc`, passed on.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: as `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        // SAFETY: `ptr` came from this allocator with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_add(
            new_size as isize - layout.size() as isize,
            Ordering::Relaxed,
        );
        // SAFETY: as `dealloc`, and `new_size` is the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// The heap bytes live in this process now.
fn live_bytes() -> isize {
    LIVE.load(Ordering::Relaxed)
}

/// Each top-level field of `value`'s pretty `Debug` text, and how many bytes it prints.
fn field_sizes(value: &impl std::fmt::Debug) -> BTreeMap<String, usize> {
    let text = format!("{value:#?}");
    let mut sizes = BTreeMap::new();
    let mut field: Option<&str> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("    ")
            && !rest.starts_with(' ')
            && let Some((name, _)) = rest.split_once(':')
        {
            field = Some(name);
        }
        if let Some(name) = field {
            *sizes.entry(name.to_owned()).or_insert(0) += line.len() + 1;
        }
    }
    sizes
}

/// What a host and its jar hold, right after a collection.
struct Snapshot {
    /// Draws run when it was taken.
    draw: usize,
    /// Live heap bytes when the collection ended, before the snapshot's own maps.
    live: isize,
    /// The bytes the snapshot's own maps hold, which stay live until it drops.
    own: isize,
    host: BTreeMap<String, usize>,
    jar: BTreeMap<String, usize>,
}

/// Fields whose `Debug` text grows by a few bytes once and then stops: counters gaining a digit
/// (`next_iterator: 99` to `100`) or the freed-number ranges splitting at a live object kept
/// across a collection. A field that grows per draw grows by far more over the run.
const FIELD_SLACK: usize = 64;

/// How many live bytes a draw may leave on average: allocator and `Vec` capacities settle,
/// they don't climb. A leaked call log was 13,000-23,000 bytes a draw, and the `subList` rows
/// about 120.
const BYTES_PER_DRAW: f64 = 24.0;

const POUCH: &str = "runes:small_rune_pouch";
const FIRE: &str = "runes:fire_stone";
const FROST: &str = "runes:frost_stone";
const CONTENTS: &str = "bundleapi:custom_bundle_contents";

/// The pouch's contents as the game stores them: `(item, count)` stacks, size multiplier 4.
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

/// The registered item `id`'s `Item` object.
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

/// A tooltip frame with no glyph table, as Sunrise's is, holding only the game font's answers.
fn answered_frame(answers: &FontAnswers) -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        screen_scope: true,
        metrics: Arc::new(TextMetrics {
            answers: answers.clone(),
            ..TextMetrics::default()
        }),
        ..ClientSnapshot::default()
    })
}

/// A stand-in for the game's font, asked text by text (`mod/src/client_item_parts.rs`'s
/// `answer_font_questions`). Its widths aren't checked here, only that they're asked and
/// answered the same way each draw.
fn game_font(questions: &[FontQuestion], answers: &mut FontAnswers) -> bool {
    let font = TextMetrics {
        advances: (0u8..128).map(|c| if c < 32 { 0.0 } else { 6.0 }).collect(),
        translations: [
            (
                "item.runes.rune_pouch.empty.description",
                "Can hold mixed stacks of runes",
            ),
            ("item.minecraft.bundle.empty", "Empty"),
            ("item.minecraft.bundle.full", "Full"),
        ]
        .iter()
        .map(|(key, text)| ((*key).to_owned(), (*text).to_owned()))
        .collect(),
        ..TextMetrics::default()
    };
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

/// The real jar's client host after bootstrap and `main`, and what a pouch draw needs.
struct Rig {
    jar: NativeJar,
    host: StdHost,
    pouch_item: Value,
    overrides: ItemOverrides,
    tooltip_owner: String,
    property: Value,
    /// A pouch holding 10 fire and 20 frost runes, in the form the bridge builds it from
    /// (`mod/src/item_parts.rs`'s `java_stack_request`): the contents patch, and the game's
    /// prototype values of every item the contents name.
    filled: ItemStackRequest,
    /// The same pouch with no contents.
    empty: ItemStackRequest,
}

impl Rig {
    fn new(jar_path: &Path) -> Self {
        let jar = NativeJar::from_path(jar_path).expect("load RUNES_JAR");
        let mut host = jar
            .try_seed_host(
                StdHost::new()
                    .with_game_version("26.3")
                    .with_environment_type("CLIENT"),
            )
            .expect("seed Runes client host");
        let budget = ExecutionBudget::UNBOUNDED;
        assert_eq!(
            jar.run_bootstrap_static_injectors(&mut host, &budget),
            Vec::<String>::new()
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
        let filled = ItemStackRequest {
            item_override: Some(pouch_item.clone()),
            item_id: POUCH.to_owned(),
            count: 1,
            patch: vec![(
                CONTENTS.to_owned(),
                Some(RuntimeComponentValue::Nbt(contents(&[
                    (FIRE, 10),
                    (FROST, 20),
                ]))),
            )],
            defaults: defaults.clone(),
            named_prototypes: vec![
                (FIRE.to_owned(), defaults.clone()),
                (FROST.to_owned(), defaults.clone()),
            ],
            ..ItemStackRequest::default()
        };
        let empty = ItemStackRequest {
            patch: Vec::new(),
            named_prototypes: Vec::new(),
            ..filled.clone()
        };
        let (_, codec) = host.client_range_select_properties()[0].clone();
        let decoded = jar.decode_range_select_property(
            &mut host,
            &codec,
            &pouch_model_node(jar_path),
            &budget,
        );
        let property = decoded
            .exact()
            .and_then(Result::ok)
            .expect("the fullness node decodes");
        let overrides = jar.item_overrides(&pouch_item, &host);
        let tooltip_owner = overrides.owners[&ItemHook::TooltipImage].clone();
        Self {
            jar,
            host,
            pouch_item,
            overrides,
            tooltip_owner,
            property,
            filled,
            empty,
        }
    }
}

/// One draw's answer, compared with the first draw's.
#[derive(Debug, Clone, PartialEq)]
enum Answer {
    Fullness(Option<f32>),
    Bar(Option<bool>, Option<i32>, Option<i32>),
    Tooltip(i32, i32, usize),
    Drawn(usize),
}

/// A draw phase as the client bridge runs it: one call, in its own transient scope.
type Draw = Box<dyn FnMut(&NativeJar, &mut StdHost) -> Answer>;

/// `getTooltipImage`, then `ClientTooltipComponent.create` on its data, as
/// `ItemPart::tooltip_image` runs them.
fn make_component(
    jar: &NativeJar,
    host: &mut StdHost,
    item: &Value,
    owner: &str,
    stack: &Value,
) -> Value {
    let budget = ExecutionBudget::UNBOUNDED;
    let data = jar
        .item_tooltip_image(host, item, owner, stack, &budget)
        .exact()
        .flatten()
        .expect("a pouch has a tooltip image");
    let component = jar
        .client_tooltip_component(host, &data, &budget)
        .exact()
        .expect("Bundle API maps its data");
    host.release(&data);
    component
}

/// The fullness model property's `get` on a stack built from `request`.
fn fullness(request: ItemStackRequest, property: Value) -> Draw {
    Box::new(move |jar, host| {
        host.with_transient_rows(|host| {
            let stack = jar.build_item_stack(host, request.clone()).expect("stack");
            let value = jar.range_select_property_value(
                host,
                &property,
                &stack,
                ItemModelHolder::default(),
                0,
                &ExecutionBudget::UNBOUNDED,
            );
            Answer::Fullness(value.exact())
        })
    })
}

/// The item bar's three calls on a stack built from `request`.
fn bar(request: ItemStackRequest, item: Value, overrides: ItemOverrides) -> Draw {
    Box::new(move |jar, host| {
        host.with_transient_rows(|host| {
            let stack = jar.build_item_stack(host, request.clone()).expect("stack");
            let bar = jar
                .item_bar(
                    host,
                    &item,
                    &overrides,
                    &stack,
                    false,
                    &ExecutionBudget::UNBOUNDED,
                )
                .expect("item bar");
            assert!(bar.report.exact(), "{:?}", bar.report);
            Answer::Bar(bar.visible, bar.width, bar.color)
        })
    })
}

/// The phases a frame with a pouch on screen runs (`ItemPart::item_bar`, the fullness model
/// property, `ItemPart::tooltip_image` and the tooltip component's `draw_image`), each as its
/// bridge runs it. The tooltip draw's component is retained, as `JavaTooltipData` holds it.
fn phases(rig: &mut Rig) -> Vec<(&'static str, Draw)> {
    let budget = ExecutionBudget::UNBOUNDED;
    let tooltip_make: Draw = {
        let (request, item, owner) = (
            rig.filled.clone(),
            rig.pouch_item.clone(),
            rig.tooltip_owner.clone(),
        );
        Box::new(move |jar, host| {
            let (component, image) = host.with_transient_rows(|host| {
                let stack = jar.build_item_stack(host, request.clone()).expect("stack");
                let component = make_component(jar, host, &item, &owner, &stack);
                let image = jar.tooltip_image_with_font(
                    host,
                    &component,
                    &answered_frame,
                    16,
                    &mut game_font,
                    &budget,
                );
                (component, image)
            });
            host.release(&component);
            let image = image.expect("the tooltip image measures");
            Answer::Tooltip(image.size.width, image.size.height, image.draws.len())
        })
    };
    // The component `ItemPart::tooltip_image` made and Sunrise keeps while the tooltip shows.
    let (component, answers, size) = {
        let Rig {
            jar,
            host,
            filled,
            pouch_item,
            tooltip_owner,
            ..
        } = rig;
        host.with_transient_rows(|host| {
            let stack = jar.build_item_stack(host, filled.clone()).expect("stack");
            let component = make_component(jar, host, pouch_item, tooltip_owner, &stack);
            let image = jar
                .tooltip_image_with_font(
                    host,
                    &component,
                    &answered_frame,
                    16,
                    &mut game_font,
                    &budget,
                )
                .expect("the tooltip image measures");
            (component, image.answers, image.size)
        })
    };
    let tooltip_draw: Draw = Box::new(move |jar, host| {
        let drawn = host.with_transient_rows(|host| {
            host.begin_client_frame(answered_frame(&answers), &[]);
            let drawn = jar.draw_tooltip_component_image(
                host,
                &component,
                8,
                8,
                size.width,
                size.height,
                &budget,
            );
            host.end_client_frame();
            drawn
        });
        Answer::Drawn(drawn.exact().expect("the image draws exactly").len())
    });
    vec![
        (
            "fullness, filled",
            fullness(rig.filled.clone(), rig.property.clone()),
        ),
        (
            "item bar, filled",
            bar(
                rig.filled.clone(),
                rig.pouch_item.clone(),
                rig.overrides.clone(),
            ),
        ),
        ("tooltip make, filled", tooltip_make),
        ("tooltip draw, filled", tooltip_draw),
        (
            "fullness, empty",
            fullness(rig.empty.clone(), rig.property.clone()),
        ),
        (
            "item bar, empty",
            bar(
                rig.empty.clone(),
                rig.pouch_item.clone(),
                rig.overrides.clone(),
            ),
        ),
    ]
}

/// Run `draw` until the host's next collection, checking each answer, and snapshot what the
/// host and jar hold then. Panics when `limit` draws ran without one.
fn until_collected(
    jar: &NativeJar,
    host: &mut StdHost,
    draw: &mut Draw,
    first: &Answer,
    drawn: &mut usize,
    name: &str,
) -> Snapshot {
    const LIMIT: usize = 20_000;
    let collections = host.collection_count();
    for _ in 0..LIMIT {
        let answer = draw(jar, host);
        assert_eq!(&answer, first, "{name}: draw {drawn}");
        *drawn += 1;
        if host.collection_count() != collections {
            // Read before the snapshot allocates its own maps, which are counted apart.
            let live = live_bytes();
            let (host, jar) = (field_sizes(&*host), field_sizes(jar));
            return Snapshot {
                draw: *drawn,
                live,
                own: live_bytes() - live,
                host,
                jar,
            };
        }
    }
    panic!("{name}: no collection in {LIMIT} draws");
}

/// The fields of `after` that printed more than `before` by over [`FIELD_SLACK`] bytes.
fn grown(
    owner: &str,
    before: &BTreeMap<String, usize>,
    after: &BTreeMap<String, usize>,
) -> Vec<String> {
    after
        .iter()
        .filter_map(|(name, &size)| {
            let was = before.get(name).copied().unwrap_or(0);
            (size > was + FIELD_SLACK).then(|| format!("{owner}.{name}: {was} -> {size} bytes"))
        })
        .collect()
}

#[test]
fn pouch_draws_leave_no_host_or_jar_table_growing() {
    let Some(jar_path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes draw growth test skipped");
        return;
    };
    std::thread::Builder::new()
        .name("runes-draw-growth".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || check(Path::new(&jar_path), 1_000))
        .expect("spawn test thread")
        .join()
        .unwrap();
}

/// Each phase: warm up, run to a collection (snapshot A), `draws` more, run to the next
/// collection (snapshot B). Nothing may grow from A to B, and nothing may count. Every phase
/// is checked before the test fails, so one run names every table that grew.
fn check(jar_path: &Path, draws: usize) {
    let mut rig = Rig::new(jar_path);
    let counted = rig.host.unknown_host_call_count();
    let mut failures = Vec::new();
    for (name, mut draw) in phases(&mut rig) {
        let Rig { jar, host, .. } = &mut rig;
        let first = draw(jar, host);
        let mut drawn = 1;
        // Warm up: the first draws initialise classes and settle the caches.
        for _ in 0..200 {
            assert_eq!(draw(jar, host), first, "{name}: warm-up");
            drawn += 1;
        }
        let a = until_collected(jar, host, &mut draw, &first, &mut drawn, name);
        for _ in 0..draws {
            assert_eq!(draw(jar, host), first, "{name}: draw {drawn}");
            drawn += 1;
        }
        let b = until_collected(jar, host, &mut draw, &first, &mut drawn, name);
        // A's maps are still live when B is read.
        let per_draw = (b.live - a.live - a.own) as f64 / (b.draw - a.draw) as f64;
        eprintln!(
            "{name:<22} {first:?}: draws {}..{}, live bytes {} -> {} ({per_draw:+.1} a draw), \
             collections {}",
            a.draw,
            b.draw,
            a.live,
            b.live,
            host.collection_count()
        );
        let mut grew = grown("host", &a.host, &b.host);
        grew.extend(grown("jar", &a.jar, &b.jar));
        if per_draw > BYTES_PER_DRAW {
            grew.push(format!(
                "live bytes: {per_draw:.1} a draw over {} draws",
                b.draw - a.draw
            ));
        }
        failures.extend(grew.into_iter().map(|line| format!("{name}: {line}")));
        assert_eq!(
            host.unknown_host_call_count(),
            counted,
            "{name}: {:?}",
            host.unknown_host_calls()
        );
    }
    assert!(
        failures.is_empty(),
        "grew across draws:\n{}",
        failures.join("\n")
    );
}

/// The process's resident set (`VmRSS`), in KiB; `None` off Linux.
fn rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()
}

/// Process RSS and live heap bytes over 10,000 frames, each drawing every phase once (a filled
/// and an empty pouch's fullness and bar, a filled pouch's tooltip made and drawn). Not a
/// check: run it in a release build,
/// `RUNES_JAR=... cargo test --release --test fixture_runes_draw_growth -- --ignored --nocapture`.
#[test]
#[ignore = "an RSS probe: run it in a release build"]
fn pouch_draw_rss_probe() {
    let Some(jar_path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; pouch draw RSS probe skipped");
        return;
    };
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || rss_probe(Path::new(&jar_path)))
        .unwrap()
        .join()
        .unwrap();
}

fn rss_probe(jar_path: &Path) {
    let mut rig = Rig::new(jar_path);
    let mut phases = phases(&mut rig);
    let Rig { jar, host, .. } = &mut rig;
    let phases_per_frame = phases.len();
    let mut frame = |host: &mut StdHost| {
        for (_, draw) in phases.iter_mut() {
            draw(jar, host);
        }
    };
    for _ in 0..200 {
        frame(host);
    }
    let (rss0, live0) = (rss_kib().unwrap_or(0), live_bytes());
    let start = std::time::Instant::now();
    // Each collection's time and live set, and the draws since the one before.
    let mut collections: Vec<(f64, usize, usize)> = Vec::new();
    let (mut seen, mut since) = (host.collection_count(), 0usize);
    let mut frame = |host: &mut StdHost| {
        frame(host);
        since += phases_per_frame;
        if host.collection_count() != seen {
            seen = host.collection_count();
            let stats = host.last_collection().expect("a collection ran");
            collections.push((
                stats.elapsed.as_secs_f64() * 1e3,
                stats.entries_after,
                since,
            ));
            since = 0;
        }
    };
    // The slowest frames of each block show whether collections slow down with uptime.
    eprintln!("frames  RSS MiB  live MiB  collections  frame p99 ms  max ms");
    for step in 1..=10 {
        let mut times: Vec<f64> = (0..1_000)
            .map(|_| {
                let started = std::time::Instant::now();
                frame(host);
                started.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        times.sort_by(f64::total_cmp);
        eprintln!(
            "{:>6}  {:>7.1}  {:>8.1}  {:>11}  {:>12.2}  {:>6.2}",
            step * 1_000,
            rss_kib().unwrap_or(0) as f64 / 1024.0,
            live_bytes() as f64 / (1024.0 * 1024.0),
            host.collection_count(),
            times[989],
            times[999]
        );
    }
    drop(frame);
    let mut times: Vec<f64> = collections.iter().map(|(ms, _, _)| *ms).collect();
    times.sort_by(f64::total_cmp);
    let mut gaps: Vec<usize> = collections.iter().map(|(_, _, since)| *since).collect();
    gaps.sort_unstable();
    if !times.is_empty() {
        let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
        eprintln!(
            "{} collections: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms; draws between p50 {}, max {}; \
             live entries max {}",
            times.len(),
            at(0.5),
            at(0.99),
            times[times.len() - 1],
            gaps[gaps.len() / 2],
            gaps[gaps.len() - 1],
            collections
                .iter()
                .map(|(_, live, _)| *live)
                .max()
                .unwrap_or(0)
        );
    }
    let draws = 10_000.0 * phases_per_frame as f64;
    eprintln!(
        "10,000 frames ({draws} draws) in {:?}: RSS {:+.1} bytes a draw, live {:+.1} bytes a draw",
        start.elapsed(),
        (rss_kib().unwrap_or(0) as f64 - rss0 as f64) * 1024.0 / draws,
        (live_bytes() - live0) as f64 / draws
    );
}
