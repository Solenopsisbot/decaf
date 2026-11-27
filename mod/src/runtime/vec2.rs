//! `net.minecraft.world.phys.Vec2`'s constants (`ZERO`, `ONE`, `UNIT_X`, …): one object each,
//! whose `x`/`y` the model answers.
//!
//! Each constant is keyed as a jar object, `Vec2#n`, with `n` from the running heap's allocator
//! when it is first read (`Host::allocating_static_field`). #669: they were `Vec2#900000`..,
//! numbers the heap also hands out; once it had handed out and freed one, the constant was
//! born "collected", and reading its `x` failed. They are collection roots
//! ([`State::root_texts`]): a static final field is never unreachable.
use crate::runtime::jar_object::Numbers;
use crate::runtime::{MethodRef, NativeError, Value, jar_object};
use std::collections::BTreeMap;

const OWNER: &str = "net/minecraft/world/phys/Vec2";
#[derive(Debug, Clone, Copy)]
struct Row {
    x: f32,
    y: f32,
}
fn row(name: &str) -> Option<Row> {
    Some(match name {
        "ZERO" => Row { x: 0.0, y: 0.0 },
        "ONE" => Row { x: 1.0, y: 1.0 },
        "UNIT_X" => Row { x: 1.0, y: 0.0 },
        "NEG_UNIT_X" => Row { x: -1.0, y: 0.0 },
        "UNIT_Y" => Row { x: 0.0, y: 1.0 },
        "NEG_UNIT_Y" => Row { x: 0.0, y: -1.0 },
        "MAX" => Row {
            x: f32::MAX,
            y: f32::MAX,
        },
        "MIN" => Row {
            x: f32::from_bits(1),
            y: f32::from_bits(1),
        },
        _ => return None,
    })
}
#[derive(Debug, Default, Clone)]
pub(crate) struct State {
    handles: BTreeMap<String, Value>,
}
impl State {
    pub(crate) fn entry_count(&self) -> usize {
        self.handles.len()
    }
    pub(crate) fn retain_keys(&mut self, keep: impl Fn(&str) -> bool) {
        self.handles
            .retain(|_, value| jar_object::object_key(value).is_some_and(&keep));
    }
    /// The constants' keys: roots of a collection, so their numbers are never freed.
    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        out.extend(self.handles.values().filter_map(jar_object::object_key));
    }
}
/// A `getstatic` of a `Vec2` constant: the one object for it, made with `numbers` on its first
/// read; unanswered when it is not made yet and there is no allocator.
pub(crate) fn static_field(
    state: &mut State,
    r: &MethodRef,
    numbers: Option<&mut Numbers<'_>>,
) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER
        || r.descriptor != "Lnet/minecraft/world/phys/Vec2;"
        || row(&r.name).is_none()
    {
        return None;
    }
    if let Some(v) = state.handles.get(&r.name) {
        return Some(Ok(v.clone()));
    }
    let v = match numbers?.handle(OWNER) {
        Ok(v) => v,
        Err(error) => return Some(Err(error)),
    };
    state.handles.insert(r.name.clone(), v.clone());
    Some(Ok(v))
}
pub(crate) fn get_field(
    state: &State,
    receiver: &Value,
    r: &MethodRef,
) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER || r.descriptor != "F" || !state.handles.values().any(|v| v == receiver) {
        return None;
    }
    let name = state
        .handles
        .iter()
        .find_map(|(n, v)| (v == receiver).then_some(n.as_str()))?;
    let v = row(name)?;
    Some(Ok(match r.name.as_str() {
        "x" => Value::Float(v.x as f64),
        "y" => Value::Float(v.y as f64),
        _ => return None,
    }))
}
