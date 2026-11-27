# Rune pouch tooltip oracle

`java25.txt` is the output of `oracle/PouchTooltipOracle.java` on Java 25.0.5+8 against the
Minecraft 26.3 client jar and its libraries, with Bundle API 5.0.0 (the
`META-INF/jars/bundle-api-5.0.0+26.3-fabric.jar` Runes 1.3.2 nests) on the classpath. It measures
and draws Bundle API's own `CustomBundleTooltipComponent` for Runes' small rune pouch (size
multiplier 4) empty, with 10 fire runes, and with 4 x 64, on vanilla's `Font` (over the advance
table `tests/fixture_client_font.rs` uses), `StringSplitter`, `ClientLanguage` and
`GuiGraphicsExtractor`. `runes:fire_stone` is registered as a plain `Item` under that id: the
tooltip reads a rune's id, count and max stack size (64) only. See the oracle's header for what
each piece is and why. Neither jar is part of this repository; the launcher's log prefix is
stripped.

The oracle makes no `Minecraft` (it stops well before `Minecraft.<init>`) and opens no window or
connection; it was run under `unshare -rn` (no network).

```sh
MC="$HOME/.minecraft"
# bundle-api.jar: unzip -p "$RUNES_JAR" META-INF/jars/bundle-api-5.0.0+26.3-fabric.jar > bundle-api.jar
CP="$MC/versions/26.3/26.3.jar:bundle-api.jar:<the 26.3 libraries, as in ../client_font/README.md>"
javac -nowarn -cp "$CP" -d out oracle/*.java
unshare -rn java -cp "out:$CP" PouchTooltipOracle | sed 's/^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: //' > java25.txt
```
