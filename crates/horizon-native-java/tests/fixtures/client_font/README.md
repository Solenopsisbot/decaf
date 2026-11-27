# Client font oracle

`java25.txt` is the output of `oracle/FontOracle.java` on Java 25.0.5+8 against the Minecraft 26.3
client jar and its libraries, running the demo in `demo/` (the same sources
`tests/fixture_client_font.rs` compiles against stubs; `demo/mixin/` is left out, since plain Java
runs no Mixin and the oracle builds the component as the handler does) on the real client
classes: vanilla's `Font` (built with `new Font(Provider)` over a glyph provider whose advances are
the test's table), `StringSplitter`, `ClientLanguage` (its private constructor, holding the
fixture's two translations) and `GuiGraphicsExtractor` (`textWithWordWrap`, `centeredText` and
the `blitSprite` overloads run their real bodies; a recorder overrides the two leaves they end
in). The jar is read from the launcher's install and is not part of this repository; the
launcher's log prefix (`[hh:mm:ss] [main/INFO]: [STDOUT]: `) is stripped.

The oracle makes no `Minecraft` and opens no window; it was run under `unshare -rn` (no network).

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
    if ok and "artifact" in lib.get("downloads", {}):
        out.append(f"{mc}/libraries/" + lib["downloads"]["artifact"]["path"])
print(":".join(out))
PY
)"
javac -nowarn -cp "$CP" -d out demo/*.java oracle/net/fabricmc/api/ModInitializer.java oracle/*.java
unshare -rn java -cp "out:$CP" FontOracle | sed 's/^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: //' > java25.txt
```

The recorder prints every `blitSprite` it is handed, a zero-sized one too; 26.3 draws nothing for
one in any `gui.scaling`, so the test leaves those lines out (`draws_something`).
