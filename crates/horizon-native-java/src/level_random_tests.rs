//! `Level.getRandom()` on a lent level stream (`level_random.rs`), against Java 25.
//!
//! `tests/fixtures/level_random/demo/LevelRandoms.java` draws every mapped `RandomSource` method
//! in a fixed order; `java25.txt` is the same demo on a real 26.3 `LegacyRandomSource` (see the
//! fixture's README). Here it runs through Decaf with a lent stream that is vanilla's 48-bit
//! LCG, as Sunrise's `LevelRandom` is.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::{DRAW_AFTER_DEFERRED_EFFECT, LevelRandomStream, NativeJar, StdHost, Value};

/// The oracle's seed (`java25.txt`).
const SEED: i64 = 6_301_295_841;
const OVERWORLD: &str = "minecraft:overworld";
const LEVEL: &str = "Lnet/minecraft/world/level/Level;";

/// `new LegacyRandomSource(seed)`: `java.util.Random`'s LCG, and `MarsagliaPolarGaussian`'s
/// pairs (`Math.log`, `Math.sqrt`).
struct Legacy {
    seed: u64,
    gaussian: Option<f64>,
}

impl Legacy {
    fn new(seed: i64) -> Self {
        Self {
            seed: (seed as u64 ^ 0x5_deec_e66d) & ((1 << 48) - 1),
            gaussian: None,
        }
    }
    fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(0x5_deec_e66d).wrapping_add(11) & ((1 << 48) - 1);
        (self.seed >> (48 - bits)) as i64 as i32
    }
}

impl LevelRandomStream for Legacy {
    fn next_int(&mut self) -> i32 {
        self.next(32)
    }
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "Decaf asked the stream for nextInt({bound})");
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }
    fn next_long(&mut self) -> i64 {
        (i64::from(self.next(32)) << 32).wrapping_add(i64::from(self.next(32)))
    }
    fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }
    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * 5.960_464_5e-8
    }
    fn next_double(&mut self) -> f64 {
        let high = i64::from(self.next(26));
        let low = i64::from(self.next(27));
        ((high << 27) + low) as f64 * 1.110_223_024_625_156_5e-16
    }
    fn next_gaussian(&mut self) -> f64 {
        if let Some(next) = self.gaussian.take() {
            return next;
        }
        loop {
            let d = 2.0 * self.next_double() - 1.0;
            let e = 2.0 * self.next_double() - 1.0;
            let f = d * d + e * e;
            if f < 1.0 && f != 0.0 {
                let g = (-2.0 * crate::fdlibm::log(f) / f).sqrt();
                self.gaussian = Some(e * g);
                return d * g;
            }
        }
    }
}

fn fixture_dir() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures/level_random"),
        manifest.join("../crates/horizon-native-java/tests/fixtures/level_random"),
    ]
    .into_iter()
    .find(|dir| dir.join("demo/LevelRandoms.java").is_file())
    .expect("level random fixture directory")
}

fn java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            java_files(&path, out)
        } else if path.extension().is_some_and(|e| e == "java") {
            out.push(path)
        }
    }
}

/// The demo, compiled with `--release 21` against the compile-only stubs.
fn jar() -> &'static NativeJar {
    static JAR: OnceLock<NativeJar> = OnceLock::new();
    JAR.get_or_init(|| {
        use std::io::Write;
        let fixtures = fixture_dir();
        let scratch =
            std::env::temp_dir().join(format!("horizon-level-random-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let classes = scratch.join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        let mut sources = Vec::new();
        java_files(&fixtures.join("stubs"), &mut sources);
        java_files(&fixtures.join("demo"), &mut sources);
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        assert!(
            Command::new(javac)
                .args(["--release", "21", "-nowarn", "-d"])
                .arg(&classes)
                .args(&sources)
                .status()
                .unwrap()
                .success()
        );
        let mut bytes = Vec::new();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        let mut files: Vec<PathBuf> = std::fs::read_dir(classes.join("demo"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        files.sort();
        for path in files {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            zip.start_file(format!("demo/{name}"), options).unwrap();
            zip.write_all(&std::fs::read(path).unwrap()).unwrap();
        }
        zip.finish().unwrap();
        let _ = std::fs::remove_dir_all(&scratch);
        NativeJar::from_bytes(&bytes).unwrap()
    })
}

/// `demo.LevelRandoms.<method>(Level)` on the overworld's server level.
fn call(host: &mut StdHost, method: &str, descriptor: &str) -> Result<Value, crate::NativeError> {
    jar().invoke_method_with_args(
        "demo/LevelRandoms",
        method,
        descriptor,
        None,
        &[StdHost::server_level_handle(OVERWORLD)],
        host,
    )
}

/// [`call`] with the overworld's stream lent.
fn lent(
    host: &mut StdHost,
    stream: &mut Legacy,
    method: &str,
    descriptor: &str,
) -> Result<Value, crate::NativeError> {
    host.with_lent_level_random(OVERWORLD, stream, |host| call(host, method, descriptor))
}

#[test]
fn a_lent_level_stream_answers_every_mapped_draw_as_java_25() {
    let expected = std::fs::read_to_string(fixture_dir().join("java25.txt")).unwrap();
    let mut host = StdHost::new();
    let mut stream = Legacy::new(SEED);
    let answer = lent(
        &mut host,
        &mut stream,
        "run",
        &format!("({LEVEL})Ljava/lang/String;"),
    );
    assert!(
        matches!(&answer, Ok(Value::Ref(Some(text))) if text == expected.trim_end()),
        "{answer:?}\nexpected {}",
        expected.trim_end()
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
    // The stream is where Java's is after the demo: the next draw is the next value.
    let mut oracle = Legacy::new(SEED);
    let mut replay = StdHost::new();
    let _ = lent(
        &mut replay,
        &mut oracle,
        "run",
        &format!("({LEVEL})Ljava/lang/String;"),
    );
    assert_eq!(stream.next_long(), oracle.next_long());
}

#[test]
fn without_a_lend_get_random_stays_counted() {
    let mut host = StdHost::new();
    // `kept = level.getRandom()` answers the neutral null; `kept.nextInt()` then throws.
    let answer = call(&mut host, "keep", &format!("({LEVEL})I"));
    assert!(answer.is_err(), "{answer:?}");
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "net/minecraft/world/level/Level#getRandom()Lnet/minecraft/util/RandomSource;".into(),
            1
        )]
    );
    // A lend of another level answers nothing for this one either.
    let mut host = StdHost::new();
    let mut stream = Legacy::new(SEED);
    let answer = host.with_lent_level_random("minecraft:the_nether", &mut stream, |host| {
        call(host, "keep", &format!("({LEVEL})I"))
    });
    assert!(answer.is_err(), "{answer:?}");
    assert_eq!(
        host.unknown_host_calls(),
        vec![(
            "net/minecraft/world/level/Level#getRandom()Lnet/minecraft/util/RandomSource;".into(),
            1
        )]
    );
    assert_eq!(
        stream.next_long(),
        Legacy::new(SEED).next_long(),
        "nothing drew"
    );
}

#[test]
fn a_kept_handle_draws_only_inside_a_lend_and_stays_the_same_object() {
    let mut host = StdHost::new();
    let mut stream = Legacy::new(SEED);
    let mut oracle = Legacy::new(SEED);
    let first = lent(&mut host, &mut stream, "keep", &format!("({LEVEL})I"));
    assert_eq!(first.ok(), Some(Value::Int(oracle.next_int())));
    // Outside the lend nothing answers the kept handle: counted, and the stream doesn't move.
    let outside =
        jar().invoke_method_with_args("demo/LevelRandoms", "drawKept", "()I", None, &[], &mut host);
    assert_eq!(outside.ok(), Some(Value::Int(0)));
    assert_eq!(
        host.unknown_host_calls(),
        vec![("net/minecraft/util/RandomSource#nextInt()I".into(), 1)]
    );
    // In the next lend, `getRandom()` is the object Java kept, and it draws again.
    let same = lent(
        &mut host,
        &mut stream,
        "keptIsCurrent",
        &format!("({LEVEL})Z"),
    );
    assert_eq!(same.ok(), Some(Value::Int(1)));
    let again = host.with_lent_level_random(OVERWORLD, &mut stream, |host| {
        jar().invoke_method_with_args("demo/LevelRandoms", "drawKept", "()I", None, &[], host)
    });
    assert_eq!(again.ok(), Some(Value::Int(oracle.next_int())));
    assert_eq!(
        host.unknown_host_calls().len(),
        1,
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn set_seed_on_the_level_stream_is_counted_and_reseeds_nothing() {
    let mut host = StdHost::new();
    let mut stream = Legacy::new(SEED);
    let answer = lent(&mut host, &mut stream, "reseed", &format!("({LEVEL})I"));
    // Not `new LegacyRandomSource(42).nextInt()`: the live stream's own next value.
    assert_eq!(answer.ok(), Some(Value::Int(Legacy::new(SEED).next_int())));
    assert_eq!(
        host.unknown_host_calls(),
        vec![("net/minecraft/util/RandomSource#setSeed(J)V".into(), 1)]
    );
}

/// The choke point every recorded effect passes (`StdHost::note_deferred_effect`): an
/// allow-listed effect leaves later draws exact; any other makes every later lent draw
/// counted, still answered by the live stream. A draw before the effect stays exact.
#[test]
fn a_draw_after_a_deferred_effect_that_may_draw_the_stream_is_counted() {
    let mut host = StdHost::new();
    let mut stream = Legacy::new(SEED);
    let mut oracle = Legacy::new(SEED);
    let values = host.with_lent_level_random(OVERWORLD, &mut stream, |host| {
        let before = call(host, "keep", &format!("({LEVEL})I"));
        host.note_deferred_effect(true);
        let quiet =
            jar().invoke_method_with_args("demo/LevelRandoms", "drawKept", "()I", None, &[], host);
        let counted_before = host.unknown_host_call_count();
        host.note_deferred_effect(false);
        let after =
            jar().invoke_method_with_args("demo/LevelRandoms", "drawKept", "()I", None, &[], host);
        (before, quiet, counted_before, after)
    });
    assert_eq!(values.0.ok(), Some(Value::Int(oracle.next_int())));
    assert_eq!(values.1.ok(), Some(Value::Int(oracle.next_int())));
    assert_eq!(values.2, 0);
    assert_eq!(values.3.ok(), Some(Value::Int(oracle.next_int())));
    assert_eq!(
        host.unknown_host_calls(),
        vec![(DRAW_AFTER_DEFERRED_EFFECT.into(), 1)]
    );
    // The mark belonged to that lend: the next one starts exact.
    let next = host.with_lent_level_random(OVERWORLD, &mut stream, |host| {
        jar().invoke_method_with_args("demo/LevelRandoms", "drawKept", "()I", None, &[], host)
    });
    assert_eq!(next.ok(), Some(Value::Int(oracle.next_int())));
    assert_eq!(host.unknown_host_call_count(), 1);
}

/// A lend that unwinds puts the host's lend back too: an enclosing lend keeps answering.
#[test]
fn an_unwinding_inner_lend_restores_the_outer_one() {
    let mut host = StdHost::new();
    let mut outer = Legacy::new(SEED);
    let mut oracle = Legacy::new(SEED);
    let value = host.with_lent_level_random(OVERWORLD, &mut outer, |host| {
        let mut inner = Legacy::new(7);
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            host.with_lent_level_random(OVERWORLD, &mut inner, |_| panic!("inner call panicked"))
        }));
        assert!(unwound.is_err());
        call(host, "keep", &format!("({LEVEL})I"))
    });
    assert_eq!(value.ok(), Some(Value::Int(oracle.next_int())));
    assert!(host.unknown_host_calls().is_empty());
}
