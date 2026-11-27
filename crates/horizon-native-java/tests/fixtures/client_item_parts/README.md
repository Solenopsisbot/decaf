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
# Copy the launcher's files into a scratch folder first; never read them in place.
SCRATCH="<scratch>"
cp -r ~/.minecraft/versions/26.3 ~/.minecraft/libraries "$SCRATCH"/
CP="$SCRATCH/26.3/26.3.jar:$(python3 - "$SCRATCH" <<'PY'
import json, sys
root = sys.argv[1]
d = json.load(open(f"{root}/26.3/26.3.json"))
out = []
for lib in d["libraries"]:
    rules = lib.get("rules") or []
    ok = not rules
    for r in rules:
        if r.get("os", {}).get("name", "linux") == "linux":
            ok = r["action"] == "allow"
    if ok:
        out.append(f"{root}/libraries/" + lib["downloads"]["artifact"]["path"])
print(":".join(out))
PY
)"
javac -nowarn -cp "$CP" -d out demo/*.java oracle/net/fabricmc/api/ModInitializer.java oracle/*.java
# No network (the client classes must never reach one), no display, a scratch home.
unshare -rn true || { echo "needs a network namespace"; exit 1; }
env -u DISPLAY -u WAYLAND_DISPLAY HOME="$SCRATCH/home" XDG_CONFIG_HOME="$SCRATCH/home/.config" \
  XDG_CACHE_HOME="$SCRATCH/home/.cache" XDG_DATA_HOME="$SCRATCH/home/.local/share" \
  unshare -rn java -Djava.io.tmpdir="$SCRATCH/tmp" -cp "out:$CP" ClientItemPartsOracle \
  | sed 's/^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: //' > java25.txt
```
