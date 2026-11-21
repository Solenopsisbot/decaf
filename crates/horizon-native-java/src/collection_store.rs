//! The host's mutable collections by marker, each with a generation (#203).
//!
//! A hash collection's bucket table (`hash_table.rs`) mirrors the host's list
//! of its entries. Before #203 every member call reconciled the two
//! (`Runtime::resync_hash_table`), an O(n) pass (O(n²) with the table's
//! linear lookups) even when nothing had touched the list. The store gives
//! every collection a generation that moves on **every** mutable access:
//! `insert`, `get_mut`, `entry`, `remove` (`retain_markers` drops whole
//! collections and changes no kept one). There is no other way to
//! change a list (no `DerefMut`), so a table synced at generation `g` still
//! mirrors its list while the generation is `g`, and the reconciliation can
//! be skipped. Reads go through `Deref` to the map and move nothing. A
//! mutable access that changes nothing only costs the next call a
//! reconciliation.

use std::collections::{BTreeMap, HashMap, HashSet, btree_map};

use crate::Value;

#[derive(Clone, Debug, Default)]
pub struct CollectionStore {
    map: BTreeMap<String, Vec<Value>>,
    /// Backing capacity for the plain ArrayList/Vector rows this host owns:
    /// `(capacity, vector)`. Hash tables keep their own lazy table state.
    capacities: BTreeMap<String, (usize, bool)>,
    generations: HashMap<String, u64>,
    next: u64,
    /// The collections someone mirrors elsewhere (an `Arrays.asList` view's
    /// array, `array_views.rs`), and how many mutable accesses reached one:
    /// while that count stands still, no mirror needs a look.
    watched: HashSet<String>,
    watched_writes: u64,
    /// Per-collection known `(run_id, epoch)` for `IdentityHashMap` /
    /// `IdentityHashSet`, stored directly on the host collection store so
    /// `remove`, epoch bumps, and `retain_markers` (GC) prune entries
    /// automatically without any separate runtime side map.
    next_identity_run: u64,
    identity_epochs: HashMap<String, (u64, u64)>,
}

impl std::ops::Deref for CollectionStore {
    type Target = BTreeMap<String, Vec<Value>>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl CollectionStore {
    fn bump(&mut self, marker: &str) -> u64 {
        self.next += 1;
        let next = self.next;
        if !self.watched.is_empty() && self.watched.contains(marker) {
            self.watched_writes += 1;
        }
        match self.generations.get_mut(marker) {
            Some(generation) => *generation = next,
            None => {
                self.generations.insert(marker.to_owned(), next);
            }
        }
        next
    }

    /// The generation of `marker`'s list: it changes whenever the list may
    /// have (`0` for a marker never written).
    pub(crate) fn generation(&self, marker: &str) -> u64 {
        self.generations.get(marker).copied().unwrap_or(0)
    }

    pub(crate) fn insert(&mut self, marker: String, values: Vec<Value>) -> Option<Vec<Value>> {
        self.bump(&marker);
        self.map.insert(marker, values)
    }

    pub(crate) fn remove(&mut self, marker: &str) -> Option<Vec<Value>> {
        self.bump(marker);
        self.capacities.remove(marker);
        self.identity_epochs.remove(marker);
        self.map.remove(marker)
    }

    /// Allocate a fresh per-invocation run identifier for identity-epoch tracking
    /// and drop any stale entries from earlier runs or earlier epochs.
    pub(crate) fn next_identity_run(&mut self) -> u64 {
        self.next_identity_run = self.next_identity_run.saturating_add(1);
        self.identity_epochs.clear();
        self.next_identity_run
    }

    /// Clear per-collection identity-epoch entries after an epoch bump (every
    /// pre-existing collection had an older epoch and defaults to `0 < epoch`).
    pub(crate) fn clear_identity_epochs(&mut self) {
        self.identity_epochs.clear();
    }

    /// `(run_id, epoch)` at which `marker`'s identity-collection contents were
    /// last known, if recorded during an active bumped invocation.
    pub(crate) fn collection_identity_epoch(&self, marker: &str) -> Option<(u64, u64)> {
        self.identity_epochs.get(marker).copied()
    }

    /// Record the `(run_id, epoch)` at which `marker` was freshly constructed.
    pub(crate) fn set_collection_identity_epoch(
        &mut self,
        marker: String,
        run_id: u64,
        epoch: u64,
    ) {
        if epoch == 0 {
            self.identity_epochs.remove(&marker);
        } else {
            self.identity_epochs.insert(marker, (run_id, epoch));
        }
    }

    /// Number of live per-collection identity-epoch entries tracked in this store.
    pub(crate) fn identity_epoch_count(&self) -> usize {
        self.identity_epochs.len()
    }

    pub(crate) fn capacity(&self, marker: &str) -> Option<(usize, bool)> {
        self.capacities.get(marker).copied()
    }

    pub(crate) fn set_capacity(&mut self, marker: String, capacity: usize, vector: bool) {
        self.capacities.insert(marker, (capacity, vector));
    }

    pub(crate) fn get_mut(&mut self, marker: &str) -> Option<&mut Vec<Value>> {
        if self.map.contains_key(marker) {
            self.bump(marker);
        }
        self.map.get_mut(marker)
    }

    pub(crate) fn entry(&mut self, marker: String) -> btree_map::Entry<'_, String, Vec<Value>> {
        self.bump(&marker);
        self.map.entry(marker)
    }

    /// Watch `marker`: a mutable access to it moves [`Self::watched_writes`].
    pub(crate) fn watch(&mut self, marker: &str) {
        self.watched.insert(marker.to_owned());
    }

    /// How many mutable accesses reached a watched collection so far.
    pub(crate) fn watched_writes(&self) -> u64 {
        self.watched_writes
    }

    /// Keep the collections whose marker `keep` accepts, dropping the others whole (the
    /// heap's collector): no kept list changes, so no kept generation moves and no watched
    /// collection is written.
    pub(crate) fn retain_markers(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.map.retain(|marker, _| keep(marker));
        self.watched.retain(|marker| self.map.contains_key(marker));
        self.generations
            .retain(|marker, _| self.map.contains_key(marker));
        self.capacities
            .retain(|marker, _| self.map.contains_key(marker));
        self.identity_epochs
            .retain(|marker, _| self.map.contains_key(marker));
    }
}
