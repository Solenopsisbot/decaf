# vanilla_static Java 25 capture

Run from this fixture directory (`crates/horizon-native-java/tests/fixtures/vanilla_static`):

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
L=~/ava/scratch/vanilla-values/oracle/libraries/server
CP=~/ava/minecraft-26.3-server-game.jar:$(find "$L" -name '*.jar' | tr '\n' ':')
rm -rf classes && mkdir -p classes
$J/javac -cp "$CP" -d classes demo/Probe.java oracle/Capture483.java
$J/java -cp "classes:$CP" Capture483 > java25.txt
```

The first field is `CommonComponents.EMPTY.getString()`, whose value is the empty string, so the capture begins with `|`.
