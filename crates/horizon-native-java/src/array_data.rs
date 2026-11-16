//! The runtime's arrays, with a compact backing for primitive ones (#274).
//!
//! Every array used to be a `Vec<Value>`: a `Value` is 32 bytes, so EntityCulling's
//! `new byte[4 << 20]` (`ArrayOcclusionCache`) held 128 MiB where Java holds 4 MiB. An array is
//! now [`ArrayData`]: a `byte[]` keeps one `i8` per element, every other array keeps its
//! `Value`s as before (the other primitive kinds follow in later slices of
//! hj-notes/compact-arrays-plan.md). The kind is chosen once, from the array's class, when the
//! array is made, so a typed array never holds a reference.
//!
//! Loads and stores answer as the JVM's typed instructions do: `baload` sign-extends to an
//! `int`, `bastore` keeps the low 8 bits (`(byte) value`), so a store from bytecode javac did not
//! narrow (`bipush`-free generated code, Kotlin) reads back what Java reads. Readers that want
//! every element as a [`Value`] (most models) use [`ArrayData::values`], which borrows a reference
//! array and builds the `Value`s of a typed one.

use std::borrow::Cow;

use crate::Value;

/// One array's elements.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ArrayData {
    /// Any array not yet given a typed backing: reference arrays (checked by `aastore`, mirrored
    /// by `Arrays.asList` views) and the primitive kinds of later slices.
    Ref(Vec<Value>),
    /// `byte[]`: one `i8` per element.
    Byte(Vec<i8>),
}

impl From<Vec<Value>> for ArrayData {
    fn from(values: Vec<Value>) -> Self {
        Self::Ref(values)
    }
}

/// The `int` a `bastore` (or a model) writes: the low 8 bits of an `int`, a `Byte`'s own value.
fn byte_of(value: &Value) -> Option<i8> {
    match value {
        Value::Int(value) => Some(*value as i8),
        Value::Boxed(crate::Boxed::Byte(value)) => Some(*value),
        _ => None,
    }
}

impl ArrayData {
    /// The array of `class` (`[B`, `[Ljava/lang/String;`...) holding `values`: a `byte[]` whose
    /// values are all `int`s keeps them as bytes (each narrowed as `bastore` narrows); anything
    /// else keeps its `Value`s.
    pub(crate) fn of_class(class: &str, values: Vec<Value>) -> Self {
        if class == "[B"
            && let Some(bytes) = values.iter().map(byte_of).collect::<Option<Vec<i8>>>()
        {
            return Self::Byte(bytes);
        }
        Self::Ref(values)
    }

    /// A new `byte[length]` of zeros, or `None` where the allocator refuses (never an abort).
    pub(crate) fn zeroed_bytes(length: usize) -> Option<Self> {
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).ok()?;
        bytes.resize(length, 0);
        Some(Self::Byte(bytes))
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Ref(values) => values.len(),
            Self::Byte(bytes) => bytes.len(),
        }
    }

    /// Element `index` as the JVM's load pushes it (`baload`: the byte sign-extended to `int`).
    pub(crate) fn load(&self, index: usize) -> Option<Value> {
        match self {
            Self::Ref(values) => values.get(index).cloned(),
            Self::Byte(bytes) => bytes.get(index).map(|byte| Value::Int(i32::from(*byte))),
        }
    }

    /// `boolean[]` shares the JVM's `baload` opcode with `byte[]`, but its
    /// stored value is always the low bit and its load is always `0` or `1`.
    pub(crate) fn boolean_load(&self, index: usize) -> Option<Value> {
        match self {
            Self::Ref(values) => values.get(index).and_then(|value| match value {
                Value::Int(value) => Some(Value::Int(value & 1)),
                _ => None,
            }),
            Self::Byte(_) => None,
        }
    }

    /// Store `value` at `index` as the JVM's store does (`bastore`: the low 8 bits). `false` when
    /// `index` is out of range, or the value is no `int` for a `byte[]` (the caller's type error).
    pub(crate) fn store(&mut self, index: usize, value: Value) -> bool {
        match self {
            Self::Ref(values) => match values.get_mut(index) {
                Some(slot) => {
                    *slot = value;
                    true
                }
                None => false,
            },
            Self::Byte(bytes) => match (bytes.get_mut(index), byte_of(&value)) {
                (Some(slot), Some(byte)) => {
                    *slot = byte;
                    true
                }
                _ => false,
            },
        }
    }

    /// Store the value of a `bastore` into a `boolean[]` (`value & 1`).
    pub(crate) fn boolean_store(&mut self, index: usize, value: Value) -> bool {
        match self {
            Self::Ref(values) => match (values.get_mut(index), value) {
                (Some(slot), Value::Int(value)) => {
                    *slot = Value::Int(value & 1);
                    true
                }
                _ => false,
            },
            Self::Byte(_) => false,
        }
    }

    /// `Arrays.fill(a, value)` over the whole array; `false` for a value its kind cannot hold.
    pub(crate) fn fill(&mut self, value: Value) -> bool {
        match self {
            Self::Ref(values) => {
                values.fill(value);
                true
            }
            Self::Byte(bytes) => match byte_of(&value) {
                Some(byte) => {
                    bytes.fill(byte);
                    true
                }
                None => false,
            },
        }
    }

    /// Every element as a [`Value`]: a reference array borrowed, a typed one built (as its loads
    /// would push each element). Hot paths use [`Self::load`]/[`Self::len`] or the typed slices.
    pub(crate) fn values(&self) -> Cow<'_, [Value]> {
        match self {
            Self::Ref(values) => Cow::Borrowed(values),
            Self::Byte(bytes) => Cow::Owned(
                bytes
                    .iter()
                    .map(|byte| Value::Int(i32::from(*byte)))
                    .collect(),
            ),
        }
    }

    /// A reference array's elements, for the paths that only ever write those (`aastore`'s
    /// views, `toArray(T[])`); `None` for a typed array.
    pub(crate) fn refs_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Self::Ref(values) => Some(values),
            Self::Byte(_) => None,
        }
    }

    /// A `byte[]`'s bytes.
    pub(crate) fn bytes(&self) -> Option<&[i8]> {
        match self {
            Self::Byte(bytes) => Some(bytes),
            Self::Ref(_) => None,
        }
    }

    pub(crate) fn bytes_mut(&mut self) -> Option<&mut Vec<i8>> {
        match self {
            Self::Byte(bytes) => Some(bytes),
            Self::Ref(_) => None,
        }
    }

    /// The values the heap collector traces: a typed array holds no references.
    pub(crate) fn references(&self) -> &[Value] {
        match self {
            Self::Ref(values) => values,
            Self::Byte(_) => &[],
        }
    }

    /// `src[src_pos..src_pos + length]` into `self[dst_pos..]` when both are `byte[]`
    /// (`System.arraycopy`, which copies as if through a temporary, so overlap is safe). The caller
    /// checked the bounds. `None` when either is not a `byte[]`.
    pub(crate) fn copy_bytes(
        &mut self,
        dst_pos: usize,
        source: &[i8],
        src_pos: usize,
        length: usize,
    ) -> Option<()> {
        let target = self.bytes_mut()?;
        target
            .get_mut(dst_pos..dst_pos + length)?
            .copy_from_slice(source.get(src_pos..src_pos + length)?);
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `baload` sign-extends, `bastore` keeps the low 8 bits, a `byte[]` costs a byte per element.
    #[test]
    fn a_byte_array_narrows_and_sign_extends() {
        let mut array = ArrayData::of_class("[B", vec![Value::Int(0); 3]);
        assert!(array.store(0, Value::Int(200)));
        assert!(array.store(1, Value::Int(300)));
        assert!(array.store(2, Value::Int(-129)));
        assert_eq!(array.load(0), Some(Value::Int(-56)));
        assert_eq!(array.load(1), Some(Value::Int(44)));
        assert_eq!(array.load(2), Some(Value::Int(127)));
        assert!(!array.store(3, Value::Int(1)));
        assert_eq!(array.bytes().map(<[i8]>::len), Some(3));
        assert_eq!(
            ArrayData::zeroed_bytes(4 << 20)
                .unwrap()
                .bytes()
                .map(<[i8]>::len),
            Some(4 << 20)
        );
    }

    /// Only a `byte[]` of `int`s is typed; anything else keeps its values unchanged.
    #[test]
    fn other_arrays_keep_their_values() {
        let refs = vec![Value::Ref(Some("x".into()))];
        assert_eq!(
            ArrayData::of_class("[B", refs.clone()),
            ArrayData::Ref(refs.clone())
        );
        assert_eq!(
            ArrayData::of_class("[I", vec![Value::Int(300)]).load(0),
            Some(Value::Int(300))
        );
    }

    #[test]
    fn boolean_array_bastore_masks_to_one() {
        let mut array = ArrayData::of_class("[Z", vec![Value::Int(0)]);
        assert!(array.boolean_store(0, Value::Int(300)));
        assert_eq!(array.boolean_load(0), Some(Value::Int(0)));
        assert!(array.boolean_store(0, Value::Int(301)));
        assert_eq!(array.boolean_load(0), Some(Value::Int(1)));
    }
}
