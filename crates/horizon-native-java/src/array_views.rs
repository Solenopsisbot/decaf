//! `Arrays.asList`: a fixed-size `java.util.Arrays$ArrayList` view over its
//! array (#156 P14d2).
//!
//! It used to answer the text literal `list/a,b`: a copy (a write to the
//! array or through the list never reached the other), as growable as an
//! `ArrayList`, and equal to every other literal with the same text.
//!
//! The view is now a collection handle of class `Arrays$ArrayList`, one per
//! call (two views of one array are two objects that share it). Its
//! `subList` is a window on the same array (`AbstractList$RandomAccessSubList`),
//! and a window's `subList` another. Each keeps a row in the host's
//! collection table that mirrors its part of the array, which lives in the
//! runtime during a call, so every host reader (`toString`, `equals`, a copy,
//! a list holding it) sees the array as it is:
//!
//! - **array → rows:** after the runtime writes a viewed array (`aastore`,
//!   `System.arraycopy`, `Array.set`, a `toArray(T[])` fill), it pushes the
//!   array into every view and window of it, and into the snapshot of every
//!   iterator over one, so iteration reads the array live as `ArrayItr` does
//!   ([`StdHost::array_written`]);
//! - **rows → array:** the collection table watches the rows. Before the
//!   runtime reads or writes an array or makes a call, it pulls each row
//!   written since its last sync into the array, then pushes the array to
//!   the others ([`StdHost::changed_array_views`]). While no row was written,
//!   this is one counter comparison.
//!
//! A view's or window's own members run here, before any other model
//! ([`Runtime::invoke_array_view`]): `get`/`set` on the live array with the
//! array's bounds and store checks, `forEach` reading it live, `subList`, and
//! every member that would change the size, through one guard that answers as
//! JDK 25's `AbstractList`/`AbstractCollection`/`AbstractList.SubList` do. A
//! size-changing member a model sends the host directly is counted there
//! ([`StdHost::fixed_size_member`]).

use std::collections::{BTreeMap, BTreeSet};

use crate::array_data::ArrayData;
use crate::{MethodRef, NativeError, StdHost, Value, expect_ref};

/// The class Java 25's `Arrays.asList` answers.
pub(crate) const CLASS: &str = "java/util/Arrays$ArrayList";

/// The class of its `subList` (`AbstractList.subList` on a `RandomAccess` list).
pub(crate) const WINDOW_CLASS: &str = "java/util/AbstractList$RandomAccessSubList";

/// One view or window: where its elements start in the array, its row's
/// generation when it last matched the array, and whether it is a `subList`.
#[derive(Clone, Debug)]
struct View {
    array: String,
    offset: usize,
    synced: u64,
    window: bool,
}

/// Which arrays the host's views mirror.
#[derive(Clone, Debug, Default)]
pub(crate) struct ArrayViews {
    views: BTreeMap<String, View>,
    /// An array's key → its views and windows.
    by_array: BTreeMap<String, BTreeSet<String>>,
    /// The collection table's watched writes at the last pull.
    pulled: u64,
    /// An iterator over a view or window → that view (`ArrayItr` reads the
    /// array, so its snapshot follows it).
    iterators: BTreeMap<String, String>,
}

fn throw(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

fn unsupported(message: Option<&str>) -> NativeError {
    throw(
        "java/lang/UnsupportedOperationException",
        message.map(str::to_owned),
    )
}

impl StdHost {
    /// A new view (`window`: a `subList`) of `array` from `offset`, holding
    /// its `values` now.
    pub(crate) fn new_array_view(
        &mut self,
        array: &str,
        offset: usize,
        values: Vec<Value>,
        window: bool,
    ) -> Value {
        let view = self.new_collection(if window { WINDOW_CLASS } else { CLASS }, values);
        if let Some(marker) = expect_ref(&view).cloned() {
            self.mutable_collections.watch(&marker);
            let synced = self.mutable_collections.generation(&marker);
            self.array_views
                .by_array
                .entry(array.to_owned())
                .or_default()
                .insert(marker.clone());
            let view = View {
                array: array.to_owned(),
                offset,
                synced,
                window,
            };
            self.array_views.views.insert(marker, view);
        }
        view
    }

    /// Note that `iterator` iterates `view`, when it is one.
    pub(crate) fn note_array_view_iterator(&mut self, iterator: &str, view: &str) {
        if self.array_views.views.contains_key(view) {
            self.array_views
                .iterators
                .insert(iterator.to_owned(), view.to_owned());
        }
    }

    /// Forget the views whose rows a collection dropped.
    pub(crate) fn prune_array_views(&mut self) {
        let rows = &self.mutable_collections;
        let views = &mut self.array_views;
        let iterators = &self.iterators;
        views.views.retain(|view, _| rows.contains_key(view));
        views
            .iterators
            .retain(|iterator, view| iterators.contains_key(iterator) && rows.contains_key(view));
        views.by_array.retain(|_, of| {
            of.retain(|view| rows.contains_key(view));
            !of.is_empty()
        });
    }

    /// The array the view `marker` mirrors (an edge for the collector:
    /// the view's `get`/`set` work on it).
    pub(crate) fn array_view_array(&self, marker: &str) -> Option<&String> {
        self.array_views.views.get(marker).map(|view| &view.array)
    }

    /// Every view and window, and every iterator over one.
    pub(crate) fn array_view_markers(&self) -> impl Iterator<Item = &String> {
        self.array_views
            .views
            .keys()
            .chain(self.array_views.iterators.keys())
    }

    /// The view an iterator over one reads (an edge for the collector: the
    /// iterator follows the view's array while it lives).
    pub(crate) fn array_view_of_iterator(&self, marker: &str) -> Option<&String> {
        self.array_views.iterators.get(marker)
    }

    pub(crate) fn has_array_views_of(&self, array: &str) -> bool {
        self.array_views.by_array.contains_key(array)
    }

    /// The array `marker` views, where its part starts, how long it is, and
    /// whether it is a `subList`.
    pub(crate) fn array_view_of(&self, marker: &str) -> Option<(String, usize, usize, bool)> {
        let view = self.array_views.views.get(marker)?;
        let length = self.mutable_collections.get(marker)?.len();
        Some((view.array.clone(), view.offset, length, view.window))
    }

    /// The runtime wrote the array `array`, which now holds `values`: every
    /// view and window of it holds its part, and so does every iterator
    /// over one (`ArrayItr` reads the array).
    pub(crate) fn array_written(&mut self, array: &str, values: &[Value]) {
        let Some(views) = self.array_views.by_array.get(array).cloned() else {
            return;
        };
        let current = self.array_views.pulled == self.mutable_collections.watched_writes();
        for marker in views {
            let Some(offset) = self.array_views.views.get(&marker).map(|view| view.offset) else {
                continue;
            };
            let mut part = None;
            if let Some(row) = self.mutable_collections.get_mut(&marker)
                && let Some(now) = values.get(offset..offset + row.len())
            {
                row.clone_from_slice(now);
                part = Some(now.to_vec());
            }
            if let Some(part) = part {
                for (iterator, view) in &self.array_views.iterators {
                    if *view == marker
                        && let Some((snapshot, _)) = self.iterators.get_mut(iterator)
                        && snapshot.len() == part.len()
                    {
                        snapshot.clone_from(&part);
                    }
                }
            }
            let synced = self.mutable_collections.generation(&marker);
            if let Some(view) = self.array_views.views.get_mut(&marker) {
                view.synced = synced;
            }
        }
        // What this push wrote needs no pull.
        if current {
            self.array_views.pulled = self.mutable_collections.watched_writes();
        }
    }

    /// Each view or window written since it last matched its array: the
    /// array, where the part starts, and the part now. Empty, at the cost of
    /// one comparison, while none was written.
    pub(crate) fn changed_array_views(&mut self) -> Vec<(String, usize, Vec<Value>)> {
        if self.array_views.pulled == self.mutable_collections.watched_writes() {
            return Vec::new();
        }
        let changed = self
            .array_views
            .views
            .iter()
            .filter(|(marker, view)| self.mutable_collections.generation(marker) != view.synced)
            .filter_map(|(marker, view)| {
                let row = self.mutable_collections.get(marker)?;
                Some((view.array.clone(), view.offset, row.clone()))
            })
            .collect();
        self.array_views.pulled = self.mutable_collections.watched_writes();
        changed
    }

    /// A member a model sends the host directly that could change a view's
    /// size or store into its array without the array's checks: counted (a
    /// jar's own calls are answered by [`crate::Runtime::invoke_array_view`]
    /// before any model).
    pub(crate) fn fixed_size_member(
        &mut self,
        r: &MethodRef,
        marker: &str,
    ) -> Option<Result<Value, NativeError>> {
        if !self.array_views.views.contains_key(marker) {
            return None;
        }
        let writes = matches!(
            r.name.as_str(),
            "add"
                | "addAll"
                | "addFirst"
                | "addLast"
                | "remove"
                | "removeAll"
                | "retainAll"
                | "removeIf"
                | "removeFirst"
                | "removeLast"
                | "clear"
                | "set"
        );
        writes.then(|| crate::Host::unknown_operand_call(self, r))
    }
}

impl<H: crate::Host> crate::Runtime<'_, H> {
    pub(crate) fn reverse_view(&mut self, value: &Value) -> bool {
        let Some(marker) = crate::expect_ref(value) else { return false; };
        let Some((array, offset, length, _)) = self.host.array_view_of(marker) else { return false; };
        let Some(slots) = self.arrays.get_mut(&array).and_then(ArrayData::refs_mut) else { return false; };
        if let Some(part) = slots.get_mut(offset..offset + length) { part.reverse(); self.push_array_views(&array); true } else { false }
    }
    /// Bring every array a view or window was written through up to date
    /// ([`StdHost::changed_array_views`]); one comparison while none was.
    pub(crate) fn pull_array_views(&mut self) {
        for (array, offset, values) in self.host.changed_array_views() {
            // A view is only ever of a reference array (`Arrays.asList(T...)`).
            if let Some(slots) = self.arrays.get_mut(&array).and_then(ArrayData::refs_mut)
                && let Some(part) = slots.get_mut(offset..offset + values.len())
            {
                part.clone_from_slice(&values);
            }
            self.push_array_views(&array);
        }
    }

    /// The runtime wrote the array `key`: its views hold what it holds now.
    pub(crate) fn push_array_views(&mut self, key: &str) {
        if self.host.has_array_views(key)
            && let Some(values) = self
                .arrays
                .get(key)
                .map(|array| array.values().into_owned())
        {
            self.host.array_written(key, &values);
        }
    }

    /// `Arrays.asList(array)`: a view of an array this runtime owns, an
    /// empty one of the shared empty array, else counted (an array an
    /// unmodelled call made has no elements to show).
    pub(crate) fn as_list(&mut self, r: &MethodRef, array: &Value) -> Result<Value, NativeError> {
        let Some(key) = crate::host_array::array_key(array) else {
            return self.host.unknown_operand_call(r);
        };
        let values = if crate::host_array::is_shared_empty(key) {
            Vec::new()
        } else {
            match self.arrays.get(key) {
                Some(values) => values.values().into_owned(),
                None => return self.host.unknown_operand_call(r),
            }
        };
        let key = key.to_owned();
        match self.host.array_view(&key, 0, values, false) {
            Some(view) => Ok(view),
            None => self.host.unknown_operand_call(r),
        }
    }

    /// A view's or window's own members, answered on its array (see the
    /// module doc); `None` for any other receiver or member.
    pub(crate) fn invoke_array_view(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some((key, offset, length, window)) = receiver
            .and_then(expect_ref)
            .and_then(|marker| self.host.array_view_of(marker))
        else {
            return Ok(None);
        };
        self.pull_array_views();
        let elements = |this: &Self| -> Vec<Value> {
            this.arrays
                .get(&key)
                .and_then(|array| array.references().get(offset..offset + length))
                .map(<[Value]>::to_vec)
                .unwrap_or_default()
        };
        // `a[i]` on a view (`ArrayIndexOutOfBoundsException`), `Objects.checkIndex`
        // on a window; both say `Index i out of bounds for length n`.
        let slot = |index: i32| -> Result<usize, NativeError> {
            usize::try_from(index)
                .ok()
                .filter(|index| *index < length)
                .map(|index| offset + index)
                .ok_or_else(|| {
                    throw(
                        if window {
                            "java/lang/IndexOutOfBoundsException"
                        } else {
                            "java/lang/ArrayIndexOutOfBoundsException"
                        },
                        Some(format!("Index {index} out of bounds for length {length}")),
                    )
                })
        };
        // `AbstractList.rangeCheckForAdd`, before an insertion refuses.
        let insertion = |index: i32| -> Result<(), NativeError> {
            if (0..=length as i64).contains(&i64::from(index)) {
                Ok(())
            } else {
                Err(throw(
                    "java/lang/IndexOutOfBoundsException",
                    Some(format!("Index: {index}, Size: {length}")),
                ))
            }
        };
        let null = || throw("java/lang/NullPointerException", None);
        // A view's iterator is `ArrayItr`, whose `remove` says "remove"; a
        // window's reaches the root's `remove(int)`, which says nothing.
        let iterator_remove = if window { None } else { Some("remove") };
        let int = |value: &Value| crate::expect_int(r, value);
        let answer = match (r.name.as_str(), r.descriptor.as_str(), args) {
            ("get", "(I)Ljava/lang/Object;", [index]) => {
                let at = slot(int(index)?)?;
                self.arrays
                    .get(&key)
                    .and_then(|array| array.references().get(at))
                    .cloned()
            }
            // `E old = a[index]; a[index] = element;`: the bounds, then the
            // array's store check (`aastore`'s).
            ("set", "(ILjava/lang/Object;)Ljava/lang/Object;", [index, value]) => {
                let at = slot(int(index)?)?;
                let array = crate::host_array::handle(key.clone());
                self.check_array_store(&array, value)?;
                let old = self
                    .arrays
                    .get_mut(&key)
                    .and_then(|array| array.refs_mut()?.get_mut(at))
                    .map(|element| std::mem::replace(element, value.clone()));
                self.push_array_views(&key);
                old
            }
            // `for (E e : a) action.accept(e)`: each element as it is when reached.
            ("forEach", "(Ljava/util/function/Consumer;)V", [action]) => {
                if *action == Value::Ref(None) {
                    return Err(null());
                }
                for index in 0..length {
                    self.pull_array_views();
                    let element = self
                        .arrays
                        .get(&key)
                        .and_then(|array| array.references().get(offset + index))
                        .cloned()
                        .unwrap_or(Value::Ref(None));
                    self.run_functional(
                        crate::collection_callback::ACCEPT_OBJECT,
                        action,
                        &[element],
                    )?;
                }
                Some(Value::Void)
            }
            // `AbstractList.subListRangeCheck`, then a window on the same array.
            ("subList", "(II)Ljava/util/List;", [from, to]) => {
                let (from, to) = (int(from)?, int(to)?);
                if from < 0 {
                    return Err(throw(
                        "java/lang/IndexOutOfBoundsException",
                        Some(format!("fromIndex = {from}")),
                    ));
                }
                if to as i64 > length as i64 {
                    return Err(throw(
                        "java/lang/IndexOutOfBoundsException",
                        Some(format!("toIndex = {to}")),
                    ));
                }
                if from > to {
                    return Err(throw(
                        "java/lang/IllegalArgumentException",
                        Some(format!("fromIndex({from}) > toIndex({to})")),
                    ));
                }
                let (from, to) = (from as usize, to as usize);
                let part = elements(self)[from..to].to_vec();
                match self.host.array_view(&key, offset + from, part, true) {
                    Some(window) => Some(window),
                    None => Some(self.host.unknown_operand_call(r)?),
                }
            }
            _ => self.fixed_size(r, args, &elements(self), window, iterator_remove, insertion)?,
        };
        Ok(answer)
    }

    /// The one guard of a view's or window's size: what JDK 25 answers for a
    /// member that would add or remove (`AbstractList`, `AbstractCollection`,
    /// `AbstractList.SubList`). It throws `UnsupportedOperationException` at
    /// the point Java reaches `add`/`remove`, and answers "nothing changed"
    /// when Java finds nothing to do; a comparison it cannot make exactly is
    /// counted. `None` for a member that changes no size.
    fn fixed_size(
        &mut self,
        r: &MethodRef,
        args: &[Value],
        elements: &[Value],
        window: bool,
        iterator_remove: Option<&str>,
        insertion: impl Fn(i32) -> Result<(), NativeError>,
    ) -> Result<Option<Value>, NativeError> {
        let null = || throw("java/lang/NullPointerException", None);
        let empty = elements.is_empty();
        let int = |value: &Value| crate::expect_int(r, value);
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str(), args) {
            // `add(e)` is `add(size(), e)`; a window checks the index first.
            ("add", "(Ljava/lang/Object;)Z", _) | ("addLast", _, _) => {
                return Err(unsupported(None));
            }
            ("add", "(ILjava/lang/Object;)V", [index, _]) => {
                if window {
                    insertion(int(index)?)?;
                }
                return Err(unsupported(None));
            }
            ("addFirst", _, _) => return Err(unsupported(None)),
            ("remove", "(I)Ljava/lang/Object;", [index]) => {
                let index = int(index)?;
                if window && !(0..elements.len() as i64).contains(&i64::from(index)) {
                    return Err(throw(
                        "java/lang/IndexOutOfBoundsException",
                        Some(format!(
                            "Index {index} out of bounds for length {}",
                            elements.len()
                        )),
                    ));
                }
                return Err(unsupported(None));
            }
            // `List.removeFirst`/`removeLast`: an empty list throws first.
            ("removeFirst" | "removeLast", "()Ljava/lang/Object;", []) => {
                return Err(if empty {
                    throw("java/util/NoSuchElementException", None)
                } else {
                    unsupported(None)
                });
            }
            // Each of `c`'s elements is added: nothing to add is `false`.
            ("addAll", "(Ljava/util/Collection;)Z", [added])
            | ("addAll", "(ILjava/util/Collection;)Z", [_, added]) => {
                if let [index, _] = args {
                    insertion(int(index)?)?;
                }
                if *added == Value::Ref(None) {
                    return Err(null());
                }
                match self.collection_elements(added) {
                    Ok(added) if added.is_empty() => Value::Int(0),
                    Ok(_) => return Err(unsupported(None)),
                    Err(_) => self.host.unknown_operand_call(r)?,
                }
            }
            // `removeRange(0, size())`: its iterator's `remove` per element.
            ("clear", "()V", []) => {
                if empty {
                    Value::Void
                } else {
                    return Err(unsupported(None));
                }
            }
            // The iterator's `remove` at the first `o.equals(e)`.
            ("remove", "(Ljava/lang/Object;)Z", [needle]) => {
                if self.abstract_list_index_of(elements, needle)? >= 0 {
                    return Err(unsupported(iterator_remove));
                }
                Value::Int(0)
            }
            // At the first element `c` contains (`removeAll`) or lacks
            // (`retainAll`).
            (name @ ("removeAll" | "retainAll"), "(Ljava/util/Collection;)Z", [other]) => {
                if *other == Value::Ref(None) {
                    return Err(null());
                }
                for element in elements {
                    match self.argument_contains(other, element)? {
                        Some(contained) if contained == (name == "removeAll") => {
                            return Err(unsupported(iterator_remove));
                        }
                        Some(_) => {}
                        None => return self.host.unknown_operand_call(r).map(Some),
                    }
                }
                Value::Int(0)
            }
            // Each element's test in order, the iterator's `remove` at the
            // first that passes.
            ("removeIf", "(Ljava/util/function/Predicate;)Z", [predicate]) => {
                if *predicate == Value::Ref(None) {
                    return Err(null());
                }
                for element in elements {
                    if self.predicate_holds(predicate, element)? {
                        return Err(unsupported(iterator_remove));
                    }
                }
                Value::Int(0)
            }
            _ => return Ok(None),
        }))
    }
}
