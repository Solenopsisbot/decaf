```sh
J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
L=$HOME/ava/scratch/vanilla-values/oracle/libraries/server
CP=$HOME/ava/minecraft-26.3-server-game.jar:$(find "$L" -name "*.jar" | tr "\n" ":")
rm -rf out && mkdir out
$J/javac -cp "$CP" -d out demo/Probe.java oracle/Capture.java
$J/java -cp "out:$CP" Capture
```
