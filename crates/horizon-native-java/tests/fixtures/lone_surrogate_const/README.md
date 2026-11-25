# String constants with an unpaired surrogate (#447, hunt H24 `sur.*`)

Captured with Java 25 (`openjdk version "25.0.5.1" 2026-11-17`), no library jar (so no jar hash).

```sh
J=/usr/lib/jvm/java-25-openjdk/bin
mkdir classes
$J/javac --release 21 -d classes demo/Probe.java
$J/java -cp classes demo.Probe > java25.txt
```

`demo/Probe.java` has one method, `row(int)`, that answers `label=value`; `main` prints rows `0..ROWS`.
`java25.txt` is that output, never edited (36 lines).

- `ok.*` (11 rows): constants with no unpaired surrogate: a pair, a genuine U+FFFD, a folded
  `"\ud83d" + "\ude00"` (javac folds it into the pair), a concatenation and a `switch` over them. The runtime
  answers these exactly and counts nothing.
- `lone.*` (25 rows): a constant with an unpaired surrogate (`"\ud83d"`, `"\ude00"`, `"a\ud83d"`,
  `"\ud83db"`, `"\udc00\ud83d"`, ...) used through `hashCode`, `charAt`, `length`, `equals`, `compareTo`,
  `toCharArray`, `String.valueOf`, `Arrays.hashCode`, a `HashMap` key, `==`, a string `switch`, a pattern
  `switch` (`SwitchBootstraps.typeSwitch` with string labels), a concatenation whose recipe holds the half, a
  `static final` constant, a non-final static field and a reflective read of the `ConstantValue`.
  The runtime counts every one of these; their Java answers are in `java25.txt` for the record
  (for example `lone.highHash=55357`, `lone.equalsOther=false`, `lone.mapGet=1,null`), and the runtime does
  not answer them.

Before this change a `CONSTANT_Utf8` with an unpaired surrogate was read with U+FFFD in its place, so `"\ud83d"`
had length 1 and char 65533, `"\ud83d".equals("\ude00")` was `true`, and `"\ud83d"` and `"\ude00"` were one
`switch` label. Hunt H24 (`hunt/H24-checksums/compare.txt`) shows 13 such rows silent (`sur.litHigh=65533`
against `55357`), and H17 has the same shape.

The test (`tests/fixture_lone_surrogate_const.rs`) runs each row on a fresh host: `ok.*` must equal its line
and count nothing; `lone.*` must count a call (`java/lang/String#ldc()Ljava/lang/String;`, or
`java/lang/invoke/StringConcatFactory#makeConcatWithConstants...`, or the refusal by name).
