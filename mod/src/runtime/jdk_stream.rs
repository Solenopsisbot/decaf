//! `java.util.Comparator` values and the stream sources and value-list
//! terminals config and service code reaches beyond `map`/`filter`/`collect`.
//!
//! A stream source owns the list of its elements (`Runtime::streams`); the
//! stages and the per-element terminals run lazily over it
//! (`stream_pipeline.rs`). This module adds the sources and terminals real
//! jars stopped on:
//!
//! * `IntStream.range`/`rangeClosed`/`of` and the `LongStream` ranges, and
//!   `sum` (Xaero's config options enumerate their indices with
//!   `IntStream.rangeClosed(min, max).mapToObj(...)`).
//! * The comparators `sorted`, `max` and `min` take (rrls picks its
//!   highest-priority service with `stream.max(COMPARATOR)`).
//!
//! A comparator the runtime makes is a callback handle
//! (`callback_marker::composite`, `HostKind::Callback`), never text, so no
//! `String` is one and none can be read out of one (#261 re-review). Its
//! operation and operands are the row's:
//!
//! * `cmp.natural`: `Comparator.naturalOrder()`, one object;
//! * `cmp.reverse-natural`: `reverseOrder()` (both classes), one object;
//! * `cmp.reverse` over `c`: `c.reversed()`, `Collections.reverseOrder(c)`,
//!   with the JDK's unwrapping (`c.reversed().reversed() == c`, natural and
//!   reverse order each other's `reversed()`); equal by its operand, as
//!   `ReverseComparator2` is;
//! * `cmp.key` over `f`: `Comparator.comparing(f)` and the `comparingInt`,
//!   `comparingLong`, `comparingDouble` forms;
//! * `cmp.keycmp` over `f`, `c`: `Comparator.comparing(f, c)`;
//! * `cmp.then` over `a`, `b`: `a.thenComparing(b)`;
//! * `cmp.entry-key` / `cmp.entry-value`: `Map.Entry.comparingByKey()` /
//!   `comparingByValue()`, one object each.
//!
//! Any other comparator value (a jar lambda, method reference or
//! `Comparator` object) is called through the callback path. Natural order
//! covers the boxed numbers, characters, booleans and strings
//! (`String.compareTo` over UTF-16 units); anything else is refused with a
//! named reason rather than ordered arbitrarily.

use crate::runtime::jdk_model::MAX_SPLITERATOR_ELEMENTS;
use crate::runtime::{Boxed, MethodRef, NativeError, Value, expect_ref};

/// The streams whose JDK spliterator reports SIZED (#166 F3): the lazy
/// pipeline (`stream_pipeline.rs`) reads it, so `count()` of a SIZED
/// pipeline answers the size without traversing (JDK 25's
/// `ReduceOps.makeRefCounting`): no `peek` or `map` runs, and a `sorted()`
/// never compares, so `Stream.of(new int[0], new int[0]).sorted().count()`
/// is `2`, not the `ClassCastException` of comparing two arrays.
#[derive(Default)]
pub(crate) struct StreamPlan {
    sized: std::collections::BTreeSet<String>,
}

/// A comparator the runtime makes: a callback composite (`callback_marker`)
/// whose operation is `cmp.<op>` over `operands`. An operand-free one is a
/// singleton (one key), as the JDK's are.
fn comparator(op: &str, operands: &[&Value]) -> Value {
    crate::runtime::callback_marker::composite(&format!("cmp.{op}"), operands)
}

/// The comparator `value` is when the runtime made it: its `cmp.` operation
/// (without the prefix) and operands.
fn made_comparator(value: &Value) -> Option<(String, Vec<Value>)> {
    let (op, operands) = crate::runtime::callback_marker::composite_of(value)?;
    Some((op.strip_prefix("cmp.")?.to_owned(), operands))
}

/// `Objects.requireNonNull(value)`: a `NullPointerException` with no
/// message, as every `Comparator` factory throws for a `null` argument.
fn require_non_null(values: &[Value]) -> Result<(), NativeError> {
    if values.contains(&Value::Ref(None)) {
        return Err(NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: None,
            frame: None,
        });
    }
    Ok(())
}

/// `Collections.reverseOrder(c)`, which `Comparator.reversed()` calls: the
/// reverse-order singleton for `null`, natural order for it, the operand
/// of a reversed comparator (`c.reversed().reversed() == c`), otherwise a
/// new `ReverseComparator2` over `c`.
fn reverse_of(c: &Value) -> Value {
    if *c == Value::Ref(None) {
        return comparator("reverse-natural", &[]);
    }
    match made_comparator(c) {
        Some((op, _)) if op == "reverse-natural" => comparator("natural", &[]),
        Some((op, _)) if op == "natural" => comparator("reverse-natural", &[]),
        Some((op, operands)) if op == "reverse" && operands.len() == 1 => operands[0].clone(),
        _ => comparator("reverse", &[c]),
    }
}

/// `Comparator`'s static factories and `Map.Entry`'s comparators.
pub(crate) fn comparator_static(
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let value = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        ("java/util/Comparator", "naturalOrder", "()Ljava/util/Comparator;") => {
            comparator("natural", &[])
        }
        (
            "java/util/Comparator" | "java/util/Collections",
            "reverseOrder",
            "()Ljava/util/Comparator;",
        ) => comparator("reverse-natural", &[]),
        (
            "java/util/Collections",
            "reverseOrder",
            "(Ljava/util/Comparator;)Ljava/util/Comparator;",
        ) => reverse_of(args.first().unwrap_or(&Value::Ref(None))),
        (
            "java/util/Comparator",
            "comparing" | "comparingInt" | "comparingLong" | "comparingDouble",
            descriptor,
        ) if descriptor.ends_with(")Ljava/util/Comparator;") => {
            if let Err(error) = require_non_null(args) {
                return Some(Err(error));
            }
            match args {
                [key] => comparator("key", &[key]),
                [key, order] => comparator("keycmp", &[key, order]),
                _ => return None,
            }
        }
        ("java/util/Map$Entry", "comparingByKey", "()Ljava/util/Comparator;") => {
            comparator("entry-key", &[])
        }
        ("java/util/Map$Entry", "comparingByValue", "()Ljava/util/Comparator;") => {
            comparator("entry-value", &[])
        }
        _ => return None,
    };
    Some(Ok(value))
}

/// `reversed()` and `thenComparing*` on any comparator value: one the
/// runtime made or a jar lambda. Combining needs no callback to run.
pub(crate) fn comparator_instance(
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != "java/util/Comparator" || matches!(receiver, Value::Ref(None)) {
        return None;
    }
    let value = match (r.name.as_str(), r.descriptor.as_str()) {
        ("reversed", "()Ljava/util/Comparator;") => reverse_of(receiver),
        ("thenComparing", "(Ljava/util/Comparator;)Ljava/util/Comparator;") => {
            let other = args.first()?;
            if let Err(error) = require_non_null(args) {
                return Some(Err(error));
            }
            comparator("then", &[receiver, other])
        }
        (
            "thenComparing" | "thenComparingInt" | "thenComparingLong" | "thenComparingDouble",
            descriptor,
        ) if descriptor.ends_with(")Ljava/util/Comparator;") => {
            if let Err(error) = require_non_null(args) {
                return Some(Err(error));
            }
            let key = match args {
                [key] => comparator("key", &[key]),
                [key, order] => comparator("keycmp", &[key, order]),
                _ => return None,
            };
            comparator("then", &[receiver, &key])
        }
        _ => return None,
    };
    Some(Ok(value))
}

/// An integral boxed or primitive value, widened to `long`.
fn integral(value: &Value) -> Option<i64> {
    match value {
        Value::Int(value) => Some(i64::from(*value)),
        Value::Long(value) => Some(*value),
        Value::Boxed(boxed @ (Boxed::Integer(_) | Boxed::Short(_) | Boxed::Byte(_))) => {
            Some(i64::from(boxed.int()))
        }
        _ => None,
    }
}

fn floating(value: &Value) -> Option<f64> {
    match value {
        Value::Float(value) => Some(*value),
        Value::Boxed(Boxed::Float(bits)) => Some(f64::from(f32::from_bits(*bits))),
        _ => None,
    }
}

/// `Comparable.compareTo` for the values whose natural order the JDK defines
/// and the runtime can read.
pub(crate) fn natural_compare(a: &Value, b: &Value) -> Result<i32, NativeError> {
    if matches!(a, Value::Ref(None)) || matches!(b, Value::Ref(None)) {
        return Err(NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: None,
            frame: None,
        });
    }
    if let (Some(a), Some(b)) = (integral(a), integral(b)) {
        return Ok(a.cmp(&b) as i32);
    }
    // `Character.compareTo` is the code-unit difference, not a sign.
    let character = |value: &Value| match value {
        Value::Boxed(Boxed::Character(unit)) => Some(i32::from(*unit)),
        _ => None,
    };
    if let (Some(a), Some(b)) = (character(a), character(b)) {
        return Ok(a - b);
    }
    if let (Some(a), Some(b)) = (floating(a), floating(b)) {
        // `Double.compare`: -0.0 < 0.0 and NaN above everything.
        return Ok(a.total_cmp(&b) as i32);
    }
    let boolean = |value: &Value| match value {
        Value::Boxed(Boxed::Boolean(flag)) => Some(*flag),
        _ => None,
    };
    if let (Some(a), Some(b)) = (boolean(a), boolean(b)) {
        return Ok(a.cmp(&b) as i32);
    }
    if let (Value::Ref(Some(a)), Value::Ref(Some(b))) = (a, b) {
        // `String.compareTo`: the first differing UTF-16 unit, else the
        // length difference.
        let (mut left, mut right) = (a.encode_utf16(), b.encode_utf16());
        loop {
            match (left.next(), right.next()) {
                (Some(x), Some(y)) if x != y => return Ok(i32::from(x) - i32::from(y)),
                (Some(_), Some(_)) => continue,
                _ => {
                    return Ok(a.encode_utf16().count() as i32 - b.encode_utf16().count() as i32);
                }
            }
        }
    }
    // `((Comparable) a).compareTo(b)`: a handle whose kind is known not to
    // be `Comparable` (an array) fails the cast, as `ComparableTimSort` does.
    if let Value::Host(handle) = a
        && handle.instance_of("java/lang/Comparable", &crate::runtime::host_ref::JdkHierarchy) == Some(false)
        && let Some(class) = handle.class()
    {
        return Err(crate::runtime::checkcast::class_cast_exception(
            class,
            "java/lang/Comparable",
        ));
    }
    // `String.compareTo(Object)`/`Integer.compareTo(Object)`... are bridges
    // that cast the argument to their own class: a handle of another known
    // class fails that cast (`"s".compareTo(new int[0])`).
    let own = match a {
        Value::Ref(Some(text)) if crate::runtime::value_class::is_plain_string(text) => {
            Some("java/lang/String")
        }
        Value::Boxed(boxed) => Some(boxed.class()),
        _ => None,
    };
    if let (Some(own), Value::Host(other)) = (own, b)
        && other.instance_of(own, &crate::runtime::host_ref::JdkHierarchy) == Some(false)
        && let Some(class) = other.class()
    {
        return Err(crate::runtime::checkcast::class_cast_exception(class, own));
    }
    Err(NativeError::Metadata(format!(
        "natural order of {} and {} is not modelled",
        super::describe_value(a),
        super::describe_value(b)
    )))
}

impl<H: crate::runtime::Host> crate::runtime::Runtime<'_, H> {
    /// `comparator.compare(a, b)`, for a modelled comparator marker or any
    /// comparator value the callback path can run.
    pub(crate) fn compare_with(
        &mut self,
        comparator: &Value,
        a: &Value,
        b: &Value,
    ) -> Result<i32, NativeError> {
        // One level of a composed comparator (#250): a chain as deep as a jar builds it ends
        // in a `StackOverflowError`, as Java's lambda chain does.
        let _level = crate::runtime::host_recursion::enter()?;
        // `sorted(null)` and `List.sort(null)` mean natural order.
        if *comparator == Value::Ref(None) {
            return self.natural_order(a, b);
        }
        // Any comparator the runtime did not make (a jar lambda, method
        // reference or `Comparator` object) runs its own `compare`.
        let Some((op, operands)) = made_comparator(comparator) else {
            let result = self.apply_callback(comparator, &[a.clone(), b.clone()])?;
            return super::boxed_integer(&result).ok_or_else(|| {
                NativeError::Metadata(format!(
                    "Comparator.compare answered {}",
                    super::describe_value(&result)
                ))
            });
        };
        let operand = |at: usize| operands.get(at).cloned().unwrap_or(Value::Ref(None));
        match op.as_str() {
            "natural" => self.natural_order(a, b),
            "reverse-natural" => self.natural_order(b, a),
            "reverse" => self.compare_with(&operand(0), b, a),
            "key" => {
                let key = operand(0);
                let left = self.apply_callback(&key, std::slice::from_ref(a))?;
                let right = self.apply_callback(&key, std::slice::from_ref(b))?;
                self.natural_order(&left, &right)
            }
            "keycmp" => {
                let (key, order) = (operand(0), operand(1));
                let left = self.apply_callback(&key, std::slice::from_ref(a))?;
                let right = self.apply_callback(&key, std::slice::from_ref(b))?;
                self.compare_with(&order, &left, &right)
            }
            "then" => match self.compare_with(&operand(0), a, b)? {
                0 => self.compare_with(&operand(1), a, b),
                order => Ok(order),
            },
            "entry-key" | "entry-value" => {
                // `Map.Entry.comparingByKey/Value`: `c1.getKey().compareTo(c2.getKey())`, each
                // read as the entry's own accessor reads it (`host_map_entry.rs`). An entry whose
                // contents are unknown (a Gson reading after a write, B8) is that accessor's
                // counted call, answering its neutral value, never an internal error (#221
                // review 7, B12).
                let key = op == "entry-key";
                let mut part = |entry: &Value| -> Result<Value, NativeError> {
                    match self.host.map_entry_parts(entry) {
                        Some((k, v)) => Ok(if key { k } else { v }),
                        None => self.host.unknown_operand_call(&MethodRef {
                            owner: "java/util/Map$Entry".into(),
                            name: if key { "getKey" } else { "getValue" }.into(),
                            descriptor: "()Ljava/lang/Object;".into(),
                        }),
                    }
                };
                let left = part(a)?;
                let right = part(b)?;
                self.natural_order(&left, &right)
            }
            _ => Err(NativeError::Metadata(format!(
                "unsupported comparator cmp.{op}"
            ))),
        }
    }

    /// `Comparator.compare(a, b)` called by jar code on a modelled marker.
    pub(crate) fn invoke_modelled_comparator(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != "java/util/Comparator"
            || r.name != "compare"
            || r.descriptor != "(Ljava/lang/Object;Ljava/lang/Object;)I"
        {
            return Ok(None);
        }
        let Some(receiver) = receiver.filter(|value| made_comparator(value).is_some()) else {
            return Ok(None);
        };
        let (Some(a), Some(b)) = (args.first(), args.get(1)) else {
            return Ok(None);
        };
        Ok(Some(Value::Int(self.compare_with(receiver, a, b)?)))
    }

    /// `List.sort(comparator)`, `Collections.sort(list)` and
    /// `Collections.sort(list, comparator)` on a list the host holds.
    ///
    /// The host keeps the elements but cannot run a jar comparator, so the
    /// runtime sorts them here and writes them back through the host's own
    /// `clear`/`add`. A list the host does not hold mutably (an immutable
    /// `List.of`) is left to the host.
    pub(crate) fn invoke_list_sort(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let (list, comparator) = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("java/util/Collections", "sort", "(Ljava/util/List;)V") => {
                (args.first().cloned(), Value::Ref(None))
            }
            ("java/util/Collections", "sort", "(Ljava/util/List;Ljava/util/Comparator;)V") => (
                args.first().cloned(),
                args.get(1).cloned().unwrap_or(Value::Ref(None)),
            ),
            (owner, "sort", "(Ljava/util/Comparator;)V") if super::is_list_owner(owner) => (
                receiver.cloned(),
                args.first().cloned().unwrap_or(Value::Ref(None)),
            ),
            _ => return Ok(None),
        };
        let Some(list) = list else {
            return Ok(None);
        };
        let Some(items) = self.host.mutable_collection_values(&list) else {
            return Ok(None);
        };
        let mut values = items.clone();
        // `ArrayList.sort` sorts its own array, which a comparator that
        // throws leaves part-merged; `List.sort`'s default sorts a copy and
        // writes nothing back then.
        let in_place = self
            .host_collection_class(&list)
            .is_some_and(|class| class == "java/util/ArrayList");
        // A comparator that changes the list: the JDK sorts its array (or a
        // copy) around the change and then checks `modCount`. Not modelled:
        // counted, with the exception a structural change ends in.
        let expected = self.capture_mod_count(&list);
        let changed_under = |this: &mut Self| -> Result<Option<bool>, NativeError> {
            let structural = this.capture_mod_count(&list) != expected;
            let now = this.host.mutable_collection_values(&list);
            let same = now.is_some_and(|now| crate::runtime::same_items(&now, &items));
            Ok((structural || !same).then_some(structural))
        };
        let sorted = match self.sort_in_place(&mut values, &comparator) {
            Ok(()) => values,
            Err(error) => {
                if in_place {
                    self.host.replace_mutable_collection(&list, values);
                }
                return Err(error);
            }
        };
        if let Some(structural) = changed_under(self)? {
            self.host.unknown_operand_call(r)?;
            if structural {
                return Err(NativeError::UncaughtException {
                    class: "java/util/ConcurrentModificationException".into(),
                    message: None,
                    frame: None,
                });
            }
        }
        let method = |name: &str, descriptor: &str| MethodRef {
            owner: "java/util/List".into(),
            name: name.into(),
            descriptor: descriptor.into(),
        };
        // `Arrays.asList`'s view sorts its array in place: no structural
        // change (`array_views.rs`).
        if matches!(
            self.host_collection_class(&list).as_deref(),
            Some(crate::runtime::array_views::CLASS | crate::runtime::array_views::WINDOW_CLASS)
        ) {
            self.host.replace_mutable_collection(&list, sorted);
            return Ok(Some(Value::Void));
        }
        self.host
            .invoke_instance(&method("clear", "()V"), &list, &[])?;
        let add = method("add", "(Ljava/lang/Object;)Z");
        for value in sorted {
            self.host.invoke_instance(&add, &list, &[value])?;
        }
        Ok(Some(Value::Void))
    }

    /// `Arrays.sort(values, comparator)`: TimSort (`tim_sort.rs`), which makes
    /// the JDK's comparisons in the JDK's order. `null` is natural order.
    pub(crate) fn sort_values(
        &mut self,
        values: Vec<Value>,
        comparator: &Value,
    ) -> Result<Vec<Value>, NativeError> {
        let mut values = values;
        self.sort_in_place(&mut values, comparator)?;
        Ok(values)
    }

    /// TimSort `values` in place; a comparison that throws leaves them as
    /// the JDK's array is left.
    pub(crate) fn sort_in_place(
        &mut self,
        values: &mut [Value],
        comparator: &Value,
    ) -> Result<(), NativeError> {
        let comparator = comparator.clone();
        crate::runtime::tim_sort::sort(values, &mut |a, b| self.compare_with(&comparator, a, b))
    }

    /// `IntStream`/`LongStream` sources: `range`, `rangeClosed` and `of`.
    pub(crate) fn primitive_stream_source(
        &mut self,
        r: &MethodRef,
        args: &[Value],
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        let long = match r.owner.as_str() {
            "java/util/stream/IntStream" => false,
            "java/util/stream/LongStream" => true,
            _ => return Ok(None),
        };
        let values = match (r.name.as_str(), r.descriptor.as_str()) {
            (
                "range" | "rangeClosed",
                "(II)Ljava/util/stream/IntStream;" | "(JJ)Ljava/util/stream/LongStream;",
            ) => {
                let start = args.first().and_then(integral).unwrap_or_default();
                let end = args.get(1).and_then(integral).unwrap_or_default();
                let end = if r.name == "rangeClosed" {
                    end.saturating_add(1)
                } else {
                    end
                };
                let count = end.saturating_sub(start).max(0);
                if count > MAX_SPLITERATOR_ELEMENTS as i64 {
                    return Err(NativeError::Metadata(format!(
                        "{}.{} spans {count} elements; the bounded stream holds at most {MAX_SPLITERATOR_ELEMENTS}",
                        r.owner, r.name
                    )));
                }
                (start..end)
                    .map(|value| {
                        if long {
                            Value::Long(value)
                        } else {
                            Value::Int(value as i32)
                        }
                    })
                    .collect()
            }
            ("of", "(I)Ljava/util/stream/IntStream;" | "(J)Ljava/util/stream/LongStream;") => {
                args.to_vec()
            }
            ("of", "([I)Ljava/util/stream/IntStream;" | "([J)Ljava/util/stream/LongStream;") => {
                match args.first() {
                    Some(array) => self.array(array)?.into_owned(),
                    None => return Ok(None),
                }
            }
            ("empty", _) => Vec::new(),
            _ => return Ok(None),
        };
        // `range`, `of` and `empty` are SIZED.
        let stream = self.new_stream(class, method, values);
        self.mark_sized(&stream);
        Ok(Some(stream))
    }

    /// The value-list terminals beyond the interpreter's core set, applied to
    /// the receiver stream's output `values` (the stages and per-element
    /// terminals are `stream_pipeline.rs`'s).
    pub(crate) fn more_stream_stages(
        &mut self,
        r: &MethodRef,
        values: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let result = match (r.name.as_str(), r.descriptor.as_str()) {
            // The stream's elements through the host's iterator table, the
            // same iterator a `List.iterator()` over them answers.
            ("iterator", "()Ljava/util/Iterator;") => {
                let list = self.new_host_collection(
                    "java/util/ArrayList",
                    values.to_vec(),
                    (&r.owner, &r.name, &r.descriptor),
                )?;
                self.host.invoke_instance(
                    &MethodRef {
                        owner: "java/util/List".into(),
                        name: "iterator".into(),
                        descriptor: "()Ljava/util/Iterator;".into(),
                    },
                    &list,
                    &[],
                )?
            }
            ("sum", "()I") => Value::Int(
                values
                    .iter()
                    .filter_map(integral)
                    .fold(0i32, |sum, value| sum.wrapping_add(value as i32)),
            ),
            ("sum", "()J") => Value::Long(
                values
                    .iter()
                    .filter_map(integral)
                    .fold(0i64, i64::wrapping_add),
            ),
            ("sum", "()D") => Value::Float(values.iter().filter_map(floating).sum()),
            _ => return Ok(None),
        };
        Ok(Some(result))
    }
}

impl<H: crate::runtime::Host> crate::runtime::Runtime<'_, H> {
    /// A stream built from a source whose spliterator reports SIZED
    /// (`Stream.of`, an array, a collection, `IntStream.range`).
    pub(crate) fn mark_sized(&mut self, stream: &Value) {
        if let Some(marker) = expect_ref(stream) {
            self.stream_plan.sized.insert(marker.clone());
        }
    }

    /// Whether a stream's pipeline reports SIZED.
    pub(crate) fn stream_is_sized(&self, marker: &str) -> bool {
        self.stream_plan.sized.contains(marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Value {
        Value::Ref(Some(value.into()))
    }

    #[test]
    fn natural_order_matches_the_jdk_compare_to() {
        assert_eq!(
            natural_compare(&Value::Boxed(Boxed::Integer(3)), &Value::Int(7)).unwrap(),
            -1
        );
        assert_eq!(
            natural_compare(&Value::Long(9), &Value::Long(2)).unwrap(),
            1
        );
        // "apple".compareTo("apricot") == 'p' - 'r'
        assert_eq!(
            natural_compare(&text("apple"), &text("apricot")).unwrap(),
            -2
        );
        // "ab".compareTo("abcd") == 2 - 4
        assert_eq!(natural_compare(&text("ab"), &text("abcd")).unwrap(), -2);
        assert_eq!(
            natural_compare(
                &Value::Boxed(Boxed::Boolean(false)),
                &Value::Boxed(Boxed::Boolean(true))
            )
            .unwrap(),
            -1
        );
        assert!(matches!(
            natural_compare(&Value::Ref(None), &Value::Int(1)),
            Err(NativeError::UncaughtException { class, .. })
                if class == "java/lang/NullPointerException"
        ));
    }

    #[test]
    fn comparators_are_callback_handles_with_the_jdks_identities() {
        let call = |name: &str| MethodRef {
            owner: "java/util/Comparator".into(),
            name: name.into(),
            descriptor: "()Ljava/util/Comparator;".into(),
        };
        let lambda = crate::runtime::callback_marker::lambda_from_text(
            "demo/A#x",
            1,
            "java/util/Comparator",
            "demo/A#cmp(II)I",
            "",
        );
        let reversed = comparator_instance(&call("reversed"), &lambda, &[])
            .unwrap()
            .unwrap();
        assert!(crate::runtime::callback_marker::callback_text(&reversed).is_some());
        assert_eq!(
            made_comparator(&reversed),
            Some(("reverse".into(), vec![lambda.clone()]))
        );
        // `c.reversed().reversed() == c`; the singletons are one object each.
        let back = comparator_instance(&call("reversed"), &reversed, &[])
            .unwrap()
            .unwrap();
        assert_eq!(back, lambda);
        let natural = comparator_static(&call("naturalOrder"), &[])
            .unwrap()
            .unwrap();
        let reverse = comparator_static(&call("reverseOrder"), &[])
            .unwrap()
            .unwrap();
        assert_eq!(
            comparator_instance(&call("reversed"), &natural, &[])
                .unwrap()
                .unwrap(),
            reverse
        );
        assert_eq!(
            comparator_instance(&call("reversed"), &reverse, &[])
                .unwrap()
                .unwrap(),
            natural
        );
        // A `String` spelled like the old text form is no comparator.
        assert_eq!(made_comparator(&text("@cmp:natural")), None);
    }
}
