`demo/Probe.java` calls the Puzzles Lib client-constructor API shape and is captured on Java 25.

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
$J/javac --release 25 -d capture $(find stubs demo -name "*.java")
$J/java -cp capture demo.Probe | head -1
```
