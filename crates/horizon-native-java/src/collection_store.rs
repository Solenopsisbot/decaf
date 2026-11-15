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
    generations: HashMap<String, u64>,
    next: u64,
    /// The collections someone mirrors elsewhere (an `Arrays.asList` view's
    /// array, `array_views.rs`), and how many mutable accesses reached one:
    /// while that count stands still, no mirror needs a look.
    watched: HashSet<String>,
    watched_writes: u64,
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
        self.map.remove(marker)
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
    }
}
