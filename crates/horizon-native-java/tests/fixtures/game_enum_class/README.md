# Game enums whose constants the host doesn't list

`demo/Probe.java` is the only class the test ships. `java25.txt` is its output on Java 25 with
the real 26.3 server jar and the server's own libraries, captured on arcweld:

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
L=$HOME/ava/scratch/vanilla-values/oracle/libraries/server
CP=$HOME/ava/minecraft-26.3-server-game.jar:$(find $L -name '*.jar' | tr '\n' ':')
$J/javac -nowarn -d out -cp "$CP" demo/Probe.java
$J/java -cp "out:$CP" demo.Probe > java25.txt
```

`java -version`: openjdk 25.0.5 2026-10-20 LTS (Temurin-25.0.5+8). No bootstrap is needed.

The first five rows (`exact()`) are compared byte for byte. The last three (`counted()`) are
Java's answers for calls the host doesn't model; the test asserts those calls are counted.
