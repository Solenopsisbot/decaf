//! Every object a jar can see takes its number from the heap's one allocator (#669): no host
//! model mints a key or serial from a counter of its own, or from a truncated copy of the
//! heap's.
//!
//! The probes are javac-compiled at test time from `tests/fixtures/<dir>` (only `demo/` classes
//! ship in the jar, so a stubbed library's calls reach the runtime's models). Each expected
//! string is what the same `demo` class printed on Java 25 (OpenJDK 25.0.5), kept in that
//! fixture's `java25.txt` or in the fixture test that already checks it.
//!
//! These run in-crate because the failure needs a heap whose allocator is already past an
//! `i32`, as every game hook leaves it ([`crate::game_hooks::GAME_HOOK_OBJECT_BASE`]), and only
//! the crate can set that directly.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::game_hooks::GAME_HOOK_OBJECT_BASE;
use crate::{NativeJar, StdHost, Value};

/// `tests/fixtures/<path>`, from this crate or from the mod that vendors it.
fn fixture(path: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures").join(path),
        manifest
            .join("../crates/horizon-native-java/tests/fixtures")
            .join(path),
    ]
    .into_iter()
    .find(|path| path.exists())
    .unwrap_or_else(|| panic!("tests/fixtures/{path} is in crates/horizon-native-java"))
}

fn java_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            java_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "java") {
            out.push(path);
        }
    }
}

fn class_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            class_files(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "class") {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((name, path));
        }
    }
}

/// Compile `sources` (each relative to `tests/fixtures`: a `.java` file, or a directory whose
/// every `.java` file is taken) with `javac --release 21` and pack the `demo/` classes as a
/// Fabric jar.
fn compile(name: &str, sources: &[&str]) -> NativeJar {
    use std::io::Write;
    let mut files = Vec::new();
    for source in sources {
        let path = fixture(source);
        if path.is_dir() {
            java_files(&path, &mut files);
        } else {
            files.push(path);
        }
    }
    let scratch = std::env::temp_dir().join(format!(
        "horizon-native-java-host-minted-keys-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let classes = scratch.join("classes");
    std::fs::create_dir_all(&classes).unwrap();
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "21", "-nowarn", "-d"])
        .arg(&classes)
        .args(&files)
        .status()
        .expect("javac runs");
    assert!(status.success(), "javac failed on the {name} probe");
    let mut compiled = Vec::new();
    class_files(&classes, &classes, &mut compiled);
    let mut bytes = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("fabric.mod.json", options).unwrap();
        zip.write_all(
            br#"{"schemaVersion":1,"id":"host_minted_keys","version":"1","entrypoints":{"main":[]}}"#,
        )
        .unwrap();
        for (entry, path) in compiled {
            if entry.starts_with("demo/") {
                zip.start_file(entry, options).unwrap();
                zip.write_all(&std::fs::read(path).unwrap()).unwrap();
            }
        }
        zip.finish().unwrap();
    }
    let _ = std::fs::remove_dir_all(&scratch);
    NativeJar::from_bytes(&bytes).expect("the probe jar parses")
}

/// A host whose heap has numbered past every `i32`, as one game hook leaves it.
fn after_a_game_hook(jar: &NativeJar) -> StdHost {
    let mut host = jar.seed_host(StdHost::new());
    host.heap.next_object = GAME_HOOK_OBJECT_BASE;
    host
}

/// `String <class>.<method>()`: its answer and the calls it counted.
fn run(
    jar: &NativeJar,
    host: &mut StdHost,
    class: &str,
    method: &str,
) -> (String, Vec<(String, usize)>) {
    let value = jar
        .invoke_method(class, method, "()Ljava/lang/String;", None, host)
        .unwrap_or_else(|error| panic!("{class}.{method} failed: {error}"));
    let Value::Ref(Some(text)) = value else {
        panic!("{class}.{method} answered {value:?}");
    };
    (text, host.unknown_host_calls())
}

// ---------------------------------------------------------------------------------------------
// config_serial (`client_config.rs`)

fn config_jar() -> &'static NativeJar {
    static JAR: OnceLock<NativeJar> = OnceLock::new();
    JAR.get_or_init(|| {
        compile(
            "configs",
            &["host_values/p7/stubs", "host_values/p7/demo/Configs.java"],
        )
    })
}

/// Two YACL builders and two configlib builders made after a game hook are two objects, as on
/// Java 25: `fixture_host_values_p7`'s rows for the same methods, captured against the real
/// libraries (yet_another_config_lib_v3 3.9.7, supermartijn642configlib 1.1.8a). Before #669 the
/// serial was `i32::try_from(next_object).unwrap_or(i32::MAX)`: past `1 << 40` every builder's
/// serial was `i32::MAX`, one key for all, so `b == c` and `b.equals(c)` answered `true` and a
/// `HashSet` of two held one.
#[test]
fn config_objects_made_after_a_game_hook_are_distinct() {
    let jar = config_jar();
    for (method, java25) in [
        ("yaclTwoBuilders", "falsefalse"),
        ("yaclBuildersInCollections", "22false2"),
        ("configlibTwoBuilders", "falsefalse"),
    ] {
        let mut host = after_a_game_hook(jar);
        let (answer, counted) = run(jar, &mut host, "demo/Configs", method);
        assert_eq!(answer, java25, "{method}");
        assert!(counted.is_empty(), "{method}: {counted:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// EnumSet.allOf (`library_dispatch.rs`)

/// `EnumSet.allOf` answers `values()`, in its order, and an empty enum's empty set; a failing
/// static initializer's `ExceptionInInitializerError` (then `NoClassDefFoundError`) comes out of
/// it as on Java 25 (`tests/fixtures/host_minted_keys/enum_set/java25.txt`, a JDK 25.0.5 run of
/// the same class). Before #669 a failing or empty `values()` was swallowed and the set made up
/// from the enum's static fields: `[null]` for an empty enum with a static field of its own
/// type, and `<enum>#<ordinal>` handles, numbers the heap hands out to other objects, for a
/// broken one.
#[test]
fn enum_set_all_of_answers_values_or_fails_as_java() {
    let jar = compile("enum_set", &["host_minted_keys/enum_set"]);
    let oracle =
        std::fs::read_to_string(fixture("host_minted_keys/enum_set/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    let (answer, counted) = run(&jar, &mut host, "demo/EnumSetProbe", "run");
    assert_eq!(answer, oracle.trim_end());
    assert!(counted.is_empty(), "{counted:?}");
}

// ---------------------------------------------------------------------------------------------
// ServiceLoader serials (`service_loader.rs`)

/// The heap number a `ServiceLoader` handle's serial holds, however it is kept.
fn loader_serial(loader: &Value) -> usize {
    let Value::Host(handle) = loader else {
        panic!("{loader:?} is no loader");
    };
    match handle.values().unwrap().as_slice() {
        [_, Value::Int(serial)] => *serial as usize,
        [_, Value::Long(serial)] => *serial as usize,
        other => panic!("{other:?}"),
    }
}

/// A loader made after a game hook is not one made before it (Java 25:
/// `tests/fixtures/host_minted_keys/service_loader/java25.txt`). The second load is numbered
/// `1 << 40` past the first; before #669 the serial was kept as `serial as i32`, which wrapped it
/// onto the first loader's: one key, so `first == second` answered `true`.
#[test]
fn a_service_loader_after_a_game_hook_is_a_new_loader() {
    let jar = compile("service_loader", &["host_minted_keys/service_loader"]);
    let oracle =
        std::fs::read_to_string(fixture("host_minted_keys/service_loader/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    host.heap.next_object = 7;
    let first = jar
        .invoke_method("demo/ServiceKeys", "load", "()Ljava/lang/Object;", None, &mut host)
        .unwrap();
    host.heap.next_object = GAME_HOOK_OBJECT_BASE + loader_serial(&first);
    let answer = jar
        .invoke_method_with_args(
            "demo/ServiceKeys",
            "same",
            "(Ljava/lang/Object;)Ljava/lang/String;",
            None,
            &[first],
            &mut host,
        )
        .unwrap();
    assert_eq!(answer, Value::Ref(Some(oracle.trim_end().to_owned())));
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

// ---------------------------------------------------------------------------------------------
// `Field` copy numbers (`reflect_override.rs`)

/// Each `getDeclaredField` is a new `Field` copy numbered by the heap (Java 25:
/// `tests/fixtures/host_minted_keys/field_copies/java25.txt`). With one number left, the second
/// copy finds the numbers exhausted: counted by name and the call fails, as for every allocation
/// (#667). Before #669 the copy saturated at the mark, unnamed, and the second and third copies
/// shared it.
#[test]
fn field_copies_take_heap_numbers_and_count_running_out() {
    let jar = compile("field_copies", &["host_minted_keys/field_copies"]);
    let oracle =
        std::fs::read_to_string(fixture("host_minted_keys/field_copies/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    let (answer, counted) = run(&jar, &mut host, "demo/FieldCopies", "run");
    assert_eq!(answer, oracle.trim_end());
    assert!(counted.is_empty(), "{counted:?}");

    let mut host = jar.seed_host(StdHost::new());
    host.heap.next_object = usize::MAX - 1;
    let failed = jar
        .invoke_method("demo/FieldCopies", "run", "()Ljava/lang/String;", None, &mut host)
        .unwrap_err()
        .to_string();
    assert!(
        failed.contains("allocation numbers are exhausted"),
        "{failed}"
    );
    let counted = host.unknown_host_calls();
    assert!(
        counted.iter().any(|(name, _)| name
            == "java/lang/reflect/Field#<new>: allocation numbers exhausted"),
        "{counted:?}"
    );
}
