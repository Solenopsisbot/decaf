//! Fabric Loader's `SemanticVersion`, for the version texts Java 25 has shown
//! (`tests/fixtures/fabric_semantic_version/java25.txt`, Fabric Loader 0.20.1).
//!
//! `ModMetadata.getVersion()` is `Version.parse(text)` (`V1ModMetadataParser` and
//! `BuiltinModMetadata`): a `SemanticVersionImpl` when `SemanticVersionImpl(text, false)`
//! accepts the text, else a `StringVersion`. Carpet reads the game version that way
//! (`(SemanticVersion) version`, then `getVersionComponent(1)` and `(2)`).
//!
//! The runtime holds no parser: a text it answers for is one row of [`SEMANTIC`], each row a case
//! the oracle printed, with every accessor the oracle exercised. For every other text
//! `getVersion()` is counted (an unknown host call), never a plain `String`: the loader answers
//! those texts, including the ones the oracle showed to be a `StringVersion` (`v1.0.0`, `abc`),
//! with a `StringVersion`, a class this runtime does not model, so a cast, `instanceof`,
//! `getClass()` or `toString` on a `String` would differ from it even where the friendly string
//! matches. This holds for every route to a `Version` (a jar's `fabric.mod.json`, the built-in
//! `minecraft` and `java` mods, and a mod known only by the host's mod list).
//!
//! Answered on a handle: `getFriendlyString`, `toString`, `getVersionComponentCount`,
//! `getVersionComponent(int)` (negative: `RuntimeException`; at or past the count: `0`; both
//! shown, with `Integer.MIN_VALUE`, `Integer.MAX_VALUE` and count + 7), `getPrereleaseKey`,
//! `getBuildKey` (`java.util.Optional<String>`), `hasWildcard` (always `false` here), and
//! `instanceof` of `Version`, `SemanticVersion` and `Comparable`. Counted: `compareTo`, `equals`,
//! `hashCode`, `Version.parse` and `SemanticVersion.parse` (the oracle shows their throws, which
//! the runtime does not model).

use crate::runtime::host_ref::{HostKind, HostRef};
use crate::runtime::{MethodRef, NativeError, Value, host_optional};

const VERSION: &str = "net/fabricmc/loader/api/Version";
const SEMANTIC_VERSION: &str = "net/fabricmc/loader/api/SemanticVersion";

/// One text the oracle parsed to a `SemanticVersion`.
struct Row {
    text: &'static str,
    components: &'static [i32],
    prerelease: Option<&'static str>,
    build: Option<&'static str>,
}

const fn row(
    text: &'static str,
    components: &'static [i32],
    prerelease: Option<&'static str>,
    build: Option<&'static str>,
) -> Row {
    Row {
        text,
        components,
        prerelease,
        build,
    }
}

/// Every text `java25.txt` shows `Version.parse` turning into a `SemanticVersion`.
const SEMANTIC: &[Row] = &[
    row("26.3+v260915", &[26, 3], None, Some("v260915")),
    row("1.0.0", &[1, 0, 0], None, None),
    row(
        "1.0.0-beta.2+build.5",
        &[1, 0, 0],
        Some("beta.2"),
        Some("build.5"),
    ),
    row("1.2", &[1, 2], None, None),
    row("1.0.0-SNAPSHOT", &[1, 0, 0], Some("SNAPSHOT"), None),
    row("1.0", &[1, 0], None, None),
    row("1", &[1], None, None),
    row("1.0.0.0", &[1, 0, 0, 0], None, None),
    row("26.3", &[26, 3], None, None),
    row("1.20.1-rc.1", &[1, 20, 1], Some("rc.1"), None),
    row("1.21.1", &[1, 21, 1], None, None),
    row("25", &[25], None, None),
    row("1.2.3", &[1, 2, 3], None, None),
    row("1.2.0+mc26.3", &[1, 2, 0], None, Some("mc26.3")),
    row("0.1", &[0, 1], None, None),
    row("2.0-beta.1", &[2, 0], Some("beta.1"), None),
    row("0.19.5", &[0, 19, 5], None, None),
    row("0.5.5", &[0, 5, 5], None, None),
    row("0.165.0+26.3", &[0, 165, 0], None, Some("26.3")),
    row("2.0.6+fcdff87f5d", &[2, 0, 6], None, Some("fcdff87f5d")),
    row("3.1.2+fcdff87f5d", &[3, 1, 2], None, Some("fcdff87f5d")),
    row("4.1.9+ffef5f675d", &[4, 1, 9], None, Some("ffef5f675d")),
    row("3.1.0+5af7744b5d", &[3, 1, 0], None, Some("5af7744b5d")),
];

fn lookup(text: &str) -> Option<&'static Row> {
    SEMANTIC.iter().find(|row| row.text == text)
}

/// The `Version` a mod reads for `text`: a handle for a text the oracle shows as a
/// `SemanticVersion`, `None` otherwise. The caller counts `getVersion()` for `None`: no plain
/// `String` is ever handed out as a `Version`, because the loader's answer for such a text is a
/// `StringVersion` (a class this runtime does not model), so a cast or `instanceof` on a `String`
/// would differ from it even where the text matches.
pub(crate) fn version_value(text: &str) -> Option<Value> {
    lookup(text).map(|_| Value::Host(HostRef::new(HostKind::FabricVersion, text)))
}

/// The classes a version handle is declared as (`ClassRule::Unknown`): its class is the loader's
/// `SemanticVersionImpl`, which a mod never names.
pub(crate) fn declared_types(_: &str) -> &'static [&'static str] {
    &[SEMANTIC_VERSION, VERSION, "java/lang/Comparable"]
}

/// What a call on a version handle is.
pub(crate) enum Call {
    /// Not a version handle.
    Other,
    /// A version handle's call the oracle does not show: counted.
    Counted,
    /// Answered.
    Answer(Result<Value, NativeError>),
}

/// An instance call whose receiver may be a version handle.
pub(crate) fn instance_call(r: &MethodRef, receiver: &Value, args: &[Value]) -> Call {
    let Value::Host(handle) = receiver else {
        return Call::Other;
    };
    if handle.kind() != HostKind::FabricVersion {
        return Call::Other;
    }
    let Some(row) = lookup(handle.key()) else {
        return Call::Counted;
    };
    if r.owner != VERSION && r.owner != SEMANTIC_VERSION {
        return Call::Counted;
    }
    let answer = match (r.name.as_str(), r.descriptor.as_str(), args) {
        ("getFriendlyString" | "toString", "()Ljava/lang/String;", []) => {
            Value::Ref(Some(row.text.to_owned()))
        }
        ("getVersionComponentCount", "()I", []) => Value::Int(row.components.len() as i32),
        ("getVersionComponent", "(I)I", [Value::Int(index)]) => {
            if *index < 0 {
                return Call::Answer(Err(NativeError::UncaughtException {
                    class: "java/lang/RuntimeException".into(),
                    message: Some("Tried to access negative version number component!".into()),
                    frame: None,
                }));
            }
            Value::Int(row.components.get(*index as usize).copied().unwrap_or(0))
        }
        ("getPrereleaseKey", "()Ljava/util/Optional;", []) => key(row.prerelease),
        ("getBuildKey", "()Ljava/util/Optional;", []) => key(row.build),
        ("hasWildcard", "()Z", []) => Value::Int(0),
        _ => return Call::Counted,
    };
    Call::Answer(Ok(answer))
}

fn key(text: Option<&str>) -> Value {
    match text {
        Some(text) => host_optional::optional_wrap(Value::Ref(Some(text.to_owned()))),
        None => host_optional::empty_optional(),
    }
}

/// `Version` handles never `equals` or hash as the oracle shows: counted.
pub(crate) const METHODS: crate::runtime::host_ref::Methods = crate::runtime::host_ref::Methods {
    to_string: |handle, _| Ok(Some(handle.key().to_owned())),
    hash_code: |_, _| Ok(None),
    equals: |_, _, _| Ok(None),
    crosses_jars: |_, _| true,
    compare_to: None,
    ..crate::runtime::host_events::DECLARED_TYPE_METHODS
};
