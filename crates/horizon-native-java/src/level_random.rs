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
//!   count draws nothing and is counted rather than stalling the tick.
//! - `fork()`: one `nextLong()`, then a seeded `LegacyRandomSource` Decaf
//!   keeps itself; `forkPositional()`: one `nextLong()`, then a
//!   `LegacyPositionalRandomFactory` of it.
//! - `setSeed(J)` and `next(I)`: counted. The lend lets Java draw, never
//!   reseed, and the stream has no raw `next(bits)`.
//!
//! # Effects applied after the call
//!
//! Decaf carries some of a call's work out after the Java call returns. If the
//! embedder's handling of it may draw this same stream, and Java produced it
//! *before* a later draw of the lent stream, vanilla would draw for it first
//! and Decaf draws for Java first: the two swap. That work is of two kinds,
//! and a lent draw is counted ([`DRAW_AFTER_DEFERRED_EFFECT`]) after either:
//!
//! - a recorded effect not on the allow-list of effects known to draw nothing
//!   ([`StdHost::note_deferred_effect`], `live_menu::draws_no_level_random`).
//!   Criteria and recipe unlocks are not on it: in vanilla they can complete
//!   an advancement at once, and `AdvancementRewards.grant` rolls its loot
//!   with `LootContext$Builder.create(Optional.empty())`, which falls back to
//!   `ServerLevel.getRandom()`. Neither is a drop, which in 26.3 draws only
//!   entity randoms (`ItemEntity.<init>(Level,DDD,ItemStack)` its own
//!   `Entity.random`, `createItemStackToDrop` the player's): a drop stays off
//!   the list only because the embedder's drop path isn't verified to leave
//!   the level stream alone;
//! - a write to the player's inventory, which is no effect but state the
//!   embedder writes back after the call, and whose `inventory_changed`
//!   criterion can reward the same way ([`StdHost::inventory_baseline`]).
//!
//! The draw is still answered (the stream is the real one), but the call never
//! claims an exact order it doesn't have.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::live_menu::MenuPlayerFacts;
use crate::random_source::{self, State};
use crate::recipe_models::{self, StackComponents};
use crate::{MethodRef, NativeError, StdHost, Value};

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

    /// The player's inventory as a call starts, by stack contents, for
    /// [`Self::inventory_written`]; taken only while a stream is lent (no
    /// other call needs it). `None` when the facts give no inventory: the
    /// call then has none to write back.
    #[inline(never)]
    pub(crate) fn inventory_baseline(
        &self,
        facts: Option<&MenuPlayerFacts>,
    ) -> Option<Vec<Option<StackComponents>>> {
        let lend = self.level_random_lend.as_ref()?;
        if !slot_live(lend.id) {
            return None;
        }
        let inventory = facts?.inventory.as_ref()?;
        Some(
            inventory
                .iter()
                .map(|stack| recipe_models::extract_stack_components(self, stack))
                .collect(),
        )
    }

    /// Whether the running call has changed the player's inventory: a slot's
    /// stack differs, by contents, from the one it started with (the runtime
    /// keeps its own stack objects, so identity says nothing). The embedder
    /// writes the inventory back after the call, so a later lent draw may be
    /// out of order (module docs). A stack Decaf can't read, or an inventory it
    /// can no longer read, counts as changed.
    #[inline(never)]
    fn inventory_written(&self) -> bool {
        let Some(active) = self.block_use.active_use.as_ref() else {
            return false;
        };
        let (Some(start), Some(facts)) = (active.inventory_start.as_ref(), active.facts.as_ref())
        else {
            return false;
        };
        let Some(now) = crate::live_menu::inventory_out(self, &active.player_inventory, facts)
        else {
            return true;
        };
        now.len() != start.len()
            || now.iter().zip(start).any(|(now, start)| {
                match (recipe_models::extract_stack_components(self, now), start) {
                    (Some(now), Some(start)) => now != *start,
                    _ => true,
                }
            })
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

    /// A call on `dimension`'s lent handle: answered by the lent stream when
    /// that lend is live, `None` (counted by the caller) otherwise.
    #[inline(never)]
    pub(crate) fn lent_random_call(
        &mut self,
        dimension: &str,
        instance: &str,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let lend = self.level_random_lend.clone()?;
        // A handle of another level, of another open of this one, or one Java
        // kept past its lend: counted.
        if lend.dimension != dimension || lend.instance != instance || !slot_live(lend.id) {
            return None;
        }
        let draw = parse_draw(r, args)?;
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
            Draw::Consume(count) if count > CONSUME_LIMIT => return None,
            draw => with_slot(lend.id, |stream| answer(stream, draw))?,
        };
        if lend.effect_pending || self.inventory_written() {
            *self
                .unknown_host_calls
                .entry(DRAW_AFTER_DEFERRED_EFFECT.to_owned())
                .or_default() += 1;
        }
        Some(Ok(match answered {
            Answer::Value(value) => {
                if draw.opens_bit_scope() {
                    self.random_bit_scope = Some(r.clone());
                }
                value
            }
            Answer::Fork(seed) => self.make_random_handle(random_source::legacy_state(seed)),
            Answer::ForkPositional(seed) => {
                self.make_random_handle(State::LegacyPositional { seed })
            }
        }))
    }
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
