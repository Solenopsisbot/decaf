//! `LootItemConditionalFunction.commonFields(instance)`, as a loot function subclass calls it
//! unqualified (so its `invokestatic` names the subclass), answers the modelled `Products.P1`. A jar
//! class that declares its own `commonFields` keeps its own body: the model never replaces jar code
//! (#410 review). The stronger Java oracle facts are each checked exactly when Decaf can decide
//! them and otherwise run as counted calls; only `demo/` ships, with game and DFU classes as
//! compile-only stubs.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;

const ORACLE: &str = include_str!("fixtures/loot_common_fields/java25.txt");

const SOURCES: &[(&str, &str)] = &[
    (
        "com/mojang/serialization/MapCodec.java",
        "package com.mojang.serialization; public abstract class MapCodec<A> { public abstract java.util.stream.Stream<Object> keys(DynamicOps<?> ops); }",
    ),
    (
        "com/mojang/serialization/DynamicOps.java",
        "package com.mojang.serialization; public interface DynamicOps<A> {}",
    ),
    (
        "com/mojang/serialization/JsonOps.java",
        "package com.mojang.serialization; public class JsonOps implements DynamicOps<Object> { public static final JsonOps INSTANCE = null; }",
    ),
    (
        "com/mojang/datafixers/Products.java",
        "package com.mojang.datafixers; public interface Products { final class P1<F, T1> { public com.mojang.datafixers.kinds.App t1() { return null; } public com.mojang.datafixers.kinds.App t2() { return null; } } }",
    ),
    (
        "com/mojang/datafixers/kinds/App.java",
        "package com.mojang.datafixers.kinds; public interface App<F, A> {}",
    ),
    (
        "com/mojang/serialization/codecs/RecordCodecBuilder.java",
        r#"package com.mojang.serialization.codecs;
public final class RecordCodecBuilder<O, F> implements com.mojang.datafixers.kinds.App<RecordCodecBuilder.Mu<O>, F> {
  public static <O> Instance<O> instance() { return null; }
  public static com.mojang.serialization.MapCodec<?> build(com.mojang.datafixers.kinds.App a) { return null; }
  public static final class Instance<O> { }
  public static final class Mu<O> { }
}"#,
    ),
    (
        "net/minecraft/world/level/storage/loot/functions/LootItemConditionalFunction.java",
        r#"package net.minecraft.world.level.storage.loot.functions;
import com.mojang.datafixers.Products;
import com.mojang.serialization.codecs.RecordCodecBuilder;
public abstract class LootItemConditionalFunction {
  protected LootItemConditionalFunction(java.util.Optional<?> condition) {}
  protected static <T extends LootItemConditionalFunction> Products.P1<RecordCodecBuilder.Mu<T>, Object> commonFields(RecordCodecBuilder.Instance<T> i) { return null; }
}"#,
    ),
    (
        "demo/Inherits.java",
        r#"package demo;
import net.minecraft.world.level.storage.loot.functions.LootItemConditionalFunction;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import com.mojang.datafixers.Products;
public final class Inherits extends LootItemConditionalFunction {
  Inherits(java.util.Optional<?> condition) { super(condition); }
  static Products.P1 product() { return commonFields(RecordCodecBuilder.instance()); }
  static Object component() { return product().t1(); }
  public static String productClass() { return product().getClass().getName(); }
  public static String builderClass() { return component().getClass().getName(); }
  static java.util.function.Function<Object,Object> getter() throws Exception {
    java.lang.reflect.Field f = RecordCodecBuilder.class.getDeclaredField("getter");
    f.setAccessible(true);
    return (java.util.function.Function<Object,Object>) f.get(component());
  }
  public static String emptySame() throws Exception {
    var getter = getter();
    var empty = java.util.Optional.empty();
    return "emptySame=" + (getter.apply(new Inherits(empty)) == empty);
  }
  public static String nonemptySame() throws Exception {
    var getter = getter();
    var nonempty = java.util.Optional.of(new Object());
    return "nonemptySame=" + (getter.apply(new Inherits(nonempty)) == nonempty);
  }
  public static String codecClass() {
    return "codec=" + RecordCodecBuilder.build((com.mojang.datafixers.kinds.App) component()).getClass().getName();
  }
  public static String keys() {
    var codec = RecordCodecBuilder.build((com.mojang.datafixers.kinds.App) component());
    return "keys=" + codec.keys(com.mojang.serialization.JsonOps.INSTANCE).map(Object::toString).sorted().collect(java.util.stream.Collectors.joining(","));
  }
  public static String nullInstanceClass() {
    try { commonFields(null); return "returned"; }
    catch (NullPointerException e) { return e.getClass().getName(); }
  }
  public static String nullInstanceMessage() {
    try { commonFields(null); return "returned"; }
    catch (NullPointerException e) { return e.getMessage(); }
  }
  public static String outOfRange() { return String.valueOf(product().t2()); }
  public static String fields() { return String.valueOf(commonFields(new RecordCodecBuilder.Instance<>()) != null); }
  public static String shape() { Products.P1 p = product(); return (p != null) + "|" + (p.t1() != null); }
}"#,
    ),
    (
        "demo/Declares.java",
        r#"package demo;
import com.mojang.datafixers.Products;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.minecraft.world.level.storage.loot.functions.LootItemConditionalFunction;
public final class Declares extends LootItemConditionalFunction {
  Declares() { super(java.util.Optional.empty()); }
  static boolean ran;
  protected static <T extends LootItemConditionalFunction> Products.P1<RecordCodecBuilder.Mu<T>, Object> commonFields(RecordCodecBuilder.Instance<T> i) { ran = true; return null; }
  public static String fields() { Object p = commonFields(null); return ran + "|" + (p != null); }
}"#,
    ),
];

fn fields(fixture: &Fixture, class: &str) -> (String, usize) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let value = fixture
        .jar
        .invoke_method(class, "fields", "()Ljava/lang/String;", None, &mut host)
        .unwrap();
    let Value::Ref(Some(text)) = value else {
        panic!("{class}: {value:?}");
    };
    (text, host.unknown_host_call_count())
}

#[test]
fn inherited_common_fields_is_modelled_and_a_jar_override_runs_its_own_body() {
    let fixture = Fixture::build("loot-common-fields", SOURCES);
    // Inherited from the vanilla class: the modelled P1, nothing counted.
    assert_eq!(fields(&fixture, "demo/Inherits"), ("true".into(), 0));
    let mut host = fixture.jar.seed_host(StdHost::new());
    let shape = fixture
        .jar
        .invoke_method(
            "demo/Inherits",
            "shape",
            "()Ljava/lang/String;",
            None,
            &mut host,
        )
        .unwrap();
    assert_eq!(shape, Value::Ref(Some("true|true".into())));
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
    // Declared by the jar: its own body runs (it answers null), never the model.
    assert_eq!(fields(&fixture, "demo/Declares"), ("true|false".into(), 0));
}

#[test]
fn strong_oracle_facts_are_exact_or_counted_individually() {
    let fixture = Fixture::build("loot-strong-oracle", SOURCES);
    let expected: Vec<_> = ORACLE.trim_end().split('|').collect();
    for (method, expected) in [
        "productClass",
        "builderClass",
        "emptySame",
        "nonemptySame",
        "codecClass",
        "keys",
    ]
    .into_iter()
    .zip(expected)
    {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let value = fixture.jar.invoke_method(
            "demo/Inherits",
            method,
            "()Ljava/lang/String;",
            None,
            &mut host,
        );
        if host.unknown_host_calls().is_empty() {
            assert_eq!(
                value.unwrap(),
                Value::Ref(Some(expected.into())),
                "{method}"
            );
        } else {
            eprintln!("{method}: COUNT {:?}", host.unknown_host_calls());
        }
    }
}

#[test]
fn nonexistent_product_accessor_is_counted() {
    let fixture = Fixture::build("loot-product-accessor", SOURCES);
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_strict_unknown_host_calls());
    assert!(
        fixture
            .jar
            .invoke_method(
                "demo/Inherits",
                "outOfRange",
                "()Ljava/lang/String;",
                None,
                &mut host
            )
            .is_err()
    );
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "com/mojang/datafixers/Products$P1#t2()Lcom/mojang/datafixers/kinds/App;".into(),
            1
        )]
    );
}

#[test]
fn null_instance_message_is_counted() {
    let fixture = Fixture::build("loot-null-instance", SOURCES);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.jar.invoke_method(
        "demo/Inherits",
        "nullInstanceMessage",
        "()Ljava/lang/String;",
        None,
        &mut host,
    );
    assert!(
        !host.unknown_host_calls().is_empty(),
        "the helpful NPE cause is not modelled"
    );
}
