//! A Java call run with its level's live `Level.getRandom()` stream lent.
//!
//! Java reads a draw at once (`level.getRandom().nextFloat()` is a pitch it uses on the next
//! instruction), so the sound-seed pattern of drawing at the replay can't work here. Instead the
//! Java call itself runs inside Sunrise's [`LevelRef::with_random`] closure, with the stream it
//! lends handed to the runtime ([`StdHost::with_lent_level_random`]) for exactly that call:
//! `getRandom()` on that level answers a handle whose draws come from Sunrise's live stream, in
//! Java's order (`runtime/level_random.rs`).
//!
//! The closure borrows the whole `ServerHost`, so everything the call reads of Sunrise must be
//! read before it (the bridges already run Java on facts read up front) and everything it does
//! is carried out after it. When Sunrise declines to lend (a stale handle, a level that is not
//! resident, a host without server authority), the call runs anyway, with `getRandom()` counted
//! as before.

use mod_api::{LevelRef, RandomSource, ServerHost};

use crate::runtime::{LevelRandomStream, StdHost};

/// Sunrise's lent stream, as the runtime's [`LevelRandomStream`]. Each method is one SDK draw.
struct Lent<'a>(&'a mut dyn RandomSource);

impl LevelRandomStream for Lent<'_> {
    fn next_int(&mut self) -> i32 {
        self.0.next_int()
    }
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        // The runtime throws Java's `IllegalArgumentException` for `bound <= 0` itself; the
        // SDK would answer 0 without drawing.
        debug_assert!(bound > 0);
        self.0.next_int_bound(bound)
    }
    fn next_long(&mut self) -> i64 {
        self.0.next_long()
    }
    fn next_boolean(&mut self) -> bool {
        self.0.next_boolean()
    }
    fn next_float(&mut self) -> f32 {
        self.0.next_float()
    }
    fn next_double(&mut self) -> f64 {
        self.0.next_double()
    }
    fn next_gaussian(&mut self) -> f64 {
        self.0.next_gaussian()
    }
}

/// Run `run` on `jar_host` with `level`'s live random stream lent, when Sunrise lends it; else
/// run it with nothing lent. `dimension` is the level Java is handed: the lend is made only when
/// `level` is that level, so a handle of one level never answers for another.
pub(crate) fn run_with_level_random<R>(
    host: &mut dyn ServerHost,
    level: Option<LevelRef>,
    dimension: &str,
    jar_host: &mut StdHost,
    run: impl FnOnce(&mut StdHost) -> R,
) -> R {
    let mut run = Some(run);
    if let Some(level) = level.filter(|level| level.dimension_id() == dimension) {
        // Which open of the level this is: a handle Java kept from another world, or from an
        // earlier open of a runtime level, never draws this one's stream.
        let instance = format!("{}/{}", level.epoch().get(), level.generation());
        let lent = level.with_random(host, |random| {
            let run = run.take().expect("Sunrise runs the draw once");
            jar_host.with_lent_level_random(dimension, &instance, &mut Lent(random), run)
        });
        if let Some(result) = lent {
            return result;
        }
    }
    // Sunrise declined (the closure never ran): `getRandom()` stays counted.
    let run = run.take().expect("the call has not run");
    run(jar_host)
}
