# StreamCodec factory oracle

Captured with Java 25.0.5+8 using:

```sh
L=$HOME/ava/scratch/vanilla-values/oracle/libraries/server
J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
$J/javac -cp $HOME/ava/minecraft-26.3-server-game.jar:$(find "$L" -name '*.jar' | tr '\n' ':') -d /tmp/stream-codecs demo/Probe.java
$J/java -cp /tmp/stream-codecs:$HOME/ava/minecraft-26.3-server-game.jar:$(find "$L" -name '*.jar' | tr '\n' ':') demo.Probe
```
