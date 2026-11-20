Java 25 oracle for `LootItemConditionalFunction.commonFields` on the real 26.3 server and DFU jars.
`oracle/Probe.java` boots the game, calls the inherited helper with a real
`RecordCodecBuilder.Instance`, observes the optional getter for empty and
non-empty instances, builds the codec, and lists its keys. `java25.txt` is the
direct output captured from OpenJDK 25.0.5+8 against the real jars.
