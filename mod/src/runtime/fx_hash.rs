//! Fast, deterministic 64-bit non-cryptographic hasher for internal runtime caches
//! and GC tables keyed by JVM class names, member descriptors, and object markers.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

const K: u64 = 0x517cc1b727220a95;

#[derive(Default)]
pub(crate) struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add_to_hash(&mut self, value: u64) {
        self.hash = self.hash.rotate_left(5).bitxor(value).wrapping_mul(K);
    }
}

trait BitXorExt {
    fn bitxor(self, rhs: Self) -> Self;
}

impl BitXorExt for u64 {
    #[inline]
    fn bitxor(self, rhs: Self) -> Self {
        self ^ rhs
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            let chunk = u64::from_ne_bytes(bytes[..8].try_into().unwrap());
            self.add_to_hash(chunk);
            bytes = &bytes[8..];
        }
        if bytes.len() >= 4 {
            let chunk = u32::from_ne_bytes(bytes[..4].try_into().unwrap());
            self.add_to_hash(u64::from(chunk));
            bytes = &bytes[4..];
        }
        if bytes.len() >= 2 {
            let chunk = u16::from_ne_bytes(bytes[..2].try_into().unwrap());
            self.add_to_hash(u64::from(chunk));
            bytes = &bytes[2..];
        }
        if let Some(&b) = bytes.first() {
            self.add_to_hash(u64::from(b));
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add_to_hash(u64::from(i));
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add_to_hash(u64::from(i));
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add_to_hash(u64::from(i));
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add_to_hash(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add_to_hash(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub(crate) type FxBuildHasher = BuildHasherDefault<FxHasher>;
pub(crate) type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;
pub(crate) type FxHashSet<V> = HashSet<V, FxBuildHasher>;
