# Entity type lookup capture

```sh
J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
CP=$HOME/ava/minecraft-26.3-server-game.jar:$(find $HOME/ava/scratch/vanilla-values/oracle/libraries/server -name '*.jar' | paste -sd: -)
O=$HOME/ava/scratch/entity-type-lookup-classes
mkdir -p "$O"
$J/javac -cp "$CP" -d "$O" demo/Probe.java oracle/OracleMain.java
$J/java -cp "$O:$CP" oracle.OracleMain > java25.txt
# 's/^\[[^]]*\] \[main\/INFO\]: \[STDOUT\]: //' logs/latest.log > java25.txt
```
