//! Exact object methods for held annotation proxies.
//!
//! Annotation members themselves are decoded by `annotation_values`; this
//! module supplies the JDK `AnnotationInvocationHandler` object contract.

use super::*;
use crate::host_ref::{ClassHierarchy, HostKind, HostRef, Methods};

pub(crate) fn class_of_key(key: &str) -> &str {
    key.strip_prefix("annotation/L")
        .and_then(|rest| rest.split_once(';').map(|(name, _)| name))
        .unwrap_or("java/lang/annotation/Annotation")
}

fn annotation_instance_of(
    handle: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    hierarchy.is_instance(class_of_key(handle.key()), wanted)
}

pub(crate) const METHODS: Methods = Methods {
    instance_of: annotation_instance_of,
    ..crate::host_events::DECLARED_TYPE_METHODS
};

#[derive(Clone)]
struct Member {
    name: String,
    descriptor: String,
    value: AnnotationValue,
}

impl<H: Host> Runtime<'_, H> {
    fn annotation_members(&self, annotation: &Annotation) -> Option<Vec<Member>> {
        let class = annotation
            .descriptor
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))?;
        let file = self.class(class)?;
        file.methods
            .iter()
            .filter(|method| method.descriptor.starts_with("()") && method.name != "<init>")
            .map(|method| {
                let value = annotation
                    .values
                    .get(&method.name)
                    .cloned()
                    .or_else(|| file.annotation_default(&method.name))?;
                Some(Member {
                    name: method.name.clone(),
                    descriptor: method.descriptor[2..].to_owned(),
                    value,
                })
            })
            .collect()
    }

    fn annotation_value_hash(&self, value: &AnnotationValue, descriptor: &str) -> Option<i32> {
        if let Some(element) = descriptor.strip_prefix('[') {
            let AnnotationValue::Array(values) = value else {
                return None;
            };
            let mut hash = 1i32;
            for value in values {
                hash = hash
                    .wrapping_mul(31)
                    .wrapping_add(self.annotation_value_hash(value, element)?);
            }
            return Some(hash);
        }
        Some(match (value, descriptor) {
            (AnnotationValue::Bool(value), "Z") => {
                if *value {
                    1231
                } else {
                    1237
                }
            }
            (AnnotationValue::Int(value), "B" | "C" | "S" | "I") => *value,
            (AnnotationValue::Long(value), "J") => (*value ^ (*value >> 32)) as i32,
            (AnnotationValue::Float(value), "F") => (f32::to_bits(*value as f32)) as i32,
            (AnnotationValue::Float(value), "D") => {
                let bits = value.to_bits();
                (bits ^ (bits >> 32)) as i32
            }
            (AnnotationValue::String(value), "Ljava/lang/String;") => {
                crate::hash_map_order::java_string_hash(value)
            }
            (AnnotationValue::Annotation(annotation), descriptor)
                if descriptor.starts_with('L') =>
            {
                self.annotation_hash(annotation)?
            }
            // Class and enum identity hash codes are VM identity values. Do not
            // invent a stable value for them: leave this proxy call counted.
            (AnnotationValue::String(_), descriptor) if descriptor.starts_with('L') => return None,
            _ => return None,
        })
    }

    fn annotation_hash(&self, annotation: &Annotation) -> Option<i32> {
        let mut hash = 0i32;
        for member in self.annotation_members(annotation)? {
            let value_hash = self.annotation_value_hash(&member.value, &member.descriptor)?;
            hash = hash.wrapping_add(
                crate::hash_map_order::java_string_hash(&member.name).wrapping_mul(127)
                    ^ value_hash,
            );
        }
        Some(hash)
    }

    fn quote_annotation_string(value: &str) -> String {
        let mut out = String::with_capacity(value.len() + 2);
        out.push('"');
        for ch in value.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{0008}' => out.push_str("\\b"),
                '\u{000c}' => out.push_str("\\f"),
                c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    fn quote_annotation_char(value: i32) -> String {
        let ch = char::from_u32(value as u32).unwrap_or('\u{fffd}');
        let escaped = match ch {
            '\\' => "\\\\".to_owned(),
            '\'' => "\\'".to_owned(),
            '\n' => "\\n".to_owned(),
            '\r' => "\\r".to_owned(),
            '\t' => "\\t".to_owned(),
            '\u{0008}' => "\\b".to_owned(),
            '\u{000c}' => "\\f".to_owned(),
            c if c.is_control() => format!("\\u{:04x}", c as u32),
            c => c.to_string(),
        };
        format!("'{escaped}'")
    }

    fn annotation_class_text(value: &str) -> String {
        let name = match value {
            "V" => "void".to_owned(),
            "Z" => "boolean".to_owned(),
            "B" => "byte".to_owned(),
            "C" => "char".to_owned(),
            "S" => "short".to_owned(),
            "I" => "int".to_owned(),
            "J" => "long".to_owned(),
            "F" => "float".to_owned(),
            "D" => "double".to_owned(),
            name if name.starts_with('L') && name.ends_with(';') => {
                name[1..name.len() - 1].replace('/', ".")
            }
            name => name.replace('/', "."),
        };
        format!("{name}.class")
    }

    fn annotation_value_text(&self, value: &AnnotationValue, descriptor: &str) -> Option<String> {
        if let Some(element) = descriptor.strip_prefix('[') {
            let AnnotationValue::Array(values) = value else {
                return None;
            };
            let values = values
                .iter()
                .map(|value| self.annotation_value_text(value, element))
                .collect::<Option<Vec<_>>>()?;
            return Some(format!("{{{}}}", values.join(", ")));
        }
        Some(match (value, descriptor) {
            (AnnotationValue::String(value), "Ljava/lang/String;") => {
                Self::quote_annotation_string(value)
            }
            (AnnotationValue::String(value), "Ljava/lang/Class;") => {
                Self::annotation_class_text(value)
            }
            (AnnotationValue::String(value), descriptor) if descriptor.starts_with('L') => {
                value.rsplit_once(";.").map(|(_, name)| name.to_owned())?
            }
            (AnnotationValue::Bool(value), "Z") => value.to_string(),
            (AnnotationValue::Int(value), "B") => format!("(byte)0x{:02x}", *value as u8),
            (AnnotationValue::Int(value), "C") => Self::quote_annotation_char(*value),
            (AnnotationValue::Int(value), "S" | "I") => value.to_string(),
            (AnnotationValue::Long(value), "J") => format!("{value}L"),
            (AnnotationValue::Float(value), "F") => {
                if value.is_nan() {
                    "0.0f/0.0f".to_owned()
                } else if value.is_infinite() {
                    if value.is_sign_negative() {
                        "-1.0f/0.0f".to_owned()
                    } else {
                        "1.0f/0.0f".to_owned()
                    }
                } else {
                    format!("{}f", crate::format_java_float32(*value))
                }
            }
            (AnnotationValue::Float(value), "D") => {
                if value.is_nan() {
                    "0.0/0.0".to_owned()
                } else if value.is_infinite() {
                    if value.is_sign_negative() {
                        "-1.0/0.0".to_owned()
                    } else {
                        "1.0/0.0".to_owned()
                    }
                } else {
                    crate::format_java_float(*value)
                }
            }
            (AnnotationValue::Annotation(annotation), _) => self.annotation_text(annotation)?,
            _ => return None,
        })
    }

    fn annotation_text(&self, annotation: &Annotation) -> Option<String> {
        let name = annotation
            .descriptor
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))?
            .rsplit(['/', '$'])
            .next()?;
        let members = self.annotation_members(annotation)?;
        let values = members
            .iter()
            .map(|member| {
                Some(format!(
                    "{}={}",
                    member.name,
                    self.annotation_value_text(&member.value, &member.descriptor)?
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(format!("@{name}({})", values.join(", ")))
    }

    fn annotation_value_equal(
        &self,
        left: &AnnotationValue,
        right: &AnnotationValue,
        descriptor: &str,
    ) -> Option<bool> {
        if let Some(element) = descriptor.strip_prefix('[') {
            let (AnnotationValue::Array(left), AnnotationValue::Array(right)) = (left, right)
            else {
                return Some(false);
            };
            if left.len() != right.len() {
                return Some(false);
            }
            let mut equal = true;
            for (left, right) in left.iter().zip(right) {
                equal &= self.annotation_value_equal(left, right, element)?;
            }
            return Some(equal);
        }
        match (left, right, descriptor) {
            (AnnotationValue::Annotation(left), AnnotationValue::Annotation(right), descriptor)
                if descriptor.starts_with('L') =>
            {
                self.annotation_equal(left, right)
            }
            // Class and enum members use VM identity, never their class-file
            // spelling. Without both canonical handles this proxy call counts.
            (AnnotationValue::String(_), AnnotationValue::String(_), descriptor)
                if descriptor.starts_with('L') =>
            {
                None
            }
            (left, right, _) => Some(left == right),
        }
    }

    fn annotation_equal(&self, left: &Annotation, right: &Annotation) -> Option<bool> {
        if left.descriptor != right.descriptor {
            return Some(false);
        }
        let left = self.annotation_members(left)?;
        let right = self.annotation_members(right)?;
        let mut equal = true;
        for member in left {
            let other = right
                .iter()
                .find(|other| other.name == member.name && other.descriptor == member.descriptor)?;
            equal &=
                self.annotation_value_equal(&member.value, &other.value, &member.descriptor)?;
        }
        Some(equal)
    }

    pub(super) fn annotation_object_method(
        &mut self,
        annotation: &Annotation,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("equals", "(Ljava/lang/Object;)Z") => {
                let Some(other) = args.first() else {
                    return self.host.unknown_operand_call(r).map(Some);
                };
                match other {
                    Value::Ref(None) => Ok(Some(Value::Int(0))),
                    Value::Ref(_) => Ok(Some(Value::Int(0))),
                    Value::Host(handle) if handle.kind() == HostKind::AnnotationProxy => {
                        let Some(other) = self.annotations.get(handle.key()) else {
                            return Ok(None);
                        };
                        let equal = self.annotation_equal(annotation, other);
                        Ok(equal.map(|equal| Value::Int(i32::from(equal))))
                    }
                    Value::Host(handle) => {
                        let wanted = annotation
                            .descriptor
                            .strip_prefix('L')
                            .and_then(|name| name.strip_suffix(';'))
                            .unwrap_or(&annotation.descriptor);
                        match handle.instance_of(wanted, self) {
                            Some(false) => Ok(Some(Value::Int(0))),
                            _ => Ok(None),
                        }
                    }
                    _ => Ok(Some(Value::Int(0))),
                }
            }
            ("hashCode", "()I") => Ok(self.annotation_hash(annotation).map(Value::Int)),
            ("toString", "()Ljava/lang/String;") => Ok(self
                .annotation_text(annotation)
                .map(|text| Value::Ref(Some(text)))),
            _ => Ok(None),
        }
    }
}
