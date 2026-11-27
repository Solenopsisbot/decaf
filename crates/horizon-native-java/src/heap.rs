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
//! The recipe model's rows (stacks, templates, component patches: `recipe_models.rs`) are
//! host state an embedder may hold, so they are roots, except the ones made inside a transient
//! scope ([`StdHost::begin_transient_rows`]: a client draw, a server item hook), which are
//! objects like any other once the scope closes.
//!
//! Collection is linear in what it traces: each object's fields are a range of the sorted
//! field table, and the Kotlin lazy states a text keeps are found with one automaton pass over
//! the text ([`LazyHolders`]), not a walk of every field per text (#609 round 4 bug 1).
//!
//! The heap is bounded after collection ([`MAX_RESIDENT_FIELDS`],
//! [`MAX_RESIDENT_ARRAYS`], [`MAX_RESIDENT_BYTES`]); a store over a bound is
//! refused with an error that names it, so the callback that grew it fails
//! instead of the server running out of memory.

use std::collections::{BTreeMap, BTreeSet};

use crate::fx_hash::{FxHashMap, FxHashSet};
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

/// Bitmask flags persisted on [`JavaHeap`] so hot-path guards skip `BTreeMap`
/// lookups when a feature has never been constructed on this heap.
pub(crate) const HEAP_FLAG_GUAVA_MULTIMAP: u8 = 1 << 0;
pub(crate) const HEAP_FLAG_GUAVA_BIMAP: u8 = 1 << 1;
pub(crate) const HEAP_FLAG_GUAVA_TYPE_TOKEN: u8 = 1 << 2;
pub(crate) const HEAP_FLAG_UNCERTAIN_COLLECTION: u8 = 1 << 3;

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
    /// Collections this host's heap has had ([`StdHost::collection_count`]).
    collections: usize,
    /// Texts the storing caller holds outside the host (the game-hook tier's
    /// fields on game objects), roots of this store's collection only.
    roots: Vec<String>,
    /// Feature-presence flags (`HEAP_FLAG_*`) accumulated on this heap.
    pub(crate) heap_flags: u8,
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

/// An open transient scope of recipe-model rows ([`StdHost::begin_transient_rows`]); close it with
/// [`StdHost::end_transient_rows`].
#[must_use = "a transient scope seals its rows only when it is closed"]
#[derive(Debug)]
pub struct TransientRows {
    depth: usize,
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

    /// Open a transient scope: every stack, template, component patch and other recipe-model row
    /// made from now until the scope closes ([`Self::end_transient_rows`]), by the embedder's
    /// calls or by the jar's code, becomes an ordinary object of the heap collector when the
    /// outermost scope closes, kept only while something reaches it, rather than a root for the
    /// host's life.
    ///
    /// While a scope is open its rows are roots, as every row was before scopes: the embedder
    /// can hold any value it was handed between its calls inside the scope, and a collection
    /// in between (every call's store may collect) frees none of them. The embedder promises
    /// not to hold one after the scope closes, except through [`Self::retain`] (or the jar's own
    /// roots, which the collector sees). That holds for a call whose answer the embedder turns
    /// into its own data before it returns: a client draw (a model property's number, an item
    /// bar, a tooltip image's draws) and a server click, use or destroy, whose stacks become
    /// game stacks. Rows made outside any scope stay roots.
    ///
    /// Scopes nest; only the outermost close seals. A scope that is never closed (an embedder
    /// that unwound past it) leaves its rows roots: as before scopes, never a missed root.
    pub fn begin_transient_rows(&mut self) -> TransientRows {
        TransientRows {
            depth: self.recipes.enter_transient(),
        }
    }

    /// Close the scope `scope` opened ([`Self::begin_transient_rows`]).
    pub fn end_transient_rows(&mut self, scope: TransientRows) {
        self.recipes.leave_transient(scope.depth);
    }

    /// Run `body` in a transient scope ([`Self::begin_transient_rows`]), closed however `body`
    /// ends, a panic included.
    pub fn with_transient_rows<R>(&mut self, body: impl FnOnce(&mut Self) -> R) -> R {
        let scope = self.begin_transient_rows();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(self)));
        self.end_transient_rows(scope);
        match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    /// How many recipe-model rows (stacks, templates, patches, ...) the host keeps, and how
    /// many of them are collectable ([`Self::with_transient_rows`]).
    pub fn recipe_row_counts(&self) -> (usize, usize) {
        (
            self.recipes.entry_count(),
            self.recipes.collectable_rows.len(),
        )
    }

    pub(crate) fn take_resident_heap(&mut self) -> JavaHeap {
        let (live, collections) = (self.heap.live, self.heap.collections);
        let heap = std::mem::take(&mut self.heap);
        self.heap.live = live;
        self.heap.collections = collections;
        heap
    }

    /// How many collections this host's resident heap has had: what a test checks to know the
    /// collector ran, rather than how long something took.
    pub fn collection_count(&self) -> usize {
        self.heap.collections
    }

    /// Keep `heap` as this host's resident heap, collecting it first when it
    /// grew past the trigger or a bound. The heap is kept even when it is
    /// refused, so a jar's other callbacks still see their objects; the error
    /// fails the callback that grew it.
    pub(crate) fn store_resident_heap(&mut self, mut heap: JavaHeap) -> Result<(), NativeError> {
        heap.live = self.heap.live;
        heap.collections = self.heap.collections;
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
            heap.collections += 1;
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
            visit_value_texts(value, |t| out.push(t));
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
        // The `BlockPlaceContext` snapshots of the placements running now:
        // each is released when its invocation ends (#635), so this roots a
        // snapshot only while the jar can reach it through the call.
        for context in self.placement_contexts.values() {
            value(&context.clicked_face, out);
            match &context.placer {
                crate::Placer::Player { player, .. } => value(player, out),
                crate::Placer::None {
                    direction: Some(direction),
                } => value(direction, out),
                crate::Placer::None { direction: None } | crate::Placer::Unknown => {}
            }
            value(&context.item_in_hand, out);
            value(&context.level, out);
        }
        for (_, values) in self.placement_arrays.values() {
            values.iter().for_each(|item| value(item, out));
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
        // The data component values a builder holds (`component:<type>` settings, the
        // `ItemLore` of `Item.Properties.component(LORE, ..)`): a builder keeps what it was
        // given, as `Item.Properties`' component map does, until its item is declared.
        out.extend(
            self.item_property_builders
                .values()
                .flat_map(|builder| builder.iter())
                .filter(|(key, value)| key.starts_with("component:") && !value.is_empty())
                .map(|(_, value)| value.as_str()),
        );
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
            .flat_map(BTreeMap::values)
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
        // Active block-use state and open-menu effects.
        self.block_use.root_texts(out);
        // Game constants the models made as jar objects (`Vec2.ZERO`, `MobCategory.CREATURE`):
        // static finals, never unreachable, so their heap numbers are never freed (#669).
        self.vec2.root_texts(out);
        self.tool_material.root_texts(out);
        self.entity_registration.root_texts(out);
        // Seeded `Level.random` handles kept per dimension (`level_sound.rs`).
        for held in self.level_sound.level_random_handles.values() {
            value(held, out);
        }
        // Host subsystems whose values and object keys are walked directly rather
        // than `Debug`-formatted into temporary strings.
        self.neoforge_bus.root_texts(out);
        self.threads.root_texts(out);
        for req in &self.capability_requests {
            out.push(req.owner.as_str());
            out.push(req.capability.as_str());
            if let Some(pos) = &req.position {
                out.push(pos.as_str());
            }
            if let Some(side) = &req.side {
                out.push(side.as_str());
            }
        }
        self.client.root_texts(out);
        if let Some(firing) = &self.game_event {
            firing.root_texts(out);
        }
        for container in &self.fabric_entrypoint_containers {
            out.push(container.class.as_str());
        }
        self.content_builders.root_texts(out);
        for row in &self.holder_rows {
            out.push(row.register.as_str());
            if let Some((registry, _)) = &row.id {
                value(registry, out);
            }
        }
    }

    /// Root texts that live behind interior mutability (`ImcHandle`'s shared bus)
    /// and must be copied out as owned strings while tracing.
    fn opaque_root_texts(&self, out: &mut Vec<String>) {
        self.imc.root_texts_owned(out);
    }

    /// What the host keeps for the object `marker` in a table keyed by it: a
    /// view's source, a sublist's parent, an iterator's collection and
    /// guard, a comparator, a reference's referent, a throwable's cause, a
    /// collection's layout. Edges from `marker` when it is collectable,
    /// roots when not.
    fn keyed_root_texts<'a>(&'a self, marker: &str, out: &mut Vec<&'a str>) {
        fn push_val<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
            visit_value_texts(value, |t| out.push(t));
        }
        if let Some(s) = self.io_paths.get(marker) {
            out.push(s.as_str());
        }
        if let Some(s) = self.set_from_map_views.get(marker) {
            out.push(s.as_str());
        }
        if let Some(Some(s)) = self.unmodifiable_collections.get(marker) {
            out.push(s.as_str());
        }
        if let Some((s, _)) = self.reversed_views.get(marker) {
            out.push(s.as_str());
        }
        if let Some(v) = self.synchronized_views.get(marker) {
            push_val(v, out);
        }
        if let Some(s) = self.view_copies.get(marker) {
            out.push(s.as_str());
        }
        if let Some(link) = self.sub_lists.get(marker) {
            out.push(link.parent());
        }
        if let Some(s) = self.iterator_sources.get(marker) {
            out.push(s.as_str());
        }
        if let Some(v) = self.iterator_last.get(marker) {
            push_val(v, out);
        }
        if let Some(iter) = self.list_iterators.get(marker) {
            push_val(iter.source(), out);
        }
        if let Some(guard) = self.iterator_guards.get(marker) {
            guard.root_texts(out);
        }
        if let Some(v) = self.collection_comparators.get(marker) {
            push_val(v, out);
        }
        if let Some(v) = self.references.get(marker) {
            push_val(v, out);
        }
        if let Some(Some(v)) = self.throwable_causes.get(marker) {
            push_val(v, out);
        }
        if let Some(list) = self.throwable_suppressed.get(marker) {
            for v in list {
                push_val(v, out);
            }
        }
        if let Some(table) = self.hash_tables.get(marker) {
            table.root_texts(out);
        }
        if let Some(tree) = self.red_black_trees.get(marker) {
            tree.root_texts(out);
        }
        if let Some(table) = self.probe_tables.get(marker) {
            table.root_texts(out);
        }
        if let Some(v) = self.block_items.get(marker) {
            push_val(v, out);
        }
        if let Some(v) = self.item_lores.get(marker) {
            push_val(v, out);
        }
        if let Some(s) = self.state_definition_builders.get(marker) {
            out.push(s.as_str());
        }
        if let Some(s) = self.array_view_array(marker) {
            out.push(s);
        }
        if let Some(s) = self.array_view_of_iterator(marker) {
            out.push(s);
        }
        if let Some((key, _)) = self.render_objects.get_key_value(marker) {
            out.push(key.as_str());
        }
        if let Some(attr) = self.ranged_attributes.get(marker)
            && let Some(desc) = &attr.description_id
        {
            out.push(desc.as_str());
        }
        crate::menu_models::keyed_root_texts(&self.menu_states, marker, out);
        self.block_use.keyed_root_texts(marker, out);
        if let Some((key, lines)) = self.tooltip_sinks.get_key_value(marker) {
            out.push(key.as_str());
            for line in lines {
                push_val(line, out);
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
            .chain(self.item_lores.keys())
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
            .chain(self.block_use.keyed_table_keys())
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
        self.item_lores.retain(|key, _| keep(key));
        self.item_required_features.retain(|key, _| keep(key));
        self.mod_stamps.retain(|key, _| keep(key));
        self.deque_layouts.retain(|key, _| keep(key));
        self.empty_collections.retain(|key| keep(key));
        self.ranged_attributes.retain(|key, _| keep(key));
        self.voxel_shapes.retain(|key, _| keep(key));
        self.menu_states.retain(|key, _| keep(key));
        self.block_use.prune_keyed_tables(&keep);
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
            .or_else(|| serial.strip_prefix("ir"))
            .or_else(|| serial.strip_prefix("hit"))
            // A `RandomSource` handle (`random_source.rs`): `<class>#random<n>`.
            .or_else(|| serial.strip_prefix("random"))
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
        + host.block_use.entry_count()
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
/// Visit every text `value` names other objects through without allocating for
/// plain references, primitives, or bare handles (no row / held list).
pub(crate) fn visit_value_texts<'a>(value: &'a Value, mut visit: impl FnMut(&'a str)) {
    match value {
        Value::Ref(Some(text)) => visit(text),
        Value::Host(handle) => {
            visit(handle.key());
            if handle.row().is_none() && handle.held().is_empty() {
                return;
            }
            let mut seen: FxHashSet<*const u8> = FxHashSet::default();
            let mut pending: Vec<&'a Value> = Vec::new();
            if let Some(row) = handle.row() {
                seen.insert(std::ptr::from_ref(row).cast::<u8>());
                pending.extend(row.values());
            }
            let held = handle.held();
            if !held.is_empty() && seen.insert(held.as_ptr().cast::<u8>()) {
                pending.extend(held);
            }
            while let Some(current) = pending.pop() {
                if let Some(text) = value_text(current) {
                    visit(text);
                }
                let Value::Host(h) = current else {
                    continue;
                };
                if let Some(row) = h.row()
                    && seen.insert(std::ptr::from_ref(row).cast::<u8>())
                {
                    pending.extend(row.values());
                }
                let held = h.held();
                if !held.is_empty() && seen.insert(held.as_ptr().cast::<u8>()) {
                    pending.extend(held);
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn value_texts(value: &Value) -> Vec<&str> {
    let mut out = Vec::new();
    visit_value_texts(value, |t| out.push(t));
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

enum TailEntry {
    One(u32),
    Many(Vec<u32>),
}

impl TailEntry {
    fn push(&mut self, idx: u32) {
        match self {
            TailEntry::One(first) => {
                let a = *first;
                *self = TailEntry::Many(vec![a, idx]);
            }
            TailEntry::Many(list) => list.push(idx),
        }
    }
}

/// Parse an ASCII digit slice as a canonical `u64` (no leading zeros except `"0"` itself).
fn parse_canonical_u64(digits: &[u8]) -> Option<u64> {
    if digits.is_empty() || (digits.len() > 1 && digits[0] == b'0') {
        return None;
    }
    let mut value = 0u64;
    for &b in digits {
        value = value.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(value)
}

const SRC_FIELDS: u16 = 1 << 0;
const SRC_ARRAYS: u16 = 1 << 1;
const SRC_COLLECTIONS: u16 = 1 << 2;
const SRC_REPEATED: u16 = 1 << 3;
const SRC_ITERATORS: u16 = 1 << 4;
const SRC_ITER_LAST: u16 = 1 << 5;
const SRC_ENUMS: u16 = 1 << 6;
const SRC_COMPONENTS: u16 = 1 << 7;
const SRC_MAP_ENTRIES: u16 = 1 << 8;
const SRC_RECIPES: u16 = 1 << 9;
const SRC_KEYED: u16 = 1 << 10;

/// The collectable markers of a heap, borrowed for the trace and indexed by
/// their trailing maximal ASCII digit run so [`scan_text`] tests only the 1-2
/// markers sharing each digit run instead of every marker length.
///
/// Each marker also records which tables contain it (`sources` bitmask) and,
/// for `heap.fields`, its contiguous slice in `field_values`, eliminating
/// per-node `BTreeMap` lookups during the BFS trace.
struct Collectable<'a> {
    markers: Vec<&'a str>,
    sources: Vec<u16>,
    /// Contiguous slice `(start, end)` inside `field_values` for each marker.
    field_spans: Vec<(u32, u32)>,
    field_values: Vec<&'a Value>,
    /// Values from non-collectable receivers in `heap.fields` (roots), collected
    /// during the same linear pass so `heap.fields` is scanned only once.
    root_field_values: Vec<&'a Value>,
    index: FxHashMap<&'a str, u32>,
    by_num: FxHashMap<u64, TailEntry>,
    by_tail: FxHashMap<&'a str, TailEntry>,
    all_digits: Vec<u32>,
}

impl<'a> Collectable<'a> {
    #[inline]
    fn register(&mut self, marker: &'a str, src_bit: u16) -> u32 {
        use std::collections::hash_map::Entry;
        match self.index.entry(marker) {
            Entry::Occupied(occ) => {
                let idx = *occ.get();
                self.sources[idx as usize] |= src_bit;
                idx
            }
            Entry::Vacant(vac) => {
                let idx = self.markers.len() as u32;
                vac.insert(idx);
                self.markers.push(marker);
                self.sources.push(src_bit);
                self.field_spans.push((0, 0));
                let bytes = marker.as_bytes();
                let mut digit_start = bytes.len();
                while digit_start > 0 && bytes[digit_start - 1].is_ascii_digit() {
                    digit_start -= 1;
                }
                if digit_start == 0 {
                    self.all_digits.push(idx);
                } else if digit_start < bytes.len() {
                    let tail_bytes = &bytes[digit_start..];
                    if let Some(num) = parse_canonical_u64(tail_bytes) {
                        self.by_num
                            .entry(num)
                            .and_modify(|e| e.push(idx))
                            .or_insert(TailEntry::One(idx));
                    } else {
                        let tail = &marker[digit_start..];
                        self.by_tail
                            .entry(tail)
                            .and_modify(|e| e.push(idx))
                            .or_insert(TailEntry::One(idx));
                    }
                }
                idx
            }
        }
    }

    fn new(heap: &'a JavaHeap, host: &'a StdHost) -> Self {
        let est = (heap.fields.len() / 2)
            .saturating_add(heap.arrays.len())
            .saturating_add(host.mutable_collections.len())
            .saturating_add(host.recipes.collectable_rows.len())
            .saturating_add(256);
        let mut out = Self {
            markers: Vec::with_capacity(est),
            sources: Vec::with_capacity(est),
            field_spans: Vec::with_capacity(est),
            field_values: Vec::with_capacity(heap.fields.len()),
            root_field_values: Vec::new(),
            index: FxHashMap::with_capacity_and_hasher(est, Default::default()),
            by_num: FxHashMap::with_capacity_and_hasher(est, Default::default()),
            by_tail: FxHashMap::default(),
            all_digits: Vec::new(),
        };

        // Single linear pass over `heap.fields`. Because `field_receiver(key)`
        // splits at the first `|`, all entries for a given receiver form one
        // contiguous run in `BTreeMap` order.
        let mut cur_recv = "";
        let mut cur_idx: Option<u32> = None;
        let mut cur_start: u32 = 0;
        for (key, value) in &heap.fields {
            let recv = field_receiver(key);
            if recv != cur_recv {
                if let Some(idx) = cur_idx {
                    out.field_spans[idx as usize] = (cur_start, out.field_values.len() as u32);
                }
                cur_recv = recv;
                if collectable(recv) {
                    let idx = out.register(recv, SRC_FIELDS);
                    cur_idx = Some(idx);
                    cur_start = out.field_values.len() as u32;
                } else {
                    cur_idx = None;
                }
            }
            if cur_idx.is_some() {
                out.field_values.push(value);
            } else {
                out.root_field_values.push(value);
            }
        }
        if let Some(idx) = cur_idx {
            out.field_spans[idx as usize] = (cur_start, out.field_values.len() as u32);
        }

        for marker in heap.arrays.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_ARRAYS);
            }
        }
        for marker in heap.string_builders.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, 0);
            }
        }
        for marker in heap.enum_constants.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, 0);
            }
        }
        for marker in host.mutable_collections.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_COLLECTIONS);
            }
        }
        for marker in host.repeated_collections.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_REPEATED);
            }
        }
        for marker in host.set_from_map_views.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_KEYED);
            }
        }
        for marker in host.random_sources.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, 0);
            }
        }
        for marker in host.iterators.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_ITERATORS);
            }
        }
        for marker in host.iterator_last.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_ITER_LAST);
            }
        }
        for marker in host.enumerations.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_ENUMS);
            }
        }
        for marker in host.component_states.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_COMPONENTS);
            }
        }
        for marker in host.map_entries.keys().map(String::as_str) {
            if collectable(marker) {
                out.register(marker, SRC_MAP_ENTRIES);
            }
        }
        for (block, _) in host.block_states.values() {
            if collectable(block.as_str()) {
                out.register(block.as_str(), 0);
            }
        }
        for marker in host.keyed_table_keys() {
            if collectable(marker) {
                out.register(marker, SRC_KEYED);
            }
        }
        // The recipe rows a transient scope minted (`recipe_models::RecipeState::
        // collectable_rows`): `recipe-stack/<n>` and the like, which end in their serial
        // as every marker here does, so `scan_text` finds them where a text names them.
        for marker in host.recipes.collectable_rows.iter().map(String::as_str) {
            out.register(marker, SRC_RECIPES);
        }
        out
    }

    #[inline]
    fn contains(&self, marker: &str) -> bool {
        self.index.contains_key(marker)
    }

    #[inline]
    fn check_entry(
        &self,
        prefix: &[u8],
        entry: &TailEntry,
        reached: &mut [bool],
        pending: &mut Vec<u32>,
    ) {
        match entry {
            TailEntry::One(idx) => {
                let i = *idx as usize;
                if !reached[i] && prefix.ends_with(self.markers[i].as_bytes()) {
                    reached[i] = true;
                    pending.push(*idx);
                }
            }
            TailEntry::Many(list) => {
                for &idx in list {
                    let i = idx as usize;
                    if !reached[i] && prefix.ends_with(self.markers[i].as_bytes()) {
                        reached[i] = true;
                        pending.push(idx);
                    }
                }
            }
        }
    }

    /// Mark every collectable marker `text` names (itself or embedded) and push
    /// newly reached indices onto `pending`.
    fn scan(&self, text: &str, reached: &mut [bool], pending: &mut Vec<u32>) {
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if !bytes[i].is_ascii_digit() {
                i += 1;
                continue;
            }
            let start = i;
            let mut canonical = bytes[start] != b'0';
            let mut num = u64::from(bytes[start] - b'0');
            i += 1;
            while i < len && bytes[i].is_ascii_digit() {
                if canonical {
                    match num
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(u64::from(bytes[i] - b'0')))
                    {
                        Some(next) => num = next,
                        None => canonical = false,
                    }
                }
                i += 1;
            }
            let end = i;
            let prefix = &bytes[..end];
            if canonical || end - start == 1 {
                if let Some(entry) = self.by_num.get(&num) {
                    self.check_entry(prefix, entry, reached, pending);
                }
            } else if let Some(entry) = self.by_tail.get(&text[start..end]) {
                self.check_entry(prefix, entry, reached, pending);
            }
            if !self.all_digits.is_empty() {
                for &idx in &self.all_digits {
                    let j = idx as usize;
                    if !reached[j] && prefix.ends_with(self.markers[j].as_bytes()) {
                        reached[j] = true;
                        pending.push(idx);
                    }
                }
            }
        }
    }

    /// Single-pass trace of `text`: marks collectable markers, notes `#<digits>`
    /// allocation numbers into `live`, and notes `#menu<digits>` numbers into `live_menu`.
    #[inline]
    fn scan_text(
        &self,
        text: &str,
        reached: &mut [bool],
        pending: &mut Vec<u32>,
        live: &mut Vec<usize>,
        live_menu: &mut Vec<usize>,
    ) {
        let bytes = text.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if !bytes[i].is_ascii_digit() {
                i += 1;
                continue;
            }
            let start = i;
            let mut canonical = bytes[start] != b'0';
            let mut num = u64::from(bytes[start] - b'0');
            i += 1;
            while i < len && bytes[i].is_ascii_digit() {
                if canonical {
                    match num
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(u64::from(bytes[i] - b'0')))
                    {
                        Some(next) => num = next,
                        None => canonical = false,
                    }
                }
                i += 1;
            }
            let end = i;
            let prefix = &bytes[..end];
            let is_u64 = canonical || end - start == 1;
            if is_u64 {
                if let Some(entry) = self.by_num.get(&num) {
                    self.check_entry(prefix, entry, reached, pending);
                }
            } else if let Some(entry) = self.by_tail.get(&text[start..end]) {
                self.check_entry(prefix, entry, reached, pending);
            }
            if !self.all_digits.is_empty() {
                for &idx in &self.all_digits {
                    let j = idx as usize;
                    if !reached[j] && prefix.ends_with(self.markers[j].as_bytes()) {
                        reached[j] = true;
                        pending.push(idx);
                    }
                }
            }
            if start > 0 && bytes[start - 1] == b'#' {
                if is_u64 {
                    if let Ok(n) = usize::try_from(num) {
                        live.push(n);
                    }
                } else if let Ok(n) = text[start..end].parse::<usize>() {
                    live.push(n);
                }
            } else if start >= 5 && &bytes[start - 5..start] == b"#menu" {
                if is_u64 {
                    if let Ok(n) = usize::try_from(num) {
                        live_menu.push(n);
                    }
                } else if let Ok(n) = text[start..end].parse::<usize>() {
                    live_menu.push(n);
                }
            }
        }
    }
}

/// Every Kotlin lazy state of a heap (`kotlin-lazy|<holder>` fields), and an automaton that
/// finds, in one pass over a text, every holder the text contains (Aho-Corasick over the
/// holders' bytes).
///
/// A lazy state lives while some traced text contains its holder (`text.contains(holder)`,
/// the holder being a `KotlinLazy` handle's key, which a value embeds wherever it sits, #301).
/// Testing each holder against each text walked every heap field once per traced text, which
/// made a collection O(texts x fields): with a pouch drawn every frame the heap grew until one
/// collection took minutes (#609 round 4 bug 1). This answers the same `contains` for all
/// holders at once in O(text + matches).
struct LazyHolders<'h> {
    /// Holder and value of each lazy state, in field order.
    holders: Vec<(&'h str, &'h Value)>,
    /// Trie edges per node, sorted by byte; node 0 is the root.
    next: Vec<Vec<(u8, u32)>>,
    /// The longest proper suffix of a node's path that is also a trie path.
    fail: Vec<u32>,
    /// The holders that end exactly at a node.
    own: Vec<Vec<u32>>,
    /// The nearest node on the fail chain (the node itself excluded) with holders of its own.
    dict: Vec<u32>,
    /// Holders of length 0, which every text contains.
    empty: Vec<u32>,
}

const NO_NODE: u32 = u32::MAX;

impl<'h> LazyHolders<'h> {
    fn new(heap: &'h JavaHeap) -> Self {
        let holders: Vec<(&str, &Value)> = heap
            .fields
            .iter()
            .filter_map(|(key, value)| kotlin_lazy_field_handle(key).map(|holder| (holder, value)))
            .collect();
        let mut out = Self {
            holders,
            next: vec![Vec::new()],
            fail: vec![0],
            own: vec![Vec::new()],
            dict: vec![NO_NODE],
            empty: Vec::new(),
        };
        for index in 0..out.holders.len() {
            let holder = out.holders[index].0;
            let index = index as u32;
            if holder.is_empty() {
                out.empty.push(index);
                continue;
            }
            let mut node = 0u32;
            for &byte in holder.as_bytes() {
                node = match out.child(node, byte) {
                    Some(child) => child,
                    None => {
                        let child = out.next.len() as u32;
                        out.next.push(Vec::new());
                        out.fail.push(0);
                        out.own.push(Vec::new());
                        out.dict.push(NO_NODE);
                        let edges = &mut out.next[node as usize];
                        let at = edges.partition_point(|&(b, _)| b < byte);
                        edges.insert(at, (byte, child));
                        child
                    }
                };
            }
            out.own[node as usize].push(index);
        }
        // Fail and dictionary links, breadth first (a node's fail target is shallower).
        let mut queue = std::collections::VecDeque::new();
        for &(_, child) in &out.next[0] {
            queue.push_back(child);
        }
        while let Some(node) = queue.pop_front() {
            for at in 0..out.next[node as usize].len() {
                let (byte, child) = out.next[node as usize][at];
                let mut fallback = out.fail[node as usize];
                let target = loop {
                    if let Some(target) = out.child(fallback, byte) {
                        break target;
                    }
                    if fallback == 0 {
                        break 0;
                    }
                    fallback = out.fail[fallback as usize];
                };
                out.fail[child as usize] = target;
                out.dict[child as usize] = if out.own[target as usize].is_empty() {
                    out.dict[target as usize]
                } else {
                    target
                };
                queue.push_back(child);
            }
        }
        out
    }

    fn is_empty(&self) -> bool {
        self.holders.is_empty()
    }

    fn child(&self, node: u32, byte: u8) -> Option<u32> {
        let edges = &self.next[node as usize];
        edges
            .binary_search_by_key(&byte, |&(b, _)| b)
            .ok()
            .map(|at| edges[at].1)
    }

    /// Call `each` with the index of every holder `text` contains (a holder found at several
    /// places is reported at each).
    fn matches(&self, text: &str, mut each: impl FnMut(usize)) {
        for &index in &self.empty {
            each(index as usize);
        }
        if self.next[0].is_empty() {
            return;
        }
        let mut node = 0u32;
        for &byte in text.as_bytes() {
            loop {
                if let Some(child) = self.child(node, byte) {
                    node = child;
                    break;
                }
                if node == 0 {
                    break;
                }
                node = self.fail[node as usize];
            }
            let mut out = node;
            while out != NO_NODE {
                for &index in &self.own[out as usize] {
                    each(index as usize);
                }
                out = self.dict[out as usize];
            }
        }
    }
}

/// Mark every lazy state whose holder `text` contains, then those its value's text contains,
/// and so on; the collectable markers their values name go to `pending`. The same closure the
/// field-by-field walk reached (the order differs, which nothing reads).
fn mark_kotlin_lazy_states<'t>(
    lazy: &LazyHolders<'t>,
    text: &'t str,
    reached_lazy: &mut FxHashSet<String>,
    reached_markers: &mut [bool],
    pending: &mut Vec<u32>,
    collectable: &Collectable<'_>,
) {
    if lazy.is_empty() {
        return;
    }
    let mut texts = vec![text];
    while let Some(text) = texts.pop() {
        lazy.matches(text, |index| {
            let (holder, value) = lazy.holders[index];
            if reached_lazy.contains(holder) {
                return;
            }
            reached_lazy.insert(holder.to_owned());
            if let Some(value) = value_text(value) {
                collectable.scan(value, reached_markers, pending);
                texts.push(value);
            }
        });
    }
}

fn text_has_shape_probe(text: &str) -> bool {
    crate::voxel_shape::parse_shape_probe_key(text).is_some()
        || text.contains("hsprobe:")
        || text.contains("ShapeProbe")
}

fn value_has_shape_probe(value: &Value) -> bool {
    let mut seen: FxHashSet<*const u8> = FxHashSet::default();
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
        // Collectable recipe rows are not roots (`root_texts` skips them).
        || host
            .recipes
            .collectable_rows
            .iter()
            .flat_map(|key| host.recipes.row_values(key))
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
        host.keyed_root_texts(key, &mut keyed);
        if keyed.iter().any(|text| text_has_shape_probe(text)) {
            return true;
        }
    }
    false
}

/// Compact owned set of dead markers backed by one contiguous `String` arena
/// indexed by trailing canonical `u64` digit run, eliminating per-dead-marker
/// heap allocations and full-string hashing during sweep.
struct DeadSet {
    arena: String,
    spans: Vec<(u32, u32)>,
    by_num: FxHashMap<u64, TailEntry>,
    other: FxHashSet<String>,
    count: usize,
}

impl DeadSet {
    fn with_capacity(est: usize) -> Self {
        Self {
            arena: String::with_capacity(est.saturating_mul(36)),
            spans: Vec::with_capacity(est),
            by_num: FxHashMap::with_capacity_and_hasher(est, Default::default()),
            other: FxHashSet::default(),
            count: 0,
        }
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline]
    fn trailing_u64(bytes: &[u8]) -> Option<u64> {
        let mut start = bytes.len();
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if start == bytes.len() {
            return None;
        }
        parse_canonical_u64(&bytes[start..])
    }

    #[inline]
    fn insert(&mut self, marker: &str) {
        self.count += 1;
        let bytes = marker.as_bytes();
        if let Some(num) = Self::trailing_u64(bytes) {
            let start = self.arena.len() as u32;
            self.arena.push_str(marker);
            let end = self.arena.len() as u32;
            let idx = self.spans.len() as u32;
            self.spans.push((start, end));
            self.by_num
                .entry(num)
                .and_modify(|e| e.push(idx))
                .or_insert(TailEntry::One(idx));
        } else {
            self.other.insert(marker.to_owned());
        }
    }

    #[inline]
    fn contains(&self, marker: &str) -> bool {
        let bytes = marker.as_bytes();
        if let Some(num) = Self::trailing_u64(bytes) {
            let Some(entry) = self.by_num.get(&num) else {
                return false;
            };
            let arena = self.arena.as_bytes();
            return match entry {
                TailEntry::One(idx) => {
                    let (s, e) = self.spans[*idx as usize];
                    &arena[s as usize..e as usize] == bytes
                }
                TailEntry::Many(list) => list.iter().any(|&idx| {
                    let (s, e) = self.spans[idx as usize];
                    &arena[s as usize..e as usize] == bytes
                }),
            };
        }
        !self.other.is_empty() && self.other.contains(marker)
    }
}

/// Drop every collectable object of `heap` (and every collectable collection
/// or iterator of `host`) that nothing reaches from the roots.
///
/// Roots: the static fields and host tables ([`StdHost::root_texts`]), the
/// caller's `extra` texts, and the contents of every object that is not
/// collectable. Edges: an object's instance fields, array elements,
/// collection elements and iterator snapshot.
pub(crate) fn collect(heap: &mut JavaHeap, host: &mut StdHost, extra: &[String]) {
    let render_live = if host.render_objects.is_empty() {
        BTreeSet::new()
    } else {
        let mut render_roots = Vec::new();
        host.root_texts(&mut render_roots);
        render_roots.extend(extra.iter().map(String::as_str));
        crate::render_math::reachable_keys(&host.render_objects, render_roots)
    };
    let (dead, reached_kotlin_lazy, limit, live, live_menu) = {
        let collectable = Collectable::new(heap, host);
        if collectable.markers.is_empty() {
            drop(collectable);
            host.render_objects
                .retain(|key, _| render_live.contains(key));
            host.render_handles.retain(|key| render_live.contains(key));
            if host.retained_shape_probes {
                host.retained_shape_probes = has_live_shape_probe(heap, host, extra);
            }
            return;
        }
        let mut reached = vec![false; collectable.markers.len()];
        let mut pending: Vec<u32> = Vec::with_capacity(collectable.markers.len() / 2);
        let mut reached_kotlin_lazy: FxHashSet<String> = FxHashSet::default();
        // Built once per collection; borrows the heap only while the trace reads it.
        let lazy = LazyHolders::new(heap);
        let has_lazy = !lazy.is_empty();
        // Every allocation number a text the trace reaches names: the objects
        // found live, those without a row of their own included (`FreedObjects`).
        let mut live = Vec::with_capacity(collectable.markers.len());
        let mut live_menu = Vec::new();
        {
            let mut roots: Vec<&str> = extra.iter().map(String::as_str).collect();
            host.root_texts(&mut roots);
            // Opaque state, and what the per-object tables keep for an object
            // that is never collected, are roots too.
            let mut owned = Vec::new();
            host.opaque_root_texts(&mut owned);
            for key in host.keyed_table_keys() {
                if !collectable.contains(key) {
                    host.keyed_root_texts(key, &mut roots);
                }
            }
            // A collection cached for something that is never collected
            // (`System.getProperties()`, a mod container's root paths) is a root.
            for ((identity, _), cached) in &host.cached_collections {
                if !cache_owner(identity).is_some_and(|owner| collectable.contains(owner.as_str()))
                {
                    roots.push(cached.as_str());
                }
            }
            roots.extend(owned.iter().map(String::as_str));
            for value in &collectable.root_field_values {
                visit_value_texts(value, |t| roots.push(t));
            }
            for (key, elements) in host.mutable_collections.iter() {
                if !collectable.contains(key.as_str()) {
                    for value in elements {
                        visit_value_texts(value, |t| roots.push(t));
                    }
                }
            }
            for (key, entry) in &host.iterators {
                if !collectable.contains(key.as_str()) {
                    for value in entry.values.iter().chain(entry.origin.iter()) {
                        visit_value_texts(value, |t| roots.push(t));
                    }
                }
            }
            for text in roots {
                if has_lazy {
                    mark_kotlin_lazy_states(
                        &lazy,
                        text,
                        &mut reached_kotlin_lazy,
                        &mut reached,
                        &mut pending,
                        &collectable,
                    );
                }
                collectable.scan_text(text, &mut reached, &mut pending, &mut live, &mut live_menu);
            }
        }
        // The collections cached for an object (a map's `keySet()`/`values()`/
        // `entrySet()`, #233): edges from it, so each lives as long as its map.
        let mut cached_for: FxHashMap<u32, Vec<&str>> = FxHashMap::default();
        for ((identity, _), cached) in &host.cached_collections {
            if let Some(owner) = cache_owner(identity)
                && let Some(&idx) = collectable.index.get(owner.as_str())
            {
                cached_for.entry(idx).or_default().push(cached.as_str());
            }
        }
        let has_cached = !cached_for.is_empty();
        let mut keyed_buf: Vec<&str> = Vec::new();
        while let Some(idx) = pending.pop() {
            let i = idx as usize;
            let marker = collectable.markers[i];
            let src = collectable.sources[i];

            let mut trace_value = |value: &Value| {
                visit_value_texts(value, |text| {
                    if has_lazy {
                        mark_kotlin_lazy_states(
                            &lazy,
                            text,
                            &mut reached_kotlin_lazy,
                            &mut reached,
                            &mut pending,
                            &collectable,
                        );
                    }
                    collectable.scan_text(
                        text,
                        &mut reached,
                        &mut pending,
                        &mut live,
                        &mut live_menu,
                    );
                });
            };

            if src & SRC_FIELDS != 0 {
                let (start, end) = collectable.field_spans[i];
                for value in &collectable.field_values[start as usize..end as usize] {
                    trace_value(value);
                }
            }
            if src & SRC_ARRAYS != 0
                && let Some(array) = heap.arrays.get(marker)
            {
                for value in array.references() {
                    trace_value(value);
                }
            }
            if src & SRC_COLLECTIONS != 0
                && let Some(elements) = host.mutable_collections.get(marker)
            {
                for value in elements {
                    trace_value(value);
                }
            }
            if src & SRC_REPEATED != 0
                && let Some((_, value)) = host.repeated_collections.get(marker)
            {
                trace_value(value);
            }
            if src & SRC_ITERATORS != 0
                && let Some(entry) = host.iterators.get(marker)
            {
                for value in &entry.values {
                    trace_value(value);
                }
                if let Some(origin) = &entry.origin {
                    trace_value(origin);
                }
            }
            if src & SRC_ITER_LAST != 0
                && let Some(value) = host.iterator_last.get(marker)
            {
                trace_value(value);
            }
            if src & SRC_ENUMS != 0
                && let Some(enumeration) = host.enumerations.get(marker)
            {
                trace_value(enumeration.held());
            }
            if src & SRC_COMPONENTS != 0
                && let Some(state) = host.component_states.get(marker)
            {
                for value in &state.siblings {
                    trace_value(value);
                }
                if let Some(args) = &state.args {
                    for value in args {
                        trace_value(value);
                    }
                }
            }
            if src & SRC_MAP_ENTRIES != 0 {
                for value in host.map_entries.held(marker) {
                    trace_value(value);
                }
            }
            if src & SRC_RECIPES != 0 {
                host.recipes.visit_row_values(marker, &mut trace_value);
            }
            if src & SRC_MAP_ENTRIES != 0
                && let Some(map) = host.map_entries.map_of(marker)
            {
                collectable.scan(map, &mut reached, &mut pending);
            }
            if has_cached && let Some(cached_list) = cached_for.get(&idx) {
                for cached in cached_list {
                    collectable.scan_text(
                        cached,
                        &mut reached,
                        &mut pending,
                        &mut live,
                        &mut live_menu,
                    );
                }
            }
            // What the host keeps for this object in a table keyed by it.
            if src & SRC_KEYED != 0 {
                keyed_buf.clear();
                host.keyed_root_texts(marker, &mut keyed_buf);
                for &text in &keyed_buf {
                    if has_lazy {
                        mark_kotlin_lazy_states(
                            &lazy,
                            text,
                            &mut reached_kotlin_lazy,
                            &mut reached,
                            &mut pending,
                            &collectable,
                        );
                    }
                    collectable.scan_text(
                        text,
                        &mut reached,
                        &mut pending,
                        &mut live,
                        &mut live_menu,
                    );
                }
            }
        }
        drop(lazy);
        // Every number below the heap's next one that was not found live is
        // freed, so a later read of its fields is refused by name rather than
        // answered with defaults. A number with no row that nothing reached is
        // freed too: that keeps the ranges as few as the live numbers between
        // them (#226 re-check 3), and an object with no row has nothing a stale
        // handle could misread.
        let mut limit = heap.next_object;
        let reached_count = reached.iter().filter(|&&r| r).count();
        let dead_est = collectable.markers.len().saturating_sub(reached_count);
        let mut dead = DeadSet::with_capacity(dead_est);
        for (i, &marker) in collectable.markers.iter().enumerate() {
            let is_reached = reached[i];
            if !is_reached {
                dead.insert(marker);
            }
            if let Some((_, number)) = crate::jar_object::parse_key(marker)
                && let Ok(number) = number.parse::<usize>()
            {
                if is_reached {
                    live.push(number);
                }
                limit = limit.max(number.saturating_add(1));
            } else if is_reached
                && let Some((_, number)) = crate::jar_object::menu_object_key(marker)
                && let Ok(number) = number.parse::<usize>()
            {
                live_menu.push(number);
            }
        }
        (dead, reached_kotlin_lazy, limit, live, live_menu)
    };
    let keep = |marker: &str| !dead.contains(marker);
    host.freed_objects.rebuild(limit, live);
    // Every number below `limit` is now freed or live, so the heap's next object starts at
    // `limit` at the earliest: a number this collection freed is never handed out again,
    // whatever named it. A host key that spells an allocation key (`<text>#<n>`) with a number
    // of its own counter raises `limit` past the heap's next object; without this, each object
    // the heap then allocated read as collected on its first unwritten field (#609, Runes
    // round 2 bug 1). Numbers are no answer Java sees (`identityHashCode` is counted).
    heap.next_object = heap.next_object.max(limit);
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
    if !dead.is_empty() || !reached_kotlin_lazy.is_empty() {
        let mut last_recv = String::new();
        let mut last_keep = true;
        heap.fields.retain(|key, _| {
            if let Some(holder) = kotlin_lazy_field_handle(key) {
                reached_kotlin_lazy.contains(holder)
            } else {
                let recv = field_receiver(key);
                if recv != last_recv.as_str() {
                    last_recv.clear();
                    last_recv.push_str(recv);
                    last_keep = keep(recv);
                }
                last_keep
            }
        });
    }
    if !dead.is_empty() {
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
        host.recipes.prune_rows(keep);
    }
    if !host.render_objects.is_empty() {
        host.render_objects
            .retain(|key, _| render_live.contains(key));
        host.render_handles.retain(|key| render_live.contains(key));
    }
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
        self.heap_flags |= heap.heap_flags;
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
            collections: 0,
            roots,
            heap_flags: self.heap_flags,
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields)
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

    /// The holder matcher answers `text.contains(holder)` for every holder at once: holders that
    /// nest, overlap, share prefixes or suffixes, repeat in the text, are empty, or hold
    /// multi-byte characters, and a generated set besides.
    #[test]
    fn the_lazy_holder_matcher_answers_contains_for_every_holder() {
        fn check(holders: &[String], texts: &[String]) {
            let mut heap = JavaHeap::default();
            for (index, holder) in holders.iter().enumerate() {
                heap.fields.insert(
                    format!("{KOTLIN_LAZY_FIELD_PREFIX}{holder}"),
                    Value::Int(index as i32),
                );
            }
            let lazy = LazyHolders::new(&heap);
            for text in texts {
                let mut found = BTreeSet::new();
                lazy.matches(text, |index| {
                    found.insert(lazy.holders[index].0);
                });
                let expected: BTreeSet<&str> = holders
                    .iter()
                    .map(String::as_str)
                    .filter(|holder| text.contains(holder))
                    .collect();
                assert_eq!(found, expected, "{text:?}");
            }
        }
        let strings = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        check(
            &strings(&[
                "",
                "a",
                "ab",
                "abc",
                "bc",
                "c",
                "abab",
                "ba",
                "J:7",
                "J:71",
                "\u{e9}",
                "a\u{e9}",
                "s:SYNCHRONIZED,J:7,@indy:demo/Mod#1",
            ]),
            &strings(&[
                "",
                "a",
                "abab",
                "xabcx",
                "cba",
                "bbbbc",
                "J:7",
                "J:1",
                "\u{e9}a\u{e9}",
                "[s:SYNCHRONIZED,J:71,@indy:demo/Mod#1]",
                "[s:SYNCHRONIZED,J:7,@indy:demo/Mod#1]",
            ]),
        );
        // Generated over a small alphabet, so holders overlap in every way.
        let mut seed = 0x2545_f491_u64;
        let mut next = move |bound: u64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) % bound
        };
        let mut word = |max: u64| {
            (0..next(max))
                .map(|_| ["a", "b", "#", "1"][next(4) as usize])
                .collect::<String>()
        };
        let holders: Vec<String> = (0..200).map(|_| word(7)).collect();
        let texts: Vec<String> = (0..400).map(|_| word(40)).collect();
        check(&holders, &texts);
    }

    /// A lazy state lives while a traced text contains its holder, and what its value names lives
    /// with it, transitively; one nothing reaches goes. Same closure as before #609 round 4.
    #[test]
    fn lazy_states_live_while_a_traced_text_contains_their_holder() {
        let (mut host, mut heap) = graph("demo/Mod#unused");
        // A static embeds holder `L1`; L1's value embeds holder `L2` and names node 3; L2's value
        // names node 1. `L3` is reached by nothing.
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
            "demo/Mod#LAZYLjava/lang/Object;".into(),
            text("@lazy[L1,J:7]"),
        );
        heap.fields.insert(
            format!("{KOTLIN_LAZY_FIELD_PREFIX}L1,J:7"),
            text("[L2,J:8|demo/Node#3]"),
        );
        heap.fields.insert(
            format!("{KOTLIN_LAZY_FIELD_PREFIX}L2,J:8"),
            text("demo/Node#1"),
        );
        heap.fields.insert(
            format!("{KOTLIN_LAZY_FIELD_PREFIX}L3,J:9"),
            text("demo/Node#1"),
        );
        collect(&mut heap, &mut host, &[]);
        let lazy: Vec<&str> = heap
            .fields
            .keys()
            .filter_map(|key| kotlin_lazy_field_handle(key))
            .collect();
        assert_eq!(lazy, ["L1,J:7", "L2,J:8"]);
        assert_eq!(
            objects(&heap)
                .into_iter()
                .filter(|object| object.starts_with("demo/"))
                .collect::<Vec<_>>(),
            ["demo/Node#1", "demo/Node#2", "demo/Node#3", "demo/Node#4"]
        );
    }

    fn stack_holding(host: &mut StdHost, value: Value) -> Value {
        let mut patch = crate::recipe_models::ComponentPatch::default();
        patch.entries.insert("demo:held".into(), Some(value));
        host.recipes
            .alloc_stack(crate::recipe_models::LiveStack::from_item_id(
                "minecraft:stone".into(),
                1,
                patch,
            ))
    }

    fn key(value: &Value) -> String {
        value_text(value).unwrap().to_owned()
    }

    /// A row made in a transient scope is an object: kept while something reaches it, its values
    /// edges, dropped once nothing does. A row made outside one is a root, as before.
    #[test]
    fn rows_made_in_a_transient_scope_live_only_while_reached() {
        let (mut host, mut heap) = graph("demo/Mod#unused");
        let (reached, unreached) = host.with_transient_rows(|host| {
            (
                stack_holding(host, text("demo/Node#1")),
                stack_holding(host, text("demo/Node#3")),
            )
        });
        let rooted = stack_holding(&mut host, text("[Ldemo/Node;#1"));
        std::sync::Arc::make_mut(&mut host.persistent_static_fields)
            .insert("demo/Mod#HELDLjava/lang/Object;".into(), reached.clone());
        collect(&mut heap, &mut host, &[]);
        let stacks: Vec<&String> = host.recipes.stacks.keys().collect();
        assert_eq!(stacks, [&key(&reached), &key(&rooted)]);
        assert_eq!(
            host.recipes.collectable_rows.iter().collect::<Vec<_>>(),
            [&key(&reached)]
        );
        // Node 1 lives through the reached row, the array through the root row; node 3 went
        // with the row nothing reached.
        assert_eq!(
            objects(&heap),
            [
                "demo/Node#1",
                "demo/Node#2",
                "[Ldemo/Node;#0",
                "[Ldemo/Node;#1"
            ]
        );
        // The static lets go: the row and what only it reached go too.
        std::sync::Arc::make_mut(&mut host.persistent_static_fields)
            .insert("demo/Mod#HELDLjava/lang/Object;".into(), Value::Ref(None));
        collect(&mut heap, &mut host, &[]);
        assert_eq!(
            host.recipes.stacks.keys().collect::<Vec<_>>(),
            [&key(&rooted)]
        );
        assert!(host.recipes.collectable_rows.is_empty());
        assert_eq!(objects(&heap), ["[Ldemo/Node;#1"]);
        let _ = unreached;
    }

    /// While a scope is open its rows are roots: an embedder holds what it was handed between its
    /// calls, and any call's store may collect. Only the outermost close makes them collectable.
    #[test]
    fn rows_of_an_open_scope_survive_collections_until_it_closes() {
        let (mut host, mut heap) = graph("demo/Mod#unused");
        let outer = host.begin_transient_rows();
        let held = stack_holding(&mut host, text("demo/Node#1"));
        let inner = host.begin_transient_rows();
        let inner_held = stack_holding(&mut host, text("demo/Node#3"));
        host.end_transient_rows(inner);
        // Inner closed, outer open: nothing is collectable yet.
        collect(&mut heap, &mut host, &[]);
        assert!(host.recipes.collectable_rows.is_empty());
        assert_eq!(
            host.recipes.stacks.keys().collect::<Vec<_>>(),
            [&key(&held), &key(&inner_held)]
        );
        assert!(objects(&heap).contains(&"demo/Node#1"));
        assert!(objects(&heap).contains(&"demo/Node#3"));
        host.end_transient_rows(outer);
        collect(&mut heap, &mut host, &[]);
        assert!(host.recipes.stacks.is_empty());
        assert!(!objects(&heap).contains(&"demo/Node#1"));
    }

    /// A scope never closed leaves its rows roots, as rows were before scopes.
    #[test]
    fn an_unclosed_scope_leaves_its_rows_roots() {
        let (mut host, mut heap) = graph("demo/Mod#unused");
        let scope = host.begin_transient_rows();
        let held = stack_holding(&mut host, text("demo/Node#1"));
        drop(scope); // never handed to `end_transient_rows`
        collect(&mut heap, &mut host, &[]);
        assert_eq!(
            host.recipes.stacks.keys().collect::<Vec<_>>(),
            [&key(&held)]
        );
        assert!(objects(&heap).contains(&"demo/Node#1"));
    }

    /// The scope's depth is restored however its body ends.
    #[test]
    fn a_transient_scope_closes_on_unwind() {
        let mut host = StdHost::new();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            host.with_transient_rows(|_| panic!("body failed"))
        }));
        assert!(caught.is_err());
        let stack = stack_holding(&mut host, Value::Int(1));
        assert!(host.recipes.collectable_rows.is_empty(), "{stack:?}");
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
            "demo/Mod#ROOTLjava/lang/Object;".into(),
            crate::callback_marker::lambda_from_text(
                "demo/Mod#init",
                4,
                "",
                "demo/Mod#lambda$0(Ldemo/Node;)V",
                "rdemo/Node#3",
            ),
        );
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
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

        std::sync::Arc::make_mut(&mut host.persistent_static_fields).clear();
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

    /// A collectable key numbered past the heap's next object (a host key that spells an
    /// allocation key with a counter of its own) raises the collection's limit; the heap's next
    /// object follows it, so a number the collection freed is never handed out again (#609,
    /// Runes round 2 bug 1: every object built after it read as collected).
    #[test]
    fn the_next_object_never_starts_inside_the_freed_numbers() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap {
            next_object: 3,
            ..Default::default()
        };
        heap.fields.insert(field("demo/Item#1", "x"), Value::Int(1));
        std::sync::Arc::make_mut(&mut host.persistent_static_fields)
            .insert("demo/Mod#ITEMLdemo/Item;".into(), text("demo/Item#1"));
        // Unreached, numbered by something other than this heap.
        heap.fields
            .insert(field("demo/Elsewhere#50", "x"), Value::Int(2));
        collect(&mut heap, &mut host, &[]);
        assert!(host.freed_objects.holds_key("demo/Elsewhere#50"));
        assert!(!host.freed_objects.holds_key("demo/Item#1"));
        assert_eq!(heap.next_object, 51);
        let next = format!("demo/Screen#{}", heap.next_object);
        assert!(!host.freed_objects.holds_key(&next), "{next} is born freed");
    }

    /// A key numbered at the top moves the heap's next number to `usize::MAX`, the exhausted
    /// mark the allocator never hands out (`Runtime::new_object_key`): the next call refuses
    /// instead of numbering two objects alike.
    #[test]
    fn a_key_at_the_top_exhausts_the_heap_instead_of_wrapping() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();
        heap.fields.insert(
            field(&format!("demo/Top#{}", usize::MAX - 1), "x"),
            Value::Int(1),
        );
        collect(&mut heap, &mut host, &[]);
        assert_eq!(heap.next_object, usize::MAX);
    }

    /// A tooltip sink is no allocation key: live during its `appendHoverText`'s store, its
    /// serial (one per tooltip frame, process-wide) never moves the heap's limit, and the lines
    /// it holds are roots.
    #[test]
    fn a_live_tooltip_sink_neither_moves_the_limit_nor_loses_its_lines() {
        let (mut host, mut heap) = graph("demo/Node#1");
        heap.next_object = 5;
        // A line object the sink holds and nothing else reaches.
        heap.fields
            .insert(field("demo/Line#4", "text"), text("hint"));
        let sink = host.tooltip_sink();
        let Value::Host(handle) = &sink else {
            panic!("{sink:?}")
        };
        assert!(
            crate::jar_object::parse_key(handle.key()).is_none(),
            "{} spells an allocation key",
            handle.key()
        );
        host.tooltip_sinks
            .get_mut(handle.key())
            .unwrap()
            .push(crate::jar_object::handle("demo/Line#4".into()));
        collect(&mut heap, &mut host, &[]);
        assert_eq!(heap.next_object, 5);
        assert!(!host.freed_objects.holds_key("demo/Line#4"));
        assert!(heap.fields.contains_key(&field("demo/Line#4", "text")));
        assert_eq!(host.take_tooltip_sink(&sink).len(), 1);
    }

    /// Every `#<digits>` a text holds, embedded keys included.
    #[test]
    fn numbers_are_read_from_every_key_a_text_names() {
        let heap = JavaHeap::default();
        let host = StdHost::new();
        let collectable = Collectable::new(&heap, &host);
        let mut reached = Vec::new();
        let mut pending = Vec::new();
        let mut out = Vec::new();
        let mut out_menu = Vec::new();
        collectable.scan_text(
            "demo/Lock#5",
            &mut reached,
            &mut pending,
            &mut out,
            &mut out_menu,
        );
        let callback = crate::callback_marker::test_token("demo/Main#lambda|demo/Cell#12|x#|#");
        collectable.scan_text(
            crate::callback_marker::callback_text(&callback).unwrap(),
            &mut reached,
            &mut pending,
            &mut out,
            &mut out_menu,
        );
        collectable.scan_text(
            "HostRef { key: \"demo/A#7\" } #neutral3",
            &mut reached,
            &mut pending,
            &mut out,
            &mut out_menu,
        );
        assert_eq!(out, [5, 12, 7]);
    }

    /// An object with no row of its own (`new Object()` kept as a lock) is
    /// live while a root names it, so its number is never freed; one nothing
    /// names is freed with the rowed garbage around it, and the host's data
    /// for it (a `UUID`'s bits) goes with it.
    #[test]
    fn an_object_without_rows_is_live_while_named() {
        let (mut host, mut heap) = graph("demo/Node#1");
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
            "demo/Mod#LOCKLjava/lang/Object;".into(),
            text("java/lang/Object#9"),
        );
        host.constructed_uuids
            .insert("java/util/UUID#10".into(), (1, 2));
        host.constructed_uuids
            .insert("java/util/UUID#11".into(), (3, 4));
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).clear();
        host.release(&matrix);
        collect(&mut heap, &mut host, &[]);
        assert!(host.render_objects.is_empty());
    }

    #[test]
    fn entity_registration_handles_follow_callback_collection_roots() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();
        // The models number their objects with the heap's allocator (#669).
        let mut next = heap.next_object;
        let mut numbers = crate::jar_object::Numbers::new(&mut next);
        let category = crate::entity_registration::category_constant(
            &mut host.entity_registration,
            Some(&mut numbers),
            "CREATURE",
        )
        .unwrap()
        .unwrap();
        let builder = crate::entity_registration::static_call(
            &mut host.entity_registration,
            &crate::MethodRef {
                owner: "net/minecraft/world/entity/EntityType$Builder".into(),
                name: "of".into(),
                descriptor: "()".into(),
            },
            &[Value::Ref(Some("factory".into())), category],
            Some(&mut numbers),
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
            Some(&mut numbers),
        )
        .unwrap()
        .unwrap();
        let accessor = crate::entity_registration::define_id(
            &mut host.entity_registration,
            "net/minecraft/world/entity/Entity",
            Some(&mut numbers),
        )
        .unwrap()
        .unwrap();
        heap.next_object = next;
        assert_eq!(
            crate::entity_registration::accessor_id(&host.entity_registration, &accessor),
            Some(8)
        );
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
            "demo/Mod#ROOTLjava/util/List;".into(),
            text("java/util/ArrayList#neutral0"),
        );
        collect(&mut heap, &mut host, &[]);
        // The built entity type and the accessor's id row, both reached from the root list.
        assert_eq!(host.entity_registration.entry_count(), 2);
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).clear();
        collect(&mut heap, &mut host, &[]);
        assert_eq!(host.entity_registration.entry_count(), 0);
        assert!(heap.fields.is_empty());
        // The category constant is a root: its number is never freed.
        assert!(
            !host.freed_objects.holds_key(
                crate::jar_object::object_key(
                    &crate::entity_registration::category_constant(
                        &mut host.entity_registration,
                        None,
                        "CREATURE"
                    )
                    .unwrap()
                    .unwrap()
                )
                .unwrap()
            )
        );
    }

    /// A placement snapshot whose hand item is the jar object `item`.
    fn placement_with_item(item: &str) -> crate::BlockPlaceContextData {
        crate::BlockPlaceContextData {
            clicked_pos: [1, 2, 3],
            clicked_face: text("enum/net/minecraft/core/Direction/UP"),
            click_location: [1.5, 2.5, 3.5],
            inside: false,
            replacing_clicked_on_block: false,
            can_place: true,
            placer: crate::Placer::Player {
                player: text("demo/Player#7"),
                yaw: 0.0,
                pitch: 0.0,
                secondary_use: false,
            },
            item_in_hand: text(item),
            level: text("demo/Level#8"),
        }
    }

    /// #635: a placement snapshot is a root while it is registered and
    /// nothing once released. Its hand item, player and level (jar objects
    /// here, so the collector may free them) survive a collection that runs
    /// while the placement does, and the first collection after it frees
    /// them, with the `Direction[]` hand-off it answered.
    #[test]
    fn a_placement_snapshot_roots_its_objects_only_until_it_is_released() {
        let mut host = StdHost::new();
        let mut heap = JavaHeap::default();
        for object in ["demo/Item#6", "demo/Player#7", "demo/Level#8"] {
            heap.fields.insert(field(object, "tag"), Value::Int(41));
        }
        heap.next_object = 9;
        let context = host.new_block_place_context(placement_with_item("demo/Item#6"));
        let array = host.store_placement_direction_array(
            value_text(&context).unwrap(),
            ["DOWN", "NORTH", "EAST", "SOUTH", "WEST", "UP"],
        );
        assert!(host.host_array_values(&array).is_some());
        collect(&mut heap, &mut host, &[]);
        assert_eq!(
            objects(&heap),
            ["demo/Item#6", "demo/Level#8", "demo/Player#7"]
        );

        host.release_block_place_context(&context);
        assert_eq!(host.block_place_context_count(), 0);
        assert_eq!(host.placement_array_count(), 0);
        assert!(host.host_array_values(&array).is_none());
        collect(&mut heap, &mut host, &[]);
        assert!(objects(&heap).is_empty(), "{:?}", objects(&heap));
        assert!(host.freed_objects.holds_key("demo/Item#6"));
    }

    /// The scope `NativeJar::block_state_for_placement` holds releases its
    /// snapshot when the invocation unwinds, not only when it returns.
    #[test]
    fn a_placement_scope_releases_its_snapshot_on_unwind() {
        let mut host = StdHost::new();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let scope =
                crate::PlacementScope::open(&mut host, placement_with_item("demo/Item#6")).unwrap();
            assert_eq!(scope.host.block_place_context_count(), 1);
            panic!("a host model panicked mid-placement");
        }));
        assert!(unwound.is_err());
        assert_eq!(host.block_place_context_count(), 0);
        assert_eq!(host.placement_array_count(), 0);
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
            "placement_contexts",
            "placement_arrays",
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
            "item_lores",
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
            "block_use",
            "level_sound",
        ];
        const HOLDS_NO_OBJECT: &[&str] = &[
            // A dimension name and a lend id (`level_random.rs`).
            "level_random_lend",
            // Stream names (`level_random.rs`).
            "level_random_unknown",
            // Handle keys, numbered once and never reused (`random_source.rs`).
            "random_derived_unknown",
            // A key counter (`random_source.rs`).
            "next_random_source",
            // Counts of entrypoint declarations the runner reached (`lib.rs`).
            "entrypoint_declarations_run",
            "next_unmodifiable_collection",
            "next_reversed_view",
            "next_ranged_attribute",
            "next_voxel_shape",
            "next_shape_extraction",
            "next_menu_object",
            "next_random_source",
            "next_placement_handle",
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
            "initialized_game_mixins",
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
            // Whether the bootstrap Mixin callbacks ran (`bootstrap_mixins.rs`): a flag.
            "bootstrap_injectors_ran",
            // A count of registrations handled (`jar_components.rs`).
            "component_defaults_declared",
            // Active depth of ephemeral call-log scopes (`rewind_call_log` at exit).
            "suppress_call_log_depth",
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
                    live: crate::menu_models::LiveState::default(),
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
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
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
            "demo/Leak#HOLDERLdemo/Holder;".into(),
            crate::jar_object::handle("demo/Holder#1".into()),
        );
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
            "demo/Leak#ARRLjava/lang/Object;".into(),
            Value::Host(crate::host_ref::HostRef::new(
                crate::host_ref::HostKind::Array,
                "[Ljava/lang/Object;#2",
            )),
        );
        std::sync::Arc::make_mut(&mut host.persistent_static_fields).insert(
            "demo/Leak#LISTLjava/util/List;".into(),
            text("java/util/ArrayList#neutral3"),
        );
        heap.next_object = 4;

        // While any live reference holds the probe, GC keeps `retained_shape_probes` armed.
        collect(&mut heap, &mut host, &[]);
        assert!(host.has_retained_shape_probes());

        // Drop the static probe; instance field, array, and list still hold it.
        std::sync::Arc::make_mut(&mut host.persistent_static_fields)
            .remove("demo/Leak#SAVEDLnet/minecraft/world/level/BlockGetter;");
        collect(&mut heap, &mut host, &[]);
        assert!(host.has_retained_shape_probes());

        // Drop holder and array; list still holds it.
        std::sync::Arc::make_mut(&mut host.persistent_static_fields)
            .remove("demo/Leak#HOLDERLdemo/Holder;");
        std::sync::Arc::make_mut(&mut host.persistent_static_fields)
            .remove("demo/Leak#ARRLjava/lang/Object;");
        collect(&mut heap, &mut host, &[]);
        assert!(host.has_retained_shape_probes());

        // Drop the list too: GC sweeps the list and disarms `retained_shape_probes`.
        std::sync::Arc::make_mut(&mut host.persistent_static_fields)
            .remove("demo/Leak#LISTLjava/util/List;");
        collect(&mut heap, &mut host, &[]);
        assert!(!host.has_retained_shape_probes());
    }
}
