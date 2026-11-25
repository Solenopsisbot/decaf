# Java 25 `Pattern.asPredicate()` oracle

`java25.txt` is the raw output of `demo.Probe` (`rows`) on JDK 25, never edited.
`java25_counted.txt` is the same probe's `counted` mode: what the JDK prints for the
cases the runtime counts instead of answering (a `Pattern` with flags, a supplementary
character, a `test` argument that is not a `String` including every object kind the review
tried, reflection on the predicate's class, a pattern the JDK rejects at `compile`). It is not an oracle the runtime
claims to match; the tests only assert those calls are counted.

```sh
java -version
# (run with /usr/lib/jvm/java-25-openjdk/bin/java)
# openjdk version "25.0.5.1" 2026-11-17
# OpenJDK Runtime Environment (build 25.0.5.1)
# OpenJDK 64-Bit Server VM (build 25.0.5.1, mixed mode, sharing)
mkdir out && /usr/lib/jvm/java-25-openjdk/bin/javac --release 21 -d out demo/Probe.java
/usr/lib/jvm/java-25-openjdk/bin/java -cp out demo.Probe > java25.txt
/usr/lib/jvm/java-25-openjdk/bin/java -cp out demo.Probe counted > java25_counted.txt
```

No jar is involved: the probe only uses JDK classes.

Answered, with the oracle rows: `Pattern.compile(String).asPredicate()` and `test` on a
`String` (`matcher(s).find()` on a fresh matcher), including the Predicate handed to
`Stream.filter`, `List.removeIf` and `negate`/`and`/`or`, the `anyMatch`, `allMatch`, `noneMatch` and `count` of a stream and `Optional.filter`, and a jar's own lambda,
method reference, anonymous and named `Predicate` going through the same call site.
Counted, never guessed: `Pattern.compile(String, int)` (flags), a `test` on `null`, a
`StringBuilder` or any non-`String` (a jar object, `new Object()`, a collection, a `File`, a
`BigInteger`, ...: the JDK throws `ClassCastException`; a host handle is never read as the
`String` its key spells), a real `String` whose text spells an object's key (the runtime cannot
tell it from the object), an input with a supplementary character, `toString`/`hashCode` and
the class of the predicate (the JDK's is a hidden `Pattern$$Lambda`, final, superclass `Object`,
the one interface `Predicate`; `instanceof` is answered on that, with oracle rows), and
`asPredicate()` on a pattern the engine does not compile (a pattern the JDK rejects with
`PatternSyntaxException`, or one outside the engine's subset: the two are not told apart), or on
`Pattern.compile(null)` (a `NullPointerException` on the JDK).

The count belongs to the enclosing JDK operation too. When `test` is one of the counted calls
above and a `removeIf`, a stream's `filter` (ended by `collect` or `count`), `anyMatch`,
`allMatch` or `noneMatch`, `Optional.filter` or a `negate`/`and`/`or` runs it, that operation stops at
the count (`jdk_call::ABANDONED`): nothing is removed, kept, counted or answered from the host's
neutral `false`. The `enclosing` rows of `java25_counted.txt` print what the JDK does for each.
