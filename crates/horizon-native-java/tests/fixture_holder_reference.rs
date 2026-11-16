//! A Java 25 oracle slice for holders returned by Registry.registerForHolder.
//! The host probe exercises the pre-freeze state. The committed oracle records
//! both states from a real writable 26.3 MappedRegistry run.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{RunLoader, StdHost, Value};

const REAL_ORACLE: &str = include_str!("fixtures/holder_reference/java25.txt");
const EXPECTED: &str = "demo:item|Optional[demo:item]|value|truefalse|truefalse|Optional[ResourceKey[minecraft:item / demo:item]]|Left[ResourceKey[minecraft:item / demo:item]]|REFERENCE|truefalse|Reference{ResourceKey[minecraft:item / demo:item]=value}|truefalse|false";

const IDENTIFIER: &str = r#"package net.minecraft.resources;
public final class Identifier {
    public static Identifier parse(String text) { return null; }
}
"#;

const RESOURCE_KEY: &str = r#"package net.minecraft.resources;
public class ResourceKey<T> {
    public static <T> ResourceKey<T> create(ResourceKey<?> registry, Identifier id) { return null; }
    public static <T> ResourceKey<T> createRegistryKey(Identifier id) { return null; }
}
"#;

const REGISTRY: &str = r#"package net.minecraft.core;
public interface Registry<T> {
    net.minecraft.resources.ResourceKey<?> key();
    static <R,T extends R> Holder.Reference<T> registerForHolder(Registry<R> registry, net.minecraft.resources.ResourceKey<R> key, T value) { return null; }
    static <R,T extends R> Holder.Reference<T> registerForHolder(Registry<R> registry, net.minecraft.resources.Identifier id, T value) { return null; }
}
"#;

const BUILT_IN_REGISTRIES: &str = r#"package net.minecraft.core.registries;
import net.minecraft.core.Registry;
public final class BuiltInRegistries {
    public static final Registry<Object> ITEM = null;
}
"#;

const HOLDER: &str = r#"package net.minecraft.core;
import java.util.Optional;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import com.mojang.datafixers.util.Either;
public interface Holder<T> {
    T value();
    boolean isBound();
    boolean areComponentsBound();
    boolean is(Identifier id);
    boolean is(ResourceKey<T> key);
    Optional<ResourceKey<T>> unwrapKey();
    Either<ResourceKey<T>, T> unwrap();
    Kind kind();
    default Optional<String> getRegisteredNameIfPresent() { return unwrapKey().map(k -> k.toString()); }
    default String getRegisteredName() { return getRegisteredNameIfPresent().orElse("[unregistered]"); }
    class Reference<T> implements Holder<T> {
        public ResourceKey<T> key() { return null; }
        public T value() { return null; }
        public boolean is(Identifier id) { return false; }
        public boolean is(ResourceKey<T> key) { return false; }
        public boolean isBound() { return false; }
        public boolean areComponentsBound() { return false; }
        public Optional<ResourceKey<T>> unwrapKey() { return Optional.empty(); }
        public Either<ResourceKey<T>, T> unwrap() { return null; }
        public Kind kind() { return null; }
    }
    enum Kind { REFERENCE, DIRECT }
}
"#;

const EITHER: &str = r#"package com.mojang.datafixers.util;
public abstract class Either<L,R> {
    public static <L,R> Either<L,R> left(L value) { return null; }
    public static <L,R> Either<L,R> right(R value) { return null; }
}
"#;

const PROBE: &str = r#"package demo;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.*;
public final class Probe {
    public static String holder() {
        Registry<Object> registry = BuiltInRegistries.ITEM;
        Holder.Reference<Object> h = Registry.registerForHolder(registry, Identifier.parse("demo:item"), "value");
        Holder.Reference<Object> other = Registry.registerForHolder(registry, Identifier.parse("demo:other"), "other");
        ResourceKey<Object> key = h.key();
        Object forged = "rhold:hacked";
        String value;
        try { value = String.valueOf(h.value()); }
        catch (IllegalStateException e) { value = e.getMessage(); }
        return h.getRegisteredName() + "|" + h.getRegisteredNameIfPresent()
            + "|" + value + "|" + h.is(Identifier.parse("demo:item")) + h.is(Identifier.parse("demo:other"))
            + "|" + h.is(key) + h.is(other.key()) + "|" + h.unwrapKey() + "|" + h.unwrap()
            + "|" + h.kind() + "|" + h.isBound() + h.areComponentsBound()
            + "|" + h + "|" + h.equals(h) + h.equals(other)
            + "|" + (forged instanceof Holder);
    }
}
"#;

#[test]
fn self_registered_holders_match_java_25() {
    let fixture = javac_fixture::Fixture::build(
        "registry-holder",
        &[
            ("net/minecraft/resources/Identifier.java", IDENTIFIER),
            ("net/minecraft/resources/ResourceKey.java", RESOURCE_KEY),
            ("net/minecraft/core/Registry.java", REGISTRY),
            (
                "net/minecraft/core/registries/BuiltInRegistries.java",
                BUILT_IN_REGISTRIES,
            ),
            ("net/minecraft/core/Holder.java", HOLDER),
            ("com/mojang/datafixers/util/Either.java", EITHER),
            ("demo/Probe.java", PROBE),
        ],
    );
    assert_eq!(fixture.text("holder"), EXPECTED);

    let mut fabric_without_sync = fixture
        .jar
        .seed_host(StdHost::new().with_run_loader(RunLoader::Fabric));
    let without_sync = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "holder",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut fabric_without_sync,
        )
        .unwrap();
    assert_eq!(without_sync, Value::Ref(Some(EXPECTED.into())));

    let mut fabric_with_sync = fixture.jar.seed_host(
        StdHost::new()
            .with_run_loader(RunLoader::Fabric)
            .with_mod("fabric-registry-sync-v0", "8.0.4"),
    );
    let bound = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "holder",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut fabric_with_sync,
        )
        .unwrap();
    assert_eq!(bound, Value::Ref(Some(EXPECTED.into())));

    assert!(REAL_ORACLE.contains("before|"));
    assert!(REAL_ORACLE.contains("after|"));
    assert!(REAL_ORACLE.contains("fabric-sync|"));
    assert_eq!(REAL_ORACLE.lines().count(), 3);
}
