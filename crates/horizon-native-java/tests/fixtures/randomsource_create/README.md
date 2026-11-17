# randomsource_create
```sh
J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
L=$HOME/ava/scratch/vanilla-values/oracle/libraries/server
CP=$HOME/ava/minecraft-26.3-server-game.jar:$(find "$L" -name '*.jar' | tr '\n' ':')
mkdir -p classes
"$J/javac" -cp "$CP" -d classes demo/Probe.java
"$J/java" -cp "classes:$CP" demo.Probe
```
