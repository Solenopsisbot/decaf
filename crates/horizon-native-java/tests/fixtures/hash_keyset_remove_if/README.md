# `HashMap.keySet().removeIf` and `LinkedHashMap.keySet().removeIf`

`demo/Probe.java` is the only class the test ships. `java25.txt` is its raw output on Java 25
(`javac --release 21`, the release the fixture builder uses, then `java`), never edited:

```sh
J=/usr/lib/jvm/java-25-openjdk/bin
$J/javac --release 21 -d out demo/Probe.java
$J/java -cp out demo.Probe > java25.txt
```

- `java -version`: openjdk 25.0.5.1 2026-11-17 (OpenJDK Runtime Environment (build 25.0.5.1))
- `lib/modules` SHA-256: `2bffefbbdb53eb2c283925185a79dca3da792bbdcd849605409286f023f9d9a0`
- No library jar: the probe uses the JDK only.

`java25.txt` has no trailing newline: the probe's last row ends without one.

Found by `fabric-carpet-26.3+v260915.jar` (MIT): `Translations.updateLanguage` calls
`Map.keySet().removeIf(...)` on a `HashMap<String, String>` (bytecode `287..298`).

Not in the oracle: a `Predicate.test` the run cannot answer (its body reaches a counted call such as
`Thread.holdsLock`, whose result is the host's neutral `false`). `removeIf` is then counted as a whole
and the walk stops at that test (no later key is tested); removals made before it were exact and stay.
The rows of `counted_predicate_tests_count_the_remove_if` in `tests/fixture_hash_keyset_remove_if.rs`
are the run's state at that stop, not Java's, and assert the counted `removeIf`.
