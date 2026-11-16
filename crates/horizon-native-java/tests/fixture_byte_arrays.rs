//! `byte[]` end to end on a compact backing (#274).
//!
//! The runtime kept every array as `Value`s (32 bytes each), so EntityCulling's
//! `new byte[4 << 20]` cache held 128 MiB. A `byte[]` now keeps one byte per element
//! (`array_data.rs`). These are the paths the runtime models on one, each as Java 25 answers it:
//! `baload` sign extension, `arraylength`, the bounds exception, `System.arraycopy` within one
//! array in both directions and its refusals (kinds, `null`, bounds), `Arrays.copyOf`/
//! `copyOfRange`, `clone`, a 4 MiB array written and read by index and copied, and a `byte[][]`
//! (`multianewarray`'s byte leaves). Nothing is counted.
//!
//! The probe prints, compares, hashes and fills its arrays with loops: `Arrays.toString`/`equals`/
//! `hashCode`/`fill` on a `byte[]` are counted on main and on this branch alike, a later slice.
//!
//! `fixtures/byte_arrays/demo/Probe.java` runs unchanged on both sides; its `main` on Java 25
//! (Temurin 25.0.5, `javac --release 21`) printed `java25.txt`, the same on two runs. `bastore`'s
//! narrowing of an `int` javac did not narrow is `lib.rs`'s raw-bytecode test.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;

const ORACLE: &str = include_str!("fixtures/byte_arrays/java25.txt");

const RAW_BOOLEAN: &str = r#"package demo;

public final class RawBoolean {
    public static int raw(int value) {
        byte[] values = new byte[1];
        values[0] = (byte) value;
        return values[0];
    }
}
"#;

/// Turn javac's checked byte store into the shared JVM `bastore` with an
/// unnarrowed int on the stack, and change its newarray operand from byte to
/// boolean. The resulting method is the review's hand-written bytecode case.
fn patch_raw_boolean(bytes: &mut [u8]) {
    let newarray = bytes
        .windows(2)
        .position(|window| window == [0xbc, 8])
        .expect("RawBoolean newarray byte opcode");
    bytes[newarray + 1] = 4;
    let store = bytes
        .windows(2)
        .position(|window| window == [0x91, 0x54])
        .expect("RawBoolean i2b/bastore sequence");
    bytes[store] = 0;
}

fn expected(method: &str) -> &'static str {
    ORACLE
        .lines()
        .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no oracle line for {method}"))
}

#[test]
fn byte_arrays_answer_as_java_25() {
    let fixture = javac_fixture::Fixture::build_with_patches(
        "byte-arrays",
        &[
            (
                "demo/Probe.java",
                include_str!("fixtures/byte_arrays/demo/Probe.java"),
            ),
            ("demo/RawBoolean.java", RAW_BOOLEAN),
        ],
        &[("demo/RawBoolean.class", patch_raw_boolean)],
    );
    let mut failures = Vec::new();
    for method in [
        "basics",
        "overlap",
        "copyErrors",
        "copies",
        "big",
        "grid",
        "reflection",
        "rawBoolean",
        "reflectionMatrix",
    ] {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let text = fixture.text_on(&mut host, method);
        let unknown = host.unknown_host_calls();
        if text != expected(method) || !unknown.is_empty() {
            failures.push(format!(
                "{method}\n  java:   {}\n  native: {text}\n  unknown: {unknown:?}",
                expected(method)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
