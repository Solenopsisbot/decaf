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

use std::collections::BTreeMap;

use mod_api::{LevelRef, RandomSource, ServerHost};

use crate::runtime::{LevelRandomStream, StdHost, is_counted_level_stream_draw};
use crate::{JarRow, LOG};

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

/// Report one block-use, live-menu or item-part call's counted calls on `row`, described by
/// `what` (`menu 3`, `block runes:crafting_altar useWithoutItem`). The unmodelled host calls go
/// into `block_use.unknown_host_calls` and a WARN. The level-stream draws the lend answered but
/// counted for their position (`runtime/level_random.rs`) are no unmodelled call: they reach the
/// jar's counted calls (its summary, report and stop line) with every other counted call, and
/// are logged here as what they are.
pub(crate) fn note_call_counts(row: &mut JarRow, what: &str, calls: &BTreeMap<String, usize>) {
    note_call_counts_stopped(row, what, calls, None);
}

/// [`note_call_counts`] for a live-menu call that may have stopped at a counted step:
/// `stopped` is the key of the call as a whole (`handleContainerClick`, `ServerPlayer.tick`),
/// counted once because a step inside it counted on its own
/// (`LiveMenuOutcome::stopped_at_counted_step`). That count says where the call stopped, not
/// that a fact was missing: like the draws, it reaches the jar's counted calls but is no
/// unmodelled host call, and is logged as the stop it is.
pub(crate) fn note_call_counts_stopped(
    row: &mut JarRow,
    what: &str,
    calls: &BTreeMap<String, usize>,
    stopped: Option<&str>,
) {
    let mut draws = BTreeMap::new();
    let mut stops = BTreeMap::new();
    let mut unmodelled = BTreeMap::new();
    for (key, count) in calls {
        let kind = if is_counted_level_stream_draw(key) {
            &mut draws
        } else if Some(key.as_str()) == stopped {
            &mut stops
        } else {
            &mut unmodelled
        };
        kind.insert(key, *count);
    }
    let unmodelled_count: usize = unmodelled.values().copied().sum();
    row.block_use.unknown_host_calls += unmodelled_count;
    if unmodelled_count > 0 {
        log::warn!(
            "{LOG}: {}: {what} leaned on {unmodelled_count} unmodelled host call(s): {unmodelled:?}",
            row.id
        );
    }
    let draw_count: usize = draws.values().copied().sum();
    if draw_count > 0 {
        log::info!(
            "{LOG}: {}: {what} counted {draw_count} level-stream draw(s), answered from the live stream but not claimed to be at vanilla's position: {draws:?}",
            row.id
        );
    }
    if !stops.is_empty() {
        log::info!(
            "{LOG}: {}: {what} stopped at a counted step and was counted as a whole: {stops:?}",
            row.id
        );
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
