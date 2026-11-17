Captured with Java 25.0.5+8 and the real 26.3 server-game.jar.

```sh
CP=$HOME/ava/minecraft-26.3-server-game.jar:$(find $HOME/ava/scratch/vanilla-values/oracle/libraries/server -name "*.jar" | tr "\\n" ":")
$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin/javac -cp "$CP" -d . demo/Probe.java oracle/OracleMain.java
$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin/java -cp ".:$CP" oracle.OracleMain 2>&1 | sed -n "s/^\\[[^]]*\\] \\[main\/INFO\\]: \\[STDOUT\\]: //p" > java25.txt
```
```
