//! Exact bounded model of `Collections.newSetFromMap` over the host's
//! existing mutable collection handles.

use crate::{Boxed, Host, MethodRef, NativeError, StdHost, Value, expect_ref};

const SET_CLASS: &str = "java/util/Collections$SetFromMap";

impl StdHost {
    /// The JDK's live `SetFromMap` factory. Unsupported map implementations
    /// stay counted; the existing mutable map table remains the source of
    /// truth for supported views.
    pub(crate) fn new_set_from_map(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if r.owner != "java/util/Collections"
            || r.name != "newSetFromMap"
            || r.descriptor != "(Ljava/util/Map;)Ljava/util/Set;"
        {
            return None;
        }
        let Some(map) = args.first().and_then(expect_ref) else {
            return Some(Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: Some(
                    "Cannot invoke \"java.util.Map.isEmpty()\" because \"map\" is null".into(),
                ),
                frame: None,
            }));
        };
        let values = self.mutable_collections.get(map)?;
        if !values.is_empty() {
            return Some(Err(NativeError::UncaughtException {
                class: "java/lang/IllegalArgumentException".into(),
                message: Some("Map is non-empty".into()),
                frame: None,
            }));
        }
        let class = map.split_once('#').map_or(map.as_str(), |(class, _)| class);
        // `SetFromMap` delegates every operation to the map. The bounded host
        // can prove the exact order/equality/mod-count contract only for these
        // two map implementations; every other Map stays counted (#316).
        if !matches!(
            class,
            "java/util/HashMap"
                | "java/util/LinkedHashMap"
                | "java/util/concurrent/ConcurrentHashMap"
        ) {
            return None;
        }
        let marker = crate::jar_object::collection_key_for(
            SET_CLASS,
            crate::jar_object::CollectionFamily::Neutral,
            self.next_set_from_map as usize,
        );
        self.next_set_from_map = self.next_set_from_map.saturating_add(1);
        self.mutable_collections.insert(marker.clone(), Vec::new());
        self.set_from_map_views
            .insert(marker.clone(), map.to_owned());
        Some(Ok(crate::jar_object::collection(marker)))
    }

    pub(crate) fn set_from_map_remove_if(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if r.name != "removeIf" || r.descriptor != "(Ljava/util/function/Predicate;)Z" {
            return None;
        }
        let marker = crate::jar_object::object_key(receiver)?.to_owned();
        let map = self.set_from_map_views.get(&marker)?.clone();
        let predicate = args.first()?.clone();
        let keys: Vec<Value> = self
            .mutable_collections
            .get(&map)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| pair[0].clone())
            .collect();
        let mut removed = Vec::new();
        for key in keys {
            let result = match self.invoke_instance(
                &MethodRef {
                    owner: "java/util/function/Predicate".into(),
                    name: "test".into(),
                    descriptor: "(Ljava/lang/Object;)Z".into(),
                },
                &predicate,
                std::slice::from_ref(&key),
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if matches!(
                result,
                Value::Int(flag) if flag != 0
            ) || matches!(result, Value::Boxed(Boxed::Boolean(true)))
            {
                removed.push(key);
            }
        }
        let values = self.mutable_collections.get_mut(&map)?;
        let mut changed = false;
        let mut kept = Vec::with_capacity(values.len());
        for pair in values.as_chunks::<2>().0 {
            let keep = !removed.iter().any(|key| pair[0] == *key);
            changed |= !keep;
            if keep {
                kept.extend(pair.iter().cloned());
            }
        }
        *values = kept;
        Some(Ok(Value::Int(changed as i32)))
    }

    pub(crate) fn set_from_map_retain_all(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if r.name != "retainAll" || r.descriptor != "(Ljava/util/Collection;)Z" {
            return None;
        }
        let marker = crate::jar_object::object_key(receiver)?.to_owned();
        let map = self.set_from_map_views.get(&marker)?.clone();
        let source = args.first()?;
        let retained = match crate::runtime_collection_values(self, source) {
            Ok(values) => values,
            Err(error) => return Some(Err(error)),
        };
        let values = self.mutable_collections.get_mut(&map)?;
        let mut changed = false;
        let mut kept = Vec::with_capacity(values.len());
        for pair in values.as_chunks::<2>().0 {
            let keep = retained.iter().any(|value| value == &pair[0]);
            changed |= !keep;
            if keep {
                kept.extend(pair.iter().cloned());
            }
        }
        *values = kept;
        Some(Ok(Value::Int(changed as i32)))
    }

    /// Mutators that SetFromMap must route to its backing map. Reads and
    /// iteration use `set_from_map_values` through `mutable_values`.
    pub(crate) fn set_from_map_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        // The table is keyed by the host handle's identity. A String spelling
        // the key is deliberately rejected before this lookup (#156 P12b).
        let Value::Host(handle) = receiver else {
            return None;
        };
        if handle.kind() != crate::host_ref::HostKind::Collection {
            return None;
        }
        let map = self.set_from_map_views.get(handle.key())?.clone();
        let chm = self.is_chm_set(receiver);
        let values = self.mutable_collections.get_mut(&map)?;
        let position = |values: &[Value], key: &Value| -> Option<Option<usize>> {
            Some(
                values
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .position(|pair| pair[0] == *key),
            )
        };
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("add", "(Ljava/lang/Object;)Z") => {
                let key = args.first()?.clone();
                if chm && matches!(key, Value::Ref(None)) {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/NullPointerException".into(),
                        message: Some("null".into()),
                        frame: None,
                    }));
                }
                let Some(position) = position(values, &key) else {
                    return Some(self.unknown_host_call(r));
                };
                if position.is_some() {
                    return Some(Ok(Value::Int(0)));
                }
                values.extend([key, Value::Boxed(Boxed::Boolean(true))]);
                Some(Ok(Value::Int(1)))
            }
            ("remove", "(Ljava/lang/Object;)Z") => {
                let key = args.first()?;
                if chm && matches!(key, Value::Ref(None)) {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/NullPointerException".into(),
                        message: Some(
                            "Cannot invoke \"Object.hashCode()\" because \"key\" is null".into(),
                        ),
                        frame: None,
                    }));
                }
                let Some(position) = position(values, key) else {
                    return Some(self.unknown_host_call(r));
                };
                let Some(index) = position else {
                    return Some(Ok(Value::Int(0)));
                };
                values.drain(index * 2..index * 2 + 2);
                Some(Ok(Value::Int(1)))
            }
            ("clear", "()V") => {
                values.clear();
                Some(Ok(Value::Void))
            }
            // `AbstractCollection.addAll` / `AbstractSet.removeAll` on a
            // `ConcurrentHashMap`-backed set: the result and the set after it
            // do not depend on either collection's order. A null element
            // throws part-way, at a point that does, so it stays counted;
            // so does an argument the host holds no elements for.
            ("addAll" | "removeAll", "(Ljava/util/Collection;)Z") if chm => {
                let source = args.first()?.clone();
                let Ok(others) = crate::runtime_collection_values(self, &source) else {
                    return Some(self.unknown_host_call(r));
                };
                if matches!(source, Value::Ref(None))
                    || others.iter().any(|value| matches!(value, Value::Ref(None)))
                {
                    return Some(self.unknown_host_call(r));
                }
                let values = self.mutable_collections.get_mut(&map)?;
                let mut changed = false;
                if r.name == "addAll" {
                    for key in others {
                        let present = values.as_chunks::<2>().0.iter().any(|pair| pair[0] == key);
                        if !present {
                            values.extend([key, Value::Boxed(Boxed::Boolean(true))]);
                            changed = true;
                        }
                    }
                } else {
                    let mut kept = Vec::with_capacity(values.len());
                    for pair in values.as_chunks::<2>().0 {
                        let drop = others.iter().any(|value| value == &pair[0]);
                        changed |= drop;
                        if !drop {
                            kept.extend(pair.iter().cloned());
                        }
                    }
                    *values = kept;
                }
                Some(Ok(Value::Int(i32::from(changed))))
            }
            _ => None,
        }
    }
}

pub(crate) fn known_set_hash(value: &Value) -> Option<i32> {
    match value {
        Value::Int(value) => Some(*value),
        Value::Long(value) => Some((*value ^ ((*value as u64) >> 32) as i64) as i32),
        Value::Float(value) => Some(value.to_bits() as i32),
        Value::Ref(None) => Some(0),
        Value::Ref(Some(marker)) if !marker.contains('#') && !marker.contains('/') => {
            Some(crate::jdk_model::java_string_hash(marker))
        }
        _ => None,
    }
}

pub(crate) fn is_set_value(value: &Value) -> bool {
    let Some(marker) = expect_ref(value) else {
        return false;
    };
    let class = marker
        .split_once('#')
        .map_or(marker.as_str(), |(class, _)| class);
    class == "set" || crate::is_set_owner(class)
}

/// The calls on a `ConcurrentHashMap`-backed set whose answer depends on the
/// map's iteration order, which only a JVM's hash table layout decides: they
/// are counted, never answered. Membership, size, equality and `hashCode` do
/// not depend on order and are answered.
pub(crate) fn chm_set_ordered_read(name: &str, descriptor: &str) -> bool {
    matches!(
        (name, descriptor),
        ("iterator", "()Ljava/util/Iterator;")
            | ("toArray", "()[Ljava/lang/Object;")
            | ("toArray", "([Ljava/lang/Object;)[Ljava/lang/Object;")
            | ("toArray", "(Ljava/util/function/IntFunction;)[Ljava/lang/Object;")
            // Runs the action in iteration order (#548 review).
            | ("forEach", "(Ljava/util/function/Consumer;)V")
            // Tests the predicate in iteration order: a predicate's side
            // effects would show it (#548 re-review).
            | ("removeIf", "(Ljava/util/function/Predicate;)Z")
            | ("stream", "()Ljava/util/stream/Stream;")
            | ("parallelStream", "()Ljava/util/stream/Stream;")
            | ("spliterator", "()Ljava/util/Spliterator;")
            | ("toString", "()Ljava/lang/String;")
    )
}

#[cfg(test)]
mod tests {
    use crate::host_ref::HostKind;
    use crate::jar_object::{self, CollectionFamily};
    use crate::{Host, MethodRef, StdHost, Value};

    fn factory() -> MethodRef {
        MethodRef {
            owner: "java/util/Collections".into(),
            name: "newSetFromMap".into(),
            descriptor: "(Ljava/util/Map;)Ljava/util/Set;".into(),
        }
    }

    #[test]
    fn a_string_spelling_the_exact_set_marker_cannot_mutate_the_backing_map() {
        let mut host = StdHost::new();
        let map_key =
            jar_object::collection_key_for("java/util/HashMap", CollectionFamily::Neutral, 9001);
        host.mutable_collections.insert(map_key.clone(), Vec::new());
        let map = jar_object::collection(map_key.clone());
        let set = host
            .new_set_from_map(&factory(), std::slice::from_ref(&map))
            .expect("HashMap is supported")
            .expect("empty backing map succeeds");
        let (marker, handle_kind) = match &set {
            Value::Host(handle) => (handle.key().to_owned(), handle.kind()),
            other => panic!("expected host set, got {other:?}"),
        };
        assert_eq!(handle_kind, HostKind::Collection);
        let forged = Value::Ref(Some(marker.clone()));
        let add = MethodRef {
            owner: "java/util/Set".into(),
            name: "add".into(),
            descriptor: "(Ljava/lang/Object;)Z".into(),
        };
        let _ = host
            .invoke_instance(&add, &forged, &[Value::Ref(Some("forged".into()))])
            .expect("the forged String remains an ordinary counted receiver");
        assert!(host.mutable_collections.get(&map_key).unwrap().is_empty());
        assert!(
            host.unknown_host_calls()
                .iter()
                .any(|(call, _)| call == "java/util/Set#add(Ljava/lang/Object;)Z")
        );
    }
}
