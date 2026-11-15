//! Frame-owned client values and exact math for the 26.3 client surface.

use crate::{MethodRef, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum ClientHitResult {
    Block {
        pos: [i32; 3],
        location: [f64; 3],
        face: String,
    },
    Entity {
        entity_id: i32,
        location: [f64; 3],
    },
    Miss {
        location: [f64; 3],
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientOptions {
    /// Filled from `Options.particles().get()` by the game.
    pub particles: Option<String>,
    /// Filled from `Options.sensitivity().get()` by the game.
    pub sensitivity: Option<f64>,
}

fn vec3_value(coords: [f64; 3]) -> Value {
    crate::game_object::client_value(format!(
        "{}{},{},{}",
        crate::client_values::VEC3,
        crate::client_values::coord_text(coords[0]),
        crate::client_values::coord_text(coords[1]),
        crate::client_values::coord_text(coords[2]),
    ))
}

fn vec3_key(value: &Value) -> Option<[f64; 3]> {
    let text = crate::game_object::text(value)?;
    crate::client_values::vec3(text.strip_prefix(crate::client_values::VEC3)?)
}

fn double(value: &Value) -> Option<f64> {
    match value {
        Value::Float(value) => Some(*value),
        Value::Int(value) => Some(f64::from(*value)),
        Value::Long(value) => Some(*value as f64),
        _ => None,
    }
}

fn args3(args: &[Value], at: usize) -> Option<[f64; 3]> {
    Some([
        double(args.get(at)?)?,
        double(args.get(at + 1)?)?,
        double(args.get(at + 2)?)?,
    ])
}

/// Exact `Vec3` arithmetic used by the client frame.
pub(crate) fn vec3_call(key: &str, r: &MethodRef, args: &[Value]) -> Option<Value> {
    let coords = crate::client_values::vec3(key.strip_prefix(crate::client_values::VEC3)?)?;
    let returns = r.descriptor.split_once(')')?.1;
    let value = match (r.name.as_str(), r.descriptor.as_str()) {
        ("add", "(D)Lnet/minecraft/world/phys/Vec3;") => {
            let d = double(args.first()?)?;
            [coords[0] + d, coords[1] + d, coords[2] + d]
        }
        ("add", "(DDD)Lnet/minecraft/world/phys/Vec3;") => {
            let d = args3(args, 0)?;
            [coords[0] + d[0], coords[1] + d[1], coords[2] + d[2]]
        }
        ("add", "(Lnet/minecraft/world/phys/Vec3;)Lnet/minecraft/world/phys/Vec3;") => {
            let d = vec3_key(args.first()?)?;
            [coords[0] + d[0], coords[1] + d[1], coords[2] + d[2]]
        }
        ("subtract", "(D)Lnet/minecraft/world/phys/Vec3;") => {
            let d = double(args.first()?)?;
            [coords[0] - d, coords[1] - d, coords[2] - d]
        }
        ("subtract", "(DDD)Lnet/minecraft/world/phys/Vec3;") => {
            let d = args3(args, 0)?;
            [coords[0] - d[0], coords[1] - d[1], coords[2] - d[2]]
        }
        ("subtract", "(Lnet/minecraft/world/phys/Vec3;)Lnet/minecraft/world/phys/Vec3;") => {
            let d = vec3_key(args.first()?)?;
            [coords[0] - d[0], coords[1] - d[1], coords[2] - d[2]]
        }
        ("scale", "(D)Lnet/minecraft/world/phys/Vec3;") => {
            let d = double(args.first()?)?;
            [coords[0] * d, coords[1] * d, coords[2] * d]
        }
        ("multiply", "(DDD)Lnet/minecraft/world/phys/Vec3;") => {
            let d = args3(args, 0)?;
            [coords[0] * d[0], coords[1] * d[1], coords[2] * d[2]]
        }
        ("normalize", "()Lnet/minecraft/world/phys/Vec3;") => {
            let length =
                (coords[0] * coords[0] + coords[1] * coords[1] + coords[2] * coords[2]).sqrt();
            if length < 9.999_999_747_378_752e-6 {
                [0.0, 0.0, 0.0]
            } else {
                [coords[0] / length, coords[1] / length, coords[2] / length]
            }
        }
        ("length", "()D") => {
            return Some(Value::Float(
                (coords[0] * coords[0] + coords[1] * coords[1] + coords[2] * coords[2]).sqrt(),
            ));
        }
        ("lengthSqr", "()D") => {
            return Some(Value::Float(
                coords[0] * coords[0] + coords[1] * coords[1] + coords[2] * coords[2],
            ));
        }
        ("horizontalDistance", "()D") => {
            return Some(Value::Float(
                (coords[0] * coords[0] + coords[2] * coords[2]).sqrt(),
            ));
        }
        ("horizontalDistanceSqr", "()D") => {
            return Some(Value::Float(coords[0] * coords[0] + coords[2] * coords[2]));
        }
        ("distanceTo", "(Lnet/minecraft/world/phys/Vec3;)D")
        | ("distanceToSqr", "(Lnet/minecraft/world/phys/Vec3;)D") => {
            let other = vec3_key(args.first()?)?;
            let dx = other[0] - coords[0];
            let dy = other[1] - coords[1];
            let dz = other[2] - coords[2];
            let square = dx * dx + dy * dy + dz * dz;
            return Some(Value::Float(if r.name == "distanceTo" {
                square.sqrt()
            } else {
                square
            }));
        }
        ("dot", "(Lnet/minecraft/world/phys/Vec3;)D") => {
            let other = vec3_key(args.first()?)?;
            return Some(Value::Float(
                coords[0] * other[0] + coords[1] * other[1] + coords[2] * other[2],
            ));
        }
        ("cross", "(Lnet/minecraft/world/phys/Vec3;)Lnet/minecraft/world/phys/Vec3;") => {
            let other = vec3_key(args.first()?)?;
            [
                coords[1] * other[2] - coords[2] * other[1],
                coords[2] * other[0] - coords[0] * other[2],
                coords[0] * other[1] - coords[1] * other[0],
            ]
        }
        _ => return None,
    };
    (returns == "Lnet/minecraft/world/phys/Vec3;").then(|| vec3_value(value))
}

pub(crate) fn block_pos_value(pos: [i32; 3]) -> Value {
    crate::game_object::server_value(format!("block-pos/{},{},{}", pos[0], pos[1], pos[2]))
}

/// `BlockPos.containing(double, double, double)`, using vanilla `Mth.floor`.
pub(crate) fn block_pos_static(r: &MethodRef, args: &[Value]) -> Option<Value> {
    if r.owner != "net/minecraft/core/BlockPos"
        || r.name != "containing"
        || r.descriptor != "(DDD)Lnet/minecraft/core/BlockPos;"
    {
        return None;
    }
    let coords = [
        double(args.first()?)?,
        double(args.get(1)?)?,
        double(args.get(2)?)?,
    ];
    Some(block_pos_value(coords.map(floor)))
}

pub(crate) fn block_pos_call(text: &str, r: &MethodRef) -> Option<Value> {
    let mut coords = text.strip_prefix("block-pos/")?.split(',');
    let x = coords.next()?.parse().ok()?;
    let y = coords.next()?.parse().ok()?;
    let z = coords.next()?.parse().ok()?;
    if coords.next().is_some() {
        return None;
    }
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("getX", "()I") => Some(Value::Int(x)),
        ("getY", "()I") => Some(Value::Int(y)),
        ("getZ", "()I") => Some(Value::Int(z)),
        _ => None,
    }
}

/// `Mth.floor`: `(int) d`, one less when the truncating cast rounded up.
pub(crate) fn floor(value: f64) -> i32 {
    let truncated = value as i32;
    if value < f64::from(truncated) {
        truncated.wrapping_sub(1)
    } else {
        truncated
    }
}

pub(crate) fn hit_value(hit: &ClientHitResult) -> Value {
    match hit {
        ClientHitResult::Block {
            pos,
            location,
            face,
        } => crate::game_object::server_value(format!(
            "game/hit-result/block/{},{},{}/{},{},{}/{}",
            location[0], location[1], location[2], pos[0], pos[1], pos[2], face
        )),
        ClientHitResult::Entity {
            entity_id,
            location,
        } => crate::game_object::server_value(format!(
            "game/hit-result/entity/{},{},{}/{}",
            location[0], location[1], location[2], entity_id
        )),
        ClientHitResult::Miss { location } => crate::game_object::server_value(format!(
            "game/hit-result/miss/{},{},{}",
            location[0], location[1], location[2]
        )),
    }
}

pub(crate) fn hit_call(text: &str, r: &MethodRef) -> Option<Value> {
    let rest = text.strip_prefix("game/hit-result/")?;
    let (kind, values) = rest.split_once('/')?;
    let parts = values.split('/').collect::<Vec<_>>();
    let mut location_parts = parts.first()?.split(',');
    let location = [
        location_parts.next()?.parse().ok()?,
        location_parts.next()?.parse().ok()?,
        location_parts.next()?.parse().ok()?,
    ];
    match (kind, r.name.as_str(), r.descriptor.as_str()) {
        ("miss" | "entity" | "block", "getType", "()Lnet/minecraft/world/phys/HitResult$Type;") => {
            Some(crate::game_object::client_value(format!(
                "enum/net/minecraft/world/phys/HitResult$Type/{}",
                kind.to_ascii_uppercase()
            )))
        }
        ("miss" | "entity" | "block", "getLocation", "()Lnet/minecraft/world/phys/Vec3;") => {
            Some(vec3_value(location))
        }
        ("block", "getBlockPos", "()Lnet/minecraft/core/BlockPos;") => {
            let mut position = parts.get(1)?.split(',');
            Some(block_pos_value([
                position.next()?.parse().ok()?,
                position.next()?.parse().ok()?,
                position.next()?.parse().ok()?,
            ]))
        }
        ("block", "getDirection", "()Lnet/minecraft/core/Direction;") => {
            Some(crate::game_object::client_value(format!(
                "enum/net/minecraft/core/Direction/{}",
                parts.get(2)?
            )))
        }
        ("entity", "getEntity", _) => Some(crate::game_object::server_value(format!(
            "game/entity/{}",
            parts.get(1)?
        ))),
        _ => None,
    }
}

pub(crate) fn options_call(option: &str, values: &ClientOptions, r: &MethodRef) -> Option<Value> {
    if r.name != "get" {
        return None;
    }
    match option {
        "particles" => values.particles.as_ref().map(|status| {
            crate::game_object::client_value(format!(
                "enum/net/minecraft/client/ParticleStatus/{status}"
            ))
        }),
        "sensitivity" => values.sensitivity.map(Value::Float),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec3_handle_preserves_nan_payload_through_arithmetic() {
        let input = 0x7ff0_0000_0000_0001u64;
        let key = format!(
            "{}{},{},{}",
            crate::client_values::VEC3,
            crate::client_values::coord_text(f64::from_bits(input)),
            0.0,
            0.0
        );
        let method = MethodRef {
            owner: "net/minecraft/world/phys/Vec3".into(),
            name: "add".into(),
            descriptor: "(DDD)Lnet/minecraft/world/phys/Vec3;".into(),
        };
        let result = vec3_call(
            &key,
            &method,
            &[Value::Float(0.0), Value::Float(0.0), Value::Float(0.0)],
        )
        .expect("Vec3 add is modelled");
        let text = crate::game_object::text(&result).expect("Vec3 handle text");
        let coords =
            crate::client_values::vec3(text.strip_prefix(crate::client_values::VEC3).unwrap())
                .expect("Vec3 coordinates");
        assert_eq!(coords[0].to_bits(), 0x7ff8_0000_0000_0001);
    }
}
