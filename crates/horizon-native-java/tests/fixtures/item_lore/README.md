# Item lore fixture captures

`java25.txt` holds the exact Minecraft 26.3 `DataComponents.LORE` codec (`ItemLore.CODEC`) +
`JsonOps.INSTANCE` output for each lore `fixture_item_lore.rs`'s demo mod builds, and the
`ItemLore` constructor's refusal past 256 lines. It was captured on OpenJDK 25.0.5+8 against the
real Minecraft 26.3 server jar and its bundled libraries. The CI fixture compiles its demo mod
against compile-only stubs (only `demo/` classes ship) and compares the runtime's item
declarations with these rows.

To regenerate (with the unpacked Minecraft 26.3 server `META-INF` directory and OpenJDK 25):

```sh
MC_META=/path/to/mc263-server/META-INF
CP="$MC_META/versions/26.3/server-26.3.jar:$(find "$MC_META/libraries" -name '*.jar' | tr '\n' ':')"
"$JAVA25/bin/javac" --release 25 -d out -cp "$CP" \
  crates/horizon-native-java/tests/fixtures/item_lore/ItemLoreOracle.java
"$JAVA25/bin/java" -cp "out:$CP" ItemLoreOracle \
  | sed -n 's/^.*CASE://p' \
  > crates/horizon-native-java/tests/fixtures/item_lore/java25.txt
```

The real Runes jar test is opt-in through `RUNES_JAR`.
