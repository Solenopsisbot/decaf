//! Java 25 fixture for the 26.3 client input surface. The expected lines were
//! recorded from the real client.jar on OpenJDK 25; stubs only let javac build
//! the probe, so execution crosses the native host model.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{NativeError, StdHost};
const ORACLE: &str = include_str!("fixtures/client_keys/java25.txt");
const PROBE: &str = include_str!("fixtures/client_keys/demo/Probe.java");
const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier {}",
    ),
    (
        "com/mojang/blaze3d/platform/InputConstants.java",
        r#"package com.mojang.blaze3d.platform;
public final class InputConstants {
  public static final Key UNKNOWN = null;
  public static Key getKey(String n) { return null; }
  public enum Type { KEYBOARD, MOUSE; public Key getOrCreate(int n) { return null; } }
  public static final class Key { public Type getType(){return null;} public int getValue(){return 0;} public String getName(){return null;} public String toString(){return null;} public boolean equals(Object o){return false;} }
}"#,
    ),
    (
        "net/minecraft/client/KeyMapping.java",
        r#"package net.minecraft.client;
import com.mojang.blaze3d.platform.InputConstants;
public class KeyMapping {
  public static final class Category { public static final Category MISC = null; }
  public KeyMapping(String n, InputConstants.Type t, int k, Category c) {}
  public InputConstants.Key getDefaultKey(){return null;} public InputConstants.Key getKey(){return null;}
  public void setKey(InputConstants.Key k) {} public boolean same(KeyMapping k){return false;}
}"#,
    ),
    (
        "net/minecraft/client/Minecraft.java",
        "package net.minecraft.client; import java.io.File; public class Minecraft { public File gameDirectory; public static Minecraft getInstance(){return null;} }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item; public final class ItemStack { public static final ItemStack EMPTY = null; public boolean isEmpty(){return false;} }",
    ),
];
fn fixture() -> javac_fixture::Fixture {
    let mut s = STUBS.to_vec();
    s.push(("demo/Probe.java", PROBE));
    javac_fixture::Fixture::build("client-keys", &s)
}
#[test]
fn client_keys_match_java25() {
    let f = fixture();
    let mut host = f.jar.seed_host(StdHost::new());
    let printed = f.text_on(&mut host, "run");
    assert_eq!(printed, ORACLE);
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
#[test]
fn canonical_identity_has_no_counted_reference_comparisons() {
    let f = fixture();
    for method in ["idKey", "idCreate", "idUnknown", "idEmpty"] {
        let mut host = f.jar.seed_host(StdHost::new());
        assert_eq!(f.text_on(&mut host, method), "true", "{method}");
        assert!(
            host.unknown_host_calls().is_empty(),
            "{method}: {:?}",
            host.unknown_host_calls()
        );
    }
}

#[test]
fn game_directory_is_counted_without_a_file_handle() {
    let f = fixture();
    let mut host = f.jar.seed_host(StdHost::new());
    let outcome = f.jar.invoke_method_with_args(
        "demo/Probe",
        "dir",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert!(matches!(
        outcome,
        Err(NativeError::UnknownHostCall(r))
            if r.owner == "net/minecraft/client/Minecraft" && r.name == "gameDirectory"
    ));
}

#[test]
fn null_key_name_matches_java_exception() {
    let f = fixture();
    let mut host = f.jar.seed_host(StdHost::new());
    assert_eq!(
        f.text_on(&mut host, "badNull"),
        "java.lang.NullPointerException|Cannot invoke \"String.startsWith(String)\" because \"name\" is null"
    );
}

#[test]
fn unknown_key_name_is_counted_as_java_exception() {
    let f = fixture();
    let mut host = f.jar.seed_host(StdHost::new());
    let printed = f.text_on(&mut host, "bad");
    assert_eq!(
        printed,
        "java.lang.IllegalArgumentException|Unknown key name: key.scancode.4"
    );
}
