//! `ArrayDeque`'s iterator `remove()` and `descendingIterator()`
//! (`iteration_guard.rs`, `array_deque.rs`).
//!
//! `iterator().remove()` is JDK 25's `delete(lastRet)`: it drops the
//! element in the slot `next()` read, moves the shorter side of the array,
//! and leaves the cursor on the element `next()` returns next. The
//! `DescendingIterator` walks the same live array backwards. Each probe
//! method makes a `witness` iterator before its script and drains it after:
//! a witness reads the live array from the old `head`, so what it prints
//! (or the `ConcurrentModificationException` where it meets an emptied slot)
//! shows where the script left the elements, not only what `toString` says.
//!
//! Every expected value is a row of `fixtures/deque_iterator_remove/java25.txt`,
//! OpenJDK 25.0.5.1's output for the same `demo/Probe.java` (see the README
//! there). A script's own exception is its class and message after the text
//! it had printed, then `|`, the deque, `|`, the witness.
//!
//! A `remove()` whose `lastRet` slot is no longer inside the live array (the
//! element was polled, the deque cleared, the array grown under the
//! iterator) is a `delete` of a slot outside `[head, tail)`: the JDK moves
//! slots the model does not hold (an `ArrayIndexOutOfBoundsException` from
//! `arraycopy`, or the neighbour removed), so the host counts it.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/deque_iterator_remove/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/deque_iterator_remove/java25.txt");

/// The rows whose `remove()` acts on a slot outside the live array.
const COUNTED: &[&str] = &[
    "descstaleAddSmall4",
    "descstaleClearGrown",
    "descstaleClearRoomy6",
    "descstaleClearSmall4",
    "descstaleClearWrapped",
    "descstalePollLastGrown",
    "descstalePollLastRoomy6",
    "descstalePollLastSmall4",
    "descstalePollLastWrapped",
    "staleAddFirstWrapped",
    "staleAddSmall4",
    "staleAddWrapped",
    "staleClearGrown",
    "staleClearRoomy6",
    "staleClearSmall4",
    "staleClearWrapped",
    "stalePollFirstGrown",
    "stalePollFirstRoomy6",
    "stalePollFirstSmall4",
    "stalePollFirstWrapped",
];

fn fixture(test: &str) -> Fixture {
    Fixture::build(test, &[("demo/Probe.java", PROBE)])
}

/// The oracle's `(method, answer)` rows that pass `keep`.
fn rows(keep: impl Fn(&str) -> bool) -> Vec<(&'static str, &'static str)> {
    ORACLE
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(method, _)| keep(method))
        .collect()
}

/// Runs each `(method, JDK 25 answer)` and checks that nothing was counted;
/// every row that differs is listed in one failure.
fn assert_exact(test: &str, cases: &[(&str, &str)]) {
    assert!(!cases.is_empty());
    let fixture = fixture(test);
    let mut wrong = Vec::new();
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let text = fixture.text_on(&mut host, method);
        if &text != expected || host.unknown_host_call_count() != 0 {
            wrong.push(format!(
                "{method}: expected {expected:?}, got {text:?}, counted {:?}",
                host.unknown_host_calls()
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} rows differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// `iterator().remove()` after `next()` drops that element, the loop goes on,
/// a second `remove()` or one before `next()` is an `IllegalStateException`,
/// `forEachRemaining` leaves `lastRet` on the last element it handed over,
/// and the deque's later steps follow the layout `delete` left.
#[test]
fn array_deque_iterator_remove_drops_the_element_next_returned() {
    assert_exact(
        "array-deque-iterator-remove",
        &rows(|method| !method.starts_with("desc") && !COUNTED.contains(&method)),
    );
}

/// `descendingIterator()`, its steps, `forEachRemaining` and `remove()`.
#[test]
fn array_deque_descending_iterator_and_its_remove() {
    assert_exact(
        "array-deque-descending-iterator",
        &rows(|method| method.starts_with("desc") && !COUNTED.contains(&method)),
    );
}

/// A `remove()` on a slot the deque no longer holds an element in is counted.
#[test]
fn array_deque_iterator_remove_on_a_stale_slot_is_counted() {
    let fixture = fixture("array-deque-iterator-remove-stale");
    for method in COUNTED {
        assert!(
            ORACLE
                .lines()
                .any(|line| line.starts_with(&format!("{method}="))),
            "{method} is not an oracle row"
        );
        let mut host = fixture.jar.seed_host(StdHost::new());
        fixture.text_on(&mut host, method);
        let counted = host.unknown_host_calls();
        assert!(
            counted
                .iter()
                .any(|(call, _)| call == "java/util/Iterator#remove()V"),
            "{method}: {counted:?}"
        );
    }
}
