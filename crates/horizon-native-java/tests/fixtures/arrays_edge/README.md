# Array calls on a String spelled like an array key (#171)

`demo/Probe.java` is the only class the test ships; it needs nothing but the JDK. Its three methods pass the String
`"[I#0"` (spelled like the runtime's internal array key) where an array is expected. `java25.txt` holds Java 25's
answers, one per method, prefixed with the method's name as the test reads them. Captured on arcweld:

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
$J/javac -d out demo/Probe.java
# Run.java prints Probe.getLength(), copySrc() and copyDst(), one per line:
$J/javac -cp out -d out Run.java && $J/java -cp out Run
```

`java -version`: openjdk 25.0.5 2026-10-20 LTS (Temurin-25.0.5+8). Recaptured by the coordinator on 2026-11-17: the
three answers match `java25.txt` exactly.
