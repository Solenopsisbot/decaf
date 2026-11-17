# Fabric CustomValue iterator capture

Run from this fixture directory:
```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
L=~/ava/scratch/vanilla-values/oracle/libraries/server
CP=$HOME/ava/hj-tools/fabric-loader-0.19.5.jar:$HOME/ava/minecraft-26.3-server-game.jar:$(find "$L" -name "*.jar" | tr "\n" ":")
rm -rf out && mkdir out
$J/javac -cp "$CP" -d out demo/Probe.java oracle/OracleMain.java
$J/java -cp "$CP:out" oracle.OracleMain fabric.mod.json
```
```
