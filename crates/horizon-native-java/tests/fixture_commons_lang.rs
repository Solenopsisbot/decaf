//! Expected values were recorded from Java 25.0.5 with commons-lang3 3.20.0.
//! The compile-only stubs keep CI independent of the third-party jar.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const STRING_UTILS_STUB: &str = r#"package org.apache.commons.lang3;

public final class StringUtils {
    public static boolean isNotEmpty(CharSequence value) { throw new AssertionError(); }
    public static String capitalize(String value) { throw new AssertionError(); }
    public static String rightPad(String value, int width) { throw new AssertionError(); }
    public static String rightPad(String value, int width, char pad) { throw new AssertionError(); }
    public static String rightPad(String value, int width, String pad) { throw new AssertionError(); }
    public static String leftPad(String value, int width) { throw new AssertionError(); }
    public static String leftPad(String value, int width, char pad) { throw new AssertionError(); }
    public static String leftPad(String value, int width, String pad) { throw new AssertionError(); }
    public static String repeat(String value, int count) { throw new AssertionError(); }
    public static boolean containsIgnoreCase(CharSequence value, CharSequence search) { throw new AssertionError(); }
    public static <T extends CharSequence> T defaultIfBlank(T value, T fallback) { throw new AssertionError(); }
}
"#;

const VALIDATE_STUB: &str = r#"package org.apache.commons.lang3;

public final class Validate {
    public static void isTrue(boolean expression) { throw new AssertionError(); }
    public static void isTrue(boolean expression, String message, Object... values) {
        throw new AssertionError();
    }
}
"#;

const CONSUMERS_STUB: &str = r#"package org.apache.commons.lang3.function;

import java.util.function.Consumer;

public final class Consumers {
    public static <T> Consumer<T> nop() { throw new AssertionError(); }
}
"#;

const PROBE: &str = include_str!("fixtures/commons_lang/Probe.java");

#[test]
fn commons_lang_slice_two_matches_java25() {
    let fixture = Fixture::build(
        "commons-lang-slice-2",
        &[
            (
                "org/apache/commons/lang3/StringUtils.java",
                STRING_UTILS_STUB,
            ),
            ("org/apache/commons/lang3/Validate.java", VALIDATE_STUB),
            (
                "org/apache/commons/lang3/function/Consumers.java",
                CONSUMERS_STUB,
            ),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let before = host.unknown_host_call_count();
    let surrogate = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "surrogate",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert!(
        surrogate.is_err(),
        "surrogate unexpectedly answered: {surrogate:?}"
    );
    assert_eq!(host.unknown_host_call_count(), before + 1);
    assert!(host.unknown_host_calls().iter().any(|(call, _)| call == "org/apache/commons/lang3/StringUtils#rightPad(Ljava/lang/String;IC)Ljava/lang/String;"));
    let expected = include_str!("fixtures/commons_lang/oracle/java25.txt")
        .lines()
        .filter(|line| !line.starts_with("surrogate="))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let actual = [
        "capitalize",
        "pad",
        "unicode",
        "nullRules",
        "validate",
        "nopIdentity",
    ]
    .into_iter()
    .map(|name| format!("{name}={}", fixture.text_on(&mut host, name)))
    .collect::<Vec<_>>()
    .join(
        "
",
    ) + "
";
    assert_eq!(actual, expected);
    assert_eq!(
        host.unknown_host_call_count(),
        1,
        "{:?}",
        host.unknown_host_calls()
    );
}
