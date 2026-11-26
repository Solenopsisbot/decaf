# Live menu oracle

`java25.txt` is the output of `oracle/LiveMenuOracle.java` on Java 25.0.5+8 against the
Minecraft 26.3 server jar and its libraries, running `script.txt` on the demo bench menu in
`demo/` (the same sources `tests/fixture_live_menu.rs` compiles against stubs). The jar is read
from a local install and is not part of this repository. The launcher's log prefix
(`[hh:mm:ss] [main/INFO]: [STDOUT]: `) is stripped.

```sh
SERVER="<path-to-26.3-server>/META-INF"
CP="$SERVER/versions/26.3/server-26.3.jar:$(find "$SERVER/libraries" -name '*.jar' | tr '\n' ':')"
javac -XDignore.symbol.file -cp "$CP" -d out demo/*.java item_click/*.java oracle/demo/Types.java \
  oracle/net/fabricmc/api/ModInitializer.java oracle/*.java
java -cp "out:$CP" LiveMenuOracle script.txt | sed 's/^\[[^]]*\] \[main\/INFO\]: \[STDOUT\]: //' > java25.txt
java -cp "out:$CP" LiveMenuOracle script_item_click.txt item_click \
  | sed 's/^\[[^]]*\] \[main\/INFO\]: \[STDOUT\]: //' | grep -v '^WARNING' > item_click_java25.txt
java -cp "out:$CP" CaptureItemHooks | sed 's/^\[[^]]*\] \[main\/INFO\]: \[STDOUT\]: //' \
  > ../../../src/live_menu_item_hooks_java25.txt
java -cp "out:$CP" MenuLayoutOracle | sed 's/^\[[^]]*\] \[main\/INFO\]: \[STDOUT\]: //' \
  | grep -E '^(slot|data_slots)' > layout_java25.txt
```

`layout_java25.txt` is the layout of the menu the bench's `MenuType` supplier builds
(`new BenchMenu(id, inventory)`): each slot's class, its container's class, its container slot and
`Slot.getMaxStackSize()`, then the data slot count. `menu_type_declaration_is_the_java25_layout`
checks `NativeJar::menu_type_declarations` against it.

`types/Types.java` builds the menu's `MenuType` for the Decaf fixture; the plain server jar keeps
that constructor private (Fabric's access widener opens it for mods), so the oracle compiles
`oracle/demo/Types.java` instead, which nothing it runs reads.

`item_click_java25.txt` is the same oracle with `item_click`: the demo grab item
(`item_click/GrabItem.java`, registered by `GrabProbe` into the built-in item registry opened
again) sits in hotbar slot 3, and `script_item_click.txt`'s pickup clicks run its own
`overrideOtherStackedOnMe` / `overrideStackedOnOther` inside the bench menu's `doClick`.
`jar_item_click_overrides_in_a_live_menu_match_java25` checks Decaf's live-menu bridge against it.
