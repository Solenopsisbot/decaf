//! A jar's bundled libraries are on its class path (`META-INF/jars/*.jar`).
//!
//! Fabric's Jar-in-Jar puts a bundle's classes on the mod's class path, and a
//! loader's dependency graph sees the bundled jars' own mod ids. Before this,
//! `respackopts` called `dev.jfronny.commons.ref.WeakSet` from its bundled
//! `libjf-base` dependency and the runtime answered "class not on the class
//! path" (`docs/in-game-survey.md`, "Top causes" §5).

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, dependencies};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const OUTER: &str = r##"package demo;

public class Entry implements net.fabricmc.api.ModInitializer {
    @Override
    public void onInitialize() {
        System.out.println(Inner.tag());
    }
}
"##;

const INNER: &str = r##"package demo;

public class Inner {
    public static String tag() {
        return "from-inner";
    }
}
"##;

const MOD_INITIALIZER: &str = r##"package net.fabricmc.api;

public interface ModInitializer {
    void onInitialize();
}
"##;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

fn metadata_jar(metadata: &str) -> NativeJar {
    let mut bytes = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut bytes);
    zip.start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(metadata.as_bytes()).unwrap();
    zip.finish().unwrap();
    NativeJar::from_bytes(&bytes.into_inner()).unwrap()
}

/// Compile `sources` under `root/src` into `root/classes`.
fn compile(root: &Path, classpath: Option<&Path>, sources: &[(&str, &str)]) {
    let classes = root.join("classes");
    let mut javac = Command::new(javac());
    javac.args(["--release", "21", "-d"]).arg(&classes);
    if let Some(classpath) = classpath {
        javac.arg("-cp").arg(classpath);
    }
    for (path, source) in sources {
        let file = root.join("src").join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        javac.arg(file);
    }
    assert!(javac.status().unwrap().success(), "javac failed");
}

/// A jar holding every class under `root/classes` whose name starts with
/// `prefix`, plus `metadata`.
fn jar_from(root: &Path, prefix: &str, metadata: Option<&str>) -> Vec<u8> {
    let path = root.join(format!("{}.jar", prefix.replace('/', "-")));
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    if let Some(metadata) = metadata {
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(metadata.as_bytes()).unwrap();
    }
    let classes = root.join("classes");
    for class in files_under(&classes.join(prefix)) {
        let entry = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(entry, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    fs::read(&path).unwrap()
}

struct Fixture {
    root: PathBuf,
    jar: NativeJar,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn build_with_layout(test: &str, nested_dir: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = scratch::unique_path(&format!(
            "horizon-native-java-nested-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };

        // The bundled library compiles on its own, exactly like a published
        // library jar.
        let inner_root = fixture.root.join("inner");
        compile(&inner_root, None, &[("demo/Inner.java", INNER)]);
        let inner = jar_from(
            &inner_root,
            "demo",
            if nested_dir.starts_with("META-INF/jarjar/") {
                None
            } else {
                Some(r#"{"schemaVersion":1,"id":"inner-lib","version":"1.0"}"#)
            },
        );

        let outer_root = fixture.root.join("outer");
        compile(
            &outer_root,
            Some(&inner_root.join("classes")),
            &[
                ("demo/Entry.java", OUTER),
                ("net/fabricmc/api/ModInitializer.java", MOD_INITIALIZER),
            ],
        );
        let classes = outer_root.join("classes");
        let nested_entry = format!("{nested_dir}inner.jar");
        let jar_path = outer_root.join("outer.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        if nested_dir.starts_with("META-INF/jarjar/") {
            zip.start_file("META-INF/neoforge.mods.toml", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"modLoader = \"javafml\"\n[[mods]]\nmodId = \"outer-fixture\"\nversion = \"1.0\"\n")
                .unwrap();
            zip.start_file(
                "META-INF/jarjar/metadata.json",
                SimpleFileOptions::default(),
            )
            .unwrap();
            let metadata = format!(
                r#"{{"jars":[{{"path":"{nested_entry}","identifier":{{"group":"demo","artifact":"inner"}},"version":{{"range":"[1,2)","artifactVersion":"1.0"}}}}]}}"#,
            );
            zip.write_all(metadata.as_bytes()).unwrap();
        } else {
            zip.start_file("fabric.mod.json", SimpleFileOptions::default())
                .unwrap();
            let metadata = format!(
                r#"{{"schemaVersion":1,"id":"outer-fixture","version":"1.0","entrypoints":{{"main":"demo.Entry"}},"depends":{{"inner-lib":">=1"}},"jars":[{{"file":"{nested_entry}"}}]}}"#,
            );
            zip.write_all(metadata.as_bytes()).unwrap();
        }
        zip.start_file(&nested_entry, SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&inner).unwrap();
        for class in files_under(&classes.join("demo")) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();

        fixture.jar = NativeJar::from_path(&jar_path).unwrap();
        fixture
    }

    fn build(test: &str) -> Self {
        Self::build_with_layout(test, "META-INF/jars/")
    }

    fn build_jarjar(test: &str) -> Self {
        Self::build_with_layout(test, "META-INF/jarjar/")
    }

    /// Run the entrypoint and return what it printed.
    fn printed(&self) -> String {
        let mut host = self
            .jar
            .seed_host(StdHost::new().with_environment_type("SERVER"));
        let entries = self.jar.discover_entrypoints_on("main", "SERVER");
        for entry in &entries {
            self.jar
                .execute_with_budget(
                    &entry.class,
                    entry.method.as_deref(),
                    "main",
                    &mut host,
                    &horizon_native_java::ExecutionBudget::within(std::time::Duration::from_secs(
                        30,
                    )),
                )
                .unwrap();
        }
        host.take_stdout()
    }

    fn printed_jarjar(&self) -> String {
        let mut host = self
            .jar
            .seed_host(StdHost::new().with_environment_type("SERVER"));
        self.jar
            .execute_with_budget(
                "demo.Entry",
                Some("onInitialize"),
                "main",
                &mut host,
                &horizon_native_java::ExecutionBudget::within(std::time::Duration::from_secs(30)),
            )
            .unwrap();
        host.take_stdout()
    }
}

#[test]
fn a_bundled_jar_s_class_is_on_the_outer_jar_s_class_path() {
    let fixture = Fixture::build("classpath");
    assert_eq!(fixture.printed().trim(), "from-inner");
}

#[test]
fn a_bundle_provides_the_mod_ids_of_its_bundled_jars() {
    let fixture = Fixture::build("plan");
    let jars = [("outer.jar", &fixture.jar)];
    let plan = dependencies::plan(&jars);
    // `outer-fixture`’s nested jar declares the id `inner-lib`; nothing else
    // is missing.
    assert!(plan.missing.is_empty(), "missing: {:?}", plan.missing);
}

#[test]
fn a_neoforge_jarjar_library_is_on_the_outer_jar_s_class_path() {
    let fixture = Fixture::build_jarjar("jarjar-classpath");
    assert_eq!(fixture.printed_jarjar().trim(), "from-inner");
}

#[test]
fn fabric_accepts_a_dotdot_nested_entry_name() {
    let fixture = Fixture::build_with_layout("dotdot", "../");
    assert_eq!(fixture.printed().trim(), "from-inner");
}

#[test]
fn duplicate_fabric_ids_and_aliases_refuse_with_fabric_message() {
    let first = metadata_jar(r#"{"schemaVersion":1,"id":"duplicate","version":"1"}"#);
    let second = metadata_jar(r#"{"schemaVersion":1,"id":"duplicate","version":"2"}"#);
    let host = first.try_seed_host(StdHost::new()).unwrap();
    let error = second.try_seed_host(host).unwrap_err();
    assert_eq!(error.to_string(), "duplicate mod duplicate");

    let repeated_alias = metadata_jar(
        r#"{"schemaVersion":1,"id":"alias-owner","version":"1","provides":["alias","alias"]}"#,
    );
    let error = repeated_alias.try_seed_host(StdHost::new()).unwrap_err();
    assert_eq!(error.to_string(), "duplicate mod alias");
}

/// The mods the probe reports for the bake (`content.mods`): the jar's own, its nested jar's,
/// and the `provides` aliases of both, since Fabric's `isModLoaded` finds an alias. The bake makes
/// a package of each, so Sunrise's `mod_loaded` (package ids) answers as the jar's code was told.
#[test]
fn loaded_jar_mods_hold_nested_mods_and_provides_aliases() {
    let mut bytes = std::io::Cursor::new(Vec::new());
    let mut child = zip::ZipWriter::new(&mut bytes);
    child
        .start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    child
        .write_all(br#"{"schemaVersion":1,"id":"child","version":"1","provides":["child_api"]}"#)
        .unwrap();
    child.finish().unwrap();
    let child = bytes.into_inner();
    let mut bytes = std::io::Cursor::new(Vec::new());
    let mut outer = zip::ZipWriter::new(&mut bytes);
    outer
        .start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    outer
        .write_all(
            br#"{"schemaVersion":1,"id":"outer","version":"1","provides":["outer_compat"],"jars":[{"file":"META-INF/jars/child.jar"}]}"#,
        )
        .unwrap();
    outer
        .start_file("META-INF/jars/child.jar", SimpleFileOptions::default())
        .unwrap();
    outer.write_all(&child).unwrap();
    outer.finish().unwrap();
    let jar = NativeJar::from_bytes(&bytes.into_inner()).unwrap();
    let host = jar.try_seed_host(StdHost::new()).unwrap();
    assert_eq!(
        host.loaded_jar_mods(jar.fabric_id.as_deref()),
        ["outer", "child", "child_api", "outer_compat"]
    );
}

#[test]
fn unlisted_and_explicitly_empty_fabric_jars_do_not_leak_mods() {
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut bytes);
        for (name, content) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
        bytes.into_inner()
    }
    let child = zip(&[(
        "fabric.mod.json",
        br#"{"schemaVersion":1,"id":"child","version":"1"}"#,
    )]);
    let outer = |jars: &str| {
        NativeJar::from_bytes(&zip(&[
            ("fabric.mod.json", jars.as_bytes()),
            ("META-INF/jars/child.jar", &child),
        ]))
        .unwrap()
    };
    let dependent = NativeJar::from_bytes(&zip(&[(
        "fabric.mod.json",
        br#"{"schemaVersion":1,"id":"dependent","version":"1","depends":{"child":">=1"}}"#,
    )]))
    .unwrap();
    let unlisted = outer(r#"{"schemaVersion":1,"id":"outer","version":"1"}"#);
    let explicit_empty = outer(r#"{"schemaVersion":1,"id":"outer","version":"1","jars":[]}"#);
    for outer in [&unlisted, &explicit_empty] {
        let plan = dependencies::plan(&[("dependent.jar", &dependent), ("outer.jar", outer)]);
        assert_eq!(plan.missing.get(&0), Some(&vec!["child".to_owned()]));
    }
    let listed = outer(
        r#"{"schemaVersion":1,"id":"outer","version":"1","jars":[{"file":"META-INF/jars/child.jar"}]}"#,
    );
    let plan = dependencies::plan(&[("dependent.jar", &dependent), ("outer.jar", &listed)]);
    assert!(plan.missing.is_empty(), "missing: {:?}", plan.missing);
}

#[test]
fn outer_resource_wins_over_a_nested_resource_with_the_same_name() {
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut bytes);
        for (name, content) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
        bytes.into_inner()
    }
    let nested = zip(&[
        (
            "fabric.mod.json",
            br#"{"id":"nested-resource","version":"1"}"#,
        ),
        ("shared.txt", b"nested"),
    ]);
    let outer = NativeJar::from_bytes(&zip(&[
        (
            "fabric.mod.json",
            br#"{"id":"outer-resource","version":"1","jars":[{"file":"META-INF/jars/nested.jar"}]}"#,
        ),
        ("shared.txt", b"outer"),
        ("META-INF/jars/nested.jar", &nested),
    ]))
    .unwrap();
    let host = outer.seed_host(StdHost::new());
    assert_eq!(host.resource("shared.txt"), Some("jar-resource/shared.txt"));
}
