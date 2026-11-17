# StreamCodec map/apply oracle
Captured with Java 25.0.5+8 using:

```sh
L=$HOME/ava/scratch/vanilla-values/oracle/libraries/server
J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
$J/javac -cp $HOME/ava/minecraft-26.3-server-game.jar:$(find "$L" -name '*.jar' | tr '\n' ':') -d /tmp/stream-codec-map-apply demo/Probe.java
$J/java -cp /tmp/stream-codec-map-apply:$HOME/ava/minecraft-26.3-server-game.jar:$(find "$L" -name '*.jar' | tr '\n' ':') demo.Probe
```
