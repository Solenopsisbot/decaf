# DataFixerUpper `Pair` and codecs as host objects (#156 P5c)

`demo/HostCodecs.java` makes pairs through the real API and reads an unmodelled codec constant.
`host_codec_tests.rs` compiles it against the compile-only stubs in `stubs/` and ships only
`demo/*`, so every call reaches the runtime's model.

The expected answers are `oracle/HostCodecsOracle.java` on OpenJDK 25.0.5+8 against the 26.3
server's `datafixerupper-10.0.21.jar`, `guava-33.6.0-jre.jar`, `failureaccess-1.0.3.jar` and
`fastutil`, two runs alike except an identity hash. The libraries are the player's own and are
never committed.

## Re-capturing `oracle/java25.txt`

On arcweld, with Java 25.0.5+8 selected by `~/ava/env.sh`, the committed rows are captured
with this exact command. It compiles and runs against the real 26.3 server game and its
DataFixerUpper, Guava, fastutil, and Netty libraries; no temporary source or jar is used.

```sh
set -eu -o pipefail
. ~/ava/env.sh
SERVER_LIBS="$HOME/ava/scratch/vanilla-values/oracle/libraries/server"
DATAFIXER="$SERVER_LIBS/com/mojang/datafixerupper/10.0.21/datafixerupper-10.0.21.jar"
GAME="$HOME/ava/minecraft-26.3-server-game.jar"
FASTUTIL="$SERVER_LIBS/it/unimi/dsi/fastutil/8.5.18/fastutil-8.5.18.jar"
GUAVA="$SERVER_LIBS/com/google/guava/guava/33.6.0-jre/guava-33.6.0-jre.jar"
FAILUREACCESS="$SERVER_LIBS/com/google/guava/failureaccess/1.0.3/failureaccess-1.0.3.jar"
NETTY_BUFFER="$SERVER_LIBS/io/netty/netty-buffer/4.2.16.Final/netty-buffer-4.2.16.Final.jar"
NETTY_COMMON="$SERVER_LIBS/io/netty/netty-common/4.2.16.Final/netty-common-4.2.16.Final.jar"
CP="$DATAFIXER:$GAME:$FASTUTIL:$GUAVA:$FAILUREACCESS:$NETTY_BUFFER:$NETTY_COMMON"
ORACLE=/tmp/host-oracle345
rm -rf "$ORACLE/classes" && mkdir -p "$ORACLE/classes"
"$JAVA_HOME/bin/javac" --release 25 \
  -cp "$CP" \
  -d "$ORACLE/classes" \
  crates/horizon-native-java/tests/fixtures/host_codecs/demo/HostCodecs.java \
  crates/horizon-native-java/tests/fixtures/host_codecs/oracle/HostCodecsOracle.java
"$JAVA_HOME/bin/java" \
  -cp "$ORACLE/classes:$CP" \
  HostCodecsOracle > "$ORACLE/java25-rerun.txt"
cmp -s "$ORACLE/java25-rerun.txt" crates/horizon-native-java/tests/fixtures/host_codecs/oracle/java25.txt
```

The jars used above are the 26.3 server libraries. Their SHA-256 values are:

| Jar | SHA-256 |
| --- | --- |
| `datafixerupper-10.0.21.jar` | `3547cb1da8993fb7ebd42494d0e78997d2cfbe90c421f5efd022863e6909941e` |
| `minecraft-26.3-server-game.jar` | `a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6` |
| `fastutil-8.5.18.jar` | `9094ae67d01d0ad246f886f11ad557fc2e79c72cbf3feed83e1512a8ae90a74a` |
| `guava-33.6.0-jre.jar` | `dc573e1fca4fd5454f4a5fd3d7da2df03002876a4175bafc14a95980dd7713b3` |
| `failureaccess-1.0.3.jar` | `cbfc3906b19b8f55dd7cfd6dfe0aa4532e834250d7f080bd8d211a3e246b59cb` |
| `netty-buffer-4.2.16.Final.jar` | `cc36ae9fbd0b03fe755eb4eb4424ca53b59cfd297d7fd47d49e5b1059beded6c` |
| `netty-common-4.2.16.Final.jar` | `9825ee68a0dc4cd2b53e2f532502401b2211bad9b77e8b04882d9e64487283ff` |

The final `cmp` must succeed; the output is never hand-edited.
