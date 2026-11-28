# Gson `toJson(JsonElement)` oracle (F7)

`java25.txt` is the raw, unedited stdout of `demo.Probe.main` (one `label=value` row per `Probe` method; `\n`, `\r`
and `\t` are escaped by the probe so every row is one line), run on Java 25 with the real Gson 2.14.0 jar (the version
the game ships). Two runs were byte-identical (`cmp`).

```
$ /usr/lib/jvm/java-25-openjdk/bin/java -version
openjdk version "25.0.5.1" 2026-11-17
OpenJDK Runtime Environment (build 25.0.5.1)
OpenJDK 64-Bit Server VM (build 25.0.5.1, mixed mode, sharing)
```

Gson jar: `gson-2.14.0.jar`, SHA-256 `2cbd119bf1961c28788310963dc80ba65f58cdeec1dd139c8bdb1240faa2c36f`.

Capture, from the repository root, with `G` the jar above:

```sh
J=/usr/lib/jvm/java-25-openjdk/bin
$J/javac --release 21 -cp "$G" -d out crates/horizon-native-java/tests/fixtures/gson_tojson_element/demo/Probe.java
$J/java -cp "out:$G" demo.Probe > crates/horizon-native-java/tests/fixtures/gson_tojson_element/oracle/java25.txt
```

The Rust fixture compiles the same `Probe.java` with `javac --release 21` against signature-only copies of the Gson
API it calls and ships only `demo/`, so each Gson call reaches the runtime's model. Compiled against the real jar and
against the copies, `javap -c -p` of `demo.Probe` and its nested classes gives the same `invoke*`, `getstatic`, `new`,
`ldc` and `checkcast` instructions (842 lines, identical apart from constant-pool indices).

## What the rows show

A tree (`JsonObject`, `JsonArray`, `JsonPrimitive`, `JsonNull`) given to `toJson(JsonElement)`,
`toJson(JsonElement, Appendable)` or `toJson(JsonElement, JsonWriter)` is written by `JsonElement`'s own adapter. Every
option that only reaches the adapter list or the reflective writer leaves the text byte-identical to a default `Gson`'s
(`adapterJsonObject`, `adapterJsonArray`, `adapterJsonPrimitive`, `adapterJsonElement`, `hierarchyJsonElement`,
`adapterString`, `adapterBoolean`, `adapterNumber`, `adapterFactory`, `namingStrategy`, `exclusionStrategies`,
`exclusionSerialization`, `exclusionDeserialization`, `exposeOnly`, `complexMapKeys`, `dateFormat`, `version`,
`objectToNumber`, `numberToNumber`, `innerClasses`, `modMenuStyle` = `prettyConfig`; also `longsAsStrings` and
`specialFloatsOption` = `defaultRich`). The options the writer does use change the text: `prettyRich`, `nullsRich`,
`noHtmlRich`, `prettyNullsNoHtmlRich`, `nonExecutable` (the `)]}'` prefix),
`formatTabs`, `formatCrlf`, `formatCompactSpaced`, `formatPrettyNoSpace`, `everythingAtOnce`, `jsonWriterOptions`.

Other rows: `defaultConfig`/`prettyConfig` (the shape Mod Menu writes), `defaultRich` (escapes, control characters,
U+2028/9, supplementary characters, keys that need escaping, `int`/`long`/`double`/`float`/`short`/`byte`/`char`,
nested containers, `JsonNull` members and elements), `topLevels`, `nullElement*` (a null `JsonElement` is `null`),
`nullMemberTop`, `stringWriter`, `stringBuilderAppends`, `jsonWriterCompact`, `roundTrip`, `objectDefault`.

Rows the runtime keeps counting: `nanDefault` (`NaN`) and `infinityInArray` (`[1,Infinity]`) (a default `Gson` writes
them, a strict one throws; the runtime does not record the writer's strictness), and `objectWithAdapter`
(`toJson(Object)` of a tree with an adapter registered: same text as the default, but a different overload).
