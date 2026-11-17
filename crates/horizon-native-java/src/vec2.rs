use crate::{MethodRef, NativeError, Value, jar_object};
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
}
pub(crate) fn static_field(state: &mut State, r: &MethodRef) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER
        || r.descriptor != "Lnet/minecraft/world/phys/Vec2;"
        || row(&r.name).is_none()
    {
        return None;
    }
    let id = [
        "ZERO",
        "ONE",
        "UNIT_X",
        "NEG_UNIT_X",
        "UNIT_Y",
        "NEG_UNIT_Y",
        "MAX",
        "MIN",
    ]
    .iter()
    .position(|name| *name == r.name)
    .unwrap()
        + 900000;
    // 900000+ cannot collide: the jar never allocates a Vec2 itself; new Vec2 is counted.
    let v = state
        .handles
        .entry(r.name.clone())
        .or_insert_with(|| jar_object::handle(format!("{OWNER}#{id}")));
    Some(Ok(v.clone()))
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
