# Item parts oracle

`java25.txt` is the output of `oracle/ItemPartsOracle.java` on Java 25.0.5+8 against the
Minecraft 26.3 server jar and its libraries, running the demo tally item in `demo/` (the same
sources `tests/fixture_item_parts.rs` compiles against stubs) through its own overrides on the
real `ItemStack`, `Slot`, `Inventory` and player classes. The jar is read from a local install
and is not part of this repository. The launcher's log prefix
(`[hh:mm:ss] [main/INFO]: [STDOUT]: `) is stripped.

```sh
SERVER="<path-to-26.3-server>/META-INF"
CP="$SERVER/versions/26.3/server-26.3.jar:$(find "$SERVER/libraries" -name '*.jar' | tr '\n' ':')"
javac -nowarn -XDignore.symbol.file -cp "$CP" -d out demo/*.java \
  oracle/net/fabricmc/api/ModInitializer.java oracle/*.java
java -cp "out:$CP" ItemPartsOracle | sed 's/^\[[^]]*\] \[main\/INFO\]: \[STDOUT\]: //' > java25.txt
```

The oracle opens the built-in item and data component registries again after bootstrap (a mod
registers before they freeze), binds every item's components as a server reload does, and makes
the server player and level with `Unsafe.allocateInstance`; their drops, stats and sounds are
printed instead of carried out. A tally's component is printed as `NbtOps` encodes it
(`Tally.CODEC.encodeStart(NbtOps.INSTANCE, ..)`), field by field with each tag's type.
