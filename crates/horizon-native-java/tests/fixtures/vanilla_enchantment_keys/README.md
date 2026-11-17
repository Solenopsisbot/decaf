`demo/Probe.java` on OpenJDK 25.0.5 (two runs identical), driven by `capture/Capture.java`
(`SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();` then prints `Probe.rows()`),
against `minecraft-26.3-server-game.jar` (SHA-256
`a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6`) plus every server
library jar of 26.3 on the classpath, prints `java25.txt`:

    javac --release 25 -cp "$GAME:$LIBS" -d classes demo/Probe.java capture/Capture.java
    java -cp "classes:$GAME:$LIBS" Capture > java25.txt

`tests/fixture_vanilla_enchantment_keys.rs` compiles the same `demo/Probe.java` against
stubs and compares the interpreter's `rows()` with this file byte for byte.
