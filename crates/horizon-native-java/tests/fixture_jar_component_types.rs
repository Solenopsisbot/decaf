//! A jar's own data component types (docs/java-item-parts.md steps 1 and 2): the NBT their codec
//! encodes, their registration (from an entrypoint, or from a Mixin into a game class's static
//! initializer), and the default components the jar's items give them.
//!
//! Expected NBT is `fixtures/jar_component_types/java25.txt`, captured from Minecraft 26.3's real
//! `NbtOps.INSTANCE` on OpenJDK 25.0.5+8 by `fixtures/jar_component_types/NbtOracle.java` (see the
//! README beside it). The demo classes compile against compile-only stubs, so every DFU and game
//! call reaches the runtime's models as a real mod's does. The order the bootstrap Mixin callbacks
//! run in is `bootstrap_clinit_order_26_3.txt`'s, measured on the 26.3 server with JDK 25
//! (`tools/trace_bootstrap_clinit_order.py`): `DataComponents` finishes its `<clinit>` before
//! `DataComponentPredicates`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{
    DfuCodecResult, ExecutionBudget, JarComponentType, Nbt, StdHost, Value, content::declarations,
};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;

const JAVA25: &str = include_str!("fixtures/jar_component_types/java25.txt");
const POUCH: &str = include_str!("fixtures/jar_component_types/demo/Pouch.java");
const GAUGE: &str = include_str!("fixtures/jar_component_types/demo/Gauge.java");
const CASES: &str = include_str!("fixtures/jar_component_types/demo/Cases.java");

/// DataFixerUpper's codec surface the demo codecs use, as compile-only stubs.
const DFU_STUBS: &[(&str, &str)] = &[
    (
        "com/mojang/datafixers/kinds/App.java",
        "package com.mojang.datafixers.kinds; public interface App<F, A> {}",
    ),
    (
        "com/mojang/datafixers/kinds/Applicative.java",
        "package com.mojang.datafixers.kinds; public interface Applicative<F, Mu> {}",
    ),
    (
        "com/mojang/datafixers/util/Function6.java",
        "package com.mojang.datafixers.util; public interface Function6<T1, T2, T3, T4, T5, T6, R> { R apply(T1 a, T2 b, T3 c, T4 d, T5 e, T6 f); }",
    ),
    (
        "com/mojang/datafixers/Products.java",
        "package com.mojang.datafixers;\n\
         import com.mojang.datafixers.kinds.*; import com.mojang.datafixers.util.Function6; import java.util.function.*;\n\
         public interface Products {\n\
           final class P2<F, T1, T2> {\n\
             public <R> App<F, R> apply(Applicative<F, ?> a, BiFunction<T1, T2, R> f) { throw new AssertionError(); }\n\
           }\n\
           final class P6<F, T1, T2, T3, T4, T5, T6> {\n\
             public <R> App<F, R> apply(Applicative<F, ?> a, Function6<T1, T2, T3, T4, T5, T6, R> f) { throw new AssertionError(); }\n\
           }\n\
         }",
    ),
    (
        "com/mojang/serialization/MapCodec.java",
        "package com.mojang.serialization;\n\
         import com.mojang.serialization.codecs.RecordCodecBuilder; import java.util.function.Function;\n\
         public abstract class MapCodec<A> {\n\
           public <O> RecordCodecBuilder<O, A> forGetter(Function<O, A> getter) { throw new AssertionError(); }\n\
         }",
    ),
    (
        "com/mojang/serialization/Codec.java",
        "package com.mojang.serialization;\n\
         import com.mojang.serialization.codecs.PrimitiveCodec;\n\
         import java.util.List; import java.util.function.Function;\n\
         public interface Codec<A> {\n\
           PrimitiveCodec<Boolean> BOOL = null;\n\
           PrimitiveCodec<Byte> BYTE = null;\n\
           PrimitiveCodec<Short> SHORT = null;\n\
           PrimitiveCodec<Integer> INT = null;\n\
           PrimitiveCodec<Long> LONG = null;\n\
           PrimitiveCodec<Float> FLOAT = null;\n\
           PrimitiveCodec<Double> DOUBLE = null;\n\
           PrimitiveCodec<String> STRING = null;\n\
           MapCodec<A> fieldOf(String name);\n\
           MapCodec<A> optionalFieldOf(String name, A defaultValue);\n\
           Codec<List<A>> listOf();\n\
           Codec<List<A>> listOf(int minSize, int maxSize);\n\
           <S> Codec<S> xmap(Function<? super A, ? extends S> to, Function<? super S, ? extends A> from);\n\
         }",
    ),
    (
        "com/mojang/serialization/codecs/PrimitiveCodec.java",
        "package com.mojang.serialization.codecs;\n\
         import com.mojang.serialization.Codec;\n\
         public interface PrimitiveCodec<A> extends Codec<A> {}",
    ),
    (
        "com/mojang/serialization/codecs/RecordCodecBuilder.java",
        "package com.mojang.serialization.codecs;\n\
         import com.mojang.datafixers.Products; import com.mojang.datafixers.kinds.*;\n\
         import com.mojang.serialization.Codec; import java.util.function.Function;\n\
         public final class RecordCodecBuilder<O, F> implements App<RecordCodecBuilder.Mu<O>, F> {\n\
           public static final class Mu<O> {}\n\
           public static <O> Codec<O> create(Function<Instance<O>, ? extends App<Mu<O>, O>> builder) { throw new AssertionError(); }\n\
           public static final class Instance<O> implements Applicative<Mu<O>, Instance.Mu<O>> {\n\
             public static final class Mu<O> {}\n\
             public <T1, T2> Products.P2<RecordCodecBuilder.Mu<O>, T1, T2> group(App<RecordCodecBuilder.Mu<O>, T1> t1, App<RecordCodecBuilder.Mu<O>, T2> t2) { throw new AssertionError(); }\n\
             public <T1, T2, T3, T4, T5, T6> Products.P6<RecordCodecBuilder.Mu<O>, T1, T2, T3, T4, T5, T6> group(App<RecordCodecBuilder.Mu<O>, T1> t1, App<RecordCodecBuilder.Mu<O>, T2> t2, App<RecordCodecBuilder.Mu<O>, T3> t3, App<RecordCodecBuilder.Mu<O>, T4> t4, App<RecordCodecBuilder.Mu<O>, T5> t5, App<RecordCodecBuilder.Mu<O>, T6> t6) { throw new AssertionError(); }\n\
           }\n\
         }",
    ),
];

/// The game, Fabric and Mixin surface the demo mods use, as compile-only stubs.
const GAME_STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api; public interface ModInitializer { void onInitialize(); }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { return null; } }",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources; public final class ResourceKey<T> { public static <T> ResourceKey<T> create(ResourceKey<? extends net.minecraft.core.Registry<T>> r, Identifier id) { return null; } }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core; import net.minecraft.resources.Identifier; public interface Registry<T> { static <V, T extends V> T register(Registry<V> r, Identifier id, T v) { return v; } }",
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        "package net.minecraft.core; public interface DefaultedRegistry<T> extends Registry<T> {}",
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.DefaultedRegistry; import net.minecraft.core.Registry; import net.minecraft.core.component.DataComponentType; import net.minecraft.world.item.Item; public final class BuiltInRegistries { public static final DefaultedRegistry<Item> ITEM = null; public static final Registry<DataComponentType<?>> DATA_COMPONENT_TYPE = null; public static final Registry<Object> DATA_COMPONENT_PREDICATE_TYPE = null; }",
    ),
    (
        "net/minecraft/core/registries/Registries.java",
        "package net.minecraft.core.registries; import net.minecraft.core.Registry; import net.minecraft.resources.ResourceKey; import net.minecraft.world.item.Item; public final class Registries { public static final ResourceKey<Registry<Item>> ITEM = null; }",
    ),
    (
        "net/minecraft/core/component/DataComponentType.java",
        "package net.minecraft.core.component; import com.mojang.serialization.Codec; public interface DataComponentType<T> { static <T> Builder<T> builder() { return null; } class Builder<T> { public Builder<T> persistent(Codec<T> codec) { return this; } public Builder<T> cacheEncoding() { return this; } public DataComponentType<T> build() { return null; } } }",
    ),
    (
        "net/minecraft/core/component/DataComponents.java",
        "package net.minecraft.core.component; public final class DataComponents {}",
    ),
    (
        "net/minecraft/core/component/predicates/DataComponentPredicates.java",
        "package net.minecraft.core.component.predicates; public final class DataComponentPredicates {}",
    ),
    (
        "net/minecraft/client/Minecraft.java",
        "package net.minecraft.client; public class Minecraft {}",
    ),
    (
        "net/minecraft/world/item/Item.java",
        "package net.minecraft.world.item; import net.minecraft.core.component.DataComponentType; import net.minecraft.resources.ResourceKey; public class Item { public static class Properties { public Properties setId(ResourceKey<Item> k) { return this; } public Properties stacksTo(int n) { return this; } public <T> Properties component(DataComponentType<T> t, T v) { return this; } } public Item(Properties p) {} }",
    ),
    // Mixin's own retentions: `@Mixin` is class-file only, `@Inject` and `@At` runtime.
    (
        "org/spongepowered/asm/mixin/Mixin.java",
        "package org.spongepowered.asm.mixin;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.CLASS) @Target(ElementType.TYPE)\npublic @interface Mixin { Class<?>[] value() default {}; String[] targets() default {}; int priority() default 1000; }\n",
    ),
    (
        "org/spongepowered/asm/mixin/injection/At.java",
        "package org.spongepowered.asm.mixin.injection;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.RUNTIME) @Target({})\npublic @interface At { String value() default \"\"; }\n",
    ),
    (
        "org/spongepowered/asm/mixin/injection/Inject.java",
        "package org.spongepowered.asm.mixin.injection;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.RUNTIME) @Target(ElementType.METHOD)\npublic @interface Inject { String[] method(); At[] at(); boolean cancellable() default false; }\n",
    ),
    (
        "org/spongepowered/asm/mixin/injection/callback/CallbackInfo.java",
        "package org.spongepowered.asm.mixin.injection.callback;\npublic class CallbackInfo { public void cancel() {} }\n",
    ),
];

/// The demo codecs: shared by every test here and by `NbtOracle`.
fn demo_sources() -> Vec<(&'static str, &'static str)> {
    let mut sources = DFU_STUBS.to_vec();
    sources.extend_from_slice(GAME_STUBS);
    sources.push(("demo/Pouch.java", POUCH));
    sources.push(("demo/Gauge.java", GAUGE));
    sources.push(("demo/Cases.java", CASES));
    sources
}

fn java25() -> BTreeMap<&'static str, &'static str> {
    JAVA25
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect()
}

/// An NBT tag as `NbtOracle.show` prints it.
fn show(nbt: &Nbt) -> String {
    match nbt {
        Nbt::Byte(value) => format!("byte({value})"),
        Nbt::Short(value) => format!("short({value})"),
        Nbt::Int(value) => format!("int({value})"),
        Nbt::Long(value) => format!("long({value})"),
        Nbt::Float(bits) => format!("float({bits:08x})"),
        Nbt::Double(bits) => format!("double({bits:016x})"),
        Nbt::String(value) => format!("string({value})"),
        Nbt::IntArray(values) => format!(
            "int_array[{}]",
            values
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Nbt::List(values) => format!(
            "list[{}]",
            values.iter().map(show).collect::<Vec<_>>().join(",")
        ),
        Nbt::Compound(entries) => format!(
            "compound{{{}}}",
            entries
                .iter()
                .map(|(key, value)| format!("{key}:{}", show(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => format!("other({other:?})"),
    }
}

/// The content-file JSON of an oracle row whose tags all have one (compounds, lists, ints and
/// strings): what an item's declaration must carry for that value.
fn row_json(row: &str) -> Json {
    fn parse(text: &str) -> (Json, &str) {
        if let Some(rest) = text.strip_prefix("compound{") {
            let mut object = serde_json::Map::new();
            let mut rest = rest;
            while !rest.starts_with('}') {
                let (key, after) = rest.split_once(':').unwrap();
                let (value, after) = parse(after);
                object.insert(key.to_owned(), value);
                rest = after.strip_prefix(',').unwrap_or(after);
            }
            return (Json::Object(object), &rest[1..]);
        }
        if let Some(rest) = text.strip_prefix("list[") {
            let mut values = Vec::new();
            let mut rest = rest;
            while !rest.starts_with(']') {
                let (value, after) = parse(rest);
                values.push(value);
                rest = after.strip_prefix(',').unwrap_or(after);
            }
            return (Json::Array(values), &rest[1..]);
        }
        let (kind, rest) = text.split_once('(').unwrap();
        let (value, rest) = rest.split_once(')').unwrap();
        let json = match kind {
            "int" => json!(value.parse::<i32>().unwrap()),
            "string" => json!(value),
            other => panic!("no content JSON for {other}"),
        };
        (json, rest)
    }
    let (json, rest) = parse(row);
    assert!(rest.is_empty(), "{row}");
    json
}

fn call(
    fixture: &javac_fixture::Fixture,
    host: &mut StdHost,
    method: &str,
    descriptor: &str,
    n: i32,
) -> Value {
    fixture
        .jar
        .invoke_method_with_args(
            "demo/Cases",
            method,
            descriptor,
            None,
            &[Value::Int(n)],
            host,
        )
        .unwrap_or_else(|error| panic!("Cases.{method}({n}): {error:?}"))
}

/// Each case's NBT is exactly what `codec.encodeStart(NbtOps.INSTANCE, value)` gives on JDK 25:
/// the record's fields as a compound without the optional field at its default, a boolean as a
/// `ByteTag`, each number its own tag, a list of ints a `ListTag` (not an `IntArrayTag`), `-0.0`
/// as `+0.0` (`DoubleTag.valueOf`), a NaN float kept, and a too-long bounded list an error.
#[test]
fn nbt_encode_matches_java25_nbt_ops() {
    let fixture = javac_fixture::Fixture::build("jar_component_nbt", &demo_sources());
    let mut host = fixture.jar.seed_host(StdHost::new());
    let expected = java25();
    // `Cases.COUNT`, one oracle row per case.
    assert_eq!(expected.len(), 9);
    for n in 0..expected.len() as i32 {
        let Value::Ref(Some(name)) = call(&fixture, &mut host, "name", "(I)Ljava/lang/String;", n)
        else {
            panic!("case {n} has no name");
        };
        let codec = call(
            &fixture,
            &mut host,
            "codec",
            "(I)Lcom/mojang/serialization/Codec;",
            n,
        );
        let value = call(&fixture, &mut host, "value", "(I)Ljava/lang/Object;", n);
        let actual = match fixture
            .jar
            .encode_dfu_codec_nbt(&codec, &value, &mut host)
            .unwrap()
        {
            DfuCodecResult::Ok(nbt) => show(&nbt),
            DfuCodecResult::Error { message, .. } => format!("ERROR|{message}"),
            DfuCodecResult::Unmodelled(reason) => panic!("{name}: unmodelled: {reason}"),
        };
        assert_eq!(actual, expected[name.as_str()], "{name}");
    }
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

/// Each registered item's declaration file, by path.
fn item_files(host: &StdHost) -> BTreeMap<String, Json> {
    declarations(host.registrations())
        .iter()
        .filter(|declaration| !declaration.is_block())
        .map(|declaration| (declaration.path().to_owned(), declaration.file_json()))
        .collect()
}

fn unknown(host: &StdHost) -> BTreeMap<String, usize> {
    host.unknown_host_calls().into_iter().collect()
}

/// Types registered from an entrypoint, as Runes' items would be with a type of their own.
const ENTRYPOINT_DEMO: &str = r#"package demo;
import java.util.List;
import net.fabricmc.api.ModInitializer;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;

public final class Probe implements ModInitializer {
  static final DataComponentType<Pouch> POUCH =
      DataComponentType.<Pouch>builder().persistent(Pouch.CODEC).cacheEncoding().build();
  static final DataComponentType<Gauge> GAUGE =
      DataComponentType.<Gauge>builder().persistent(Gauge.CODEC).build();
  static final DataComponentType<Integer> CHARGE = DataComponentType.<Integer>builder().build();
  static final DataComponentType<Integer> LEVEL =
      DataComponentType.<Integer>builder().persistent(com.mojang.serialization.Codec.INT).build();
  static final DataComponentType<Pouch> NEVER_REGISTERED =
      DataComponentType.<Pouch>builder().persistent(Pouch.CODEC).build();

  static Identifier id(String path) { return Identifier.fromNamespaceAndPath("fixture", path); }

  static void item(String path, Item.Properties properties) {
    Registry.register(BuiltInRegistries.ITEM, id(path),
        new Item(properties.setId(ResourceKey.create(Registries.ITEM, id(path)))));
  }

  static Pouch pouch(int multiplier) { return new Pouch(new Pouch.Names(List.of()), multiplier); }

  @Override
  public void onInitialize() {
    Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE, id("pouch"), POUCH);
    Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE, id("gauge"), GAUGE);
    Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE, id("charge"), CHARGE);
    Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE, id("level"), LEVEL);
    item("small", new Item.Properties().stacksTo(1).component(POUCH, pouch(4)));
    item("plain", new Item.Properties().component(POUCH, pouch(1)));
    item("named", new Item.Properties().component(POUCH,
        new Pouch(new Pouch.Names(List.of("a", "b")), 12)));
    item("removed", new Item.Properties().component(POUCH, pouch(8)).component(POUCH, null));
    item("replaced", new Item.Properties().component(POUCH, pouch(8)).component(POUCH, pouch(4)));
    // Counted, each left out of its declaration: NBT with a ShortTag and a FloatTag (no content
    // JSON spells them), a transient type, a value the host has no object for (a boxed int), and
    // a type never registered.
    item("gauged", new Item.Properties().component(GAUGE,
        new Gauge(true, 5000000000L, 0.5, (byte) 7, (short) -3, 1.25f)));
    item("charged", new Item.Properties().component(CHARGE, 3));
    item("levelled", new Item.Properties().component(LEVEL, 5));
    item("stray", new Item.Properties().component(NEVER_REGISTERED, pouch(4)));
  }
}
"#;

/// Step 1 and 2 from an entrypoint: each registered type is listed with whether it is persistent,
/// and each item declares the NBT of its default (as content JSON) for the types it can, counting
/// the rest.
#[test]
fn an_entrypoint_registered_type_is_listed_and_its_item_defaults_are_declared() {
    let mut sources = demo_sources();
    sources.push(("demo/Probe.java", ENTRYPOINT_DEMO));
    let fixture = javac_fixture::Fixture::build("jar_component_entrypoint", &sources);
    let entry = fixture.jar.discover_entrypoint("main").unwrap();
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("SERVER"));
    fixture
        .jar
        .execute(&entry.class, entry.method.as_deref(), "main", &mut host)
        .unwrap();
    fixture.jar.declare_jar_component_defaults(&mut host);
    let kind = |id: &str, persistent| JarComponentType {
        id: id.into(),
        persistent,
    };
    assert_eq!(
        host.jar_component_types(),
        vec![
            kind("fixture:pouch", true),
            kind("fixture:gauge", true),
            kind("fixture:charge", false),
            kind("fixture:level", true),
        ]
    );
    let rows = java25();
    let files = item_files(&host);
    let component = |path: &str| files[path].get("components").cloned();
    assert_eq!(
        component("small"),
        Some(json!({"fixture:pouch": row_json(rows["pouch_multiplier_4"])}))
    );
    assert_eq!(
        component("plain"),
        Some(json!({"fixture:pouch": row_json(rows["pouch_default_multiplier"])}))
    );
    assert_eq!(
        component("named"),
        Some(json!({"fixture:pouch": row_json(rows["pouch_items"])}))
    );
    assert_eq!(component("removed"), None);
    assert_eq!(
        component("replaced"),
        Some(json!({"fixture:pouch": row_json(rows["pouch_multiplier_4"])}))
    );
    assert_eq!(files["small"]["max_stack_size"], 1);
    for path in ["gauged", "charged", "levelled", "stray"] {
        assert_eq!(component(path), None, "{path}");
    }
    assert_eq!(
        unknown(&host),
        BTreeMap::from([(
            "net/minecraft/core/component/DataComponentType#codec".to_owned(),
            4
        )])
    );
    // A second pass declares nothing twice and counts nothing again.
    fixture.jar.declare_jar_component_defaults(&mut host);
    assert_eq!(item_files(&host), files);
    assert_eq!(unknown(&host).values().sum::<usize>(), 4);
}

/// The Bundle API shape: the type is registered by a Mixin at the end of `DataComponents`'s static
/// initializer, the entrypoint only uses it.
const MIXIN_DEMO: &[(&str, &str)] = &[
    (
        "demo/Types.java",
        r#"package demo;
import net.minecraft.core.component.DataComponentType;
public final class Types {
  public static final DataComponentType<Pouch> POUCH =
      DataComponentType.<Pouch>builder().persistent(Pouch.CODEC).cacheEncoding().build();
  public static final Object PREDICATE = new Object();
  public static final Object HEAD = new Object();
  public static final Object CLIENT = new Object();
}
"#,
    ),
    (
        "demo/mixin/TypesMixin.java",
        r#"package demo.mixin;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
@Mixin(DataComponents.class)
public class TypesMixin {
  @Inject(method = "<clinit>", at = @At("TAIL"))
  private static void registerPouch(CallbackInfo ci) {
    Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE,
        Identifier.fromNamespaceAndPath("fixture", "pouch"), demo.Types.POUCH);
  }
}
"#,
    ),
    (
        "demo/mixin/PredicatesMixin.java",
        r#"package demo.mixin;
import net.minecraft.core.Registry;
import net.minecraft.core.component.predicates.DataComponentPredicates;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
@Mixin(DataComponentPredicates.class)
public class PredicatesMixin {
  @Inject(method = "<clinit>", at = @At("TAIL"))
  private static void registerPredicate(CallbackInfo ci) {
    Registry.register(BuiltInRegistries.DATA_COMPONENT_PREDICATE_TYPE,
        Identifier.fromNamespaceAndPath("fixture", "pouch"), demo.Types.PREDICATE);
  }
}
"#,
    ),
    (
        "demo/mixin/HeadMixin.java",
        r#"package demo.mixin;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
@Mixin(DataComponents.class)
public class HeadMixin {
  @Inject(method = "<clinit>", at = @At("HEAD"))
  private static void early(CallbackInfo ci) {
    Registry.register(BuiltInRegistries.DATA_COMPONENT_PREDICATE_TYPE,
        Identifier.fromNamespaceAndPath("fixture", "head"), demo.Types.HEAD);
  }
}
"#,
    ),
    (
        "demo/mixin/ClientMixin.java",
        r#"package demo.mixin;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
@Mixin(DataComponents.class)
public class ClientMixin {
  @Inject(method = "<clinit>", at = @At("TAIL"))
  private static void clientOnly(CallbackInfo ci) {
    Registry.register(BuiltInRegistries.DATA_COMPONENT_PREDICATE_TYPE,
        Identifier.fromNamespaceAndPath("fixture", "client"), demo.Types.CLIENT);
  }
}
"#,
    ),
    (
        "demo/mixin/MinecraftMixin.java",
        r#"package demo.mixin;
import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
@Mixin(Minecraft.class)
public class MinecraftMixin {
  @Inject(method = "<clinit>", at = @At("TAIL"))
  private static void notBootstrapped(CallbackInfo ci) {}
}
"#,
    ),
    (
        "demo/Probe.java",
        r#"package demo;
import java.util.List;
import net.fabricmc.api.ModInitializer;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.Item;
public final class Probe implements ModInitializer {
  @Override
  public void onInitialize() {
    Identifier id = Identifier.fromNamespaceAndPath("fixture", "sack");
    Registry.register(BuiltInRegistries.ITEM, id, new Item(new Item.Properties()
        .setId(ResourceKey.create(Registries.ITEM, id))
        .component(Types.POUCH, new Pouch(new Pouch.Names(List.of()), 4))));
  }
}
"#,
    ),
];

/// `fabric.mod.json` and the Mixin config: `PredicatesMixin` is listed before `TypesMixin`, so
/// Mixin's apply order alone would run it first; the bootstrap finishes `DataComponents` first.
const MOD_JSON: &str = r#"{"schemaVersion":1,"id":"fixture","version":"1.0.0","entrypoints":{"main":["demo.Probe"]},"mixins":["fixture.mixins.json"]}"#;
const MIXINS_JSON: &str = r#"{"required":true,"package":"demo.mixin","mixins":["PredicatesMixin","TypesMixin","HeadMixin","MinecraftMixin"],"client":["ClientMixin"]}"#;

fn mixin_fixture(test: &str) -> javac_fixture::Fixture {
    let mut sources = demo_sources();
    sources.extend_from_slice(MIXIN_DEMO);
    javac_fixture::Fixture::build_with_mod_json_and_extra(
        test,
        &sources,
        MOD_JSON,
        &[("fixture.mixins.json", MIXINS_JSON.as_bytes())],
    )
}

fn rows(host: &StdHost) -> Vec<(String, String)> {
    host.registrations()
        .iter()
        .map(|entry| (entry.registry.clone(), entry.id.clone()))
        .collect()
}

/// On a server, the `TAIL` callbacks into `DataComponents` and `DataComponentPredicates` run before
/// the entrypoint, in the order those classes finish initializing (`DataComponents` first, against
/// the config's order); the `HEAD` callback and the one into a class the bootstrap does not
/// initialize are counted and not run; the client-only mixin does not apply. The entrypoint's item
/// then declares the type's default.
#[test]
fn a_mixin_at_the_end_of_a_game_static_initializer_registers_the_type_before_the_entrypoint() {
    let fixture = mixin_fixture("jar_component_mixin");
    let entry = fixture.jar.discover_entrypoint("main").unwrap();
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("SERVER"));
    let budget = ExecutionBudget::UNBOUNDED;
    assert_eq!(
        fixture
            .jar
            .run_bootstrap_static_injectors(&mut host, &budget),
        Vec::<String>::new()
    );
    fixture
        .jar
        .execute(&entry.class, entry.method.as_deref(), "main", &mut host)
        .unwrap();
    fixture.jar.declare_jar_component_defaults(&mut host);
    let pair = |registry: &str, id: &str| (registry.to_owned(), id.to_owned());
    assert_eq!(
        rows(&host),
        vec![
            pair("minecraft:data_component_type", "fixture:pouch"),
            pair("minecraft:data_component_predicate_type", "fixture:pouch"),
            pair("minecraft:item", "fixture:sack"),
        ]
    );
    assert_eq!(
        host.jar_component_types(),
        vec![JarComponentType {
            id: "fixture:pouch".into(),
            persistent: true
        }]
    );
    assert_eq!(
        item_files(&host)["sack"]["components"],
        json!({"fixture:pouch": row_json(java25()["pouch_multiplier_4"])})
    );
    assert_eq!(
        unknown(&host),
        BTreeMap::from([
            ("net/minecraft/client/Minecraft#<clinit>".to_owned(), 1),
            (
                "net/minecraft/core/component/DataComponents#<clinit>".to_owned(),
                1
            ),
        ])
    );
    // Once per host: a second call runs and counts nothing.
    assert!(
        fixture
            .jar
            .run_bootstrap_static_injectors(&mut host, &budget)
            .is_empty()
    );
    assert_eq!(rows(&host).len(), 3);
    assert_eq!(unknown(&host).values().sum::<usize>(), 2);
}

/// On a client the order of two classes' initializers is not measured: callbacks into more than
/// one class are counted, and none runs, so nothing registers the type.
#[test]
fn on_a_client_callbacks_into_several_game_classes_are_counted_not_run() {
    let fixture = mixin_fixture("jar_component_mixin_client");
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("CLIENT"));
    let budget = ExecutionBudget::UNBOUNDED;
    assert!(
        fixture
            .jar
            .run_bootstrap_static_injectors(&mut host, &budget)
            .is_empty()
    );
    assert!(rows(&host).is_empty(), "{:?}", rows(&host));
    // DataComponents: the HEAD callback, TypesMixin and ClientMixin; DataComponentPredicates:
    // PredicatesMixin; Minecraft: not bootstrapped.
    assert_eq!(
        unknown(&host),
        BTreeMap::from([
            ("net/minecraft/client/Minecraft#<clinit>".to_owned(), 1),
            (
                "net/minecraft/core/component/DataComponents#<clinit>".to_owned(),
                3
            ),
            (
                "net/minecraft/core/component/predicates/DataComponentPredicates#<clinit>"
                    .to_owned(),
                1
            ),
        ])
    );
}

/// A Mixin class `demo.mixin.<name>` injecting at the `TAIL` of `target`'s `<clinit>` (`import`
/// is the target's binary name), whose handler runs `body`.
fn tail_mixin(name: &str, import: &str, body: &str) -> String {
    let simple = import.rsplit('.').next().unwrap();
    format!(
        r#"package demo.mixin;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import {import};
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
@Mixin({simple}.class)
public class {name} {{
  @Inject(method = "<clinit>", at = @At("TAIL"))
  private static void tail(CallbackInfo ci) {{ {body} }}
}}
"#
    )
}

/// `Registry.register(BuiltInRegistries.<registry>, <ns>:<path>, new Object())`.
fn register(registry: &str, id: &str) -> String {
    let (namespace, path) = id.split_once(':').unwrap();
    format!(
        "Registry.register(BuiltInRegistries.{registry}, Identifier.fromNamespaceAndPath(\"{namespace}\", \"{path}\"), new Object());"
    )
}

/// A top-level mod jar `mod_id` whose one Mixin config lists `mixins` (`(class, source)`).
fn mod_jar(test: &str, mod_id: &str, mixins: &[(&str, String)]) -> javac_fixture::Fixture {
    let mut sources = demo_sources();
    let paths: Vec<String> = mixins
        .iter()
        .map(|(name, _)| format!("demo/mixin/{name}.java"))
        .collect();
    for ((_, source), path) in mixins.iter().zip(&paths) {
        sources.push((path.as_str(), source.as_str()));
    }
    let mod_json = format!(
        r#"{{"schemaVersion":1,"id":"{mod_id}","version":"1.0.0","mixins":["{mod_id}.mixins.json"]}}"#
    );
    let names: Vec<String> = mixins
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    let config = format!(
        r#"{{"required":true,"package":"demo.mixin","mixins":[{}]}}"#,
        names.join(",")
    );
    let config_path = format!("{mod_id}.mixins.json");
    javac_fixture::Fixture::build_with_mod_json_and_extra(
        test,
        &sources,
        &mod_json,
        &[(config_path.as_str(), config.as_bytes())],
    )
}

fn server_host(fixture: &javac_fixture::Fixture) -> StdHost {
    fixture
        .jar
        .seed_host(StdHost::new().with_environment_type("SERVER"))
}

/// Review of #609 steps 1-2: the bootstrap pass is one pass over every loaded jar. Two different
/// mods injecting the same game class's `<clinit>` are ordered by Mixin across their configs,
/// which is not modelled: both callbacks are counted, on each jar's own host, and neither runs.
/// Their callbacks into another class (one mod each) still run.
#[test]
fn two_mods_injecting_one_game_initializer_are_counted_not_run() {
    let a = mod_jar(
        "bootstrap_shared_a",
        "alpha",
        &[
            (
                "AlphaTypes",
                tail_mixin(
                    "AlphaTypes",
                    "net.minecraft.core.component.DataComponents",
                    &register("DATA_COMPONENT_PREDICATE_TYPE", "alpha:types"),
                ),
            ),
            (
                "AlphaPredicates",
                tail_mixin(
                    "AlphaPredicates",
                    "net.minecraft.core.component.predicates.DataComponentPredicates",
                    &register("DATA_COMPONENT_PREDICATE_TYPE", "alpha:predicates"),
                ),
            ),
        ],
    );
    let b = mod_jar(
        "bootstrap_shared_b",
        "beta",
        &[(
            "BetaTypes",
            tail_mixin(
                "BetaTypes",
                "net.minecraft.core.component.DataComponents",
                &register("DATA_COMPONENT_PREDICATE_TYPE", "beta:types"),
            ),
        )],
    );
    let (mut host_a, mut host_b) = (server_host(&a), server_host(&b));
    let failures = horizon_native_java::run_bootstrap_injectors(
        &mut [
            horizon_native_java::BootstrapUnit {
                runner: &a.jar,
                host: &mut host_a,
                sources: vec![&a.jar],
            },
            horizon_native_java::BootstrapUnit {
                runner: &b.jar,
                host: &mut host_b,
                sources: vec![&b.jar],
            },
        ],
        &ExecutionBudget::UNBOUNDED,
    );
    assert!(failures.is_empty(), "{failures:?}");
    let pair = |registry: &str, id: &str| (registry.to_owned(), id.to_owned());
    assert_eq!(
        rows(&host_a),
        vec![pair(
            "minecraft:data_component_predicate_type",
            "alpha:predicates"
        )]
    );
    assert!(rows(&host_b).is_empty(), "{:?}", rows(&host_b));
    let shared = "net/minecraft/core/component/DataComponents#<clinit>".to_owned();
    assert_eq!(unknown(&host_a), BTreeMap::from([(shared.clone(), 1)]));
    assert_eq!(unknown(&host_b), BTreeMap::from([(shared, 1)]));
}

/// Callbacks of two mods into different classes run in the order the server's bootstrap finishes
/// those classes, across jars and before any entrypoint: the second jar's `DataComponents`
/// callback (position 601) runs before the first jar's `DataComponentPredicates` one (1499). The
/// latter throws, so the game does not start: the pass reports it, after the former ran. (Run per
/// jar, the first jar's callback would throw first and the second's never run.)
#[test]
fn callbacks_of_two_mods_run_in_the_bootstraps_global_order() {
    let a = mod_jar(
        "bootstrap_order_a",
        "alpha",
        &[(
            "AlphaPredicates",
            tail_mixin(
                "AlphaPredicates",
                "net.minecraft.core.component.predicates.DataComponentPredicates",
                "if (true) throw new IllegalStateException(\"alpha refuses\");",
            ),
        )],
    );
    let b = mod_jar(
        "bootstrap_order_b",
        "beta",
        &[(
            "BetaTypes",
            tail_mixin(
                "BetaTypes",
                "net.minecraft.core.component.DataComponents",
                &register("DATA_COMPONENT_PREDICATE_TYPE", "beta:types"),
            ),
        )],
    );
    let (mut host_a, mut host_b) = (server_host(&a), server_host(&b));
    let failures = horizon_native_java::run_bootstrap_injectors(
        &mut [
            horizon_native_java::BootstrapUnit {
                runner: &a.jar,
                host: &mut host_a,
                sources: vec![&a.jar],
            },
            horizon_native_java::BootstrapUnit {
                runner: &b.jar,
                host: &mut host_b,
                sources: vec![&b.jar],
            },
        ],
        &ExecutionBudget::UNBOUNDED,
    );
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].starts_with(
            "demo/mixin/AlphaPredicates#tail at the end of net/minecraft/core/component/predicates/DataComponentPredicates.<clinit>"
        ) && failures[0].contains("alpha refuses"),
        "{failures:?}"
    );
    assert_eq!(
        rows(&host_b),
        vec![(
            "minecraft:data_component_predicate_type".to_owned(),
            "beta:types".to_owned()
        )]
    );
    assert!(unknown(&host_a).is_empty() && unknown(&host_b).is_empty());
    // Each host had its pass: a later pass over either runs nothing.
    assert!(
        horizon_native_java::run_bootstrap_injectors(
            &mut [horizon_native_java::BootstrapUnit {
                runner: &b.jar,
                host: &mut host_b,
                sources: vec![&b.jar],
            }],
            &ExecutionBudget::UNBOUNDED,
        )
        .is_empty()
    );
    assert_eq!(rows(&host_b).len(), 1);
}

/// One mod in two jars (a library both bundle: the same mod id) is one mod to Mixin, not two:
/// its callback is not a second mod's into the same class, and it runs on each jar's host.
#[test]
fn one_mod_in_two_jars_runs_on_each_host() {
    let mixin = || {
        tail_mixin(
            "LibraryTypes",
            "net.minecraft.core.component.DataComponents",
            &register("DATA_COMPONENT_PREDICATE_TYPE", "library:types"),
        )
    };
    let a = mod_jar("bootstrap_same_a", "library", &[("LibraryTypes", mixin())]);
    let b = mod_jar("bootstrap_same_b", "library", &[("LibraryTypes", mixin())]);
    let (mut host_a, mut host_b) = (server_host(&a), server_host(&b));
    let failures = horizon_native_java::run_bootstrap_injectors(
        &mut [
            horizon_native_java::BootstrapUnit {
                runner: &a.jar,
                host: &mut host_a,
                sources: vec![&a.jar],
            },
            horizon_native_java::BootstrapUnit {
                runner: &b.jar,
                host: &mut host_b,
                sources: vec![&b.jar],
            },
        ],
        &ExecutionBudget::UNBOUNDED,
    );
    assert!(failures.is_empty(), "{failures:?}");
    for host in [&host_a, &host_b] {
        assert_eq!(rows(host).len(), 1);
        assert!(unknown(host).is_empty());
    }
}

/// The probe's `--link`ed jar is a loaded mod on the same host: its callbacks join the pass, so a
/// second mod's callback into the same class is counted there too.
#[test]
fn a_linked_jars_callbacks_join_its_hosts_pass() {
    let a = mod_jar(
        "bootstrap_linked_a",
        "alpha",
        &[(
            "AlphaTypes",
            tail_mixin(
                "AlphaTypes",
                "net.minecraft.core.component.DataComponents",
                &register("DATA_COMPONENT_PREDICATE_TYPE", "alpha:types"),
            ),
        )],
    );
    let b = mod_jar(
        "bootstrap_linked_b",
        "beta",
        &[(
            "BetaPredicates",
            tail_mixin(
                "BetaPredicates",
                "net.minecraft.core.component.predicates.DataComponentPredicates",
                &register("DATA_COMPONENT_PREDICATE_TYPE", "beta:predicates"),
            ),
        )],
    );
    let mut host = server_host(&a);
    // As `main.rs` links each `--link` jar before the pass.
    host.link_jar("beta.jar", &b.jar).unwrap();
    let failures = horizon_native_java::run_bootstrap_injectors(
        &mut [horizon_native_java::BootstrapUnit {
            runner: &a.jar,
            host: &mut host,
            sources: vec![&b.jar, &a.jar],
        }],
        &ExecutionBudget::UNBOUNDED,
    );
    assert!(failures.is_empty(), "{failures:?}");
    // The global order: `DataComponents` (alpha) first, though beta is the first source.
    let ids: Vec<String> = rows(&host).into_iter().map(|(_, id)| id).collect();
    assert_eq!(ids, ["alpha:types", "beta:predicates"]);
}
