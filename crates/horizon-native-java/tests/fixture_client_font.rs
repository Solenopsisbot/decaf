//! The client surface's `Font.width` / `Font.split` and 26.3 `blitSprite` (`client_font.rs`,
//! `client_surface.rs`), against Java 25 and the real 26.3 client.
//!
//! `fixtures/client_font/demo/` holds a tooltip component that wraps a text to 40 pixels
//! (`Font.split` for its height, `textWithWordWrap` to draw it), measures its lines, a component
//! and a `String` (`Font.width`, three `centeredText` overloads), and draws GUI sprites on
//! `RenderPipelines.GUI_TEXTURED` through each whole-sprite `blitSprite` overload, one of them zero
//! wide. `java25.txt` is `oracle/FontOracle.java` on Java 25.0.5+8 against the Minecraft 26.3
//! client jar: the same demo on vanilla's own `Font` (over a glyph provider with this test's
//! advance table, [`advances`]), `StringSplitter`, `ClientLanguage` and `GuiGraphicsExtractor`.
//! This test runs the demo on a client host in a tooltip's screen frame whose metrics are that
//! table, and prints the same lines from the recorded draws.
//!
//! Counted cases (never in `java25.txt`): a glyph past the table, a translation the frame lacks,
//! another pipeline, a scaled pose; and the embedder's answers (`FontAnswers`), which answer
//! what the glyph model can't.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use std::collections::BTreeMap;
use std::sync::Arc;

use horizon_native_java::{
    ExecutionBudget, ItemHook, StdHost, Value,
    client_surface::{
        ClientSnapshot, ClientText, DrawOp, FontAnswers, FontQuestion, TextMetrics, WidthOrder,
    },
};

const JAVA25: &str = include_str!("fixtures/client_font/java25.txt");
/// `oracle/FontCasesOracle.java`: 26.3's `Font.split` / `Font.width` of single inputs.
const CASES25: &str = include_str!("fixtures/client_font/cases25.txt");

const MOD_JSON: &str = r#"{"schemaVersion":1,"id":"demo","version":"1.0.0","environment":"*","entrypoints":{"main":["demo.Probe"]},"mixins":["demo.mixins.json"]}"#;
const MIXINS_JSON: &str =
    r#"{"required":true,"package":"demo.mixin","mixins":[],"client":["CreateMixin"]}"#;

/// The translations the oracle's `ClientLanguage` holds.
const TRANSLATIONS: &[(&str, &str)] = &[
    ("demo.label.description", "Can hold mixed stacks of runes"),
    ("demo.label.title", "Pouch Label"),
];

/// Compile-only stubs for every platform class the demo names.
const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api; public interface ModInitializer { void onInitialize(); }",
    ),
    (
        "org/spongepowered/asm/mixin/Mixin.java",
        "package org.spongepowered.asm.mixin;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.CLASS) @Target(ElementType.TYPE)\npublic @interface Mixin { Class<?>[] value() default {}; }",
    ),
    (
        "org/spongepowered/asm/mixin/injection/At.java",
        "package org.spongepowered.asm.mixin.injection;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.RUNTIME) @Target({})\npublic @interface At { String value() default \"\"; }",
    ),
    (
        "org/spongepowered/asm/mixin/injection/Inject.java",
        "package org.spongepowered.asm.mixin.injection;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.RUNTIME) @Target(ElementType.METHOD)\npublic @interface Inject { String[] method(); At[] at(); boolean cancellable() default false; }",
    ),
    (
        "org/spongepowered/asm/mixin/injection/callback/CallbackInfo.java",
        "package org.spongepowered.asm.mixin.injection.callback; public class CallbackInfo { public void cancel() {} }",
    ),
    (
        "org/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable.java",
        "package org.spongepowered.asm.mixin.injection.callback; public class CallbackInfoReturnable<R> extends CallbackInfo { public void setReturnValue(R value) {} }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { throw new AssertionError(); } public static Identifier withDefaultNamespace(String p) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources; public final class ResourceKey<T> { public static <T> ResourceKey<T> create(ResourceKey<? extends net.minecraft.core.Registry<T>> r, Identifier id) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core; import net.minecraft.resources.Identifier; public interface Registry<T> { static <V, T extends V> T register(Registry<V> r, Identifier id, T value) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        "package net.minecraft.core; public interface DefaultedRegistry<T> extends Registry<T> {}",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.*; import net.minecraft.world.item.Item; public final class BuiltInRegistries { public static final DefaultedRegistry<Item> ITEM = null; }",
    ),
    (
        "net/minecraft/core/registries/Registries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.Registry; import net.minecraft.resources.ResourceKey; import net.minecraft.world.item.Item; public final class Registries { public static final ResourceKey<Registry<Item>> ITEM = null; }",
    ),
    (
        "net/minecraft/world/inventory/tooltip/TooltipComponent.java",
        "package net.minecraft.world.inventory.tooltip; public interface TooltipComponent {}",
    ),
    (
        "net/minecraft/network/chat/FormattedText.java",
        "package net.minecraft.network.chat; public interface FormattedText {}",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat; public interface Component extends FormattedText { static MutableComponent translatable(String key) { throw new AssertionError(); } static MutableComponent literal(String text) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat; public final class MutableComponent implements Component {}",
    ),
    (
        "net/minecraft/util/FormattedCharSequence.java",
        "package net.minecraft.util; public interface FormattedCharSequence {}",
    ),
    (
        "com/mojang/renderpearl/api/pipeline/RenderPipeline.java",
        "package com.mojang.renderpearl.api.pipeline; public class RenderPipeline {}",
    ),
    (
        "net/minecraft/client/renderer/RenderPipelines.java",
        "package net.minecraft.client.renderer; import com.mojang.renderpearl.api.pipeline.RenderPipeline; public class RenderPipelines { public static final RenderPipeline GUI_TEXTURED = null; public static final RenderPipeline GUI_TEXTURED_PREMULTIPLIED_ALPHA = null; }",
    ),
    (
        "net/minecraft/client/gui/Font.java",
        "package net.minecraft.client.gui;\nimport java.util.List; import net.minecraft.network.chat.FormattedText; import net.minecraft.util.FormattedCharSequence;\npublic class Font {\n  public int width(String s) { throw new AssertionError(); }\n  public int width(FormattedText t) { throw new AssertionError(); }\n  public int width(FormattedCharSequence t) { throw new AssertionError(); }\n  public List<FormattedCharSequence> split(FormattedText t, int width) { throw new AssertionError(); }\n}",
    ),
    (
        "org/joml/Matrix3x2fStack.java",
        "package org.joml; public class Matrix3x2fStack { public Matrix3x2fStack pushMatrix() { return this; } public Matrix3x2fStack popMatrix() { return this; } public Matrix3x2fStack scale(float x, float y) { return this; } }",
    ),
    (
        "net/minecraft/client/gui/GuiGraphicsExtractor.java",
        "package net.minecraft.client.gui;\nimport com.mojang.renderpearl.api.pipeline.RenderPipeline; import net.minecraft.network.chat.*; import net.minecraft.resources.Identifier; import net.minecraft.util.FormattedCharSequence;\npublic class GuiGraphicsExtractor {\n  public org.joml.Matrix3x2fStack pose() { throw new AssertionError(); }\n  public int textWithWordWrap(Font f, FormattedText t, int x, int y, int w, int c) { throw new AssertionError(); }\n  public void blitSprite(RenderPipeline p, Identifier s, int x, int y, int w, int h) { throw new AssertionError(); }\n  public void blitSprite(RenderPipeline p, Identifier s, int x, int y, int w, int h, int c) { throw new AssertionError(); }\n  public void blitSprite(RenderPipeline p, Identifier s, int x, int y, int w, int h, float a) { throw new AssertionError(); }\n  public void centeredText(Font f, FormattedCharSequence t, int x, int y, int c) { throw new AssertionError(); }\n  public void centeredText(Font f, Component t, int x, int y, int c) { throw new AssertionError(); }\n  public void centeredText(Font f, String t, int x, int y, int c) { throw new AssertionError(); }\n}",
    ),
    (
        "net/minecraft/client/gui/screens/inventory/tooltip/ClientTooltipComponent.java",
        "package net.minecraft.client.gui.screens.inventory.tooltip;\nimport net.minecraft.client.gui.*; import net.minecraft.world.inventory.tooltip.TooltipComponent;\npublic interface ClientTooltipComponent {\n  static ClientTooltipComponent create(TooltipComponent data) { throw new AssertionError(); }\n  int getHeight(Font font);\n  int getWidth(Font font);\n  default void extractImage(Font font, int x, int y, int w, int h, GuiGraphicsExtractor g) {}\n}",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item; public final class ItemStack { public int getCount() { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/world/item/Item.java",
        "package net.minecraft.world.item;\nimport java.util.Optional; import net.minecraft.resources.ResourceKey; import net.minecraft.world.inventory.tooltip.TooltipComponent;\npublic class Item {\n  public static class Properties {\n    public Properties setId(ResourceKey<Item> key) { return this; }\n    public Properties stacksTo(int n) { return this; }\n  }\n  public Item(Properties p) {}\n  public Optional<TooltipComponent> getTooltipImage(ItemStack stack) { return Optional.empty(); }\n}",
    ),
];

/// The demo with `tooltip` as its component's source (the oracle's, or a counted variant's).
fn fixture(test: &str, tooltip: &str, stubs: &[(&str, &str)]) -> javac_fixture::Fixture {
    let mut sources = STUBS.to_vec();
    sources.extend_from_slice(stubs);
    sources.extend_from_slice(&[
        (
            "demo/Label.java",
            include_str!("fixtures/client_font/demo/Label.java"),
        ),
        (
            "demo/LabelItem.java",
            include_str!("fixtures/client_font/demo/LabelItem.java"),
        ),
        ("demo/LabelTooltip.java", tooltip),
        (
            "demo/Probe.java",
            include_str!("fixtures/client_font/demo/Probe.java"),
        ),
        (
            "demo/mixin/CreateMixin.java",
            include_str!("fixtures/client_font/demo/mixin/CreateMixin.java"),
        ),
    ]);
    javac_fixture::Fixture::build_with_mod_json_and_extra(
        test,
        &sources,
        MOD_JSON,
        &[("demo.mixins.json", MIXINS_JSON.as_bytes())],
    )
}

/// The oracle's advance table (`FontOracle.advance`), for chars 0..127.
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

fn metrics() -> TextMetrics {
    TextMetrics {
        advances: advances(),
        translations: TRANSLATIONS
            .iter()
            .map(|(key, text)| ((*key).to_owned(), (*text).to_owned()))
            .collect(),
        ..TextMetrics::default()
    }
}

/// A tooltip's screen frame (as the embedder's is) with `metrics`.
fn frame(metrics: TextMetrics) -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        screen_scope: true,
        gui_size: [320, 240],
        metrics: Arc::new(metrics),
        ..ClientSnapshot::default()
    })
}

/// A client host that has run the entrypoint, and the label's tooltip component for `kind`.
struct Demo {
    fixture: javac_fixture::Fixture,
    host: StdHost,
}

impl Demo {
    fn new(test: &str, tooltip: &str) -> Self {
        Self::new_with_stubs(test, tooltip, &[])
    }

    /// [`Self::new`] with more compile-only stubs.
    fn new_with_stubs(test: &str, tooltip: &str, stubs: &[(&str, &str)]) -> Self {
        let fixture = fixture(test, tooltip, stubs);
        let mut host = fixture
            .jar
            .seed_host(StdHost::new().with_environment_type("CLIENT"));
        assert_eq!(
            fixture
                .jar
                .run_bootstrap_static_injectors(&mut host, &ExecutionBudget::UNBOUNDED),
            Vec::<String>::new()
        );
        let entry = fixture.jar.discover_entrypoint("main").unwrap();
        fixture
            .jar
            .execute(&entry.class, entry.method.as_deref(), "main", &mut host)
            .unwrap();
        Self { fixture, host }
    }

    fn component(&mut self, kind: i32) -> Value {
        let jar = &self.fixture.jar;
        let budget = ExecutionBudget::UNBOUNDED;
        let item = self
            .host
            .registrations()
            .iter()
            .find(|entry| entry.registry == "minecraft:item" && entry.id == "demo:label")
            .map(|entry| entry.value.clone())
            .expect("demo:label is registered");
        let owner = jar.item_overrides(&item, &self.host).owners[&ItemHook::TooltipImage].clone();
        let stack = jar
            .make_item_stack(
                &mut self.host,
                Some(item.clone()),
                "demo:label",
                kind,
                Vec::new(),
                Vec::new(),
                false,
                Vec::new(),
            )
            .unwrap();
        let data = jar
            .item_tooltip_image(&mut self.host, &item, &owner, &stack, &budget)
            .exact()
            .flatten()
            .expect("a label");
        jar.client_tooltip_component(&mut self.host, &data, &budget)
            .exact()
            .expect("CreateMixin maps it")
    }
}

/// A recorded draw as the oracle prints it. A translatable text prints its translation, as the
/// oracle's `FormattedCharSequence` spells it.
fn print(op: &DrawOp, translations: &BTreeMap<String, String>) -> String {
    match op {
        DrawOp::Text {
            text,
            at: [x, y],
            argb,
            shadow,
            scale,
        } => {
            assert_eq!(*scale, [1.0, 1.0]);
            let text = match text {
                ClientText::Literal(text) => text.clone(),
                ClientText::Translatable(key) => translations[key].clone(),
                ClientText::Json(json) => panic!("a styled text in the demo: {json}"),
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
        } => {
            let id = if sprite.contains(':') {
                sprite.clone()
            } else {
                format!("minecraft:{sprite}")
            };
            format!(
                "  sprite gui_textured {id} {} {} {} {} {}",
                *x as i32, *y as i32, *w as i32, *h as i32, *argb as i32
            )
        }
        other => panic!("unexpected draw {other:?}"),
    }
}

/// A zero-sized sprite (the oracle's recorder prints it from `blitSprite`'s colour overload):
/// 26.3 draws nothing for one in any `gui.scaling` (`Stretch` returns at offsets 0-10 of
/// `blitSprite(RenderPipeline, TextureAtlasSprite, IIIII)`, `Tile` at 0-10 of `blitTiledSprite`,
/// and a `NineSlice`'s pieces are all empty), so the surface records none.
fn draws_something(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("  sprite ") else {
        return true;
    };
    let fields: Vec<&str> = rest.split(' ').collect();
    fields[4] != "0" && fields[5] != "0"
}

fn run_script() -> Vec<String> {
    let mut demo = Demo::new(
        "client_font",
        include_str!("fixtures/client_font/demo/LabelTooltip.java"),
    );
    let budget = ExecutionBudget::UNBOUNDED;
    let translations: BTreeMap<String, String> = metrics().translations;
    let mut out = Vec::new();
    for kind in 1..=5 {
        let component = demo.component(kind);
        let jar = &demo.fixture.jar;
        demo.host.begin_client_frame(frame(metrics()), &[]);
        let size = jar.tooltip_component_size(&mut demo.host, &component, &budget);
        let size = size
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("kind {kind}: the size counted: {:?}", size.report));
        out.push(format!(
            "label kind={kind} width={} height={}",
            size.width, size.height
        ));
        out.push(format!(
            "  draw at 10,20 box {}x{}",
            size.width, size.height
        ));
        let drawn = jar.draw_tooltip_component_image(
            &mut demo.host,
            &component,
            10,
            20,
            size.width,
            size.height,
            &budget,
        );
        let draws = drawn
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("kind {kind}: extractImage counted: {:?}", drawn.report));
        let frame_out = demo.host.end_client_frame();
        assert_eq!(frame_out.font_questions, Vec::new(), "kind {kind}");
        out.extend(draws.iter().map(|op| print(op, &translations)));
        demo.host.release(&component);
    }
    assert_eq!(
        demo.host.unknown_host_call_count(),
        0,
        "{:?}",
        demo.host.unknown_host_calls()
    );
    out
}

fn on_big_stack<T: Send + 'static>(run: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap()
}

/// The whole script, line for line, against the JDK 25 / client 26.3 oracle.
#[test]
fn font_split_width_and_blit_sprite_match_java25() {
    let expected: Vec<String> = JAVA25
        .lines()
        .filter(|line| draws_something(line))
        .map(str::to_owned)
        .collect();
    assert_eq!(on_big_stack(run_script), expected);
}

/// A fake embedder with no glyph table, as Sunrise is: the game's font is `metrics()` (the
/// oracle's table and translations), asked text by text as Sunrise's `text_width` / `text_split`
/// would be. Answers what it can; `false` for anything it can't.
fn game_font(questions: &[FontQuestion], answers: &mut FontAnswers) -> bool {
    let font = metrics();
    for question in questions {
        match question {
            FontQuestion::Width(text) => {
                let Some(width) = font.exact_width(text, WidthOrder::Logical) else {
                    return false;
                };
                answers.widths.insert(text.clone(), width);
            }
            FontQuestion::Split(text, width) => {
                let Some(lines) = font.exact_split(text, *width) else {
                    return false;
                };
                answers.splits.insert((text.clone(), *width), lines);
            }
        }
    }
    true
}

/// A tooltip frame whose font holds only the embedder's answers (no glyph table).
fn answered_frame(answers: &FontAnswers) -> Arc<ClientSnapshot> {
    frame(TextMetrics {
        answers: answers.clone(),
        ..TextMetrics::default()
    })
}

/// The path Sunrise runs: no glyph table, every text asked of the game's font between runs
/// (`NativeJar::tooltip_image_with_font`), each probe's counts rewound, and the image then drawn
/// in a later frame with the answers, as the factory's `draw_image` does. Every label's size and
/// draws are the oracle's, and nothing stays counted.
#[test]
fn the_sunrise_path_asks_the_game_font_and_matches_java25() {
    let expected: Vec<String> = JAVA25
        .lines()
        .filter(|line| draws_something(line))
        .map(str::to_owned)
        .collect();
    let printed = on_big_stack(|| {
        let mut demo = Demo::new(
            "client_font_sunrise_path",
            include_str!("fixtures/client_font/demo/LabelTooltip.java"),
        );
        let budget = ExecutionBudget::UNBOUNDED;
        let translations = metrics().translations;
        let mut out = Vec::new();
        for kind in 1..=5 {
            let component = demo.component(kind);
            let jar = &demo.fixture.jar;
            let image = jar
                .tooltip_image_with_font(
                    &mut demo.host,
                    &component,
                    &answered_frame,
                    16,
                    &mut game_font,
                    &budget,
                )
                .unwrap_or_else(|| {
                    panic!(
                        "kind {kind}: no image: {:?}",
                        demo.host.unknown_host_calls()
                    )
                });
            // A run stops at its first unanswered text: kind 1 asks the title's width, the
            // description's split, each of its six lines' widths and the count label's, a run
            // each (ten in all).
            assert!(image.runs > 1, "kind {kind} asked nothing");
            if kind == 1 {
                assert_eq!(image.runs, 10);
            }
            let size = image.size;
            out.push(format!(
                "label kind={kind} width={} height={}",
                size.width, size.height
            ));
            out.push(format!(
                "  draw at 10,20 box {}x{}",
                size.width, size.height
            ));
            demo.host
                .begin_client_frame(answered_frame(&image.answers), &[]);
            let drawn = jar.draw_tooltip_component_image(
                &mut demo.host,
                &component,
                10,
                20,
                size.width,
                size.height,
                &budget,
            );
            assert_eq!(demo.host.end_client_frame().font_questions, Vec::new());
            let draws = drawn
                .clone()
                .exact()
                .unwrap_or_else(|| panic!("kind {kind}: {:?}", drawn.report));
            out.extend(draws.iter().map(|op| print(op, &translations)));
            demo.host.release(&component);
        }
        // Every probe's refusals were rewound; the runs that answered counted nothing.
        assert_eq!(
            demo.host.unknown_host_call_count(),
            0,
            "{:?}",
            demo.host.unknown_host_calls()
        );
        out
    });
    assert_eq!(printed, expected);
}

/// A probe that counts anything but the font's refusals keeps its counts, and no image is
/// answered. Here `getWidth` first reaches a class whose `<clinit>` stores a counted call's
/// placeholder in a `static final`, then asks the font: a rewind would drop that count, and the
/// next run (which doesn't run `<clinit>` again) would read the placeholder with nothing counted.
#[test]
fn a_probe_that_counted_more_than_the_font_keeps_its_counts() {
    let source = include_str!("fixtures/client_font/demo/LabelTooltip.java")
        .replace(
            "    return Math.max(WRAP, font.width(TITLE));",
            "    Object kept = Kept.VALUE;\n    return Math.max(WRAP, font.width(TITLE)) + (kept == null ? 1 : 0);",
        )
        .replace(
            "  private final Label label;",
            "  static final class Kept {\n    static final Object VALUE = net.minecraft.util.Unmodelled.make();\n  }\n\n  private final Label label;",
        );
    assert!(source.contains("Kept.VALUE") && source.contains("Unmodelled.make()"));
    on_big_stack(move || {
        let mut demo = Demo::new_with_stubs(
            "client_font_kept_placeholder",
            &source,
            &[(
                "net/minecraft/util/Unmodelled.java",
                "package net.minecraft.util; public final class Unmodelled { public static Object make() { throw new AssertionError(); } }",
            )],
        );
        let budget = ExecutionBudget::UNBOUNDED;
        let component = demo.component(4);
        let jar = &demo.fixture.jar;
        let mut asked = 0;
        let image = jar.tooltip_image_with_font(
            &mut demo.host,
            &component,
            &answered_frame,
            8,
            &mut |questions, answers| {
                asked += 1;
                game_font(questions, answers)
            },
            &budget,
        );
        assert_eq!(image, None);
        // The first run asked the title's width, but what else it counted keeps it from being
        // a probe: nothing was answered, and both counts stand.
        assert_eq!(asked, 0);
        let counted: BTreeMap<String, usize> = demo.host.unknown_host_calls().into_iter().collect();
        assert!(
            counted
                .keys()
                .any(|key| key.starts_with("net/minecraft/util/Unmodelled#make")),
            "{counted:?}"
        );
        assert!(
            counted
                .keys()
                .any(|key| key.starts_with("net/minecraft/client/gui/Font#width")),
            "{counted:?}"
        );
    });
}

/// Every row of `cases25.txt` (26.3's `Font.split` and `Font.width` of single inputs, on the
/// oracle's table): the glyph model's answer is the oracle's.
#[test]
fn split_and_width_cases_match_java25() {
    let unescape = |text: &str| text.replace("\\n", "\n").replace("\\\\", "\\");
    let font = metrics();
    let mut checked = 0;
    for line in CASES25.lines() {
        if let Some(rest) = line.strip_prefix("split \"") {
            let (text, rest) = rest.split_once("\" ").unwrap();
            let (width, rest) = rest.split_once(' ').unwrap();
            let (count, lines) = rest.split_once(' ').unwrap();
            let lines = lines.strip_prefix('[').unwrap().strip_suffix(']').unwrap();
            let expected: Vec<ClientText> = if count == "0" {
                Vec::new()
            } else {
                lines
                    .split('|')
                    .map(|line| ClientText::Literal(unescape(line)))
                    .collect()
            };
            assert_eq!(expected.len(), count.parse::<usize>().unwrap(), "{line}");
            let text = ClientText::Literal(unescape(text));
            assert_eq!(
                font.exact_split(&text, width.parse().unwrap()),
                Some(expected),
                "{line}"
            );
            checked += 1;
        } else if let Some(rest) = line.strip_prefix("width \"") {
            let (text, width) = rest.rsplit_once("\" ").unwrap();
            let text = ClientText::Literal(unescape(text));
            assert_eq!(
                font.exact_width(&text, WidthOrder::Logical),
                Some(width.parse().unwrap()),
                "{line}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 29);
}

/// A glyph past the table and a translation the frame lacks are no answer: counted, with the
/// question recorded. Neither is guessed (no fallback advance, no key for a translation).
#[test]
fn an_unknown_glyph_or_translation_counts() {
    on_big_stack(|| {
        let mut demo = Demo::new(
            "client_font_unknown",
            &include_str!("fixtures/client_font/demo/LabelTooltip.java")
                .replace("\"wrap supercalifragilistic words\"", "\"caf\\u00e9\""),
        );
        let budget = ExecutionBudget::UNBOUNDED;
        let cafe = demo.component(2);
        let described = demo.component(1);
        let jar = &demo.fixture.jar;
        demo.host.begin_client_frame(frame(metrics()), &[]);
        let size = jar.tooltip_component_size(&mut demo.host, &cafe, &budget);
        assert!(!size.report.exact());
        assert!(
            size.report
                .unknown_host_calls
                .keys()
                .any(|key| key.starts_with("net/minecraft/client/gui/Font#split")),
            "{:?}",
            size.report
        );
        assert_eq!(
            demo.host.end_client_frame().font_questions,
            [FontQuestion::Split(
                ClientText::Literal("caf\u{e9}".into()),
                40
            )]
        );
        let mut lacking = metrics();
        lacking.translations.remove("demo.label.description");
        demo.host.begin_client_frame(frame(lacking), &[]);
        let size = jar.tooltip_component_size(&mut demo.host, &described, &budget);
        assert!(!size.report.exact());
        assert_eq!(
            demo.host.end_client_frame().font_questions,
            [FontQuestion::Split(
                ClientText::Translatable("demo.label.description".into()),
                40
            )]
        );
    });
}

/// `blitSprite` on a pipeline other than `GUI_TEXTURED` (here `null`, and another
/// `RenderPipelines` constant), and on `GUI_TEXTURED` under a scaled pose (a nine-slice's borders
/// would scale with it), counts.
#[test]
fn another_pipeline_or_a_scaled_pose_counts() {
    let source = include_str!("fixtures/client_font/demo/LabelTooltip.java");
    for (test, replaced, by) in [
        (
            "client_font_pipeline",
            "graphics.blitSprite(RenderPipelines.GUI_TEXTURED, SLOT,",
            "graphics.blitSprite(null, SLOT,",
        ),
        (
            "client_font_other_pipeline",
            "graphics.blitSprite(RenderPipelines.GUI_TEXTURED, SLOT,",
            "graphics.blitSprite(RenderPipelines.GUI_TEXTURED_PREMULTIPLIED_ALPHA, SLOT,",
        ),
        (
            "client_font_scaled",
            "graphics.blitSprite(RenderPipelines.GUI_TEXTURED, SLOT,",
            "graphics.pose().pushMatrix(); graphics.pose().scale(2f, 2f); graphics.blitSprite(RenderPipelines.GUI_TEXTURED, SLOT,",
        ),
    ] {
        assert!(source.contains(replaced));
        let source = source.replace(replaced, by);
        on_big_stack(move || {
            let mut demo = Demo::new(test, &source);
            let budget = ExecutionBudget::UNBOUNDED;
            let component = demo.component(4);
            let jar = &demo.fixture.jar;
            demo.host.begin_client_frame(frame(metrics()), &[]);
            let size = jar
                .tooltip_component_size(&mut demo.host, &component, &budget)
                .exact()
                .expect("the size is exact");
            let drawn = jar.draw_tooltip_component_image(
                &mut demo.host,
                &component,
                10,
                20,
                size.width,
                size.height,
                &budget,
            );
            demo.host.end_client_frame();
            assert!(!drawn.report.exact(), "{test}");
            // The other constant may already count where it is read; `null` and the scaled pose
            // count on the draw itself.
            if test != "client_font_other_pipeline" {
                assert!(
                    drawn
                        .report
                        .unknown_host_calls
                        .keys()
                        .any(|key| key.contains("GuiGraphicsExtractor#blitSprite")),
                    "{test}: {:?}",
                    drawn.report
                );
            }
        });
    }
}
