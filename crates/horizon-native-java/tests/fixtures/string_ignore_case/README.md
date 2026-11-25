# Case-insensitive String comparisons

`demo/Probe.java` is the only class the test ships; it needs nothing but the JDK. Each public static method answers one
row, and `main` prints them as `label=result`, one per line. `java25.txt` is that output, raw and unedited, from
OpenJDK 25.0.5.1 (`/usr/lib/jvm/java-25-openjdk`). A call that throws prints `exception class:message`. No jar is involved
(the probe calls only `java.lang`/`java.util`), so there is no jar SHA-256.

```sh
J=/usr/lib/jvm/java-25-openjdk/bin
$J/javac --release 21 -d classes demo/Probe.java
$J/java -XX:ActiveProcessorCount=1 -Xmx512m -cp classes demo.Probe > java25.txt
```

`java -version`:

```
openjdk version "25.0.5.1" 2026-11-17
OpenJDK Runtime Environment (build 25.0.5.1)
OpenJDK 64-Bit Server VM (build 25.0.5.1, mixed mode, sharing)
```

Two runs are byte-identical (`cmp`). Rows with a `grid_` label run one call over every ordered pair of a string pool
(or every offset and length for `regionMatches`) and print the results in one line.

The probe holds no unpaired surrogate: a runtime `String` is valid Unicode and cannot hold one.

## Counted rows (no `java25.txt` line)

A `java/lang/String#n` jar-object marker (an unmodelled constructor's result) or a host handle in place of a `String`
is a value no Java 25 program can hold, so it has no oracle row. `demo/Counted.java` gives the test entry points
(`equalsIgnoreCase`, `compareToIgnoreCase`, both `regionMatches` overloads, `String.CASE_INSENSITIVE_ORDER.compare`);
the test calls each with a marker, a jar-object handle spelling `java/lang/String#n` and a plain host handle, as the
receiver, as the argument and as both, and asserts the call is counted (`java/lang/String#<method><descriptor>`, the
comparator as `compareToIgnoreCase`) and never answered from the key text the value carries. A real `String` pair in
the same entry points is answered and counts nothing. For the comparator, the bridge casts the first operand first:
a wrong class there is a `ClassCastException` whatever the second is (`cmporder_raw_int_first` and `_second` are the
Java rows), and a marker in either position is counted.
