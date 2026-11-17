# Entity-registration oracle provenance

These files are captured from Minecraft 26.3's `server-game.jar` with Java
25.0.5+8. The exact commands are the `*-command.txt` files in this directory;
`command.txt` runs `Probe.java`, `subclass-command.txt` runs
`SubclassProbe.java`, and `pool-command.txt` runs `PoolProbe.java`.

On arcweld the command class path is:

```
~/ava/minecraft-26.3-server-game.jar:$(python3 -c 'import pathlib; print(":".join(map(str,pathlib.Path("libraries").rglob("*.jar"))))')
```

The capture was made with `javac`/`java` from `~/ava/jdk/jdk-25.0.5+8`.
The committed tables are the byte-for-byte stdout captures; SHA-256 values of
the jars and rerun outputs must be recorded when refreshing a table.
`FullPoolProbe.java` is the producer for `full-pool-java25.tsv` and
`PoolProbe.java` for `vanilla_entity_data_pool.tsv`.
