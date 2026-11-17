//! Vanilla registry `ResourceKey` constants used by 26.3 mod bootstrap.
use crate::{MethodRef, Value};
const ENCHANTMENTS: &str = "net/minecraft/world/item/enchantment/Enchantments";
const DAMAGE_TYPES: &str = "net/minecraft/world/damagesource/DamageTypes";
const KEY: &str = "Lnet/minecraft/resources/ResourceKey;";
fn captured(name: &str) -> Option<&'static str> {
    include_str!("vanilla_enchantments_capture.txt")
        .lines()
        .find_map(|l| {
            let (n, rest) = l.split_once('|')?;
            (n == name).then_some(rest)
        })
}
pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    if r.descriptor != KEY {
        return None;
    }
    if r.owner == DAMAGE_TYPES {
        let location = include_str!("vanilla_damage_types_capture.txt")
            .lines()
            .find_map(|l| {
                let (name, location) = l.split_once('|')?;
                (name == r.name).then_some(location)
            })?;
        return Some(crate::host_ids::resource_key_named(
            "minecraft:damage_type",
            location,
        ));
    }
    if r.owner != ENCHANTMENTS {
        return None;
    }
    let rest = captured(&r.name)?;
    let (_registry, location) = rest.split_once('|')?;
    Some(crate::host_ids::resource_key_named(
        "minecraft:enchantment",
        location,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_enchantment_key_is_interned() {
        let r = MethodRef {
            owner: ENCHANTMENTS.into(),
            name: "KNOCKBACK".into(),
            descriptor: KEY.into(),
        };
        let field = static_field(&r).unwrap();
        let created =
            crate::host_ids::resource_key_named("minecraft:enchantment", "minecraft:knockback");
        assert_eq!(field, created);
        match (field, created) {
            (Value::Host(a), Value::Host(b)) => assert_eq!(a.same_object(&b), Some(true)),
            _ => panic!("expected canonical host keys"),
        }
    }
}
#[cfg(test)]
mod captured_walk_tests {
    use super::*;
    #[test]
    fn all_captured_rows_are_correct_and_interned() {
        let mut correct = 0;
        let mut counted = 0;
        let mut wrong = 0;
        for line in include_str!("vanilla_enchantments_capture.txt").lines() {
            let mut p = line.split('|');
            let name = p.next().unwrap();
            let registry = p.next().unwrap();
            let location = p.next().unwrap();
            let r = MethodRef {
                owner: ENCHANTMENTS.into(),
                name: name.into(),
                descriptor: KEY.into(),
            };
            match static_field(&r) {
                Some(Value::Host(_)) => {
                    correct += 1;
                    let constructed = crate::host_ids::resource_key_named(registry, location);
                    assert_eq!(static_field(&r), Some(constructed));
                }
                None => counted += 1,
                _ => wrong += 1,
            }
        }
        assert_eq!((correct, counted, wrong), (43, 0, 0));
    }
}

#[cfg(test)]
mod method_semantics_tests {
    #[test]
    fn canonical_key_exposes_registry_and_identifier_parts() {
        let key =
            crate::host_ids::resource_key_named("minecraft:enchantment", "minecraft:knockback");
        assert_eq!(
            crate::host_ids::resource_key_parts(&key),
            Some(("minecraft:enchantment".into(), "minecraft:knockback".into()))
        );
    }
}
