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
    /// Whether the annotation type declares an `AnnotationDefault` for the
    /// element, whatever value this annotation holds.
    has_default: bool,
}

/// `java.util.HashMap`'s table size after `count` puts into a map made by
/// `new HashMap<>(0)`: one slot, doubled each time the size passes
/// three quarters of the table. The JDK keeps an annotation type's defaults in
/// such a map, and its bucket order is the order a default is printed in.
fn default_table_size(count: usize) -> usize {
    let (mut table, mut threshold) = (1usize, 0usize);
    for size in 1..=count {
        if size > threshold {
            table *= 2;
            threshold = table * 3 / 4;
        }
    }
    table
}

/// Whether a string constant may hold an unpaired surrogate the class reader
/// replaced with U+FFFD (#447): it then cannot be told from a real U+FFFD, and
/// a hash or an equal answer would be a guess.
fn may_have_lost_surrogate(value: &str) -> bool {
    value.contains('\u{fffd}')
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
            .filter(|method| {
                // The elements are the annotation interface's abstract
                // methods; a constant's `<clinit>` is not one.
                method.descriptor.starts_with("()")
                    && method.access & 0x0400 != 0
                    && method.name != "<init>"
                    && method.name != "<clinit>"
            })
            .map(|method| {
                let default = file.annotation_default(&method.name);
                let has_default = default.is_some();
                let value = annotation.values.get(&method.name).cloned().or(default)?;
                Some(Member {
                    name: method.name.clone(),
                    descriptor: method.descriptor[2..].to_owned(),
                    value,
                    has_default,
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
            // `Float.hashCode` and `Double.hashCode` take the canonical NaN.
            (AnnotationValue::Float(value), "F") => {
                if value.is_nan() {
                    0x7fc0_0000
                } else {
                    f32::to_bits(*value as f32) as i32
                }
            }
            (AnnotationValue::Float(value), "D") => {
                let bits = if value.is_nan() {
                    0x7ff8_0000_0000_0000
                } else {
                    value.to_bits()
                };
                (bits ^ (bits >> 32)) as i32
            }
            (AnnotationValue::String(value), "Ljava/lang/String;") => {
                if may_have_lost_surrogate(value) {
                    return None;
                }
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

    /// One UTF-16 unit of a `String` or `char` element, as the JDK escapes it:
    /// the named escapes, then `\uXXXX` for everything outside printable ASCII
    /// (so a supplementary character is its two surrogates).
    fn push_annotation_unit(out: &mut String, unit: u16, quote: char) {
        match unit {
            0x08 => out.push_str("\\b"),
            0x09 => out.push_str("\\t"),
            0x0a => out.push_str("\\n"),
            0x0c => out.push_str("\\f"),
            0x0d => out.push_str("\\r"),
            0x5c => out.push_str("\\\\"),
            unit if unit == quote as u16 => {
                out.push('\\');
                out.push(quote);
            }
            0x20..=0x7e => out.push(unit as u8 as char),
            unit => out.push_str(&format!("\\u{unit:04x}")),
        }
    }

    fn quote_annotation_string(value: &str) -> Option<String> {
        if may_have_lost_surrogate(value) {
            return None;
        }
        let mut out = String::with_capacity(value.len() + 2);
        out.push('"');
        for unit in value.encode_utf16() {
            Self::push_annotation_unit(&mut out, unit, '"');
        }
        out.push('"');
        Some(out)
    }

    fn quote_annotation_char(value: i32) -> String {
        let mut out = String::from("'");
        Self::push_annotation_unit(&mut out, value as u16, '\'');
        out.push('\'');
        out
    }

    /// A `Class` element: its canonical name and `.class`. A class the runtime
    /// cannot name canonically (a nested class it holds no `InnerClasses` for,
    /// a local or anonymous one) is `None`, counted.
    fn annotation_class_text(&self, value: &str) -> Option<String> {
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
            array if array.starts_with('[') => self.canonical_class_name(array)??,
            name => {
                let internal = name.strip_prefix('L')?.strip_suffix(';')?;
                self.canonical_class_name(internal)??
            }
        };
        Some(format!("{name}.class"))
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
                Self::quote_annotation_string(value)?
            }
            (AnnotationValue::String(value), "Ljava/lang/Class;") => {
                self.annotation_class_text(value)?
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

    /// The elements in the order the JDK's `AnnotationInvocationHandler` holds
    /// them, which is the order `toString` prints them in: it starts from a
    /// copy of the type's defaults, a `HashMap<String, Object>` iterated in
    /// bucket order, and an explicit value replaces its default in place, so
    /// the elements the type gives no default follow in the class file's order.
    ///
    /// Two defaulted elements in one bucket sit in the order the JVM listed the
    /// type's methods, which it does not define: `None`, counted.
    fn annotation_print_order<'a>(
        annotation: &Annotation,
        members: &'a [Member],
    ) -> Option<Vec<&'a Member>> {
        let defaulted: Vec<&Member> = members.iter().filter(|m| m.has_default).collect();
        let mask = (default_table_size(defaulted.len()) - 1) as u32;
        let mut bucketed: Vec<(u32, &Member)> = defaulted
            .into_iter()
            .map(|member| {
                let hash = crate::hash_map_order::java_string_hash(&member.name) as u32;
                ((hash ^ (hash >> 16)) & mask, member)
            })
            .collect();
        bucketed.sort_by_key(|(bucket, _)| *bucket);
        if bucketed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return None;
        }
        let mut ordered: Vec<&Member> = bucketed.into_iter().map(|(_, member)| member).collect();
        // Class-file order, then (a hand-built annotation keeps none) by name.
        let listed = annotation.order.iter().map(String::as_str);
        let unlisted = annotation
            .values
            .keys()
            .map(String::as_str)
            .filter(|name| !annotation.order.iter().any(|listed| listed == name));
        for name in listed.chain(unlisted) {
            if let Some(member) = members
                .iter()
                .find(|member| member.name == name && !member.has_default)
            {
                ordered.push(member);
            }
        }
        (ordered.len() == members.len()).then_some(ordered)
    }

    /// `AnnotationInvocationHandler.toStringImpl`: `@` and the type's canonical
    /// name, then the elements in [`Self::annotation_print_order`] as
    /// `name=value`, except a lone element named `value`, which prints bare.
    fn annotation_text(&self, annotation: &Annotation) -> Option<String> {
        let internal = annotation
            .descriptor
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))?;
        let name = self.canonical_class_name(internal)??;
        let members = self.annotation_members(annotation)?;
        let ordered = Self::annotation_print_order(annotation, &members)?;
        let lone_value = ordered.len() == 1 && ordered[0].name == "value";
        let values = ordered
            .iter()
            .map(|member| {
                let text = self.annotation_value_text(&member.value, &member.descriptor)?;
                Some(if lone_value {
                    text
                } else {
                    format!("{}={text}", member.name)
                })
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
            (
                AnnotationValue::String(left),
                AnnotationValue::String(right),
                "Ljava/lang/String;",
            ) => {
                // Unequal text stays unequal after the lossy read, equal text
                // may not have been.
                if left != right {
                    Some(false)
                } else if may_have_lost_surrogate(left) {
                    None
                } else {
                    Some(true)
                }
            }
            // Class and enum members use VM identity, never their class-file
            // spelling. Without both canonical handles this proxy call counts.
            (AnnotationValue::String(_), AnnotationValue::String(_), descriptor)
                if descriptor.starts_with('L') =>
            {
                None
            }
            // `Float.equals` and `Double.equals` compare the canonical bits: a
            // NaN equals itself and `0.0` differs from `-0.0`.
            (AnnotationValue::Float(left), AnnotationValue::Float(right), "F" | "D") => {
                Some((left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits())
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
                    return Ok(Some(Value::Int(0)));
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
