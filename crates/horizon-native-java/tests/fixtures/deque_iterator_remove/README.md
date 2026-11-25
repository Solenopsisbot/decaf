# ArrayDeque iterator remove and descendingIterator, Java 25 capture

`demo/Probe.java` runs every scenario of `ArrayDeque`'s `iterator()` and
`descendingIterator()` followed by `remove()`: on a deque built from a
collection (capacity `n + 1`), by `add` (capacity 17), wrapped around the end
of its array, and grown. Each method makes a `witness` iterator first and
drains it last, so the output shows where the elements sit in the live array,
not only what `toString` prints.

Run with Java 25 (`java -version`: `openjdk version "25.0.5.1" 2026-11-17`,
OpenJDK 64-Bit Server VM, build 25.0.5.1):
```
J=/usr/lib/jvm/java-25-openjdk/bin
mkdir out && javac --release 21 -d out demo/Probe.java && $J/java -cp out demo.Probe > java25.txt
```
Each line is `method=answer`: the script's own output (an exception as class and
message), `|`, the deque's `toString`, `|`, the witness drained until it ends or
throws. `THROW ...` is the probe method itself throwing: after `clear()` under an
iterator that then removed, `delete` fails with an
`ArrayIndexOutOfBoundsException` and the deque's `toString` then throws
`ConcurrentModificationException`.

The rows listed as `COUNTED` in `fixture_deque_iterator_remove.rs` call `remove()`
on a slot the deque no longer holds an element in; the JDK answers them with
whatever `delete`'s slot arithmetic does outside `[head, tail)`, so the host
counts them and the rows are not compared.
