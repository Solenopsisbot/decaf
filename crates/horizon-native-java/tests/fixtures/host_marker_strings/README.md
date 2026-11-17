# A jar String prints as itself (#330)

`demo/Probe.java` is the only class the test ships; it needs nothing but the JDK. `java25.txt` is
the output of `strings()` on Java 25, captured on arcweld:

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
$J/javac -d out demo/Probe.java
$J/java -cp out demo.Probe > java25.txt
```

`java -version`: openjdk 25.0.5 2026-10-20 LTS (Temurin-25.0.5+8).

`collections()` is not in the capture: Java prints each lambda's identity hash there, which
differs on every run, so the test asserts those calls are counted instead.
