//! `String#toUpperCase`/`toLowerCase` with and without a `Locale`, and
//! `Character#toUpperCase`/`toLowerCase` of a `char` or an `int`: the runtime
//! answers exactly Java 25's answer only where it can model it exactly --
//! ASCII receivers (any non-tailored `Locale` for the `Locale` overloads),
//! single code points for `Character` -- and counts every other case instead
//! of silently returning the wrong text.
//!
//! `demo/Probe` runs unchanged on both sides: `main` on Java 25 (its output
//! is `java25.txt`), the same static probes in Horizon.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{NativeError, StdHost, Value};

const ORACLE: &str = include_str!("fixtures/string_case_mapping/java25.txt");
/// The same probe with `-Duser.language=tr -Duser.country=TR`: the default
/// locale the no-argument forms use.
const ORACLE_TR: &str = include_str!("fixtures/string_case_mapping/java25_tr.txt");

const SOURCES: &[(&str, &str)] = &[(
    "demo/Probe.java",
    include_str!("fixtures/string_case_mapping/demo/Probe.java"),
)];

/// Rows the runtime answers exactly: the runtime's text must equal the oracle
/// line and nothing may be counted.
const ANSWERED: &[&str] = &[
    // No `Locale`: ASCII receivers fold the same in every locale.
    "emptyUpper",
    "emptyLower",
    "asciiLowerUpper",
    "asciiLowerLower",
    "asciiUpperUpper",
    "asciiUpperLower",
    // No `Locale`, ASCII without the letter a tailored default locale maps
    // differently (`I` lowercase, `i` uppercase).
    "asciiCapIUpper",
    "asciiSmallILower",
    "asciiNoIUpper",
    "asciiNoIILower",
    // The `Locale` overloads: ASCII with a non-tailored locale.
    "asciiRootLower",
    "asciiRootUpper",
    "asciiCapIRootLower",
    "asciiSmallIRootUpper",
    // `Character`: single code points, simple mapping.
    "charUpperSharp",
    "charLowerSharp",
    "charUpperE",
    "charLowerE",
    "charUpperAMacron",
    "charLowerAMacron",
    "charLowerSigma",
    "charUpperSigma",
    "charUpperIDotless",
    "charLowerIDot",
    "charUpperIDot",
    "charLowerIDotless",
    "charLowerKelvin",
    "charUpperRoman",
    "charLowerRoman",
    "intLowerKelvin",
    "intUpperDeseret",
    "intLowerDeseret",
    "intUpperKelvin",
    "intLowerRoman",
    "intUpperRoman",
];

/// Rows the runtime must not answer: each is recorded as an unknown host call
/// and never compared against the oracle.
const COUNTED: &[&str] = &[
    // No `Locale`: ASCII with the letter a tailored default locale maps
    // differently, which the runtime cannot know.
    "asciiCapILower",
    "asciiSmallIUpper",
    "asciiTitleLower",
    "asciiMiniUpper",
    // No `Locale`: anything outside ASCII.
    "l1EUpper",
    "l1ELower",
    "l1YenUpper",
    "l1YenLower",
    "l1SharpUpper",
    "l1SharpLower",
    "l1MicroUpper",
    "l1MicroLower",
    "leAMacronUpper",
    "leAMacronLower",
    "greekSigmaLower",
    "greekSigmaUpper",
    "turkishIDotUpper",
    "turkishIDotLower",
    "turkishIDotlessUpper",
    "turkishIDotlessLower",
    "kelvinUpper",
    "kelvinLower",
    "romanUpper",
    "romanLower",
    "deseretUpper",
    "deseretLower",
    "loneLowSurrogateUpper",
    "loneLowSurrogateLower",
    "loneHighSurrogateUpper",
    "loneHighSurrogateLower",
    // The `Locale` overloads: tailored locales (Turkish here), a null
    // locale, and every non-ASCII receiver.
    "asciiTrLower",
    "asciiTrUpper",
    "asciiCapITrLower",
    "asciiSmallITrUpper",
    "idotRootLower",
    "idotTrLower",
    "idotlessRootUpper",
    "idotlessTrUpper",
    "sharpRootUpper",
    // A null `Locale`: Java throws `NullPointerException`.
    "asciiNullLocaleUpper",
    "asciiNullLocaleLower",
];

/// The `Locale` overload rows among `COUNTED`.
const COUNTED_LOCALE: &[&str] = &[
    "asciiTrLower",
    "asciiTrUpper",
    "asciiCapITrLower",
    "asciiSmallITrUpper",
    "idotRootLower",
    "idotTrLower",
    "idotlessRootUpper",
    "idotlessTrUpper",
    "sharpRootUpper",
    "asciiNullLocaleUpper",
    "asciiNullLocaleLower",
];

fn expected(method: &str) -> &'static str {
    ORACLE
        .lines()
        .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no oracle line for {method}"))
}

struct Run {
    /// The text the row answered, if it answered text.
    text: Option<String>,
    /// A non-text result: the counted `String` a counted call leaves (a
    /// `String#n` jar-object marker), never a value Java could return here.
    marker: Option<Value>,
    unknown: Vec<(String, usize)>,
    error: Option<NativeError>,
}

/// Run one static `String demo.Probe#method()` on a fresh seeded host,
/// keeping what the run reported as unknown host calls.
fn run(fixture: &javac_fixture::Fixture, method: &str) -> Run {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let result = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        method,
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    let (text, marker) = match &result {
        Ok(Value::Ref(Some(text))) => (Some(text.clone()), None),
        Ok(marker @ Value::Host(_)) => (None, Some(marker.clone())),
        Ok(other) => panic!("{method} returned neither a string nor a marker: {other:?}"),
        Err(_) => (None, None),
    };
    Run {
        text,
        marker,
        unknown: host.unknown_host_calls(),
        error: result.err(),
    }
}

/// The rows whose receiver is a `String` constant over a lone surrogate:
/// loading it is counted (`String#ldc`, #447/#658) and yields a counted
/// `String`, so the case mapping and `row` see a counted marker, not text.
const LONE_SURROGATE: &[&str] = &[
    "loneLowSurrogateUpper",
    "loneLowSurrogateLower",
    "loneHighSurrogateUpper",
    "loneHighSurrogateLower",
];

/// The unknown-host-call key one row's case method is counted under.
fn counted_key(method: &str) -> String {
    let case = if method.ends_with("Upper") {
        "Upper"
    } else {
        "Lower"
    };
    let descriptor = if COUNTED_LOCALE.contains(&method) {
        "(Ljava/util/Locale;)Ljava/lang/String;"
    } else {
        "()Ljava/lang/String;"
    };
    format!("java/lang/String#to{case}Case{descriptor}")
}

#[test]
fn answered_case_mappings_match_java_25_with_nothing_counted() {
    let fixture = javac_fixture::Fixture::build("string-case-mapping", SOURCES);
    for method in ANSWERED {
        let run = run(&fixture, method);
        assert!(run.error.is_none(), "{method} failed: {:?}", run.error);
        assert_eq!(
            run.text.as_deref(),
            Some(expected(method)),
            "{method}: unknown calls {:#?}",
            run.unknown
        );
        assert!(
            run.unknown.is_empty(),
            "{method}: answered but counted {:#?}",
            run.unknown
        );
    }
}

/// A counted row never answers text. How the run then ends depends on what
/// the counted call left behind, and neither ending is Java's (every row
/// prints normally on Java 25, `java25.txt`):
///
/// - A text receiver (`"É".toUpperCase()`): the host's neutral result is
///   `null`, so `row` stops at `text.length()` and the run fails.
/// - A lone-surrogate constant (`"\uDC00".toUpperCase()`): the load itself is
///   counted and yields a counted `String` (#658), the case mapping of it is
///   counted, and `row` of a counted `String` is counted in turn (#423), so
///   the run ends with that counted marker. It must be the marker, never text.
#[test]
fn counted_case_mappings_are_never_answered() {
    let fixture = javac_fixture::Fixture::build("string-case-mapping", SOURCES);
    for method in COUNTED {
        let run = run(&fixture, method);
        assert!(
            run.text.is_none(),
            "{method}: answered {:?} although it must stay counted",
            run.text
        );
        let lone = LONE_SURROGATE.contains(method);
        if lone {
            assert!(
                run.marker.is_some(),
                "{method}: a counted String's case mapping must leave its counted marker, got {:?}",
                run.error
            );
            assert!(
                run.unknown
                    .iter()
                    .any(|(key, _)| key == "java/lang/String#ldc()Ljava/lang/String;"),
                "{method}: the lone-surrogate constant's load must be counted, got {:#?}",
                run.unknown
            );
        } else {
            assert!(
                run.error.is_some(),
                "{method}: returned {:?} instead of failing on the counted call's null",
                run.marker
            );
        }
        assert!(
            !run.unknown.is_empty(),
            "{method}: no unknown host calls although it must stay counted"
        );
        assert!(
            run.unknown
                .iter()
                .any(|(key, _)| key.as_str() == counted_key(method)),
            "{method}: expected a counted {expected} call, got {unknown:#?}",
            expected = counted_key(method),
            unknown = run.unknown
        );
    }
}

#[test]
fn every_oracle_row_is_accounted_for() {
    let covered: Vec<&str> = ANSWERED.iter().chain(COUNTED.iter()).copied().collect();
    let missing: Vec<&str> = ORACLE
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.split_once('=').unwrap_or((line, "")).0)
        .filter(|label| !covered.contains(label))
        .collect();
    assert!(
        missing.is_empty(),
        "oracle rows not covered by the fixture: {missing:?}"
    );
    assert_eq!(
        ANSWERED.len() + COUNTED.len(),
        ORACLE.lines().filter(|line| !line.is_empty()).count(),
        "fixture rows and oracle rows differ in number"
    );
}

/// The no-argument forms use the process default locale, which the runtime
/// does not know. Every row it answers has the same oracle line under the
/// `it_IT` default and under a Turkish default; the ASCII rows it counts for
/// that reason are exactly the ones whose oracle line differs.
#[test]
fn default_locale_rows_are_answered_only_where_the_default_cannot_matter() {
    let line = |oracle: &'static str, method: &str| -> &'static str {
        oracle
            .lines()
            .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
            .unwrap_or_else(|| panic!("no oracle line for {method}"))
    };
    for method in ANSWERED {
        assert_eq!(
            line(ORACLE, method),
            line(ORACLE_TR, method),
            "{method} is answered but the default locale changes it"
        );
    }
    let default_locale_ascii = [
        "asciiCapILower",
        "asciiSmallIUpper",
        "asciiTitleLower",
        "asciiMiniUpper",
    ];
    for method in default_locale_ascii {
        assert_ne!(
            line(ORACLE, method),
            line(ORACLE_TR, method),
            "{method} is counted but the default locale does not change it"
        );
        assert!(COUNTED.contains(&method), "{method} must be counted");
    }
}

/// A receiver that is a jar-object marker (`String#n`: an unmodelled
/// constructor left a reference instead of text) is not text: every case
/// mapping of it is counted, never folded.
#[test]
fn case_mapping_of_a_marker_receiver_stays_counted() {
    let source = r#"package demo;
public final class Probe {
    static String marker() {
        // `new String(char[])` with a lone surrogate is not modelled: the
        // constructor stays counted and the receiver is a marker.
        return new String(new char[] {'A', '\uD83D', 'C'});
    }
    public static String lower() { return marker().toLowerCase(); }
    public static String upper() { return marker().toUpperCase(); }
    public static String lowerRoot() { return marker().toLowerCase(java.util.Locale.ROOT); }
    public static String upperRoot() { return marker().toUpperCase(java.util.Locale.ROOT); }
}"#;
    let fixture =
        javac_fixture::Fixture::build("string-case-marker", &[("demo/Probe.java", source)]);
    for (method, key) in [
        ("lower", "java/lang/String#toLowerCase()Ljava/lang/String;"),
        ("upper", "java/lang/String#toUpperCase()Ljava/lang/String;"),
        (
            "lowerRoot",
            "java/lang/String#toLowerCase(Ljava/util/Locale;)Ljava/lang/String;",
        ),
        (
            "upperRoot",
            "java/lang/String#toUpperCase(Ljava/util/Locale;)Ljava/lang/String;",
        ),
    ] {
        // A counted call leaves a marker (or fails), never text.
        let mut host = fixture.jar.seed_host(StdHost::new());
        let result = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        assert!(
            !matches!(&result, Ok(Value::Ref(Some(_)))),
            "{method}: answered {result:?} for a marker receiver"
        );
        let unknown = host.unknown_host_calls();
        assert!(
            unknown.iter().any(|(call, _)| call == key),
            "{method}: expected {key} counted, got {unknown:#?}"
        );
    }
}
