# Level random oracle

`java25.txt` is `oracle/LevelRandomsOracle.java` on Java 25.0.5+8 against the Minecraft 26.3
server jar and its libraries: `demo.LevelRandoms.all(random, random)` on a real
`new LegacyRandomSource(6301295841L)`, the class vanilla's `Level.random` is
(`RandomSource.create()`). `src/level_random_tests.rs` runs the same demo through Decaf, where
`LevelRandoms.run(level)` draws `level.getRandom()` on a server level whose live stream the test
lends (`StdHost::with_lent_level_random`), and compares the line with this file.

The stubs in `stubs/` are compile-only (the method set and descriptors of 26.3's
`RandomSource`, checked with `javap`); the oracle compiles against the real jar instead. The jar is
read from a local install and is not part of this repository.

```sh
SERVER="<path-to-26.3-server>/META-INF"
CP="$SERVER/versions/26.3/server-26.3.jar:$(find "$SERVER/libraries" -name '*.jar' | tr '\n' ':')"
javac --release 21 -nowarn -cp "$CP" -d out demo/*.java oracle/*.java
java -cp "out:$CP" LevelRandomsOracle 6301295841 > java25.txt
```
