# DataFixerUpper `Pair` and codecs as host objects (#156 P5c)

`demo/HostCodecs.java` makes pairs through the real API and reads an unmodelled codec constant.
`host_codec_tests.rs` compiles it against the compile-only stubs in `stubs/` and ships only
`demo/*`, so every call reaches the runtime's model.

The expected answers are the committed Java 25.0.5+8 output from HostCodecsOracle against the
real 26.3 server game and the listed server libraries. The libraries are the player's own and
are never committed.

## Re-capturing `oracle/java25.txt`

On arcweld, with Java 25.0.5+8 selected by `~/ava/env.sh`, the committed rows are captured
with this exact command against the real 26.3 server game and the server libraries needed to
initialize `Identifier.CODEC`. The output is never hand-edited.

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
NETTY_CODEC_BASE="$SERVER_LIBS/io/netty/netty-codec-base/4.2.16.Final/netty-codec-base-4.2.16.Final.jar"
BRIGADIER="$SERVER_LIBS/com/mojang/brigadier/1.3.11/brigadier-1.3.11.jar"
JOML="$SERVER_LIBS/org/joml/joml/1.10.9/joml-1.10.9.jar"
AUTHLIB="$SERVER_LIBS/com/mojang/authlib/10.0.77/authlib-10.0.77.jar"
GSON="$SERVER_LIBS/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar"
COMMONS_LANG="$SERVER_LIBS/org/apache/commons/commons-lang3/3.20.0/commons-lang3-3.20.0.jar"
CP="$DATAFIXER:$GAME:$FASTUTIL:$GUAVA:$FAILUREACCESS:$NETTY_BUFFER:$NETTY_COMMON:$NETTY_CODEC_BASE:$BRIGADIER:$JOML:$AUTHLIB:$GSON:$COMMONS_LANG"
ORACLE=/tmp/host-oracle-cov17
rm -rf "$ORACLE/classes" && mkdir -p "$ORACLE/classes"
"$JAVA_HOME/bin/javac" --release 25   -cp "$CP"   -d "$ORACLE/classes"   demo/HostCodecs.java   oracle/HostCodecsOracle.java
"$JAVA_HOME/bin/java"   -cp "$ORACLE/classes:$CP"   HostCodecsOracle > "$ORACLE/java25-rerun.txt"
cp "$ORACLE/java25-rerun.txt" java25.txt
cmp -s "$ORACLE/java25-rerun.txt" java25.txt
```

The jars used above are the 26.3 server libraries. Their SHA-256 values are:

| Jar | SHA-256 |
| --- | --- |
| `minecraft-26.3-server-game.jar` | `a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6` |
| `datafixerupper-10.0.21.jar` | `3547cb1da8993fb7ebd42494d0e78997d2cfbe90c421f5efd022863e6909941e` |
| `fastutil-8.5.18.jar` | `9094ae67d01d0ad246f886f11ad557fc2e79c72cbf3feed83e1512a8ae90a74a` |
| `guava-33.6.0-jre.jar` | `dc573e1fca4fd5454f4a5fd3d7da2df03002876a4175bafc14a95980dd7713b3` |
| `failureaccess-1.0.3.jar` | `cbfc3906b19b8f55dd7cfd6dfe0aa4532e834250d7f080bd8d211a3e246b59cb` |
| `netty-buffer-4.2.16.Final.jar` | `cc36ae9fbd0b03fe755eb4eb4424ca53b59cfd297d7fd47d49e5b1059beded6c` |
| `netty-common-4.2.16.Final.jar` | `9825ee68a0dc4cd2b53e2f532502401b2211bad9b77e8b04882d9e64487283ff` |
| `netty-codec-base-4.2.16.Final.jar` | `feb410225938d9970de6b624a0c031d079804fa5cc5e1ec6e9298f16db5998a7` |
| `brigadier-1.3.11.jar` | `9d09cf4819de83dfcd38bcb212d5f2227131791cef3abbfca3321d6fb2b9a64d` |
| `joml-1.10.9.jar` | `feca4db853371704338621c120acc5cc32300d8af635fa42414b6213301e216b` |
| `authlib-10.0.77.jar` | `521672866582633a412c82171d3cfa416ecaccdf05a43baf809c16f9aec07068` |
| `gson-2.14.0.jar` | `2cbd119bf1961c28788310963dc80ba65f58cdeec1dd139c8bdb1240faa2c36f` |
| `commons-lang3-3.20.0.jar` | `69e5c9fa35da7a51a5fd2099dfe56a2d8d32cf233e2f6d770e796146440263f4` |

The final `cmp` must succeed; the output is never hand-edited.
