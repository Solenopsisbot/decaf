//! Declared Fabric Jar-in-Jar and NeoForge JarJar members.
use crate::{NativeJar, StdHost};
use std::cmp::Ordering;
const MAX_NESTING: usize = 8;
const MAX_NESTED_JARS: usize = 256;

/// The key Fabric uses in `ModDiscoverer.jijDedupMap`: the nested entry's
/// CRC-32 in the high word and uncompressed byte length in the low word.
pub(crate) fn identity(jar: &NativeJar) -> u64 {
    if let Some(hash) = jar.source_hash {
        return hash;
    }
    let mut bytes = Vec::new();
    for (name, payload) in &jar.resource_bytes {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(payload);
        bytes.push(0xff);
    }
    (u64::from(crc32fast::hash(&bytes)) << 32) | bytes.len() as u64
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedJar {
    pub entry: String,
    pub result: Result<usize, String>,
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct NestedCandidate {
    pub entry: String,
    pub parent: String,
    pub jar: NativeJar,
    pub depth: usize,
    /// Whether this candidate came from a top-level mod path (`createPlain`).
    /// Direct Jar-in-Jar entries have depth zero in our walk but are still
    /// non-root candidates to Fabric's resolver.
    is_root: bool,
    pub order: usize,
    /// Fabric's `ModCandidateImpl.hash(ZipEntry)`: CRC-32 of the nested jar
    /// bytes in the high word and their uncompressed size in the low word.
    pub hash: u64,
    /// Parent candidates, nearest first, for Fabric's recursive tie-break.
    parent_chain: Vec<(String, Option<String>, usize)>,
    /// NeoForge JarJar's `group:artifact` identity, when declared.
    pub neoforge_coordinate: Option<String>,
    /// JarJar's selected artifact version and requested Maven range.
    pub neoforge_artifact_version: Option<String>,
    pub neoforge_version_range: Option<String>,
    /// A JarJar declaration is a class-path entry even when its jar has no
    /// `neoforge.mods.toml` (the common library case).
    pub neoforge_declared: bool,
}
fn fabric_entries(jar: &NativeJar) -> Vec<String> {
    let Some(bytes) = jar.resource_bytes.get("fabric.mod.json") else {
        return Vec::new();
    };
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|v| v.get("jars").cloned())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| {
            v.get("file")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        // Fabric checks only ZipEntry.isDirectory and the .jar suffix. The
        // entry is read by exact name from the in-memory ZIP, so `..` is not a
        // filesystem traversal and is harmless here.
        .filter(|p| p.ends_with(".jar"))
        .collect()
}
fn neoforge_entries(jar: &NativeJar) -> Vec<String> {
    let Some(bytes) = jar.resource_bytes.get("META-INF/jarjar/metadata.json") else {
        return Vec::new();
    };
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|v| v.get("jars").cloned())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| {
            v.get("path")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        // JarJar also reads the named entry from the in-memory ZIP by its
        // exact name; `..` cannot escape to the filesystem.
        .filter(|p| p.ends_with(".jar"))
        .collect()
}
fn neoforge_declarations(
    jar: &NativeJar,
) -> std::collections::BTreeMap<String, (Option<String>, Option<String>, Option<String>)> {
    let Some(bytes) = jar.resource_bytes.get("META-INF/jarjar/metadata.json") else {
        return Default::default();
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Default::default();
    };
    value
        .get("jars")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let path = item.get("path")?.as_str()?.to_owned();
            let identifier = item.get("identifier");
            let coordinate = identifier
                .and_then(|v| v.get("group").and_then(serde_json::Value::as_str))
                .zip(identifier.and_then(|v| v.get("artifact").and_then(serde_json::Value::as_str)))
                .map(|(group, artifact)| format!("{group}:{artifact}"));
            let range = item
                .get("version")
                .and_then(|v| v.get("range").and_then(serde_json::Value::as_str))
                .map(str::to_owned);
            let artifact_version = item
                .get("version")
                .and_then(|v| v.get("artifactVersion").and_then(serde_json::Value::as_str))
                .map(str::to_owned);
            Some((path, (coordinate, artifact_version, range)))
        })
        .collect()
}
fn declared_entries(jar: &NativeJar) -> Vec<(String, bool)> {
    fabric_entries(jar)
        .into_iter()
        .map(|entry| (entry, true))
        .chain(
            neoforge_entries(jar)
                .into_iter()
                .map(|entry| (entry, false)),
        )
        .collect()
}
/// Discover nested candidates for one Fabric physical environment.
///
/// Fabric's `ModDiscoverer` only scans a candidate's declared nested jars when
/// that candidate loads in the active environment.  `client_side_only` is a
/// property of the metadata, not a reason to skip the candidate on a client;
/// the old unconditional gate dropped every nested class from client-only
/// mods while probing the client role.
fn candidates_for_side(jar: &NativeJar, side: &str) -> Vec<NestedCandidate> {
    let mut out = Vec::new();
    let mut by_hash = std::collections::BTreeMap::<u64, usize>::new();
    let mut ordinal = 0;
    fn walk(
        parent: &str,
        jar: &NativeJar,
        depth: usize,
        out: &mut Vec<NestedCandidate>,
        by_hash: &mut std::collections::BTreeMap<u64, usize>,
        ordinal: &mut usize,
        parent_chain: &[(String, Option<String>, usize)],
        side: &str,
    ) {
        // ModDiscoverer.computeJarFile/computeJarStream returns a candidate
        // without scanning its declared jars when the parent metadata does
        // not load in the active environment (the branch at bytecode offsets
        // 110-149 and 82-118 respectively).  Its nested candidates therefore
        // never enter resolution on that side.
        if jar.client_side_only && side != "CLIENT" {
            return;
        }
        if depth >= MAX_NESTING || out.len() >= MAX_NESTED_JARS {
            return;
        };
        for (entry, fabric_entry) in declared_entries(jar) {
            if out.len() >= MAX_NESTED_JARS {
                break;
            };
            let Some(bytes) = jar.resource_bytes.get(&entry) else {
                continue;
            };
            let hash = (u64::from(crc32fast::hash(bytes)) << 32) | bytes.len() as u64;
            if fabric_entry && let Some(&index) = by_hash.get(&hash) {
                // ModDiscoverer keeps one ModScanTask for this hash and adds
                // each new parent to that candidate. Its nested children are
                // therefore scanned once, even when several parents bundle
                // the same bytes.
                let candidate = &mut out[index];
                candidate.depth = candidate.depth.min(depth);
                for parent in parent_chain {
                    if !candidate.parent_chain.contains(parent) {
                        candidate.parent_chain.push(parent.clone());
                    }
                }
                continue;
            }
            let Ok(nested) = NativeJar::from_bytes(bytes) else {
                continue;
            };
            // Fabric's ModDiscoverer only creates a nested candidate after
            // ModMetadataParser finds fabric.mod.json.  A plain library in a
            // Fabric `jars` declaration is therefore absent from both the
            // resolver and Knot's selected candidate class path.  NeoForge
            // JarJar libraries are different: JarSelector deliberately keeps
            // them as class-path entries without mod metadata.
            if fabric_entry && nested.fabric_id.is_none() {
                continue;
            }
            // `computeJarStream` applies the same environment gate to the
            // nested candidate itself.  It is absent from the server-side
            // resolver and therefore cannot contribute classes there.
            if nested.client_side_only && side != "CLIENT" {
                continue;
            }
            let jarjar = neoforge_declarations(jar);
            let (neoforge_coordinate, neoforge_artifact_version, neoforge_version_range) =
                jarjar.get(&entry).cloned().unwrap_or_default();
            let child_parent = format!("{parent}|{entry}");
            out.push(NestedCandidate {
                entry: entry.clone(),
                parent: parent.to_owned(),
                jar: nested.clone(),
                depth,
                is_root: false,
                order: *ordinal,
                hash,
                parent_chain: parent_chain.to_vec(),
                neoforge_coordinate,
                neoforge_artifact_version,
                neoforge_version_range,
                neoforge_declared: !neoforge_entries(jar).is_empty(),
            });
            if fabric_entry {
                by_hash.insert(hash, out.len() - 1);
            }
            *ordinal += 1;
            let mut child_chain = parent_chain.to_vec();
            child_chain.insert(0, (candidate_id(jar), candidate_version(jar), depth));
            walk(
                &child_parent,
                &nested,
                depth + 1,
                out,
                by_hash,
                ordinal,
                &child_chain,
                side,
            );
        }
    }
    walk(
        &root_identity(jar),
        jar,
        0,
        &mut out,
        &mut by_hash,
        &mut ordinal,
        &[],
        side,
    );
    out
}

/// The root component of a nested class-path id. Path-loaded roots use their
/// own path; in-memory roots use Fabric's byte identity before metadata labels,
/// so same-id roots with different bytes cannot share a component.
fn root_identity(jar: &NativeJar) -> String {
    jar.source_path
        .clone()
        .or_else(|| jar.source_hash.map(|hash| format!("jar-{hash:016x}")))
        .or_else(|| jar.fabric_id.clone())
        .or_else(|| jar.neoforge_id.clone())
        .unwrap_or_else(|| format!("jar-{:016x}", identity(jar)))
}

/// Discover nested candidates using Fabric's server-side environment, which
/// is retained for the resolver and fixture APIs that predate side-aware
/// discovery.
#[allow(dead_code)]
pub fn candidates(jar: &NativeJar) -> Vec<NestedCandidate> {
    candidates_for_side(jar, "SERVER")
}

fn candidate_id(jar: &NativeJar) -> String {
    jar.fabric_id
        .clone()
        .or_else(|| jar.neoforge_id.clone())
        .unwrap_or_default()
}

fn candidate_version(jar: &NativeJar) -> Option<String> {
    jar.fabric_version
        .clone()
        .or_else(|| jar.neoforge_version.clone())
}

fn priority_is_root(candidate: &NestedCandidate) -> bool {
    // The per-jar nested selector receives direct children without the outer
    // candidate, so depth zero is the selector's root tier. The resolver
    // oracle still records the true createPlain/createNested root bit.
    candidate.is_root || candidate.depth == 0
}
#[derive(Clone, Debug)]
enum FabricVersion<'a> {
    Semantic {
        components: Vec<i64>,
        wildcard: bool,
        prerelease: Option<&'a str>,
    },
    String(&'a str),
}

fn fabric_version(value: Option<&str>) -> FabricVersion<'_> {
    let value = value.unwrap_or("0");
    let (without_build, _) = value
        .split_once('+')
        .map_or((value, None), |(a, b)| (a, Some(b)));
    let (numbers, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(a, b)| (a, Some(b)));
    // Version.parse(String) calls VersionParser.parse(..., false): X-ranges
    // are accepted only by the resolver's private parser, and therefore are
    // deliberately StringVersion values on the public API.
    let components = numbers.split('.').collect::<Vec<_>>();
    let valid_prerelease = prerelease.is_none_or(|v| {
        v.is_empty()
            || v.split('.').all(|part| {
                !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
    });
    let mut parsed = Vec::with_capacity(components.len());
    for component in components {
        if component.is_empty() || !component.chars().all(|c| c.is_ascii_digit()) {
            return FabricVersion::String(value);
        }
        let Ok(component) = component.parse::<i32>() else {
            return FabricVersion::String(value);
        };
        if component < 0 {
            return FabricVersion::String(value);
        }
        parsed.push(i64::from(component));
    }
    if parsed.is_empty() || !valid_prerelease {
        FabricVersion::String(value)
    } else {
        FabricVersion::Semantic {
            components: parsed,
            wildcard: false,
            prerelease,
        }
    }
}

fn semantic_cmp(a: &FabricVersion<'_>, b: &FabricVersion<'_>) -> Ordering {
    let (
        FabricVersion::Semantic {
            components: ac,
            wildcard: aw,
            prerelease: ap,
        },
        FabricVersion::Semantic {
            components: bc,
            wildcard: bw,
            prerelease: bp,
        },
    ) = (a, b)
    else {
        return Ordering::Equal;
    };
    for i in 0..ac.len().max(bc.len()) {
        let av = ac.get(i).copied().unwrap_or(if *aw { i64::MIN } else { 0 });
        let bv = bc.get(i).copied().unwrap_or(if *bw { i64::MIN } else { 0 });
        if av != i64::MIN && bv != i64::MIN {
            match av.cmp(&bv) {
                Ordering::Equal => {}
                other => return other,
            }
        }
    }
    match (ap, bp) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => {
            if *bw {
                Ordering::Equal
            } else {
                Ordering::Less
            }
        }
        (None, Some(_)) => {
            if *aw {
                Ordering::Equal
            } else {
                Ordering::Greater
            }
        }
        (Some(a), Some(b)) => {
            let mut ai = a.split('.');
            let mut bi = b.split('.');
            loop {
                match (ai.next(), bi.next()) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(x), Some(y)) => {
                        let xn = x.chars().all(|c| c.is_ascii_digit());
                        let yn = y.chars().all(|c| c.is_ascii_digit());
                        let cmp = match (xn, yn) {
                            (true, true) => x.len().cmp(&y.len()).then_with(|| x.cmp(y)),
                            (true, false) => Ordering::Less,
                            (false, true) => Ordering::Greater,
                            (false, false) => x.cmp(y),
                        };
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn version_cmp(left: Option<&str>, right: Option<&str>) -> Ordering {
    let av = fabric_version(left);
    let bv = fabric_version(right);
    match (&av, &bv) {
        (FabricVersion::Semantic { .. }, FabricVersion::Semantic { .. }) => semantic_cmp(&av, &bv),
        (FabricVersion::String(a), FabricVersion::String(b)) => a.cmp(b),
        (FabricVersion::Semantic { .. }, FabricVersion::String(right)) => {
            semantic_friendly(&av).as_str().cmp(right)
        }
        (FabricVersion::String(left), FabricVersion::Semantic { .. }) => {
            left.cmp(&semantic_friendly(&bv).as_str())
        }
    }
}

fn semantic_friendly(version: &FabricVersion<'_>) -> String {
    let FabricVersion::Semantic {
        components,
        prerelease,
        ..
    } = version
    else {
        return String::new();
    };
    let mut result = components
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(".");
    if let Some(prerelease) = prerelease {
        result.push('-');
        result.push_str(prerelease);
    }
    result
}

#[derive(Clone, Debug)]
struct MavenInterval {
    low: Option<String>,
    high: Option<String>,
    low_inclusive: bool,
    high_inclusive: bool,
}

#[derive(Clone, Debug)]
struct MavenRange {
    recommended: Option<String>,
    restrictions: Vec<MavenInterval>,
}

fn maven_cmp(a: &str, b: &str) -> Ordering {
    #[derive(Clone, Debug)]
    enum Item {
        Big(String),
        Str(String),
        List(Vec<Item>),
        Int(String),
        Long(String),
    }
    impl Item {
        fn kind(&self) -> u8 {
            match self {
                Self::Big(_) => 0,
                Self::Str(_) => 1,
                Self::List(_) => 2,
                Self::Int(_) => 3,
                Self::Long(_) => 4,
            }
        }
        fn is_null(&self) -> bool {
            match self {
                Self::Big(v) | Self::Int(v) | Self::Long(v) => v == "0",
                Self::Str(v) => comparable_qualifier(v) == "5",
                Self::List(v) => v.is_empty(),
            }
        }
        fn compare(&self, other: Option<&Self>) -> Ordering {
            let Some(other) = other else {
                return match self {
                    Self::Big(v) | Self::Int(v) | Self::Long(v) => {
                        if v == "0" {
                            Ordering::Equal
                        } else {
                            Ordering::Greater
                        }
                    }
                    Self::Str(v) => comparable_qualifier(v).as_str().cmp("5"),
                    Self::List(v) => {
                        for item in v {
                            let cmp = item.compare(None);
                            if cmp != Ordering::Equal {
                                return cmp;
                            }
                        }
                        Ordering::Equal
                    }
                };
            };
            match (self.kind(), other.kind()) {
                (0, 0) => numeric_cmp(item_value(self), item_value(other)),
                (0, 1..=4) => Ordering::Greater,
                (1, 0 | 2 | 3 | 4) => Ordering::Less,
                (2, 0) => Ordering::Less,
                (2, 1) => Ordering::Greater,
                (2, 3 | 4) => Ordering::Less,
                (1, 1) => comparable_qualifier(item_value(self))
                    .cmp(&comparable_qualifier(item_value(other))),
                (2, 2) => {
                    let (Self::List(left), Self::List(right)) = (self, other) else {
                        unreachable!()
                    };
                    for i in 0..left.len().max(right.len()) {
                        let cmp = match (left.get(i), right.get(i)) {
                            (Some(a), Some(b)) => a.compare(Some(b)),
                            (Some(a), None) => a.compare(None),
                            (None, Some(b)) => b.compare(None).reverse(),
                            (None, None) => Ordering::Equal,
                        };
                        if cmp != Ordering::Equal {
                            return cmp;
                        }
                    }
                    Ordering::Equal
                }
                (3, 3) => numeric_cmp(item_value(self), item_value(other)),
                (4, 4) => numeric_cmp(item_value(self), item_value(other)),
                (3, 0) => Ordering::Less,
                (3, 1 | 2) => Ordering::Greater,
                (3, 4) => Ordering::Less,
                (4, 0) => Ordering::Less,
                (4, 1 | 2) => Ordering::Greater,
                (4, 3) => Ordering::Greater,
                _ => unreachable!(),
            }
        }
    }
    fn item_value(item: &Item) -> &str {
        match item {
            Item::Big(v) | Item::Str(v) | Item::Int(v) | Item::Long(v) => v,
            Item::List(_) => unreachable!(),
        }
    }
    fn numeric_cmp(a: &str, b: &str) -> Ordering {
        a.len().cmp(&b.len()).then_with(|| a.cmp(b))
    }
    fn comparable_qualifier(value: &str) -> String {
        match value {
            "alpha" => "0".into(),
            "beta" => "1".into(),
            "milestone" => "2".into(),
            "rc" => "3".into(),
            "snapshot" => "4".into(),
            "" => "5".into(),
            "sp" => "6".into(),
            other => format!("7-{other}"),
        }
    }
    fn parse_item(is_digit: bool, value: &str, transition: bool) -> Item {
        if is_digit {
            let value = value.trim_start_matches('0');
            let value = if value.is_empty() { "0" } else { value };
            match value.len() {
                0..=9 => Item::Int(value.into()),
                10..=18 => Item::Long(value.into()),
                _ => Item::Big(value.into()),
            }
        } else {
            let value = if transition && value == "a" {
                "alpha"
            } else if transition && value == "b" {
                "beta"
            } else if transition && value == "m" {
                "milestone"
            } else {
                value
            };
            let value = match value {
                "ga" | "final" | "release" => "",
                "cr" => "rc",
                value => value,
            };
            Item::Str(value.to_owned())
        }
    }
    fn normalize(items: &mut Vec<Item>) {
        for item in items.iter_mut() {
            if let Item::List(children) = item {
                normalize(children);
            }
        }
        let mut index = items.len();
        while index > 0 {
            index -= 1;
            if items[index].is_null() {
                items.remove(index);
            } else if !matches!(items[index], Item::List(_)) {
                break;
            }
        }
    }
    fn parse(value: &str) -> Item {
        let value = value.to_ascii_lowercase();
        let bytes = value.as_bytes();
        let mut nodes = vec![Vec::new()];
        let mut current = 0usize;
        let mut stack: Vec<(usize, usize)> = Vec::new();
        let mut start = 0usize;
        let mut is_digit = false;
        // The parser is deliberately written in the same state-machine shape as Maven's
        // bytecode. Pointers only refer to vectors owned by `root` and its nested lists.
        for i in 0..bytes.len() {
            let c = bytes[i] as char;
            if c == '.' || c == '-' {
                if i == start {
                    nodes[current].push(Item::Int("0".into()));
                } else {
                    nodes[current].push(parse_item(is_digit, &value[start..i], false));
                }
                start = i + 1;
                if c == '-'
                    || (!is_digit
                        && c == '.'
                        && bytes.get(i + 1).is_some_and(|next| !next.is_ascii_digit()))
                {
                    let slot = nodes[current].len();
                    nodes[current].push(Item::List(Vec::new()));
                    let child = nodes.len();
                    nodes.push(Vec::new());
                    stack.push((current, slot));
                    current = child;
                    is_digit = false;
                }
                continue;
            }
            if c.is_ascii_digit() {
                if !is_digit && i > start {
                    if !nodes[current].is_empty() {
                        let slot = nodes[current].len();
                        nodes[current].push(Item::List(Vec::new()));
                        let child = nodes.len();
                        nodes.push(Vec::new());
                        stack.push((current, slot));
                        current = child;
                    }
                    nodes[current].push(parse_item(false, &value[start..i], true));
                    let slot = nodes[current].len();
                    nodes[current].push(Item::List(Vec::new()));
                    let child = nodes.len();
                    nodes.push(Vec::new());
                    stack.push((current, slot));
                    current = child;
                    start = i;
                }
                is_digit = true;
            } else if is_digit {
                if i > start {
                    nodes[current].push(parse_item(true, &value[start..i], false));
                }
                let slot = nodes[current].len();
                nodes[current].push(Item::List(Vec::new()));
                let child = nodes.len();
                nodes.push(Vec::new());
                stack.push((current, slot));
                current = child;
                start = i;
                is_digit = false;
            }
        }
        if bytes.len() > start {
            nodes[current].push(parse_item(is_digit, &value[start..], false));
        }
        while let Some((parent, slot)) = stack.pop() {
            let child = std::mem::take(&mut nodes[current]);
            nodes[parent][slot] = Item::List(child);
            current = parent;
        }
        normalize(&mut nodes[0]);
        // A dot-separated qualifier following the first qualifier is flattened by
        // Maven's parser (while a `+` string remains in its hyphen list).
        let dot_qualifier = value
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'.' && !pair[1].is_ascii_digit());
        if nodes[0].len() > 1 && dot_qualifier && !value.contains('-') {
            let mut promoted = Vec::new();
            if let Item::List(items) = &mut nodes[0][1]
                && items.len() > 1
                && matches!(&items[0], Item::Str(value) if !value.starts_with('+'))
            {
                if matches!(&items[1], Item::List(_)) {
                    promoted.push(items.remove(0));
                } else {
                    promoted.append(items);
                }
            }
            for item in promoted.into_iter().rev() {
                nodes[0].insert(1, item);
            }
        }
        Item::List(std::mem::take(&mut nodes[0]))
    }
    let left = parse(a);
    let right = parse(b);
    left.compare(Some(&right))
}

fn maven_range(value: &str) -> Result<MavenRange, String> {
    let original = value.to_owned();
    let mut rest = value;
    let mut restrictions = Vec::new();
    let mut previous_upper: Option<String> = None;
    while rest.starts_with('[') || rest.starts_with('(') {
        let close_paren = rest.find(')');
        let close_bracket = rest.find(']');
        let close = match (close_paren, close_bracket) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => return Err(format!("Unbounded range: {original}")),
        };
        let part = &rest[..=close];
        let interval = parse_maven_interval(part)?;
        if let (Some(previous), Some(lower)) = (&previous_upper, &interval.low)
            && maven_cmp(lower, previous) == Ordering::Less
        {
            return Err(format!("Ranges overlap: {original}"));
        }
        previous_upper = interval.high.clone();
        restrictions.push(interval);
        rest = rest[close + 1..].trim_start();
        if rest.starts_with(',') {
            rest = rest[1..].trim_start();
        }
    }
    let recommended = if rest.is_empty() {
        None
    } else {
        if !restrictions.is_empty() {
            return Err(format!(
                "Only fully-qualified sets allowed in multiple set scenario: {original}"
            ));
        }
        Some(rest.to_owned())
    };
    if recommended.is_some() {
        restrictions.push(MavenInterval {
            low: None,
            high: None,
            low_inclusive: false,
            high_inclusive: false,
        });
    }
    Ok(MavenRange {
        recommended,
        restrictions,
    })
}

fn parse_maven_interval(part: &str) -> Result<MavenInterval, String> {
    let bytes = part.as_bytes();
    let low_inclusive = bytes.first() == Some(&b'[');
    let high_inclusive = bytes.last() == Some(&b']');
    let body = part[1..part.len() - 1].trim();
    let Some(comma) = body.find(',') else {
        if low_inclusive && high_inclusive {
            let version = body.to_owned();
            return Ok(MavenInterval {
                low: Some(version.clone()),
                high: Some(version),
                low_inclusive: true,
                high_inclusive: true,
            });
        }
        return Err(format!("Single version must be surrounded by []: {part}"));
    };
    let low = body[..comma].trim();
    let high = body[comma + 1..].trim();
    let low = (!low.is_empty()).then(|| low.to_owned());
    let high = (!high.is_empty()).then(|| high.to_owned());
    if let (Some(low), Some(high)) = (&low, &high) {
        let cmp = maven_cmp(high, low);
        if cmp == Ordering::Less || (cmp == Ordering::Equal && low_inclusive && !high_inclusive) {
            return Err(format!("Range defies version ordering: {part}"));
        }
    }
    Ok(MavenInterval {
        low,
        high,
        low_inclusive,
        high_inclusive,
    })
}

fn intersect(a: &[MavenInterval], b: &[MavenInterval]) -> Vec<MavenInterval> {
    let mut out = Vec::new();
    for left in a {
        for right in b {
            let (low, low_inclusive) = match (&left.low, &right.low) {
                (None, x) => (x.clone(), right.low_inclusive),
                (x, None) => (x.clone(), left.low_inclusive),
                (Some(x), Some(y)) => match maven_cmp(x, y) {
                    Ordering::Greater => (Some(x.clone()), left.low_inclusive),
                    Ordering::Less => (Some(y.clone()), right.low_inclusive),
                    Ordering::Equal => (Some(x.clone()), left.low_inclusive && right.low_inclusive),
                },
            };
            let (high, high_inclusive) = match (&left.high, &right.high) {
                (None, x) => (x.clone(), right.high_inclusive),
                (x, None) => (x.clone(), left.high_inclusive),
                (Some(x), Some(y)) => match maven_cmp(x, y) {
                    Ordering::Less => (Some(x.clone()), left.high_inclusive),
                    Ordering::Greater => (Some(y.clone()), right.high_inclusive),
                    Ordering::Equal => {
                        (Some(x.clone()), left.high_inclusive && right.high_inclusive)
                    }
                },
            };
            if let (Some(low), Some(high)) = (&low, &high) {
                let c = maven_cmp(low, high);
                if c == Ordering::Greater
                    || (c == Ordering::Equal && !(low_inclusive && high_inclusive))
                {
                    continue;
                }
            }
            out.push(MavenInterval {
                low,
                high,
                low_inclusive,
                high_inclusive,
            });
        }
    }
    out
}

fn maven_in_range(version: &str, ranges: &[MavenInterval]) -> bool {
    ranges.iter().any(|r| {
        let low = r.low.as_ref().is_none_or(|low| {
            maven_cmp(version, low) == Ordering::Greater
                || (r.low_inclusive && maven_cmp(version, low) == Ordering::Equal)
        });
        let high = r.high.as_ref().is_none_or(|high| {
            maven_cmp(version, high) == Ordering::Less
                || (r.high_inclusive && maven_cmp(version, high) == Ordering::Equal)
        });
        low && high
    })
}

fn maven_restrict(left: &MavenRange, right: &MavenRange) -> MavenRange {
    let restrictions = if left.restrictions.is_empty() || right.restrictions.is_empty() {
        Vec::new()
    } else {
        intersect(&left.restrictions, &right.restrictions)
    };
    let recommended = if !restrictions.is_empty() {
        left.recommended
            .as_ref()
            .filter(|v| maven_in_range(v, &restrictions))
            .cloned()
            .or_else(|| {
                right
                    .recommended
                    .as_ref()
                    .filter(|v| maven_in_range(v, &restrictions))
                    .cloned()
            })
    } else {
        left.recommended
            .clone()
            .or_else(|| right.recommended.clone())
    };
    MavenRange {
        recommended,
        restrictions,
    }
}

#[cfg(test)]
fn select(jar: &NativeJar) -> Vec<NestedCandidate> {
    select_checked(jar).unwrap_or_default()
}

#[cfg(test)]
fn select_fabric_candidates(fabric: Vec<NestedCandidate>) -> Result<Vec<NestedCandidate>, String> {
    select_fabric_candidates_with_roots(fabric, &[])
}

fn select_fabric_candidates_with_roots(
    mut fabric: Vec<NestedCandidate>,
    roots: &[NativeJar],
) -> Result<Vec<NestedCandidate>, String> {
    // `net.fabricmc.loader.impl.discovery.ModPrioSorter.sort` first calls
    // List.sort (bytecode offsets 0-4), then recognizes the first non-root at
    // offsets 228-273 and removes a nested candidate that overlaps an earlier
    // root at offsets 387-531. This is the only overlap decision Decaf can make
    // without Fabric's SAT solver.
    let root_candidates = fabric
        .iter()
        .filter(|candidate| priority_is_root(candidate))
        .cloned()
        .collect::<Vec<_>>();
    let root_keys = root_candidates
        .iter()
        .flat_map(candidate_keys)
        .collect::<std::collections::BTreeSet<_>>();
    fabric.retain(|candidate| {
        priority_is_root(candidate)
            || candidate_keys(candidate)
                .iter()
                .all(|key| !root_keys.contains(key))
    });
    // `net.fabricmc.loader.impl.discovery.ModResolver.findCompatibleSet` invokes
    // `preselectMod` at offsets 458-501; `ModResolver.preselectMod` invokes
    // `ModResolver.selectMod` at offsets 0-6, and `selectMod`'s put/throw branches
    // at offsets 0-39 and 40-108 reject duplicate ids (and repeated aliases within
    // one candidate) before `ModSolver.solve` at offsets 504-516. Keep those exact
    // simple refusals; an alias collision between surviving candidates is left to
    // the SAT refusal below.
    // ModSolver is the first component that can choose between two different
    // byte sequences for one id. Reproduce only the forced case: no metadata
    // declaration may mention the id, and the newer copy has no declarations
    // of its own. SAT-shaped cases remain honest refusals.
    let mut by_id = std::collections::BTreeMap::<String, Vec<NestedCandidate>>::new();
    for candidate in &fabric {
        by_id
            .entry(candidate_id(&candidate.jar))
            .or_default()
            .push(candidate.clone());
    }
    for (id, group) in by_id {
        if group.len() < 2 {
            continue;
        }
        let mut ordered = group;
        ordered.sort_by(|left, right| {
            version_cmp(
                candidate_version(&right.jar).as_deref(),
                candidate_version(&left.jar).as_deref(),
            )
            .then_with(|| left.order.cmp(&right.order))
        });
        let newest = ordered[0].clone();
        let newest_version = candidate_version(&newest.jar);
        if ordered.iter().skip(1).any(|candidate| {
            version_cmp(
                candidate_version(&candidate.jar).as_deref(),
                newest_version.as_deref(),
            ) == Ordering::Equal
        }) {
            return Err(format!(
                "Fabric's mod resolver would decide equal-version copies of {id}; Decaf cannot prove its byte tie-break"
            ));
        }
        let all_candidates = fabric
            .iter()
            .map(|candidate| &candidate.jar)
            .chain(roots.iter())
            .collect::<Vec<_>>();
        if let Some(reason) = constraint_blocks_forced_newest(&id, &newest, &all_candidates) {
            return Err(reason);
        }
        let keep = identity(&newest.jar);
        fabric.retain(|candidate| {
            candidate_id(&candidate.jar) != id || identity(&candidate.jar) == keep
        });
    }

    let mut selected_ids = std::collections::BTreeSet::new();
    for candidate in &fabric {
        let id = candidate_id(&candidate.jar);
        if !selected_ids.insert(id.clone()) {
            return Err(format!("duplicate mod {id}"));
        }
        let mut candidate_keys = std::collections::BTreeSet::new();
        candidate_keys.insert(id);
        for alias in &candidate.jar.fabric_provides {
            if !candidate_keys.insert(alias.clone()) {
                return Err(format!("duplicate mod {alias}"));
            }
        }
    }

    // ModPrioSorter.sort returns immediately at offsets 193-202 when no alias
    // overlaps exist; ModResolver then calls the SAT solver. Alias overlaps that
    // remain after root-first pruning (including cycles and version ties) belong
    // to that SAT call, so Decaf refuses them instead of imitating its result.
    for (index, left) in fabric.iter().enumerate() {
        let left_keys = candidate_keys(left);
        for right in fabric.iter().skip(index + 1) {
            if left_keys
                .iter()
                .any(|key| candidate_keys(right).contains(key))
            {
                return Err(format!(
                    "Fabric's mod resolver would decide this conflict between {} and {}; Decaf does not model its SAT solver",
                    candidate_id(&left.jar),
                    candidate_id(&right.jar),
                ));
            }
        }
    }
    // On solver success, `ModResolver.findCompatibleSet` sorts the selected list by
    // id at bytecode offsets 839-849. This is an ordering rule, not conflict solving.
    fabric.sort_by_key(|candidate| candidate_id(&candidate.jar));
    Ok(fabric)
}

/// Check whether the newest copy is provably compatible with every metadata
/// constraint in the complete discovery set. A false/unknown result is a
/// refusal: the bounded resolver must never guess at Fabric's SAT outcome.
fn constraint_blocks_forced_newest(
    id: &str,
    newest: &NestedCandidate,
    all_candidates: &[&NativeJar],
) -> Option<String> {
    for source in all_candidates {
        let Some(document) = source
            .resource_bytes
            .get("fabric.mod.json")
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        else {
            continue;
        };
        let newest_keys = candidate_keys_for_jar(&newest.jar);
        for kind in ["depends", "breaks", "conflicts"] {
            let Some(entries) = document.get(kind).and_then(serde_json::Value::as_object) else {
                continue;
            };
            for (constraint_id, raw_range) in entries {
                // Fabric resolves a dependency by every id a candidate owns,
                // including `provides` aliases, rather than only its primary id.
                if !newest_keys.iter().any(|key| key == constraint_id) {
                    continue;
                }
                let Some(matches) = fabric_range_matches_value(
                    raw_range,
                    candidate_version(&newest.jar).as_deref(),
                ) else {
                    return Some(format!(
                        "Fabric's mod resolver would decide version constraints for {constraint_id}; Decaf cannot prove the range"
                    ));
                };
                let excludes_newest = match kind {
                    "depends" => !matches,
                    "breaks" | "conflicts" => matches,
                    _ => false,
                };
                if excludes_newest {
                    return Some(format!(
                        "Fabric's mod resolver would decide version constraints for {constraint_id}; Decaf refuses the constrained newest copy"
                    ));
                }
            }
        }
    }

    let document = newest
        .jar
        .resource_bytes
        .get("fabric.mod.json")
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())?;
    let dependencies = document
        .get("depends")
        .and_then(serde_json::Value::as_object)?;
    for (dependency_id, raw_range) in dependencies {
        // `parse_metadata` already removes loader-provided dependencies from
        // `required_mods`; those are satisfied by Fabric's builtins rather
        // than by this nested candidate closure.
        if !newest
            .jar
            .required_mods
            .iter()
            .any(|required| required == dependency_id)
        {
            continue;
        }
        let found = all_candidates.iter().any(|candidate| {
            candidate_keys_for_jar(candidate)
                .iter()
                .any(|key| key == dependency_id)
                && fabric_range_matches_value(raw_range, candidate_version(candidate).as_deref())
                    == Some(true)
        });
        if !found {
            return Some(format!(
                "Fabric's mod resolver would decide dependencies of {id}; Decaf cannot prove dependency {dependency_id}"
            ));
        }
    }
    None
}

fn candidate_keys_for_jar(jar: &NativeJar) -> Vec<String> {
    std::iter::once(candidate_id(jar))
        .chain(jar.fabric_provides.iter().cloned())
        .collect()
}

#[derive(Clone, Copy)]
enum FabricPredicateOperator {
    GreaterEqual,
    LessEqual,
    Greater,
    Less,
    Equal,
    SameToNextMinor,
    SameToNextMajor,
}

#[derive(Clone)]
struct FabricPredicateTerm {
    operator: FabricPredicateOperator,
    reference: String,
    semantic: bool,
    components: Vec<i64>,
}

/// Parse the exact bounded form of Fabric's VersionPredicateParser. Any
/// syntax which cannot be represented here returns `None`, causing selection
/// to refuse rather than inventing a SAT result.
fn fabric_predicate_terms(range: &str) -> Option<Vec<FabricPredicateTerm>> {
    let mut terms = Vec::new();
    // A comma remains part of a version token and therefore falls back to
    // StringVersion exactly as the Java parser does.
    // `VersionPredicateParser.parse` splits on a single space (`split(" ")`),
    // then `String.trim()`s each token, which strips every char <= U+0020 but
    // only at the ends: a tab or newline inside a token stays part of it.
    for raw in range.split(' ') {
        let raw = raw.trim_matches(|c: char| c <= ' ');
        if raw.is_empty() || raw == "*" {
            continue;
        }
        let (operator, expected) = [
            (">=", FabricPredicateOperator::GreaterEqual),
            ("<=", FabricPredicateOperator::LessEqual),
            (">", FabricPredicateOperator::Greater),
            ("<", FabricPredicateOperator::Less),
            ("=", FabricPredicateOperator::Equal),
            ("~", FabricPredicateOperator::SameToNextMinor),
            ("^", FabricPredicateOperator::SameToNextMajor),
        ]
        .into_iter()
        .find_map(|(prefix, operator)| {
            raw.strip_prefix(prefix)
                .map(|expected| (operator, expected))
        })
        .unwrap_or((FabricPredicateOperator::Equal, raw));
        if expected.is_empty() {
            return None;
        }

        let parsed = predicate_semantic_parts(expected);
        let Some((components, wildcard, prerelease)) = parsed else {
            // VersionParser falls back to StringVersion. Inclusive operators
            // (including ^/~) then match only the exact friendly string;
            // exclusive operators are rejected by VersionPredicateParser.
            if matches!(
                operator,
                FabricPredicateOperator::Greater | FabricPredicateOperator::Less
            ) {
                return None;
            }
            terms.push(FabricPredicateTerm {
                operator,
                reference: expected.to_owned(),
                semantic: false,
                components: Vec::new(),
            });
            continue;
        };
        if let Some(wildcard) = wildcard {
            if !matches!(operator, FabricPredicateOperator::Equal) || prerelease.is_some() {
                if wildcard == 0
                    && matches!(
                        operator,
                        FabricPredicateOperator::Equal
                            | FabricPredicateOperator::GreaterEqual
                            | FabricPredicateOperator::LessEqual
                            | FabricPredicateOperator::SameToNextMinor
                            | FabricPredicateOperator::SameToNextMajor
                    )
                {
                    terms.push(FabricPredicateTerm {
                        operator,
                        reference: expected.to_owned(),
                        semantic: false,
                        components: Vec::new(),
                    });
                    continue;
                }
                return None;
            }
            if wildcard == 0 {
                terms.push(FabricPredicateTerm {
                    operator,
                    reference: expected.to_owned(),
                    semantic: false,
                    components: Vec::new(),
                });
                continue;
            }
            let base = components[..wildcard].to_vec();
            if wildcard == 1 {
                terms.push(FabricPredicateTerm {
                    operator: FabricPredicateOperator::SameToNextMajor,
                    reference: semantic_string(&base, Some("")),
                    semantic: true,
                    components: base,
                });
            } else if wildcard == 2 {
                terms.push(FabricPredicateTerm {
                    operator: FabricPredicateOperator::SameToNextMinor,
                    reference: semantic_string(&base, Some("")),
                    semantic: true,
                    components: base,
                });
            } else {
                let mut upper = base.clone();
                *upper.last_mut()? += 1;
                terms.push(FabricPredicateTerm {
                    operator: FabricPredicateOperator::GreaterEqual,
                    reference: semantic_string(&base, Some("")),
                    semantic: true,
                    components: base,
                });
                terms.push(FabricPredicateTerm {
                    operator: FabricPredicateOperator::Less,
                    reference: semantic_string(&upper, None),
                    semantic: true,
                    components: upper,
                });
            }
            continue;
        }
        if matches!(
            operator,
            FabricPredicateOperator::SameToNextMinor | FabricPredicateOperator::SameToNextMajor
        ) && components.is_empty()
        {
            return None;
        }
        terms.push(FabricPredicateTerm {
            operator,
            reference: expected.to_owned(),
            semantic: true,
            components,
        });
    }
    Some(terms)
}

fn predicate_semantic_parts(value: &str) -> Option<(Vec<i64>, Option<usize>, Option<&str>)> {
    let (without_build, _) = value
        .split_once('+')
        .map_or((value, None), |(a, b)| (a, Some(b)));
    let (numbers, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(a, b)| (a, Some(b)));
    if prerelease.is_some_and(|v| {
        !v.is_empty()
            && !v.split('.').all(|part| {
                !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
    }) {
        return None;
    }
    let mut components = Vec::new();
    let mut wildcard = None;
    for (index, component) in numbers.split('.').enumerate() {
        if component == "x" || component == "X" || component == "*" {
            if wildcard.is_some() || index + 1 != numbers.split('.').count() {
                return None;
            }
            wildcard = Some(index);
            components.push(0);
        } else {
            let value = component.parse::<i32>().ok()?;
            if value < 0 || wildcard.is_some() {
                return None;
            }
            components.push(i64::from(value));
        }
    }
    if components.is_empty() {
        return None;
    }
    Some((components, wildcard, prerelease))
}

fn semantic_string(components: &[i64], prerelease: Option<&str>) -> String {
    let mut value = components
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(".");
    if let Some(prerelease) = prerelease {
        value.push('-');
        value.push_str(prerelease);
    }
    value
}

fn fabric_predicate_matches(range: &str, version: Option<&str>) -> Option<bool> {
    // `VersionParser.parse` throws "Version must be a non-empty string!" for
    // an empty version, so there is no answer to give.
    let version = version.filter(|version| !version.is_empty())?;
    let terms = fabric_predicate_terms(range)?;
    let candidate = fabric_version(Some(version));
    Some(terms.iter().all(|term| {
        let cmp = version_cmp(Some(version), Some(&term.reference));
        match (&candidate, term.semantic, term.operator) {
            (FabricVersion::Semantic { .. }, true, FabricPredicateOperator::GreaterEqual) => {
                cmp != Ordering::Less
            }
            (FabricVersion::Semantic { .. }, true, FabricPredicateOperator::LessEqual) => {
                cmp != Ordering::Greater
            }
            (FabricVersion::Semantic { .. }, true, FabricPredicateOperator::Greater) => {
                cmp == Ordering::Greater
            }
            (FabricVersion::Semantic { .. }, true, FabricPredicateOperator::Less) => {
                cmp == Ordering::Less
            }
            (FabricVersion::Semantic { .. }, true, FabricPredicateOperator::Equal) => {
                cmp == Ordering::Equal
            }
            (
                FabricVersion::Semantic { components, .. },
                true,
                FabricPredicateOperator::SameToNextMajor,
            ) => cmp != Ordering::Less && components.first() == term.components.first(),
            (
                FabricVersion::Semantic { components, .. },
                true,
                FabricPredicateOperator::SameToNextMinor,
            ) => {
                cmp != Ordering::Less
                    && components.first().copied().unwrap_or(0)
                        == term.components.first().copied().unwrap_or(0)
                    && components.get(1).copied().unwrap_or(0)
                        == term.components.get(1).copied().unwrap_or(0)
            }
            (
                _,
                false,
                FabricPredicateOperator::Equal
                | FabricPredicateOperator::GreaterEqual
                | FabricPredicateOperator::LessEqual
                | FabricPredicateOperator::SameToNextMinor
                | FabricPredicateOperator::SameToNextMajor,
            ) => version == term.reference,
            _ => false,
        }
    }))
}

fn fabric_range_matches_value(value: &serde_json::Value, version: Option<&str>) -> Option<bool> {
    match value {
        serde_json::Value::String(range) => fabric_predicate_matches(range, version),
        serde_json::Value::Array(ranges) => {
            if ranges.is_empty() {
                return Some(false);
            }
            let mut matched = false;
            for range in ranges {
                let serde_json::Value::String(range) = range else {
                    return None;
                };
                matched |= fabric_predicate_matches(range, version)?;
            }
            Some(matched)
        }
        _ => None,
    }
}

fn candidate_keys(candidate: &NestedCandidate) -> Vec<String> {
    std::iter::once(candidate_id(&candidate.jar))
        .chain(candidate.jar.fabric_provides.iter().cloned())
        .collect()
}

/// Select the nested jars, preserving JarSelector's refusal when a declared
/// coordinate has no artifact in the restricted Maven range.
#[allow(dead_code)]
pub fn select_checked(jar: &NativeJar) -> Result<Vec<NestedCandidate>, String> {
    select_checked_for_side(jar, "SERVER")
}

pub(crate) fn select_checked_for_side(
    jar: &NativeJar,
    side: &str,
) -> Result<Vec<NestedCandidate>, String> {
    let all = candidates_for_side(jar, side);
    let mut neoforge_groups: std::collections::BTreeMap<String, Vec<NestedCandidate>> =
        Default::default();
    let mut fabric = Vec::new();
    for candidate in all {
        if let Some(coordinate) = candidate.neoforge_coordinate.clone() {
            neoforge_groups
                .entry(coordinate)
                .or_default()
                .push(candidate);
        } else {
            fabric.push(candidate);
        }
    }
    let mut selected = select_fabric_candidates_with_roots(fabric, std::slice::from_ref(jar))?;
    for (_coordinate, group) in neoforge_groups {
        if group.len() == 1 {
            selected.extend(group);
            continue;
        }
        let mut allowed: Option<MavenRange> = None;
        for candidate in &group {
            let Ok(range) = candidate
                .neoforge_version_range
                .as_deref()
                .map_or_else(|| Err("missing version range".to_owned()), maven_range)
            else {
                allowed = Some(MavenRange {
                    recommended: None,
                    restrictions: Vec::new(),
                });
                break;
            };
            allowed = Some(match allowed {
                Some(ref current) => maven_restrict(current, &range),
                None => range,
            });
            if allowed
                .as_ref()
                .is_some_and(|range| range.restrictions.is_empty() && range.recommended.is_none())
            {
                break;
            }
        }
        let Some(allowed) = allowed else {
            return Err(neoforge_no_match_error(&group));
        };
        if allowed.restrictions.is_empty() && allowed.recommended.is_none() {
            return Err(neoforge_no_match_error(&group));
        }
        let matches = |candidate: &NestedCandidate| {
            candidate
                .neoforge_artifact_version
                .as_deref()
                .is_some_and(|v| maven_in_range(v, &allowed.restrictions))
        };
        if let Some(recommended) = allowed.recommended {
            if let Some(candidate) = group.iter().find(|candidate| {
                candidate.neoforge_artifact_version.as_deref() == Some(recommended.as_str())
            }) {
                selected.push(candidate.clone());
            } else {
                return Err(neoforge_no_match_error(&group));
            }
        } else {
            let coordinate = group
                .first()
                .and_then(|candidate| candidate.neoforge_coordinate.clone());
            let requester = group
                .first()
                .map(|candidate| candidate.parent.clone())
                .unwrap_or_else(|| "unknown".into());
            if let Some(candidate) = group.into_iter().find(matches) {
                // JarSelector uses Collection.stream().findFirst(), preserving
                // metadata iteration order; it never chooses the maximum.
                selected.push(candidate);
            } else {
                return Err(format!(
                    "Some mods have agreed upon an acceptable version range for : §6{}§r, but no jar was provided which matched the range. Requested by: §e{}§r.",
                    coordinate.as_deref().unwrap_or("unknown:unknown"),
                    requester,
                ));
            }
        }
    }
    selected.sort_by_key(|c| c.order);
    Ok(selected)
}

fn neoforge_no_match_error(group: &[NestedCandidate]) -> String {
    let coordinate = group
        .first()
        .and_then(|candidate| candidate.neoforge_coordinate.as_deref())
        .unwrap_or("unknown:unknown");
    let requester = group
        .iter()
        .map(|candidate| candidate.parent.as_str())
        .next()
        .unwrap_or("unknown");
    // In-memory roots without metadata labels use a hash only for class-path
    // identity; Fabric/JarJar's diagnostic still names the logical outer jar.
    let requester = requester
        .strip_prefix("jar-")
        .map_or(requester, |_| "outer");
    format!(
        "Some mods have agreed upon an acceptable version range for : §6{coordinate}§r, but no jar was provided which matched the range. Requested by: §e{requester}§r."
    )
}
pub fn link_nested_jars(host: &mut StdHost, jar: &NativeJar) -> Vec<NestedJar> {
    let mut out = Vec::new();
    let selected = match select_checked_for_side(jar, host.environment_type.as_str()) {
        Ok(selected) => selected,
        Err(error) => {
            return vec![NestedJar {
                entry: "<jarjar-resolution>".into(),
                result: Err(error),
            }];
        }
    };
    for candidate in selected {
        let id = format!("{}!{}", candidate.parent, candidate.entry);
        let result = host
            .link_nested_jar(id, &candidate.jar)
            .map_err(|e| e.to_string());
        if result.is_ok() {
            let parent = jar
                .fabric_id
                .as_deref()
                .or(jar.neoforge_id.as_deref())
                .unwrap_or("jar");
            if let Some(id) = candidate.jar.fabric_id.as_deref() {
                host.fabric_origins.insert(
                    id.to_owned(),
                    ("nested".into(), format!("{parent}|{}", candidate.entry)),
                );
            }
            for alias in &candidate.jar.fabric_provides {
                host.fabric_origins.insert(
                    alias.clone(),
                    ("nested".into(), format!("{parent}|{}", candidate.entry)),
                );
            }
        }
        out.push(NestedJar {
            entry: candidate.entry,
            result,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn jar(entries: &[(&str, &[u8])]) -> NativeJar {
        let mut bytes = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            for (name, content) in entries {
                zip.start_file(*name, SimpleFileOptions::default()).unwrap();
                zip.write_all(content).unwrap();
            }
            zip.finish().unwrap();
        }
        NativeJar::from_bytes(&bytes.into_inner()).unwrap()
    }

    #[test]
    fn jarjar_library_without_mod_metadata_is_selected() {
        let outer = jar(&[
            (
                "META-INF/jarjar/metadata.json",
                br#"{"jars":[{"path":"META-INF/jarjar/lib.jar"}]}"#,
            ),
            ("META-INF/jarjar/lib.jar", &empty_zip()),
        ]);
        let selected = select(&outer);
        assert_eq!(selected.len(), 1);
        assert!(selected[0].jar.neoforge_id.is_none());
    }

    #[test]
    fn jarjar_duplicate_coordinate_keeps_highest_artifact_version() {
        let empty_bytes = empty_zip();
        let outer = jar(&[
            (
                "META-INF/jarjar/metadata.json",
                br#"{"jars":[{"path":"META-INF/jarjar/old.jar","identifier":{"group":"g","artifact":"a"},"version":{"range":"[1,3)","artifactVersion":"1.0"}},{"path":"META-INF/jarjar/new.jar","identifier":{"group":"g","artifact":"a"},"version":{"range":"[1,3)","artifactVersion":"2.0"}}]}"#,
            ),
            ("META-INF/jarjar/old.jar", &empty_bytes),
            ("META-INF/jarjar/new.jar", &empty_bytes),
        ]);
        let selected = select(&outer);
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].neoforge_artifact_version.as_deref(),
            Some("1.0")
        );
    }

    #[test]
    fn fabric_only_links_declared_entries_and_explicit_empty_lists_link_none() {
        let nested = empty_zip();
        let unlisted = jar(&[("META-INF/jars/unlisted.jar", &nested)]);
        assert!(candidates(&unlisted).is_empty());
        let explicit_empty = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"x","version":"1","jars":[]}"#,
            ),
            ("META-INF/jars/unlisted.jar", &nested),
        ]);
        assert!(candidates(&explicit_empty).is_empty());
        let listed = jar(&[
            ("fabric.mod.json", br#"{"schemaVersion":1,"id":"x","version":"1","jars":[{"file":"META-INF/jars/child.jar"}]}"#),
            ("META-INF/jars/child.jar", &nested),
        ]);
        assert!(candidates(&listed).is_empty());
    }

    #[test]
    fn client_side_nested_candidates_follow_the_active_environment() {
        let child = zip_bytes(&jar(&[(
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"client-lib","version":"1"}"#,
        )]));
        let outer = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"client-carrier","version":"1","environment":"client","jars":[{"file":"META-INF/jars/client.jar"}]}"#,
            ),
            ("META-INF/jars/client.jar", &child),
        ]);

        assert!(candidates_for_side(&outer, "SERVER").is_empty());
        let client = candidates_for_side(&outer, "CLIENT");
        assert_eq!(client.len(), 1);
        assert_eq!(client[0].jar.fabric_id.as_deref(), Some("client-lib"));
    }

    #[test]
    fn identical_fabric_nested_bytes_merge_across_parents() {
        let child = zip_bytes(&jar(&[(
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"shared-lib","version":"1"}"#,
        )]));
        let outer = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"outer","version":"1","jars":[{"file":"a.jar"},{"file":"b.jar"}]}"#,
            ),
            ("a.jar", &child),
            ("b.jar", &child),
        ]);
        let selected = select_checked(&outer).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].jar.fabric_id.as_deref(), Some("shared-lib"));
    }

    #[test]
    fn discarded_candidate_is_not_linked() {
        let older = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"shared-lib","version":"1.0"}"#,
            ),
            ("discarded.marker", b"old"),
        ]);
        let newer = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"shared-lib","version":"2.0"}"#,
            ),
            ("selected.marker", b"new"),
        ]);
        let outer = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"carrier","version":"1","jars":[{"file":"old.jar"},{"file":"new.jar"}]}"#,
            ),
            ("old.jar", &zip_bytes(&older)),
            ("new.jar", &zip_bytes(&newer)),
        ]);
        let host = outer.seed_host(StdHost::new());
        assert!(host.resource("selected.marker").is_some());
        assert!(host.resource("discarded.marker").is_none());
    }

    #[test]
    fn real_jar_selection_matches_captured_fabric_rows() {
        let Some(corpus) = std::env::var_os("HORIZON_CORPUS") else {
            println!("skipped real-jar oracle: HORIZON_CORPUS is unset");
            return;
        };
        let corpus = std::path::PathBuf::from(corpus);
        let oracle_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracles");
        let Ok(cases) = std::fs::read_to_string(oracle_dir.join("mod_prio_real_cases.txt")) else {
            println!("skipped real-jar oracle: case list is absent");
            return;
        };
        let Ok(oracle) = std::fs::read_to_string(oracle_dir.join("mod_prio_real_java25.txt"))
        else {
            println!("skipped real-jar oracle: captured rows are absent");
            return;
        };
        let mut checked = 0;
        for line in cases
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let (name, paths) = line.split_once('|').unwrap();
            let paths = paths
                .strip_prefix("jar=")
                .unwrap()
                .split('|')
                .map(|path| corpus.join(path))
                .collect::<Vec<_>>();
            if paths.iter().any(|path| !path.is_file()) {
                println!("skipped real-jar oracle case {name}: a corpus jar is missing");
                continue;
            }
            let Some(row) = oracle
                .lines()
                .find(|row| row.starts_with(&format!("{name}|")))
            else {
                panic!("missing captured Fabric row {name}");
            };
            let fabric_refused = !row.contains("|OK|");
            let expected = row
                .split_once("|OK|")
                .map(|(_, value)| value)
                .unwrap_or_default()
                .split(',')
                .filter(|field| field.contains("@nested@"))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mut all_candidates = Vec::new();
            for path in &paths {
                let root = NativeJar::from_path(path).unwrap();
                all_candidates.extend(candidates(&root));
            }
            let mut seen = std::collections::BTreeSet::new();
            all_candidates.retain(|candidate| seen.insert(identity(&candidate.jar)));
            // The captured oracle is the server-side ModResolver result.
            // `NativeJar` already records the metadata's client-only gate;
            // those candidates are inactive before SAT and must not enter the
            // comparison or the class path.
            all_candidates.retain(|candidate| !candidate.jar.client_side_only);
            let actual_result = select_fabric_candidates(all_candidates);
            if fabric_refused {
                // Fabric's real ModResolver rejected this census closure;
                // Decaf must refuse too, without copying its diagnostic.
                // Root dependency satisfaction is outside this nested-candidate
                // selector, so preserve a refusal for a closure missing one of
                // the roots' required ids as well as SAT-shaped nested cases.
                let available = paths
                    .iter()
                    .filter_map(|path| NativeJar::from_path(path).ok())
                    .filter_map(|root| root.fabric_id.or(root.neoforge_id))
                    .collect::<std::collections::BTreeSet<_>>();
                let missing_root_dependency = paths
                    .iter()
                    .filter_map(|path| NativeJar::from_path(path).ok())
                    .any(|root| root.required_mods.iter().any(|id| !available.contains(id)));
                assert!(
                    actual_result.is_err() || missing_root_dependency,
                    "{name} unexpectedly selected"
                );
                checked += 1;
                continue;
            }
            if actual_result.is_err() {
                // A successful Fabric row can still be outside Decaf's
                // modeled discovery subset. Count that row as a refusal
                // rather than inventing a selected candidate.
                checked += 1;
                continue;
            }
            let mut actual = actual_result
                .unwrap()
                .into_iter()
                .map(|candidate| {
                    format!(
                        "{}@{}@nested@{}",
                        candidate_id(&candidate.jar),
                        candidate_version(&candidate.jar).unwrap_or_default(),
                        candidate.depth + 1
                    )
                })
                .collect::<Vec<_>>();
            actual.sort();
            let mut expected = expected;
            expected.sort();
            assert_eq!(actual, expected, "real Fabric row {name}");
            checked += 1;
        }
        if checked == 0 {
            println!("skipped real-jar oracle: no complete captured cases");
        }
    }

    #[test]
    fn client_nested_selection_matches_captured_fabric_rows() {
        let Some(corpus) = std::env::var_os("HORIZON_CORPUS") else {
            println!("skipped real client-jar oracle: HORIZON_CORPUS is unset");
            return;
        };
        let corpus = std::path::PathBuf::from(corpus);
        let oracle_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracles");
        let Ok(cases) = std::fs::read_to_string(oracle_dir.join("mod_prio_client_real_cases.txt"))
        else {
            println!("skipped real client-jar oracle: case list is absent");
            return;
        };
        let Ok(oracle) =
            std::fs::read_to_string(oracle_dir.join("mod_prio_client_real_java25.txt"))
        else {
            println!("skipped real client-jar oracle: captured rows are absent");
            return;
        };
        let mut checked = 0;
        for line in cases
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let (name, paths) = line.split_once('|').unwrap();
            let paths = paths
                .strip_prefix("jar=")
                .unwrap()
                .split('|')
                .map(|path| corpus.join(path))
                .collect::<Vec<_>>();
            if paths.iter().any(|path| !path.is_file()) {
                println!("skipped captured client case {name}: a corpus jar is missing");
                continue;
            }
            let Some(row) = oracle
                .lines()
                .find(|row| row.starts_with(&format!("{name}|")))
            else {
                panic!("missing captured Fabric client row {name}");
            };
            assert!(
                row.contains("|OK|"),
                "captured client row {name} refused: {row}"
            );
            let expected = row
                .split_once("|OK|")
                .map(|(_, value)| value)
                .unwrap_or_default()
                .split(',')
                .filter(|field| field.contains("@nested@"))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mut all_candidates = Vec::new();
            for path in &paths {
                let root = NativeJar::from_path(path).unwrap();
                all_candidates.extend(candidates_for_side(&root, "CLIENT"));
            }
            let mut seen = std::collections::BTreeSet::new();
            all_candidates.retain(|candidate| seen.insert(identity(&candidate.jar)));
            let mut actual = select_fabric_candidates(all_candidates)
                .unwrap()
                .into_iter()
                .map(|candidate| {
                    format!(
                        "{}@{}@nested@{}",
                        candidate_id(&candidate.jar),
                        candidate_version(&candidate.jar).unwrap_or_default(),
                        candidate.depth + 1
                    )
                })
                .collect::<Vec<_>>();
            actual.sort();
            let mut expected = expected;
            expected.sort();
            assert_eq!(actual, expected, "real Fabric client row {name}");
            checked += 1;
        }
        assert!(checked > 0, "no complete real client-jar oracle cases");
    }

    #[test]
    fn generated_nested_candidates_match_real_discoverer_rows() {
        // Captured with ModPrioSorterOracle on Fabric Loader 0.19.5 / JDK 25
        // from the same synthetic carrier jars. Loom's generated metadata is
        // an ordinary V1 metadata document by the time ModDiscoverer parses
        // it: its id/version are selected like every other candidate, while
        // its generated marker does not add provides or dependencies.
        let expected = include_str!("generated_nested_real_java25.txt");
        let generated = |version: &str, marker: &str| {
            jar(&[
                (
                    "fabric.mod.json",
                    format!(
                        r#"{{"schemaVersion":1,"id":"generated-lib","version":"{version}","custom":{{"fabric-loom:generated":true}}}}"#
                    )
                    .as_bytes(),
                ),
                ("marker.txt", marker.as_bytes()),
            ])
        };
        let carrier = |id: &str, child: &NativeJar| {
            jar(&[
                (
                    "fabric.mod.json",
                    format!(
                        r#"{{"schemaVersion":1,"id":"{id}","version":"1","jars":[{{"file":"META-INF/jars/lib.jar"}}]}}"#
                    )
                    .as_bytes(),
                ),
                ("META-INF/jars/lib.jar", &zip_bytes(child)),
            ])
        };
        let one = carrier("carrier", &generated("1.0", "one"));
        let selected = select_checked(&one).unwrap();
        let single = selected
            .iter()
            .map(|candidate| {
                format!(
                    "{}@{}@nested@{}",
                    candidate_id(&candidate.jar),
                    candidate_version(&candidate.jar).unwrap_or_default(),
                    candidate.depth + 1
                )
            })
            .collect::<Vec<_>>();
        let expected_nested = |name: &str| {
            expected
                .lines()
                .find(|line| line.starts_with(&format!("{name}|OK|")))
                .unwrap()
                .split_once("|OK|")
                .unwrap()
                .1
                .split(',')
                .filter(|field| field.contains("@nested@"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(single, expected_nested("generated-single"));

        let first = carrier("carrier_a", &generated("1.0", "one"));
        let second = carrier("carrier_b", &generated("2.0", "two"));
        let mut closure = candidates(&first);
        closure.extend(candidates(&second));
        let selected = select_fabric_candidates(closure).unwrap();
        let mut pair = selected
            .iter()
            .filter(|candidate| candidate_id(&candidate.jar) == "generated-lib")
            .map(|candidate| {
                format!(
                    "{}@{}@nested@{}",
                    candidate_id(&candidate.jar),
                    candidate_version(&candidate.jar).unwrap_or_default(),
                    candidate.depth + 1
                )
            })
            .collect::<Vec<_>>();
        pair.sort();
        assert_eq!(pair, expected_nested("generated-two"));

        let plain = carrier("plain-carrier", &jar(&[]));
        assert!(candidates_for_side(&plain, "CLIENT").is_empty());
        assert!(expected_nested("plain-library").is_empty());
    }

    #[test]
    fn review579_nested_shapes_match_java25_rows() {
        // Captured by the real Fabric Loader 0.19.5 ModDiscoverer on JDK 25.
        // Fabric selects the older copy for the three constrained shapes and
        // the newer copy when two top-level roots bundle the same path.
        let rows = include_str!("review579_nested_real_java25.txt");
        let child = |id: &str, version: &str, extra: Option<&str>| {
            let suffix = extra.unwrap_or("");
            jar(&[(
                "fabric.mod.json",
                format!(r#"{{"schemaVersion":1,"id":"{id}","version":"{version}"{suffix}}}"#)
                    .as_bytes(),
            )])
        };
        let carrier = |id: &str, constraint: &str, children: Vec<(&str, NativeJar)>| {
            let files = children
                .iter()
                .map(|(entry, _)| format!(r#"{{"file":"{entry}"}}"#))
                .collect::<Vec<_>>()
                .join(",");
            let metadata = format!(
                r#"{{"schemaVersion":1,"id":"{id}","version":"1"{constraint},"jars":[{files}]}}"#
            );
            let mut entries = vec![("fabric.mod.json".to_owned(), metadata.into_bytes())];
            let mut bytes = Vec::new();
            for (entry, nested) in children {
                bytes.push((entry.to_owned(), zip_bytes(&nested)));
            }
            entries.extend(bytes);
            let refs = entries
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
                .collect::<Vec<_>>();
            jar(&refs)
        };
        let old = child("shared-lib", "1.0", None);
        let new = child("shared-lib", "2.0", None);
        let alias_old = child("shared-lib", "1.0", Some(r#","provides":["shared-api"]"#));
        let alias_new = child("shared-lib", "2.0", Some(r#","provides":["shared-api"]"#));
        let pinner = child(
            "pinner",
            "1.0",
            Some(r#","depends":{"shared-lib":"<2.0.0"}"#),
        );
        let constrained = [
            (
                "root-constraint",
                carrier(
                    "carrier",
                    r#","depends":{"shared-lib":"<2.0.0"}"#,
                    vec![
                        ("shared-old.jar", old.clone()),
                        ("shared-new.jar", new.clone()),
                    ],
                ),
            ),
            (
                "root-breaks",
                carrier(
                    "carrier",
                    r#","breaks":{"shared-lib":">=2.0.0"}"#,
                    vec![
                        ("shared-old.jar", old.clone()),
                        ("shared-new.jar", new.clone()),
                    ],
                ),
            ),
            (
                "nested-sibling-constraint",
                carrier(
                    "carrier",
                    "",
                    vec![
                        ("META-INF/jars/pinner.jar", pinner),
                        ("META-INF/jars/shared-old.jar", old.clone()),
                        ("META-INF/jars/shared-new.jar", new.clone()),
                    ],
                ),
            ),
            (
                "alias-constraint",
                carrier(
                    "carrier",
                    r#","depends":{"shared-api":"<2.0.0"}"#,
                    vec![
                        ("shared-old.jar", alias_old.clone()),
                        ("shared-new.jar", alias_new.clone()),
                    ],
                ),
            ),
            (
                "caret-breaks",
                carrier(
                    "carrier",
                    r#","breaks":{"shared-api":"^1.2"}"#,
                    vec![
                        (
                            "shared-old.jar",
                            child("shared-lib", "1.2", Some(r#","provides":["shared-api"]"#)),
                        ),
                        (
                            "shared-new.jar",
                            child("shared-lib", "1.3", Some(r#","provides":["shared-api"]"#)),
                        ),
                    ],
                ),
            ),
            (
                "tilde-breaks",
                carrier(
                    "carrier",
                    r#","breaks":{"shared-api":"~1.2"}"#,
                    vec![
                        (
                            "shared-old.jar",
                            child("shared-lib", "1.1.9", Some(r#","provides":["shared-api"]"#)),
                        ),
                        (
                            "shared-new.jar",
                            child("shared-lib", "1.2.5", Some(r#","provides":["shared-api"]"#)),
                        ),
                    ],
                ),
            ),
        ];
        for (name, root) in constrained {
            let row = rows
                .lines()
                .find(|line| line.starts_with(&format!("{name}|")))
                .unwrap_or_else(|| panic!("missing oracle row {name}"));
            let actual =
                select_fabric_candidates_with_roots(candidates(&root), std::slice::from_ref(&root));
            // Fabric's row is OK and names the older nested copy; Decaf
            // refuses rather than inventing the SAT choice.
            if name == "alias-constraint" {
                assert!(row.contains("shared-lib@1.0@nested@1"));
            } else if name == "tilde-breaks" {
                assert!(row.contains("shared-lib@1.1.9@nested@1"));
            } else if name == "caret-breaks" {
                assert!(!row.contains("shared-lib@"));
            } else {
                assert!(row.contains("shared-lib@1.0@nested@1"));
            }
            assert!(
                actual.is_err(),
                "{name} unexpectedly selected a constrained newest copy"
            );
        }

        let first = carrier("carrier-first", "", vec![("META-INF/jars/shared.jar", old)]);
        let second = carrier(
            "carrier-second",
            "",
            vec![("META-INF/jars/shared.jar", new)],
        );
        let row = rows
            .lines()
            .find(|line| line.starts_with("two-top-level-same-path|"))
            .unwrap();
        let mut all = candidates(&first);
        all.extend(candidates(&second));
        let actual =
            select_fabric_candidates_with_roots(all, &[first.clone(), second.clone()]).unwrap();
        let selected = actual
            .iter()
            .map(|candidate| {
                format!(
                    "{}@{}@nested@{}",
                    candidate_id(&candidate.jar),
                    candidate_version(&candidate.jar).unwrap_or_default(),
                    candidate.depth + 1
                )
            })
            .collect::<Vec<_>>();
        assert!(row.contains("shared-lib@2.0@nested@1"));
        assert_eq!(selected, ["shared-lib@2.0@nested@1"]);

        let mut host = StdHost::new();
        assert!(
            link_nested_jars(&mut host, &first)
                .into_iter()
                .all(|nested| nested.result.is_ok())
        );
        assert!(
            link_nested_jars(&mut host, &second)
                .into_iter()
                .all(|nested| nested.result.is_ok())
        );
        let ids = host.linked_classes().jar_ids().collect::<Vec<_>>();
        assert!(
            ids.iter()
                .any(|id| id.starts_with("jar-") && id.ends_with("!META-INF/jars/shared.jar"))
        );
        assert!(
            ids.iter()
                .filter(|id| id.starts_with("jar-") && id.ends_with("!META-INF/jars/shared.jar"))
                .count()
                == 2
        );
    }

    #[test]
    fn overlapping_fabric_aliases_are_refused_without_sat_imitation() {
        let one = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"one","version":"1","provides":["shared"]}"#,
            ),
            ("one.txt", b"one"),
        ]);
        let two = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"two","version":"2","provides":["shared"]}"#,
            ),
            ("two.txt", b"two"),
        ]);
        let outer = jar(&[
            (
                "fabric.mod.json",
                br#"{"schemaVersion":1,"id":"outer","version":"1","jars":[{"file":"one.jar"},{"file":"two.jar"}]}"#,
            ),
            ("one.jar", &zip_bytes(&one)),
            ("two.jar", &zip_bytes(&two)),
        ]);
        let error = select_checked(&outer).unwrap_err();
        assert!(error.contains("does not model its SAT solver"));
    }

    #[test]
    fn overlapping_fabric_alias_cycle_is_refused_without_sat_imitation() {
        let child = |id: &str, provides: &str| {
            jar(&[(
                "fabric.mod.json",
                format!(
                    r#"{{"schemaVersion":1,"id":"{id}","version":"1","provides":["{provides}"]}}"#
                )
                .as_bytes(),
            )])
        };
        let a = child("a", "b");
        let b = child("b", "c");
        let c = child("c", "a");
        let outer = jar(&[
            ("fabric.mod.json", br#"{"schemaVersion":1,"id":"outer","version":"1","jars":[{"file":"a.jar"},{"file":"b.jar"},{"file":"c.jar"}]}"#),
            ("a.jar", &zip_bytes(&a)),
            ("b.jar", &zip_bytes(&b)),
            ("c.jar", &zip_bytes(&c)),
        ]);
        let error = select_checked(&outer).unwrap_err();
        assert!(error.contains("does not model its SAT solver"));
    }

    #[test]
    fn fabric_selection_matches_the_java25_mod_prio_oracle() {
        for case in include_str!("nested_jars_cases.txt").lines() {
            let (name, specs) = case.split_once('|').unwrap();
            let mut parsed = Vec::new();
            for (order, raw) in specs.split(';').filter(|raw| !raw.is_empty()).enumerate() {
                let fields = raw.split(',').collect::<Vec<_>>();
                parsed.push((
                    fields[0].to_owned(),
                    fields[1].to_owned(),
                    fields[2].to_owned(),
                    fields[3] == "1",
                    if fields[4] == "-" {
                        Vec::new()
                    } else {
                        fields[4].split('/').map(str::to_owned).collect()
                    },
                    (fields.get(5).copied().unwrap_or("-") != "-"
                        && !fields.get(5).copied().unwrap_or("").is_empty())
                    .then(|| fields[5].to_owned()),
                    fields.get(6).copied().unwrap_or("-").to_owned(),
                    order,
                ));
            }
            let mut depths = BTreeMap::new();
            for (label, _, _, root, _, parent, _, _) in &parsed {
                let depth = if *root {
                    0
                } else {
                    parent
                        .as_ref()
                        .and_then(|parent| depths.get(parent).copied())
                        .unwrap_or(0)
                        + 1
                };
                depths.insert(label.clone(), depth);
            }
            let candidates = parsed
                .iter()
                .map(|(label, id, version, root, provides, parent, dependencies, order)| {
                    let metadata = if dependencies == "-" || dependencies.is_empty() {
                        format!(r#"{{"schemaVersion":1,"id":"{id}","version":"{version}"}}"#)
                    } else {
                        let entries = dependencies
                            .split('/')
                            .map(|dep| {
                                let (dep_id, range) = dep.split_once(':').unwrap_or((dep, ">=0"));
                                format!(r#""{dep_id}":"{range}""#)
                            })
                            .collect::<Vec<_>>()
                            .join(",");
                        format!(r#"{{"schemaVersion":1,"id":"{id}","version":"{version}","depends":{{{entries}}}}}"#)
                    };
                    let jar = NativeJar {
                        fabric_id: Some(id.clone()),
                        fabric_version: Some(version.clone()),
                        fabric_provides: provides.clone(),
                        resource_bytes: BTreeMap::from([(String::from("fabric.mod.json"), metadata.into_bytes())]),
                        ..NativeJar::default()
                    };
                    let depth = depths[label];
                    let parent_chain = parent
                        .as_ref()
                        .and_then(|parent| {
                            parsed.iter().find(|(candidate, ..)| candidate == parent)
                        })
                        .map(|(_, id, version, _, _, _, _, _)| {
                            vec![(id.clone(), Some(version.clone()), depth - 1)]
                        })
                        .unwrap_or_default();
                    NestedCandidate {
                        entry: label.clone(),
                        parent: parent.clone().unwrap_or_else(|| "outer".into()),
                        jar,
                        depth,
                        is_root: *root,
                        order: *order,
                        hash: 0,
                        parent_chain,
                        neoforge_coordinate: None,
                        neoforge_artifact_version: None,
                        neoforge_version_range: None,
                        neoforge_declared: false,
                    }
                })
                .collect();
            let actual = select_fabric_candidates(candidates);
            let expected = include_str!("nested_jars_java25.txt")
                .lines()
                .find_map(|line| {
                    let mut fields = line.splitn(4, '|');
                    (fields.next()? == name).then(|| {
                        (
                            fields.next().unwrap_or_default().to_owned(),
                            fields.next().unwrap_or_default().to_owned(),
                            fields.next().unwrap_or_default().to_owned(),
                        )
                    })
                })
                .unwrap_or_else(|| panic!("missing Java oracle row {name}"));
            let decaf_refuses = matches!(
                name,
                "four-cycle"
                    | "duplicate-id"
                    | "root-alias-id"
                    | "newer-missing-dependency"
                    | "pinned-upper-bound"
                    | "equal-version-bytes"
            );
            match expected.0.as_str() {
                "OK" => {
                    // Fabric oracle outcome: OK. Decaf still honestly refuses
                    // the SAT-shaped conflict cases listed above.
                    let expected = expected
                        .1
                        .split(',')
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    match actual {
                        Err(error) if decaf_refuses => {
                            assert!(!error.is_empty(), "{name}");
                        }
                        Err(error) => panic!("{name} unexpectedly refused: {error}"),
                        Ok(actual) => {
                            let actual = actual
                                .iter()
                                .map(|candidate| {
                                    format!(
                                        "{}@{}@{}@{}",
                                        candidate_id(&candidate.jar),
                                        candidate_version(&candidate.jar).unwrap_or_default(),
                                        if candidate.is_root { "root" } else { "nested" },
                                        candidate.depth
                                    )
                                })
                                .collect::<Vec<_>>();
                            assert_eq!(actual, expected, "{name}");
                        }
                    }
                }
                "ERROR" => {
                    // Fabric oracle outcome: ERROR. Decaf refuses without
                    // copying Fabric's SAT explanation or metadata name.
                    assert!(actual.is_err(), "{name} unexpectedly selected candidates");
                }
                other => panic!("unknown oracle status {other} for {name}"),
            }
        }
    }

    #[test]
    fn fabric_root_beats_an_overlapping_nested_candidate() {
        let inner = jar(&[(
            "fabric.mod.json",
            br#"{"id":"bb","version":"1","provides":["aa"]}"#,
        )]);
        let carrier = jar(&[
            (
                "fabric.mod.json",
                br#"{"id":"carrier","version":"1","jars":[{"file":"bb.jar"}]}"#,
            ),
            ("bb.jar", &zip_bytes(&inner)),
        ]);
        let outer = jar(&[
            (
                "fabric.mod.json",
                br#"{"id":"outer","version":"1","jars":[{"file":"aa.jar"},{"file":"carrier.jar"}]}"#,
            ),
            (
                "aa.jar",
                &zip_bytes(&jar(&[(
                    "fabric.mod.json",
                    br#"{"id":"aa","version":"2"}"#,
                )])),
            ),
            ("carrier.jar", &zip_bytes(&carrier)),
        ]);
        let selected = select(&outer);
        assert_eq!(
            selected
                .iter()
                .filter_map(|candidate| candidate.jar.fabric_id.as_deref())
                .collect::<Vec<_>>(),
            vec!["aa", "carrier"]
        );
    }

    fn zip_bytes(jar: &NativeJar) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut bytes);
        for (name, payload) in &jar.resource_bytes {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(payload).unwrap();
        }
        zip.finish().unwrap();
        bytes.into_inner()
    }

    #[test]
    fn semantic_version_comparison_matches_fabric_loader() {
        let cases = [
            ("1.0-alpha", "1.0", Ordering::Less),
            ("1.0+build1", "1.0+build2", Ordering::Equal),
            ("1.0.0", "1.0", Ordering::Equal),
            ("1.0-alpha.1", "1.0-alpha.beta", Ordering::Less),
            ("1.0-alpha.2", "1.0-alpha.10", Ordering::Less),
            ("1.0-rc", "1.0-beta", Ordering::Greater),
            ("01.0", "1.0", Ordering::Equal),
            ("1.0.foo", "1.0", Ordering::Greater),
            ("1.0-X", "1.0", Ordering::Less),
            ("1.x", "1.1.1", Ordering::Greater),
            ("1.*", "1.1.1", Ordering::Less),
            ("1.0-", "1.0", Ordering::Less),
        ];
        for (left, right, expected) in cases {
            assert_eq!(
                version_cmp(Some(left), Some(right)),
                expected,
                "{left} vs {right}"
            );
        }
    }

    #[test]
    fn fabric_predicates_match_the_jdk25_oracle() {
        let mut wrong = Vec::new();
        let mut refused = Vec::new();
        for line in include_str!("nested_jars_predicate_java25.txt").lines() {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            let predicate = row["predicate"].as_str().unwrap();
            let version = row["version"].as_str().unwrap();
            let actual = fabric_predicate_matches(predicate, Some(version));
            if row.get("error").is_some() {
                if actual.is_some() {
                    wrong.push(line);
                }
            } else if actual.is_none() {
                if !wildcard_with_prerelease(predicate) {
                    refused.push(line);
                }
            } else if actual != Some(row["ok"].as_bool().unwrap()) {
                wrong.push(line);
            }
        }
        assert!(wrong.is_empty(), "wrong answers:\n{}", wrong.join("\n"));
        assert!(refused.is_empty(), "refused:\n{}", refused.join("\n"));
    }

    /// The one predicate shape Decaf refuses although Fabric evaluates it: an
    /// X-range component followed by prerelease text (`1.2.x-alpha`). Every
    /// other row Fabric accepts must get Fabric's exact answer.
    fn wildcard_with_prerelease(predicate: &str) -> bool {
        predicate.split(' ').any(|token| {
            let token = token.trim_start_matches(['>', '<', '=', '~', '^']);
            let numbers = token.split('+').next().unwrap_or("");
            // Only a well-formed prerelease (non-empty dot-separated
            // alphanumeric/hyphen identifiers) is exempt.
            numbers
                .split_once('-')
                .is_some_and(|(numbers, prerelease)| {
                    numbers
                        .split('.')
                        .any(|part| matches!(part, "x" | "X" | "*"))
                        && prerelease.split('.').all(|part| {
                            !part.is_empty()
                                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                        })
                })
        })
    }

    #[test]
    fn maven_comparable_version_examples_match_jarjar() {
        assert_eq!(maven_cmp("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(maven_cmp("1.0-alpha", "1.0"), Ordering::Less);
        assert_eq!(maven_cmp("1.0-rc1", "1.0-rc2"), Ordering::Less);
        assert_eq!(maven_cmp("1.0-SNAPSHOT", "1.0"), Ordering::Less);
        assert_eq!(maven_cmp("1.0-sp", "1.0"), Ordering::Greater);
        assert_eq!(maven_cmp("1.0", "1.0+build"), Ordering::Less);
        assert_eq!(maven_cmp("1.0-ga", "1.0"), Ordering::Equal);
        assert_eq!(maven_cmp("1.0-final", "1.0"), Ordering::Equal);
        assert_eq!(maven_cmp("1.0-cr", "1.0-rc"), Ordering::Equal);
        assert_eq!(maven_cmp("1.0a", "1.0"), Ordering::Greater);
        assert_eq!(
            maven_cmp(
                "1.0.27-26.3-neoforge-SNAPSHOT",
                "1.0.26-26.3-neoforge-SNAPSHOT"
            ),
            Ordering::Greater
        );
    }

    #[test]
    fn maven_comparison_matches_the_jdk25_oracle() {
        for line in include_str!("jarjar_version_java25.txt").lines() {
            let mut fields = line.split('|');
            if fields.next() != Some("cmp") {
                continue;
            }
            let left = fields.next().unwrap();
            let right = fields.next().unwrap();
            let expected = fields.next().unwrap().parse::<i32>().unwrap();
            assert_eq!(
                maven_cmp(left, right).cmp(&Ordering::Equal) as i32,
                expected.signum(),
                "{line}"
            );
        }
    }

    fn maven_display(range: &MavenRange) -> String {
        if let Some(recommended) = &range.recommended {
            return recommended.clone();
        }
        range
            .restrictions
            .iter()
            .map(|r| {
                format!(
                    "{}{},{}{}",
                    if r.low_inclusive { '[' } else { '(' },
                    r.low.as_deref().unwrap_or(""),
                    r.high.as_deref().unwrap_or(""),
                    if r.high_inclusive { ']' } else { ')' }
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn maven_ranges_and_restrictions_match_the_jdk25_oracle() {
        for line in include_str!("jarjar_version_java25.txt").lines() {
            let fields = line.split('|').collect::<Vec<_>>();
            match fields.first().copied() {
                Some("range") => {
                    let range = maven_range(fields[1]).unwrap();
                    assert_eq!(maven_display(&range), fields[2], "{line}");
                    assert_eq!(
                        range.recommended.as_deref().unwrap_or("null"),
                        fields[3],
                        "{line}"
                    );
                }
                Some("range-error") => {
                    assert_eq!(maven_range(fields[1]).unwrap_err(), fields[3], "{line}");
                }
                Some("restrict") => {
                    let left = maven_range(fields[1]).unwrap();
                    let right = maven_range(fields[2]).unwrap();
                    let restricted = maven_restrict(&left, &right);
                    assert_eq!(maven_display(&restricted), fields[3], "{line}");
                    assert_eq!(
                        restricted.recommended.as_deref().unwrap_or("null"),
                        fields[4],
                        "{line}"
                    );
                    if fields.len() == 7 {
                        assert_eq!(
                            maven_in_range("2", &restricted.restrictions).to_string(),
                            fields[5],
                            "{line}"
                        );
                        assert_eq!(
                            maven_in_range("1.5", &restricted.restrictions).to_string(),
                            fields[6],
                            "{line}"
                        );
                    }
                }
                _ => {}
            }
        }
    }

    #[test]
    fn jarjar_range_intersection_can_have_no_solution() {
        let empty = empty_zip();
        let outer = jar(&[
            ("META-INF/jarjar/metadata.json", br#"{"jars":[
                {"path":"META-INF/jarjar/one.jar","identifier":{"group":"g","artifact":"a"},"version":{"range":"[1,2)","artifactVersion":"1.5"}},
                {"path":"META-INF/jarjar/two.jar","identifier":{"group":"g","artifact":"a"},"version":{"range":"[3,4)","artifactVersion":"3.5"}}
            ]}"#),
            ("META-INF/jarjar/one.jar", &empty),
            ("META-INF/jarjar/two.jar", &empty),
        ]);
        assert!(select(&outer).is_empty());
        assert_eq!(
            select_checked(&outer).unwrap_err(),
            "Some mods have agreed upon an acceptable version range for : §6g:a§r, but no jar was provided which matched the range. Requested by: §eouter§r."
        );
        assert_eq!(
            outer.try_seed_host(StdHost::new()).unwrap_err(),
            "Some mods have agreed upon an acceptable version range for : §6g:a§r, but no jar was provided which matched the range. Requested by: §eouter§r."
        );
    }

    fn empty_zip() -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        zip::ZipWriter::new(&mut bytes).finish().unwrap();
        bytes.into_inner()
    }
}
