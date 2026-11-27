# Client item parts oracle

`java25.txt` is the output of `oracle/ClientItemPartsOracle.java` on Java 25.0.5+8 against the
Minecraft 26.3 client jar and its libraries, running the demo in `demo/` (the same sources
`tests/fixture_client_item_parts.rs` compiles against stubs; `demo/mixin/` is left out, since
plain Java runs no Mixin and the oracle does what each handler does) on the real client classes:
a numeric item model property registered at the end of `RangeSelectItemModelProperties.bootstrap()`
and decoded through vanilla's `RangeSelectItemModelProperties.MAP_CODEC`, an item's
`getTooltipImage`, and the tooltip component's `getWidth`, `getHeight` and `extractImage`. The jar
is read from the launcher's install and is not part of this repository; the launcher's log
prefix (`[hh:mm:ss] [main/INFO]: [STDOUT]: `) is stripped.

```sh
MC="$HOME/.minecraft"
CP="$MC/versions/26.3/26.3.jar:$(python3 - <<'PY'
import json, os
mc = os.path.expanduser("~/.minecraft")
d = json.load(open(f"{mc}/versions/26.3/26.3.json"))
out = []
for lib in d["libraries"]:
    rules = lib.get("rules") or []
    ok = not rules
    for r in rules:
        if r.get("os", {}).get("name", "linux") == "linux":
            ok = r["action"] == "allow"
    if ok:
        out.append(f"{mc}/libraries/" + lib["downloads"]["artifact"]["path"])
print(":".join(out))
PY
)"
javac -nowarn -cp "$CP" -d out demo/*.java oracle/net/fabricmc/api/ModInitializer.java oracle/*.java
java -cp "out:$CP" ClientItemPartsOracle | sed 's/^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: //' > java25.txt
```
