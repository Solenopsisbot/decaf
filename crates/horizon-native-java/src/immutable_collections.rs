//! `Set.of`/`Map.of`/`Map.ofEntries` and `IdentityHashMap`, as JDK 25 runs
//! them: what is exact and what no run can reproduce.
//!
//! `Set.of`, `Map.of` and `Map.ofEntries` build `ImmutableCollections`'
//! `Set12`/`SetN`/`Map1`/`MapN`. Their duplicate checks and lookups are
//! deterministic: `Set12` compares `e0.equals(e1)`, and `SetN`/`MapN` place
//! each element by `probe`, linear probing from `floorMod(hashCode, buckets)`
//! over twice as many buckets as elements, calling `equals` on each occupied
//! slot. The host found keys by marker, so `Set.of(new Key(1), new Key(1))`
//! built a two-element set instead of throwing "duplicate element:
//! Key[id=1]". This module runs those bodies and keeps each set's probe
//! layout beside it, so `contains`/`get`/`containsKey`/`containsValue`/
//! `hashCode`/`equals` call the elements' `hashCode` and `equals` as the JDK
//! does.
//!
//! Their **iteration order** is not reproducible: `SetN`/`MapN` iterators
//! start at a slot chosen from `SALT32L`, and `Set12`/`MapN` reverse when
//! `REVERSE`, both random per JVM run. So a call that shows the order of a
//! `Set.of`/`Map.of` with two or more elements (`iterator`, `forEach`,
//! `toString`, `stream`, passing it to a host model) is counted.
//!
//! The varargs factories (`List.of(E...)`, `Set.of(E...)`,
//! `Map.ofEntries(Entry...)`) used to keep the varargs array as one element;
//! their elements are now the array's. `Set.copyOf`/`Map.copyOf` of jar
//! elements (a `HashSet` copy, then `Set.of`) are counted.
//!
//! An `IdentityHashMap` compares keys with `==`. A jar object's identity is
//! its marker, but two equal strings or boxes may or may not be the same
//! object, so a lookup by a string or box whose equal value is a key is
//! counted, as is the map's iteration (identity hashes).

use super::*;
use object_methods::Equality;

// The classes `Set.of`/`Map.of` make (`java_immutable.rs`): one or two
// elements (one entry) are a `Set12` (`Map1`), which compares in order, any
// other count a probed `SetN` (`MapN`).
const SETS: &[&str] = &[
    "java/util/ImmutableCollections$SetN",
    "java/util/ImmutableCollections$Set12",
];
const MAPS: &[&str] = &[
    "java/util/ImmutableCollections$MapN",
    "java/util/ImmutableCollections$Map1",
];
const IDENTITY_MAP: &str = "java/util/IdentityHashMap";
const IDENTITY_SET: &str = "java/util/Collections$SetFromMap";

/// A `SetN`/`MapN` probe table: each bucket's element (a map's key), the
/// value itself, so a handle keeps the origin that says which object it is
/// (`Set.of(h, h)` throws). `unknown`: an element with an identity hash sits
/// at a slot no run can name.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ProbeTable {
    slots: Vec<Option<Value>>,
    unknown: bool,
}

/// Where `probe` ends.
enum Probe {
    Found(usize),
    Empty(usize),
    Unknown,
}

/// Comparison of two values under Java reference identity (`==`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IdentityMatch {
    Same,
    Different,
    Unknown,
}

/// Key and value allocation provenance tracked for one `IdentityHashMap` (or
/// `IdentityHashSet`), stamped with the host `CollectionStore` generation at
/// which those tokens were stored. Any mutation outside this module (e.g.
/// `Map.Entry.setValue`, `Iterator.remove`, `replaceAll`, view mutators, or
/// `Collections.synchronizedMap`) advances the host generation and
/// automatically invalidates stale tokens on read.
#[derive(Clone, Debug, Default)]
pub(crate) struct IdentityCollectionTokens {
    generation: Option<u64>,
    keys: Vec<reference_identity::CallIdentity>,
    values: Vec<reference_identity::CallIdentity>,
}

/// Classification of an identity collection or view over one for exact
/// `equals`/`hashCode` semantics.
enum IdentityCollectionKind {
    /// `IdentityHashMap` (directly or through `unmodifiableMap`/`synchronizedMap`).
    Map(Value),
    /// `Sets.newIdentityHashSet()` (`Collections$SetFromMap` standalone, or
    /// wrapped by `unmodifiableSet`/`synchronizedSet`).
    Set(Value),
    /// `IdentityHashMap.keySet()` (or wrapped by `unmodifiableSet`/`synchronizedSet`).
    KeySet(Value),
    /// `IdentityHashMap.entrySet()` (or wrapped by `unmodifiableSet`/`synchronizedSet`).
    EntrySet(Value),
    /// `IdentityHashMap.values()` (or wrapped by `unmodifiableCollection`/`synchronizedCollection`).
    Values(Value),
}

/// Per-invocation identity epoch and allocation provenance for
/// `IdentityHashMap` / `IdentityHashSet`.
///
/// Any counted unknown call that is not provably unable to run jar code or
/// touch Java state bumps `epoch`. Each freshly constructed identity
/// collection stores its `(run_id, known_epoch)` inside `CollectionStore` so
/// removal, epoch bumps, and GC prune it automatically; derived handles
/// (views, live entries, iterators, list iterators, enumerations, spliterators,
/// streams and their stages) resolve their root directly from their own
/// host/runtime state with no separate side map.
#[derive(Clone, Debug, Default)]
pub(crate) struct IdentityCollections {
    run_id: u64,
    epoch: u64,
    tokens: std::collections::BTreeMap<String, IdentityCollectionTokens>,
}

fn same_handle(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Host(left), Value::Host(right)) => left.same_object(right) == Some(true),
        _ => a == b && crate::jar_object::object_key(a).is_some(),
    }
}

/// The JVM's helpful message for `o.equals(..)` on a null `o`
/// (`Set12.contains`, `Map1.get`/`containsKey`/`containsValue`).
fn null_receiver() -> NativeError {
    thrown(
        "java/lang/NullPointerException",
        Some("Cannot invoke \"Object.equals(Object)\" because \"o\" is null".into()),
    )
}

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

/// The synthetic field a collection marker carries once a counted call may have changed it
/// (#464): every later call on it is counted too.
const UNCERTAIN_FIELD: &str = "#collectionUncertain";

/// The collection members that may change their collection.
fn mutates(name: &str) -> bool {
    matches!(
        name,
        "put"
            | "putAll"
            | "putIfAbsent"
            | "remove"
            | "replace"
            | "replaceAll"
            | "compute"
            | "computeIfAbsent"
            | "computeIfPresent"
            | "merge"
            | "add"
            | "addAll"
            | "removeAll"
            | "retainAll"
            | "removeIf"
            | "clear"
            | "setValue"
    )
}

impl<H: Host> Runtime<'_, H> {
    fn uncertain_key(collection: &Value) -> Option<String> {
        expect_ref(collection).map(|marker| format!("{marker}|{UNCERTAIN_FIELD}"))
    }

    fn is_identity_collection(&self, collection: &Value) -> bool {
        matches!(
            self.host_collection_class(collection).as_deref(),
            Some(IDENTITY_MAP | IDENTITY_SET)
        ) && !self.host.is_set_from_map(collection)
    }

    fn collection_known_epoch(&self, marker: &str) -> u64 {
        self.host
            .identity_known_epoch(marker)
            .filter(|&(run_id, _)| run_id == self.identity_collections.run_id)
            .map_or(0, |(_, epoch)| epoch)
    }

    /// Record that a freshly constructed identity collection starts with fully
    /// known contents at the current runtime epoch.
    pub(crate) fn note_fresh_identity_collection(&mut self, collection: &Value) {
        if let Some(marker) = crate::jar_object::object_key(collection) {
            let epoch = self.identity_collections.epoch;
            if epoch > 0 {
                self.host
                    .set_identity_known_epoch(marker, self.identity_collections.run_id, epoch);
            }
            self.identity_collections.tokens.remove(marker);
        }
    }

    /// Advance the invocation's identity epoch after a counted unknown call
    /// that could have run jar bytecode or mutated Java state, invalidating
    /// every pre-existing identity collection (static, deep, captured, or
    /// host-held) without any reachability walk.
    pub(crate) fn bump_identity_epoch(&mut self) {
        if self.identity_collections.run_id == 0 {
            self.identity_collections.run_id = self.host.next_identity_run();
        } else {
            self.host.clear_identity_known_epochs();
        }
        self.identity_collections.epoch = self.identity_collections.epoch.saturating_add(1);
        self.identity_collections.tokens.clear();
    }

    /// Whether a counted unknown call is provably unable to run jar bytecode or
    /// touch any Java collection or static state: a host-modelled `final` pure
    /// value-domain JDK class (not present in the jar or linked jars) whose
    /// receiver is not a jar object and whose receiver and arguments contain no
    /// jar objects, lambdas, collections, arrays, or host handles at all.
    pub(crate) fn counted_call_is_identity_inert(
        &self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> bool {
        if !self.host.models_class(&r.owner)
            || self.has_class(&r.owner)
            || self.linked.contains_key(&r.owner)
            || receiver.is_some_and(crate::jar_object::is_object)
        {
            return false;
        }
        // Only `final` JDK value classes (no jar subclassing possible; excludes
        // non-final value types such as `BigInteger` / `BigDecimal`).
        let pure_value_owner = matches!(
            r.owner.as_str(),
            "java/lang/Boolean"
                | "java/lang/Byte"
                | "java/lang/Short"
                | "java/lang/Character"
                | "java/lang/Integer"
                | "java/lang/Long"
                | "java/lang/Float"
                | "java/lang/Double"
                | "java/lang/Math"
                | "java/lang/StrictMath"
                | "java/lang/String"
                | "java/math/MathContext"
                | "java/math/RoundingMode"
                | "java/time/Instant"
                | "java/time/Duration"
                | "java/time/LocalDate"
                | "java/time/LocalTime"
                | "java/time/LocalDateTime"
                | "java/time/Period"
                | "java/time/Year"
                | "java/time/YearMonth"
                | "java/time/MonthDay"
                | "java/time/DayOfWeek"
                | "java/time/Month"
                | "java/time/ZoneOffset"
                | "java/util/UUID"
        );
        if !pure_value_owner {
            return false;
        }
        let is_inert_operand = |value: &Value| {
            !crate::jar_object::is_object(value)
                && match value {
                    Value::Void
                    | Value::Int(_)
                    | Value::Long(_)
                    | Value::Float(_)
                    | Value::Boxed(_)
                    | Value::Ref(None) => true,
                    Value::Ref(Some(text)) => value_class::is_plain_string(text),
                    Value::Host(_) => false,
                }
        };
        receiver.is_none_or(is_inert_operand) && args.iter().all(is_inert_operand)
    }

    /// Whether a counted call may have changed `collection` (#464), either
    /// directly or via a runtime-wide identity epoch bump.
    pub(crate) fn collection_uncertain(&self, collection: &Value) -> bool {
        if Self::uncertain_key(collection).is_some_and(|key| self.fields.contains_key(&key)) {
            return true;
        }
        let current_epoch = self.identity_collections.epoch;
        if current_epoch == 0 || !self.is_identity_collection(collection) {
            return false;
        }
        let known = crate::jar_object::object_key(collection)
            .map(|marker| self.collection_known_epoch(marker))
            .unwrap_or(0);
        known < current_epoch
    }

    /// Resolve an identity collection or ANY handle derived from one (views,
    /// live/wrapped entries, iterators, list iterators, enumerations,
    /// spliterators, streams and their intermediate stages,
    /// `Collections.unmodifiable*`/`synchronized*`/`reversed`/`subList`
    /// wrappers) directly from the handle's own host/runtime state.
    pub(crate) fn identity_root_marker(&self, value: &Value) -> Option<String> {
        self.identity_root_marker_depth(value, 0)
    }

    fn identity_root_marker_depth(&self, value: &Value, depth: usize) -> Option<String> {
        if depth >= MAX_SUPERCLASS_WALK {
            return None;
        }
        if self.is_identity_collection(value) {
            return crate::jar_object::object_key(value).map(str::to_owned);
        }
        // Live or wrapped `Map.Entry` handles (from `forEach`, `iterator().next()`,
        // `toArray(T[])`, `compute`/`merge`, etc.). Detached copies (`SimpleEntry`,
        // `SimpleImmutableEntry`, `KeyValueHolder`) have neither link and return `None`.
        if let Some(inner) = self.host.unwrapped_view_entry(value) {
            return self.identity_root_marker_depth(&inner, depth + 1);
        }
        if let Some(map_marker) = self.host.map_entry_source(value) {
            return self.identity_root_marker_depth(
                &crate::jar_object::stored_value(&map_marker),
                depth + 1,
            );
        }
        if let Value::Host(handle) = value {
            match handle.kind() {
                crate::host_ref::HostKind::Iterator => {
                    let origin = self.host.host_iterator_origin(handle.key())?;
                    return self.identity_root_marker_depth(&origin, depth + 1);
                }
                crate::host_ref::HostKind::ListIterator => {
                    let origin = self.host.list_iterator_source(handle.key())?;
                    return self.identity_root_marker_depth(&origin, depth + 1);
                }
                crate::host_ref::HostKind::Enumeration => {
                    let origin = self.host.enumeration_source(handle.key())?;
                    return self.identity_root_marker_depth(&origin, depth + 1);
                }
                crate::host_ref::HostKind::Spliterator => {
                    let origin = self.pipelines.splits.source_of(handle.key())?;
                    return self.identity_root_marker_depth(&origin, depth + 1);
                }
                _ => {}
            }
        }
        if let Some(wrapped) = self.host.synchronized_wrapped(value) {
            return self.identity_root_marker_depth(&wrapped, depth + 1);
        }
        if let Some(source_map) = self.host.set_from_map_source(value) {
            return self.identity_root_marker_depth(&source_map, depth + 1);
        }
        let marker = expect_ref(value)?;
        if let Some(inner) = self.host.unmodifiable_view_source(marker) {
            return self
                .identity_root_marker_depth(&crate::jar_object::stored_value(&inner), depth + 1);
        }
        if let Some(source) = self.host.view_copy_source(marker) {
            return self
                .identity_root_marker_depth(&crate::jar_object::stored_value(&source), depth + 1);
        }
        if let Some(base) = self.host.reversed_view_source(marker) {
            return self
                .identity_root_marker_depth(&crate::jar_object::stored_value(&base), depth + 1);
        }
        if let Some(link) = self.host.sub_list_link(marker) {
            return self.identity_root_marker_depth(
                &crate::jar_object::stored_value(link.parent()),
                depth + 1,
            );
        }
        if self.is_live_stream(marker) {
            let root = self.pipelines.root_stream(marker);
            for origin in self.pipelines.source_origins(root).into_iter().flatten() {
                if let Some(found) = self.identity_root_marker_depth(&origin, depth + 1) {
                    return Some(found);
                }
            }
            if let Some(origin) = self
                .streams
                .get(root)
                .and_then(|entry| entry.origin.as_ref())
            {
                return self.identity_root_marker_depth(origin, depth + 1);
            }
        }
        None
    }

    /// Resolve `value` (an identity collection or ANY handle derived from one)
    /// to its root identity collection whenever that root is uncertain.
    pub(crate) fn uncertain_identity_root(&self, value: &Value, r: &MethodRef) -> Option<Value> {
        if r.name == "getClass" {
            return None;
        }
        // `UnmodifiableEntry.setValue` always throws `UnsupportedOperationException`
        // regardless of backing-map contents.
        if r.name == "setValue" && self.host.unwrapped_view_entry(value).is_some() {
            return None;
        }
        if self.collection_uncertain(value) {
            return Some(value.clone());
        }
        let root_marker = self.identity_root_marker(value)?;
        let root_value = crate::jar_object::stored_value(&root_marker);
        self.collection_uncertain(&root_value).then_some(root_value)
    }

    /// Count a call dispatched against an uncertain identity collection or any
    /// handle derived from one (views, entries, iterators, spliterators,
    /// streams, wrappers) before any model can answer from stale contents.
    #[inline(never)]
    pub(crate) fn count_if_uncertain_identity_operand(
        &mut self,
        r: &MethodRef,
        op: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if matches!(op, 0xb6 | 0xb9)
            && let Some(receiver) = receiver
            && let Some(root) = self.uncertain_identity_root(receiver, r)
        {
            return self.count_identity_call(r, &root).map(Some);
        }
        let reads_collection_arg = matches!(
            r.name.as_str(),
            "equals"
                | "hashCode"
                | "hash"
                | "putAll"
                | "addAll"
                | "containsAll"
                | "removeAll"
                | "retainAll"
                | "copyOf"
                | "<init>"
                | "stream"
                | "concat"
                | "spliterator"
                | "spliteratorUnknownSize"
                | "iterator"
                | "enumeration"
                | "size"
                | "isEmpty"
                | "toArray"
                | "elementsEqual"
                | "getOnlyElement"
                | "getFirst"
                | "getLast"
        ) || (receiver.is_none()
            && matches!(r.name.as_str(), "toString" | "valueOf"));
        if reads_collection_arg
            && let Some(root) = args
                .iter()
                .find_map(|arg| self.uncertain_identity_root(arg, r))
        {
            if matches!(
                r.name.as_str(),
                "putAll" | "addAll" | "removeAll" | "retainAll" | "<init>"
            ) && let Some(receiver) = receiver
            {
                self.mark_collection_uncertain(receiver);
            }
            return self.count_identity_call(r, &root).map(Some);
        }
        Ok(None)
    }

    fn identity_collection_kind(&self, value: &Value) -> Option<IdentityCollectionKind> {
        let (root_view, _) = self.root_map_of_view(value.clone());
        match self.host_collection_class(&root_view).as_deref() {
            Some(IDENTITY_MAP) => return Some(IdentityCollectionKind::Map(root_view)),
            Some(IDENTITY_SET) if !self.host.is_set_from_map(&root_view) => {
                return Some(IdentityCollectionKind::Set(root_view));
            }
            _ => {}
        }
        let view = expect_ref(&root_view)?;
        let source = self.host.view_copy_source(view)?;
        let (source_root, _) = self.root_map_of_view(crate::jar_object::stored_value(&source));
        if self.host_collection_class(&source_root).as_deref() != Some(IDENTITY_MAP) {
            return None;
        }
        match self.host.view_copy_kind(view).as_deref() {
            Some("keySet") => Some(IdentityCollectionKind::KeySet(source_root)),
            Some("entrySet") => Some(IdentityCollectionKind::EntrySet(source_root)),
            Some("values") => Some(IdentityCollectionKind::Values(source_root)),
            _ => None,
        }
    }

    /// Exact JDK 25 `hashCode()` for `IdentityHashMap`, `IdentityHashSet`
    /// (`Sets.newIdentityHashSet()`), and their `keySet`/`entrySet`/`values`
    /// views at every size: `0` when empty or every hashed element is `null`,
    /// otherwise `Some(None)` so the caller counts instead of falling back to
    /// value-based `AbstractMap`/`AbstractSet` hashing. Returns `None` for
    /// non-identity collections.
    pub(crate) fn identity_collection_hash(
        &self,
        value: &Value,
    ) -> Result<Option<Option<i32>>, NativeError> {
        let Some(kind) = self.identity_collection_kind(value) else {
            return Ok(None);
        };
        match kind {
            IdentityCollectionKind::Values(_) => Ok(Some(None)),
            IdentityCollectionKind::Map(root) | IdentityCollectionKind::EntrySet(root) => {
                if self.collection_uncertain(&root) {
                    return Ok(Some(None));
                }
                let entries = self.collection_elements(&root)?;
                if entries.iter().all(|v| *v == Value::Ref(None)) {
                    Ok(Some(Some(0)))
                } else {
                    Ok(Some(None))
                }
            }
            IdentityCollectionKind::Set(root) => {
                if self.collection_uncertain(&root) {
                    return Ok(Some(None));
                }
                let elements = self.collection_elements(&root)?;
                if elements.iter().all(|v| *v == Value::Ref(None)) {
                    Ok(Some(Some(0)))
                } else {
                    Ok(Some(None))
                }
            }
            IdentityCollectionKind::KeySet(root) => {
                if self.collection_uncertain(&root) {
                    return Ok(Some(None));
                }
                let entries = self.collection_elements(&root)?;
                if entries.iter().step_by(2).all(|v| *v == Value::Ref(None)) {
                    Ok(Some(Some(0)))
                } else {
                    Ok(Some(None))
                }
            }
        }
    }

    /// Compare `root_a` (an `IdentityHashMap`) with any host `Map` `other`
    /// under `IdentityHashMap.equals` (`==` on both keys and values).
    fn identity_map_equals_host_map(
        &mut self,
        root_a: &Value,
        other: &Value,
        r: &MethodRef,
    ) -> Result<bool, NativeError> {
        if self.collection_uncertain(root_a) || self.uncertain_identity_root(other, r).is_some() {
            self.host.unknown_operand_call(r)?;
            return Ok(false);
        }
        let other_id_root = match self.identity_collection_kind(other) {
            Some(IdentityCollectionKind::Map(root_b)) => Some(root_b),
            _ => None,
        };
        if let Some(root_b) = other_id_root.as_ref()
            && same_handle(root_a, root_b)
        {
            return Ok(true);
        }
        let mine = self.collection_elements(root_a)?;
        let theirs = self.collection_elements(other)?;
        if mine.len() != theirs.len() {
            return Ok(false);
        }
        if mine.is_empty() {
            return Ok(true);
        }
        let n = mine.len() / 2;
        let my_keys: Vec<Value> = mine.iter().step_by(2).cloned().collect();
        let my_vals: Vec<Value> = mine.iter().skip(1).step_by(2).cloned().collect();
        let my_marker = crate::jar_object::object_key(root_a).map(str::to_owned);
        let (my_kt, my_vt) = self.load_identity_tokens(my_marker.as_deref(), n);
        let (their_kt, their_vt) = if let Some(root_b) = other_id_root.as_ref() {
            let marker_b = crate::jar_object::object_key(root_b).map(str::to_owned);
            self.load_identity_tokens(marker_b.as_deref(), n)
        } else {
            (vec![None; n], vec![None; n])
        };
        for (i, [tk, tv]) in theirs.as_chunks::<2>().0.iter().enumerate() {
            let Some(pos) = self.identity_position(&my_keys, &my_kt, tk, their_kt[i].as_ref())
            else {
                self.host.unknown_operand_call(r)?;
                return Ok(false);
            };
            let Some(idx) = pos else {
                return Ok(false);
            };
            match self.identity_match(tv, their_vt[i].as_ref(), &my_vals[idx], my_vt[idx].as_ref())
            {
                IdentityMatch::Same => {}
                IdentityMatch::Different => return Ok(false),
                IdentityMatch::Unknown => {
                    self.host.unknown_operand_call(r)?;
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Compare a non-identity host `Map` `a` against an `IdentityHashMap`
    /// `root_b` under `AbstractMap.equals`: for each `(k, v)` in `a`, look up
    /// `k` in `root_b` by `==` (`idMap.get(k)`) and compare values with
    /// `Objects.equals`.
    fn host_map_equals_identity_map(
        &mut self,
        a: &Value,
        root_b: &Value,
        r: &MethodRef,
    ) -> Result<bool, NativeError> {
        if self.collection_uncertain(root_b) {
            self.host.unknown_operand_call(r)?;
            return Ok(false);
        }
        let mine = self.collection_elements(a)?;
        let theirs = self.collection_elements(root_b)?;
        if mine.len() != theirs.len() {
            return Ok(false);
        }
        if mine.is_empty() {
            return Ok(true);
        }
        let n = theirs.len() / 2;
        let their_keys: Vec<Value> = theirs.iter().step_by(2).cloned().collect();
        let their_vals: Vec<Value> = theirs.iter().skip(1).step_by(2).cloned().collect();
        let marker_b = crate::jar_object::object_key(root_b).map(str::to_owned);
        let (their_kt, _) = self.load_identity_tokens(marker_b.as_deref(), n);
        for [mk, mv] in mine.as_chunks::<2>().0 {
            let Some(pos) = self.identity_position(&their_keys, &their_kt, mk, None) else {
                self.host.unknown_operand_call(r)?;
                return Ok(false);
            };
            let Some(idx) = pos else {
                return Ok(false);
            };
            if !self.objects_equals(mv, &their_vals[idx])? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Exact JDK 25 `equals(Object)` whenever either operand is an identity
    /// collection or a view over one (`IdentityHashMap`, `IdentityHashSet`,
    /// `keySet`, `entrySet`, `values`, and `unmodifiable*`/`synchronized*`
    /// wrappers), in both directions (including `HashMap.equals(IdentityHashMap)`
    /// and `IdentityHashMap.equals(HashMap)`). Returns `Ok(None)` when neither
    /// operand requires identity-collection equality semantics.
    pub(crate) fn identity_collection_equals(
        &mut self,
        a: &Value,
        b: &Value,
        r: &MethodRef,
    ) -> Result<Option<bool>, NativeError> {
        let kind_a = self.identity_collection_kind(a);
        let kind_b = self.identity_collection_kind(b);
        if kind_a.is_none() && kind_b.is_none() {
            return Ok(None);
        }
        match kind_a {
            Some(IdentityCollectionKind::Values(root_a)) => {
                if self.collection_uncertain(&root_a) {
                    self.host.unknown_operand_call(r)?;
                    return Ok(Some(false));
                }
                Ok(Some(same_handle(a, b)))
            }
            Some(IdentityCollectionKind::Map(root_a)) => {
                if self.collection_uncertain(&root_a) {
                    self.host.unknown_operand_call(r)?;
                    return Ok(Some(false));
                }
                if same_handle(a, b) {
                    return Ok(Some(true));
                }
                if *b == Value::Ref(None) {
                    return Ok(Some(false));
                }
                match self.host_collection_kind(b) {
                    Some(object_methods::CollectionKind::Map) => {
                        self.identity_map_equals_host_map(&root_a, b, r).map(Some)
                    }
                    Some(_) => Ok(Some(false)),
                    None if !matches!(self.equality_of(b), Equality::Jar { .. })
                        && !crate::jar_object::is_object(b) =>
                    {
                        Ok(Some(false))
                    }
                    None => match self.known_instance_of(b, "java/util/Map") {
                        Some(false) => Ok(Some(false)),
                        Some(true) | None => {
                            self.host.unknown_operand_call(r)?;
                            Ok(Some(false))
                        }
                    },
                }
            }
            Some(IdentityCollectionKind::Set(root_a))
            | Some(IdentityCollectionKind::KeySet(root_a)) => {
                if self.collection_uncertain(&root_a) {
                    self.host.unknown_operand_call(r)?;
                    return Ok(Some(false));
                }
                if same_handle(a, b) {
                    return Ok(Some(true));
                }
                if *b == Value::Ref(None) {
                    return Ok(Some(false));
                }
                match self.host_collection_kind(b) {
                    Some(object_methods::CollectionKind::Set) => {}
                    Some(_) => return Ok(Some(false)),
                    None if !matches!(self.equality_of(b), Equality::Jar { .. })
                        && !crate::jar_object::is_object(b) =>
                    {
                        return Ok(Some(false));
                    }
                    None => match self.known_instance_of(b, "java/util/Set") {
                        Some(false) => return Ok(Some(false)),
                        Some(true) | None => {
                            self.host.unknown_operand_call(r)?;
                            return Ok(Some(false));
                        }
                    },
                }
                if self.uncertain_identity_root(b, r).is_some() {
                    self.host.unknown_operand_call(r)?;
                    return Ok(Some(false));
                }
                let root_b_same_kind = match kind_b {
                    Some(IdentityCollectionKind::Set(rb))
                    | Some(IdentityCollectionKind::KeySet(rb)) => Some(rb),
                    _ => None,
                };
                if let Some(rb) = root_b_same_kind.as_ref()
                    && same_handle(&root_a, rb)
                {
                    return Ok(Some(true));
                }
                let my_keys: Vec<Value> =
                    if self.host_collection_class(&root_a).as_deref() == Some(IDENTITY_MAP) {
                        self.collection_elements(&root_a)?
                            .into_iter()
                            .step_by(2)
                            .collect()
                    } else {
                        self.collection_elements(&root_a)?
                    };
                let their_keys = self.collection_elements(b)?;
                if my_keys.len() != their_keys.len() {
                    return Ok(Some(false));
                }
                if my_keys.is_empty() {
                    return Ok(Some(true));
                }
                let marker_a = crate::jar_object::object_key(&root_a).map(str::to_owned);
                let (my_kt, _) = self.load_identity_tokens(marker_a.as_deref(), my_keys.len());
                let their_kt = if let Some(rb) = root_b_same_kind.as_ref() {
                    let marker_b = crate::jar_object::object_key(rb).map(str::to_owned);
                    self.load_identity_tokens(marker_b.as_deref(), their_keys.len())
                        .0
                } else {
                    vec![None; their_keys.len()]
                };
                for (i, item) in their_keys.iter().enumerate() {
                    let Some(pos) =
                        self.identity_position(&my_keys, &my_kt, item, their_kt[i].as_ref())
                    else {
                        self.host.unknown_operand_call(r)?;
                        return Ok(Some(false));
                    };
                    if pos.is_none() {
                        return Ok(Some(false));
                    }
                }
                Ok(Some(true))
            }
            Some(IdentityCollectionKind::EntrySet(root_a)) => {
                if self.collection_uncertain(&root_a) {
                    self.host.unknown_operand_call(r)?;
                    return Ok(Some(false));
                }
                if same_handle(a, b) {
                    return Ok(Some(true));
                }
                if *b == Value::Ref(None) {
                    return Ok(Some(false));
                }
                match self.host_collection_kind(b) {
                    Some(object_methods::CollectionKind::Set) => {}
                    Some(_) => return Ok(Some(false)),
                    None if !matches!(self.equality_of(b), Equality::Jar { .. })
                        && !crate::jar_object::is_object(b) =>
                    {
                        return Ok(Some(false));
                    }
                    None => match self.known_instance_of(b, "java/util/Set") {
                        Some(false) => return Ok(Some(false)),
                        Some(true) | None => {
                            self.host.unknown_operand_call(r)?;
                            return Ok(Some(false));
                        }
                    },
                }
                if self.uncertain_identity_root(b, r).is_some() {
                    self.host.unknown_operand_call(r)?;
                    return Ok(Some(false));
                }
                if let Some(IdentityCollectionKind::EntrySet(root_b)) = kind_b {
                    return self
                        .identity_map_equals_host_map(&root_a, &root_b, r)
                        .map(Some);
                }
                let mine = self.collection_elements(&root_a)?;
                let their_entries = self.collection_elements(b)?;
                if mine.len() / 2 != their_entries.len() {
                    return Ok(Some(false));
                }
                if their_entries.is_empty() {
                    return Ok(Some(true));
                }
                let n = mine.len() / 2;
                let my_keys: Vec<Value> = mine.iter().step_by(2).cloned().collect();
                let my_vals: Vec<Value> = mine.iter().skip(1).step_by(2).cloned().collect();
                let marker_a = crate::jar_object::object_key(&root_a).map(str::to_owned);
                let (my_kt, my_vt) = self.load_identity_tokens(marker_a.as_deref(), n);
                for entry in &their_entries {
                    let Some((ek, ev)) = self.host.map_entry_parts(entry) else {
                        if crate::jar_object::is_object(entry) {
                            self.host.unknown_operand_call(r)?;
                        }
                        return Ok(Some(false));
                    };
                    let Some(pos) = self.identity_position(&my_keys, &my_kt, &ek, None) else {
                        self.host.unknown_operand_call(r)?;
                        return Ok(Some(false));
                    };
                    let Some(idx) = pos else {
                        return Ok(Some(false));
                    };
                    match self.identity_match(&ev, None, &my_vals[idx], my_vt[idx].as_ref()) {
                        IdentityMatch::Same => {}
                        IdentityMatch::Different => return Ok(Some(false)),
                        IdentityMatch::Unknown => {
                            self.host.unknown_operand_call(r)?;
                            return Ok(Some(false));
                        }
                    }
                }
                Ok(Some(true))
            }
            None => match kind_b {
                // Reverse direction: `HashMap.equals(IdentityHashMap)` (and any
                // non-identity host `Map` comparing `.equals(identityMap)`).
                Some(IdentityCollectionKind::Map(root_b))
                    if self.host_collection_kind(a)
                        == Some(object_methods::CollectionKind::Map) =>
                {
                    self.host_map_equals_identity_map(a, &root_b, r).map(Some)
                }
                Some(
                    IdentityCollectionKind::Set(root_b)
                    | IdentityCollectionKind::KeySet(root_b)
                    | IdentityCollectionKind::EntrySet(root_b),
                ) if self.collection_uncertain(&root_b) => {
                    self.host.unknown_operand_call(r)?;
                    Ok(Some(false))
                }
                _ => Ok(None),
            },
        }
    }

    /// Mark `collection` uncertain and drop any tracked identity provenance
    /// tokens so every later read or write on it is counted.
    pub(crate) fn mark_collection_uncertain(&mut self, collection: &Value) {
        if let Some(key) = Self::uncertain_key(collection) {
            self.fields.insert(key, Value::Int(1));
        }
        if let Some(marker) = crate::jar_object::object_key(collection) {
            self.identity_collections.tokens.remove(marker);
        }
    }

    /// Walk through any chain of `Collections.unmodifiable*` and
    /// `Collections.synchronized*` wrappers to the underlying root map.
    /// Also reports whether any unmodifiable wrapper was traversed.
    pub(crate) fn root_map_of_view(&self, mut map: Value) -> (Value, bool) {
        let mut saw_unmodifiable = false;
        for _ in 0..MAX_SUPERCLASS_WALK {
            if let Some(wrapped) = self.host.synchronized_wrapped(&map) {
                map = wrapped;
                continue;
            }
            if let Some(marker) = expect_ref(&map)
                && let Some(inner) = self.host.unmodifiable_view_source(marker)
            {
                saw_unmodifiable = true;
                map = crate::jar_object::stored_value(&inner);
                continue;
            }
            break;
        }
        (map, saw_unmodifiable)
    }

    /// Concrete class name of a root map behind an `entrySet` view, including
    /// non-`java/util` maps such as Gson's `LinkedTreeMap` and Guava's maps.
    pub(crate) fn map_root_class(&self, map: &Value) -> Option<String> {
        self.host_collection_class(map).or_else(|| {
            let marker = expect_ref(map)?;
            let class = crate::jar_object::key_class(marker)
                .or_else(|| marker.split_once('#').map(|(class, _)| class))?;
            (!class.is_empty()).then(|| class.to_owned())
        })
    }

    /// Load `(key_tokens, value_tokens)` for `marker`, padded to `len`. Drops
    /// stale tokens automatically whenever the identity epoch moved or the host
    /// collection's generation changed outside this module.
    fn load_identity_tokens(
        &self,
        marker: Option<&str>,
        len: usize,
    ) -> (
        Vec<reference_identity::CallIdentity>,
        Vec<reference_identity::CallIdentity>,
    ) {
        let Some(marker) = marker else {
            return (vec![None; len], vec![None; len]);
        };
        let current_epoch = self.identity_collections.epoch;
        if current_epoch > 0 {
            let known = self.collection_known_epoch(marker);
            if known < current_epoch {
                return (vec![None; len], vec![None; len]);
            }
        }
        let current_gen = self.host.collection_generation(marker);
        let (mut keys, mut values) = self
            .identity_collections
            .tokens
            .get(marker)
            .filter(|entry| entry.generation == current_gen)
            .map(|entry| (entry.keys.clone(), entry.values.clone()))
            .unwrap_or_default();
        keys.resize(len, None);
        values.resize(len, None);
        (keys, values)
    }

    /// Store `(keys, values)` for `marker` stamped with its current host
    /// collection generation.
    fn store_identity_tokens(
        &mut self,
        marker: Option<String>,
        keys: Vec<reference_identity::CallIdentity>,
        values: Vec<reference_identity::CallIdentity>,
    ) {
        let Some(marker) = marker else {
            return;
        };
        let generation = self.host.collection_generation(&marker);
        self.identity_collections.tokens.insert(
            marker,
            IdentityCollectionTokens {
                generation,
                keys,
                values,
            },
        );
        self.host
            .note_identity_token_table_len(self.identity_collections.tokens.len());
    }

    /// Count `r`, an identity-collection call whose answer depends on an identity the runtime
    /// doesn't keep, and answer its neutral value. Never fall through to the rest of the
    /// dispatch: a counted call must not also run (#464). A counted mutation leaves `changed`
    /// uncertain, so its later calls are counted rather than answered from contents the call may
    /// have changed.
    fn count_identity_call(
        &mut self,
        r: &MethodRef,
        changed: &Value,
    ) -> Result<Value, NativeError> {
        let neutral = self.host.unknown_operand_call(r)?;
        if mutates(&r.name)
            && let Some(key) = Self::uncertain_key(changed)
        {
            self.fields.insert(key, Value::Int(1));
        }
        Ok(neutral)
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `probe(pe)`: `floorMod(pe.hashCode(), buckets)`, then `pe.equals(ee)`
    /// slot by slot until an empty slot.
    fn probe(&mut self, table: &ProbeTable, key: &Value) -> Result<Probe, NativeError> {
        let hash = self.java_hash(key)?;
        let buckets = table.slots.len();
        let Some(hash) = hash.filter(|_| !table.unknown) else {
            // Without a position, only an answer no probe order can change:
            // a pure key's membership, or a key that keeps `Object.equals`.
            let pure = self.pure_equality(key);
            let identity = matches!(
                self.equality_of(key),
                Equality::Identity | Equality::Platform
            );
            if !pure && !identity {
                // A handle: its row's `equals` against every slot. Equal
                // handles hash alike, so Java's probe from the key's slot
                // meets the equal one before an empty slot, and the same
                // object (its origin, kept in the slot) is proved equal. A
                // chat component is not: it changes in place, and its slot
                // keeps the hash it had when the set was made (#193).
                if let Value::Host(handle) = key
                    && handle.kind() != crate::host_ref::HostKind::Component
                {
                    for (slot, stored) in table.slots.iter().enumerate() {
                        let Some(stored) = stored else { continue };
                        if handle.equals(stored, &mut object_methods::RuntimeNested::new(self))?
                            == Some(true)
                        {
                            return Ok(Probe::Found(slot));
                        }
                    }
                }
                return Ok(Probe::Unknown);
            }
            for (slot, stored) in table.slots.iter().enumerate() {
                let Some(stored) = stored else { continue };
                if object_methods::identical(stored, key)
                    || (pure && self.java_equals(key, stored)?)
                {
                    return Ok(Probe::Found(slot));
                }
            }
            return Ok(Probe::Unknown);
        };
        if buckets == 0 {
            return Ok(Probe::Empty(0));
        }
        let mut index = i64::from(hash).rem_euclid(buckets as i64) as usize;
        loop {
            let Some(stored) = &table.slots[index] else {
                return Ok(Probe::Empty(index));
            };
            if self.java_equals(key, stored)? {
                return Ok(Probe::Found(index));
            }
            index = (index + 1) % buckets;
        }
    }

    /// Lay out `keys` as `SetN(input)`/`MapN(input)` does, throwing on a
    /// duplicate (`duplicate element: e` / `duplicate key: k`).
    fn lay_out(&mut self, keys: &[Value], what: &str) -> Result<ProbeTable, NativeError> {
        let mut table = ProbeTable {
            slots: vec![None; keys.len() * 2],
            unknown: false,
        };
        for key in keys {
            if *key == Value::Ref(None) {
                return Err(thrown("java/lang/NullPointerException", None));
            }
            match self.probe(&table, key)? {
                Probe::Found(_) => {
                    let text = self.value_of_text(key, "Ljava/lang/Object;", "", "of")?;
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        Some(format!("duplicate {what}: {text}")),
                    ));
                }
                Probe::Empty(slot) => table.slots[slot] = Some(key.clone()),
                Probe::Unknown => {
                    // An identity-hashed element: kept, at a slot no run can
                    // name. A handle is a duplicate of a stored handle its
                    // row's `equals` matches (equal objects hash alike, so
                    // Java's probe meets it): the proved same object for an
                    // identity kind, equal contents for a value kind (a chat
                    // component). An unknown answer is counted, never merged
                    // by key.
                    if let Value::Host(handle) = key {
                        let stored_handles: Vec<Value> =
                            table.slots.iter().flatten().cloned().collect();
                        for other in stored_handles {
                            if !matches!(other, Value::Host(_)) {
                                continue;
                            }
                            match handle
                                .equals(&other, &mut object_methods::RuntimeNested::new(self))?
                            {
                                Some(true) => {
                                    let text =
                                        self.value_of_text(key, "Ljava/lang/Object;", "", "of")?;
                                    return Err(thrown(
                                        "java/lang/IllegalArgumentException",
                                        Some(format!("duplicate {what}: {text}")),
                                    ));
                                }
                                Some(false) => {}
                                None => {
                                    self.host.unknown_operand_call(&MethodRef {
                                        owner: "java/lang/Object".into(),
                                        name: "equals".into(),
                                        descriptor: "(Ljava/lang/Object;)Z".into(),
                                    })?;
                                }
                            }
                        }
                    }
                    table.unknown = true;
                    if let Some(free) = table.slots.iter().position(Option::is_none) {
                        table.slots[free] = Some(key.clone());
                    }
                }
            }
        }
        Ok(table)
    }

    /// The host's unmodifiable `kind` (`List`/`Set`/`Map`) over `values`,
    /// through its fixed-arity factory for any count.
    fn host_unmodifiable(&mut self, kind: &str, values: &[Value]) -> Result<Value, NativeError> {
        self.host
            .immutable_collection(&kind.to_ascii_lowercase(), values.to_vec())
            .ok_or_else(|| NativeError::Metadata("immutable collection host missing".into()))
    }

    /// `Set.of(elements)`: `Set12` for one or two, `SetN` otherwise.
    pub(crate) fn set_of(&mut self, elements: &[Value]) -> Result<Value, NativeError> {
        match elements {
            [] => {}
            [one] if *one == Value::Ref(None) => {
                return Err(thrown("java/lang/NullPointerException", None));
            }
            [_] => {}
            [first, second] => {
                // `e0.equals(Objects.requireNonNull(e1))`.
                if *second == Value::Ref(None) || *first == Value::Ref(None) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                if self.java_equals(first, second)? {
                    let text = self.value_of_text(first, "Ljava/lang/Object;", "", "of")?;
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        Some(format!("duplicate element: {text}")),
                    ));
                }
            }
            _ => {}
        }
        let table = if elements.len() > 2 {
            Some(self.lay_out(elements, "element")?)
        } else {
            None
        };
        let set = self.host_unmodifiable("Set", elements)?;
        if let Some(marker) = expect_ref(&set) {
            self.host
                .store_probe_table(marker, table.unwrap_or_default());
        }
        Ok(set)
    }

    /// `Map.of(k0, v0, ...)`/`Map.ofEntries`: `Map1` for one pair, `MapN`
    /// otherwise (keys and values must not be null).
    fn map_of(&mut self, pairs: &[Value]) -> Result<Value, NativeError> {
        if pairs.contains(&Value::Ref(None)) {
            return Err(thrown("java/lang/NullPointerException", None));
        }
        let keys: Vec<Value> = pairs.iter().step_by(2).cloned().collect();
        let table = if keys.len() > 1 {
            Some(self.lay_out(&keys, "key")?)
        } else {
            None
        };
        let map = self.host_unmodifiable("Map", pairs)?;
        if let Some(marker) = expect_ref(&map) {
            self.host
                .store_probe_table(marker, table.unwrap_or_default());
        }
        Ok(map)
    }

    /// The factories: `List.of`/`Set.of`/`Map.of` (fixed and varargs),
    /// `Map.ofEntries`, and the counted `Set.copyOf`/`Map.copyOf` of jar
    /// elements.
    fn immutable_factory(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let varargs = r.descriptor.starts_with("([");
        let array = |runtime: &Self| -> Option<Vec<Value>> {
            let array = args.first()?;
            if *array == Value::Ref(None) {
                return None;
            }
            runtime.array(array).ok().map(std::borrow::Cow::into_owned)
        };
        match (r.owner.as_str(), r.name.as_str()) {
            ("java/util/List", "of") if varargs => {
                let Some(elements) = array(self) else {
                    return Ok(None);
                };
                if elements.contains(&Value::Ref(None)) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                self.host_unmodifiable("List", &elements).map(Some)
            }
            ("java/util/Set", "of") => {
                let elements = if varargs {
                    let Some(elements) = array(self) else {
                        return Ok(None);
                    };
                    elements
                } else {
                    args.to_vec()
                };
                self.set_of(&elements).map(Some)
            }
            ("java/util/Map", "of") => self.map_of(args).map(Some),
            ("java/util/Map", "ofEntries") => {
                let Some(entries) = array(self) else {
                    return Ok(None);
                };
                let mut pairs = Vec::with_capacity(entries.len() * 2);
                for entry in &entries {
                    if *entry == Value::Ref(None) {
                        return Err(thrown("java/lang/NullPointerException", None));
                    }
                    let Some((key, value)) = self.host.map_entry_parts(entry) else {
                        return Ok(None);
                    };
                    pairs.extend([key, value]);
                }
                self.map_of(&pairs).map(Some)
            }
            ("java/util/Set" | "java/util/Map", "copyOf") => {
                let Some(source) = args.first() else {
                    return Ok(None);
                };
                let elements = self.collection_elements(source).unwrap_or_default();
                let jar = elements.iter().any(|value| {
                    matches!(
                        self.equality_of(value),
                        Equality::Jar {
                            equals: Some(_),
                            ..
                        }
                    )
                });
                if jar {
                    return self.host.unknown_operand_call(r).map(|_| None);
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Whether `value` is a `Set.of`/`Map.of` of two or more elements, whose
    /// iteration order is random per JVM run.
    pub(crate) fn random_order(&mut self, value: &Value) -> bool {
        let Some(marker) = expect_ref(value).cloned() else {
            return false;
        };
        let Some((map, table)) = self.probed_kind(value) else {
            return false;
        };
        self.host.store_probe_table(&marker, table);
        let size = self
            .collection_elements(value)
            .map_or(0, |entries| entries.len());
        let random = size / if map { 2 } else { 1 } >= 2;
        if random {
            self.host
                .mark_nondeterministic("java-random-immutable-iteration");
        }
        random
    }

    /// The class of a `Set.of`/`Map.of` this module laid out.
    fn probed_kind(&mut self, value: &Value) -> Option<(bool, ProbeTable)> {
        let marker = expect_ref(value)?;
        let class = marker.split_once('#').map(|(class, _)| class)?;
        let map = if SETS.contains(&class) {
            false
        } else if MAPS.contains(&class) {
            true
        } else {
            return None;
        };
        let table = self.host.take_probe_table(marker)?;
        Some((map, table))
    }

    /// `Set.of`/`Map.of`/`Map.ofEntries`/`List.of`, `IdentityHashMap`, and the
    /// members of a laid-out `Set.of`/`Map.of`. `Ok(None)` leaves the call to
    /// the rest of the dispatch.
    pub(crate) fn invoke_immutable_collection(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
        identity_args: &[reference_identity::CallIdentity],
    ) -> Result<Option<Value>, NativeError> {
        if opcode == 0xb8 {
            if let Some(answer) = self.immutable_factory(r, args)? {
                return Ok(Some(answer));
            }
            return self.unordered_argument(r, args);
        }
        if opcode == 0xb7
            && r.owner == IDENTITY_MAP
            && r.name == "<init>"
            && let Some(receiver) = receiver
        {
            self.note_fresh_identity_collection(receiver);
            return self.unordered_argument(r, args);
        }
        let Some(receiver) = receiver.filter(|_| matches!(opcode, 0xb6 | 0xb9)) else {
            return self.unordered_argument(r, args);
        };
        // A counted call may have changed this collection or its root (#464): its contents are unknown.
        if let Some(root) = self.uncertain_identity_root(receiver, r) {
            return self.count_identity_call(r, &root).map(Some);
        }
        // Exact identity `hashCode()` / `equals(Object)` for identity collections
        // and views at every size (including empty and single-entry maps/sets,
        // and cross-map comparisons such as `HashMap.equals(IdentityHashMap)`).
        if r.name == "hashCode"
            && r.descriptor == "()I"
            && args.is_empty()
            && let Some(hash) = self.identity_collection_hash(receiver)?
        {
            return match hash {
                Some(code) => Ok(Some(Value::Int(code))),
                None => self.host.unknown_operand_call(r).map(Some),
            };
        }
        if r.name == "equals"
            && r.descriptor == "(Ljava/lang/Object;)Z"
            && let [other] = args
            && let Some(equal) = self.identity_collection_equals(receiver, other, r)?
        {
            return Ok(Some(Value::Int(i32::from(equal))));
        }
        // SetFromMap is a live mutable view; its retain/remove operations must
        // reach the backing map through hash collection dispatch.
        if self.host.is_set_from_map(receiver) {
            return Ok(None);
        }
        if crate::host_map_entry::is_identity_entry(receiver)
            && let Some(answer) = self.identity_entry_member(r, receiver, args, identity_args)?
        {
            return Ok(Some(answer));
        }
        if let ("equals", "(Ljava/lang/Object;)Z", [other]) =
            (r.name.as_str(), r.descriptor.as_str(), args)
            && matches!(
                other,
                Value::Host(handle)
                    if matches!(
                        handle.kind(),
                        crate::host_ref::HostKind::KotlinEnumEntries
                            | crate::host_ref::HostKind::KotlinEmptyCollection
                    )
            )
            && let (Some(left_kind), Some(right_kind)) = (
                self.host_collection_kind(receiver),
                self.host_collection_kind(other),
            )
            && left_kind == right_kind
        {
            let left = self.collection_elements(receiver)?;
            let right = self.collection_elements(other)?;
            if left_kind == object_methods::CollectionKind::List {
                if left.len() != right.len() {
                    return Ok(Some(Value::Int(0)));
                }
                for (left, right) in left.iter().zip(&right) {
                    if !self.objects_equals(left, right)? {
                        return Ok(Some(Value::Int(0)));
                    }
                }
                return Ok(Some(Value::Int(1)));
            }
            return Ok(Some(Value::Int(
                (left.is_empty() && right.is_empty()) as i32,
            )));
        }
        if self.host_collection_class(receiver).as_deref() == Some(IDENTITY_MAP) {
            return self.identity_map_member(r, receiver, args, identity_args);
        }
        // Gson's `LinkedTreeMap` (a `fromJson` read's map) keeps `Map`'s
        // default `compute`/`merge` bodies too, which the host's map model
        // cannot run over a jar function (#337). With a plain `String` key its
        // `compareTo` lookup is the `equals` one, so the stored key equal to
        // `key` is found as `identity_compute` finds it. Any other key is left
        // to the host, which counts it (#253 re-check B5).
        if self.host_collection_class(receiver).as_deref() == Some(gson_tree::TREE_MAP)
            && matches!(args.first(), Some(Value::Ref(Some(key))) if value_class::is_plain_string(key))
        {
            let entries = match self.host.mutable_collection_entries(receiver) {
                Some(entries) => entries,
                None => self.collection_elements(receiver)?,
            };
            if let Some(answer) =
                self.identity_compute(r, receiver, &entries, args, identity_args, false)?
            {
                return Ok(Some(answer));
            }
        }
        if self.host_collection_class(receiver).as_deref() == Some(IDENTITY_SET)
            && !self.host.is_set_from_map(receiver)
        {
            return self.identity_set_member(r, receiver, args, identity_args);
        }
        if let Some(answer) = self.identity_view_member(r, receiver, args, identity_args)? {
            return Ok(answer);
        }
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        let Some((map, table)) = self.probed_kind(receiver) else {
            return self.unordered_argument(r, args);
        };
        let result = self.probed_member(r, receiver, map, &table, args);
        self.host.store_probe_table(&marker, table);
        result
    }

    /// `Sets.newIdentityHashSet()` is `Collections.newSetFromMap` over an
    /// `IdentityHashMap`: set membership compares handles with `==`, never
    /// their `equals` methods.
    fn identity_set_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
        identity_args: &[reference_identity::CallIdentity],
    ) -> Result<Option<Value>, NativeError> {
        let marker = crate::jar_object::object_key(receiver).map(str::to_owned);
        if r.name == "clear" {
            if let Some(marker) = marker.as_deref() {
                self.identity_collections.tokens.remove(marker);
            }
            return Ok(None);
        }
        let Some(value) = args.first() else {
            return Ok(None);
        };
        let mut values = self.collection_elements(receiver)?;
        let (mut tokens, _) = self.load_identity_tokens(marker.as_deref(), values.len());
        let wanted = identity_args.first().cloned().flatten();
        let Some(position) = self.identity_position(&values, &tokens, value, wanted.as_ref())
        else {
            return self.count_identity_call(r, receiver).map(Some);
        };
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("add", "(Ljava/lang/Object;)Z") => {
                if position.is_some() {
                    Ok(Some(Value::Int(0)))
                } else {
                    values.push(value.clone());
                    tokens.push(wanted);
                    self.host.replace_mutable_collection(receiver, values);
                    self.store_identity_tokens(marker, tokens, Vec::new());
                    Ok(Some(Value::Int(1)))
                }
            }
            ("contains", "(Ljava/lang/Object;)Z") => {
                Ok(Some(Value::Int(position.is_some() as i32)))
            }
            ("remove", "(Ljava/lang/Object;)Z") => {
                let Some(index) = position else {
                    return Ok(Some(Value::Int(0)));
                };
                values.remove(index);
                tokens.remove(index);
                self.host.replace_mutable_collection(receiver, values);
                self.store_identity_tokens(marker, tokens, Vec::new());
                Ok(Some(Value::Int(1)))
            }
            _ => Ok(None),
        }
    }

    /// A `Set.of`/`Map.of` of two or more elements passed to a host model
    /// that may show its order: counted.
    fn unordered_argument(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !self.shows_argument_order(r) {
            return Ok(None);
        }
        for arg in args {
            let Some(marker) = expect_ref(arg).cloned() else {
                continue;
            };
            let _ = &marker;
            if self.random_order(arg) {
                return self.host.unknown_operand_call(r).map(|_| None);
            }
        }
        Ok(None)
    }

    /// The members of a `Set.of`/`Map.of`.
    fn probed_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        map: bool,
        table: &ProbeTable,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if (map
            && matches!(
                r.name.as_str(),
                "put"
                    | "putAll"
                    | "putIfAbsent"
                    | "remove"
                    | "replace"
                    | "replaceAll"
                    | "compute"
                    | "computeIfAbsent"
                    | "computeIfPresent"
                    | "merge"
                    | "clear"
            ))
            || (!map
                && matches!(
                    r.name.as_str(),
                    "add" | "remove" | "clear" | "removeAll" | "retainAll"
                ))
        {
            return Err(thrown("java/lang/UnsupportedOperationException", None));
        }
        let entries = self.collection_elements(receiver)?;
        let size = if map {
            entries.len() / 2
        } else {
            entries.len()
        };
        let boolean = |flag: bool| Some(Value::Int(i32::from(flag)));
        let value_at = |entries: &[Value], key: &Value| -> Value {
            let marker = collection_item_marker(key);
            entries
                .chunks(2)
                .find(|pair| collection_item_marker(&pair[0]) == marker)
                .and_then(|pair| pair.get(1).cloned())
                .unwrap_or(Value::Ref(None))
        };
        // A key's slot: `Set12`/`Map1` compare in order; `SetN`/`MapN` probe.
        let find =
            |runtime: &mut Self, key: &Value| -> Result<Option<Option<Value>>, NativeError> {
                let step = if map { 2 } else { 1 };
                let stored: Vec<Value> = entries.iter().step_by(step).cloned().collect();
                if *key == Value::Ref(None) {
                    // `SetN`/`MapN`: `Objects.requireNonNull(o)`; `Set12`/`Map1`:
                    // `o.equals(..)`.
                    return Err(if table.slots.is_empty() {
                        null_receiver()
                    } else {
                        thrown("java/lang/NullPointerException", None)
                    });
                }
                if !table.slots.is_empty() {
                    return Ok(match runtime.probe(table, key)? {
                        Probe::Found(slot) => match &table.slots[slot] {
                            Some(stored) => Some(Some(stored.clone())),
                            None => Some(None),
                        },
                        Probe::Empty(_) => Some(None),
                        Probe::Unknown => None,
                    });
                }
                // `Set12.contains`: `o.equals(e0) || e1.equals(o)`; `Map1`:
                // `o.equals(k0)`.
                if let Some(first) = stored.first()
                    && runtime.java_equals(key, first)?
                {
                    return Ok(Some(Some(first.clone())));
                }
                if let Some(second) = stored.get(1)
                    && runtime.java_equals(second, key)?
                {
                    return Ok(Some(Some(second.clone())));
                }
                Ok(Some(None))
            };
        match (map, r.name.as_str(), r.descriptor.as_str(), args) {
            (false, "contains", "(Ljava/lang/Object;)Z", [key])
            | (true, "containsKey", "(Ljava/lang/Object;)Z", [key]) => {
                if size == 0 {
                    if *key == Value::Ref(None) {
                        return Err(thrown("java/lang/NullPointerException", None));
                    }
                    return Ok(boolean(false));
                }
                match find(self, key)? {
                    Some(found) => Ok(boolean(found.is_some())),
                    None => self.host.unknown_operand_call(r).map(|_| None),
                }
            }
            (true, "get", "(Ljava/lang/Object;)Ljava/lang/Object;", [key])
            | (true, "getOrDefault", _, [key, _]) => {
                let default = args.get(1).cloned().unwrap_or(Value::Ref(None));
                if size == 0 {
                    if *key == Value::Ref(None) {
                        return Err(thrown("java/lang/NullPointerException", None));
                    }
                    return Ok(Some(default));
                }
                match find(self, key)? {
                    Some(Some(stored)) => Ok(Some(value_at(&entries, &stored))),
                    Some(None) => Ok(Some(default)),
                    None => self.host.unknown_operand_call(r).map(|_| None),
                }
            }
            (_, "hashCode", "()I", []) => {
                // `Set12`/`Map1`: in element order; `SetN`/`MapN`: slot order.
                let mut keys: Vec<Value> = if table.slots.is_empty() || table.unknown {
                    entries
                        .iter()
                        .step_by(if map { 2 } else { 1 })
                        .cloned()
                        .collect()
                } else {
                    table.slots.iter().flatten().cloned().collect()
                };
                if table.unknown && keys.iter().any(|key| !self.pure_equality(key)) {
                    return self.host.unknown_operand_call(r).map(|_| None);
                }
                let mut hash = 0_i32;
                for key in keys.drain(..) {
                    let Some(part) = self.java_hash(&key)? else {
                        return self.host.unknown_operand_call(r).map(|_| None);
                    };
                    let part = if map {
                        let Some(value) = self.java_hash(&value_at(&entries, &key))? else {
                            return self.host.unknown_operand_call(r).map(|_| None);
                        };
                        part ^ value
                    } else {
                        part
                    };
                    hash = hash.wrapping_add(part);
                }
                Ok(Some(Value::Int(hash)))
            }
            (false, "equals", "(Ljava/lang/Object;)Z", [other]) => {
                // `AbstractImmutableSet.equals`: `contains(e)` for each of the
                // other set's elements, in its order.
                if other == receiver {
                    return Ok(boolean(true));
                }
                if self.host_collection_kind(other) != Some(object_methods::CollectionKind::Set) {
                    return Ok(None);
                }
                let theirs = self.collection_elements(other)?;
                if theirs.len() != size {
                    return Ok(boolean(false));
                }
                for element in &theirs {
                    if *element == Value::Ref(None) {
                        return Ok(boolean(false));
                    }
                    match find(self, element)? {
                        Some(Some(_)) => {}
                        Some(None) => return Ok(boolean(false)),
                        None => return self.host.unknown_operand_call(r).map(|_| None),
                    }
                }
                Ok(boolean(true))
            }
            (false, "containsAll", "(Ljava/util/Collection;)Z", [other]) => {
                // `AbstractCollection.containsAll`: `contains(e)` for each of
                // the other's elements, in its order.
                if self.host_collection_kind(other).is_none() || self.random_order(other) {
                    return Ok(None);
                }
                for element in self.collection_elements(other)? {
                    if size == 0 {
                        if element == Value::Ref(None) {
                            return Err(thrown("java/lang/NullPointerException", None));
                        }
                        return Ok(boolean(false));
                    }
                    match find(self, &element)? {
                        Some(Some(_)) => {}
                        Some(None) => return Ok(boolean(false)),
                        None => return self.host.unknown_operand_call(r).map(|_| None),
                    }
                }
                Ok(boolean(true))
            }
            (true, "containsValue", "(Ljava/lang/Object;)Z", [wanted]) => {
                // `Map1`: `o.equals(v0)`; `MapN`: `requireNonNull(o)`, then
                // `o.equals(v)` in slot order.
                if *wanted == Value::Ref(None) {
                    return Err(if table.slots.is_empty() && size == 1 {
                        null_receiver()
                    } else {
                        thrown("java/lang/NullPointerException", None)
                    });
                }
                let keys: Vec<Value> = if table.slots.is_empty() {
                    entries.iter().step_by(2).cloned().collect()
                } else {
                    if table.unknown && !self.pure_equality(wanted) {
                        return self.host.unknown_operand_call(r).map(|_| None);
                    }
                    table.slots.iter().flatten().cloned().collect()
                };
                for key in &keys {
                    let stored = value_at(&entries, key);
                    if self.java_equals(wanted, &stored)? {
                        return Ok(boolean(true));
                    }
                }
                Ok(boolean(false))
            }
            // `getClass()` names the class, never the order (a `SetN` is an
            // object now, #156 P14c, so its `getClass` arrives here).
            (_, "size" | "isEmpty" | "getClass", _, []) => Ok(None),
            // Anything else may show the order: random for two or more.
            _ if size >= 2 => self.host.unknown_operand_call(r).map(|_| None),
            _ => Ok(None),
        }
    }

    /// Whether `a == b` is unknown where the host compares `a.equals(b)`: two
    /// equal strings or boxes (one object or two), or two equal host handles
    /// of a kind that is not canonical from different constructions
    /// (`HostRef::same_object`). Where it is known, the host's equality
    /// answer is the identity answer: unequal values are different objects,
    /// and an equal handle that is the same object is a hit.
    fn identity_unknown(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Host(left), Value::Host(right)) => left.same_object(right).is_none(),
            _ => self.equality_of(a) == Equality::Platform && a == b,
        }
    }

    /// Whether `receiver` is an identity-based collection, a view over one, or
    /// a live `IdentityHashMap` entry, so argument allocation provenance needs
    /// to be collected for calls on it.
    pub(crate) fn tracks_identity_args(&self, receiver: &Value) -> bool {
        if crate::host_map_entry::is_identity_entry(receiver) {
            return true;
        }
        if matches!(
            self.host_collection_class(receiver).as_deref(),
            Some(IDENTITY_MAP | IDENTITY_SET)
        ) {
            return true;
        }
        expect_ref(receiver)
            .and_then(|view| self.host.view_copy_source(view))
            .is_some_and(|source| {
                let (root, _) = self.root_map_of_view(crate::jar_object::stored_value(&source));
                self.host_collection_class(&root).as_deref() == Some(IDENTITY_MAP)
            })
    }

    /// Compare two values by Java object identity (`==`), using optional
    /// bytecode allocation provenance (`ValueIdentity`).
    fn identity_match(
        &self,
        a: &Value,
        tok_a: Option<&reference_identity::ValueIdentity>,
        b: &Value,
        tok_b: Option<&reference_identity::ValueIdentity>,
    ) -> IdentityMatch {
        if a != b {
            return IdentityMatch::Different;
        }
        match (a, b) {
            (Value::Host(left), Value::Host(right)) => match left.same_object(right) {
                Some(true) => IdentityMatch::Same,
                Some(false) => IdentityMatch::Different,
                None => IdentityMatch::Unknown,
            },
            _ if self.equality_of(a) != Equality::Platform => IdentityMatch::Same,
            _ => match (tok_a, tok_b) {
                (
                    Some(reference_identity::ValueIdentity::Fresh(left)),
                    Some(reference_identity::ValueIdentity::Fresh(right)),
                ) => {
                    if left == right {
                        IdentityMatch::Same
                    } else {
                        IdentityMatch::Different
                    }
                }
                (
                    Some(reference_identity::ValueIdentity::Interned(left)),
                    Some(reference_identity::ValueIdentity::Interned(right)),
                ) => {
                    if left == right {
                        IdentityMatch::Same
                    } else {
                        IdentityMatch::Different
                    }
                }
                (
                    Some(reference_identity::ValueIdentity::CachedBox {
                        class: lc,
                        bits: lb,
                    }),
                    Some(reference_identity::ValueIdentity::CachedBox {
                        class: rc,
                        bits: rb,
                    }),
                ) => {
                    if lc == rc && lb == rb {
                        IdentityMatch::Same
                    } else {
                        IdentityMatch::Different
                    }
                }
                (
                    Some(reference_identity::ValueIdentity::CachedBox { .. }),
                    Some(reference_identity::ValueIdentity::Fresh(_)),
                )
                | (
                    Some(reference_identity::ValueIdentity::Fresh(_)),
                    Some(reference_identity::ValueIdentity::CachedBox { .. }),
                ) => IdentityMatch::Different,
                (
                    Some(reference_identity::ValueIdentity::Fresh(_)),
                    Some(reference_identity::ValueIdentity::Interned(text)),
                )
                | (
                    Some(reference_identity::ValueIdentity::Interned(text)),
                    Some(reference_identity::ValueIdentity::Fresh(_)),
                ) => {
                    if self.string_pool.is_intern_ambiguous(text) {
                        IdentityMatch::Unknown
                    } else {
                        IdentityMatch::Different
                    }
                }
                _ => IdentityMatch::Unknown,
            },
        }
    }

    /// Find `wanted` by Java reference identity among `stored_items` (with
    /// parallel `stored_tokens`). Returns `None` when any stored item's
    /// identity with `wanted` is unknown (so the caller must count the call),
    /// `Some(Some(index))` on a proven match, or `Some(None)` when every
    /// stored item is provably a different object.
    fn identity_position(
        &self,
        stored_items: &[Value],
        stored_tokens: &[reference_identity::CallIdentity],
        wanted: &Value,
        wanted_token: Option<&reference_identity::ValueIdentity>,
    ) -> Option<Option<usize>> {
        let mut found = None;
        for (index, stored) in stored_items.iter().enumerate() {
            let stored_token = stored_tokens.get(index).and_then(Option::as_ref);
            match self.identity_match(wanted, wanted_token, stored, stored_token) {
                IdentityMatch::Same => {
                    if found.is_none() {
                        found = Some(index);
                    }
                }
                IdentityMatch::Different => {}
                IdentityMatch::Unknown => return None,
            }
        }
        Some(found)
    }

    /// `compute`/`computeIfAbsent`/`computeIfPresent`/`merge` on an
    /// `IdentityHashMap` (`by_identity == true`), whose stored key equal to
    /// `key` (if any) is compared by reference identity, or on Gson's
    /// `LinkedTreeMap` with a plain `String` key (`by_identity == false`,
    /// #337), whose `compareTo` lookup is content equality. `Ok(None)` for
    /// every other member.
    fn identity_compute(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        entries: &[Value],
        args: &[Value],
        identity_args: &[reference_identity::CallIdentity],
        by_identity: bool,
    ) -> Result<Option<Value>, NativeError> {
        let name = r.name.as_str();
        let (key, function, incoming) = match (name, args) {
            ("merge", [key, value, function]) => (key, function, Some(value)),
            ("compute" | "computeIfAbsent" | "computeIfPresent", [key, function]) => {
                (key, function, None)
            }
            _ => return Ok(None),
        };
        let null_pointer = || thrown("java/lang/NullPointerException", None);
        if *function == Value::Ref(None) || incoming == Some(&Value::Ref(None)) {
            return Err(null_pointer());
        }
        let marker = by_identity
            .then(|| crate::jar_object::object_key(receiver).map(str::to_owned))
            .flatten();
        let key_token = if by_identity {
            identity_args.first().cloned().flatten()
        } else {
            None
        };
        let old_index = if by_identity {
            let stored_keys: Vec<Value> = entries.iter().step_by(2).cloned().collect();
            let (tokens, _) = self.load_identity_tokens(marker.as_deref(), stored_keys.len());
            let Some(found) =
                self.identity_position(&stored_keys, &tokens, key, key_token.as_ref())
            else {
                return self.count_identity_call(r, receiver).map(Some);
            };
            found
        } else {
            entries.chunks(2).position(|pair| pair[0] == *key)
        };
        let old = old_index.map_or(Value::Ref(None), |index| entries[index * 2 + 1].clone());
        let from_callback = !(name == "merge" && old == Value::Ref(None));
        let new = match name {
            // `v = get(k); if (v == null) { nv = f(k); if (nv != null) put }`.
            "computeIfAbsent" if old != Value::Ref(None) => return Ok(Some(old)),
            "computeIfAbsent" => self.apply_callback(function, std::slice::from_ref(key))?,
            // `if ((old = get(k)) != null) { nv = f(k, old); put or remove }`.
            "computeIfPresent" if old == Value::Ref(None) => return Ok(Some(Value::Ref(None))),
            "computeIfPresent" | "compute" => {
                self.apply_callback(function, &[key.clone(), old.clone()])?
            }
            // `nv = old == null ? value : f(old, value)`.
            _ if old == Value::Ref(None) => incoming.cloned().unwrap_or(Value::Ref(None)),
            _ => self.apply_callback(
                function,
                &[old.clone(), incoming.cloned().unwrap_or(Value::Ref(None))],
            )?,
        };
        if by_identity && self.collection_uncertain(receiver) {
            return self.count_identity_call(r, receiver).map(Some);
        }
        // The function may have changed the map: reload both the entries and
        // (for `IdentityHashMap`) the identity side tables so `put`/`remove`
        // act on the map as it is now.
        let mut entries = match self.host.mutable_collection_entries(receiver) {
            Some(entries) => entries,
            None => self.collection_elements(receiver)?,
        };
        if !by_identity {
            let current_index = entries.chunks(2).position(|pair| pair[0] == *key);
            match (current_index, new == Value::Ref(None)) {
                (Some(index), true) => {
                    if name == "compute" || old != Value::Ref(None) {
                        entries.drain(index * 2..index * 2 + 2);
                    }
                }
                (Some(index), false) => entries[index * 2 + 1] = new.clone(),
                (None, true) => {}
                (None, false) => entries.extend([key.clone(), new.clone()]),
            }
            self.host.replace_mutable_collection(receiver, entries);
            return Ok(Some(new));
        }
        let reloaded_keys: Vec<Value> = entries.iter().step_by(2).cloned().collect();
        let (mut tokens, mut value_tokens) =
            self.load_identity_tokens(marker.as_deref(), reloaded_keys.len());
        let Some(current_index) =
            self.identity_position(&reloaded_keys, &tokens, key, key_token.as_ref())
        else {
            return self.count_identity_call(r, receiver).map(Some);
        };
        let new_value_token = if from_callback {
            None
        } else {
            identity_args.get(1).cloned().flatten()
        };
        match (current_index, new == Value::Ref(None)) {
            (Some(index), true) => {
                // `compute`: `if (old != null || containsKey(k)) remove(k)`;
                // the others remove only a present non-null value.
                if name == "compute" || old != Value::Ref(None) {
                    entries.drain(index * 2..index * 2 + 2);
                    tokens.remove(index);
                    value_tokens.remove(index);
                }
            }
            (Some(index), false) => {
                entries[index * 2 + 1] = new.clone();
                value_tokens[index] = new_value_token;
            }
            (None, true) => {}
            (None, false) => {
                entries.extend([key.clone(), new.clone()]);
                tokens.push(key_token);
                value_tokens.push(new_value_token);
            }
        }
        self.host.replace_mutable_collection(receiver, entries);
        self.store_identity_tokens(marker, tokens, value_tokens);
        Ok(Some(new))
    }

    /// A member of a live `IdentityHashMap$EntryIterator$Entry`: keep the
    /// backing map's value-provenance table in sync with `setValue`, count any
    /// call when the backing map is uncertain, and count+mark uncertain when
    /// duplicate equal-valued keys make `setValue`'s target slot ambiguous.
    fn identity_entry_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
        identity_args: &[reference_identity::CallIdentity],
    ) -> Result<Option<Value>, NativeError> {
        let Some(map_marker) = self.host.map_entry_source(receiver) else {
            return Ok(None);
        };
        let map_value = crate::jar_object::stored_value(&map_marker);
        if self.host_collection_class(&map_value).as_deref() != Some(IDENTITY_MAP) {
            return Ok(None);
        }
        if self.collection_uncertain(&map_value) {
            return self.count_identity_call(r, &map_value).map(Some);
        }
        if r.name != "setValue" || r.descriptor != "(Ljava/lang/Object;)Ljava/lang/Object;" {
            return Ok(None);
        }
        let Some(entries) = self.host.mutable_collection_entries(&map_value) else {
            return Ok(None);
        };
        let Some((entry_key, _)) = self.host.map_entry_parts(receiver) else {
            return Ok(None);
        };
        let mut matching = entries
            .chunks(2)
            .enumerate()
            .filter_map(|(index, pair)| (pair.first() == Some(&entry_key)).then_some(index));
        let Some(index) = matching.next() else {
            // Detached/removed entry: let host handle (count or update detached state).
            return Ok(None);
        };
        if matching.next().is_some() {
            // Multiple entries have equal-valued keys (e.g. distinct `new String("k")`
            // instances), so `StdHost::map_entry_set_value`'s value-equality lookup
            // cannot identify which slot belongs to this entry.
            return self.count_identity_call(r, &map_value).map(Some);
        }
        let pair_count = entries.len() / 2;
        let (keys, mut values) = self.load_identity_tokens(Some(&map_marker), pair_count);
        let before_gen = self.host.collection_generation(&map_marker);
        let old = self.host.invoke_instance(r, receiver, args)?;
        let after_gen = self.host.collection_generation(&map_marker);
        if after_gen != before_gen {
            values[index] = identity_args.first().cloned().flatten();
            self.store_identity_tokens(Some(map_marker.clone()), keys, values);
            self.refresh_map_views(&map_marker);
        }
        Ok(Some(old))
    }

    /// A member of a `keySet()`/`values()`/`entrySet()` copy of an
    /// `IdentityHashMap` (`hash_collections.rs` notes each copy's map): the
    /// map compares keys, values and entries' parts with `==`, so an argument
    /// whose identity with an equal stored one is unknown or disagrees with
    /// the host view copy's equality lookup is counted. `None` for any other
    /// receiver, or when the copy's equality answer is the identity one.
    fn identity_view_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
        identity_args: &[reference_identity::CallIdentity],
    ) -> Result<Option<Option<Value>>, NativeError> {
        let Some(view) = expect_ref(receiver) else {
            return Ok(None);
        };
        let Some(source) = self.host.view_copy_source(view) else {
            return Ok(None);
        };
        let (source, saw_unmodifiable) =
            self.root_map_of_view(crate::jar_object::stored_value(&source));
        if self.host_collection_class(&source).as_deref() != Some(IDENTITY_MAP) {
            return Ok(None);
        }
        if self.collection_uncertain(&source) {
            return self
                .count_identity_call(r, &source)
                .map(|value| Some(Some(value)));
        }
        let entries = match self.host.mutable_collection_entries(&source) {
            Some(entries) => entries,
            None => self.collection_elements(&source)?,
        };
        let keys: Vec<Value> = entries.iter().step_by(2).cloned().collect();
        let values: Vec<Value> = entries.iter().skip(1).step_by(2).cloned().collect();
        let source_marker = crate::jar_object::object_key(&source).map(str::to_owned);
        let (key_tokens, value_tokens) =
            self.load_identity_tokens(source_marker.as_deref(), keys.len());
        // The arguments as the copy's elements: one, or a collection's.
        let mut wanted = Vec::new();
        for (arg_index, arg) in args.iter().enumerate() {
            match self.host_collection_kind(arg) {
                Some(_) => {
                    for element in self.collection_elements(arg).unwrap_or_default() {
                        wanted.push((element, None));
                    }
                }
                None => wanted.push((arg.clone(), identity_args.get(arg_index).cloned().flatten())),
            }
        }
        let kind = self.host.view_copy_kind(view);
        let unknown = |runtime: &Self, value: &Value, among: &[Value]| {
            among
                .iter()
                .any(|stored| runtime.identity_unknown(value, stored))
        };
        // Falling through delegates to the host's equality-based view copy, so
        // any equal-valued stored entry that is not provably the same object
        // (either unknown or a distinct allocation with equal text) must be
        // counted instead of letting equality answer wrongly.
        let ambiguous = wanted
            .iter()
            .any(|(value, wanted_token)| match kind.as_deref() {
                Some("keySet") => keys.iter().enumerate().any(|(index, stored)| {
                    if stored != value {
                        return false;
                    }
                    let stored_token = key_tokens.get(index).and_then(Option::as_ref);
                    self.identity_match(value, wanted_token.as_ref(), stored, stored_token)
                        != IdentityMatch::Same
                }),
                Some("values") => values.iter().enumerate().any(|(index, stored)| {
                    if stored != value {
                        return false;
                    }
                    let stored_token = value_tokens.get(index).and_then(Option::as_ref);
                    self.identity_match(value, wanted_token.as_ref(), stored, stored_token)
                        != IdentityMatch::Same
                }),
                Some("entrySet") => self
                    .host
                    .map_entry_parts(value)
                    .is_some_and(|(key, value)| {
                        unknown(self, &key, &keys) || unknown(self, &value, &values)
                    }),
                _ => false,
            });
        if ambiguous {
            return self
                .count_identity_call(r, &source)
                .map(|value| Some(Some(value)));
        }
        if !saw_unmodifiable && mutates(&r.name) && !matches!(r.name.as_str(), "add" | "addAll") {
            if let Some(marker) = source_marker.as_deref() {
                self.identity_collections.tokens.remove(marker);
            }
            return self
                .count_identity_call(r, &source)
                .map(|value| Some(Some(value)));
        }
        Ok(None)
    }

    /// `IdentityHashMap`: a lookup by a key whose identity with an equal key
    /// is unknown ([`Self::identity_match`]), and the iteration of two or
    /// more keys, are counted. A handle put twice is one key.
    fn identity_map_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
        identity_args: &[reference_identity::CallIdentity],
    ) -> Result<Option<Value>, NativeError> {
        let marker = crate::jar_object::object_key(receiver).map(str::to_owned);
        if r.name == "clear" {
            if let Some(marker) = marker.as_deref() {
                self.identity_collections.tokens.remove(marker);
            }
            return Ok(None);
        }
        // The stored keys themselves: a handle's construction survives only
        // in the `Value`, not in its marker.
        let entries = match self.host.mutable_collection_entries(receiver) {
            Some(entries) => entries,
            None => self.collection_elements(receiver)?,
        };
        let keyed = matches!(
            r.name.as_str(),
            "get"
                | "containsKey"
                | "put"
                | "remove"
                | "putIfAbsent"
                | "getOrDefault"
                | "replace"
                | "computeIfAbsent"
                | "computeIfPresent"
                | "compute"
                | "merge"
        );
        if keyed {
            let Some(key) = args.first() else {
                return Ok(None);
            };
            let key_token = identity_args.first().cloned().flatten();
            let stored_keys: Vec<Value> = entries.iter().step_by(2).cloned().collect();
            let (mut tokens, mut value_tokens) =
                self.load_identity_tokens(marker.as_deref(), stored_keys.len());
            let Some(position) =
                self.identity_position(&stored_keys, &tokens, key, key_token.as_ref())
            else {
                return self.count_identity_call(r, receiver).map(Some);
            };
            match r.name.as_str() {
                "get" | "getOrDefault" | "containsKey" | "remove" | "put" | "putIfAbsent"
                | "replace" => {
                    let contains = position.is_some();
                    if r.name == "containsKey" {
                        return Ok(Some(Value::Int(contains as i32)));
                    }
                    if r.name == "get" {
                        return Ok(Some(
                            position
                                .map_or(Value::Ref(None), |index| entries[index * 2 + 1].clone()),
                        ));
                    }
                    if r.name == "getOrDefault" {
                        return Ok(Some(position.map_or_else(
                            || args.get(1).cloned().unwrap_or(Value::Ref(None)),
                            |index| entries[index * 2 + 1].clone(),
                        )));
                    }
                    let mut updated = entries.clone();
                    let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
                    match r.name.as_str() {
                        "remove" => {
                            if args.len() == 2 {
                                let Some(index) = position else {
                                    return Ok(Some(Value::Int(0)));
                                };
                                let wanted_token = identity_args.get(1).and_then(Option::as_ref);
                                let stored_token = value_tokens.get(index).and_then(Option::as_ref);
                                let matches = match self.identity_match(
                                    &value,
                                    wanted_token,
                                    &updated[index * 2 + 1],
                                    stored_token,
                                ) {
                                    IdentityMatch::Same => true,
                                    IdentityMatch::Different => false,
                                    IdentityMatch::Unknown => {
                                        return self.count_identity_call(r, receiver).map(Some);
                                    }
                                };
                                if matches {
                                    updated.drain(index * 2..index * 2 + 2);
                                    tokens.remove(index);
                                    value_tokens.remove(index);
                                    self.host.replace_mutable_collection(receiver, updated);
                                    self.store_identity_tokens(marker, tokens, value_tokens);
                                }
                                return Ok(Some(Value::Int(matches as i32)));
                            }
                            let Some(index) = position else {
                                return Ok(Some(Value::Ref(None)));
                            };
                            let old = updated[index * 2 + 1].clone();
                            updated.drain(index * 2..index * 2 + 2);
                            tokens.remove(index);
                            value_tokens.remove(index);
                            self.host.replace_mutable_collection(receiver, updated);
                            self.store_identity_tokens(marker, tokens, value_tokens);
                            return Ok(Some(old));
                        }
                        "put" => {
                            if let Some(index) = position {
                                let old = updated[index * 2 + 1].clone();
                                updated[index * 2 + 1] = value;
                                value_tokens[index] = identity_args.get(1).cloned().flatten();
                                self.host.replace_mutable_collection(receiver, updated);
                                self.store_identity_tokens(marker, tokens, value_tokens);
                                return Ok(Some(old));
                            }
                            updated.extend([key.clone(), value]);
                            tokens.push(key_token);
                            value_tokens.push(identity_args.get(1).cloned().flatten());
                            self.host.replace_mutable_collection(receiver, updated);
                            self.store_identity_tokens(marker, tokens, value_tokens);
                            return Ok(Some(Value::Ref(None)));
                        }
                        "putIfAbsent" => {
                            if contains {
                                return Ok(Some(updated[position.unwrap() * 2 + 1].clone()));
                            }
                            updated.extend([key.clone(), value]);
                            tokens.push(key_token);
                            value_tokens.push(identity_args.get(1).cloned().flatten());
                            self.host.replace_mutable_collection(receiver, updated);
                            self.store_identity_tokens(marker, tokens, value_tokens);
                            return Ok(Some(Value::Ref(None)));
                        }
                        "replace" => {
                            let Some(index) = position else {
                                return Ok(Some(if args.len() == 3 {
                                    Value::Int(0)
                                } else {
                                    Value::Ref(None)
                                }));
                            };
                            let old = updated[index * 2 + 1].clone();
                            if args.len() == 3 {
                                let wanted_token = identity_args.get(1).and_then(Option::as_ref);
                                let stored_token = value_tokens.get(index).and_then(Option::as_ref);
                                let matches = match self.identity_match(
                                    &value,
                                    wanted_token,
                                    &old,
                                    stored_token,
                                ) {
                                    IdentityMatch::Same => true,
                                    IdentityMatch::Different => false,
                                    IdentityMatch::Unknown => {
                                        return self.count_identity_call(r, receiver).map(Some);
                                    }
                                };
                                if matches {
                                    updated[index * 2 + 1] = args[2].clone();
                                    value_tokens[index] = identity_args.get(2).cloned().flatten();
                                    self.host.replace_mutable_collection(receiver, updated);
                                    self.store_identity_tokens(marker, tokens, value_tokens);
                                }
                                return Ok(Some(Value::Int(matches as i32)));
                            }
                            updated[index * 2 + 1] = value;
                            value_tokens[index] = identity_args.get(1).cloned().flatten();
                            self.host.replace_mutable_collection(receiver, updated);
                            self.store_identity_tokens(marker, tokens, value_tokens);
                            return Ok(Some(old));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            return self.identity_compute(r, receiver, &entries, args, identity_args, true);
        }
        // `putAll(map)`/`new IdentityHashMap<>(map)`: each of the source's
        // keys is put by identity.
        if matches!(
            (r.name.as_str(), r.descriptor.as_str()),
            ("putAll", "(Ljava/util/Map;)V") | ("<init>", "(Ljava/util/Map;)V")
        ) {
            let Some(source) = args.first() else {
                return Ok(None);
            };
            let incoming = self.collection_elements(source).unwrap_or_default();
            let ambiguous = incoming.iter().step_by(2).any(|key| {
                entries
                    .iter()
                    .step_by(2)
                    .any(|stored| self.identity_unknown(key, stored))
            });
            if ambiguous {
                return self.count_identity_call(r, receiver).map(Some);
            }
            if let Some(marker) = marker.as_deref() {
                self.identity_collections.tokens.remove(marker);
            }
            return Ok(None);
        }
        if r.name == "containsValue" {
            let Some(value) = args.first() else {
                return Ok(None);
            };
            let ambiguous = entries
                .iter()
                .skip(1)
                .step_by(2)
                .any(|stored| self.identity_unknown(value, stored));
            if ambiguous {
                return self.count_identity_call(r, receiver).map(Some);
            }
            return Ok(None);
        }
        if r.name == "replaceAll" {
            if args.first() == Some(&Value::Ref(None)) {
                return Err(thrown("java/lang/NullPointerException", None));
            }
            if entries.is_empty() {
                return Ok(Some(Value::Void));
            }
            return self.count_identity_call(r, receiver).map(Some);
        }
        if matches!(r.name.as_str(), "size" | "isEmpty" | "clear" | "<init>") || entries.len() < 4 {
            return Ok(None);
        }
        self.host.unknown_operand_call(r).map(|_| None)
    }
}
