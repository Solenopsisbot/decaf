# Vanilla entity types capture

Generated with Java 25 and the real Minecraft 26.3 server game jar. The generator bootstraps vanilla with `SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();` and prints `BuiltInRegistries.ENTITY_TYPE.keySet()` in registry order.

```sh
J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
CP=$HOME/ava/minecraft-26.3-server-game.jar:$(find $HOME/ava/scratch/vanilla-values/oracle/libraries/server -name *.jar | paste -sd: -)
O=$HOME/ava/scratch/entity-type-table-classes
mkdir -p "$O"
$J/javac -cp "$CP" -d "$O" oracle/EntityTypesCapture.java
$J/java -cp "$O:$CP" oracle.EntityTypesCapture 2>/dev/null | sed "s/^\[[^]]*\] \[main\/INFO\]: \[STDOUT\]: //" > java25.txt
```
