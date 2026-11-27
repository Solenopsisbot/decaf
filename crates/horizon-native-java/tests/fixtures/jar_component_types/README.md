# Jar component type fixture captures

`java25.txt` holds `codec.encodeStart(NbtOps.INSTANCE, value)` for each of `demo/Cases.java`'s
values, run on OpenJDK 25.0.5+8 against the real Minecraft 26.3 server jar and its bundled
libraries (DataFixerUpper's `RecordCodecBuilder`, `Codec.listOf`, `optionalFieldOf`, `xmap`, and
26.3's `NbtOps`). Each line is `<case>=<tag>`, the tag printed with its type (`int(4)`,
`list[...]`, `compound{key:...}` with sorted keys, floats and doubles as their raw bits), or
`ERROR|<message>` for a codec error. `fixture_jar_component_types.rs` compiles the same `demo/`
sources against compile-only stubs, so every DFU call reaches the runtime's codec models, and
compares the runtime's NBT encode with these rows.

To regenerate (with the unpacked Minecraft 26.3 server `META-INF` directory and OpenJDK 25):

```sh
MC_META=/path/to/mc263-server/META-INF
CP="$MC_META/versions/26.3/server-26.3.jar:$(find "$MC_META/libraries" -name '*.jar' | tr '\n' ':')"
cd crates/horizon-native-java/tests/fixtures/jar_component_types
"$JAVA25/bin/javac" --release 25 -d out -cp "$CP" NbtOracle.java demo/*.java
"$JAVA25/bin/java" -cp "out:$CP" NbtOracle | sed -n 's/^.*CASE://p' > java25.txt
```

The real Runes jar test (`fixture_runes_component_types.rs`) is opt-in through `RUNES_JAR`.
