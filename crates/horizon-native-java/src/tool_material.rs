//! Vanilla 26.3 `ToolMaterial` constants (`WOOD` … `NETHERITE`) and their accessors.
//!
//! Each constant is one object, keyed as a jar object, `ToolMaterial#n`, with `n` from the
//! running heap's allocator when it is first read (`Host::allocating_static_field`), and a
//! collection root ([`State::root_texts`]): a static final field is never unreachable. #669:
//! they were `ToolMaterial#(800000 + ordinal)`, a number the heap also hands out, so with the
//! heap at 800000 a jar's own `new ToolMaterial(..)` took `WOOD`'s key: `==` to it, and read as
//! it. A constant is told by its handle, never by the number in its key.
use crate::jar_object::Numbers;
use crate::{MethodRef, NativeError, Value, jar_object};
use std::collections::BTreeMap;
const OWNER: &str = "net/minecraft/world/item/ToolMaterial";
#[derive(Debug, Clone, Copy)]
struct Row {
    durability: i32,
    speed: f64,
    attack: f64,
    enchant: i32,
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
        // The map is keyed by constant name; liveness is the handle's own key.
        self.handles
            .retain(|_, handle| jar_object::object_key(handle).is_some_and(&keep));
    }
    /// The constants' keys: roots of a collection, so their numbers are never freed.
    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        out.extend(self.handles.values().filter_map(jar_object::object_key));
    }
}
fn row(name: &str) -> Option<Row> {
    match name {
        "WOOD" => Some(Row {
            durability: 59,
            speed: 2.0,
            attack: 0.0,
            enchant: 15,
        }),
        "STONE" => Some(Row {
            durability: 131,
            speed: 4.0,
            attack: 1.0,
            enchant: 5,
        }),
        "COPPER" => Some(Row {
            durability: 190,
            speed: 5.0,
            attack: 1.0,
            enchant: 13,
        }),
        "IRON" => Some(Row {
            durability: 250,
            speed: 6.0,
            attack: 2.0,
            enchant: 14,
        }),
        "DIAMOND" => Some(Row {
            durability: 1561,
            speed: 8.0,
            attack: 3.0,
            enchant: 10,
        }),
        "GOLD" => Some(Row {
            durability: 32,
            speed: 12.0,
            attack: 0.0,
            enchant: 22,
        }),
        "NETHERITE" => Some(Row {
            durability: 2031,
            speed: 9.0,
            attack: 4.0,
            enchant: 15,
        }),
        _ => None,
    }
}
/// A `getstatic` of a `ToolMaterial` constant: the one object for it, made with `numbers` on
/// its first read; unanswered when it is not made yet and there is no allocator.
pub(crate) fn static_field(
    state: &mut State,
    r: &MethodRef,
    numbers: Option<&mut Numbers<'_>>,
) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER
        || r.descriptor != "Lnet/minecraft/world/item/ToolMaterial;"
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
pub(crate) fn instance_call(
    state: &mut State,
    r: &MethodRef,
    receiver: &Value,
    _args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER {
        return None;
    }
    // Only a constant this model handed out answers, told by its handle: a jar's own
    // `new ToolMaterial(..)` is a different object and stays counted.
    let name = state
        .handles
        .iter()
        .find_map(|(name, handle)| (handle == receiver).then_some(name))?;
    let x = row(name)?;
    let v = match r.name.as_str() {
        "durability" => Value::Int(x.durability),
        "speed" => Value::Float(x.speed),
        "attackDamageBonus" => Value::Float(x.attack),
        "enchantmentValue" => Value::Int(x.enchant),
        _ => return None,
    };
    Some(Ok(v))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_constants_match_java25_rows() {
        assert_eq!(row("WOOD").unwrap().durability, 59);
        assert_eq!(row("STONE").unwrap().durability, 131);
        assert_eq!(row("COPPER").unwrap().durability, 190);
        assert_eq!(row("IRON").unwrap().durability, 250);
        assert_eq!(row("DIAMOND").unwrap().durability, 1561);
        assert_eq!(row("GOLD").unwrap().durability, 32);
        assert_eq!(row("NETHERITE").unwrap().durability, 2031);
    }
}
