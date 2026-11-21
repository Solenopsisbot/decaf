//! The objects a jar's Java code built, kept with the jar's host between calls.
//!
//! The interpreter names every object by a marker (`demo/Listener#3`, an
//! array handle's `[I#3` key, an iterator handle's `iterator/...` key) and keeps its instance fields, array
//! elements, `StringBuilder` text and enum identity in per-invocation tables
//! on [`Runtime`]. Before this module those tables died with the invocation:
//! a later callback (a game hook, an event listener, a payload handler) saw
//! the jar's static fields but none of the objects they pointed at, so a stop
//! listener walking the listener list its entrypoint filled walked a list of
//! fieldless markers.
//!
//! A [`JavaHeap`] is those tables plus the object and array counters, moved
//! into the host when an invocation ends ([`Runtime::persist_resident_heap`])
//! and back out when the next one starts ([`Runtime::adopt_resident_heap`]).
//! [`StdHost`] is the resident host; a host that keeps no jar instance answers
//! `None` and every invocation starts empty, as before.
//!
//! What stays is what a later call can still reach. When a heap is stored the
//! host collects it ([`collect`]) once it has grown well past what the last
//! collection kept: everything reachable from the jar's static fields and from
//! the values the host itself holds (its call log, registrations, payload
//! handlers, posted events, holders...) is kept, the rest is dropped. A tick
//! listener that allocates every tick therefore does not grow the heap for the
//! life of the server. The collection is deterministic: it depends only on the
//! tables' contents, never on hashing order or time.
//!
//! Markers are text, and composite values embed other markers in their own
//! text (a lambda's `@indy:...` marker carries its captures, a map view its
//! map). A host map entry is a handle whose key and value (and, for a node, its
//! map) the host keeps by its key (`host_map_entry.rs`); they are its edges. Rather than parse every composite format,
//! the collector looks for any collectable marker that ends where a run of
//! digits ends inside a value's text: every collectable marker ends in its
//! allocation number, so this finds an embedded one wherever it sits. A string
//! that merely spells a live marker keeps that object alive; that is the only
//! imprecision, and it errs on the side of keeping.
//!
//! The heap is bounded after collection ([`MAX_RESIDENT_FIELDS`],
//! [`MAX_RESIDENT_ARRAYS`], [`MAX_RESIDENT_BYTES`]); a store over a bound is
//! refused with an error that names it, so the callback that grew it fails
//! instead of the server running out of memory.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::{Host, NativeError, Runtime, StdHost, Value, game_hooks::GAME_OBJECT_PREFIX};

/// Instance fields one jar's resident heap may keep after collection.
pub(crate) const MAX_RESIDENT_FIELDS: usize = 65_536;
/// Arrays one jar's resident heap may keep after collection.
pub(crate) const MAX_RESIDENT_ARRAYS: usize = 8_192;
/// Estimated bytes (marker and value text, eight per primitive) the kept
/// fields, arrays, builders and host collections may hold after collection.
pub(crate) const MAX_RESIDENT_BYTES: usize = 32 << 20;
/// Entries a heap may grow past twice what the last collection kept before
/// the next store collects it. Small heaps collect rarely; a large one at most
/// every time it doubles, so a store costs amortised O(1) per entry.
const COLLECT_SLACK: usize = 1024;

/// The instance state a jar's invocations built, kept between them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JavaHeap {
    /// `receiver|owner#name descriptor` → value, the interpreter's own keys.
    pub(crate) fields: BTreeMap<String, Value>,
    pub(crate) arrays: BTreeMap<String, crate::array_data::ArrayData>,
    pub(crate) string_builders: BTreeMap<String, String>,
    /// Enum constants by object marker: declared name and ordinal. A later
    /// call never reruns `<clinit>`, so without these `name()` on a constant a
    /// static holds would lose its answer.
    pub(crate) enum_constants: BTreeMap<String, (String, i32)>,
    /// Next object and array numbers, so a later call never reuses a marker
    /// an earlier one handed out (a reused `java/util/ArrayList#0` would
    /// replace the entrypoint's list in the host's collection table).
    pub(crate) next_object: usize,
    pub(crate) next_array: usize,
    /// Entries the last collection kept, the baseline of the next trigger.
    live: usize,
    /// Texts the storing caller holds outside the host (the game-hook tier's
    /// fields on game objects), roots of this store's collection only.
    roots: Vec<String>,
}

impl JavaHeap {
    /// Instance fields kept, across every object.
    pub fn instance_fields(&self) -> usize {
        self.fields.len()
    }

    /// Arrays kept.
    pub fn arrays(&self) -> usize {
        self.arrays.len()
    }

    /// The instance field `name` (+ `descriptor`) of `owner` on `object`, as
    /// the last stored invocation left it.
    pub fn instance_field(
        &self,
        object: &str,
        owner: &str,
        name_descriptor: &str,
    ) -> Option<&Value> {
        self.fields
            .get(&format!("{object}|{owner}#{name_descriptor}"))
    }
}

/// A position in a host's call log ([`StdHost::invocations`] and its method
/// list, posted events), to roll back what one callback logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallLogMark {
    calls: usize,
    invocations: usize,
    posted_events: usize,
}

impl StdHost {
    /// The objects this host's jar keeps between invocations.
    pub fn resident_heap(&self) -> &JavaHeap {
        &self.heap
    }

    /// Where the call log ends now; see [`Self::rewind_call_log`].
    pub fn call_log_mark(&self) -> CallLogMark {
        CallLogMark {
            calls: self.calls.len(),
            invocations: self.invocations.len(),
            posted_events: self.posted_events.len(),
        }
    }

    /// Drop what was logged after `mark`. A host kept for a server's life runs
    /// callbacks every tick; their calls are not registrations, and keeping
    /// them would grow the log (and the heap's roots) without bound.
    pub fn rewind_call_log(&mut self, mark: CallLogMark) {
        self.calls.truncate(mark.calls);
        self.invocations.truncate(mark.invocations);
        self.posted_events.truncate(mark.posted_events);
    }

    /// Keep `value`, and whatever it reaches, alive across heap collections
    /// until [`Self::release`]: a value the embedder holds from one
    /// invocation to a later one, which the jar's own roots may no longer
    /// name (#226 review).
    ///
    /// An invocation's receiver, arguments and result are kept through that
    /// invocation's own store without this (collections run only when a
    /// top-level call ends, so they are safe while the embedder has them in
    /// hand); one the embedder keeps across later calls must be retained.
    /// Holds are counted: two `retain`s need two `release`s.
    pub fn retain(&mut self, value: &Value) {
        let mut texts = pin_texts(value).into_iter();
        if let Some(own) = texts.next() {
            let entry = self
                .retained
                .entry(own.to_owned())
                .or_insert_with(|| (0, texts.map(str::to_owned).collect()));
            entry.0 += 1;
        }
    }

    /// Drop one [`Self::retain`] hold on `value`: once none is left,
    /// collections may free it (and what only it reached) when the jar no
    /// longer reaches it either.
    pub fn release(&mut self, value: &Value) {
        if let Some(own) = pin_texts(value).first()
            && let Some((holds, _)) = self.retained.get_mut(*own)
        {
            *holds -= 1;
            if *holds == 0 {
                self.retained.remove(*own);
            }
        }
    }

    /// How many distinct values [`Self::retain`] keeps: an embedder that
    /// never releases what it retains shows here as a count that only grows.
    pub fn retained_count(&self) -> usize {
        self.retained.len()
    }

    /// How many runs of freed allocation numbers the host remembers (so a
    /// read through a collected object's handle is refused by name): at most
    /// one more than the live objects, however many a jar made.
    pub fn freed_object_ranges(&self) -> usize {
        self.freed_objects.ranges()
    }

    /// How many collections the host keeps cached for what they belong to
    /// (a map's views, `System.getProperties()`): each lives as long as its
    /// owner, so this follows the live set, not how many maps a jar made.
    pub fn cached_collection_count(&self) -> usize {
        self.cached_collections.len()
    }

    /// How many `Map.Entry` objects the host keeps (`host_map_entry.rs`): each as long as
    /// something reaches it, so this follows the live entries, not how many a jar walked.
    pub fn map_entry_count(&self) -> usize {
        self.map_entries.len()
    }

    /// How many live per-collection identity-epoch entries the host keeps in
    /// `CollectionStore` (pruned automatically on removal, epoch bump, and GC).
    pub fn identity_epoch_count(&self) -> usize {
        self.mutable_collections.identity_epoch_count()
    }

    /// Peak number of `IdentityCollectionTokens` entries held concurrently in
    /// any `Runtime` invocation on this host.
    pub fn peak_identity_token_count(&self) -> usize {
        self.peak_identity_tokens
    }

    /// How many canonical `BlockState` rows the host keeps (`block_states`): each lives as
    /// long as its owning `Block` (reachable directly or through a jar reference to a state),
    /// so this follows live blocks rather than every block a jar ever constructed.
    pub fn block_state_count(&self) -> usize {
        self.block_states.len()
    }

    pub(crate) fn take_resident_heap(&mut self) -> JavaHeap {
        let live = self.heap.live;
        let heap = std::mem::take(&mut self.heap);
        self.heap.live = live;
        heap
    }

    /// Keep `heap` as this host's resident heap, collecting it first when it
    /// grew past the trigger or a bound. The heap is kept even when it is
    /// refused, so a jar's other callbacks still see their objects; the error
    /// fails the callback that grew it.
    pub(crate) fn store_resident_heap(&mut self, mut heap: JavaHeap) -> Result<(), NativeError> {
        heap.live = self.heap.live;
        let roots = std::mem::take(&mut heap.roots);
        let entries = heap_entries(&heap, self);
        // Collect once the heap doubled past what the last collection kept
        // (so a collection's cost is paid once per as many new entries as
        // live ones), or past twice the bound, whichever comes first. The
        // bound is on the live set: garbage between collections may exceed
        // it, which keeps a live set near the bound from collecting on
        // every store (#226 review).
        if !self.collections_paused
            && (entries > heap.live.saturating_mul(2).saturating_add(COLLECT_SLACK)
                || over_count_guard(&heap))
        {
            collect(&mut heap, self, &roots);
            heap.live = heap_entries(&heap, self);
            let bytes = heap_bytes(&heap, self);
            self.heap = heap;
            if bytes > MAX_RESIDENT_BYTES {
                return Err(NativeError::Metadata(format!(
                    "the jar's Java objects hold about {bytes} bytes after collection, over the resident bound of {MAX_RESIDENT_BYTES}"
                )));
            }
            if over_count_bound(&self.heap) {
                return Err(NativeError::Metadata(format!(
                    "the jar's Java objects keep {} fields and {} arrays after collection, over the resident bound of {MAX_RESIDENT_FIELDS} fields or {MAX_RESIDENT_ARRAYS} arrays",
                    self.heap.fields.len(),
                    self.heap.arrays.len()
                )));
            }
        } else {
            self.heap = heap;
        }
        Ok(())
    }

    /// Every text the host holds that may name a jar object: the roots of a
    /// collection, besides the static fields. Conservative by design: a table
    /// that is not listed here would let the collector drop an object the host
    /// still names, so every host table that stores a `Value` or a marker key
    /// is listed, including bootstrap-only ones.
    fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        fn value<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
            out.extend(value_texts(value));
        }
        for field in self.persistent_static_fields.values() {
            value(field, out);
        }
        // What the embedder holds between invocations (`StdHost::retain`).
        for (own, (_, held)) in &self.retained {
            out.push(own);
            out.extend(held.iter().map(String::as_str));
        }
        // Only the logged calls the log is read back for (event
        // registrations, `events::read_back_call`). Every other call is
        // history: a constructor's receiver is the new object itself, and
        // what any call left with the host lives in a table listed here (or
        // keyed by its object, `keyed_texts`). Rooting the whole log kept
        // every object a jar ever made alive (#197).
        for invocation in &self.invocations {
            if !crate::events::read_back_call(&invocation.method) {
                continue;
            }
            if let Some(receiver) = &invocation.receiver {
                value(receiver, out);
            }
            invocation.args.iter().for_each(|arg| value(arg, out));
        }
        for entry in &self.registrations {
            value(&entry.value, out);
        }
        self.posted_events
            .iter()
            .for_each(|event| value(event, out));
        for registration in &self.payload_registrations {
            value(&registration.mod_id, out);
            value(&registration.payload_type, out);
            value(&registration.codec, out);
            value(&registration.handler, out);
        }
        for held in self.holders.values() {
            value(held, out);
        }
        for (key, held) in &self.capability_values {
            out.push(key);
            value(held, out);
        }
        for (key, held) in &self.lazy_optionals {
            out.push(key);
            if let Some(held) = held {
                value(held, out);
            }
        }
        for (key, slots) in &self.item_handlers {
            out.push(key);
            slots.iter().for_each(|slot| value(slot, out));
        }
        out.extend(self.payload_bytes.keys().map(String::as_str));
        out.extend(self.packet_types.keys().map(String::as_str));
        out.extend(self.item_property_builders.keys().map(String::as_str));
        // Per-block metadata (`block_property_builders`, `block_copied_properties`,
        // `block_state_properties`, `state_definition_builders`, `block_default_states`,
        // `unknown_block_state_definitions`) and canonical `block_states` are traced and
        // pruned with their owning block object rather than becoming GC roots.
        // `ItemLike.asItem()`'s bounded `Item.BY_BLOCK` knowledge is keyed by
        // allocated mod objects. It is traced and pruned with the object
        // rather than becoming a lifetime root.
        out.extend(self.resource_streams.keys().map(String::as_str));
        // The command tree's callbacks and requirement lambdas (`brigadier.rs`).
        self.brigadier_root_texts(out);
        // A server's saved data (`facade_saved_data.rs`).
        self.saved_data.root_texts(out);
        // Fabric resource condition types' ids and codecs (`fabric_resource_conditions.rs`).
        for held in self.fabric_resource_conditions.values() {
            value(held, out);
        }
        // Fabric creative tab builders' and tabs' recorded arguments (`host_creative_tab.rs`).
        for held in self.creative_tab.values() {
            value(held, out);
        }
        // Fabric biome modifications' ids, selectors and modifiers (`fabric_biome_modifications.rs`).
        for held in self.fabric_biome_modifications.values() {
            value(held, out);
        }
        // Fabric synchronized recipe serializers (`fabric_api_slice2.rs`, static strong root).
        for held in self.recipe_sync_serializers.values() {
            value(held, out);
        }
        // NeoForge attachment types and live values (`neoforge_attachments.rs`).
        self.neoforge_attachments.root_texts(out);
        // The objects the loader built and keeps (an entrypoint instance,
        // a `DeferredRegister`'s values) and the JDK's shared empties.
        self.constructed_entrypoints
            .values()
            .for_each(|held| value(held, out));
        for (key, held) in &self.deferred_registers {
            out.push(key);
            if let Some((held, _)) = held {
                value(held, out);
            }
        }
        out.extend(self.shared_empties.values().map(String::as_str));
        // Table rows and iterators are host-owned roots. Their values may be
        // jar objects, so the collector must see them across callbacks.
        self.guava_tables.root_texts(out);
        // Recipe holders, component patches, and decoded mod recipes.
        self.recipes.root_texts(out);
    }

    /// The host's own state that may name a jar object without a table this
    /// module reads field by field: each one's whole `Debug` text, scanned
    /// for markers as a root (keeping too much, never too little).
    fn opaque_root_texts(&self, out: &mut Vec<String>) {
        out.push(format!("{:?}", self.neoforge_bus));
        out.push(format!("{:?}", self.imc));
        out.push(format!("{:?}", self.threads));
        out.push(format!("{:?}", self.levels));
        out.push(format!("{:?}", self.capability_requests));
        out.push(format!("{:?}", self.client));
        out.push(format!("{:?}", self.game_event));
        out.push(format!("{:?}", self.fabric_entrypoint_containers));
        out.push(format!("{:?}", self.content_builders));
        out.push(format!("{:?}", self.holder_rows));
    }

    /// What the host keeps for the object `marker` in a table keyed by it: a
    /// view's source, a sublist's parent, an iterator's collection and
    /// guard, a comparator, a reference's referent, a throwable's cause, a
    /// collection's layout. Edges from `marker` when it is collectable,
    /// roots when not.
    fn keyed_texts(&self, marker: &str, out: &mut Vec<String>) {
        fn push(entry: Option<&impl std::fmt::Debug>, out: &mut Vec<String>) {
            if let Some(entry) = entry {
                out.push(format!("{entry:?}"));
            }
        }
        push(self.io_paths.get(marker), out);
        push(self.set_from_map_views.get(marker), out);
        push(self.unmodifiable_collections.get(marker), out);
        push(self.reversed_views.get(marker), out);
        push(self.synchronized_views.get(marker), out);
        push(self.view_copies.get(marker), out);
        push(self.sub_lists.get(marker), out);
        push(self.iterator_sources.get(marker), out);
        push(self.iterator_last.get(marker), out);
        push(self.list_iterators.get(marker), out);
        push(self.iterator_guards.get(marker), out);
        push(self.collection_comparators.get(marker), out);
        push(self.references.get(marker), out);
        if self.random_sources.contains_key(marker) {
            out.push(format!("{:?}", self.random_sources.get(marker)));
        }
        push(self.throwable_causes.get(marker), out);
        push(self.throwable_suppressed.get(marker), out);
        push(self.hash_tables.get(marker), out);
        push(self.red_black_trees.get(marker), out);
        push(self.probe_tables.get(marker), out);
        push(self.block_items.get(marker), out);
        push(self.state_definition_builders.get(marker), out);
        // An `Arrays.asList` view's array, which its `get`/`set` work on, and
        // the view an iterator over one follows.
        push(self.array_view_array(marker), out);
        push(self.array_view_of_iterator(marker), out);
        if self.render_objects.contains_key(marker) {
            out.push(marker.to_owned());
        }
        push(self.ranged_attributes.get(marker), out);
        if let Some(text) = crate::menu_models::keyed_text(&self.menu_states, marker) {
            out.push(text);
        }
        if let Some(lines) = self.tooltip_sinks.get(marker) {
            out.push(marker.to_owned());
            for line in lines {
                for text in value_texts(line) {
                    out.push(text.to_owned());
                }
            }
        }
    }

    /// Every key of the tables [`Self::keyed_texts`] reads, and of the other
    /// tables kept per object.
    fn keyed_table_keys(&self) -> impl Iterator<Item = &str> {
        self.io_paths
            .keys()
            .chain(self.set_from_map_views.keys())
            .chain(self.unmodifiable_collections.keys())
            .chain(self.reversed_views.keys())
            .chain(self.synchronized_views.keys())
            .chain(self.view_copies.keys())
            .chain(self.sub_lists.keys())
            .chain(self.iterator_sources.keys())
            .chain(self.list_iterators.keys())
            .chain(self.iterator_guards.keys())
            .chain(self.collection_comparators.keys())
            .chain(self.references.keys())
            .chain(self.random_sources.keys())
            .chain(self.throwable_causes.keys())
            .chain(self.throwable_suppressed.keys())
            .chain(self.throwable_messages.keys())
            .chain(self.hash_tables.keys())
            .chain(self.red_black_trees.keys())
            .chain(self.probe_tables.keys())
            .chain(self.constructed_items.iter())
            .chain(self.constructed_blocks.iter())
            .chain(self.block_items.keys())
            .chain(self.item_required_features.keys())
            .chain(self.block_property_builders.keys())
            .chain(self.block_state_properties.keys())
            .chain(self.block_copied_properties.iter())
            .chain(self.state_definition_builders.keys())
            .chain(self.block_default_states.keys())
            .chain(self.unknown_block_state_definitions.iter())
            .chain(self.array_view_markers())
            .chain(self.render_objects.keys())
            .chain(self.ranged_attributes.keys())
            .chain(self.voxel_shapes.keys())
            .chain(self.menu_states.keys())
            .chain(self.tooltip_sinks.keys())
            .map(String::as_str)
    }

    /// Drop every per-object row of a collected object (`keep` false).
    fn prune_keyed_tables(&mut self, keep: impl Fn(&str) -> bool) {
        self.io_paths.retain(|key, _| keep(key));
        self.set_from_map_views.retain(|key, _| keep(key));
        self.unmodifiable_collections.retain(|key, _| keep(key));
        self.reversed_views.retain(|key, _| keep(key));
        self.synchronized_views.retain(|key, _| keep(key));
        self.view_copies.retain(|key, _| keep(key));
        self.view_kinds.retain(|key, _| keep(key));
        self.sub_lists.retain(|key, _| keep(key));
        self.detached_sub_lists.retain(|key| keep(key));
        self.iterator_sources.retain(|key, _| keep(key));
        self.iterator_last.retain(|key, _| keep(key));
        self.list_iterators.retain(|key, _| keep(key));
        self.iterator_guards.retain(|key, _| keep(key));
        self.collection_comparators.retain(|key, _| keep(key));
        self.references.retain(|key, _| keep(key));
        self.random_sources.retain(|key, _| keep(key));
        self.throwable_causes.retain(|key, _| keep(key));
        self.throwable_suppressed.retain(|key, _| keep(key));
        self.block_property_builders.retain(|key, _| keep(key));
        self.block_state_properties.retain(|key, _| keep(key));
        self.block_copied_properties.retain(|key| keep(key));
        self.state_definition_builders.retain(|key, _| keep(key));
        self.block_default_states.retain(|key, _| keep(key));
        self.unknown_block_state_definitions.retain(|key| keep(key));
        self.throwable_messages.retain(|key, _| keep(key));
        self.hash_tables.retain(|key, _| keep(key));
        self.red_black_trees.retain(|key, _| keep(key));
        self.probe_tables.retain(|key, _| keep(key));
        self.constructed_items.retain(|key| keep(key));
        self.constructed_blocks.retain(|key| keep(key));
        self.block_items.retain(|key, _| keep(key));
        self.item_required_features.retain(|key, _| keep(key));
        self.mod_stamps.retain(|key, _| keep(key));
        self.deque_layouts.retain(|key, _| keep(key));
        self.empty_collections.retain(|key| keep(key));
        self.ranged_attributes.retain(|key, _| keep(key));
        self.voxel_shapes.retain(|key, _| keep(key));
        self.menu_states.retain(|key, _| keep(key));
        self.tooltip_sinks.retain(|key, _| keep(key));
        self.jdk_list_of.retain(|key| keep(key));
        self.render_objects.retain(|key, _| keep(key));
    }
}

/// Whether a marker names an object the interpreter or host allocated, which
/// may be dropped once nothing reaches it. Anything else that holds state (an
/// entrypoint instance named by its class, a game object of the hook tier, a
/// host marker of another shape) is kept for the host's life, and its contents
/// are roots.
fn collectable(marker: &str) -> bool {
    if marker.starts_with(GAME_OBJECT_PREFIX) {
        return false;
    }
    // `new` names an object `<class>#<n>` (`Runtime::new_object`): the
    // common case, checked first.
    if marker.rsplit_once('#').is_some_and(|(class, number)| {
        !class.is_empty() && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
    }) {
        return true;
    }
    // A host-created ranged attribute or block property builder follows the heap.
    if let Some(serial) = marker
        .strip_prefix("ranged-attribute/")
        .or_else(|| marker.strip_prefix("block/properties/"))
    {
        return !serial.is_empty() && serial.bytes().all(|b| b.is_ascii_digit());
    }
    // A host-created VoxelShape (`cube/<n>` or `array/<n>`) follows the heap; the shared
    // singletons (`empty`, `block`) live outside `voxel_shapes` and are never collected.
    if let Some(serial) = marker
        .strip_prefix("cube/")
        .or_else(|| marker.strip_prefix("array/"))
    {
        return !serial.is_empty() && serial.bytes().all(|b| b.is_ascii_digit());
    }
    // An iterator's or enumeration's key (`host_iterator.rs`); the shared empty ones
    // (`iterator/empty`) are never collected.
    if let Some(rest) = marker
        .strip_prefix("iterator/")
        .or_else(|| marker.strip_prefix("enumeration/"))
    {
        return rest.ends_with(|c: char| c.is_ascii_digit());
    }
    // A host map entry (`host_map_entry.rs`): `<class>#entry<n>`/`<class>#node<n>`,
    // or a host-synthesized menu model object (`menu_models.rs`): `<class>#menu<n>`.
    // Non-numeric prefixes keep them out of `jar_object::parse_key` / `freed_objects`.
    if let Some((_, serial)) = marker.rsplit_once('#')
        && let Some(number) = serial
            .strip_prefix("entry")
            .or_else(|| serial.strip_prefix("node"))
            .or_else(|| serial.strip_prefix("menu"))
    {
        return !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
    }
    // A collection the host made (`<class>#neutral<n>`, `#immutable<n>`,
    // `#reversed<n>`, `jar_object::collection_key`): an object like any other.
    crate::jar_object::collection_key(marker).is_some()
}

/// The texts a hold keeps: a handle's key (and a callback's held values), or a reference's text that may name
/// an object (a marker, a callback's captures). A plain `String` holds no
/// object, and pinning each would grow the set by every string an
/// invocation answered.
fn pin_texts(value: &Value) -> Vec<&str> {
    match value {
        Value::Ref(Some(text)) if crate::value_class::is_plain_string(text) => Vec::new(),
        other => value_texts(other),
    }
}

/// The receiver of an instance-field key (`receiver|owner#name desc`).
fn field_receiver(key: &str) -> &str {
    key.rsplit_once('|').map_or(key, |(receiver, _)| receiver)
}

/// Kotlin lazy cached values share the resident Java field table. The holder
/// key is the field's ownership edge, so unreachable lazy state is pruned with
/// the heap collector rather than retained forever (#301).
const KOTLIN_LAZY_FIELD_PREFIX: &str = "kotlin-lazy|";

fn kotlin_lazy_field_handle(key: &str) -> Option<&str> {
    key.strip_prefix(KOTLIN_LAZY_FIELD_PREFIX)
}

fn heap_entries(heap: &JavaHeap, host: &StdHost) -> usize {
    heap.fields.len()
        + heap.arrays.len()
        + heap.string_builders.len()
        + heap.enum_constants.len()
        + host.mutable_collections.len()
        + host.repeated_collections.len()
        + host.set_from_map_views.len()
        + host.random_sources.len()
        + host.iterators.len()
        + host.component_states.len()
        + host.ranged_attributes.len()
        + host.voxel_shapes.len()
        + host.block_states.len()
        + host.entity_registration.entry_count()
        + host.tool_material.entry_count()
        + host.vec2.entry_count()
        + host.guava_tables.entry_count()
        + host.recipes.entry_count()
}

fn over_count_bound(heap: &JavaHeap) -> bool {
    heap.fields.len() > MAX_RESIDENT_FIELDS || heap.arrays.len() > MAX_RESIDENT_ARRAYS
}

/// Twice the bound: a heap this large collects whatever its live baseline.
fn over_count_guard(heap: &JavaHeap) -> bool {
    heap.fields.len() > 2 * MAX_RESIDENT_FIELDS || heap.arrays.len() > 2 * MAX_RESIDENT_ARRAYS
}

/// The allocation numbers of the jar objects collections freed, as ranges
/// (`start → end`, exclusive). A field of a freed object read or written later
/// is a missed root, which the runtime refuses by name instead of answering
/// the field's default (`Runtime::live_field_key`, #226 review).
///
/// Each collection rebuilds it as every number below the heap's next one
/// that the collection did not find live ([`collect`]). Keys are never
/// reused, so a number freed once stays freed, and the gaps between ranges
/// are numbers found live: the ranges are as many as the live set at most,
/// however many objects a jar made. Recording only the numbers that had a
/// row split the ranges at every allocation that never got one (a
/// `new Object()` lock), so the set grew with every such allocation
/// (#226 re-check 3).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FreedObjects {
    ranges: BTreeMap<usize, usize>,
}

impl FreedObjects {
    pub(crate) fn contains(&self, n: usize) -> bool {
        self.ranges
            .range(..=n)
            .next_back()
            .is_some_and(|(_, &end)| n < end)
    }

    /// Add every number below `limit` except the `live` ones (sorted or
    /// not). A number freed before stays freed even if a live text names it
    /// now: that is a handle a missed root kept, and reading through it stays
    /// loud. The numbers left out are live ones, so the ranges stay as many
    /// as the live set at most.
    fn rebuild(&mut self, limit: usize, mut live: Vec<usize>) {
        live.sort_unstable();
        live.dedup();
        let mut ranges: Vec<(usize, usize)> =
            std::mem::take(&mut self.ranges).into_iter().collect();
        let mut start = 0;
        for n in live.into_iter().take_while(|&n| n < limit) {
            if start < n {
                ranges.push((start, n));
            }
            start = n + 1;
        }
        if start < limit {
            ranges.push((start, limit));
        }
        ranges.sort_unstable();
        for (start, end) in ranges {
            match self.ranges.iter_mut().next_back() {
                Some((_, last_end)) if start <= *last_end => *last_end = (*last_end).max(end),
                _ => {
                    self.ranges.insert(start, end);
                }
            }
        }
    }

    /// How many ranges the set keeps (bounded by the live set).
    pub(crate) fn ranges(&self) -> usize {
        self.ranges.len()
    }

    /// Whether the object `key` (`class#n`) was collected. A key that is
    /// never collected (a game object's) is never freed.
    pub(crate) fn holds_key(&self, key: &str) -> bool {
        if !collectable(key) {
            return false;
        }
        crate::jar_object::parse_key(key)
            .and_then(|(_, number)| number.parse().ok())
            .is_some_and(|n| self.contains(n))
    }

    /// Whether a host-synthesized menu object `key` (`class#menu<n>`) was collected.
    pub(crate) fn holds_menu_key(&self, key: &str) -> bool {
        if !collectable(key) {
            return false;
        }
        crate::jar_object::menu_object_key(key)
            .and_then(|(_, number)| number.parse().ok())
            .is_some_and(|n| self.contains(n))
    }
}

fn value_bytes(value: &Value) -> usize {
    value_text(value).map_or(8, str::len)
}

/// Every text a value names other objects through: [`value_text`], and the
/// texts of every value a handle holds exactly (a callback's row, the values
/// an optional or a map entry was made of), which its key names only in
/// bounded text (#156 P15b). Every root and heap walker uses this.
///
/// The walk has no depth bound (#261 round-3 S2): a capture held 70 links
/// down a chain a jar built in a loop is as reachable as one held at the
/// top. It is a loop over a worklist, so a chain 200,000 deep takes no native
/// stack, and it visits each shared row or held list once, so a chain that
/// holds one operand twice per link (`p = p.or(p)`) is not walked 2^n times.
pub(crate) fn value_texts(value: &Value) -> Vec<&str> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        if let Some(text) = value_text(value) {
            out.push(text);
        }
        let Value::Host(handle) = value else {
            continue;
        };
        if let Some(row) = handle.row()
            && seen.insert(std::ptr::from_ref(row).cast::<u8>())
        {
            pending.extend(row.values());
        }
        let held = handle.held();
        if !held.is_empty() && seen.insert(held.as_ptr().cast::<u8>()) {
            pending.extend(held);
        }
    }
    out
}

/// The text a value names other objects through: a reference's, or a host
/// handle's key (an `Optional`'s key is its held value's marker, so a jar
/// object held only by an optional stays reachable). Every root walker uses
/// this, never a projection that drops a kind (#226 review).
pub(crate) fn value_text(value: &Value) -> Option<&str> {
    match value {
        Value::Ref(Some(text)) => Some(text),
        Value::Host(host) => Some(host.key()),
        _ => None,
    }
}

fn heap_bytes(heap: &JavaHeap, host: &StdHost) -> usize {
    let values = |values: &[Value]| values.iter().map(value_bytes).sum::<usize>();
    heap.fields
        .iter()
        .map(|(key, value)| key.len() + value_bytes(value))
        .chain(
            // A `byte[]` takes a byte per element (`array_data.rs`); a reference array its values.
            heap.arrays.iter().map(|(key, array)| {
                key.len()
                    + match array.bytes() {
                        Some(bytes) => bytes.len(),
                        None => values(array.references()),
                    }
            }),
        )
        .chain(
            heap.string_builders
                .iter()
                .map(|(key, text)| key.len() + text.len()),
        )
        .chain(
            host.mutable_collections
                .iter()
                .map(|(key, elements)| key.len() + values(elements)),
        )
        .chain(
            host.repeated_collections
                .iter()
                .map(|(key, (_, value))| key.len() + value_bytes(value)),
        )
        .chain(
            host.set_from_map_views
                .iter()
                .map(|(key, source)| key.len() + source.len()),
        )
        .chain(host.iterators.iter().map(|(key, entry)| {
            key.len() + values(&entry.values) + entry.origin.as_ref().map_or(0, value_bytes)
        }))
        .chain(host.component_states.iter().map(|(key, state)| {
            key.len()
                + serde_json::Value::Object(state.base.clone())
                    .to_string()
                    .len()
                + values(&state.siblings)
                + state.args.as_deref().map_or(0, values)
        }))
        .chain(host.ranged_attributes.iter().map(|(key, state)| {
            key.len()
                + state.description_id.as_ref().map_or(0, String::len)
                + 4 * std::mem::size_of::<f64>()
        }))
        .chain(
            host.voxel_shapes
                .iter()
                .map(|(key, shape)| key.len() + shape.estimated_bytes()),
        )
        .chain(host.block_states.iter().map(|(key, (block, values))| {
            key.len() + block.len() + values.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
        }))
        .chain(std::iter::once(host.guava_tables.estimated_bytes()))
        .sum()
}

/// The object a cached collection's identity (`StdHost::cached_collection`:
/// a handle's encoded kind and key, or a marker's text) names, when it is
/// one a collection may free.
fn cache_owner(identity: &str) -> Option<String> {
    let key = match crate::host_ref::HostRef::decode(identity) {
        Some(handle) => handle.key().to_owned(),
        None => identity.to_owned(),
    };
    collectable(&key).then_some(key)
}

/// Note the allocation number of every `#<digits>` in `text` (`class#n`
/// keys, embedded ones included): over-reading only keeps a number out of
/// the freed set, which costs a diagnostic, never an answer.
fn note_numbers(text: &str, out: &mut Vec<usize>) {
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(offset) = bytes[at..].iter().position(|&b| b == b'#') {
        let start = at + offset + 1;
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
        if end > start
            && let Ok(number) = text[start..end].parse()
        {
            out.push(number);
        }
        at = end.max(start);
    }
}

/// Note the allocation number of every `#menu<digits>` in `text` so live
/// host-synthesized menu objects stay out of `freed_menu_objects`.
fn note_menu_numbers(text: &str, out: &mut Vec<usize>) {
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(offset) = bytes[at..].windows(5).position(|w| w == b"#menu") {
        let start = at + offset + 5;
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
        if end > start
            && let Ok(number) = text[start..end].parse()
        {
            out.push(number);
        }
        at = end.max(start);
    }
}

/// The collectable markers of a heap and the distinct lengths they come in,
/// which is what [`scan`] tries at each digit-run end.
struct Collectable {
    markers: HashSet<String>,
    lengths: Vec<usize>,
}

impl Collectable {
    fn new(heap: &JavaHeap, host: &StdHost) -> Self {
        // Field keys are sorted, so an object's fields are adjacent: each
        // receiver is checked once.
        let mut previous = "";
        let markers: HashSet<String> = heap
            .fields
            .keys()
            .map(|key| field_receiver(key))
            .filter(move |receiver| std::mem::replace(&mut previous, receiver) != *receiver)
            .chain(heap.arrays.keys().map(String::as_str))
            .chain(heap.string_builders.keys().map(String::as_str))
            .chain(heap.enum_constants.keys().map(String::as_str))
            .chain(host.mutable_collections.keys().map(String::as_str))
            .chain(host.repeated_collections.keys().map(String::as_str))
            .chain(host.set_from_map_views.keys().map(String::as_str))
            .chain(host.random_sources.keys().map(String::as_str))
            .chain(host.iterators.keys().map(String::as_str))
            .chain(host.enumerations.keys().map(String::as_str))
            .chain(host.component_states.keys().map(String::as_str))
            .chain(host.map_entries.keys().map(String::as_str))
            .chain(host.block_states.values().map(|(block, _)| block.as_str()))
            .chain(host.keyed_table_keys())
            .filter(|marker| collectable(marker))
            .map(str::to_owned)
            .collect();
        let lengths: BTreeSet<usize> = markers.iter().map(String::len).collect();
        Self {
            markers,
            lengths: lengths.into_iter().collect(),
        }
    }

    /// Every collectable marker `text` names, itself or embedded: each one
    /// ends where a run of digits ends.
    fn scan(&self, text: &str, out: &mut Vec<String>) {
        let bytes = text.as_bytes();
        for end in 1..=bytes.len() {
            if !bytes[end - 1].is_ascii_digit() || bytes.get(end).is_some_and(u8::is_ascii_digit) {
                continue;
            }
            for &length in &self.lengths {
                let Some(start) = end.checked_sub(length) else {
                    break;
                };
                if let Some(candidate) = text.get(start..end)
                    && self.markers.contains(candidate)
                {
                    out.push(candidate.to_owned());
                }
            }
        }
    }
}

fn mark_kotlin_lazy_states(
    heap: &JavaHeap,
    text: &str,
    reached: &mut HashSet<String>,
    pending: &mut Vec<String>,
    collectable: &Collectable,
) {
    for (field, value) in &heap.fields {
        let Some(holder) = kotlin_lazy_field_handle(field) else {
            continue;
        };
        if holder != text && !text.contains(holder) {
            continue;
        }
        if !reached.insert(holder.to_owned()) {
            continue;
        }
        if let Some(value) = value_text(value) {
            collectable.scan(value, pending);
            mark_kotlin_lazy_states(heap, value, reached, pending, collectable);
        }
    }
}

fn text_has_shape_probe(text: &str) -> bool {
    crate::voxel_shape::parse_shape_probe_key(text).is_some()
        || text.contains("hsprobe:")
        || text.contains("ShapeProbe")
}

fn value_has_shape_probe(value: &Value) -> bool {
    let mut seen = HashSet::new();
    let mut pending = vec![value];
    while let Some(current) = pending.pop() {
        match current {
            Value::Host(handle) => {
                if handle.kind() == crate::host_ref::HostKind::ShapeProbe
                    || text_has_shape_probe(handle.key())
                {
                    return true;
                }
                if let Some(row) = handle.row()
                    && seen.insert(std::ptr::from_ref(row).cast::<u8>())
                {
                    pending.extend(row.values());
                }
                let held = handle.held();
                if !held.is_empty() && seen.insert(held.as_ptr().cast::<u8>()) {
                    pending.extend(held);
                }
            }
            Value::Ref(Some(text)) if text.contains("hsprobe:") => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Whether any live `HostKind::ShapeProbe` handle remains in `heap`, `host`, or
/// `extra` after collection swept unreachable objects. Only called when
/// `host.retained_shape_probes` is set so clean hosts pay zero cost.
fn has_live_shape_probe(heap: &JavaHeap, host: &StdHost, extra: &[String]) -> bool {
    if extra.iter().any(|text| text_has_shape_probe(text)) {
        return true;
    }
    let mut roots = Vec::new();
    host.root_texts(&mut roots);
    if roots.into_iter().any(text_has_shape_probe) {
        return true;
    }
    let mut opaque = Vec::new();
    host.opaque_root_texts(&mut opaque);
    if opaque.iter().any(|text| text_has_shape_probe(text)) {
        return true;
    }
    if heap.fields.values().any(value_has_shape_probe)
        || heap
            .arrays
            .values()
            .flat_map(crate::array_data::ArrayData::references)
            .any(value_has_shape_probe)
        || host
            .mutable_collections
            .values()
            .flatten()
            .any(value_has_shape_probe)
        || host
            .repeated_collections
            .values()
            .map(|(_, value)| value)
            .any(value_has_shape_probe)
        || host
            .iterators
            .values()
            .flat_map(|entry| entry.values.iter().chain(entry.origin.iter()))
            .any(value_has_shape_probe)
        || host.iterator_last.values().any(value_has_shape_probe)
        || host
            .enumerations
            .values()
            .map(crate::host_iterator::EnumerationOver::held)
            .any(value_has_shape_probe)
        || host
            .component_states
            .values()
            .flat_map(|state| state.siblings.iter().chain(state.args.iter().flatten()))
            .any(value_has_shape_probe)
        || host
            .map_entries
            .keys()
            .flat_map(|key| host.map_entries.held(key))
            .any(value_has_shape_probe)
    {
        return true;
    }
    if host
        .cached_collections
        .iter()
        .any(|((identity, _), cached)| {
            text_has_shape_probe(identity) || text_has_shape_probe(cached)
        })
    {
        return true;
    }
    let mut keyed = Vec::new();
    for key in host.keyed_table_keys() {
        keyed.clear();
        host.keyed_texts(key, &mut keyed);
        if keyed.iter().any(|text| text_has_shape_probe(text)) {
            return true;
        }
    }
    false
}

/// Drop every collectable object of `heap` (and every collectable collection
/// or iterator of `host`) that nothing reaches from the roots.
///
/// Roots: the static fields and host tables ([`StdHost::root_texts`]), the
/// caller's `extra` texts, and the contents of every object that is not
/// collectable. Edges: an object's instance fields, array elements,
/// collection elements and iterator snapshot.
pub(crate) fn collect(heap: &mut JavaHeap, host: &mut StdHost, extra: &[String]) {
    let collectable = Collectable::new(heap, host);
    let mut render_roots = Vec::new();
    host.root_texts(&mut render_roots);
    render_roots.extend(extra.iter().map(String::as_str));
    let render_live = crate::render_math::reachable_keys(&host.render_objects, render_roots);
    if collectable.markers.is_empty() {
        host.render_objects
            .retain(|key, _| render_live.contains(key));
        host.render_handles.retain(|key| render_live.contains(key));
        if host.retained_shape_probes {
            host.retained_shape_probes = has_live_shape_probe(heap, host, extra);
        }
        return;
    }
    let mut pending = Vec::new();
    let mut reached_kotlin_lazy = HashSet::new();
    // Every allocation number a text the trace reaches names: the objects
    // found live, those without a row of their own included (`FreedObjects`).
    let mut live = Vec::new();
    let mut live_menu = Vec::new();
    {
        let mut roots: Vec<&str> = extra.iter().map(String::as_str).collect();
        host.root_texts(&mut roots);
        // Opaque state, and what the per-object tables keep for an object
        // that is never collected, are roots too.
        let mut owned = Vec::new();
        host.opaque_root_texts(&mut owned);
        for key in host.keyed_table_keys() {
            if !collectable.markers.contains(key) {
                host.keyed_texts(key, &mut owned);
            }
        }
        // A collection cached for something that is never collected
        // (`System.getProperties()`, a mod container's root paths) is a root.
        for ((identity, _), cached) in &host.cached_collections {
            if !cache_owner(identity).is_some_and(|owner| collectable.markers.contains(&owner)) {
                owned.push(cached.clone());
            }
        }
        roots.extend(owned.iter().map(String::as_str));
        let kept_values = heap
            .fields
            .iter()
            .filter(|(key, _)| !collectable.markers.contains(field_receiver(key)))
            .map(|(_, value)| value)
            .chain(
                host.mutable_collections
                    .iter()
                    .filter(|(key, _)| !collectable.markers.contains(key.as_str()))
                    .flat_map(|(_, elements)| elements),
            )
            .chain(
                host.iterators
                    .iter()
                    .filter(|(key, _)| !collectable.markers.contains(key.as_str()))
                    .flat_map(|(_, entry)| entry.values.iter().chain(entry.origin.iter())),
            );
        for value in kept_values {
            roots.extend(value_texts(value));
        }
        for text in roots {
            mark_kotlin_lazy_states(
                heap,
                text,
                &mut reached_kotlin_lazy,
                &mut pending,
                &collectable,
            );
            collectable.scan(text, &mut pending);
            note_numbers(text, &mut live);
            note_menu_numbers(text, &mut live_menu);
        }
    }
    // The objects any per-object table keeps a row for: only these need
    // `keyed_texts`'s lookups as the trace reaches them.
    let keyed_keys: HashSet<String> = host.keyed_table_keys().map(str::to_owned).collect();
    // The collections cached for an object (a map's `keySet()`/`values()`/
    // `entrySet()`, #233): edges from it, so each lives as long as its map.
    let mut cached_for: HashMap<String, Vec<&str>> = HashMap::new();
    for ((identity, _), cached) in &host.cached_collections {
        if let Some(owner) = cache_owner(identity)
            && collectable.markers.contains(&owner)
        {
            cached_for.entry(owner).or_default().push(cached);
        }
    }
    let mut reached = BTreeSet::new();
    let mut found = Vec::new();
    while let Some(marker) = pending.pop() {
        if !reached.insert(marker.clone()) {
            continue;
        }
        let prefix = format!("{marker}|");
        let fields = heap
            .fields
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .map(|(_, value)| value);
        // A typed array holds no references (`array_data.rs`): nothing to trace.
        let arrays = heap
            .arrays
            .get(&marker)
            .into_iter()
            .flat_map(crate::array_data::ArrayData::references);
        let collections = host.mutable_collections.get(&marker).into_iter().flatten();
        let repeated = host
            .repeated_collections
            .get(&marker)
            .into_iter()
            .map(|(_, value)| value);
        let iterators = host
            .iterators
            .get(&marker)
            .into_iter()
            .flat_map(|entry| entry.values.iter().chain(entry.origin.iter()));
        let iterator_last = host.iterator_last.get(&marker);
        // An enumeration's iterator or vector (`host_iterator.rs`).
        let enumerations = host
            .enumerations
            .get(&marker)
            .map(crate::host_iterator::EnumerationOver::held);
        // A changed chat component's appended siblings and component
        // arguments (`host_component.rs`).
        let components = host
            .component_states
            .get(&marker)
            .into_iter()
            .flat_map(|state| state.siblings.iter().chain(state.args.iter().flatten()));
        // A map entry's key and value, and a node's map (`host_map_entry.rs`).
        let entries = host.map_entries.held(&marker);
        if let Some(map) = host.map_entries.map_of(&marker) {
            collectable.scan(map, &mut found);
        }
        for value in fields
            .chain(arrays)
            .chain(collections)
            .chain(repeated)
            .chain(iterators)
            .chain(iterator_last)
            .chain(enumerations)
            .chain(components)
            .chain(entries)
        {
            for text in value_texts(value) {
                mark_kotlin_lazy_states(
                    heap,
                    text,
                    &mut reached_kotlin_lazy,
                    &mut found,
                    &collectable,
                );
                collectable.scan(text, &mut found);
                note_numbers(text, &mut live);
                note_menu_numbers(text, &mut live_menu);
            }
        }
        for cached in cached_for.get(&marker).into_iter().flatten() {
            collectable.scan(cached, &mut found);
            note_numbers(cached, &mut live);
            note_menu_numbers(cached, &mut live_menu);
        }
        // What the host keeps for this object in a table keyed by it.
        if keyed_keys.contains(&marker) {
            let mut keyed = Vec::new();
            host.keyed_texts(&marker, &mut keyed);
            for text in &keyed {
                mark_kotlin_lazy_states(
                    heap,
                    text,
                    &mut reached_kotlin_lazy,
                    &mut found,
                    &collectable,
                );
                collectable.scan(text, &mut found);
                note_numbers(text, &mut live);
                note_menu_numbers(text, &mut live_menu);
            }
        }
        pending.extend(found.drain(..).filter(|next| !reached.contains(next)));
    }
    let keep = |marker: &str| !collectable.markers.contains(marker) || reached.contains(marker);
    // Every number below the heap's next one that was not found live is
    // freed, so a later read of its fields is refused by name rather than
    // answered with defaults. A number with no row that nothing reached is
    // freed too: that keeps the ranges as few as the live numbers between
    // them (#226 re-check 3), and an object with no row has nothing a stale
    // handle could misread.
    let mut limit = heap.next_object;
    for marker in &collectable.markers {
        if let Some((_, number)) = crate::jar_object::parse_key(marker)
            && let Ok(number) = number.parse::<usize>()
        {
            if reached.contains(marker) {
                live.push(number);
            }
            limit = limit.max(number.saturating_add(1));
        } else if let Some((_, number)) = crate::jar_object::menu_object_key(marker)
            && let Ok(number) = number.parse::<usize>()
            && reached.contains(marker)
        {
            live_menu.push(number);
        }
    }
    host.freed_objects.rebuild(limit, live);
    host.freed_menu_objects
        .rebuild(host.next_menu_object as usize, live_menu);
    // What the host keeps for an object with no row of its own (a
    // `ResourceLocation`'s id, a `UUID`'s bits, a `Locale`'s tags) goes with
    // it. These name no other object, so they are never edges.
    let freed = &host.freed_objects;
    host.identifier_objects
        .retain(|key, _| !freed.holds_key(key));
    host.constructed_uuids
        .retain(|key, _| !freed.holds_key(key));
    host.locales.retain(|key, _| !freed.holds_key(key));
    // Canonical BlockState markers live exactly as long as their owning Block:
    // they are not GC roots themselves, are kept alive by their block or by any
    // jar reference to `blockstate/<block>[...]` (which `Collectable::scan`
    // resolves to `<block>`), and are dropped when `<block>` is collected.
    host.block_states
        .retain(|_, (block, _)| keep(block) && !freed.holds_key(block));
    // A cached collection goes with what it was cached for, and an entry
    // whose collection went is rebuilt on its next call (#226 re-check 4:
    // every map's views used to stay cached forever).
    host.cached_collections.retain(|(identity, _), cached| {
        keep(cached)
            && cache_owner(identity).is_none_or(|owner| keep(&owner) && !freed.holds_key(&owner))
    });
    heap.fields.retain(|key, _| {
        if let Some(holder) = kotlin_lazy_field_handle(key) {
            reached_kotlin_lazy.contains(holder)
        } else {
            keep(field_receiver(key))
        }
    });
    heap.arrays.retain(|key, _| keep(key));
    heap.string_builders.retain(|key, _| keep(key));
    heap.enum_constants.retain(|key, _| keep(key));
    host.mutable_collections.retain_markers(|key| keep(key));
    host.repeated_collections.retain(|key, _| keep(key));
    host.iterators.retain(|key, _| keep(key));
    host.iterator_last.retain(|key, _| keep(key));
    host.enumerations.retain(|key, _| keep(key));
    host.component_states.retain(|key, _| keep(key));
    host.map_entries.retain(keep);
    host.entity_registration.retain_keys(keep);
    host.tool_material.retain_keys(keep);
    host.vec2.retain_keys(keep);
    host.prune_keyed_tables(keep);
    host.render_objects
        .retain(|key, _| render_live.contains(key));
    host.render_handles.retain(|key| render_live.contains(key));
    // A collected `Arrays.asList` view mirrors nothing any more.
    host.prune_array_views();
    // Disarm the per-opcode shape-probe check once GC confirms no live
    // `HostKind::ShapeProbe` handles remain anywhere in heap or host state.
    if host.retained_shape_probes {
        host.retained_shape_probes = has_live_shape_probe(heap, host, extra);
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Continue from the objects earlier invocations of this jar left on its
    /// host. Answers whether the host keeps a resident heap; when it does not,
    /// the runtime keeps whatever it was built with, as before.
    ///
    /// Entries the runtime was built with win over the host's: a caller that
    /// seeds instance state (the game-hook tier's state from before the
    /// resident heap) knows it better.
    pub(crate) fn adopt_resident_heap(&mut self) -> bool {
        let Some(mut heap) = self.host.take_persistent_heap() else {
            return false;
        };
        heap.fields.append(&mut self.fields);
        heap.arrays.append(&mut self.arrays);
        heap.string_builders.append(&mut self.string_builders);
        heap.enum_constants.append(&mut self.enum_constants);
        self.fields = heap.fields;
        self.arrays = heap.arrays;
        self.string_builders = heap.string_builders;
        self.enum_constants = heap.enum_constants;
        self.next_object = self.next_object.max(heap.next_object);
        self.next_array = self.next_array.max(heap.next_array);
        true
    }

    /// Hand this invocation's objects back to the host for the next one.
    /// Call it once, after [`Self::persist_class_state`] (the statics are the
    /// collection's roots), and only after [`Self::adopt_resident_heap`]
    /// answered `true`.
    pub(crate) fn persist_resident_heap(&mut self) -> Result<(), NativeError> {
        self.persist_resident_heap_with(Vec::new())
    }

    /// [`Self::persist_resident_heap`] for an invocation the embedder made:
    /// its receiver, arguments and result are in the embedder's hands, so
    /// they are roots of this store's collection (#226 review). Later stores
    /// keep them only if the jar reaches them or the embedder retained them
    /// (`StdHost::retain`), so a call made every tick keeps nothing.
    pub(crate) fn persist_resident_heap_crossing(
        &mut self,
        crossing: &[Value],
    ) -> Result<(), NativeError> {
        let roots = crossing
            .iter()
            .flat_map(value_texts)
            .map(str::to_owned)
            .collect();
        self.persist_resident_heap_with(roots)
    }

    /// [`Self::persist_resident_heap`] with texts the caller keeps outside the
    /// host, which keep the objects they name alive through this store.
    pub(crate) fn persist_resident_heap_with(
        &mut self,
        roots: Vec<String>,
    ) -> Result<(), NativeError> {
        let heap = JavaHeap {
            fields: std::mem::take(&mut self.fields),
            arrays: std::mem::take(&mut self.arrays),
            string_builders: std::mem::take(&mut self.string_builders),
            enum_constants: std::mem::take(&mut self.enum_constants),
            next_object: self.next_object,
            next_array: self.next_array,
            live: 0,
            roots,
        };
        self.host.store_persistent_heap(heap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Value {
        Value::Ref(Some(value.to_owned()))
    }

    fn field(object: &str, name: &str) -> String {
        format!("{object}|demo/Node#{name}Ljava/lang/Object;")
    }

    /// A host whose static `ROOT` names `root`, and a heap with a chain
    /// `root → a → b`, an orphan `c → d` and an array only `b` names.
    fn graph(root: &str) -> (StdHost, JavaHeap) {
        let mut host = StdHost::new();
        host.persistent_static_fields
            .insert("demo/Mod#ROOTLjava/lang/Object;".into(), text(root));
        let mut heap = JavaHeap::default();
        heap.fields
            .insert(field("demo/Node#1", "next"), text("demo/Node#2"));
        heap.fields.insert(
            field("demo/Node#2", "next"),
            crate::host_array::handle("[Ldemo/Node;#0"),
        );
        heap.fields
            .insert(field("demo/Node#3", "next"), text("demo/Node#4"));
        heap.fields
            .insert(field("demo/Node#4", "next"), Value::Int(4));
        heap.arrays
            .insert("[Ldemo/Node;#0".into(), vec![Value::Int(1)].into());
        heap.arrays
            .insert("[Ldemo/Node;#1".into(), vec![Value::Int(2)].into());
        (host, heap)
    }

    fn objects(heap: &JavaHeap) -> Vec<&str> {
        let mut out: Vec<&str> = heap.fields.keys().map(|key| field_receiver(key)).collect();
        out.extend(heap.arrays.keys().map(String::as_str));
        out.dedup();
        out
    }

    #[test]
    fn collection_keeps_what_the_statics_reach_and_drops_the_rest() {
        let (mut host, mut heap) = graph("demo/Node#1");
        collect(&mut heap, &mut host, &[]);
        assert_eq!(
            objects(&heap),
            ["demo/Node#1", "demo/Node#2", "[Ldemo/Node;#0"]
        );
    }

    #[test]
    fn markers_embedded_in_a_lambda_or_an_entry_keep_their_objects() {
        // A static lambda capturing node 3, and node 1 only as a map entry's
        // value inside a list the host keeps for a static.
        let (mut host, mut heap) = graph("demo/Mod#unused");
        host.persistent_static_fields.insert(
            "demo/Mod#ROOTLjava/lang/Object;".into(),
            crate::callback_marker::lambda_from_text(
                "demo/Mod#init",
                4,
                "",
                "demo/Mod#lambda$0(Ldemo/Node;)V",
                "rdemo/Node#3",
            ),
        );
        host.persistent_static_fields.insert(
            "demo/Mod#MAPLjava/util/List;".into(),
            text("java/util/ArrayList#7"),
        );
        let entry = host.mint_map_entry(
            None,
            &text("key"),
            &text("demo/Node#1"),
            crate::host_map_entry::EntryMade::Holder,
        );
        host.mutable_collections
            .insert("java/util/ArrayList#7".into(), vec![entry]);
        host.mutable_collections
            .insert("java/util/ArrayList#8".into(), vec![text("demo/Node#3")]);
        collect(&mut heap, &mut host, &[]);
        assert_eq!(
            objects(&heap),
            [
                "demo/Node#1",
                "demo/Node#2",
                "demo/Node#3",
                "demo/Node#4",
                "[Ldemo/Node;#0"
            ]
        );
        // The unreachable list is dropped; the static's is kept.
        assert_eq!(
            host.mutable_collections.keys().collect::<Vec<_>>(),
            ["java/util/ArrayList#7"]
        );
    }

    #[test]
    fn an_object_held_by_an_optional_is_kept() {
        // A static `Optional` holding node 3: the optional is a host handle,
        // whose key names the node (#156); scanning only references dropped it.
        let (mut host, mut heap) = graph("demo/Node#1");
        host.persistent_static_fields.insert(
            "demo/Mod#HELDLjava/util/Optional;".into(),
            crate::optional_wrap(text("demo/Node#3")),
        );
        collect(&mut heap, &mut host, &[]);
        assert_eq!(
            objects(&heap),
            [
                "demo/Node#1",
                "demo/Node#2",
                "demo/Node#3",
                "demo/Node#4",
                "[Ldemo/Node;#0"
            ]
        );
    }

    #[test]
    fn a_marker_that_only_shares_a_prefix_is_not_a_reference() {
        // `demo/Node#12` is not `demo/Node#1`.
        let (mut host, mut heap) = graph("demo/Node#12");
        collect(&mut heap, &mut host, &[]);
        assert!(objects(&heap).is_empty(), "{:?}", objects(&heap));
    }

    #[test]
    fn entrypoint_and_game_object_fields_are_roots() {
        let (mut host, mut heap) = graph("nothing");
        // The entrypoint instance is named by its class, a game object by
        // the hook tier's prefix: neither is collectable, both keep what
        // their fields name.
        heap.fields
            .insert(field("demo/Mod", "first"), text("demo/Node#1"));
        heap.fields
            .insert(field("game/entity/7", "tag"), text("demo/Node#3"));
        collect(&mut heap, &mut host, &[]);
        assert_eq!(heap.fields.len(), 6);
        assert_eq!(heap.arrays.len(), 1);
    }

    #[test]
    fn a_store_collects_once_the_heap_outgrows_its_trigger_and_refuses_past_its_bound() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();
        for index in 0..COLLECT_SLACK {
            heap.fields
                .insert(field(&format!("demo/Node#{index}"), "x"), Value::Int(1));
        }
        // At the trigger, nothing is collected yet.
        host.store_resident_heap(heap.clone()).unwrap();
        assert_eq!(host.resident_heap().instance_fields(), COLLECT_SLACK);
        // One past it, the unreachable nodes go.
        heap.fields
            .insert(field("demo/Node#extra1", "x"), Value::Int(1));
        heap.fields
            .insert(field("demo/Node#9999", "x"), Value::Int(1));
        host.store_resident_heap(heap).unwrap();
        assert_eq!(
            host.resident_heap().fields.keys().collect::<Vec<_>>(),
            [&field("demo/Node#extra1", "x")]
        );

        // Reachable state past the bound is refused, and still kept.
        let mut big = JavaHeap::default();
        for index in 0..=MAX_RESIDENT_FIELDS {
            big.fields
                .insert(field("demo/Mod", &format!("f{index}")), Value::Int(1));
        }
        let error = host.store_resident_heap(big).unwrap_err().to_string();
        assert!(error.contains("over the resident bound"), "{error}");
        assert_eq!(
            host.resident_heap().instance_fields(),
            MAX_RESIDENT_FIELDS + 1
        );
    }

    fn logged(
        owner: &str,
        name: &str,
        descriptor: &str,
        args: Vec<Value>,
    ) -> crate::HostInvocation {
        crate::HostInvocation {
            method: crate::MethodRef {
                owner: owner.into(),
                name: name.into(),
                descriptor: descriptor.into(),
            },
            receiver: Some(text("java/util/ArrayList#9")),
            args,
        }
    }

    /// The call log roots only the calls it is read back for (#197): an
    /// object a jar passed to `List.add` (or whose constructor the host ran)
    /// is collected, a listener object registered on an event bus is kept,
    /// with what its fields reach.
    #[test]
    fn only_a_registration_in_the_call_log_keeps_its_objects() {
        let (mut host, mut heap) = graph("demo/Elsewhere");
        host.invocations.push(logged(
            "java/util/ArrayList",
            "add",
            "(Ljava/lang/Object;)Z",
            vec![text("demo/Node#1")],
        ));
        host.invocations
            .push(logged("java/lang/Object", "<init>", "()V", vec![]));
        host.invocations.push(logged(
            "net/neoforged/bus/api/IEventBus",
            "register",
            "(Ljava/lang/Object;)V",
            vec![text("demo/Node#3")],
        ));
        collect(&mut heap, &mut host, &[]);
        assert_eq!(objects(&heap), ["demo/Node#3", "demo/Node#4"]);
    }

    /// An unmodifiable view keeps its source (and the source its elements)
    /// while the view is reachable; once nothing reaches the view, the view,
    /// its source and the view's row in the per-object tables all go.
    #[test]
    fn a_view_keeps_its_source_only_while_it_is_reachable() {
        let view = "java/util/Collections$UnmodifiableRandomAccessList#immutable0";
        let (mut host, mut heap) = graph(view);
        host.unmodifiable_collections
            .insert(view.into(), Some("java/util/ArrayList#5".into()));
        host.mutable_collections
            .insert("java/util/ArrayList#5".into(), vec![text("demo/Node#3")]);
        collect(&mut heap, &mut host, &[]);
        assert_eq!(objects(&heap), ["demo/Node#3", "demo/Node#4"]);
        assert!(
            host.mutable_collections
                .contains_key("java/util/ArrayList#5")
        );

        host.persistent_static_fields.clear();
        collect(&mut heap, &mut host, &[]);
        assert_eq!(objects(&heap), Vec::<&str>::new());
        assert!(
            !host
                .mutable_collections
                .contains_key("java/util/ArrayList#5")
        );
        assert!(host.unmodifiable_collections.is_empty());
    }

    /// A throwable is kept while something reaches it, with its cause, and
    /// collected (message and all) once nothing does: it used to be a root
    /// for the host's life.
    #[test]
    fn a_throwable_and_its_cause_live_as_long_as_it_is_reached() {
        let (mut host, mut heap) = graph("java/lang/RuntimeException#20");
        host.throwable_messages
            .insert("java/lang/RuntimeException#20".into(), Some("outer".into()));
        host.throwable_causes.insert(
            "java/lang/RuntimeException#20".into(),
            Some(text("demo/Node#3")),
        );
        host.throwable_messages.insert(
            "java/lang/RuntimeException#21".into(),
            Some("dropped".into()),
        );
        collect(&mut heap, &mut host, &[]);
        assert_eq!(objects(&heap), ["demo/Node#3", "demo/Node#4"]);
        assert_eq!(
            host.throwable_messages.keys().collect::<Vec<_>>(),
            ["java/lang/RuntimeException#20"]
        );
    }

    #[test]
    fn a_rewound_call_log_drops_only_what_came_after_the_mark() {
        let mut host = StdHost::new();
        host.posted_events.push(Value::Int(1));
        let mark = host.call_log_mark();
        host.posted_events.push(Value::Int(2));
        host.rewind_call_log(mark);
        assert_eq!(host.posted_events, [Value::Int(1)]);
    }

    /// One live object between every two dead ones, whether or not the dead
    /// ones had rows: the freed set keeps one range per live object, not per
    /// allocation (#226 re-check 3), and a number freed once stays freed.
    #[test]
    fn the_freed_set_is_bounded_by_the_live_numbers() {
        let mut freed = FreedObjects::default();
        freed.rebuild(10, vec![3, 7]);
        assert_eq!(freed.ranges(), 3);
        assert!([0, 1, 2, 4, 5, 6, 8, 9].iter().all(|&n| freed.contains(n)));
        assert!(!freed.contains(3) && !freed.contains(7) && !freed.contains(10));
        // 100,000 more allocations, every tenth kept: ranges grow with the
        // live set, never with the allocations.
        let live: Vec<usize> = (10..100_010).step_by(10).collect();
        let kept = live.len();
        freed.rebuild(100_010, live);
        assert!(freed.ranges() <= kept + 2, "{}", freed.ranges());
        // All but 20 die; 4 was freed before and stays freed though a text
        // names it now (a handle a missed root kept stays loud).
        freed.rebuild(100_010, vec![4, 20]);
        assert!(freed.contains(4) && freed.contains(30) && !freed.contains(20));
        assert_eq!(freed.ranges(), 2);
    }

    /// Every `#<digits>` a text holds, embedded keys included.
    #[test]
    fn numbers_are_read_from_every_key_a_text_names() {
        let mut out = Vec::new();
        note_numbers("demo/Lock#5", &mut out);
        let callback = crate::callback_marker::test_token("demo/Main#lambda|demo/Cell#12|x#|#");
        note_numbers(
            crate::callback_marker::callback_text(&callback).unwrap(),
            &mut out,
        );
        note_numbers("HostRef { key: \"demo/A#7\" } #neutral3", &mut out);
        assert_eq!(out, [5, 12, 7]);
    }

    /// An object with no row of its own (`new Object()` kept as a lock) is
    /// live while a root names it, so its number is never freed; one nothing
    /// names is freed with the rowed garbage around it, and the host's data
    /// for it (a `UUID`'s bits) goes with it.
    #[test]
    fn an_object_without_rows_is_live_while_named() {
        let (mut host, mut heap) = graph("demo/Node#1");
        host.persistent_static_fields.insert(
            "demo/Mod#LOCKLjava/lang/Object;".into(),
            text("java/lang/Object#9"),
        );
        host.constructed_uuids
            .insert("java/util/UUID#10".into(), (1, 2));
        host.constructed_uuids
            .insert("java/util/UUID#11".into(), (3, 4));
        host.persistent_static_fields.insert(
            "demo/Mod#IDLjava/util/UUID;".into(),
            text("java/util/UUID#11"),
        );
        heap.next_object = 12;
        collect(&mut heap, &mut host, &[]);
        let freed = &host.freed_objects;
        assert!(!freed.holds_key("java/lang/Object#9"));
        assert!(!freed.holds_key("demo/Node#1") && !freed.holds_key("demo/Node#2"));
        assert!(freed.holds_key("demo/Node#3") && freed.holds_key("demo/Node#4"));
        assert!(freed.holds_key("java/util/UUID#10") && !freed.holds_key("java/util/UUID#11"));
        assert_eq!(
            host.constructed_uuids.keys().collect::<Vec<_>>(),
            ["java/util/UUID#11"]
        );
    }

    /// Every `StdHost` field is one the collector reads (a root, an edge, a
    /// per-object table it prunes) or one shown to hold no allocation key
    /// (names, bytes, counters, flags). A new field fails here until it is
    /// classified: #233's `cached_collections` slipped past the audit, so
    /// `System.getProperties()` lost its contents and every map's views
    /// stayed cached (#226 re-check 4).

    #[test]
    fn render_objects_follow_the_collector() {
        let (mut host, mut heap) = graph("demo/Render#1");
        let mut diag = StdHost::new();
        let stack = crate::render_math::new_value("", "com/mojang/blaze3d/vertex/PoseStack#10");
        host.register_render_handle("com/mojang/blaze3d/vertex/PoseStack#10");
        diag.register_render_handle("com/mojang/blaze3d/vertex/PoseStack#10");
        diag.register_render_handle("com/mojang/blaze3d/vertex/PoseStack#10/pose/0");
        crate::render_math::construct(
            &mut host.render_objects,
            &stack,
            "com/mojang/blaze3d/vertex/PoseStack",
            &[],
        );
        let last = crate::render_math::call(
            &mut diag,
            &mut host.render_objects,
            &crate::MethodRef {
                owner: "com/mojang/blaze3d/vertex/PoseStack".into(),
                name: "last".into(),
                descriptor: "()Lcom/mojang/blaze3d/vertex/PoseStack$Pose;".into(),
            },
            &stack,
            &[],
        )
        .unwrap()
        .unwrap();
        let matrix = crate::render_math::call(
            &mut diag,
            &mut host.render_objects,
            &crate::MethodRef {
                owner: "com/mojang/blaze3d/vertex/PoseStack$Pose".into(),
                name: "pose".into(),
                descriptor: "()Lorg/joml/Matrix4f;".into(),
            },
            &last,
            &[],
        )
        .unwrap()
        .unwrap();
        host.render_handles.extend(diag.render_handles);
        host.persistent_static_fields.insert(
            "demo/Render#STACKLcom/mojang/blaze3d/vertex/PoseStack;".into(),
            stack.clone(),
        );
        host.retain(&matrix);
        collect(&mut heap, &mut host, &[]);
        assert!(
            host.render_objects
                .contains_key("com/mojang/blaze3d/vertex/PoseStack#10")
        );
        assert!(
            host.render_objects
                .contains_key("com/mojang/blaze3d/vertex/PoseStack#10/pose/0/matrix4")
        );
        host.persistent_static_fields.clear();
        host.release(&matrix);
        collect(&mut heap, &mut host, &[]);
        assert!(host.render_objects.is_empty());
    }

    #[test]
    fn entity_registration_handles_follow_callback_collection_roots() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();
        let category = crate::entity_registration::category_constant("CREATURE").unwrap();
        let builder = crate::entity_registration::static_call(
            &mut host.entity_registration,
            &crate::MethodRef {
                owner: "net/minecraft/world/entity/EntityType$Builder".into(),
                name: "of".into(),
                descriptor: "()".into(),
            },
            &[Value::Ref(Some("factory".into())), category],
        )
        .unwrap()
        .unwrap();
        let entity = crate::entity_registration::instance_call(
            &mut host.entity_registration,
            &crate::MethodRef {
                owner: "net/minecraft/world/entity/EntityType$Builder".into(),
                name: "build".into(),
                descriptor: "()".into(),
            },
            &builder,
            &[Value::Ref(Some("minecraft:probe".into()))],
        )
        .unwrap()
        .unwrap();
        let accessor = crate::entity_registration::define_id(
            &mut host.entity_registration,
            "net/minecraft/world/entity/Entity",
        )
        .unwrap()
        .unwrap();
        let entity_key = crate::jar_object::instance_key(&entity).unwrap().to_owned();
        let accessor_key = crate::jar_object::instance_key(&accessor)
            .unwrap()
            .to_owned();
        heap.fields
            .insert(field(&entity_key, "state"), Value::Int(1));
        heap.fields
            .insert(field(&accessor_key, "state"), Value::Int(1));
        host.mutable_collections.insert(
            "java/util/ArrayList#neutral0".into(),
            vec![entity.clone(), accessor.clone()],
        );
        host.persistent_static_fields.insert(
            "demo/Mod#ROOTLjava/util/List;".into(),
            text("java/util/ArrayList#neutral0"),
        );
        collect(&mut heap, &mut host, &[]);
        assert_eq!(host.entity_registration.entry_count(), 1);
        host.persistent_static_fields.clear();
        collect(&mut heap, &mut host, &[]);
        assert_eq!(host.entity_registration.entry_count(), 0);
        assert!(heap.fields.is_empty());
        assert_eq!(crate::entity_registration::accessor_id(&accessor), Some(8));
    }

    #[test]
    fn every_host_field_is_classified_for_the_collector() {
        const COLLECTOR_READS: &[&str] = &[
            "array_views",
            "resource_streams",
            "calls",
            "invocations",
            "menu_states",
            "posted_events",
            "registrations",
            "content_builders",
            "item_property_builders",
            "block_property_builders",
            "block_copied_properties",
            "block_state_properties",
            "state_definition_builders",
            "block_states",
            "block_default_states",
            "unknown_block_state_definitions",
            "constructed_items",
            "constructed_blocks",
            "block_items",
            "item_required_features",
            "payload_registrations",
            "payload_bytes",
            "fabric_entrypoint_containers",
            "constructed_entrypoints",
            "packet_types",
            "identifier_objects",
            "deferred_registers",
            "saved_data",
            "neoforge_attachments",
            "neoforge_bus",
            "holders",
            "holder_rows",
            // Map entries: their keys join the collectable set, their key, value and map are
            // edges, and `retain` prunes them and their maps' rows (`host_map_entry.rs`).
            "map_entries",
            "mutable_collections",
            "tooltip_sinks",
            "repeated_collections",
            "set_from_map_views",
            "random_sources",
            "guava_tables",
            "recipes",
            "component_states",
            "unmodifiable_collections",
            "empty_collections",
            "shared_empties",
            "cached_collections",
            "ranged_attributes",
            "voxel_shapes",
            "jdk_list_of",
            "synchronized_views",
            "reversed_views",
            "hash_tables",
            "collection_comparators",
            "red_black_trees",
            "probe_tables",
            "view_copies",
            "view_kinds",
            "locales",
            "constructed_uuids",
            "levels",
            "references",
            "iterators",
            "iterator_last",
            "enumerations",
            "iterator_guards",
            "mod_stamps",
            "sub_lists",
            "sub_list_class_names",
            "detached_sub_lists",
            "list_iterators",
            "deque_layouts",
            "iterator_sources",
            "lazy_optionals",
            "capability_values",
            "item_handlers",
            "capability_requests",
            "throwable_messages",
            "throwable_causes",
            "throwable_suppressed",
            "persistent_static_fields",
            "io_paths",
            "heap",
            "retained",
            "retained_shape_probes",
            "freed_objects",
            "freed_menu_objects",
            "collections_paused",
            "threads",
            "game_event",
            "client",
            "render_objects",
            "render_handles",
            "brigadier",
            "imc",
            "entity_registration",
            "tool_material",
            "fabric_block_api",
            "fabric_resource_conditions",
            "creative_tab",
            "fabric_biome_modifications",
            "recipe_sync_serializers",
            "vec2",
        ];
        const HOLDS_NO_OBJECT: &[&str] = &[
            "next_unmodifiable_collection",
            "next_reversed_view",
            "next_ranged_attribute",
            "next_voxel_shape",
            "next_shape_extraction",
            "next_menu_object",
            "properties",
            "environment",
            "resources",
            "resource_bytes",
            "primary_data_layer",
            "primary_data_scanner",
            "data_pack_layers",
            "resource_generation",
            "pending_container_clamp",
            "resource_marks",
            "classes",
            "linked_classes",
            "game_block_classes",
            "service_providers",
            "current_time_millis",
            "nano_time_origin",
            "mods",
            "neoforge_mod_files",
            "run_loader",
            "own_loader_classes",
            "own_mod_id",
            "fabric_metadata",
            "fabric_aliases",
            "fabric_origins",
            "fabric_resolution_errors",
            "loaded_nested_hashes",
            "environment_type",
            "game_version",
            "development_environment",
            "random_bit_scope",
            "console_out",
            "io_files",
            "io_file_bytes",
            "io_dirs",
            "inferred_declarations",
            "declared_registrations",
            "next_property_builder",
            "outgoing_payloads",
            "incoming_payloads",
            "level_resources",
            "holder_binding",
            "charset_decoders",
            "persistent_initialized_classes",
            "dfu_codec_nodes",
            "dfu_codec_callback_roots",
            "dfu_codec_refusals",
            "unknown_host_calls",
            "nondeterministic",
            "strict_unknown_host_calls",
            "next_lazy_optional",
            "next_fabric_event",
            "next_iterator",
            "next_neutral_collection",
            "next_set_from_map",
            "log4j_levels",
            // Registry ids the jar built (`registry_contents.rs`): names, no objects.
            "jar_registries",
            "registry_lifecycles",
            "random_seed_uniquifier",
            "random_time_origin",
            "fired_creative_tab_registrations",
            "peak_identity_tokens",
        ];
        let debug = format!("{:#?}", StdHost::new());
        let fields: BTreeSet<&str> = debug
            .lines()
            .filter_map(|line| line.strip_prefix("    "))
            .filter(|line| !line.starts_with(' '))
            .filter_map(|line| line.split_once(':').map(|(name, _)| name))
            .collect();
        let classified: BTreeSet<&str> = COLLECTOR_READS
            .iter()
            .chain(HOLDS_NO_OBJECT)
            .copied()
            .collect();
        let unclassified: Vec<_> = fields.difference(&classified).collect();
        let stale: Vec<_> = classified.difference(&fields).collect();
        assert!(
            unclassified.is_empty(),
            "unclassified host fields: {unclassified:?}"
        );
        assert!(
            stale.is_empty(),
            "classified fields the host lacks: {stale:?}"
        );
    }

    #[test]
    fn unrooted_menu_states_are_pruned() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();
        for i in 0..256 {
            let key = format!("demo/Menu#{i}");
            host.menu_states.insert(
                key,
                crate::menu_models::Object::Menu(crate::menu_models::MenuState {
                    menu_type: Value::Ref(None),
                    container_id: i,
                    slots: format!("slots#{i}"),
                    last_slots: Vec::new(),
                    container_listeners: Vec::new(),
                    combiner: Some(crate::menu_models::CombinerState {
                        input: format!("input#{i}"),
                        result: format!("result#{i}"),
                        definition: crate::menu_models::Definition {
                            inputs: Vec::new(),
                            result: crate::menu_models::SlotDef {
                                index: 0,
                                x: 0,
                                y: 0,
                                may_place: Value::Ref(None),
                                empty: false,
                            },
                        },
                        access: Value::Ref(None),
                        player: Value::Ref(None),
                    }),
                    seq: i as u64,
                }),
            );
        }
        heap.next_object = 256;
        collect(&mut heap, &mut host, &[]);
        assert!(host.menu_states.is_empty());
    }

    #[test]
    fn menu_gc_does_not_mark_future_jar_objects_freed() {
        // Finding 1 regression: building a menu with host-synthesized `#menu<n>` keys,
        // running GC, then allocating and using a new jar object must never trip
        // `freed_objects` ("collected while still reachable") and must never reuse
        // pruned `#menu<n>` ids.
        let mut host = StdHost::new();
        let mut heap = JavaHeap {
            next_object: 10,
            ..Default::default()
        };

        let builder = crate::menu_models::static_call(
            &mut host.menu_states,
            &mut host.next_menu_object,
            &crate::MethodRef {
                owner: "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition".into(),
                name: "create".into(),
                descriptor:
                    "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;"
                        .into(),
            },
            &[],
        )
        .unwrap()
        .unwrap();
        let builder_key = crate::expect_ref(&builder).unwrap().clone();
        assert!(
            builder_key.contains("#menu"),
            "expected non-numeric #menu<n> key, got {builder_key:?}"
        );
        // Build an AbstractContainerMenu (mints NonNullList#menu<n> via next_menu_object).
        let menu_val = crate::jar_object::handle("demo/TestMenu#3".into());
        crate::menu_models::call(
            &mut host.menu_states,
            &mut host.next_menu_object,
            &crate::MethodRef {
                owner: crate::menu_models::ABSTRACT_CONTAINER_MENU.into(),
                name: "<init>".into(),
                descriptor: "(Lnet/minecraft/world/inventory/MenuType;I)V".into(),
            },
            &menu_val,
            &[Value::Ref(None), Value::Int(1)],
        )
        .unwrap()
        .unwrap();
        let before_next = host.next_menu_object;
        assert!(before_next >= 2);

        // Run GC: menu + builder are unrooted so both are collected, and limit stays at heap.next_object (10).
        collect(&mut heap, &mut host, &[]);
        assert!(!host.menu_states.contains_key(&builder_key));
        assert!(!host.menu_states.contains_key("demo/TestMenu#3"));

        // Allocating a brand-new jar object at heap.next_object (10, 11, 50) must NEVER be marked freed.
        for id in [10usize, 11, 50] {
            let fresh_key = format!("demo/AfterGc#{id}");
            assert!(
                !host.freed_objects.holds_key(&fresh_key),
                "{fresh_key} was falsely marked freed after menu GC"
            );
        }

        // Next menu allocation uses a strictly monotonic counter (never reuses pruned ids).
        let builder2 = crate::menu_models::static_call(
            &mut host.menu_states,
            &mut host.next_menu_object,
            &crate::MethodRef {
                owner: "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition".into(),
                name: "create".into(),
                descriptor:
                    "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;"
                        .into(),
            },
            &[],
        )
        .unwrap()
        .unwrap();
        let builder2_key = crate::expect_ref(&builder2).unwrap().clone();
        assert_ne!(builder2_key, builder_key);
        assert!(host.next_menu_object > before_next);
    }

    #[test]
    fn menu_object_handles_retain_across_gc_and_track_collection_separately() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap {
            next_object: 10,
            ..Default::default()
        };

        // Construct a menu and read its `slots` field (`NonNullList#menu<n>`).
        let menu_val = crate::jar_object::handle("demo/TestMenu#3".into());
        crate::menu_models::call(
            &mut host.menu_states,
            &mut host.next_menu_object,
            &crate::MethodRef {
                owner: crate::menu_models::ABSTRACT_CONTAINER_MENU.into(),
                name: "<init>".into(),
                descriptor: "(Lnet/minecraft/world/inventory/MenuType;I)V".into(),
            },
            &menu_val,
            &[Value::Ref(None), Value::Int(1)],
        )
        .unwrap()
        .unwrap();
        let menu_slots = crate::menu_models::field(
            &host.menu_states,
            &menu_val,
            &crate::MethodRef {
                owner: crate::menu_models::ABSTRACT_CONTAINER_MENU.into(),
                name: "slots".into(),
                descriptor: "Lnet/minecraft/core/NonNullList;".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            &menu_slots,
            Value::Host(handle) if handle.kind() == crate::host_ref::HostKind::MenuObject
        ));
        let slots_key = crate::jar_object::object_key(&menu_slots)
            .expect("menu.slots must have an object_key")
            .to_owned();
        assert_eq!(
            crate::jar_object::object_class(&menu_slots),
            Some("net/minecraft/core/NonNullList")
        );
        assert_eq!(crate::jar_object::parse_key(&slots_key), None);

        // Construct a second unrooted `#menu<n>` object.
        let builder = crate::menu_models::static_call(
            &mut host.menu_states,
            &mut host.next_menu_object,
            &crate::MethodRef {
                owner: "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition".into(),
                name: "create".into(),
                descriptor:
                    "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;"
                        .into(),
            },
            &[],
        )
        .unwrap()
        .unwrap();
        let builder_key = crate::jar_object::object_key(&builder)
            .expect("builder must have an object_key")
            .to_owned();

        // `StdHost::retain(&menu_slots)` must pin `menu_slots` (not drop it as a plain String).
        host.retain(&menu_slots);
        assert_eq!(host.retained_count(), 1);

        collect(&mut heap, &mut host, &[]);
        assert!(host.menu_states.contains_key(&slots_key));
        assert!(!host.object_collected(&slots_key));
        assert!(!host.menu_states.contains_key(&builder_key));
        assert!(host.object_collected(&builder_key));
        assert!(!host.freed_objects.holds_key("demo/AfterGc#10"));

        // Releasing `menu_slots` allows the next collection to prune and mark it collected.
        host.release(&menu_slots);
        assert_eq!(host.retained_count(), 0);
        collect(&mut heap, &mut host, &[]);
        assert!(!host.menu_states.contains_key(&slots_key));
        assert!(host.object_collected(&slots_key));
    }

    #[test]
    fn synced_recipe_serializers_root_their_objects_and_held_codecs() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();

        // A JarObject serializer reachable only from Fabric's static synced set.
        let jar_ser = crate::jar_object::handle("demo/Serializer#1".into());
        heap.fields
            .insert(field("demo/Serializer#1", "delegate"), text("demo/Node#2"));
        heap.fields
            .insert(field("demo/Node#2", "tag"), Value::Int(42));
        // An unreferenced node that must be swept.
        heap.fields
            .insert(field("demo/Node#3", "tag"), Value::Int(99));

        // A RecipeSerializer record handle whose held codecs are JarObjects
        // reachable only through the synced set.
        let map_codec = crate::jar_object::handle("demo/MapCodec#4".into());
        let stream_codec = crate::jar_object::handle("demo/StreamCodec#5".into());
        heap.fields
            .insert(field("demo/MapCodec#4", "name"), text("map"));
        heap.fields
            .insert(field("demo/StreamCodec#5", "name"), text("stream"));
        let record_ser = crate::recipe_serializer::make(&map_codec, &stream_codec);

        for serializer in [jar_ser, record_ser] {
            let key = crate::fabric_api_slice2::canonical_serializer_key(&serializer).unwrap();
            host.recipe_sync_serializers.insert(key, serializer);
        }

        collect(&mut heap, &mut host, &[]);
        assert_eq!(
            objects(&heap),
            [
                "demo/MapCodec#4",
                "demo/Node#2",
                "demo/Serializer#1",
                "demo/StreamCodec#5",
            ]
        );
    }

    #[test]
    fn canonical_block_states_live_exactly_as_long_as_their_block() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();

        // Block #0 is held directly in a static field: all of its canonical states stay alive.
        let live_block = "demo/LiveBlock#0";
        host.persistent_static_fields.insert(
            "demo/Mod#LIVE_BLOCKLnet/minecraft/world/level/block/Block;".into(),
            crate::jar_object::handle(live_block.into()),
        );
        let live_north = host.canonical_block_state(
            live_block,
            BTreeMap::from([("facing".into(), "north".into())]),
        );
        let live_east = host.canonical_block_state(
            live_block,
            BTreeMap::from([("facing".into(), "east".into())]),
        );

        // Block #1 is NOT referenced directly, only through one of its canonical states in a static.
        let state_only_block = "demo/StateOnlyBlock#1";
        let state_only_east = host.canonical_block_state(
            state_only_block,
            BTreeMap::from([("facing".into(), "east".into())]),
        );
        host.persistent_static_fields.insert(
            "demo/Mod#SAVED_STATELnet/minecraft/world/level/block/state/BlockState;".into(),
            state_only_east.clone(),
        );
        heap.fields
            .insert(field(state_only_block, "tag"), Value::Int(7));

        // Blocks #2..#201 are temporary and unreachable: they and their states must be dropped.
        for i in 2..202 {
            let dead_block = format!("demo/TempBlock#{i}");
            let builder = format!("block/properties/{i}");
            host.block_property_builders
                .insert(builder, BTreeMap::new());
            host.block_property_builders
                .insert(dead_block.clone(), BTreeMap::new());
            host.block_default_states.insert(
                dead_block.clone(),
                BTreeMap::from([("facing".into(), "north".into())]),
            );
            let _ = host.canonical_block_state(
                &dead_block,
                BTreeMap::from([("facing".into(), "north".into())]),
            );
            let _ = host.canonical_block_state(
                &dead_block,
                BTreeMap::from([("facing".into(), "south".into())]),
            );
        }
        heap.next_object = 202;
        assert_eq!(host.block_state_count(), 403);

        collect(&mut heap, &mut host, &[]);

        // All 400 dead states, their property builders, and default states are gone.
        assert_eq!(host.block_state_count(), 3);
        assert!(host.block_property_builders.is_empty());
        assert!(host.block_default_states.is_empty());
        // Live block's states remain canonical and stable.
        assert_eq!(
            host.canonical_block_state(
                live_block,
                BTreeMap::from([("facing".into(), "north".into())]),
            ),
            live_north
        );
        assert_eq!(
            host.canonical_block_state(
                live_block,
                BTreeMap::from([("facing".into(), "east".into())]),
            ),
            live_east
        );
        // State-only-in-static keeps its block's fields and state row alive.
        assert_eq!(
            host.block_state_block(&state_only_east),
            Some(crate::jar_object::stored(state_only_block))
        );
        assert_eq!(objects(&heap), [state_only_block]);
    }

    #[test]
    fn retained_shape_probes_flag_disarms_once_leaked_probes_are_collected() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();
        let probe = Value::Host(crate::host_ref::HostRef::new(
            crate::host_ref::HostKind::ShapeProbe,
            "1:block-getter",
        ));

        // Simulate a leaky extraction arming `retained_shape_probes` and storing the probe
        // in a static field, an instance field, an array, and a list.
        host.retained_shape_probes = true;
        host.persistent_static_fields.insert(
            "demo/Leak#SAVEDLnet/minecraft/world/level/BlockGetter;".into(),
            probe.clone(),
        );
        heap.fields
            .insert(field("demo/Holder#1", "saved"), probe.clone());
        heap.arrays.insert(
            "[Ljava/lang/Object;#2".into(),
            crate::array_data::ArrayData::Ref(vec![probe.clone()]),
        );
        host.mutable_collections
            .insert("java/util/ArrayList#neutral3".into(), vec![probe]);
        host.persistent_static_fields.insert(
            "demo/Leak#HOLDERLdemo/Holder;".into(),
            crate::jar_object::handle("demo/Holder#1".into()),
        );
        host.persistent_static_fields.insert(
            "demo/Leak#ARRLjava/lang/Object;".into(),
            Value::Host(crate::host_ref::HostRef::new(
                crate::host_ref::HostKind::Array,
                "[Ljava/lang/Object;#2",
            )),
        );
        host.persistent_static_fields.insert(
            "demo/Leak#LISTLjava/util/List;".into(),
            text("java/util/ArrayList#neutral3"),
        );
        heap.next_object = 4;

        // While any live reference holds the probe, GC keeps `retained_shape_probes` armed.
        collect(&mut heap, &mut host, &[]);
        assert!(host.has_retained_shape_probes());

        // Drop the static probe; instance field, array, and list still hold it.
        host.persistent_static_fields
            .remove("demo/Leak#SAVEDLnet/minecraft/world/level/BlockGetter;");
        collect(&mut heap, &mut host, &[]);
        assert!(host.has_retained_shape_probes());

        // Drop holder and array; list still holds it.
        host.persistent_static_fields
            .remove("demo/Leak#HOLDERLdemo/Holder;");
        host.persistent_static_fields
            .remove("demo/Leak#ARRLjava/lang/Object;");
        collect(&mut heap, &mut host, &[]);
        assert!(host.has_retained_shape_probes());

        // Drop the list too: GC sweeps the list and disarms `retained_shape_probes`.
        host.persistent_static_fields
            .remove("demo/Leak#LISTLjava/util/List;");
        collect(&mut heap, &mut host, &[]);
        assert!(!host.has_retained_shape_probes());
    }
}
