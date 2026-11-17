# `Map.merge` on maps Gson reads (#337)

`demo/Probe.java` is the only class the test ships. `java25.txt` is its output on Java 25 with the
real Gson, captured on arcweld:

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
G=.../libraries/server/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar   # the 26.3 server's own Gson
$J/javac -d out -cp $G demo/Probe.java
$J/java -cp out:$G demo.Probe > java25.txt
```

- `java -version`: openjdk 25.0.5 2026-10-20 LTS (Temurin-25.0.5+8)
- `gson-2.14.0.jar` SHA-256: `2cbd119bf1961c28788310963dc80ba65f58cdeec1dd139c8bdb1240faa2c36f`

`java25.txt` has no trailing newline: the probe's last row ends without one.
