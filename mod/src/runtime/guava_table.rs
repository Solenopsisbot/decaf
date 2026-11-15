//! The small Guava `Table` surface reached by the census.
//!
//! The complete Guava Table family is deliberately not represented here.  The
//! corpus calls the HashBasedTable factories, put/get/contains, row, rowKeySet,
//! cellSet iteration and size.  Those calls use typed host handles and a
//! persistent store.  Every other Table member falls through to the counted
//! host path.

use super::*;
use crate::runtime::host_ref::{HostKind, HostRef};

const TABLE_PREFIX: &str = "table/";
const ROW_PREFIX: &str = "view-row/";
const ROW_KEYS_PREFIX: &str = "view-rowkeys/";
const CELLS_PREFIX: &str = "view-cells/";

#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub(crate) key: Value,
    pub(crate) cells: Vec<(Value, Value)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewKind {
    Row,
    RowKeys,
    Cells,
}

#[derive(Clone, Debug)]
pub(crate) enum State {
    Table {
        rows: Vec<Row>,
        immutable: bool,
        mod_count: u64,
    },
    View {
        table: String,
        kind: ViewKind,
        key: Option<Value>,
    },
}

#[derive(Clone, Debug)]
struct TableIterator {
    table: String,
    expected: u64,
    values: Vec<Value>,
    index: usize,
}

/// Persistent Table state owned by `StdHost`, rather than by one Runtime.
#[derive(Clone, Debug, Default)]
pub struct Store {
    pub(crate) states: BTreeMap<String, State>,
    next: usize,
    iterators: BTreeMap<String, TableIterator>,
    next_iterator: usize,
}

fn npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

fn cme() -> NativeError {
    NativeError::UncaughtException {
        class: "java/util/ConcurrentModificationException".into(),
        message: None,
        frame: None,
    }
}

fn require(value: Option<&Value>) -> Result<&Value, NativeError> {
    let value = value.unwrap_or(&Value::Ref(None));
    if matches!(value, Value::Ref(None)) {
        Err(npe())
    } else {
        Ok(value)
    }
}

fn same(left: &Value, right: &Value) -> bool {
    left == right
}

fn row_index(rows: &[Row], key: &Value) -> Option<usize> {
    rows.iter().position(|row| same(&row.key, key))
}

fn cell_index(row: &Row, key: &Value) -> Option<usize> {
    row.cells.iter().position(|(column, _)| same(column, key))
}

fn table_handle(key: &str) -> Value {
    Value::Host(HostRef::new(HostKind::GuavaTable, key))
}

fn table_key(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::GuavaTable => Some(handle.key()),
        _ => None,
    }
}

fn cell_key(value: &Value) -> Option<&HostRef> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::GuavaCell => Some(handle),
        _ => None,
    }
}

fn value_text(value: &Value) -> String {
    crate::runtime::heap::value_text(value)
        .unwrap_or_default()
        .to_owned()
}

fn new_cell(table: &Value, row: &Value, column: &Value) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::GuavaCell,
        &[table.clone(), row.clone(), column.clone()],
    ))
}

fn cell_parts(value: &Value) -> Option<(Value, Value, Value)> {
    let handle = cell_key(value)?;
    let values = handle.values()?;
    let [table, row, column] = values.as_slice() else {
        return None;
    };
    Some((table.clone(), row.clone(), column.clone()))
}

impl Store {
    fn fresh_key(&mut self, prefix: &str) -> String {
        let key = format!("{prefix}{}", self.next);
        self.next = self.next.saturating_add(1);
        key
    }

    fn table_root(&self, key: &str) -> Option<String> {
        match self.states.get(key)? {
            State::Table { .. } => Some(key.to_owned()),
            State::View { table, .. } => self.table_root(table),
        }
    }

    fn table_state(&self, key: &str) -> Option<(&[Row], bool, u64)> {
        let root = self.table_root(key)?;
        match self.states.get(&root)? {
            State::Table {
                rows,
                immutable,
                mod_count,
            } => Some((rows, *immutable, *mod_count)),
            State::View { .. } => None,
        }
    }

    fn table_state_mut(&mut self, key: &str) -> Option<(&mut Vec<Row>, bool, &mut u64)> {
        let root = self.table_root(key)?;
        match self.states.get_mut(&root)? {
            State::Table {
                rows,
                immutable,
                mod_count,
            } => Some((rows, *immutable, mod_count)),
            State::View { .. } => None,
        }
    }

    fn new_table(&mut self) -> Value {
        let key = self.fresh_key(TABLE_PREFIX);
        self.states.insert(
            key.clone(),
            State::Table {
                rows: Vec::new(),
                immutable: false,
                mod_count: 0,
            },
        );
        table_handle(&key)
    }

    fn new_view(&mut self, table: String, kind: ViewKind, key: Option<Value>) -> Value {
        let prefix = match kind {
            ViewKind::Row => ROW_PREFIX,
            ViewKind::RowKeys => ROW_KEYS_PREFIX,
            ViewKind::Cells => CELLS_PREFIX,
        };
        let view = self.fresh_key(prefix);
        self.states
            .insert(view.clone(), State::View { table, kind, key });
        table_handle(&view)
    }

    fn rows_for(&self, key: &str) -> Option<Vec<(Value, Value, Value)>> {
        let (rows, _, _) = self.table_state(key)?;
        Some(
            rows.iter()
                .flat_map(|row| {
                    row.cells
                        .iter()
                        .map(|(column, value)| (row.key.clone(), column.clone(), value.clone()))
                })
                .collect(),
        )
    }

    fn view_values(&self, key: &str) -> Option<Vec<Value>> {
        let State::View {
            table,
            kind,
            key: view_key,
        } = self.states.get(key)?
        else {
            return None;
        };
        let root = self.table_root(table)?;
        let cells = self.rows_for(&root)?;
        Some(match kind {
            ViewKind::Row => cells
                .into_iter()
                .filter(|(row, _, _)| view_key.as_ref().is_some_and(|wanted| same(row, wanted)))
                .map(|(_, column, value)| map_entry_value(&column, &value))
                .collect(),
            ViewKind::RowKeys => {
                let mut keys = Vec::new();
                for (row, _, _) in cells {
                    if !keys.iter().any(|key| same(key, &row)) {
                        keys.push(row);
                    }
                }
                keys
            }
            ViewKind::Cells => cells
                .into_iter()
                .map(|(row, column, _)| new_cell(&table_handle(&root), &row, &column))
                .collect(),
        })
    }

    fn row_lookup(&self, table: &str, row_key: &Value, column_key: &Value) -> Option<Value> {
        self.rows_for(table)?
            .into_iter()
            .find_map(|(row, column, value)| {
                (same(&row, row_key) && same(&column, column_key)).then_some(value)
            })
    }

    fn insert(
        rows: &mut Vec<Row>,
        row_key: Value,
        column_key: Value,
        value: Value,
    ) -> (Value, bool) {
        let row = match row_index(rows, &row_key) {
            Some(index) => index,
            None => {
                rows.push(Row {
                    key: row_key,
                    cells: Vec::new(),
                });
                rows.len() - 1
            }
        };
        let row_ref = &mut rows[row];
        if let Some(index) = cell_index(row_ref, &column_key) {
            return (std::mem::replace(&mut row_ref.cells[index].1, value), false);
        }
        row_ref.cells.push((column_key, value));
        (Value::Ref(None), true)
    }

    fn remove_row(&mut self, table: &str, row_key: &Value) -> Result<(), NativeError> {
        let Some((rows, immutable, mod_count)) = self.table_state_mut(table) else {
            return Ok(());
        };
        if immutable {
            return Err(NativeError::UncaughtException {
                class: "java/lang/UnsupportedOperationException".into(),
                message: None,
                frame: None,
            });
        }
        let Some(index) = row_index(rows, row_key) else {
            return Ok(());
        };
        rows.remove(index);
        *mod_count = mod_count.saturating_add(1);
        Ok(())
    }

    fn table_put(
        &mut self,
        table: &str,
        row: Value,
        column: Value,
        value: Value,
    ) -> Result<Value, NativeError> {
        require(Some(&row))?;
        require(Some(&column))?;
        require(Some(&value))?;
        let Some((rows, immutable, mod_count)) = self.table_state_mut(table) else {
            return Err(npe());
        };
        if immutable {
            return Err(NativeError::UncaughtException {
                class: "java/lang/UnsupportedOperationException".into(),
                message: None,
                frame: None,
            });
        }
        let (old, structural) = Self::insert(rows, row, column, value);
        if structural {
            *mod_count = mod_count.saturating_add(1);
        }
        Ok(old)
    }

    fn row_put(
        &mut self,
        table: &str,
        row: Value,
        column: Value,
        value: Value,
    ) -> Result<Value, NativeError> {
        self.table_put(table, row, column, value)
    }

    fn view_kind(&self, key: &str) -> Option<(String, ViewKind, Option<Value>)> {
        match self.states.get(key)? {
            State::View { table, kind, key } => Some((table.clone(), *kind, key.clone())),
            State::Table { .. } => None,
        }
    }

    fn invoke_table(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if receiver.is_none() {
            if r.owner == "com/google/common/collect/HashBasedTable"
                && r.name == "create"
                && matches!(
                    r.descriptor.as_str(),
                    "()Lcom/google/common/collect/Table;" | "(II)Lcom/google/common/collect/Table;"
                )
            {
                return Ok(Some(self.new_table()));
            }
            return Ok(None);
        }
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        if let Some((table, row, column)) = cell_parts(receiver) {
            let Some(table) = table_key(&table).map(str::to_owned) else {
                return Ok(None);
            };
            return Ok(match (r.name.as_str(), r.descriptor.as_str()) {
                ("getRowKey", "()Ljava/lang/Object;") => Some(row),
                ("getColumnKey", "()Ljava/lang/Object;") => Some(column),
                ("getValue", "()Ljava/lang/Object;") => Some(
                    self.row_lookup(&table, &row, &column)
                        .unwrap_or(Value::Ref(None)),
                ),
                _ => None,
            });
        }
        let Some(key) = table_key(receiver).map(str::to_owned) else {
            return Ok(None);
        };
        let Some(root) = self.table_root(&key) else {
            return Ok(None);
        };

        if let Some((_table, kind, view_key)) = self.view_kind(&key) {
            match kind {
                ViewKind::RowKeys => match (r.name.as_str(), r.descriptor.as_str()) {
                    ("size", "()I") => {
                        return Ok(Some(Value::Int(
                            self.view_values(&key).unwrap_or_default().len() as i32,
                        )));
                    }
                    ("isEmpty", "()Z") => {
                        return Ok(Some(Value::Int(
                            self.view_values(&key).unwrap_or_default().is_empty() as i32,
                        )));
                    }
                    ("contains", "(Ljava/lang/Object;)Z") => {
                        return Ok(Some(Value::Int(
                            self.view_values(&key)
                                .unwrap_or_default()
                                .iter()
                                .any(|v| args.first().is_some_and(|wanted| same(v, wanted)))
                                as i32,
                        )));
                    }
                    _ => {}
                },
                ViewKind::Cells => match (r.name.as_str(), r.descriptor.as_str()) {
                    ("size", "()I") => {
                        return Ok(Some(Value::Int(
                            self.view_values(&key).unwrap_or_default().len() as i32,
                        )));
                    }
                    ("isEmpty", "()Z") => {
                        return Ok(Some(Value::Int(
                            self.view_values(&key).unwrap_or_default().is_empty() as i32,
                        )));
                    }
                    ("contains", "(Ljava/lang/Object;)Z") => {
                        let wanted = args.first();
                        let found = wanted.and_then(cell_parts).is_some_and(
                            |(table_value, row, column)| {
                                table_key(&table_value).is_some_and(|table_key| table_key == root)
                                    && self.row_lookup(&root, &row, &column).is_some()
                            },
                        );
                        return Ok(Some(Value::Int(found as i32)));
                    }
                    _ => {}
                },
                ViewKind::Row => {
                    let row_key = view_key.clone().ok_or_else(npe)?;
                    match (r.name.as_str(), r.descriptor.as_str()) {
                        ("size", "()I") => {
                            let size = self
                                .rows_for(&root)
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|(row, _, _)| same(row, &row_key))
                                .count();
                            return Ok(Some(Value::Int(size as i32)));
                        }
                        ("isEmpty", "()Z") => {
                            let empty = self
                                .rows_for(&root)
                                .unwrap_or_default()
                                .into_iter()
                                .all(|(row, _, _)| !same(&row, &row_key));
                            return Ok(Some(Value::Int(empty as i32)));
                        }
                        ("get", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
                            return Ok(Some(
                                self.row_lookup(
                                    &root,
                                    &row_key,
                                    args.first().unwrap_or(&Value::Ref(None)),
                                )
                                .unwrap_or(Value::Ref(None)),
                            ));
                        }
                        ("containsKey", "(Ljava/lang/Object;)Z") => {
                            return Ok(Some(Value::Int(
                                self.row_lookup(
                                    &root,
                                    &row_key,
                                    args.first().unwrap_or(&Value::Ref(None)),
                                )
                                .is_some() as i32,
                            )));
                        }
                        ("put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
                            let column = args.first().cloned().unwrap_or(Value::Ref(None));
                            let value = args.get(1).cloned().unwrap_or(Value::Ref(None));
                            return Ok(Some(self.row_put(&root, row_key, column, value)?));
                        }
                        ("remove", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
                            let column = args.first().unwrap_or(&Value::Ref(None));
                            let value = self
                                .row_lookup(&root, &row_key, column)
                                .unwrap_or(Value::Ref(None));
                            if value != Value::Ref(None) {
                                let Some((rows, immutable, mod_count)) =
                                    self.table_state_mut(&root)
                                else {
                                    return Ok(None);
                                };
                                if immutable {
                                    return Err(NativeError::UncaughtException {
                                        class: "java/lang/UnsupportedOperationException".into(),
                                        message: None,
                                        frame: None,
                                    });
                                }
                                if let Some(index) = row_index(rows, &row_key) {
                                    let row = &mut rows[index];
                                    if let Some(cell) = cell_index(row, column) {
                                        row.cells.remove(cell);
                                    }
                                    if row.cells.is_empty() {
                                        rows.remove(index);
                                    }
                                    *mod_count = mod_count.saturating_add(1);
                                }
                            }
                            return Ok(Some(value));
                        }
                        ("clear", "()V") => {
                            self.remove_row(&root, &row_key)?;
                            return Ok(Some(Value::Void));
                        }
                        _ => {}
                    }
                }
            }
            if r.name == "iterator" && r.descriptor == "()Ljava/util/Iterator;" {
                return Ok(Some(self.new_iterator(&key)?));
            }
            return Ok(None);
        }

        match (r.name.as_str(), r.descriptor.as_str()) {
            (
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            ) => {
                let row = args.first().cloned().unwrap_or(Value::Ref(None));
                let column = args.get(1).cloned().unwrap_or(Value::Ref(None));
                let value = args.get(2).cloned().unwrap_or(Value::Ref(None));
                Ok(Some(self.table_put(&root, row, column, value)?))
            }
            ("get", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => Ok(Some(
                self.row_lookup(
                    &root,
                    args.first().unwrap_or(&Value::Ref(None)),
                    args.get(1).unwrap_or(&Value::Ref(None)),
                )
                .unwrap_or(Value::Ref(None)),
            )),
            ("contains", "(Ljava/lang/Object;Ljava/lang/Object;)Z") => Ok(Some(Value::Int(
                self.row_lookup(
                    &root,
                    args.first().unwrap_or(&Value::Ref(None)),
                    args.get(1).unwrap_or(&Value::Ref(None)),
                )
                .is_some() as i32,
            ))),
            ("size", "()I") => Ok(Some(Value::Int(
                self.rows_for(&root).unwrap_or_default().len() as i32,
            ))),
            ("row", "(Ljava/lang/Object;)Ljava/util/Map;") => {
                let row = require(args.first())?.clone();
                Ok(Some(self.new_view(root, ViewKind::Row, Some(row))))
            }
            ("rowKeySet", "()Ljava/util/Set;") => {
                Ok(Some(self.new_view(root, ViewKind::RowKeys, None)))
            }
            ("cellSet", "()Ljava/util/Set;") => {
                Ok(Some(self.new_view(root, ViewKind::Cells, None)))
            }
            _ => Ok(None),
        }
    }

    fn new_iterator(&mut self, view: &str) -> Result<Value, NativeError> {
        let root = self.table_root(view).ok_or_else(npe)?;
        let expected = self
            .table_state(&root)
            .map(|(_, _, count)| count)
            .unwrap_or(0);
        let values = self.view_values(view).unwrap_or_default();
        let key = format!("iterator/guava{}", self.next_iterator);
        self.next_iterator = self.next_iterator.saturating_add(1);
        self.iterators.insert(
            key.clone(),
            TableIterator {
                table: root,
                expected,
                values,
                index: 0,
            },
        );
        Ok(host_iterator::iterator(key))
    }

    pub(crate) fn host_invoke(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        _args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if let Some(key) = table_key(receiver)
            && r.name == "iterator"
            && r.descriptor == "()Ljava/util/Iterator;"
            && self.view_kind(key).is_some()
        {
            return Ok(Some(self.new_iterator(key)?));
        }
        let Value::Host(handle) = receiver else {
            return Ok(None);
        };
        if handle.kind() != HostKind::Iterator {
            return Ok(None);
        }
        let Some(snapshot) = self.iterators.get(handle.key()).cloned() else {
            return Ok(None);
        };
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("hasNext", "()Z") => Ok(Some(Value::Int(
                (snapshot.index < snapshot.values.len()) as i32,
            ))),
            ("next", "()Ljava/lang/Object;") => {
                let current = self
                    .table_state(&snapshot.table)
                    .map(|(_, _, count)| count)
                    .unwrap_or(snapshot.expected);
                if current != snapshot.expected {
                    return Err(cme());
                }
                let value = snapshot
                    .values
                    .get(snapshot.index)
                    .cloned()
                    .ok_or_else(|| NativeError::UncaughtException {
                        class: "java/util/NoSuchElementException".into(),
                        message: None,
                        frame: None,
                    })?;
                if let Some(iterator) = self.iterators.get_mut(handle.key()) {
                    iterator.index += 1;
                }
                Ok(Some(value))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn collection_values(&self, value: &Value) -> Option<Vec<Value>> {
        let key = table_key(value)?;
        self.view_values(key)
    }

    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        fn add<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
            if let Some(text) = crate::runtime::heap::value_text(value) {
                out.push(text);
            }
        }
        for state in self.states.values() {
            match state {
                State::Table { rows, .. } => {
                    for row in rows {
                        add(&row.key, out);
                        for (column, value) in &row.cells {
                            add(column, out);
                            add(value, out);
                        }
                    }
                }
                State::View { key, .. } => {
                    if let Some(key) = key {
                        add(key, out);
                    }
                }
            }
        }
        for iterator in self.iterators.values() {
            for value in &iterator.values {
                add(value, out);
            }
        }
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.states.len() + self.iterators.len()
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        self.states
            .values()
            .map(|state| match state {
                State::Table { rows, .. } => rows
                    .iter()
                    .map(|row| {
                        row.cells
                            .iter()
                            .map(|(c, v)| value_text(c).len() + value_text(v).len())
                            .sum::<usize>()
                            + value_text(&row.key).len()
                    })
                    .sum(),
                State::View { key, .. } => key.as_ref().map_or(0, |key| value_text(key).len()),
            })
            .sum::<usize>()
            + self
                .iterators
                .values()
                .map(|it| it.values.iter().map(|v| value_text(v).len()).sum::<usize>())
                .sum::<usize>()
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The narrow Table surface; a host without a persistent Table store leaves
    /// every call unmodelled so the normal counted path handles it.
    pub(super) fn invoke_guava_table(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(store) = self.host.guava_table_store() else {
            return Ok(None);
        };
        store.invoke_table(r, receiver, args)
    }
}
