# Gson TypeAdapter dispatch

Capture the Java 25 answers from the real Gson 2.14.0 jar, then run the same
probe through Decaf.

```sh
GSON_JAR=~/ava/scratch/vanilla-values/oracle/libraries/server/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar
rm -rf /tmp/gta-classes; mkdir -p /tmp/gta-classes
javac --release 25 -cp "$GSON_JAR" -d /tmp/gta-classes demo/Probe.java
java -cp /tmp/gta-classes:"$GSON_JAR" demo.Probe > java25.txt
cat java25.txt
```
