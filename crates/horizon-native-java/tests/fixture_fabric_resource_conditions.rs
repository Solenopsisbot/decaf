//! Fabric's `ResourceConditionType.create` and `ResourceConditions.register` (#454,
//! `fabric_resource_conditions.rs`).
//!
//! `fixtures/fabric_resource_conditions/demo/Probe.java` runs on Java 25 against the real Fabric
//! API resource-conditions module and the 26.3 jar (`java25.txt`, command in the README), and here
//! against compile-only stubs: only `demo/` ships, so every call reaches the host. `rows()` must
//! equal the capture byte for byte with nothing counted. The other probe methods are cases Decaf
//! counts rather than answers; their Java 25 answers are in `java25_counted.txt`, and each one
//! must be counted exactly as named here.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/fabric_resource_conditions/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/fabric_resource_conditions/java25.txt");

const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { return null; } }",
    ),
    (
        "com/mojang/serialization/MapCodec.java",
        "package com.mojang.serialization; public abstract class MapCodec<A> { public static <A> MapCodec<A> unit(A a) { return null; } }",
    ),
    (
        "net/fabricmc/fabric/api/resource/conditions/v1/ResourceCondition.java",
        "package net.fabricmc.fabric.api.resource.conditions.v1; public interface ResourceCondition {}",
    ),
    (
        "net/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType.java",
        "package net.fabricmc.fabric.api.resource.conditions.v1; import net.minecraft.resources.Identifier; import com.mojang.serialization.MapCodec; public interface ResourceConditionType<T extends ResourceCondition> { Identifier id(); MapCodec<T> codec(); static <T extends ResourceCondition> ResourceConditionType<T> create(Identifier id, MapCodec<T> codec) { return null; } }",
    ),
    (
        "net/fabricmc/fabric/api/resource/conditions/v1/ResourceConditions.java",
        "package net.fabricmc.fabric.api.resource.conditions.v1; public final class ResourceConditions { public static void register(ResourceConditionType<?> type) {} }",
    ),
];

const CREATE: &str = "net/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType#create(Lnet/minecraft/resources/Identifier;Lcom/mojang/serialization/MapCodec;)Lnet/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType;";
const REGISTER: &str = "net/fabricmc/fabric/api/resource/conditions/v1/ResourceConditions#register(Lnet/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType;)V";
const HASH: &str =
    "net/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType#hashCode()I";

fn fixture() -> Fixture {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    Fixture::build("fabric-resource-conditions", &sources)
}

#[test]
fn resource_condition_rows_match_java25() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    let unknown = host.unknown_host_calls();
    assert_eq!(rows, ORACLE, "unknown calls: {unknown:?}");
    assert_eq!(unknown, Vec::<(String, usize)>::new());
}

#[test]
fn uncertain_cases_are_counted() {
    let fixture = fixture();
    // (probe method, the one call it must count). Java 25's answers: `java25_counted.txt`.
    for (method, counted) in [
        // A null may be a counted call's answer, so Fabric's NullPointerException isn't thrown.
        ("nullId", CREATE),
        ("nullCodec", CREATE),
        ("registerNull", REGISTER),
        // `fabric:true` is one of Fabric's own types: registered or not depends on load order.
        ("fabricDefault", REGISTER),
        // The anonymous type's identity hash.
        ("hash", HASH),
    ] {
        let mut host = fixture.jar.seed_host(StdHost::new());
        fixture.text_on(&mut host, method);
        assert_eq!(
            host.unknown_host_calls(),
            vec![(counted.to_owned(), 1)],
            "{method}"
        );
    }
}
