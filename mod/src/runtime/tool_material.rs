use crate::runtime::{MethodRef, NativeError, Value, jar_object};
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
pub(crate) fn static_field(state: &mut State, r: &MethodRef) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER
        || r.descriptor != "Lnet/minecraft/world/item/ToolMaterial;"
        || row(&r.name).is_none()
    {
        return None;
    }
    let v = state.handles.entry(r.name.clone()).or_insert_with(|| {
        jar_object::handle(format!(
            "{OWNER}#{}",
            [
                "WOOD",
                "STONE",
                "COPPER",
                "IRON",
                "DIAMOND",
                "GOLD",
                "NETHERITE"
            ]
            .iter()
            .position(|n| *n == r.name)
            .unwrap()
                + 800000
        ))
    });
    Some(Ok(v.clone()))
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
    // Only a constant this model handed out answers: a jar's own
    // `new ToolMaterial(..)` is a different object and stays counted, whatever
    // number its allocation key ends in.
    if !state.handles.values().any(|handle| handle == receiver) {
        return None;
    }
    let key = jar_object::instance_key(receiver)?;
    let id = key.rsplit_once('#')?.1.parse::<usize>().ok()?;
    let name = [
        "WOOD",
        "STONE",
        "COPPER",
        "IRON",
        "DIAMOND",
        "GOLD",
        "NETHERITE",
    ]
    .get(id.checked_sub(800000)?)?;
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
