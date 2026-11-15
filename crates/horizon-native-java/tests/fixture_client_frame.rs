//! C4 frame reads with compile-only client signatures and the real 26.3 client oracle.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{
    StdHost,
    client_frame::{ClientHitResult, ClientOptions},
    client_surface::{ClientPlayer, ClientSnapshot, KeyPress},
};
use javac_fixture::Fixture;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const PROBE: &str = r#"
package demo;
import net.minecraft.client.Minecraft;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.HitResult;
import net.minecraft.core.BlockPos;
public final class Probe {
  public static String run() {
    Minecraft mc = Minecraft.getInstance();
    Vec3 a = mc.player.position();
    Vec3 b = a.add(-5.25, 7.5, -3.5);
    Vec3 n = a.normalize();
    HitResult h = mc.hitResult;
    BlockPos p = mc.player.blockPosition();
    BlockPos q = BlockPos.containing(1.25, -2.5, 3.75);
    return a.add(b)+"|"+a.subtract(1.5,2.5,-3.5)+"|"+a.scale(-2.0)
      +"|"+a.length()+"|"+a.lengthSqr()+"|"+n+"|"+a.distanceTo(b)
      +"|"+a.cross(b)+"|"+a.dot(b)+"|"+p+"|"+q
      +"|"+h.getType()+"|"+h.getType().ordinal()+"|"+h.getLocation()
      +"|"+mc.options.particles().get()+"|"+mc.options.sensitivity().get()
      +"|"+mc.getWindow().getGuiScale();
  }
}
"#;

const JSPECIFY_STUBS: &[(&str, &str)] = &[
    (
        "org/jspecify/annotations/Nullable.java",
        "package org.jspecify.annotations; public @interface Nullable {}\n",
    ),
    (
        "org/jspecify/annotations/NonNull.java",
        "package org.jspecify.annotations; public @interface NonNull {}\n",
    ),
    (
        "org/jspecify/annotations/NullMarked.java",
        "package org.jspecify.annotations; public @interface NullMarked {}\n",
    ),
    (
        "org/jspecify/annotations/NullnessUnspecified.java",
        "package org.jspecify.annotations; public @interface NullnessUnspecified {}\n",
    ),
];

const CLIENT_API_STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/client/Minecraft.java",
        r#"package net.minecraft.client;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.phys.HitResult;
import com.mojang.blaze3d.platform.Window;
public class Minecraft {
  public LocalPlayer player; public Options options; public HitResult hitResult;
  public static Minecraft getInstance() { throw new UnsupportedOperationException(); }
  public Window getWindow() { throw new UnsupportedOperationException(); }
}
"#,
    ),
    (
        "net/minecraft/client/player/LocalPlayer.java",
        r#"package net.minecraft.client.player;
import net.minecraft.core.BlockPos;
import net.minecraft.world.phys.Vec3;
public class LocalPlayer {
  public Vec3 position() { throw new UnsupportedOperationException(); }
  public BlockPos blockPosition() { throw new UnsupportedOperationException(); }
}
"#,
    ),
    (
        "net/minecraft/client/Options.java",
        r#"package net.minecraft.client;
public class Options {
  public OptionInstance<ParticleStatus> particles() { throw new UnsupportedOperationException(); }
  public OptionInstance<Double> sensitivity() { throw new UnsupportedOperationException(); }
}
"#,
    ),
    (
        "net/minecraft/client/ParticleStatus.java",
        r#"package net.minecraft.client;
public enum ParticleStatus { ALL, DECREASED, MINIMAL }
"#,
    ),
    (
        "net/minecraft/client/OptionInstance.java",
        r#"package net.minecraft.client;
public class OptionInstance<T> { public T get() { throw new UnsupportedOperationException(); } }
"#,
    ),
    (
        "com/mojang/blaze3d/platform/Window.java",
        r#"package com.mojang.blaze3d.platform;
public class Window { public int getGuiScale() { throw new UnsupportedOperationException(); } }
"#,
    ),
    (
        "net/minecraft/world/phys/Vec3.java",
        r#"package net.minecraft.world.phys;
public class Vec3 {
  public final double x, y, z;
  public Vec3(double x, double y, double z) { this.x=x; this.y=y; this.z=z; }
  public Vec3 add(double x, double y, double z) { throw new UnsupportedOperationException(); }
  public Vec3 add(Vec3 other) { throw new UnsupportedOperationException(); }
  public Vec3 subtract(double x, double y, double z) { throw new UnsupportedOperationException(); }
  public Vec3 scale(double scale) { throw new UnsupportedOperationException(); }
  public Vec3 normalize() { throw new UnsupportedOperationException(); }
  public double length() { throw new UnsupportedOperationException(); }
  public double lengthSqr() { throw new UnsupportedOperationException(); }
  public double distanceTo(Vec3 other) { throw new UnsupportedOperationException(); }
  public Vec3 cross(Vec3 other) { throw new UnsupportedOperationException(); }
  public double dot(Vec3 other) { throw new UnsupportedOperationException(); }
  @Override public String toString() { throw new UnsupportedOperationException(); }
}
"#,
    ),
    (
        "net/minecraft/world/phys/HitResult.java",
        r#"package net.minecraft.world.phys;
public class HitResult {
  public Type getType() { throw new UnsupportedOperationException(); }
  public Vec3 getLocation() { throw new UnsupportedOperationException(); }
  public enum Type { MISS, BLOCK, ENTITY }
}
"#,
    ),
    (
        "net/minecraft/core/BlockPos.java",
        r#"package net.minecraft.core;
public class BlockPos {
  public static BlockPos containing(double x, double y, double z) { throw new UnsupportedOperationException(); }
  @Override public String toString() { throw new UnsupportedOperationException(); }
}
"#,
    ),
];

const EXPECTED: &str = "(-2.75, 2.5, 4.0)|(-0.25, -5.0, 7.25)|(-2.5, 5.0, -7.5)|4.677071733467427|21.875|(0.2672612419124244, -0.5345224838248488, 0.8017837257372731)|9.80114789195633|(-19.375, -15.3125, -3.75)|-16.5625|BlockPos{x=1, y=-3, z=3}|BlockPos{x=1, y=-3, z=3}|MISS|0|(1.25, -2.5, 3.75)|ALL|0.5|2";

fn javac_path() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn compile_stubs(include_client_api: bool) -> (PathBuf, String) {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "horizon-client-frame-api-{}-{unique}",
        std::process::id()
    ));
    let source_root = root.join("src");
    let classes = root.join("classes");
    let mut sources = JSPECIFY_STUBS.to_vec();
    if include_client_api {
        sources.extend_from_slice(CLIENT_API_STUBS);
    }
    let mut files = Vec::new();
    for (path, source) in sources {
        let file = source_root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        files.push(file);
    }
    let mut javac = Command::new(javac_path());
    javac.args(["--release", "21", "-d"]).arg(&classes);
    for file in files {
        javac.arg(file);
    }
    assert!(
        javac.status().unwrap().success(),
        "javac failed for client API stubs"
    );
    let classpath = classes.to_string_lossy().into_owned();
    (root, classpath)
}

fn collect_jars(root: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jars(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "jar") {
            out.push(path);
        }
    }
}

fn real_client_classpath() -> Option<String> {
    let explicit = std::env::var_os("MINECRAFT_CLIENT_JAR")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    if explicit.len() > 1 {
        return Some(
            std::env::join_paths(explicit)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
    }

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut client = explicit.first().cloned();
    if client.is_none() {
        let mut candidates = Vec::new();
        collect_jars(&repo.join(".local"), &mut candidates);
        client = candidates.into_iter().find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name == "client.jar"
                        || name == "minecraft-client.jar"
                        || name == "minecraft-26.3-client.jar"
                        || name == "26.3.jar"
                })
        });
    }
    let Some(client) = client else {
        eprintln!("skipped real-client oracle: no MINECRAFT_CLIENT_JAR or .local 26.3 client jar");
        return None;
    };
    if !client.is_file() {
        eprintln!(
            "skipped real-client oracle: client jar does not exist: {}",
            client.display()
        );
        return None;
    }

    let mut jars = vec![client.clone()];
    let mut roots = Vec::new();
    for variable in ["MINECRAFT_CLIENT_LIBRARIES", "MINECRAFT_LIBRARIES"] {
        if let Some(value) = std::env::var_os(variable) {
            roots.extend(std::env::split_paths(&value));
        }
    }
    if let Some(parent) = client.parent() {
        roots.push(parent.join("libraries"));
        if let Some(grandparent) = parent.parent() {
            roots.push(grandparent.join("libraries"));
        }
    }
    let mut discovered = Vec::new();
    for root in roots.into_iter().filter(|root| root.is_dir()) {
        collect_jars(&root, &mut discovered);
    }
    discovered.sort();
    discovered.dedup();
    jars.extend(discovered);
    jars.sort();
    jars.dedup();
    if jars.len() == 1 {
        eprintln!(
            "skipped real-client oracle: no 26.3 client libraries found beside {}",
            client.display()
        );
        return None;
    }
    Some(
        std::env::join_paths(jars)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    )
}

fn assert_levels(classpath: &str, name: &str) {
    let fixture = Fixture::build_with_classpath(name, &[("demo/Probe.java", PROBE)], classpath);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let snapshot = Arc::new(ClientSnapshot {
        player: Some(ClientPlayer {
            pos: [1.25, -2.5, 3.75],
            health: 20.0,
            max_health: 20.0,
            using_item: Some(false),
            sleeping: Some(false),
            ..ClientPlayer::default()
        }),
        options: ClientOptions {
            particles: Some("ALL".into()),
            sensitivity: Some(0.5),
        },
        hit_result: Some(ClientHitResult::Miss {
            location: [1.25, -2.5, 3.75],
        }),
        gui_scale: Some(2),
        ..ClientSnapshot::default()
    });
    host.begin_client_frame(snapshot, &[KeyPress::default()]);
    let printed = fixture.text_on(&mut host, "run");
    host.end_client_frame();
    assert_eq!(printed, EXPECTED);
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}

#[test]
fn client_frame_reads_match_java25_oracle() {
    // EXPECTED is recorded from Java 25.0.5 + the real 26.3 client.jar run.
    let (stub_root, stub_classpath) = compile_stubs(true);
    assert_levels(&stub_classpath, "client-frame-stub");
    let _ = fs::remove_dir_all(stub_root);

    if let Some(client_classpath) = real_client_classpath() {
        let (jspecify_root, jspecify_classpath) = compile_stubs(false);
        let separator = if cfg!(windows) { ";" } else { ":" };
        let classpath = format!("{jspecify_classpath}{separator}{client_classpath}");
        assert_levels(&classpath, "client-frame-real");
        let _ = fs::remove_dir_all(jspecify_root);
    }
}
