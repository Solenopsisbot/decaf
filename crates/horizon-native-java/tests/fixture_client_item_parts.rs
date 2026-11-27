//! A jar's own client item code on the client runtime (`client_item_parts.rs`), against Java 25:
//! a numeric item model property a Mixin adds at the end of
//! `RangeSelectItemModelProperties.bootstrap()`, an item's `getTooltipImage`, the cancellable
//! `HEAD` Mixin into `ClientTooltipComponent.create` that maps the image's data to a client
//! component, and that component's `getWidth`, `getHeight` and `extractImage`
//! (`docs/java-item-parts.md`, steps 7 and 8).
//!
//! `fixtures/client_item_parts/demo/` holds the demo; `java25.txt` is
//! `oracle/ClientItemPartsOracle.java` run on Java 25.0.5+8 against the Minecraft 26.3 client jar
//! (see the fixture's README). This test compiles the same demo against stubs, runs the client
//! bootstrap pass (`bootstrap_mixins.rs`, which runs the property Mixin at its measured client
//! turn) and the entrypoint on a client host, then drives the same script through `NativeJar`'s
//! client item calls and prints the same lines.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use std::sync::Arc;

use horizon_native_java::{
    ExecutionBudget, ItemHook, ItemModelHolder, ItemStackRequest, StdHost, Value,
    client_surface::{ClientSnapshot, DrawOp},
};

const JAVA25: &str = include_str!("fixtures/client_item_parts/java25.txt");

const MOD_JSON: &str = r#"{"schemaVersion":1,"id":"demo","version":"1.0.0","environment":"*","entrypoints":{"main":["demo.Probe"]},"mixins":["demo.mixins.json"]}"#;
const MIXINS_JSON: &str = r#"{"required":true,"package":"demo.mixin","mixins":[],"client":["PropertiesMixin","CreateMixin"]}"#;

/// The nodes and `(count, level, owner, seed)` cases the oracle runs.
const NODES: &[&str] = &[
    r#"{"type":"minecraft:range_dispatch","property":"demo:fill","entries":[]}"#,
    r#"{"type":"minecraft:range_dispatch","property":"demo:fill","divisor":16,"entries":[]}"#,
];
/// A node the property's codec refuses.
const REFUSED: &str =
    r#"{"type":"minecraft:range_dispatch","property":"demo:fill","divisor":"many","entries":[]}"#;
const CASES: &[(i32, bool, bool, i32)] = &[
    (1, false, false, 0),
    (16, false, false, 0),
    (64, false, false, 0),
    (48, true, false, 0),
    (48, false, true, 0),
    (8, true, true, 7),
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
        "org/spongepowered/asm/mixin/Shadow.java",
        "package org.spongepowered.asm.mixin;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.RUNTIME) @Target({ElementType.FIELD, ElementType.METHOD})\npublic @interface Shadow {}",
    ),
    (
        "org/spongepowered/asm/mixin/Final.java",
        "package org.spongepowered.asm.mixin;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.RUNTIME) @Target({ElementType.FIELD, ElementType.METHOD})\npublic @interface Final {}",
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
        "com/mojang/datafixers/kinds/App.java",
        "package com.mojang.datafixers.kinds; public interface App<F, A> {}",
    ),
    (
        "com/mojang/datafixers/kinds/Applicative.java",
        "package com.mojang.datafixers.kinds; public interface Applicative<F, Mu> {}",
    ),
    (
        "com/mojang/datafixers/Products.java",
        "package com.mojang.datafixers;\nimport com.mojang.datafixers.kinds.*; import java.util.function.*;\npublic interface Products {\n  final class P1<F, T1> {\n    public <R> App<F, R> apply(Applicative<F, ?> a, Function<T1, R> f) { throw new AssertionError(); }\n  }\n}",
    ),
    (
        "com/mojang/serialization/MapCodec.java",
        "package com.mojang.serialization;\nimport com.mojang.serialization.codecs.RecordCodecBuilder; import java.util.function.Function;\npublic abstract class MapCodec<A> {\n  public <O> RecordCodecBuilder<O, A> forGetter(Function<O, A> getter) { throw new AssertionError(); }\n}",
    ),
    (
        "com/mojang/serialization/Codec.java",
        "package com.mojang.serialization;\nimport com.mojang.serialization.codecs.PrimitiveCodec;\npublic interface Codec<A> {\n  PrimitiveCodec<Integer> INT = null;\n  MapCodec<A> optionalFieldOf(String name, A defaultValue);\n}",
    ),
    (
        "com/mojang/serialization/codecs/PrimitiveCodec.java",
        "package com.mojang.serialization.codecs; import com.mojang.serialization.Codec; public interface PrimitiveCodec<A> extends Codec<A> {}",
    ),
    (
        "com/mojang/serialization/codecs/RecordCodecBuilder.java",
        "package com.mojang.serialization.codecs;\nimport com.mojang.datafixers.Products; import com.mojang.datafixers.kinds.*;\nimport com.mojang.serialization.MapCodec; import java.util.function.Function;\npublic final class RecordCodecBuilder<O, F> implements App<RecordCodecBuilder.Mu<O>, F> {\n  public static final class Mu<O> {}\n  public static <O> MapCodec<O> mapCodec(Function<Instance<O>, ? extends App<Mu<O>, O>> builder) { throw new AssertionError(); }\n  public static final class Instance<O> implements Applicative<Mu<O>, Instance.Mu<O>> {\n    public static final class Mu<O> {}\n    public <T1> Products.P1<RecordCodecBuilder.Mu<O>, T1> group(App<RecordCodecBuilder.Mu<O>, T1> t1) { throw new AssertionError(); }\n  }\n}",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { throw new AssertionError(); } }",
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
        "net/minecraft/util/ExtraCodecs.java",
        "package net.minecraft.util; public class ExtraCodecs { public static class LateBoundIdMapper<I, V> { public LateBoundIdMapper<I, V> put(I id, V value) { throw new AssertionError(); } } }",
    ),
    (
        "net/minecraft/client/multiplayer/ClientLevel.java",
        "package net.minecraft.client.multiplayer; public class ClientLevel {}",
    ),
    (
        "net/minecraft/world/entity/ItemOwner.java",
        "package net.minecraft.world.entity; public interface ItemOwner {}",
    ),
    (
        "net/minecraft/client/renderer/item/properties/numeric/RangeSelectItemModelProperty.java",
        "package net.minecraft.client.renderer.item.properties.numeric;\nimport com.mojang.serialization.MapCodec; import net.minecraft.client.multiplayer.ClientLevel;\nimport net.minecraft.world.entity.ItemOwner; import net.minecraft.world.item.ItemStack;\npublic interface RangeSelectItemModelProperty {\n  float get(ItemStack stack, ClientLevel level, ItemOwner owner, int seed);\n  MapCodec<? extends RangeSelectItemModelProperty> type();\n}",
    ),
    (
        "net/minecraft/client/renderer/item/properties/numeric/RangeSelectItemModelProperties.java",
        "package net.minecraft.client.renderer.item.properties.numeric; public class RangeSelectItemModelProperties { public static void bootstrap() {} }",
    ),
    (
        "net/minecraft/world/inventory/tooltip/TooltipComponent.java",
        "package net.minecraft.world.inventory.tooltip; public interface TooltipComponent {}",
    ),
    (
        "net/minecraft/client/gui/Font.java",
        "package net.minecraft.client.gui; public class Font {}",
    ),
    (
        "net/minecraft/client/gui/GuiGraphicsExtractor.java",
        "package net.minecraft.client.gui; public class GuiGraphicsExtractor { public void fill(int x0, int y0, int x1, int y1, int color) { throw new AssertionError(); } }",
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

fn fixture(test: &str, mixins_json: &str) -> javac_fixture::Fixture {
    fixture_with(
        test,
        mixins_json,
        include_str!("fixtures/client_item_parts/demo/mixin/CreateMixin.java"),
    )
}

fn fixture_with(test: &str, mixins_json: &str, create_mixin: &str) -> javac_fixture::Fixture {
    let mut sources = STUBS.to_vec();
    sources.extend_from_slice(&[
        (
            "demo/FillProperty.java",
            include_str!("fixtures/client_item_parts/demo/FillProperty.java"),
        ),
        (
            "demo/Meter.java",
            include_str!("fixtures/client_item_parts/demo/Meter.java"),
        ),
        (
            "demo/MeterTooltip.java",
            include_str!("fixtures/client_item_parts/demo/MeterTooltip.java"),
        ),
        (
            "demo/MeterItem.java",
            include_str!("fixtures/client_item_parts/demo/MeterItem.java"),
        ),
        (
            "demo/Probe.java",
            include_str!("fixtures/client_item_parts/demo/Probe.java"),
        ),
        (
            "demo/mixin/PropertiesMixin.java",
            include_str!("fixtures/client_item_parts/demo/mixin/PropertiesMixin.java"),
        ),
        ("demo/mixin/CreateMixin.java", create_mixin),
    ]);
    javac_fixture::Fixture::build_with_mod_json_and_extra(
        test,
        &sources,
        MOD_JSON,
        &[("demo.mixins.json", mixins_json.as_bytes())],
    )
}

/// `Float.toString` for the values the script prints.
fn java_float(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// A client host that has had the bootstrap pass and run the entrypoint, as the client loader
/// runs them (`mod/src/client.rs`).
fn client_host(fixture: &javac_fixture::Fixture) -> StdHost {
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
    host
}

fn item(host: &StdHost, id: &str) -> Value {
    host.registrations()
        .iter()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == id)
        .map(|entry| entry.value.clone())
        .unwrap_or_else(|| panic!("{id} is registered"))
}

fn frame() -> Arc<ClientSnapshot> {
    Arc::new(ClientSnapshot {
        gui_size: [320, 240],
        ..ClientSnapshot::default()
    })
}

fn run_script() -> Vec<String> {
    let fixture = fixture("client_item_parts", MIXINS_JSON);
    let jar = &fixture.jar;
    let mut host = client_host(&fixture);
    let budget = ExecutionBudget::UNBOUNDED;
    let mut out = Vec::new();
    let properties = host.client_range_select_properties();
    assert_eq!(
        properties
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["demo:fill"]
    );
    out.push("registered demo:fill".to_owned());
    let codec = properties[0].1.clone();
    for node in NODES {
        let json: serde_json::Value = serde_json::from_str(node).unwrap();
        let decoded = jar.decode_range_select_property(&mut host, &codec, &json, &budget);
        let property = decoded
            .clone()
            .exact()
            .and_then(Result::ok)
            .unwrap_or_else(|| panic!("{node} decodes: {:?}", decoded.report));
        out.push(format!("decode {node} ok"));
        for &(count, level, owner, seed) in CASES {
            let stack = jar
                .make_item_stack(
                    &mut host,
                    None,
                    "minecraft:diamond",
                    count,
                    Vec::new(),
                    Vec::new(),
                    false,
                    Vec::new(),
                )
                .unwrap();
            let outcome = jar.range_select_property_value(
                &mut host,
                &property,
                &stack,
                ItemModelHolder { level, owner },
                seed,
                &budget,
            );
            let value = outcome
                .clone()
                .exact()
                .unwrap_or_else(|| panic!("get counted: {:?}", outcome.report));
            out.push(format!(
                "  get count={count} level={level} owner={owner} seed={seed} -> {}",
                java_float(value)
            ));
        }
    }
    // A node the codec refuses (`divisor` is not an int): the game rejects the item definition.
    let json: serde_json::Value = serde_json::from_str(REFUSED).unwrap();
    let refused = jar.decode_range_select_property(&mut host, &codec, &json, &budget);
    assert!(refused.report.exact(), "{:?}", refused.report);
    assert!(matches!(refused.value, Some(Err(_))), "{:?}", refused.value);
    out.push(format!("decode {REFUSED} rejected"));
    let meter = item(&host, "demo:meter");
    let overrides = jar.item_overrides(&meter, &host);
    let owner = overrides.owners[&ItemHook::TooltipImage].clone();
    for count in [1, 3, 6] {
        let stack = jar
            .make_item_stack(
                &mut host,
                Some(meter.clone()),
                "demo:meter",
                count,
                Vec::new(),
                Vec::new(),
                false,
                Vec::new(),
            )
            .unwrap();
        let image = jar.item_tooltip_image(&mut host, &meter, &owner, &stack, &budget);
        let Some(data) = image
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("getTooltipImage counted: {:?}", image.report))
        else {
            out.push(format!("tooltip_image count={count} -> empty"));
            continue;
        };
        out.push(format!("tooltip_image count={count} -> present"));
        let created = jar.client_tooltip_component(&mut host, &data, &budget);
        let component = created
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("create counted: {:?}", created.report));
        host.begin_client_frame(frame(), &[]);
        let size = jar
            .tooltip_component_size(&mut host, &component, &budget)
            .exact()
            .expect("size exact");
        out.push(format!(
            "  component width={} height={}",
            size.width, size.height
        ));
        out.push("  draw at 10,20 box 80x50".to_owned());
        let drawn =
            jar.draw_tooltip_component_image(&mut host, &component, 10, 20, 80, 50, &budget);
        let draws = drawn
            .clone()
            .exact()
            .unwrap_or_else(|| panic!("extractImage counted: {:?}", drawn.report));
        host.end_client_frame();
        for draw in draws {
            match draw {
                // `fill(x0, y0, x1, y1, colour)`: recorded as `[x, y, w, h]`.
                DrawOp::Fill {
                    rect: [x, y, w, h],
                    argb,
                } => out.push(format!(
                    "  fill {} {} {} {} {}",
                    x as i32,
                    y as i32,
                    (x + w) as i32,
                    (y + h) as i32,
                    argb as i32
                )),
                other => panic!("unexpected draw {other:?}"),
            }
        }
        // The embedder keeps what it holds only while it uses it.
        host.release(&component);
        host.release(&data);
    }
    host.release(&codec);
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
    out
}

/// The whole script, line for line, against the JDK 25 / client 26.3 oracle.
#[test]
fn client_item_parts_match_java25() {
    let expected: Vec<&str> = JAVA25.lines().collect();
    let actual = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(run_script)
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(actual, expected);
}

/// Without the `HEAD` callback into `ClientTooltipComponent.create`, vanilla's body runs, which
/// refuses a jar's data (the oracle checks it throws `IllegalArgumentException`): counted, no
/// component. Without the bootstrap `TAIL`, no property is registered.
#[test]
fn without_the_mixins_create_counts_and_no_property_registers() {
    let fixture = fixture(
        "client_item_parts_no_mixins",
        r#"{"required":true,"package":"demo.mixin","mixins":[],"client":[]}"#,
    );
    let jar = &fixture.jar;
    let mut host = client_host(&fixture);
    let budget = ExecutionBudget::UNBOUNDED;
    assert!(host.client_range_select_properties().is_empty());
    let meter = item(&host, "demo:meter");
    let owner = jar.item_overrides(&meter, &host).owners[&ItemHook::TooltipImage].clone();
    let stack = jar
        .make_item_stack(
            &mut host,
            Some(meter.clone()),
            "demo:meter",
            3,
            Vec::new(),
            Vec::new(),
            false,
            Vec::new(),
        )
        .unwrap();
    let data = jar
        .item_tooltip_image(&mut host, &meter, &owner, &stack, &budget)
        .exact()
        .flatten()
        .expect("a meter of three");
    let created = jar.client_tooltip_component(&mut host, &data, &budget);
    assert!(created.value.is_none());
    assert_eq!(
        created.report.unknown_host_calls,
        std::collections::BTreeMap::from([(
            "net/minecraft/client/gui/screens/inventory/tooltip/ClientTooltipComponent#create(Lnet/minecraft/world/inventory/tooltip/TooltipComponent;)Lnet/minecraft/client/gui/screens/inventory/tooltip/ClientTooltipComponent;"
                .to_owned(),
            1
        )])
    );
}

/// The property's bootstrap callback is a client mixin: a server host neither runs it nor has the
/// mapper.
#[test]
fn a_server_host_registers_no_item_model_property() {
    let fixture = fixture(
        "client_item_parts_server",
        r#"{"required":true,"package":"demo.mixin","mixins":["PropertiesMixin"],"client":[]}"#,
    );
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("SERVER"));
    assert!(
        fixture
            .jar
            .run_bootstrap_static_injectors(&mut host, &ExecutionBudget::UNBOUNDED)
            .is_empty()
    );
    assert!(host.client_range_select_properties().is_empty());
}

/// `ClientTooltipComponent.create` has two overloads in 26.3 (`create(FormattedCharSequence)`
/// first): a callback that selects `create` by name alone is selected for both, which the one
/// call this runtime makes is not. It counts, and no component is made.
#[test]
fn a_name_only_create_selector_counts() {
    let source = include_str!("fixtures/client_item_parts/demo/mixin/CreateMixin.java").replace(
        "\"create(Lnet/minecraft/world/inventory/tooltip/TooltipComponent;)Lnet/minecraft/client/gui/screens/inventory/tooltip/ClientTooltipComponent;\"",
        "\"create\"",
    );
    assert!(
        !source.contains("create(L"),
        "the selector names create alone"
    );
    let fixture = fixture_with("client_item_parts_name_only_create", MIXINS_JSON, &source);
    let jar = &fixture.jar;
    let mut host = client_host(&fixture);
    let budget = ExecutionBudget::UNBOUNDED;
    let meter = item(&host, "demo:meter");
    let owner = jar.item_overrides(&meter, &host).owners[&ItemHook::TooltipImage].clone();
    let stack = jar
        .make_item_stack(
            &mut host,
            Some(meter.clone()),
            "demo:meter",
            3,
            Vec::new(),
            Vec::new(),
            false,
            Vec::new(),
        )
        .unwrap();
    let data = jar
        .item_tooltip_image(&mut host, &meter, &owner, &stack, &budget)
        .exact()
        .flatten()
        .expect("a meter of three");
    let created = jar.client_tooltip_component(&mut host, &data, &budget);
    assert!(created.value.is_none());
    assert!(
        created
            .report
            .unknown_host_calls
            .keys()
            .any(|key| key.contains("ClientTooltipComponent#create <create callback demo/mixin/CreateMixin#demo$create has a shape not run here>")),
        "{:?}",
        created.report
    );
}

/// Many live objects on the resident heap, as a real mod's heap holds: a static list of linked
/// cells (#609 round 4 bug 1).
const BALLAST: &str = "package demo;\n\
     import java.util.ArrayList; import java.util.List; import net.fabricmc.api.ModInitializer;\n\
     /** Holds many live objects. */\n\
     public final class Ballast implements ModInitializer {\n\
       public record Cell(int n, Cell next) {}\n\
       public static final List<Cell> HELD = new ArrayList<>();\n\
       @Override public void onInitialize() {\n\
         Cell last = null;\n\
         for (int i = 0; i < 12000; i++) { last = new Cell(i, last); HELD.add(last); }\n\
       }\n\
     }\n";

/// The client bridge's draw path (`mod/src/client_item_parts.rs`), over a heap of 12,000 live
/// objects: 10,000 evaluations of `demo:fill` on one stack, then 1,000 draws of a tooltip
/// component, each in a transient scope as the bridge runs them. Every answer is the oracle's,
/// nothing counts, and the heap and the rows stay bounded (each draw's stack is built fresh and
/// collected). Wall time is printed, not asserted: in game, the old collector froze the client
/// for minutes on draws like these.
#[test]
fn ten_thousand_property_draws_over_a_large_heap_stay_exact_and_bounded() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let mut sources = STUBS.to_vec();
            for (path, source) in [
                ("demo/FillProperty.java", include_str!("fixtures/client_item_parts/demo/FillProperty.java")),
                ("demo/Meter.java", include_str!("fixtures/client_item_parts/demo/Meter.java")),
                ("demo/MeterTooltip.java", include_str!("fixtures/client_item_parts/demo/MeterTooltip.java")),
                ("demo/MeterItem.java", include_str!("fixtures/client_item_parts/demo/MeterItem.java")),
                ("demo/Probe.java", include_str!("fixtures/client_item_parts/demo/Probe.java")),
                ("demo/mixin/PropertiesMixin.java", include_str!("fixtures/client_item_parts/demo/mixin/PropertiesMixin.java")),
                ("demo/mixin/CreateMixin.java", include_str!("fixtures/client_item_parts/demo/mixin/CreateMixin.java")),
                ("demo/Ballast.java", BALLAST),
            ] {
                sources.push((path, source));
            }
            let fixture = javac_fixture::Fixture::build_with_mod_json_and_extra(
                "client_item_parts_ballast",
                &sources,
                MOD_JSON,
                &[("demo.mixins.json", MIXINS_JSON.as_bytes())],
            );
            let jar = &fixture.jar;
            let mut host = client_host(&fixture);
            jar.execute("demo.Ballast", None, "main", &mut host)
                .expect("ballast onInitialize");
            let fields = host.resident_heap().instance_fields();
            assert!(fields >= 24_000, "the ballast is live: {fields} fields");
            let budget = ExecutionBudget::UNBOUNDED;
            let codec = host.client_range_select_properties()[0].1.clone();
            let json: serde_json::Value = serde_json::from_str(NODES[0]).unwrap();
            let property = jar
                .decode_range_select_property(&mut host, &codec, &json, &budget)
                .exact()
                .and_then(Result::ok)
                .expect("the default node decodes");
            // The oracle's answer for this stack, under the first node.
            let expected = JAVA25
                .lines()
                .find(|line| line.starts_with("  get count=16 level=false owner=false seed=0 -> "))
                .expect("the oracle line")
                .rsplit_once(" -> ")
                .unwrap()
                .1;
            let request = ItemStackRequest {
                item_id: "minecraft:diamond".to_owned(),
                count: 16,
                ..ItemStackRequest::default()
            };
            let counted = host.unknown_host_call_count();
            let start = std::time::Instant::now();
            for draw in 0..10_000 {
                let value = host.with_transient_rows(|host| {
                    let stack = jar.build_item_stack(host, request.clone()).unwrap();
                    jar.range_select_property_value(
                        host,
                        &property,
                        &stack,
                        ItemModelHolder { level: false, owner: false },
                        0,
                        &budget,
                    )
                    .exact()
                    .unwrap_or_else(|| panic!("draw {draw} counted"))
                });
                assert_eq!(java_float(value), expected, "draw {draw}");
            }
            let properties = start.elapsed();
            // The meter of three's component, drawn as a shown tooltip draws it every frame.
            let meter = item(&host, "demo:meter");
            let owner = jar.item_overrides(&meter, &host).owners[&ItemHook::TooltipImage].clone();
            let stack = jar
                .build_item_stack(
                    &mut host,
                    ItemStackRequest {
                        item_override: Some(meter.clone()),
                        item_id: "demo:meter".to_owned(),
                        count: 3,
                        ..ItemStackRequest::default()
                    },
                )
                .unwrap();
            let data = jar
                .item_tooltip_image(&mut host, &meter, &owner, &stack, &budget)
                .exact()
                .flatten()
                .expect("a meter of three");
            let component = jar
                .client_tooltip_component(&mut host, &data, &budget)
                .exact()
                .expect("create");
            host.release(&data);
            let lines = JAVA25.lines().collect::<Vec<_>>();
            let at = lines
                .iter()
                .position(|line| *line == "tooltip_image count=3 -> present")
                .unwrap();
            let fills: Vec<&str> = lines[at + 3..]
                .iter()
                .take_while(|line| line.starts_with("  fill "))
                .copied()
                .collect();
            let start = std::time::Instant::now();
            for draw in 0..1_000 {
                let ops = host.with_transient_rows(|host| {
                    host.begin_client_frame(frame(), &[]);
                    let ops = jar
                        .draw_tooltip_component_image(host, &component, 10, 20, 80, 50, &budget)
                        .exact()
                        .unwrap_or_else(|| panic!("draw {draw} counted"));
                    host.end_client_frame();
                    ops
                });
                let drawn: Vec<String> = ops
                    .into_iter()
                    .map(|op| match op {
                        DrawOp::Fill { rect: [x, y, w, h], argb } => format!(
                            "  fill {} {} {} {} {}",
                            x as i32,
                            y as i32,
                            (x + w) as i32,
                            (y + h) as i32,
                            argb as i32
                        ),
                        other => panic!("unexpected draw {other:?}"),
                    })
                    .collect();
                assert_eq!(drawn, fills, "draw {draw}");
            }
            let tooltips = start.elapsed();
            host.release(&component);
            eprintln!(
                "10000 property draws over {fields} live fields: {properties:?}; 1000 tooltip draws: {tooltips:?}; rows {:?}; fields now {}",
                        host.recipe_row_counts(),
                host.resident_heap().instance_fields()
            );
            assert_eq!(host.unknown_host_call_count(), counted, "{:?}", host.unknown_host_calls());
            assert!(host.resident_heap().instance_fields() <= 2 * fields + 2048);
            // Every draw's stack is collectable (the meter's, made outside a scope, is a root),
            // and what is left is garbage under the collection trigger: about twice the live
            // set, which this large heap's draws don't reach (`FillProperty` makes no garbage
            // of its own), so the bound is the trigger's.
            let (rows, collectable) = host.recipe_row_counts();
            assert_eq!(collectable + 1, rows);
            assert!(rows <= 2 * fields + 2048, "{rows} rows");

        })
        .unwrap()
        .join()
        .unwrap();
}

/// A client call's answer lives through the store that ends its own invocation: `getTooltipImage`'s
/// data until `create` takes it, `create`'s component until it is measured. The embedder retains
/// each only once the call returned, so a collection landing on that store used to free it, and
/// the next call read a "collected while still reachable" object (#609 round 4, once the draws'
/// garbage made collections frequent). A thousand makes on a small heap, a fresh stack each in a
/// transient scope, put collections on every kind of store.
#[test]
fn a_client_call_answer_survives_its_own_store() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let fixture = fixture("client_item_parts_answer_roots", MIXINS_JSON);
            let jar = &fixture.jar;
            let mut host = client_host(&fixture);
            let budget = ExecutionBudget::UNBOUNDED;
            let meter = item(&host, "demo:meter");
            let owner = jar.item_overrides(&meter, &host).owners[&ItemHook::TooltipImage].clone();
            let request = ItemStackRequest {
                item_override: Some(meter.clone()),
                item_id: "demo:meter".to_owned(),
                count: 3,
                ..ItemStackRequest::default()
            };
            // The oracle's size line for the meter of three.
            let lines: Vec<&str> = JAVA25.lines().collect();
            let at = lines
                .iter()
                .position(|line| *line == "tooltip_image count=3 -> present")
                .unwrap();
            let expected = lines[at + 1];
            let counted = host.unknown_host_call_count();
            for make in 0..1_000 {
                let size = host.with_transient_rows(|host| {
                    let stack = jar.build_item_stack(host, request.clone()).unwrap();
                    let image = jar.item_tooltip_image(host, &meter, &owner, &stack, &budget);
                    let data = image
                        .clone()
                        .exact()
                        .flatten()
                        .unwrap_or_else(|| panic!("make {make}: {:?}", image.report));
                    let created = jar.client_tooltip_component(host, &data, &budget);
                    host.release(&data);
                    let component = created
                        .clone()
                        .exact()
                        .unwrap_or_else(|| panic!("make {make}: {:?}", created.report));
                    host.begin_client_frame(frame(), &[]);
                    let size = jar.tooltip_component_size(host, &component, &budget);
                    host.end_client_frame();
                    host.release(&component);
                    size.clone()
                        .exact()
                        .unwrap_or_else(|| panic!("make {make}: {:?}", size.report))
                });
                assert_eq!(
                    format!("  component width={} height={}", size.width, size.height),
                    expected,
                    "make {make}"
                );
            }
            assert_eq!(
                host.unknown_host_call_count(),
                counted,
                "{:?}",
                host.unknown_host_calls()
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
