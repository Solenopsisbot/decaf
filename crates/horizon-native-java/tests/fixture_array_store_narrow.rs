//! `castore`, `sastore` and `bastore` narrow the int they store (JVMS 6.5): the low 16 bits (zero-extended
//! for `char[]`, sign-extended for `short[]`), the low 8 bits for `byte[]` (sign-extended on load), the low
//! bit for `boolean[]`. javac always narrows first (`i2c`, `i2s`, `i2b`), so the fixture patches those
//! instructions of `demo/Raw` into nops and the stores receive the wide int, as a hand-written class file or
//! generated code can pass it. `java25.txt` is Java 25's output for the same patched class (one
//! `name=values` line per method; see `fixtures/array_store_narrow/README.md`, which also gives the commands).
//! The byte edits below are the ones in `Run.java`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

const ORACLE: &str = include_str!("fixtures/array_store_narrow/java25.txt");
const PROBE: &str = include_str!("fixtures/array_store_narrow/demo/Probe.java");
const RAW: &str = include_str!("fixtures/array_store_narrow/demo/Raw.java");

/// Replace every `[first, second]` pair in `bytes` by `[to, second]`; returns how many there were.
fn replace(bytes: &mut [u8], first: u8, second: u8, to: u8) -> usize {
    let mut count = 0;
    for at in 0..bytes.len() - 1 {
        if bytes[at] == first && bytes[at + 1] == second {
            bytes[at] = to;
            count += 1;
        }
    }
    count
}

/// `i2c castore`, `i2s sastore` and `i2b bastore` become `nop <store>`; `boolStore`'s `newarray byte` (the
/// second in the class file, after `byteStore`'s) becomes `newarray boolean`.
fn patch_raw(bytes: &mut [u8]) {
    assert_eq!(replace(bytes, 0x92, 0x55, 0x00), 1, "i2c castore");
    assert_eq!(replace(bytes, 0x93, 0x56, 0x00), 1, "i2s sastore");
    assert_eq!(replace(bytes, 0x91, 0x54, 0x00), 2, "i2b bastore");
    let mut seen = 0;
    for at in 0..bytes.len() - 1 {
        if bytes[at] == 0xbc && bytes[at + 1] == 8 {
            seen += 1;
            if seen == 2 {
                bytes[at + 1] = 4;
            }
        }
    }
    assert_eq!(seen, 2, "newarray byte");
}

#[test]
fn array_stores_narrow_as_java25() {
    let fixture = javac_fixture::Fixture::build_with_patches(
        "array-store-narrow",
        &[("demo/Probe.java", PROBE), ("demo/Raw.java", RAW)],
        &[("demo/Raw.class", patch_raw)],
    );
    let mut mismatches = Vec::new();
    let mut checked = 0;
    for line in ORACLE.lines() {
        let (method, expected) = line.split_once('=').unwrap();
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        let got = fixture.text_on(&mut host, method);
        if got != expected {
            mismatches.push(format!("{method}\n  java25: {expected}\n  ours:   {got}"));
        }
        assert!(
            host.unknown_host_calls().is_empty(),
            "{method}: unmodelled host calls {:?}",
            host.unknown_host_calls()
        );
        checked += 1;
    }
    assert_eq!(checked, 4, "every probe method has an oracle line");
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
