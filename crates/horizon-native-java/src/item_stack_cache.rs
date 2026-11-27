//! Reusing the Java `ItemStack` a game stack was built into, while nothing has changed it.
//!
//! An embedder answers a client draw of a jar item (its item model property, its bar, its
//! tooltip image) by building a Java `ItemStack` for the game's stack
//! ([`NativeJar::build_item_stack`]) and running the item's Java code on it. Building one decodes
//! every jar data component on it with the type's own codec, which runs that codec's bytecode:
//! for a bundle, its whole contents. Drawing a pouch did this several times a frame (#609
//! round 4 bug 1).
//!
//! The game itself decodes a stack once, when the stack arrives, and draws that same object
//! every frame after. [`NativeJar::reusable_item_stack`] does the same: the stack built for one
//! set of inputs is kept and handed to the next draw with the same inputs, so long as
//! everything the build made is still exactly as the build left it.
//!
//! # When a kept stack is handed out again
//!
//! * **Same inputs.** The key is every input of the build, exactly ([`ItemStackRequest`]'s
//!   `Debug` text): item, count, component patch with each jar component's NBT, the item's
//!   prototype values, and the prototypes of the items the components name.
//! * **Nothing changed.** When a stack is kept, the collector's own edges are followed from it
//!   to every object, array, collection and recipe row the build made (the "born" set; objects
//!   that existed before the build are shared by a rebuilt stack too, so they don't count), and
//!   the full state of each is recorded ([`born_state`]). Before each reuse it is recorded again;
//!   any difference (a Java call set a component, grew a list, wrote a field, initialised a
//!   Kotlin lazy) drops the stack and the key is built fresh on every call from then on, as
//!   before this cache.
//! * **Only kinds whose whole state the record covers.** A build that made an iterator, a map
//!   entry, a chat component, a random source, a menu, a view or any other object with state in
//!   a host table the record doesn't read is never kept. What it reads: instance fields, arrays,
//!   string builders, enum constants, collections and their layouts, an unmodifiable collection
//!   that owns its elements (`List.of`), recipe rows and Kotlin lazy states.
//! * **Only an exact build.** A build that counted an unmodelled call is never kept.
//!
//! What a kept stack can't reproduce is a rebuild's identity: every draw of an unchanged stack
//! gets the same object, as the game's draws do, where without this cache each draw got a new
//! one. Java code can only tell the two apart through a reference it kept from an earlier draw,
//! and neither answer was the game's (which hands each slot's own object) before this.
//!
//! The model property's, bar's or tooltip's *answer* is never cached: the item's Java code
//! runs on every draw.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;

use crate::heap::{Collectable, kotlin_lazy_field_key, value_texts};
use crate::recipe_models::RuntimeComponentValue;
use crate::{NativeError, NativeJar, Nbt, StdHost, Value};

/// Stacks kept at once. A frame draws a handful of distinct jar stacks; the least recently used
/// goes when a new one would pass this.
const CAPACITY: usize = 64;
/// Keys remembered as not worth keeping before the set is cleared (each costs a rebuild per
/// draw, as without the cache, and nothing else).
const REFUSED_CAPACITY: usize = 4096;

/// What a Java `ItemStack` for a game stack is built from: [`NativeJar::make_item_stack`]'s
/// arguments, and the prototype values of every item its jar components name, recorded first
/// ([`NativeJar::record_item_prototype`]), since the components' codecs read them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemStackRequest {
    pub item_override: Option<Value>,
    pub item_id: String,
    pub count: i32,
    pub patch: Vec<(String, Option<RuntimeComponentValue>)>,
    pub defaults: Vec<(String, Option<Nbt>)>,
    pub opaque_components: bool,
    pub opaque_keys: Vec<String>,
    /// `(item id, its prototype values)` for each item the components name.
    pub named_prototypes: Vec<(String, Vec<(String, Option<Nbt>)>)>,
}

/// The stacks [`NativeJar::reusable_item_stack`] keeps, on the jar's host. Each kept stack is a
/// root of the heap collector ([`Self::root_texts`]) until it goes.
#[derive(Clone, Debug, Default)]
pub(crate) struct ItemStackCache {
    entries: BTreeMap<String, Kept>,
    /// Keys found not worth keeping (an inexact build, a kind the record doesn't cover, or a
    /// stack something changed): built fresh on every call.
    refused: BTreeSet<String>,
    clock: u64,
    hits: u64,
    builds: u64,
}

#[derive(Clone, Debug)]
struct Kept {
    stack: Value,
    /// The markers (and `kotlin-lazy|...` field keys) the build made that the stack reaches.
    born: Vec<String>,
    /// [`born_state`] of `born` when the stack was kept.
    state: String,
    used: u64,
}

impl ItemStackCache {
    /// The kept stacks: roots of the heap collector, so a kept stack and what it reaches live.
    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        for kept in self.entries.values() {
            out.extend(value_texts(&kept.stack));
        }
    }

    fn refuse(&mut self, key: String) {
        if self.refused.len() >= REFUSED_CAPACITY {
            self.refused.clear();
        }
        self.refused.insert(key);
    }
}

impl StdHost {
    /// `(stacks kept, draws answered with a kept stack, stacks built)` of
    /// [`NativeJar::reusable_item_stack`].
    pub fn item_stack_cache_counts(&self) -> (usize, u64, u64) {
        let cache = &self.item_stack_cache;
        (cache.entries.len(), cache.hits, cache.builds)
    }
}

impl NativeJar {
    /// A Java `ItemStack` built from `request`: the named items' prototypes recorded, then
    /// [`Self::make_item_stack`].
    pub fn build_item_stack(
        &self,
        host: &mut StdHost,
        request: ItemStackRequest,
    ) -> Result<Value, NativeError> {
        for (item_id, defaults) in request.named_prototypes {
            self.record_item_prototype(host, &item_id, defaults)?;
        }
        self.make_item_stack(
            host,
            request.item_override,
            &request.item_id,
            request.count,
            request.patch,
            request.defaults,
            request.opaque_components,
            request.opaque_keys,
        )
    }

    /// [`Self::build_item_stack`], or the stack an earlier call built from the same `request`
    /// while nothing it made has changed since (see the module docs). The stack is the host's
    /// until a later call of this lets it go: an embedder uses it for the call it asked for,
    /// and keeps it no longer.
    pub fn reusable_item_stack(
        &self,
        host: &mut StdHost,
        request: ItemStackRequest,
    ) -> Result<Value, NativeError> {
        let key = format!("{request:?}");
        let cache = &mut host.item_stack_cache;
        cache.clock += 1;
        let now = cache.clock;
        if cache.refused.contains(&key) {
            cache.builds += 1;
            return self.build_item_stack(host, request);
        }
        if let Some(kept) = host.item_stack_cache.entries.get(&key) {
            if born_state(host, &kept.born) == kept.state {
                let cache = &mut host.item_stack_cache;
                cache.hits += 1;
                let kept = cache.entries.get_mut(&key).expect("looked up above");
                kept.used = now;
                return Ok(kept.stack.clone());
            }
            // Something changed what the build made: never kept again.
            let cache = &mut host.item_stack_cache;
            cache.entries.remove(&key);
            cache.refuse(key);
            cache.builds += 1;
            return self.build_item_stack(host, request);
        }
        host.item_stack_cache.builds += 1;
        let counted = host.unknown_host_call_count();
        let existing = existing_keys(host);
        let stack = self.build_item_stack(host, request)?;
        let born = if host.unknown_host_call_count() == counted {
            born_keys(host, &stack, &existing)
        } else {
            None
        };
        let Some(born) = born else {
            host.item_stack_cache.refuse(key);
            return Ok(stack);
        };
        let state = born_state(host, &born);
        let cache = &mut host.item_stack_cache;
        if cache.entries.len() >= CAPACITY
            && let Some(oldest) = cache
                .entries
                .iter()
                .min_by_key(|(_, kept)| kept.used)
                .map(|(key, _)| key.clone())
        {
            cache.entries.remove(&oldest);
        }
        cache.entries.insert(
            key,
            Kept {
                stack: stack.clone(),
                born,
                state,
                used: now,
            },
        );
        Ok(stack)
    }
}

/// Every object, array, collection and recipe row the host and its resident heap hold now, and
/// every Kotlin lazy state: what a build found already there.
fn existing_keys(host: &StdHost) -> HashSet<String> {
    let heap = &host.heap;
    let mut out = Collectable::new(heap, host).markers;
    out.extend(recipe_row_keys(host).map(str::to_owned));
    out.extend(
        heap.fields
            .keys()
            .filter(|key| kotlin_lazy_field_key(key))
            .cloned(),
    );
    out
}

fn recipe_row_keys(host: &StdHost) -> impl Iterator<Item = &str> {
    let recipes = &host.recipes;
    recipes
        .stacks
        .keys()
        .chain(recipes.templates.keys())
        .chain(recipes.patches.keys())
        .chain(recipes.patch_builders.keys())
        .chain(recipes.ingredients.keys())
        .chain(recipes.holders.keys())
        .chain(recipes.common_infos.keys())
        .chain(recipes.custom_data.keys())
        .chain(recipes.tooltip_displays.keys())
        .map(String::as_str)
}

/// The markers (and lazy-state field keys) `stack` reaches that are not in `existing`: what its
/// build made and it still holds. `None` when one of them has state the record doesn't read.
///
/// The walk follows the edges the heap collector follows for the kinds it keeps (instance
/// fields, array elements, collection elements, a repeated collection's value, a recipe row's
/// values, a Kotlin lazy's value) and finds markers in texts the way the collector does
/// (`Collectable::scan`), here among the new ones only.
fn born_keys(host: &StdHost, stack: &Value, existing: &HashSet<String>) -> Option<Vec<String>> {
    let heap = &host.heap;
    let mut new = Collectable::new(heap, host);
    new.markers.extend(recipe_row_keys(host).map(str::to_owned));
    new.markers.retain(|marker| !existing.contains(marker));
    new.refresh_lengths();
    // Rows of host tables the record doesn't read: a new object with one isn't kept. One kind
    // of row is read: an unmodifiable collection that owns its elements (`List.of`, a codec's
    // decoded list: `unmodifiable_collections` holds `None` for it), whose state is its elements
    // and that row. A view (`Some(source)`) reads another collection, and isn't kept. Counted per
    // table, so an object with that row and any other still counts as unread.
    let mut rows: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for key in host.state_table_keys() {
        *rows.entry(key).or_default() += 1;
    }
    let read = |key: &str| usize::from(host.unmodifiable_collections.get(key) == Some(&None));
    let unread: HashSet<&str> = rows
        .into_iter()
        .filter(|&(key, count)| count > read(key))
        .map(|(key, _)| key)
        .chain(host.iterators.keys().map(String::as_str))
        .chain(host.enumerations.keys().map(String::as_str))
        .chain(host.component_states.keys().map(String::as_str))
        .chain(host.map_entries.keys().map(String::as_str))
        .chain(host.set_from_map_views.keys().map(String::as_str))
        .collect();
    let mut born = BTreeSet::new();
    let mut texts: Vec<&str> = value_texts(stack);
    let mut found = Vec::new();
    while let Some(text) = texts.pop() {
        let lazy = format!("kotlin-lazy|{text}");
        if let Some(value) = heap.fields.get(&lazy)
            && !existing.contains(&lazy)
            && born.insert(lazy)
        {
            texts.extend(value_texts(value));
        }
        new.scan(text, &mut found);
        for marker in found.drain(..) {
            if !born.insert(marker.clone()) {
                continue;
            }
            if unread.contains(marker.as_str()) {
                return None;
            }
            let prefix = format!("{marker}|");
            let fields = heap
                .fields
                .range(prefix.clone()..)
                .take_while(|(key, _)| key.starts_with(&prefix))
                .map(|(_, value)| value);
            let arrays = heap
                .arrays
                .get(&marker)
                .into_iter()
                .flat_map(crate::array_data::ArrayData::references);
            let collections = host.mutable_collections.get(&marker).into_iter().flatten();
            let repeated = host
                .repeated_collections
                .get(&marker)
                .map(|(_, value)| value);
            let rows = host.recipes.row_values(&marker);
            for value in fields
                .chain(arrays)
                .chain(collections)
                .chain(repeated)
                .chain(rows)
            {
                texts.extend(value_texts(value));
            }
        }
    }
    Some(born.into_iter().collect())
}

/// The full state of each of `born`, as text: every heap and host row keyed by it. Two equal
/// texts mean nothing Java can read through those objects changed.
fn born_state(host: &StdHost, born: &[String]) -> String {
    let heap = &host.heap;
    let mut out = String::new();
    let mut keyed = Vec::new();
    for marker in born {
        let _ = write!(out, "\u{0}{marker}\u{1}");
        if kotlin_lazy_field_key(marker) {
            let _ = write!(out, "{:?}", heap.fields.get(marker));
            continue;
        }
        let prefix = format!("{marker}|");
        for (key, value) in heap
            .fields
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
        {
            let _ = write!(out, "{key}={value:?};");
        }
        let _ = write!(
            out,
            "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
            heap.arrays.get(marker),
            heap.string_builders.get(marker),
            heap.enum_constants.get(marker),
            host.mutable_collections.get(marker),
            host.repeated_collections.get(marker),
            host.hash_tables.get(marker),
            host.red_black_trees.get(marker),
            host.probe_tables.get(marker),
            host.mod_stamps.get(marker),
            host.deque_layouts.get(marker),
            host.view_kinds.get(marker),
            host.empty_collections.contains(marker),
            host.jdk_list_of.contains(marker),
            host.detached_sub_lists.contains(marker),
            host.identifier_objects.get(marker),
            host.constructed_uuids.get(marker),
        );
        host.recipes.row_state(marker, &mut out);
        // Every other per-object table, so a row one gains later shows as a change.
        keyed.clear();
        host.keyed_texts(marker, &mut keyed);
        for text in &keyed {
            out.push_str(text);
        }
    }
    out
}
