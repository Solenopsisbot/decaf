//! Pure Minecraft 26.3 `SoundEvent` values.
//!
//! The factories allocate a record containing an Identifier and an optional
//! fixed range.  The value is represented as a GameValue handle so record
//! equality is by those fields while distinct factory calls remain distinct
//! for identity operations.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::{MethodRef, NativeError, StdHost, Value, expect_float};

const PREFIX: &str = "sound-event/";
const VARIABLE: &str = "variable";
static NEXT_SOUND_EVENT: AtomicU64 = AtomicU64::new(0);

fn key(location: &str, fixed: Option<f32>) -> String {
    let range = fixed.map_or(VARIABLE.to_owned(), |value| {
        let bits = if value.is_nan() {
            f32::NAN.to_bits()
        } else {
            value.to_bits()
        };
        format!("fixed:{bits:08x}")
    });
    format!(
        "{PREFIX}{location}/{range}#{}",
        NEXT_SOUND_EVENT.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn parse(key: &str) -> Option<(&str, Option<f32>)> {
    let rest = key.strip_prefix(PREFIX)?;
    let rest = rest.rsplit_once('#')?.0;
    let (location, range) = rest.rsplit_once('/')?;
    let fixed = if range == VARIABLE {
        None
    } else {
        let bits = u32::from_str_radix(range.strip_prefix("fixed:")?, 16).ok()?;
        Some(f32::from_bits(bits))
    };
    Some((location, fixed))
}

pub(crate) fn equals_keys(left: &str, right: &str) -> Option<bool> {
    let (left_location, left_range) = parse(left)?;
    let (right_location, right_range) = parse(right)?;
    Some(
        left_location == right_location
            && match (left_range, right_range) {
                (None, None) => true,
                (Some(a), Some(b)) => a.to_bits() == b.to_bits(),
                _ => false,
            },
    )
}

pub(crate) fn to_string(key: &str) -> Option<String> {
    let (location, fixed) = parse(key)?;
    Some(format!(
        "SoundEvent[location={location}, fixedRange={}]",
        fixed.map_or_else(
            || "Optional.empty".to_owned(),
            |value| format!("Optional[{}]", crate::runtime::format_java_float32(value as f64))
        )
    ))
}

pub(crate) fn hash_code(key: &str) -> Option<i32> {
    let (location, fixed) = parse(key)?;
    let location_hash = location.split_once(':').map_or_else(
        || crate::runtime::stable_hash(location),
        |(namespace, path)| {
            crate::runtime::stable_hash(namespace)
                .wrapping_mul(31)
                .wrapping_add(crate::runtime::stable_hash(path))
        },
    );
    let fixed_hash = fixed.map_or(0, |value| {
        let bits = if value.is_nan() {
            f32::NAN.to_bits()
        } else {
            value.to_bits()
        };
        bits as i32
    });
    Some(location_hash.wrapping_mul(31).wrapping_add(fixed_hash))
}

pub(crate) fn from_value(value: &Value) -> Option<(String, Option<f32>)> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != crate::runtime::host_ref::HostKind::GameValue {
        return None;
    }
    let (location, fixed) = parse(handle.key())?;
    Some((location.to_owned(), fixed))
}

pub(crate) fn construct_record(host: &mut StdHost, r: &MethodRef, args: &[Value]) -> Option<Value> {
    if r.owner != "net/minecraft/sounds/SoundEvent"
        || r.name != "<init>"
        || r.descriptor != "(Lnet/minecraft/resources/Identifier;Ljava/util/Optional;)V"
    {
        return None;
    }
    let location = args
        .first()
        .and_then(|value| crate::runtime::java_identifier::id_text(host, value))?;
    let optional = args.get(1)?;
    if !crate::runtime::is_optional(optional) {
        return None;
    }
    let fixed = match crate::runtime::optional_present(optional) {
        None => None,
        Some(Value::Boxed(crate::runtime::Boxed::Float(bits))) => Some(f32::from_bits(bits)),
        Some(Value::Float(v)) => Some(v as f32),
        Some(_) => return None,
    };
    Some(game_value(key(&location, fixed)))
}

fn game_value(key: String) -> Value {
    crate::runtime::game_object::value(&key).expect("sound-event key is a GameValue")
}

pub(crate) fn static_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != "net/minecraft/sounds/SoundEvent" {
        return None;
    }
    let location = args
        .first()
        .and_then(|value| crate::runtime::java_identifier::id_text(host, value))?;
    let fixed = match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "createVariableRangeEvent",
            "(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/sounds/SoundEvent;",
        ) => None,
        (
            "createFixedRangeEvent",
            "(Lnet/minecraft/resources/Identifier;F)Lnet/minecraft/sounds/SoundEvent;",
        ) => Some(
            match expect_float(r, args.get(1).unwrap_or(&Value::Float(0.0))) {
                Ok(value) => value as f32,
                Err(error) => return Some(Err(error)),
            },
        ),
        _ => return None,
    };
    Some(Ok(game_value(key(&location, fixed))))
}

pub(crate) fn instance_call(
    host: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let Value::Host(handle) = receiver else {
        return None;
    };
    if handle.kind() != crate::runtime::host_ref::HostKind::GameValue {
        return None;
    }
    let (location, fixed) = parse(handle.key())?;
    if r.owner != "net/minecraft/sounds/SoundEvent" {
        return None;
    }
    Some(match (r.name.as_str(), r.descriptor.as_str()) {
        ("location", "()Lnet/minecraft/resources/Identifier;") => {
            Ok(crate::runtime::host_ids::identifier_of_text(location))
        }
        ("getRange", "(F)F") => {
            let distance = match expect_float(r, args.first().unwrap_or(&Value::Float(0.0))) {
                Ok(value) => value as f32,
                Err(error) => return Some(Err(error)),
            };
            Ok(Value::Float(f64::from(fixed.unwrap_or(
                if distance > 1.0 {
                    16.0 * distance
                } else {
                    16.0
                },
            ))))
        }
        ("fixedRange", "()Ljava/util/Optional;") => Ok(crate::runtime::optional_wrap(
            fixed.map_or(Value::Ref(None), |value| {
                Value::Boxed(crate::runtime::Boxed::float(f64::from(value)))
            }),
        )),
        _ => host.unknown_host_call(r),
    })
}

/// `SoundEvents`' `SoundEvent` fields with their ids, read from `server-26.3.jar`'s
/// `SoundEvents.<clinit>` (`javap -c`): each is `register(String)`, which is
/// `Registry.register(BuiltInRegistries.SOUND_EVENT, id, SoundEvent.createVariableRangeEvent(id))`
/// with `id = Identifier.withDefaultNamespace(name)`. The `Holder$Reference` fields
/// (`registerForHolder`) are not here.
const VANILLA_SOUND_EVENTS: &str = crate::runtime::vendored_vanilla_sound_events_26_3_tsv::VENDORED_DATA;

/// `getstatic SoundEvents.<NAME>:SoundEvent`: the one variable-range event the field holds,
/// the same object on every read (its key carries no allocation number).
pub(crate) fn vanilla_static(r: &MethodRef) -> Option<Value> {
    if r.owner != "net/minecraft/sounds/SoundEvents"
        || r.descriptor != "Lnet/minecraft/sounds/SoundEvent;"
    {
        return None;
    }
    let id = VANILLA_SOUND_EVENTS
        .lines()
        .find_map(|line| line.split_once('\t').filter(|(name, _)| *name == r.name))?
        .1;
    Some(game_value(format!("{PREFIX}{id}/{VARIABLE}#static")))
}
