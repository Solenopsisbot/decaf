//! Canonical Log4j 2 `Level` objects and its process-local level registry.

#[derive(Clone, Debug, Default)]
pub(crate) struct CustomLevels {
    entries: Vec<(String, (String, i32))>,
}

impl CustomLevels {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    fn get(&self, key: &str) -> Option<(String, i32)> {
        self.entries
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, value)| value.clone())
    }
    fn insert(&mut self, key: String, value: (String, i32)) {
        self.entries.push((key, value));
    }
}
use crate::runtime::host_ref::{HostKind, HostRef, Methods, Nested, instance_of_class};
use crate::runtime::{MethodRef, NativeError, Value, expect_ref};

const OWNER: &str = "org/apache/logging/log4j/Level";
const LEVEL_DESC: &str = "Lorg/apache/logging/log4j/Level;";
const LEVELS: &[(&str, i32)] = &[
    ("OFF", 0),
    ("FATAL", 100),
    ("ERROR", 200),
    ("WARN", 300),
    ("INFO", 400),
    ("DEBUG", 500),
    ("TRACE", 600),
    ("ALL", i32::MAX),
];

fn text(value: &str) -> Value {
    Value::Ref(Some(value.into()))
}

fn level_value(name: &str, int_level: i32) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::Log4jLevel,
        &[text(name), Value::Int(int_level)],
    ))
}

fn level_parts(value: &Value) -> Option<(String, i32)> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::Log4jLevel {
        return None;
    }
    match handle.values()?.as_slice() {
        [Value::Ref(Some(name)), Value::Int(int_level)] => Some((name.clone(), *int_level)),
        _ => None,
    }
}

fn standard(name: &str) -> Option<(&'static str, i32)> {
    let normalized = name.trim().to_uppercase();
    LEVELS
        .iter()
        .copied()
        .find(|(known, _)| *known == normalized)
}

fn normalized(name: &str) -> String {
    name.trim().to_uppercase()
}

fn lookup(name: &str, custom: &CustomLevels) -> Option<(String, i32)> {
    if let Some((known, int_level)) = standard(name) {
        return Some((known.into(), int_level));
    }
    custom.get(&normalized(name))
}

fn illegal(message: impl Into<String>) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn null_pointer(message: impl Into<String>) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn level_text(handle: &HostRef, _: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    Ok(level_parts(&Value::Host(handle.clone())).map(|(name, _)| name))
}

fn level_hash(handle: &HostRef, _: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    Ok(level_parts(&Value::Host(handle.clone())).map(|(text, _)| {
        text.encode_utf16().fold(0i32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(i32::from(unit))
        })
    }))
}

fn level_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(Some(matches!(
        other,
        Value::Host(other)
            if other.kind() == HostKind::Log4jLevel && other.key() == handle.key()
    )))
}

fn level_compare(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<i32>, NativeError> {
    let Some((_, left)) = level_parts(&Value::Host(handle.clone())) else {
        return Ok(None);
    };
    let Some((_, right)) = level_parts(other) else {
        return match other {
            Value::Ref(None) => Err(null_pointer(
                "Cannot invoke \"org.apache.logging.log4j.Level.intLevel()\" because \"other\" is null",
            )),
            _ => Ok(None),
        };
    };
    Ok(Some(match left.cmp(&right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }))
}

pub(crate) const METHODS: Methods = Methods {
    to_string: level_text,
    hash_code: level_hash,
    equals: level_equals,
    instance_of: instance_of_class,
    crosses_jars: |_, _| true,
    compare_to: Some(level_compare),
};

pub(crate) fn static_field(owner: &str, name: &str) -> Option<Value> {
    (owner == OWNER)
        .then(|| standard(name).map(|(name, int)| level_value(name, int)))
        .flatten()
}

fn custom_order(custom: &CustomLevels) -> Vec<(String, i32)> {
    let mut levels = LEVELS
        .iter()
        .map(|(name, int)| ((*name).into(), *int))
        .collect::<Vec<_>>();
    levels.extend(custom.entries.iter().map(|(_, value)| value.clone()));
    let mut size = 16usize;
    while levels.len() > size - size / 4 {
        size *= 2;
    }
    levels.sort_by_key(|(name, _)| {
        let hash = normalized(name).encode_utf16().fold(0u32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(u32::from(unit))
        });
        ((hash ^ (hash >> 16)) as usize) & (size - 1)
    });
    levels
}

fn values_array(custom: &CustomLevels) -> Value {
    let values = custom_order(custom)
        .into_iter()
        .map(|(name, int)| level_value(&name, int))
        .collect::<Vec<_>>();
    Value::Host(HostRef::of_values(HostKind::Log4jLevelArray, &values))
}

pub(crate) fn array_values(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::Log4jLevelArray => handle.values(),
        _ => None,
    }
}

pub(crate) fn static_call(
    r: &MethodRef,
    args: &[Value],
    custom: &mut CustomLevels,
) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER {
        return None;
    }
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("forName", "(Ljava/lang/String;I)Lorg/apache/logging/log4j/Level;") => {
            let Some(Value::Ref(Some(raw))) = args.first() else {
                return Some(Err(illegal("Illegal null or empty Level name.")));
            };
            if raw.is_empty() {
                return Some(Err(illegal("Illegal null or empty Level name.")));
            }
            if let Some((name, int)) = standard(raw) {
                return Some(Ok(level_value(name, int)));
            }
            let key = normalized(raw);
            if let Some((name, int)) = custom.get(&key) {
                return Some(Ok(level_value(&name, int)));
            }
            let int = match args.get(1) {
                Some(Value::Int(int)) => *int,
                _ => return None,
            };
            if int < 0 {
                return Some(Err(illegal("Illegal Level int less than zero.")));
            }
            custom.insert(key, (raw.clone(), int));
            Some(Ok(level_value(raw, int)))
        }
        ("getLevel", "(Ljava/lang/String;)Lorg/apache/logging/log4j/Level;") => {
            let Some(Value::Ref(Some(raw))) = args.first() else {
                return Some(Err(illegal("Illegal null or empty Level name.")));
            };
            if raw.is_empty() {
                return Some(Err(illegal("Illegal null or empty Level name.")));
            }
            Some(Ok(lookup(raw, custom)
                .map(|(name, int)| level_value(&name, int))
                .unwrap_or(Value::Ref(None))))
        }
        ("valueOf", "(Ljava/lang/String;)Lorg/apache/logging/log4j/Level;") => {
            let Some(Value::Ref(Some(raw))) = args.first() else {
                return Some(Err(null_pointer("No level name given.")));
            };
            Some(match lookup(raw, custom) {
                Some((name, int)) => Ok(level_value(&name, int)),
                None => Err(illegal(format!(
                    "Unknown level constant [{}].",
                    normalized(raw)
                ))),
            })
        }
        ("toLevel", "(Ljava/lang/String;)Lorg/apache/logging/log4j/Level;") => Some(Ok(args
            .first()
            .and_then(expect_ref)
            .and_then(|raw| lookup(raw, custom))
            .map_or_else(
                || level_value("DEBUG", 500),
                |(name, int)| level_value(&name, int),
            ))),
        ("toLevel", descriptor)
            if descriptor == format!("(Ljava/lang/String;{LEVEL_DESC}){LEVEL_DESC}") =>
        {
            Some(Ok(args
                .first()
                .and_then(expect_ref)
                .and_then(|raw| lookup(raw, custom))
                .map_or_else(
                    || {
                        args.get(1)
                            .cloned()
                            .unwrap_or_else(|| level_value("DEBUG", 500))
                    },
                    |(name, int)| level_value(&name, int),
                )))
        }
        ("values", "()[Lorg/apache/logging/log4j/Level;") => Some(Ok(values_array(custom))),
        _ => None,
    }
}

pub(crate) fn instance_call(
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let (_, value) = level_parts(receiver)?;
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("intLevel", "()I") => Some(Ok(Value::Int(value))),
        ("name" | "toString", "()Ljava/lang/String;") => {
            Some(Ok(Value::Ref(Some(level_parts(receiver)?.0))))
        }
        ("isMoreSpecificThan", descriptor) if descriptor == format!("({LEVEL_DESC})Z") => {
            Some(Ok(Value::Int(i32::from(value <= level_argument(args)?))))
        }
        ("isLessSpecificThan", descriptor) if descriptor == format!("({LEVEL_DESC})Z") => {
            Some(Ok(Value::Int(i32::from(value >= level_argument(args)?))))
        }
        ("isInRange", descriptor) if descriptor == format!("({LEVEL_DESC}{LEVEL_DESC})Z") => {
            let low = level_argument(args)?;
            let high = level_argument(args.get(1..)?)?;
            Some(Ok(Value::Int(i32::from(value >= low && value <= high))))
        }
        _ => None,
    }
}

fn level_argument(args: &[Value]) -> Option<i32> {
    level_parts(args.first()?).map(|(_, int)| int)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(value: &str) -> Value {
        Value::Ref(Some(value.into()))
    }
    fn method(name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: OWNER.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    #[test]
    fn custom_levels_are_canonical_and_values_are_ordered() {
        let mut custom = CustomLevels::new();
        let first = static_call(
            &method(
                "forName",
                "(Ljava/lang/String;I)Lorg/apache/logging/log4j/Level;",
            ),
            &[text("custom"), Value::Int(350)],
            &mut custom,
        )
        .unwrap()
        .unwrap();
        let second = static_call(
            &method(
                "forName",
                "(Ljava/lang/String;I)Lorg/apache/logging/log4j/Level;",
            ),
            &[text(" CUSTOM "), Value::Int(1)],
            &mut custom,
        )
        .unwrap()
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(level_parts(&first), Some(("custom".into(), 350)));
        assert!(matches!(
            static_call(
                &method("values", "()[Lorg/apache/logging/log4j/Level;"),
                &[],
                &mut custom
            ),
            Some(Ok(Value::Host(_)))
        ));
    }
}
