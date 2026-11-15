//! Guava's `Table` family.
//!
//! Guava 33.6 stores a table as rows of insertion ordered maps.  This module
//! keeps that same observable shape for the bounded interpreter: factories
//! make an owned table, row/column views write through to it, and immutable
//! tables reject every mutator.  A table is runtime state, so its marker is
//! never a neutral reference that can become `null` at the next call.

use super::*;

const TABLE: &str = "guava-table/";
const BUILDER: &str = "guava-table-builder/";
const VIEW: &str = "guava-table-view/";

#[derive(Clone, Debug)]
pub(crate) struct Row {
    key: Value,
    cells: Vec<(Value, Value)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewKind {
    Row,
    Column,
    RowMap,
    ColumnMap,
}

#[derive(Clone, Debug)]
pub(crate) enum State {
    Table {
        rows: Vec<Row>,
        immutable: bool,
        tree: bool,
    },
    Builder {
        rows: Vec<Row>,
        tree: bool,
    },
    View {
        table: String,
        kind: ViewKind,
        key: Option<Value>,
    },
    Wrapper {
        table: String,
    },
}

fn marker(value: &Value) -> Option<&str> {
    match value {
        Value::Ref(Some(value)) if is_table_marker(value) => Some(value),
        _ => None,
    }
}

fn is_table_marker(value: &str) -> bool {
    value.starts_with(TABLE) || value.starts_with(BUILDER) || value.starts_with(VIEW)
}

pub(crate) fn is_view(value: &Value) -> bool {
    marker(value).is_some_and(|value| value.starts_with(VIEW))
}

fn null() -> Value {
    Value::Ref(None)
}

fn npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

fn unsupported() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/UnsupportedOperationException".into(),
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

fn value_key(value: &Value) -> String {
    value_text(value)
}

fn compare_rows(left: &Row, right: &Row) -> std::cmp::Ordering {
    value_key(&left.key).cmp(&value_key(&right.key))
}

impl<H: Host> Runtime<'_, H> {
    fn next_guava_marker(&mut self, prefix: &str) -> String {
        let marker = format!("{prefix}{}", self.next_guava_table);
        self.next_guava_table = self.next_guava_table.saturating_add(1);
        marker
    }

    fn new_guava_table(&mut self, immutable: bool, tree: bool, rows: Vec<Row>) -> Value {
        let marker = self.next_guava_marker(TABLE);
        self.guava_tables.insert(
            marker.clone(),
            State::Table {
                rows,
                immutable,
                tree,
            },
        );
        Value::Ref(Some(marker))
    }

    fn new_guava_view(&mut self, table: String, kind: ViewKind, key: Option<Value>) -> Value {
        let marker = self.next_guava_marker(VIEW);
        self.guava_tables
            .insert(marker.clone(), State::View { table, kind, key });
        Value::Ref(Some(marker))
    }

    fn new_guava_builder(&mut self, tree: bool) -> Value {
        let marker = self.next_guava_marker(BUILDER);
        self.guava_tables.insert(
            marker.clone(),
            State::Builder {
                rows: Vec::new(),
                tree,
            },
        );
        Value::Ref(Some(marker))
    }

    fn table_root(&self, marker: &str) -> Option<String> {
        match self.guava_tables.get(marker)? {
            State::Table { .. } | State::Builder { .. } => Some(marker.to_owned()),
            State::Wrapper { table } => Some(table.clone()),
            State::View { table, .. } => self.table_root(table),
        }
    }

    fn table_rows(&self, marker: &str) -> Option<&[Row]> {
        let root = self.table_root(marker)?;
        match self.guava_tables.get(&root)? {
            State::Table { rows, .. } | State::Builder { rows, .. } => Some(rows),
            _ => None,
        }
    }

    fn table_mut(&mut self, marker: &str) -> Option<(&mut Vec<Row>, bool, bool)> {
        let root = self.table_root(marker)?;
        match self.guava_tables.get_mut(&root)? {
            State::Table {
                rows,
                immutable,
                tree,
            } => Some((rows, *immutable, *tree)),
            State::Builder { rows, tree } => Some((rows, false, *tree)),
            _ => None,
        }
    }

    fn insert_cell(
        rows: &mut Vec<Row>,
        row_key: Value,
        column_key: Value,
        value: Value,
        tree: bool,
    ) -> Value {
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
            return std::mem::replace(&mut row_ref.cells[index].1, value);
        }
        row_ref.cells.push((column_key, value));
        if tree {
            row_ref.cells.sort_by_key(|left| value_key(&left.0));
            rows.sort_by(compare_rows);
        }
        null()
    }

    fn remove_cell(rows: &mut Vec<Row>, row_key: &Value, column_key: &Value) -> Value {
        let Some(row_index) = row_index(rows, row_key) else {
            return null();
        };
        let row = &mut rows[row_index];
        let Some(cell) = cell_index(row, column_key) else {
            return null();
        };
        let value = row.cells.remove(cell).1;
        if row.cells.is_empty() {
            rows.remove(row_index);
        }
        value
    }

    fn cell_values(&self, marker: &str) -> Vec<(Value, Value, Value)> {
        let Some(rows) = self.table_rows(marker) else {
            return Vec::new();
        };
        rows.iter()
            .flat_map(|row| {
                row.cells
                    .iter()
                    .map(|(column, value)| (row.key.clone(), column.clone(), value.clone()))
            })
            .collect()
    }

    fn view_values(&self, marker: &str) -> Vec<Value> {
        let Some(State::View { table, kind, key }) = self.guava_tables.get(marker) else {
            return Vec::new();
        };
        let cells = self.cell_values(table);
        match kind {
            ViewKind::Row => cells
                .into_iter()
                .filter(|(row, _, _)| key.as_ref().is_some_and(|key| same(row, key)))
                .map(|(_, column, value)| map_entry_value(&column, &value))
                .collect(),
            ViewKind::Column => cells
                .into_iter()
                .filter(|(_, column, _)| key.as_ref().is_some_and(|key| same(column, key)))
                .map(|(row, _, value)| map_entry_value(&row, &value))
                .collect(),
            ViewKind::RowMap | ViewKind::ColumnMap => self
                .table_rows(table)
                .into_iter()
                .flat_map(|rows| rows.iter().map(|row| row.key.clone()))
                .collect(),
        }
    }

    fn invoke_view(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(view_marker) = marker(receiver).map(str::to_owned) else {
            return Ok(None);
        };
        let Some(State::View { table, kind, key }) = self.guava_tables.get(&view_marker).cloned()
        else {
            return Ok(None);
        };
        let is_map = matches!(
            kind,
            ViewKind::Row | ViewKind::Column | ViewKind::RowMap | ViewKind::ColumnMap
        );
        let values = self.view_values(&view_marker);
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("size", "()I") => return Ok(Some(Value::Int(values.len() as i32))),
            ("isEmpty", "()Z") => return Ok(Some(Value::Int(values.is_empty() as i32))),
            ("iterator", "()Ljava/util/Iterator;") => return Ok(None),
            ("stream", "()Ljava/util/stream/Stream;") => {
                return Ok(Some(collection_value("list", &values)));
            }
            ("clear", "()V") => {
                if matches!(kind, ViewKind::Row | ViewKind::Column) {
                    let cells = self.cell_values(&table);
                    let Some((rows, immutable, _)) = self.table_mut(&table) else {
                        return Ok(None);
                    };
                    if immutable {
                        return Err(unsupported());
                    }
                    for (row, column, _) in cells {
                        let selected = match kind {
                            ViewKind::Row => key.as_ref().is_some_and(|key| same(&row, key)),
                            ViewKind::Column => key.as_ref().is_some_and(|key| same(&column, key)),
                            _ => false,
                        };
                        if selected {
                            Self::remove_cell(rows, &row, &column);
                        }
                    }
                    return Ok(Some(Value::Void));
                }
                return Ok(None);
            }
            ("contains", "(Ljava/lang/Object;)Z") => {
                let wanted = require(args.first()).cloned()?;
                return Ok(Some(Value::Int(values.iter().any(|value| {
                    if let Ok((left, right)) = map_entry_parts(value) {
                        same(&left, &wanted) || same(&right, &wanted)
                    } else {
                        same(value, &wanted)
                    }
                }) as i32)));
            }
            ("keySet", "()Ljava/util/Set;") => {
                if matches!(kind, ViewKind::RowMap | ViewKind::ColumnMap) {
                    return Ok(Some(collection_value("set", &values)));
                }
                let keys = values
                    .iter()
                    .filter_map(|value| map_entry_parts(value).ok().map(|(key, _)| key))
                    .collect::<Vec<_>>();
                return Ok(Some(collection_value("set", &keys)));
            }
            ("values", "()Ljava/util/Collection;") => {
                if matches!(kind, ViewKind::RowMap | ViewKind::ColumnMap) {
                    let child_kind = if kind == ViewKind::RowMap {
                        ViewKind::Row
                    } else {
                        ViewKind::Column
                    };
                    let children = values
                        .iter()
                        .cloned()
                        .map(|key| self.new_guava_view(table.clone(), child_kind, Some(key)))
                        .collect::<Vec<_>>();
                    return Ok(Some(collection_value("list", &children)));
                }
                let values = values
                    .iter()
                    .filter_map(|value| map_entry_parts(value).ok().map(|(_, value)| value))
                    .collect::<Vec<_>>();
                return Ok(Some(collection_value("list", &values)));
            }
            ("entrySet", "()Ljava/util/Set;") => {
                if matches!(kind, ViewKind::RowMap | ViewKind::ColumnMap) {
                    let child_kind = if kind == ViewKind::RowMap {
                        ViewKind::Row
                    } else {
                        ViewKind::Column
                    };
                    let entries = values
                        .iter()
                        .map(|key| {
                            let child =
                                self.new_guava_view(table.clone(), child_kind, Some(key.clone()));
                            map_entry_value(key, &child)
                        })
                        .collect::<Vec<_>>();
                    return Ok(Some(collection_value("set", &entries)));
                }
                return Ok(Some(collection_value("set", &values)));
            }
            _ => {}
        }
        if !is_map {
            return Ok(None);
        }
        let row_axis = matches!(kind, ViewKind::Row | ViewKind::RowMap);
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("get", "(Ljava/lang/Object;)Ljava/lang/Object;")
            | ("containsKey", "(Ljava/lang/Object;)Z")
            | ("remove", "(Ljava/lang/Object;)Ljava/lang/Object;")
            | ("put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
                let lookup = require(args.first())?.clone();
                if matches!(kind, ViewKind::RowMap | ViewKind::ColumnMap) {
                    let child_kind = if row_axis {
                        ViewKind::Row
                    } else {
                        ViewKind::Column
                    };
                    if r.name == "get" {
                        return Ok(Some(self.new_guava_view(table, child_kind, Some(lookup))));
                    }
                    return Ok(Some(Value::Int(
                        self.view_values(&view_marker)
                            .iter()
                            .any(|value| same(value, &lookup)) as i32,
                    )));
                }
                let selected = |row: &Value, column: &Value| {
                    if row_axis {
                        key.as_ref().is_some_and(|key| same(row, key)) && same(column, &lookup)
                    } else {
                        key.as_ref().is_some_and(|key| same(column, key)) && same(row, &lookup)
                    }
                };
                if r.name == "get" || r.name == "containsKey" {
                    let result = self
                        .cell_values(&table)
                        .into_iter()
                        .find(|(row, column, _)| selected(row, column))
                        .map(|(_, _, value)| value);
                    return Ok(Some(if r.name == "containsKey" {
                        Value::Int(result.is_some() as i32)
                    } else {
                        result.unwrap_or_else(null)
                    }));
                }
                let Some((rows, immutable, tree)) = self.table_mut(&table) else {
                    return Ok(None);
                };
                if immutable {
                    return Err(unsupported());
                }
                if r.name == "put" {
                    let value = require(args.get(1))?.clone();
                    let (row, column) = if row_axis {
                        (key.clone().ok_or_else(npe)?, lookup)
                    } else {
                        (lookup, key.clone().ok_or_else(npe)?)
                    };
                    return Ok(Some(Self::insert_cell(rows, row, column, value, tree)));
                }
                let (row, column) = if row_axis {
                    (key.clone().ok_or_else(npe)?, lookup)
                } else {
                    (lookup, key.clone().ok_or_else(npe)?)
                };
                Ok(Some(Self::remove_cell(rows, &row, &column)))
            }
            _ => Ok(None),
        }
    }

    /// The table factories, cells and live views this invocation reaches.
    pub(super) fn invoke_guava_table(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if receiver.is_some_and(is_view) {
            return self.invoke_view(r, receiver.expect("checked"), args);
        }
        match (r.owner.as_str(), r.name.as_str()) {
            ("com/google/common/collect/HashBasedTable", "create") => {
                return Ok(Some(self.new_guava_table(false, false, Vec::new())));
            }
            ("com/google/common/collect/TreeBasedTable", "create") => {
                return Ok(Some(self.new_guava_table(false, true, Vec::new())));
            }
            ("com/google/common/collect/Tables", "newCustomTable") => {
                return Ok(Some(self.new_guava_table(false, false, Vec::new())));
            }
            ("com/google/common/collect/Tables", "synchronizedTable") => {
                let table = args.first().and_then(marker).ok_or_else(npe)?.to_owned();
                let marker = self.next_guava_marker(TABLE);
                self.guava_tables
                    .insert(marker.clone(), State::Wrapper { table });
                return Ok(Some(Value::Ref(Some(marker))));
            }
            ("com/google/common/collect/ImmutableTable", "of") => {
                if !args.len().is_multiple_of(3) {
                    return Ok(None);
                }
                let mut rows = Vec::new();
                for chunk in args.chunks(3) {
                    require(chunk.first())?;
                    require(chunk.get(1))?;
                    require(chunk.get(2))?;
                    Self::insert_cell(
                        &mut rows,
                        chunk[0].clone(),
                        chunk[1].clone(),
                        chunk[2].clone(),
                        false,
                    );
                }
                return Ok(Some(self.new_guava_table(true, false, rows)));
            }
            ("com/google/common/collect/ImmutableTable", "builder") => {
                return Ok(Some(self.new_guava_builder(false)));
            }
            ("com/google/common/collect/ImmutableTable$Builder", "<init>") => {
                return Ok(Some(Value::Void));
            }
            _ => {}
        }
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        let Some(receiver_marker) = marker(receiver).map(str::to_owned) else {
            return Ok(None);
        };
        let mut built = None;
        if let Some(State::Builder { rows, tree }) = self.guava_tables.get_mut(&receiver_marker) {
            match r.name.as_str() {
                "put" if args.len() == 3 => {
                    require(args.first())?;
                    require(args.get(1))?;
                    require(args.get(2))?;
                    Self::insert_cell(
                        rows,
                        args[0].clone(),
                        args[1].clone(),
                        args[2].clone(),
                        *tree,
                    );
                    return Ok(Some(receiver.clone()));
                }
                "build" | "buildOrThrow" => {
                    let rows = std::mem::take(rows);
                    built = Some((rows, *tree));
                }
                _ => {}
            }
        }
        if let Some((rows, tree)) = built {
            return Ok(Some(self.new_guava_table(true, tree, rows)));
        }
        let Some(table_marker) = self.table_root(&receiver_marker) else {
            return Ok(None);
        };
        let cells = self.cell_values(&table_marker);
        match (r.name.as_str(), r.descriptor.as_str()) {
            (
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            ) => {
                let row = require(args.first())?.clone();
                let column = require(args.get(1))?.clone();
                let value = require(args.get(2))?.clone();
                let Some((rows, immutable, tree)) = self.table_mut(&receiver_marker) else {
                    return Ok(None);
                };
                if immutable {
                    return Err(unsupported());
                }
                Ok(Some(Self::insert_cell(rows, row, column, value, tree)))
            }
            ("get", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;")
            | ("contains", "(Ljava/lang/Object;Ljava/lang/Object;)Z")
            | ("remove", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
                let row = require(args.first())?;
                let column = require(args.get(1))?;
                let found = cells
                    .iter()
                    .find(|(r, c, _)| same(r, row) && same(c, column));
                if r.name == "get" {
                    return Ok(Some(
                        found
                            .map(|(_, _, value)| value.clone())
                            .unwrap_or_else(null),
                    ));
                }
                if r.name == "contains" {
                    return Ok(Some(Value::Int(found.is_some() as i32)));
                }
                let Some((rows, immutable, _)) = self.table_mut(&receiver_marker) else {
                    return Ok(None);
                };
                if immutable {
                    return Err(unsupported());
                }
                Ok(Some(Self::remove_cell(rows, row, column)))
            }
            ("containsRow", "(Ljava/lang/Object;)Z") => {
                let key = require(args.first())?;
                Ok(Some(Value::Int(
                    cells.iter().any(|(row, _, _)| same(row, key)) as i32,
                )))
            }
            ("containsColumn", "(Ljava/lang/Object;)Z") => {
                let key = require(args.first())?;
                Ok(Some(Value::Int(
                    cells.iter().any(|(_, column, _)| same(column, key)) as i32,
                )))
            }
            ("containsValue", "(Ljava/lang/Object;)Z") => {
                let key = require(args.first())?;
                Ok(Some(Value::Int(
                    cells.iter().any(|(_, _, value)| same(value, key)) as i32,
                )))
            }
            ("size", "()I") => Ok(Some(Value::Int(cells.len() as i32))),
            ("isEmpty", "()Z") => Ok(Some(Value::Int(cells.is_empty() as i32))),
            ("clear", "()V") => {
                let Some((rows, immutable, _)) = self.table_mut(&receiver_marker) else {
                    return Ok(None);
                };
                if immutable {
                    return Err(unsupported());
                }
                rows.clear();
                Ok(Some(Value::Void))
            }
            ("row", "(Ljava/lang/Object;)Ljava/util/Map;") => {
                let key = require(args.first())?.clone();
                Ok(Some(self.new_guava_view(
                    table_marker,
                    ViewKind::Row,
                    Some(key),
                )))
            }
            ("column", "(Ljava/lang/Object;)Ljava/util/Map;") => {
                let key = require(args.first())?.clone();
                Ok(Some(self.new_guava_view(
                    table_marker,
                    ViewKind::Column,
                    Some(key),
                )))
            }
            ("rowMap", "()Ljava/util/Map;") => Ok(Some(self.new_guava_view(
                table_marker,
                ViewKind::RowMap,
                None,
            ))),
            ("columnMap", "()Ljava/util/Map;") => Ok(Some(self.new_guava_view(
                table_marker,
                ViewKind::ColumnMap,
                None,
            ))),
            ("rowKeySet", "()Ljava/util/Set;") => {
                let values = self
                    .table_rows(&table_marker)
                    .into_iter()
                    .flat_map(|rows| rows.iter().map(|row| row.key.clone()))
                    .collect::<Vec<_>>();
                Ok(Some(collection_value("set", &values)))
            }
            ("columnKeySet", "()Ljava/util/Set;") => {
                let mut values = Vec::new();
                for (_, column, _) in &cells {
                    if !values.iter().any(|value| same(value, column)) {
                        values.push(column.clone());
                    }
                }
                if self
                    .table_root(&table_marker)
                    .and_then(|root| self.guava_tables.get(&root))
                    .is_some_and(|state| matches!(state, State::Table { tree: true, .. }))
                {
                    values.sort_by_key(value_key);
                }
                Ok(Some(collection_value("set", &values)))
            }
            ("values", "()Ljava/util/Collection;") => {
                let values = cells
                    .iter()
                    .map(|(_, _, value)| value.clone())
                    .collect::<Vec<_>>();
                Ok(Some(collection_value("list", &values)))
            }
            ("cellSet", "()Ljava/util/Set;") => {
                let values = cells
                    .iter()
                    .map(|(row, column, value)| map_entry_value(row, &format_cell(column, value)))
                    .collect::<Vec<_>>();
                Ok(Some(collection_value("set", &values)))
            }
            ("toString", "()Ljava/lang/String;") => Ok(Some(Value::Ref(Some(table_text(&cells))))),
            ("hashCode", "()I") => Ok(Some(Value::Int(table_hash(&cells)))),
            ("equals", "(Ljava/lang/Object;)Z") => {
                let other = args
                    .first()
                    .and_then(|value| marker(value))
                    .and_then(|other| self.table_root(other));
                let equal = other.is_some_and(|other| self.cell_values(&other) == cells);
                Ok(Some(Value::Int(equal as i32)))
            }
            _ => Ok(None),
        }
    }
}

fn format_cell(column: &Value, value: &Value) -> Value {
    Value::Ref(Some(format!(
        "{}={}",
        value_text(column),
        value_text(value)
    )))
}

fn table_text(cells: &[(Value, Value, Value)]) -> String {
    let mut rows: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for (row, column, value) in cells {
        let row_key = value_text(row);
        let entry = rows
            .iter_mut()
            .find(|(key, _)| key == &row_key)
            .map(|(_, entries)| entries);
        if let Some(entries) = entry {
            entries.push((value_text(column), value_text(value)));
        } else {
            rows.push((row_key, vec![(value_text(column), value_text(value))]));
        }
    }
    let body = rows
        .into_iter()
        .map(|(row, entries)| {
            let entries = entries
                .into_iter()
                .map(|(column, value)| format!("{column}={value}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{row}={{{entries}}}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{body}}}")
}

fn table_hash(cells: &[(Value, Value, Value)]) -> i32 {
    cells.iter().fold(0, |hash, (row, column, value)| {
        hash.wrapping_add(
            stable_hash(&value_text(row))
                ^ stable_hash(&value_text(column))
                ^ stable_hash(&value_text(value)),
        )
    })
}
