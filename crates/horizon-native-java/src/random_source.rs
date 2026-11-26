//! Seeded Minecraft 26.3 RandomSource implementations.
//!
//! Only explicit long-seeded factories and constructors are modeled. Unseeded
//! factories stay on the counted host path because their seeds come from game
//! or thread state.

use crate::host_ref::{
    ClassHierarchy, ClassSide, HostKind, HostRef, Methods, exact_class_rules_out,
};

use crate::{MethodRef, NativeError, StdHost, Value};

pub(crate) fn is_bit_conversion(r: &MethodRef) -> bool {
    matches!(
        (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()),
        ("java/lang/Double", "doubleToRawLongBits", "(D)J")
            | ("java/lang/Double", "longBitsToDouble", "(J)D")
            | ("java/lang/Float", "floatToRawIntBits", "(F)I")
            | ("java/lang/Float", "intBitsToFloat", "(I)F")
    )
}

pub(crate) fn bit_conversion(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    // These conversions are a one-shot implementation detail of a seeded
    // RandomSource draw. Never turn the JDK's global Double/Float methods
    // into a fabricated host model for unrelated calls or later callbacks.
    let previous = host.random_bit_scope.take()?;
    let follows = matches!(
        (
            r.owner.as_str(),
            r.name.as_str(),
            r.descriptor.as_str(),
            previous.name.as_str(),
            previous.descriptor.as_str(),
        ),
        (
            "java/lang/Double",
            "doubleToRawLongBits",
            "(D)J",
            "nextDouble",
            "()D"
        ) | (
            "java/lang/Float",
            "floatToRawIntBits",
            "(F)I",
            "nextFloat",
            "()F"
        ) | (
            "java/lang/Double",
            "longBitsToDouble",
            "(J)D",
            "nextLong",
            "()J"
        ) | (
            "java/lang/Float",
            "intBitsToFloat",
            "(I)F",
            "nextInt",
            "()I"
        )
    );
    if !follows {
        return None;
    }
    match (
        r.owner.as_str(),
        r.name.as_str(),
        r.descriptor.as_str(),
        args,
    ) {
        ("java/lang/Double", "doubleToRawLongBits", "(D)J", [Value::Float(value)]) => {
            Some(Ok(Value::Long(value.to_bits() as i64)))
        }
        ("java/lang/Double", "longBitsToDouble", "(J)D", [Value::Long(value)]) => {
            Some(Ok(Value::Float(f64::from_bits(*value as u64))))
        }
        ("java/lang/Float", "floatToRawIntBits", "(F)I", [Value::Float(value)]) => {
            Some(Ok(Value::Int((*value as f32).to_bits() as i32)))
        }
        ("java/lang/Float", "intBitsToFloat", "(I)F", [Value::Int(value)]) => {
            Some(Ok(Value::Float(f64::from(f32::from_bits(*value as u32)))))
        }
        _ => None,
    }
}

pub(crate) const RANDOM_SOURCE: &str = "net/minecraft/util/RandomSource";
const LEGACY: &str = "net/minecraft/world/level/levelgen/LegacyRandomSource";
const JAVA_RANDOM: &str = "java/util/Random";

/// Whether `class` is `java.util.Random`, whose objects the host models under jar-object keys
/// numbered by the running heap's allocator (`Host::random_new`).
pub(crate) fn is_java_random(class: &str) -> bool {
    class == JAVA_RANDOM
}
const SINGLE: &str = "net/minecraft/world/level/levelgen/SingleThreadedRandomSource";
const XORO: &str = "net/minecraft/world/level/levelgen/XoroshiroRandomSource";
const LEGACY_POS: &str =
    "net/minecraft/world/level/levelgen/LegacyRandomSource$LegacyPositionalRandomFactory";
const XORO_POS: &str =
    "net/minecraft/world/level/levelgen/XoroshiroRandomSource$XoroshiroPositionalRandomFactory";
const POSITIONAL: &str = "net/minecraft/world/level/levelgen/PositionalRandomFactory";
const BIT_RANDOM: &str = "net/minecraft/world/level/levelgen/BitRandomSource";

#[derive(Debug, Clone)]
pub(crate) enum State {
    Legacy {
        seed: u64,
    },
    Java {
        seed: u64,
    },
    Single {
        seed: u64,
    },
    Xoroshiro {
        lo: u64,
        hi: u64,
    },
    LegacyPositional {
        seed: i64,
    },
    XoroshiroPositional {
        lo: u64,
        hi: u64,
    },
    /// `Level.random` of the server level of `dimension`, answered by the
    /// embedder's live stream while it is lent (`level_random.rs`). Decaf
    /// holds no state for it: outside a lend its draws are counted.
    LevelLent {
        dimension: String,
        instance: String,
    },
}

impl State {
    fn class(&self) -> &'static str {
        match self {
            Self::Legacy { .. } => LEGACY,
            Self::Java { .. } => JAVA_RANDOM,
            Self::Single { .. } => SINGLE,
            Self::Xoroshiro { .. } => XORO,
            Self::LegacyPositional { .. } => LEGACY_POS,
            Self::XoroshiroPositional { .. } => XORO_POS,
            // `Level.random = RandomSource.create()`: a `LegacyRandomSource`.
            Self::LevelLent { .. } => LEGACY,
        }
    }
}

impl StdHost {
    pub(crate) fn generate_unique_seed(&mut self) -> i64 {
        self.random_seed_uniquifier = self
            .random_seed_uniquifier
            .wrapping_mul(1_181_783_497_276_652_981);
        (self.random_seed_uniquifier as i64) ^ self.random_time_origin.elapsed().as_nanos() as i64
    }

    /// The host's object for a `new` of a modelled random class. A `java.util.Random` is a
    /// jar object (`java/util/Random#n`, its `Object` methods and identity the runtime's), so
    /// its number is the running heap's next one (`next_object`), never a counter of the
    /// host's: with two counters, a `Random::new` constructor reference (the runtime's
    /// allocator) took a seeded `new Random(1L)`'s key and reseeded it (#667 review).
    pub(crate) fn random_new(&mut self, class: &str, next_object: &mut usize) -> Option<Value> {
        let state = match class {
            JAVA_RANDOM => {
                // Exhausted: the runtime refuses the allocation (`Runtime::allocate_new`).
                if *next_object == usize::MAX {
                    return None;
                }
                let key = format!("{JAVA_RANDOM}#{next_object}");
                *next_object += 1;
                self.random_sources.insert(
                    key.clone(),
                    State::Java {
                        seed: scramble_legacy(0),
                    },
                );
                return Some(crate::jar_object::handle(key));
            }
            LEGACY => State::Legacy {
                seed: scramble_legacy(0),
            },
            SINGLE => State::Single {
                seed: scramble_legacy(0),
            },
            XORO => {
                let (lo, hi) = xoro_seed(0);
                State::Xoroshiro { lo, hi }
            }
            _ => return None,
        };
        // Out of numbers (counted): the `new` allocates a plain object, whose calls count.
        self.random_handle(state).ok()
    }

    /// A new `RandomSource` handle for `state`; `None` once the numbers are exhausted
    /// (counted), when the caller keeps the stream unknown.
    pub(crate) fn make_random_handle(&mut self, state: State) -> Option<Value> {
        self.random_handle(state).ok()
    }

    pub(crate) fn draw_random_handle_long(&mut self, handle: &Value) -> Option<i64> {
        let (key, mut state) = self.random_state_mut(handle)?;
        // Decaf holds no copy of a lent level stream to draw from.
        if matches!(state, State::LevelLent { .. }) {
            return None;
        }
        let value = next_long(&mut state);
        self.save_random(key, state);
        Some(value)
    }

    /// Forget the state behind a random handle whose position is no longer
    /// known (`level_sound.rs`): with no row, every later call on the handle
    /// falls through to the counted unknown path instead of answering.
    pub(crate) fn forget_random_state(&mut self, handle: &Value) {
        if let Some(key) = Self::random_key(handle) {
            self.random_sources.remove(&key);
        }
    }

    /// The handle under `key`, made with `state` the first time: one object
    /// however often Java asks (`Level.getRandom()` of a lent level).
    pub(crate) fn keyed_random_handle(&mut self, key: String, state: State) -> Value {
        self.random_sources.entry(key.clone()).or_insert(state);
        Value::Host(HostRef::new(HostKind::RandomSource, key))
    }

    /// A new `RandomSource` handle (its own [`HostKind`], never a jar object) for `state`.
    fn random_handle(&mut self, state: State) -> Result<Value, NativeError> {
        let class = state.class();
        let key = self.next_random_key(class)?;
        self.random_sources.insert(key.clone(), state);
        Ok(Value::Host(HostRef::new(HostKind::RandomSource, key)))
    }

    /// A new `RandomSource` handle's key, `<class>#random<n>`:
    /// - `n` comes from a counter that only grows. Numbering by the table's size reused a live
    ///   source's key once another was collected or forgotten (`#0`, `#1`; `#0` goes; the next
    ///   is `#1` again), and the new source overwrote the live one's state.
    /// - `random` keeps the key out of the jar's allocation keys (`class#<digits>`, numbered by
    ///   the heap): the collector frees these with their own family (`heap::collectable`), and
    ///   no jar object's number can name one.
    ///
    /// `u64::MAX` is never handed out: past it the source is refused (counted), never given a
    /// number another source holds.
    fn next_random_key(&mut self, class: &str) -> Result<String, NativeError> {
        let number = self.next_random_source;
        if number == u64::MAX {
            let _ = self.unknown_host_call(&MethodRef {
                owner: class.to_owned(),
                name: "<new>".into(),
                descriptor: ": random source numbers exhausted".into(),
            });
            return Err(NativeError::Metadata(format!(
                "the host's random source numbers are exhausted; a new {class} would reuse a live source's key"
            )));
        }
        self.next_random_source = number + 1;
        Ok(format!("{class}#random{number}"))
    }

    fn random_key(value: &Value) -> Option<String> {
        match value {
            Value::Host(handle) if handle.kind() == HostKind::RandomSource => {
                Some(handle.key().to_owned())
            }
            Value::Host(handle)
                if handle.kind() == HostKind::JarObject
                    && class_of_key(handle.key()) == JAVA_RANDOM =>
            {
                Some(handle.key().to_owned())
            }
            _ => None,
        }
    }

    fn random_state_mut(&mut self, receiver: &Value) -> Option<(String, State)> {
        let key = Self::random_key(receiver)?;
        let state = self.random_sources.get(&key)?.clone();
        Some((key, state))
    }

    fn save_random(&mut self, key: String, state: State) {
        self.random_sources.insert(key, state);
    }

    pub(crate) fn random_static(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if r.owner != RANDOM_SOURCE {
            return None;
        }
        match (r.name.as_str(), r.descriptor.as_str(), args) {
            ("create", "(J)Lnet/minecraft/util/RandomSource;", [seed]) => {
                Some(self.random_seeded(LEGACY, as_long(seed)?))
            }
            ("create", "()Lnet/minecraft/util/RandomSource;", []) => {
                let seed = self.generate_unique_seed();
                Some(self.random_seeded(LEGACY, seed))
            }
            ("createThreadLocalInstance", "(J)Lnet/minecraft/util/RandomSource;", [seed]) => {
                Some(self.random_seeded(SINGLE, as_long(seed)?))
            }
            // create(), createThreadSafe() and createThreadLocalInstance() have
            // nondeterministic/game-owned seeds and intentionally remain counted.
            _ => None,
        }
    }

    fn random_seeded(&mut self, class: &str, seed: i64) -> Result<Value, NativeError> {
        let state = match class {
            LEGACY => State::Legacy {
                seed: scramble_legacy(seed),
            },
            SINGLE => State::Single {
                seed: scramble_legacy(seed),
            },
            XORO => {
                let (lo, hi) = xoro_seed(seed);
                State::Xoroshiro { lo, hi }
            }
            _ => return Err(NativeError::Metadata("unknown RandomSource class".into())),
        };
        self.random_handle(state)
    }

    /// A call on a random source handle. A handle derived from a level stream whose position
    /// wasn't vanilla's (`level_random.rs`: a fork drawn while the stream was unknown, or
    /// after a deferred effect) has a seed that may differ from vanilla's: its calls are
    /// answered from its own seeded state but counted (`LEVEL_STREAM_UNKNOWN`), and a handle
    /// it hands out (`fork`, `forkPositional`, `at`, `fromSeed`) is derived the same way.
    pub(crate) fn random_instance(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let derived =
            Self::random_key(receiver).filter(|key| self.random_derived_unknown.contains(key));
        let result = self.random_instance_answer(r, receiver, args);
        if derived.is_some()
            && let Some(Ok(value)) = &result
        {
            self.note_derived_unknown_call(value);
        }
        result
    }

    /// Count a call answered on a derived-unknown handle, and derive what it handed out.
    #[inline(never)]
    fn note_derived_unknown_call(&mut self, value: &Value) {
        *self
            .unknown_host_calls
            .entry(crate::level_random::LEVEL_STREAM_UNKNOWN.to_owned())
            .or_default() += 1;
        if let Some(child) = Self::random_key(value) {
            self.random_derived_unknown.insert(child);
        }
    }

    /// Mark `handle`'s source as derived from a level stream whose position wasn't vanilla's
    /// (see [`Self::random_instance`]).
    pub(crate) fn mark_random_derived_unknown(&mut self, handle: &Value) {
        if let Some(key) = Self::random_key(handle) {
            self.random_derived_unknown.insert(key);
        }
    }

    #[inline(never)]
    fn random_instance_answer(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        self.random_bit_scope = None;
        // A direct constructor is reached on a JarObject before any state row
        // exists; random_new allocated the row at the new opcode.
        let (key, mut state) = self.random_state_mut(receiver)?;
        if let State::LevelLent {
            dimension,
            instance,
        } = &state
        {
            // The live level stream, while it is lent; counted otherwise.
            return self.lent_random_call(dimension, instance, r, args);
        }
        if r.name == "<init>" && r.descriptor == "(JJ)V" {
            let (lo, hi) = match args {
                [lo, hi] => match (as_long(lo), as_long(hi)) {
                    (Some(lo), Some(hi)) => (lo as u64, hi as u64),
                    _ => {
                        return Some(Err(NativeError::Metadata(
                            "Xoroshiro constructor seeds are not long".into(),
                        )));
                    }
                },
                _ => {
                    return Some(Err(NativeError::Metadata(
                        "Xoroshiro constructor arity".into(),
                    )));
                }
            };
            if !matches!(state, State::Xoroshiro { .. }) {
                return Some(Err(NativeError::Metadata(
                    "long-pair constructor on non-Xoroshiro source".into(),
                )));
            }
            state = xoro_state(lo, hi);
            self.save_random(key, state);
            return Some(Ok(Value::Void));
        }
        if r.name == "<init>" && r.descriptor == "()V" && matches!(state, State::Java { .. }) {
            self.random_sources.remove(&key);
            return None;
        }
        if r.name == "<init>" && r.descriptor == "(J)V" {
            let seed = match args {
                [value] => match as_long(value) {
                    Some(seed) => seed,
                    None => {
                        return Some(Err(NativeError::Metadata(
                            "RandomSource seed is not long".into(),
                        )));
                    }
                },
                _ => {
                    return Some(Err(NativeError::Metadata(
                        "RandomSource constructor arity".into(),
                    )));
                }
            };
            state = match state {
                State::Legacy { .. } => State::Legacy {
                    seed: scramble_legacy(seed),
                },
                State::Java { .. } => State::Java {
                    seed: scramble_legacy(seed),
                },
                State::Single { .. } => State::Single {
                    seed: scramble_legacy(seed),
                },
                State::Xoroshiro { .. } => {
                    let (lo, hi) = xoro_seed(seed);
                    xoro_state(lo, hi)
                }
                positional => positional,
            };
            self.save_random(key, state);
            return Some(Ok(Value::Void));
        }
        if matches!(
            state,
            State::LegacyPositional { .. } | State::XoroshiroPositional { .. }
        ) {
            return Some(self.random_positional_call(key, state, r, args));
        }
        // java.util.Random shares the 48-bit LCG with LegacyRandomSource, but not
        // every method: the nextInt(origin, bound) it inherits from RandomGenerator
        // draws differently for power-of-two and wide ranges (checked against a
        // real JDK), and its messages differ. Answer only the methods Random
        // implements itself on the LCG; leave every other call counted.
        if matches!(state, State::Java { .. }) && !java_random_answers(r) {
            return None;
        }
        let result = match (r.name.as_str(), r.descriptor.as_str()) {
            ("setSeed", "(J)V") => {
                let seed = as_long(args.first()?)?;
                state = match state {
                    State::Legacy { .. } => State::Legacy {
                        seed: scramble_legacy(seed),
                    },
                    State::Java { .. } => State::Java {
                        seed: scramble_legacy(seed),
                    },
                    State::Single { .. } => State::Single {
                        seed: scramble_legacy(seed),
                    },
                    State::Xoroshiro { .. } => {
                        let (lo, hi) = xoro_seed(seed);
                        xoro_state(lo, hi)
                    }
                    positional => positional,
                };
                Ok(Value::Void)
            }
            ("nextInt", "()I") => Ok(Value::Int(next_int(&mut state))),
            ("nextInt", "(I)I") => {
                let bound = as_int(args.first()?)?;
                next_int_bound(&mut state, bound)
            }
            ("nextIntBetweenInclusive", "(II)I") => {
                let min = as_int(args.first()?)?;
                let max = as_int(args.get(1)?)?;
                let bound = max.wrapping_sub(min).wrapping_add(1);
                next_int_bound(&mut state, bound).and_then(|value| match value {
                    Value::Int(value) => Ok(Value::Int(value.wrapping_add(min))),
                    _ => Err(metadata("nextInt returned non-int")),
                })
            }
            ("nextInt", "(II)I") => {
                let origin = as_int(args.first()?)?;
                let bound = as_int(args.get(1)?)?;
                if origin >= bound {
                    Err(iae("bound - origin is non positive"))
                } else {
                    next_int_bound(&mut state, bound.wrapping_sub(origin)).and_then(|value| {
                        match value {
                            Value::Int(value) => Ok(Value::Int(value.wrapping_add(origin))),
                            _ => Err(metadata("nextInt returned non-int")),
                        }
                    })
                }
            }
            ("nextLong", "()J") => Ok(Value::Long(next_long(&mut state))),
            ("nextBoolean", "()Z") => Ok(Value::Int(i32::from(next_boolean(&mut state)))),
            ("nextFloat", "()F") => Ok(Value::Float(f64::from(next_float(&mut state)))),
            ("nextDouble", "()D") => Ok(Value::Float(next_double(&mut state))),
            // HotSpot 25's x86 Math.sqrt/log path has platform intrinsics whose
            // last-bit behavior is not available in the bounded host. Do not
            // invent a Gaussian stream: leave the call counted.
            ("nextGaussian", "()D") => return None,
            ("consumeCount", "(I)V") => {
                let count = as_int(args.first()?)?;
                for _ in 0..count.max(0) {
                    let _ = next_int(&mut state);
                }
                Ok(Value::Void)
            }
            ("triangle", "(DD)D") => {
                let mean = as_float(args.first()?)?;
                let deviation = as_float(args.get(1)?)?;
                Ok(Value::Float(
                    mean + deviation * (next_double(&mut state) - next_double(&mut state)),
                ))
            }
            ("triangle", "(FF)F") => {
                let mean = as_float(args.first()?)? as f32;
                let deviation = as_float(args.get(1)?)? as f32;
                Ok(Value::Float(f64::from(
                    mean + deviation * (next_float(&mut state) - next_float(&mut state)),
                )))
            }
            ("fork", "()Lnet/minecraft/util/RandomSource;") => {
                let child = match state {
                    State::Legacy { .. } => State::Legacy {
                        seed: scramble_legacy(next_long(&mut state)),
                    },
                    State::Single { .. } => State::Single {
                        seed: scramble_legacy(next_long(&mut state)),
                    },
                    State::Xoroshiro { .. } => {
                        let lo = next_long(&mut state) as u64;
                        let hi = next_long(&mut state) as u64;
                        xoro_state(lo, hi)
                    }
                    _ => return Some(Err(NativeError::Metadata("positional fork".into()))),
                };
                let value = match self.random_handle(child) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                self.save_random(key, state);
                return Some(Ok(value));
            }
            (
                "forkPositional",
                "()Lnet/minecraft/world/level/levelgen/PositionalRandomFactory;",
            ) => {
                let child = match state {
                    State::Legacy { .. } | State::Single { .. } => State::LegacyPositional {
                        seed: next_long(&mut state),
                    },
                    State::Xoroshiro { .. } => State::XoroshiroPositional {
                        lo: next_long(&mut state) as u64,
                        hi: next_long(&mut state) as u64,
                    },
                    _ => return Some(Err(NativeError::Metadata("positional fork".into()))),
                };
                let value = match self.random_handle(child) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                self.save_random(key, state);
                return Some(Ok(value));
            }
            _ => return None,
        };
        self.save_random(key, state);
        if matches!(
            (r.name.as_str(), r.descriptor.as_str()),
            ("nextInt", "()I") | ("nextLong", "()J") | ("nextFloat", "()F") | ("nextDouble", "()D")
        ) {
            self.random_bit_scope = Some(r.clone());
        }
        Some(result)
    }

    fn random_positional_call(
        &mut self,
        _key: String,
        state: State,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("fromSeed", "(J)Lnet/minecraft/util/RandomSource;") => {
                let seed = as_long(args.first().ok_or_else(|| metadata("missing seed"))?)
                    .ok_or_else(|| metadata("seed is not long"))?;
                let next = match state {
                    State::LegacyPositional { .. } => State::Legacy {
                        seed: scramble_legacy(seed),
                    },
                    State::XoroshiroPositional { .. } => {
                        let (lo, hi) = match state {
                            State::XoroshiroPositional { lo, hi } => (lo, hi),
                            _ => unreachable!(),
                        };
                        xoro_state(seed as u64 ^ lo, seed as u64 ^ hi)
                    }
                    _ => unreachable!(),
                };
                self.random_handle(next)
            }
            ("at", "(III)Lnet/minecraft/util/RandomSource;") => {
                let x = arg_int(args.first())?;
                let y = arg_int(args.get(1))?;
                let z = arg_int(args.get(2))?;
                let mixed = position_seed(x, y, z);
                let next = match state {
                    State::LegacyPositional { seed } => State::Legacy {
                        seed: scramble_legacy(mixed ^ seed),
                    },
                    State::XoroshiroPositional { lo, hi } => xoro_state(mixed as u64 ^ lo, hi),
                    _ => unreachable!(),
                };
                self.random_handle(next)
            }
            _ => self.unknown_host_call(r),
        }
    }
}

fn metadata(text: &str) -> NativeError {
    NativeError::Metadata(text.into())
}
fn as_long(value: &Value) -> Option<i64> {
    match value {
        Value::Long(v) => Some(*v),
        Value::Int(v) => Some(i64::from(*v)),
        _ => None,
    }
}
fn arg_int(value: Option<&Value>) -> Result<i32, NativeError> {
    value
        .and_then(as_int)
        .ok_or_else(|| metadata("random int argument"))
}
fn as_int(value: &Value) -> Option<i32> {
    match value {
        Value::Int(v) => Some(*v),
        _ => None,
    }
}
fn as_float(value: &Value) -> Option<f64> {
    match value {
        Value::Float(v) => Some(*v),
        Value::Int(v) => Some(f64::from(*v)),
        _ => None,
    }
}
pub(crate) fn iae(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message.into()),
        frame: None,
    }
}
/// The java.util.Random methods answered on its LCG state. The rest (nextInt(II),
/// nextLong(J), nextGaussian, streams...) stay counted.
fn java_random_answers(r: &MethodRef) -> bool {
    matches!(
        (r.name.as_str(), r.descriptor.as_str()),
        ("setSeed", "(J)V")
            | ("nextInt", "()I")
            | ("nextInt", "(I)I")
            | ("nextLong", "()J")
            | ("nextBoolean", "()Z")
            | ("nextFloat", "()F")
            | ("nextDouble", "()D")
    )
}
pub(crate) fn legacy_state(seed: i64) -> State {
    State::Legacy {
        seed: scramble_legacy(seed),
    }
}

pub(crate) fn draw_next_long(state: &mut State) -> i64 {
    next_long(state)
}

fn scramble_legacy(seed: i64) -> u64 {
    ((seed as u64) ^ 0x5deece66d) & ((1u64 << 48) - 1)
}
fn xoro_seed(seed: i64) -> (u64, u64) {
    let lo = seed as u64 ^ 0x6a09e667f3bcc909;
    (mix13(lo), mix13(lo.wrapping_add(0x9e3779b97f4a7c15)))
}
fn xoro_state(lo: u64, hi: u64) -> State {
    if lo | hi == 0 {
        State::Xoroshiro {
            lo: 0x9e3779b97f4a7c15,
            hi: 0x6a09e667f3bcc909,
        }
    } else {
        State::Xoroshiro { lo, hi }
    }
}
fn mix13(mut v: u64) -> u64 {
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d049bb133111eb);
    v ^ (v >> 31)
}

fn next_bits(seed: &mut u64, count: u32) -> i32 {
    *seed = seed.wrapping_mul(0x5deece66d).wrapping_add(11) & ((1u64 << 48) - 1);
    (*seed >> (48 - count)) as i32
}
fn next_int(state: &mut State) -> i32 {
    match state {
        State::Legacy { seed, .. } | State::Java { seed, .. } | State::Single { seed, .. } => {
            next_bits(seed, 32)
        }
        State::Xoroshiro { .. } => next_long(state) as i32,
        _ => 0,
    }
}
fn next_int_bound(state: &mut State, bound: i32) -> Result<Value, NativeError> {
    if bound <= 0 {
        // Minecraft's RandomSource capitalises its message; java.util.Random does not.
        return Err(iae(match state {
            State::Java { .. } => "bound must be positive",
            _ => "Bound must be positive",
        }));
    }
    let v = match state {
        State::Legacy { seed, .. } | State::Java { seed, .. } | State::Single { seed, .. } => {
            if bound & (bound - 1) == 0 {
                ((i64::from(bound) * i64::from(next_bits(seed, 31))) >> 31) as i32
            } else {
                loop {
                    let bits = next_bits(seed, 31);
                    let value = bits % bound;
                    if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                        break value;
                    }
                }
            }
        }
        State::Xoroshiro { .. } => {
            let b = bound as u32;
            let t = b.wrapping_neg() % b;
            loop {
                let p = u64::from(next_int(state) as u32) * u64::from(b);
                if p as u32 >= t {
                    break (p >> 32) as i32;
                }
            }
        }
        _ => 0,
    };
    Ok(Value::Int(v))
}
fn next_long(state: &mut State) -> i64 {
    match state {
        State::Legacy { seed, .. } | State::Java { seed, .. } | State::Single { seed, .. } => {
            (i64::from(next_bits(seed, 32)) << 32).wrapping_add(i64::from(next_bits(seed, 32)))
        }
        State::Xoroshiro { lo, hi, .. } => {
            let l = *lo;
            let h = *hi ^ l;
            let r = l.wrapping_add(*hi).rotate_left(17).wrapping_add(l);
            *lo = l.rotate_left(49) ^ h ^ (h << 21);
            *hi = h.rotate_left(28);
            r as i64
        }
        _ => 0,
    }
}
fn next_boolean(state: &mut State) -> bool {
    match state {
        State::Legacy { seed, .. } | State::Java { seed, .. } | State::Single { seed, .. } => {
            next_bits(seed, 1) != 0
        }
        State::Xoroshiro { .. } => next_long(state) & 1 != 0,
        _ => false,
    }
}
fn next_float(state: &mut State) -> f32 {
    match state {
        State::Legacy { seed, .. } | State::Java { seed, .. } | State::Single { seed, .. } => {
            next_bits(seed, 24) as f32 / 16_777_216.0
        }
        State::Xoroshiro { .. } => (next_long(state) as u64 >> 40) as f32 / 16_777_216.0,
        _ => 0.0,
    }
}
fn next_double(state: &mut State) -> f64 {
    match state {
        State::Legacy { seed, .. } | State::Java { seed, .. } | State::Single { seed, .. } => {
            (((i64::from(next_bits(seed, 26)) << 27) + i64::from(next_bits(seed, 27))) as f64)
                / 9_007_199_254_740_992.0
        }
        State::Xoroshiro { .. } => (next_long(state) as u64 >> 11) as f64 / 9_007_199_254_740_992.0,
        _ => 0.0,
    }
}
fn position_seed(x: i32, y: i32, z: i32) -> i64 {
    let mixed =
        (x.wrapping_mul(3_129_871) as i64) ^ (z as i64).wrapping_mul(116_129_781) ^ (y as i64);
    mixed
        .wrapping_mul(mixed)
        .wrapping_mul(42_317_861)
        .wrapping_add(mixed.wrapping_mul(11))
        >> 16
}

fn random_instance_of(class: &str, wanted: &str, h: &dyn ClassHierarchy) -> Option<bool> {
    if wanted == class || wanted == "java/lang/Object" {
        return Some(true);
    };
    let yes = match class {
        LEGACY | SINGLE => wanted == RANDOM_SOURCE || wanted == BIT_RANDOM,
        XORO => wanted == RANDOM_SOURCE,
        LEGACY_POS | XORO_POS => wanted == POSITIONAL,
        _ => false,
    };
    if yes {
        return Some(true);
    };
    if h.is_instance(class, wanted) == Some(true) {
        return Some(true);
    };
    (h.is_interface(wanted) == Some(false) || exact_class_rules_out(class, wanted)).then_some(false)
}
pub(crate) fn class_of_key(key: &str) -> &str {
    key.split_once('#').map_or(key, |(c, _)| c)
}
fn owns(c: &str) -> bool {
    matches!(c, LEGACY | SINGLE | XORO | LEGACY_POS | XORO_POS)
}
fn superclass(_: &str) -> Option<String> {
    Some("java/lang/Object".into())
}
fn is_interface(_: &str) -> Option<bool> {
    Some(false)
}
fn classes() -> Vec<String> {
    vec![
        LEGACY.into(),
        SINGLE.into(),
        XORO.into(),
        LEGACY_POS.into(),
        XORO_POS.into(),
    ]
}
fn class_instance_of(c: &str, w: &str, h: &dyn ClassHierarchy) -> Option<bool> {
    random_instance_of(c, w, h)
}
pub(crate) const METHODS: Methods = Methods {
    instance_of: |handle, w, h| random_instance_of(class_of_key(handle.key()), w, h),
    ..crate::host_events::DECLARED_TYPE_METHODS
};
pub(crate) const CLASS_SIDE: ClassSide = ClassSide {
    owns,
    instance_of: class_instance_of,
    superclass,
    is_interface,
    classes,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Host;

    #[test]
    fn random_bit_conversion_exact_non_nan_and_counted_nan() {
        let mut host = StdHost::new();
        let raw = MethodRef {
            owner: "java/lang/Double".into(),
            name: "doubleToRawLongBits".into(),
            descriptor: "(D)J".into(),
        };
        let exact = host.invoke(&raw, &[Value::Float(1.25)]).unwrap();
        assert_eq!(exact, Value::Long(0x3ff4_0000_0000_0000));
        assert!(host.unknown_host_calls().is_empty());
        let counted = host.invoke(&raw, &[Value::Float(f64::NAN)]).unwrap();
        assert!(matches!(counted, Value::Long(0)));
        assert_eq!(
            host.unknown_host_calls(),
            vec![("java/lang/Double#doubleToRawLongBits(D)J".into(), 1),]
        );
    }

    /// A source made after another was forgotten never takes a live source's key: numbered by
    /// the table's size, the third source here was `#1` again and overwrote the second's state.
    #[test]
    fn a_new_source_never_reuses_a_live_sources_key() {
        let mut host = StdHost::new();
        let key = |value: &Value| match value {
            Value::Host(handle) => handle.key().to_owned(),
            other => panic!("not a host handle: {other:?}"),
        };
        let first = host.random_seeded(LEGACY, 1).unwrap();
        let second = host.random_seeded(LEGACY, 2).unwrap();
        let before = host.draw_random_handle_long(&second);
        host.forget_random_state(&first);
        let third = host.random_seeded(LEGACY, 3).unwrap();
        assert_ne!(key(&third), key(&second));
        // The second source keeps its own sequence: a fresh `LegacyRandomSource(2)`'s next.
        let mut fresh = StdHost::new();
        let replay = fresh.random_seeded(LEGACY, 2).unwrap();
        fresh.draw_random_handle_long(&replay);
        assert_eq!(
            host.draw_random_handle_long(&second),
            fresh.draw_random_handle_long(&replay)
        );
        assert!(before.is_some());
    }

    /// A source's key is `<class>#random<n>`: no allocation key (`class#<digits>`), so no jar
    /// object's number names it; and past the last number a new source is refused, counted.
    #[test]
    fn source_keys_are_their_own_family_and_refuse_past_the_last_number() {
        let mut host = StdHost::new();
        let Value::Host(handle) = host.random_seeded(LEGACY, 7).unwrap() else {
            panic!("a seeded source is a host handle")
        };
        assert_eq!(handle.key(), format!("{LEGACY}#random0"));
        assert!(crate::jar_object::parse_key(handle.key()).is_none());
        host.next_random_source = u64::MAX;
        let refused = host.random_seeded(LEGACY, 8).unwrap_err().to_string();
        assert!(
            refused.contains("random source numbers are exhausted"),
            "{refused}"
        );
        assert_eq!(
            host.unknown_host_calls(),
            vec![(
                format!("{LEGACY}#<new>: random source numbers exhausted"),
                1
            )]
        );
    }

    #[test]
    fn random_state_never_aliases_a_non_random_host_handle() {
        let mut host = StdHost::new();
        let source = host.random_seeded(LEGACY, 123).unwrap();
        let key = match source {
            Value::Host(handle) => handle.key().to_owned(),
            _ => panic!("seeded source was not a host handle"),
        };
        let forged = Value::Host(HostRef::new(HostKind::JarObject, key));
        let call = MethodRef {
            owner: RANDOM_SOURCE.into(),
            name: "nextInt".into(),
            descriptor: "()I".into(),
        };
        let result = host.invoke_instance(&call, &forged, &[]).unwrap();
        assert_eq!(result, Value::Int(0));
        assert_eq!(
            host.unknown_host_calls(),
            vec![("net/minecraft/util/RandomSource#nextInt()I".into(), 1,)]
        );
    }
}
