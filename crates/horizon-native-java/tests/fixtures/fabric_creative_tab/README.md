# Fabric creative tab capture

Captured with Java 25.0.5+8 using the real Fabric creative-tab module and Minecraft 26.3 server jar:

```sh
unzip -p "$HOME/ava/hj-corpus/deps-pool/fabric-api-0.166.2+26.3.jar" META-INF/jars/fabric-creative-tab-api-v1-5.0.22+fcdff87f5d.jar > ct.jar
L="$HOME/ava/scratch/vanilla-values/oracle/libraries/server"
CP="$HOME/ava/minecraft-26.3-server-game.jar:$(find "$L" -name "*.jar" | tr "\\n" ":")"
javac -cp "$CP:$PWD/ct.jar" -d capture demo/Probe.java
java -cp "$CP:$PWD/capture:$PWD/ct.jar" demo.Probe
```
