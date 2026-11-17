# Fabric builtin pack capture

`demo/Probe.java` runs on Java 25.0.5+8 against the real Fabric resource loader
(`fabric-resource-loader-v1` 3.1.0 and `fabric-api-base` 2.0.6, nested in Fabric API 0.166.2), Fabric
Loader 0.19.5 and the 26.3 server jar. `oracle/OracleMain.java` (Java side only, never in the Decaf
jar) hands `Probe.rows(container)` a `ModContainer` whose one root is the zip file system of a jar
of `resources/`, the way Fabric Loader roots a jar mod. A jar with directory entries and one with
only file entries give the same capture (`java25.txt`). In Decaf, `rows()` asks for the running
jar's own container, and the jar holds the same `resources/`.

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
F=~/ava/hj-corpus/deps-pool/fabric-api-0.166.2+26.3.jar
unzip -p $F META-INF/jars/fabric-resource-loader-v1-3.1.0+5af7744b5d.jar > rl.jar
unzip -p $F META-INF/jars/fabric-api-base-2.0.6+fcdff87f5d.jar > base.jar
(cd resources && $J/jar cf ../res.jar .)
L=~/ava/scratch/vanilla-values/oracle/libraries/server
CP=rl.jar:base.jar:$HOME/ava/hj-tools/fabric-loader-0.19.5.jar:$HOME/ava/minecraft-26.3-server-game.jar:$(find $L -name "*.jar" | tr "\n" ":")
$J/javac -nowarn -cp "$CP" -d out demo/Probe.java oracle/OracleMain.java
$J/java -cp "$CP:out" oracle.OracleMain res.jar
```
