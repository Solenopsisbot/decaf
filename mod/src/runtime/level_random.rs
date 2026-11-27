//! `Level.getRandom()` on the embedder's live level stream.
//!
//! A server level's `random` field (`Level.random = RandomSource.create()`, a
//! `LegacyRandomSource`) is game state: every block tick, item drop and mob
//! spawn draws it, so Decaf can't keep a copy of it. An embedder that can lend
//! the live stream for one Java call (Sunrise's `LevelRef::with_random`) wraps
//! that call in [`StdHost::with_lent_level_random`]. While the lend lasts:
//!
//! - `getRandom()` on that dimension's server level answers one handle per
//!   dimension ([`State::LevelLent`], key [`lent_key`]), so `level.getRandom()
//!   == level.getRandom()` holds as it does in vanilla, and a handle Java kept
//!   from an earlier call is the same object;
//! - each draw on that handle is answered by the lent stream, at the moment
//!   Java draws, so Java sees exactly the value vanilla's stream would give it
//!   and the stream moves in Java's order.
//!
//! A handle names one *open* of a level, not just its dimension: the embedder
//! gives each lend an `instance` (Sunrise: the world's epoch and the level's
//! generation). A handle Java kept in a static from an earlier world (single
//! player keeps the jars across worlds) is a different object from this
//! world's `getRandom()` and never draws this world's stream: counted.
//!
//! Outside a lend (no lend at all, a lend of another dimension, or an
//! embedder that declined to lend), `getRandom()` stays counted, and a draw on
//! a handle Java stored earlier is counted too: nothing answers it.
//!
//! # A stream whose position is unknown
//!
//! A counted call on a level's handle may have moved vanilla's stream where
//! the live one stayed put: a draw outside a lend (the embedder's stream
//! didn't move), `setSeed`, `next(bits)`, a `consumeCount` over the limit, or
//! any other method Decaf doesn't answer. From then on the live stream is no
//! longer where vanilla's would be, so every later draw from it, in this lend
//! and every later one of that open of the level, is still answered but
//! counted ([`LEVEL_STREAM_UNKNOWN`]), as `level_sound.rs` does for the sound
//! seed streams. A fork drawn from such a stream (or after a deferred effect)
//! takes its seed from a position that may not be vanilla's, so the child, and
//! every source it hands out, is answered from its own seed but counted the
//! same way (`random_source.rs`). Only `Object`'s `equals`, `hashCode`, `getClass` and
//! `toString` are known not to move it. A new open of the level (a new
//! world) is a new stream, known again: the mark is kept by `instance`.
//!
//! # Methods
//!
//! Checked against `javap -c` of 26.3's `RandomSource`, `BitRandomSource` and
//! `LegacyRandomSource` (which overrides only `fork`, `forkPositional`,
//! `setSeed`, `next` and `nextGaussian`):
//!
//! - `nextInt()`, `nextLong()`, `nextBoolean()`, `nextFloat()`,
//!   `nextDouble()`, `nextGaussian()`: one draw of the stream each.
//! - `nextInt(bound)`: `BitRandomSource.nextInt(I)` throws
//!   `IllegalArgumentException("Bound must be positive")` for `bound <= 0`
//!   before drawing; the stream's `next_int_bound` is only asked for a
//!   positive bound.
//! - `nextInt(origin, bound)` (`RandomSource`'s default): throws
//!   `"bound - origin is non positive"` for `origin >= bound`, else
//!   `origin + nextInt(bound - origin)`, where the subtraction wraps (a range
//!   wider than `int` reaches `nextInt(I)` negative and throws there).
//! - `nextIntBetweenInclusive(min, max)`: `nextInt(max - min + 1) + min`, no
//!   check of its own: `max < min` throws from `nextInt(I)`.
//! - `triangle(D, D)` / `triangle(F, F)`: `mean + spread * (a - b)`, `a` drawn
//!   first.
//! - `consumeCount(n)`: `n` times `nextInt()`, up to [`CONSUME_LIMIT`]. A host
//!   call has no access to the interpreter's execution budget, so a larger
//!   count draws nothing and is counted rather than stalling the tick (and the
//!   stream's position is unknown from then on).
//! - `fork()`: one `nextLong()`, then a seeded `LegacyRandomSource` Decaf
//!   keeps itself; `forkPositional()`: one `nextLong()`, then a
//!   `LegacyPositionalRandomFactory` of it.
//! - `setSeed(J)` and `next(I)`: counted, and the stream's position is unknown
//!   from then on. The lend lets Java draw, never reseed, and the stream has
//!   no raw `next(bits)`.
//!
//! # Effects applied after the call
//!
//! Decaf carries some of a call's effects out after the Java call returns. If
//! the embedder's handling of one may draw this same stream, and Java recorded
//! it *before* a later draw of the lent stream, vanilla would draw for it
//! first and Decaf draws for Java first: the two swap. Every effect a call
//! records goes through one choke point ([`StdHost::note_deferred_effect`]);
//! one not on the allow-list of effects known to draw nothing
//! (`live_menu::draws_no_level_random`) marks the lend, and every lent draw
//! after the mark is counted ([`DRAW_AFTER_DEFERRED_EFFECT`]). Off the list:
//!
//! the criteria and the recipe unlock (`InventoryChangedCriterion`,
//! `RecipeCrafted`, `AwardRecipes`): in vanilla each can complete an
//! advancement at once, and `AdvancementRewards.grant` draws the level's
//! stream through its loot (`LootContext$Builder.create`, which falls back to
//! `ServerLevel.getRandom()`) or a reward function.
//!
//! A drop is on the list: in 26.3 it draws only entity randoms
//! (`ItemEntity.<init>(Level,DDD,ItemStack)` its own `Entity.random`,
//! `createItemStackToDrop` the player's), so vanilla's stream doesn't move for
//! it, and what the embedder's drop draws comes after Java's draw in either
//! order. (A bundle's `use` drops its contents, then draws its sound's pitch.)
//!
//! The player's inventory, which Decaf writes back after the call, needs no
//! mark: vanilla's `INVENTORY_CHANGED` fires only from `ServerPlayer$2
//! .slotChanged`, through `broadcastChanges` (a mid-call one records
//! `InventoryChangedCriterion`), and Sunrise's `inventory_changed` at its
//! per-tick check: both after Java's draw.
//!
//! Such a draw, and one from a stream whose position is unknown, is still
//! answered from the real stream, but its value may sit one or more positions
//! off vanilla's, so it counts as any counted call does: an `exact` region
//! that ran it fails, and what Java built on it is never claimed exact.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::random_source::{self, State};
use crate::runtime::{MethodRef, NativeError, StdHost, Value};

/// A level's live random stream, lent for one Java call
/// ([`StdHost::with_lent_level_random`]). Each method is one draw of
/// vanilla's `LegacyRandomSource` method of the same name.
pub trait LevelRandomStream {
    /// `nextInt()`.
    fn next_int(&mut self) -> i32;
    /// `nextInt(bound)`, for `bound > 0` only (Decaf throws for the rest
    /// before asking).
    fn next_int_bound(&mut self, bound: i32) -> i32;
    /// `nextLong()`.
    fn next_long(&mut self) -> i64;
    /// `nextBoolean()`.
    fn next_boolean(&mut self) -> bool;
    /// `nextFloat()`.
    fn next_float(&mut self) -> f32;
    /// `nextDouble()`.
    fn next_double(&mut self) -> f64;
    /// `nextGaussian()`.
    fn next_gaussian(&mut self) -> f64;
}

/// The counted key of a lent draw made after an effect the embedder may
/// carry out by drawing the same stream (see the module docs).
pub const DRAW_AFTER_DEFERRED_EFFECT: &str =
    "net/minecraft/util/RandomSource#<level stream draw after a deferred effect that may draw it>";

/// The counted key of a draw from a level stream whose position is no longer
/// vanilla's (see the module docs).
pub const LEVEL_STREAM_UNKNOWN: &str =
    "net/minecraft/util/RandomSource#<level stream draw after its position became unknown>";

/// Whether a counted call's `key` is one of the level-stream draws above:
/// answered from the live stream, so no unmodelled host call, but counted
/// because its position in that stream may be off vanilla's. An embedder
/// reports them as counted calls, never as unmodelled ones.
pub fn is_counted_level_stream_draw(key: &str) -> bool {
    key == DRAW_AFTER_DEFERRED_EFFECT || key == LEVEL_STREAM_UNKNOWN
}

const LEGACY: &str = "net/minecraft/world/level/levelgen/LegacyRandomSource";

/// The most `nextInt()` draws one `consumeCount(n)` answers (see the module
/// docs).
pub const CONSUME_LIMIT: i32 = 1 << 16;

/// Lend ids, unique across threads: a host is `Clone + Send`, so a lend id
/// must never repeat on another thread's slot.
static NEXT_LEND: AtomicU64 = AtomicU64::new(1);

/// The lend the host runs under: which dimension's stream, and whether an
/// effect that may draw it was recorded during it. Plain data, so `StdHost`
/// stays `Clone + Send`; the stream itself sits in [`LENT`] for the lend's
/// dynamic extent only.
#[derive(Clone, Debug)]
pub(crate) struct Lend {
    /// Matches the [`LENT`] slot's id while this lend's frame is live. A
    /// stale lend (one an unwind left behind, or one a cloned host carries)
    /// finds a different id, or none, and draws nothing.
    id: u64,
    dimension: String,
    /// Which open of the level (the embedder's identity for it).
    instance: String,
    /// An effect that may draw this stream was recorded: later lent draws
    /// are out of vanilla's order.
    effect_pending: bool,
}

/// The lent stream, type-erased: a thin pointer to the `&mut dyn
/// LevelRandomStream` on [`StdHost::with_lent_level_random`]'s stack frame.
#[derive(Clone, Copy)]
struct Slot {
    id: u64,
    stream: *mut (),
}

thread_local! {
    static LENT: Cell<Option<Slot>> = const { Cell::new(None) };
}

/// Puts the previous [`LENT`] slot back when the lend's frame ends, on return
/// or on unwind, so the pointer never outlives the borrow it came from.
struct Restore(Option<Slot>);

impl Drop for Restore {
    fn drop(&mut self) {
        LENT.with(|slot| slot.set(self.0));
    }
}

/// Whether the lend `id` installed is still live on this thread.
fn slot_live(id: u64) -> bool {
    LENT.with(Cell::get).is_some_and(|slot| slot.id == id)
}

/// Run `draw` on the stream lent under `id`. `None` when that lend's frame
/// is no longer live (or this is another thread).
#[inline(never)]
fn with_slot<T>(id: u64, draw: impl FnOnce(&mut dyn LevelRandomStream) -> T) -> Option<T> {
    let slot = LENT.with(Cell::get).filter(|slot| slot.id == id)?;
    // SAFETY: `slot.stream` points at the `&mut dyn LevelRandomStream` local of
    // the `with_lent_level_random` frame that installed this id. That frame is
    // still live: `Restore` takes the slot out before the frame (and the
    // borrow) ends, on return and on unwind, and the slot is thread-local, so
    // no other thread sees it. The frame hands the stream to nobody else while
    // its `run` executes, and `draw` here is one stream method that cannot
    // reach back into Java or this slot, so this `&mut` is the only live access
    // for its (statement-long) duration.
    let stream = unsafe { &mut *slot.stream.cast::<&mut dyn LevelRandomStream>() };
    Some(draw(&mut **stream))
}

/// The handle key of the lent stream of `dimension`'s open `instance`. Its
/// class part makes it a `LegacyRandomSource` (`instanceof`, `getClass`), as
/// `Level.random` is.
pub(crate) fn lent_key(dimension: &str, instance: &str) -> String {
    format!("{LEGACY}#level-random/{instance}/{dimension}")
}

impl StdHost {
    /// Run `run` with `dimension`'s live level stream lent: `getRandom()` on
    /// that server level answers a handle whose draws `stream` answers (see
    /// the module docs). `instance` names this open of the level (a world, a
    /// reopen), so a handle kept from another one never matches. The lend ends
    /// when `run` returns or unwinds; a handle Java keeps draws nothing after
    /// that (counted).
    pub fn with_lent_level_random<R>(
        &mut self,
        dimension: &str,
        instance: &str,
        stream: &mut dyn LevelRandomStream,
        run: impl FnOnce(&mut StdHost) -> R,
    ) -> R {
        let id = NEXT_LEND.fetch_add(1, Ordering::Relaxed);
        let mut stream = stream;
        let slot = Slot {
            id,
            stream: (&mut stream as *mut &mut dyn LevelRandomStream).cast::<()>(),
        };
        let previous_slot = LENT.with(|lent| lent.replace(Some(slot)));
        let _restore = Restore(previous_slot);
        let previous = self.level_random_lend.replace(Lend {
            id,
            dimension: dimension.to_owned(),
            instance: instance.to_owned(),
            effect_pending: false,
        });
        // An unwind out of `run` restores the host's lend too (the slot is
        // restored by `_restore`), so an enclosing lend keeps answering.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(self)));
        self.level_random_lend = previous;
        match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    /// The stable `getRandom()` handle of `dimension`'s server level while its
    /// stream is lent; `None` (so the call stays counted) otherwise.
    #[inline(never)]
    pub(crate) fn lent_level_random_handle_impl(&mut self, dimension: &str) -> Option<Value> {
        let lend = self.level_random_lend.as_ref()?;
        if lend.dimension != dimension || !slot_live(lend.id) {
            return None;
        }
        let instance = lend.instance.clone();
        Some(self.keyed_random_handle(
            lent_key(dimension, &instance),
            State::LevelLent {
                dimension: dimension.to_owned(),
                instance,
            },
        ))
    }

    /// Every effect a call records for the embedder to carry out after it
    /// returns passes here. `draws_no_level_random` is the effect's place on
    /// the allow-list; anything else marks the running lend (module docs).
    #[inline(never)]
    pub(crate) fn note_deferred_effect(&mut self, draws_no_level_random: bool) {
        if draws_no_level_random {
            return;
        }
        if let Some(lend) = self.level_random_lend.as_mut() {
            lend.effect_pending = true;
        }
    }

    /// Mark the stream of `dimension`'s open `instance` as no longer where
    /// vanilla's would be (module docs), unless `r` is one of `Object`'s
    /// methods, which can't move it.
    #[inline(never)]
    fn note_stream_moved(&mut self, dimension: &str, instance: &str, r: &MethodRef) {
        let leaves_stream = matches!(
            (r.name.as_str(), r.descriptor.as_str()),
            ("equals", "(Ljava/lang/Object;)Z")
                | ("hashCode", "()I")
                | ("getClass", "()Ljava/lang/Class;")
                | ("toString", "()Ljava/lang/String;")
        );
        if !leaves_stream {
            self.level_random_unknown
                .insert(stream_key(dimension, instance));
        }
    }

    /// A call on the lent handle of `dimension`'s open `instance`: answered by
    /// the lent stream when that lend is live, `None` (counted by the caller)
    /// otherwise. A counted call marks the stream's position unknown.
    #[inline(never)]
    pub(crate) fn lent_random_call(
        &mut self,
        dimension: &str,
        instance: &str,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let lend = self
            .level_random_lend
            .clone()
            .filter(|lend| lend.dimension == dimension && lend.instance == instance)
            .filter(|lend| slot_live(lend.id));
        // A handle kept past its lend, or drawn under another level's:
        // counted, and vanilla's stream moved where the live one didn't.
        let Some(lend) = lend else {
            self.note_stream_moved(dimension, instance, r);
            return None;
        };
        let Some(draw) = parse_draw(r, args) else {
            self.note_stream_moved(dimension, instance, r);
            return None;
        };
        let answered = match draw {
            // Java's own checks come before any draw, as the bytecode's do.
            Draw::Bound(bound) if bound <= 0 => {
                return Some(Err(random_source::iae("Bound must be positive")));
            }
            Draw::Range { origin, bound } if origin >= bound => {
                return Some(Err(random_source::iae("bound - origin is non positive")));
            }
            Draw::Range { origin, bound } if bound.wrapping_sub(origin) <= 0 => {
                return Some(Err(random_source::iae("Bound must be positive")));
            }
            Draw::Between { min, max } if max.wrapping_sub(min).wrapping_add(1) <= 0 => {
                return Some(Err(random_source::iae("Bound must be positive")));
            }
            Draw::Consume(count) if count > CONSUME_LIMIT => {
                self.note_stream_moved(dimension, instance, r);
                return None;
            }
            draw => match with_slot(lend.id, |stream| answer(stream, draw)) {
                Some(answered) => answered,
                None => {
                    self.note_stream_moved(dimension, instance, r);
                    return None;
                }
            },
        };
        let position_unknown = self
            .level_random_unknown
            .contains(&stream_key(dimension, instance));
        if position_unknown {
            *self
                .unknown_host_calls
                .entry(LEVEL_STREAM_UNKNOWN.to_owned())
                .or_default() += 1;
        }
        if lend.effect_pending {
            *self
                .unknown_host_calls
                .entry(DRAW_AFTER_DEFERRED_EFFECT.to_owned())
                .or_default() += 1;
        }
        // A fork whose draw this counted takes its seed from a position that may not be
        // vanilla's: the child (and what it hands out) counts too (`random_source.rs`).
        let derived_unknown = position_unknown || lend.effect_pending;
        let value = match answered {
            Answer::Value(value) => {
                if draw.opens_bit_scope() {
                    self.random_bit_scope = Some(r.clone());
                }
                value
            }
            // The draw happened; out of random source numbers (counted), the
            // child is refused as a seeded fork's is.
            Answer::Fork(seed) => {
                match self.make_random_handle(random_source::legacy_state(seed)) {
                    Some(child) => child,
                    None => return Some(Err(numbers_exhausted())),
                }
            }
            Answer::ForkPositional(seed) => {
                match self.make_random_handle(State::LegacyPositional { seed }) {
                    Some(child) => child,
                    None => return Some(Err(numbers_exhausted())),
                }
            }
        };
        if derived_unknown && matches!(draw, Draw::Fork | Draw::ForkPositional) {
            self.mark_random_derived_unknown(&value);
        }
        Some(Ok(value))
    }
}

/// The key [`StdHost::note_stream_moved`] marks a stream under.
fn stream_key(dimension: &str, instance: &str) -> String {
    format!("{instance}/{dimension}")
}

fn numbers_exhausted() -> NativeError {
    NativeError::Metadata("the host's random source numbers are exhausted".into())
}

/// One call Decaf answers on a lent stream.
#[derive(Clone, Copy, Debug)]
enum Draw {
    Int,
    Bound(i32),
    Range { origin: i32, bound: i32 },
    Between { min: i32, max: i32 },
    Long,
    Boolean,
    Float,
    Double,
    Gaussian,
    TriangleDouble { mean: f64, spread: f64 },
    TriangleFloat { mean: f32, spread: f32 },
    Consume(i32),
    Fork,
    ForkPositional,
}

impl Draw {
    /// The draws a raw-bit conversion may follow (`random_source::bit_conversion`),
    /// as the seeded sources allow.
    fn opens_bit_scope(self) -> bool {
        matches!(self, Self::Int | Self::Long | Self::Float | Self::Double)
    }
}

enum Answer {
    Value(Value),
    Fork(i64),
    ForkPositional(i64),
}

fn parse_draw(r: &MethodRef, args: &[Value]) -> Option<Draw> {
    let int = |index: usize| match args.get(index) {
        Some(Value::Int(value)) => Some(*value),
        _ => None,
    };
    let float = |index: usize| match args.get(index) {
        Some(Value::Float(value)) => Some(*value),
        _ => None,
    };
    Some(match (r.name.as_str(), r.descriptor.as_str()) {
        ("nextInt", "()I") => Draw::Int,
        ("nextInt", "(I)I") => Draw::Bound(int(0)?),
        ("nextInt", "(II)I") => Draw::Range {
            origin: int(0)?,
            bound: int(1)?,
        },
        ("nextIntBetweenInclusive", "(II)I") => Draw::Between {
            min: int(0)?,
            max: int(1)?,
        },
        ("nextLong", "()J") => Draw::Long,
        ("nextBoolean", "()Z") => Draw::Boolean,
        ("nextFloat", "()F") => Draw::Float,
        ("nextDouble", "()D") => Draw::Double,
        ("nextGaussian", "()D") => Draw::Gaussian,
        ("triangle", "(DD)D") => Draw::TriangleDouble {
            mean: float(0)?,
            spread: float(1)?,
        },
        ("triangle", "(FF)F") => Draw::TriangleFloat {
            mean: float(0)? as f32,
            spread: float(1)? as f32,
        },
        ("consumeCount", "(I)V") => Draw::Consume(int(0)?),
        ("fork", "()Lnet/minecraft/util/RandomSource;") => Draw::Fork,
        ("forkPositional", "()Lnet/minecraft/world/level/levelgen/PositionalRandomFactory;") => {
            Draw::ForkPositional
        }
        // `setSeed(J)`, `next(I)` and anything else: counted.
        _ => return None,
    })
}

/// Draw `draw` from the stream. The argument checks already passed.
fn answer(stream: &mut dyn LevelRandomStream, draw: Draw) -> Answer {
    Answer::Value(match draw {
        Draw::Int => Value::Int(stream.next_int()),
        Draw::Bound(bound) => Value::Int(stream.next_int_bound(bound)),
        Draw::Range { origin, bound } => {
            Value::Int(origin.wrapping_add(stream.next_int_bound(bound.wrapping_sub(origin))))
        }
        Draw::Between { min, max } => Value::Int(
            stream
                .next_int_bound(max.wrapping_sub(min).wrapping_add(1))
                .wrapping_add(min),
        ),
        Draw::Long => Value::Long(stream.next_long()),
        Draw::Boolean => Value::Int(i32::from(stream.next_boolean())),
        Draw::Float => Value::Float(f64::from(stream.next_float())),
        Draw::Double => Value::Float(stream.next_double()),
        Draw::Gaussian => Value::Float(stream.next_gaussian()),
        Draw::TriangleDouble { mean, spread } => {
            let first = stream.next_double();
            let second = stream.next_double();
            Value::Float(mean + spread * (first - second))
        }
        Draw::TriangleFloat { mean, spread } => {
            let first = stream.next_float();
            let second = stream.next_float();
            Value::Float(f64::from(mean + spread * (first - second)))
        }
        Draw::Consume(count) => {
            for _ in 0..count.max(0) {
                stream.next_int();
            }
            Value::Void
        }
        Draw::Fork => return Answer::Fork(stream.next_long()),
        Draw::ForkPositional => return Answer::ForkPositional(stream.next_long()),
    })
}
