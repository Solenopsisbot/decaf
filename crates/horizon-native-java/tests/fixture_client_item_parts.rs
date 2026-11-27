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
    ExecutionBudget, ItemHook, ItemModelHolder, StdHost, Value,
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
        (
            "demo/mixin/CreateMixin.java",
            include_str!("fixtures/client_item_parts/demo/mixin/CreateMixin.java"),
        ),
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
