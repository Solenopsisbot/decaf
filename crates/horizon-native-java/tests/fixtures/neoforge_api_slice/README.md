# NeoForge API Java 25 oracle

Captured on arcweld with Java `25.0.5+8` (`. ~/ava/env.sh`) on 2026-11-17. The source in `Oracle.java` is compiled and run against the real NeoForge 26.3.0.101/FML jars; the resulting rows are committed in `java25.txt`.

Command:

```sh
cd ~/ava/scratch/neoforge-loader-model
. ~/ava/env.sh
javac --release 25 -cp loader-12.0.12.jar:neoforge-26.3.0.101-universal.jar:bus-8.0.5.jar:minecraft-server-patched-26.3.0.101.jar:maven-artifact-3.10.0.jar:jspecify-1.0.0.jar:netty-common-4.2.16.Final.jar:netty-buffer-4.2.16.Final.jar Oracle.java
java -cp .:loader-12.0.12.jar:neoforge-26.3.0.101-universal.jar:bus-8.0.5.jar:minecraft-server-patched-26.3.0.101.jar:maven-artifact-3.10.0.jar:jspecify-1.0.0.jar:netty-common-4.2.16.Final.jar:netty-buffer-4.2.16.Final.jar:log4j-api-2.26.0.jar:logging-1.7.12.jar:slf4j-api-2.0.17.jar Oracle
```

The jars were read from these full paths and have these SHA-256 digests:

| Jar | Full path | SHA-256 |
| --- | --- | --- |
| FML loader | `~/ava/scratch/neoforge-loader-model/loader-12.0.12.jar` | `24e64f915da20669431445ffab5f567552c3e01fd3c5c2cf000e5ceb75424a27` |
| NeoForge | `~/ava/scratch/neoforge-loader-model/neoforge-26.3.0.101-universal.jar` | `bc8904db92ad31f721a5cb7a20cd2565deccb66809fd04e8169a7d5a43bbcbf8` |
| Event bus | `~/ava/scratch/neoforge-loader-model/bus-8.0.5.jar` | `705e24f37752809499a5cd0c7dd31eaabf3d933c5d3b2060168e94e9ef62593a` |
| Patched Minecraft server | `~/ava/scratch/neoforge-loader-model/minecraft-server-patched-26.3.0.101.jar` | `97cea23617c929f35978a2428a09d9b001a0ce20b16cfd427c7f30bae84c4c22` |
| Maven artifact | `~/ava/scratch/neoforge-loader-model/maven-artifact-3.10.0.jar` | `5f97d58b507cd49887ce5f50e41832573ea204e9cf6705c9187a51b8ade9b543` |
| JSpecify annotations | `~/ava/scratch/neoforge-loader-model/jspecify-1.0.0.jar` | `1fad6e6be7557781e4d33729d49ae1cdc8fdda6fe477bb0cc68ce351eafdfbab` |
| Netty common | `~/ava/scratch/neoforge-loader-model/netty-common-4.2.16.Final.jar` | `9825ee68a0dc4cd2b53e2f532502401b2211bad9b77e8b04882d9e64487283ff` |
| Netty buffer | `~/ava/scratch/neoforge-loader-model/netty-buffer-4.2.16.Final.jar` | `cc36ae9fbd0b03fe755eb4eb4424ca53b59cfd297d7fd47d49e5b1059beded6c` |
| Log4j API | `~/ava/scratch/neoforge-loader-model/log4j-api-2.26.0.jar` | `820de7bbaf430eaf2e3e5ccb92f23e2085797c5ac4d4f7fc6c8b660fbefa1dbc` |
| Logging bridge | `~/ava/scratch/neoforge-loader-model/logging-1.7.12.jar` | `3c7ac9742fa98c773275379eb1046c048b5d2ee989152a8c69e7ba696c3f8538` |
| SLF4J API | `~/ava/scratch/neoforge-loader-model/slf4j-api-2.0.17.jar` | `7b751d952061954d5abfed7181c1f645d336091b679891591d63329c622eb832` |
