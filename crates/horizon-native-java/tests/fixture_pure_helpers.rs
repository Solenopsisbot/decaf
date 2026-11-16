#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const STUBS: &[(&str, &str)] = &[
    (
        "org/lwjgl/system/MathUtil.java",
        r#"package org.lwjgl.system; public final class MathUtil { public static boolean mathIsPoT(int x){throw new AssertionError();} public static int mathRoundPoT(int x){throw new AssertionError();} }"#,
    ),
    (
        "org/apache/logging/log4j/util/Strings.java",
        r#"package org.apache.logging.log4j.util; public final class Strings { public static boolean isEmpty(CharSequence x){throw new AssertionError();} public static boolean isNotEmpty(CharSequence x){throw new AssertionError();} public static boolean isBlank(String x){throw new AssertionError();} public static boolean isNotBlank(String x){throw new AssertionError();} }"#,
    ),
    (
        "org/apache/commons/compress/utils/Lists.java",
        r#"package org.apache.commons.compress.utils; public final class Lists { public static java.util.ArrayList<Object> newArrayList(){throw new AssertionError();} }"#,
    ),
];

#[test]
fn pure_helpers_match_java25_edge_values() {
    let probe = include_str!("fixtures/pure_helpers/Probe.java");
    let mut files = STUBS.to_vec();
    files.push(("demo/Probe.java", probe));
    let fixture = Fixture::build("pure-helpers", &files);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "bits"),
        "false|true|true|false|1|1|2|4|8|-2147483648"
    );
    assert_eq!(
        fixture.text_on(&mut host, "strings"),
        "true|true|false|false|true|true|true|false"
    );
    assert_eq!(
        fixture.text_on(&mut host, "whitespace"),
        "false|false|false|false|true|true|true|true|true|false"
    );
    let before = host.unknown_host_call_count();
    assert_eq!(fixture.text_on(&mut host, "nonString"), "false|false|0");
    assert_eq!(host.unknown_host_call_count() - before, 2);
    assert_eq!(fixture.text_on(&mut host, "list"), "true|1");
    assert_eq!(host.unknown_host_call_count(), 2);
}
