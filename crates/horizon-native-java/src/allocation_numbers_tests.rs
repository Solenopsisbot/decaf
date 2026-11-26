//! The heap's allocation numbers run out loudly (#667 review): a call that would number an
//! object past the last one fails by name, counted, and every later call is refused at entry,
//! instead of numbering two objects alike (`class#usize::MAX` again and again).
//!
//! The probe is `tests/fixtures/random_handle_keys/demo/Probe.java`, compiled once per process.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::{NativeJar, StdHost, Value};

/// `tests/fixtures/random_handle_keys/<file>`, from this crate or from the mod that vendors it.
fn fixture(file: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures/random_handle_keys"),
        manifest.join("../crates/horizon-native-java/tests/fixtures/random_handle_keys"),
    ]
    .into_iter()
    .map(|dir| dir.join(file))
    .find(|path| path.is_file())
    .expect("the probe is in crates/horizon-native-java/tests/fixtures/random_handle_keys")
}

fn jar() -> &'static NativeJar {
    static JAR: OnceLock<NativeJar> = OnceLock::new();
    JAR.get_or_init(|| {
        use std::io::Write;
        let source = fixture("demo/Probe.java");
        let scratch = std::env::temp_dir().join(format!(
            "horizon-native-java-allocation-numbers-{}",
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
            .arg(&source)
            .status()
            .expect("javac runs");
        assert!(status.success(), "javac failed on the random handle keys probe");
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("fabric.mod.json", options).unwrap();
            zip.write_all(
                br#"{"schemaVersion":1,"id":"allocation_numbers","version":"1","entrypoints":{"main":["demo.Probe"]}}"#,
            )
            .unwrap();
            zip.start_file("demo/Probe.class", options).unwrap();
            zip.write_all(&std::fs::read(classes.join("demo/Probe.class")).unwrap())
                .unwrap();
            zip.finish().unwrap();
        }
        let _ = std::fs::remove_dir_all(&scratch);
        NativeJar::from_bytes(&bytes).expect("the probe jar parses")
    })
}

fn run(host: &mut StdHost) -> Result<Value, String> {
    jar()
        .invoke_method("demo/Probe", "run", "()Ljava/lang/String;", None, host)
        .map_err(|error| error.to_string())
}

#[test]
fn a_heap_out_of_numbers_fails_the_call_and_every_later_one() {
    let mut host = jar().seed_host(StdHost::new());
    // One number left: the seeded `new Random(1L)` takes it, then nothing can be numbered.
    host.heap.next_object = usize::MAX - 1;
    let failed = run(&mut host).unwrap_err();
    assert!(
        failed.contains("allocation numbers are exhausted"),
        "{failed}"
    );
    let counted = host.unknown_host_calls();
    assert!(
        counted
            .iter()
            .any(|(name, _)| name.ends_with("#<new>: allocation numbers exhausted")),
        "{counted:?}"
    );
    // The heap kept the mark: the next call is refused before it runs.
    let again = run(&mut host).unwrap_err();
    assert!(
        again.contains("allocation numbers are exhausted"),
        "{again}"
    );
}

/// Below the mark, the same probe answers as Java 25 does (the fixture's oracle).
#[test]
fn a_heap_with_numbers_left_answers_as_java() {
    let mut host = jar().seed_host(StdHost::new());
    host.heap.next_object = usize::MAX - 1_000;
    let oracle = std::fs::read_to_string(fixture("java25.txt")).unwrap();
    assert_eq!(
        run(&mut host),
        Ok(Value::Ref(Some(oracle.trim_end().to_owned())))
    );
}
