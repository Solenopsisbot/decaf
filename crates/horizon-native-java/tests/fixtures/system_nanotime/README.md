# System.nanoTime fixture

Capture with Java 25:

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
rm -rf /tmp/system-nanotime && mkdir -p /tmp/system-nanotime
$J/javac -d /tmp/system-nanotime crates/horizon-native-java/tests/fixtures/system_nanotime/demo/Probe.java
$J/java -cp /tmp/system-nanotime demo.Probe
```
