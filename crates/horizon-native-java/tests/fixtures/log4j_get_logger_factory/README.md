# Log4j getLogger(name, MessageFactory) oracle

`demo/Probe.java` on Java 25 with the Log4j API/core 2.26.0 the game ships. `main` prints `rows()`
(`java25.txt`), then the counted case as `method=answer` (`java25_counted.txt`). Log4j's own status
messages go to stderr and are not part of the capture.

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
L=~/ava/scratch/vanilla-values/oracle/libraries/server/org/apache/logging/log4j
CP=$L/log4j-api/2.26.0/log4j-api-2.26.0.jar:$L/log4j-core/2.26.0/log4j-core-2.26.0.jar
$J/javac -cp "$CP" -d ~/ava/scratch/clog/classes demo/Probe.java
$J/java -cp "$CP:$HOME/ava/scratch/clog/classes" demo.Probe
```
