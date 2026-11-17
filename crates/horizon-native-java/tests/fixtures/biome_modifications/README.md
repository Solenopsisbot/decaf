`demo/Probe.java` against the real Fabric Biome API (`fabric-biome-api-v1` 20.2.0, nested in Fabric
API 0.166.2) and the 26.3 server jar on Java 25 (`jdk-25.0.5+8`). `main` prints `rows()` on the
first line (`java25.txt`) and then the counted cases, one `method=answer` per line
(`java25_counted.txt`).

`J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin; L=~/ava/scratch/vanilla-values/oracle/libraries/server; O=~/ava/scratch/issue-476/out2; CP=~/ava/scratch/issue-476/biome.jar:~/ava/minecraft-26.3-server-game.jar:$(find $L -name "*.jar" | tr "\\n" ":"); $J/javac -cp "$CP" -d $O demo/Probe.java; $J/java -cp "$CP:$O" demo.Probe`
