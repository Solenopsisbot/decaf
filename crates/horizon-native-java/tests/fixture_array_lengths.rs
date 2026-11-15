//! Java array creation keeps the exact computed length from Java 25.
//!
//! The first case is EntityCulling's `ArrayOcclusionCache` constructor:
//! `reach=128` gives `(reach * 2)^3 / 4 == 4_194_304`, a valid byte-array
//! length. The remaining cases pin zero and partial `multianewarray`
//! dimensions and Java's negative-size exception message.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{NativeError, StdHost, Value};

const PROBE: &str = r#"package demo;

import java.util.function.Supplier;

public final class Probe {
    static String attempt(Supplier<String> s) {
        try { return "ok:" + s.get(); }
        catch (Throwable e) { return e.getClass().getName() + ":" + e.getMessage(); }
    }

    public static String computed() {
        int reach = 128;
        int reachX2 = reach * 2;
        int length = reachX2 * reachX2 * reachX2 / 4;
        byte[] cache = new byte[length];
        return length + ":" + cache.length + ":" + cache[0] + ":" + cache[length - 1];
    }

    public static String entityCullingBounded() {
        int reach = 256;
        int reachX2 = reach * 2;
        int length = reachX2 * reachX2 * reachX2 / 4;
        byte[] cache = new byte[length];
        return reach + ":" + reachX2 + ":" + length + ":" + cache.length;
    }

    public static String atBound() {
        byte[] data = new byte[1 << 24];
        return data.length + ":" + data[0] + ":" + data[data.length - 1];
    }

    public static String overBound() {
        try { byte[] ignored = new byte[(1 << 24) + 1]; return "caught"; }
        catch (Throwable t) { return "caught:" + t.getClass().getName(); }
    }

    public static String multidimOverBound() {
        try { int[][] ignored = new int[65536][65536]; return "caught"; }
        catch (Throwable t) { return "caught:" + t.getClass().getName(); }
    }

    public static String manyArrays() {
        try { int[][] ignored = new int[1 << 21][1]; return "caught"; }
        catch (Throwable t) { return "caught:" + t.getClass().getName(); }
    }

    public static String dimensions() {
        int[][][] zero = new int[0][7][9];
        int[][][] partial = new int[2][3][];
        int[][][] leafZero = new int[2][3][0];
        int[][][] innerZero = new int[2][0][];
        return zero.length + ":" + partial.length + ":" + partial[0].length + ":" + (partial[0][0] == null)
            + ":" + leafZero[1][2].length + ":" + innerZero[1].length + ":" + (innerZero[1] != null);
    }

    public static String negatives() {
        return attempt(() -> String.valueOf(new int[-1].length)) + "|"
            + attempt(() -> String.valueOf(new int[2][-3].length)) + "|"
            + attempt(() -> String.valueOf(new int[0][-3].length)) + "|"
            + attempt(() -> String.valueOf(new int[2][3][-4].length));
    }

    public static String zeroAndEmpty() {
        int[] a = new int[0];
        String[] b = new String[0];
        return a.length + ":" + b.length + ":" + java.lang.reflect.Array.getLength(
            java.lang.reflect.Array.newInstance(byte.class, 0));
    }
}
"#;

#[test]
fn array_lengths_match_java_25() {
    let fixture = javac_fixture::Fixture::build("array-lengths", &[("demo/Probe.java", PROBE)]);
    let cases = [
        ("atBound", "16777216:0:0"),
        ("computed", "4194304:4194304:0:0"),
        ("dimensions", "0:2:3:true:0:0:true"),
        (
            "negatives",
            "java.lang.NegativeArraySizeException:-1|java.lang.NegativeArraySizeException:-3|java.lang.NegativeArraySizeException:-3|java.lang.NegativeArraySizeException:-4",
        ),
        ("zeroAndEmpty", "0:0:0"),
    ];
    let mut failures = Vec::new();
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let text = match &answer {
            Ok(Value::Ref(Some(text))) => text.clone(),
            other => format!("{other:?}"),
        };
        let unknown = host.unknown_host_calls();
        if text != expected || !unknown.is_empty() {
            failures.push(format!(
                "{method}\n  jdk 25: {expected}\n  native: {text}\n  unknown: {unknown:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "differ from JDK 25:\n{}",
        failures.join("\n")
    );

    // Java 25 allocates EntityCulling's reach=256 cache (33,554,432 bytes),
    // but that would require roughly 1 GiB of Value slots in this host. The
    // runtime must refuse it with the named bound, never return a shortened or
    // otherwise guessed result.
    let mut host = fixture.jar.seed_host(StdHost::new());
    let result = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "entityCullingBounded",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    match result {
        Err(NativeError::Metadata(message)) => {
            assert!(
                message.starts_with(
                    "array of 33554432 elements exceeds the runtime's bound of 16777216"
                ),
                "unexpected bound refusal: {message}"
            );
        }
        other => panic!("EntityCulling Java-allocatable cache must be counted: {other:?}"),
    }
    assert!(
        host.unknown_host_calls().is_empty(),
        "EntityCulling array refusal counted a host call"
    );

    for method in ["overBound", "multidimOverBound", "manyArrays"] {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let result = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let expected = match method {
            "overBound" => "array of 16777217 elements exceeds the runtime's bound of 16777216",
            "multidimOverBound" => {
                "array of 4295032832 elements exceeds the runtime's bound of 16777216"
            }
            "manyArrays" => {
                "multianewarray of 2097153 arrays exceeds the runtime's bound of 1048576 arrays"
            }
            _ => unreachable!(),
        };
        match result {
            Err(NativeError::Metadata(message)) => {
                assert!(message.starts_with(expected), "{method}: {message}");
            }
            other => {
                panic!("{method} should refuse without entering catch(Throwable), got {other:?}")
            }
        }
        assert!(
            host.unknown_host_calls().is_empty(),
            "{method} counted a host call"
        );
    }
}
