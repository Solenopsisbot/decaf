//! Gson TypeAdapter dispatch against the real Gson 2.14.0 jar.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const GSON_RELATIVE: &str = "ava/scratch/vanilla-values/oracle/libraries/server/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar";

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn gson_path() -> PathBuf {
    std::env::var_os("HORIZON_GSON_JAR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(GSON_RELATIVE)
        })
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

struct Fixture {
    root: PathBuf,
    probe: NativeJar,
    gson: NativeJar,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn new() -> Self {
        static BUILDS: AtomicUsize = AtomicUsize::new(0);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "decaf-gson-type-adapter-{}-{unique}-{}",
            std::process::id(),
            BUILDS.fetch_add(1, Ordering::Relaxed)
        ));
        let source = root.join("src/demo/Probe.java");
        let classes = root.join("classes");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(
            &source,
            include_str!("fixtures/gson_type_adapter/demo/Probe.java"),
        )
        .unwrap();
        let gson_path = gson_path();
        assert!(
            gson_path.is_file(),
            "real Gson jar missing: {}",
            gson_path.display()
        );
        assert!(
            Command::new(javac())
                .args(["--release", "25", "-cp"])
                .arg(&gson_path)
                .args(["-d"])
                .arg(&classes)
                .arg(&source)
                .status()
                .unwrap()
                .success(),
            "javac failed for the real Gson TypeAdapter probe"
        );

        let jar_path = root.join("probe.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"gson-type-adapter","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for class in files_under(&classes) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();

        Self {
            root,
            probe: NativeJar::from_path(&jar_path).unwrap(),
            gson: NativeJar::from_path(&gson_path).unwrap(),
        }
    }

    fn call(&self, method: &str) -> (String, Vec<String>) {
        let mut host = self.probe.seed_host(StdHost::new());
        host.link_jar("gson", &self.gson).unwrap();
        let value = self
            .probe
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{method} failed: {error}; unknown={:?}",
                    host.unknown_host_calls()
                )
            });
        let text = match value {
            Value::Ref(Some(text)) => text,
            Value::Ref(None) => "null".into(),
            other => panic!(
                "{method} did not return a string: {other:?}; unknown={:?}",
                host.unknown_host_calls()
            ),
        };
        let unknown = host
            .unknown_host_calls()
            .into_iter()
            .map(|(call, _)| call)
            .collect();
        (text, unknown)
    }
}

fn oracle(name: &str) -> &'static str {
    include_str!("fixtures/gson_type_adapter/java25.txt")
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no Java 25 row for {name}"))
}

#[test]
fn modeled_type_adapter_rows_match_real_gson() {
    let fixture = Fixture::new();
    for name in ["normal", "exact", "nested", "isolation", "exactVsHierarchy"] {
        let (actual, unknown) = fixture.call(name);
        assert_eq!(actual, oracle(name), "{name}");
        assert!(
            unknown.is_empty(),
            "{name} counted unknown calls: {unknown:?}"
        );
    }
}

#[test]
fn unsupported_type_adapter_rows_stay_counted() {
    let fixture = Fixture::new();
    for name in [
        "nullSafe",
        "hierarchyVsExact",
        "factoryAB",
        "factoryBA",
        "lambdaSerializer",
        "lambdaDeserializer",
    ] {
        let (actual, unknown) = fixture.call(name);
        assert!(
            actual == "null" || actual == "COUNTED",
            "{name} unexpectedly answered {actual:?}"
        );
        assert!(!unknown.is_empty(), "{name} did not count an unknown call");
        if name != "nullSafe" {
            assert!(
                unknown
                    .iter()
                    .any(|call| call.starts_with("com/google/gson/Gson#")),
                "{name} did not count a Gson operation: {unknown:?}"
            );
        }
    }
}

#[test]
fn java25_capture_has_all_probe_rows() {
    for name in [
        "normal",
        "exact",
        "nested",
        "isolation",
        "nullSafe",
        "hierarchyVsExact",
        "factoryAB",
        "factoryBA",
        "lambdaSerializer",
        "lambdaDeserializer",
    ] {
        assert!(!oracle(name).is_empty(), "{name}");
    }
}
