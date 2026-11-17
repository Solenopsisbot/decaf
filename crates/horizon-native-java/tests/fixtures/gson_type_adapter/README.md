# Gson TypeAdapter dispatch

```sh
GSON_JAR=~/ava/scratch/p14e/kit2/jars/gson-2.14.0.jar
rm -rf /tmp/gta-classes; mkdir -p /tmp/gta-classes
javac --release 25 -cp "$GSON_JAR" -d /tmp/gta-classes demo/Probe.java
java -cp /tmp/gta-classes:"$GSON_JAR" demo.Probe
```
