# Gson nesting limit capture (#262)

`java25.txt` is the output of `demo/Depth.java` run on OpenJDK 25.0.5.1 with the real gson-2.14.0
jar from the Minecraft 26.3 server jar (`META-INF/libraries/com/google/code/gson/gson/2.14.0/`):

```
2cbd119bf1961c28788310963dc80ba65f58cdeec1dd139c8bdb1240faa2c36f  gson-2.14.0.jar
```

Captured from this folder:

```
javac -cp gson-2.14.0.jar -d out demo/Depth.java && java -cp out:gson-2.14.0.jar demo.Depth > java25.txt
```

Each row is one document's index (or `depth N` for the sweep), the call, and `ok` or the
exception: its class and message, its cause's, and whether it is a `JsonParseException`,
`JsonSyntaxException` and `RuntimeException`. One member name decodes to `A` and a newline, so
one document's rows span two lines.
