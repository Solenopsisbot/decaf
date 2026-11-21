# Gson TypeAdapter dispatch

Capture the Java 25 answers from the real Gson 2.14.0 jar, then run the same
probe through Decaf.

```sh
GSON_JAR=${HORIZON_GSON_JAR:?set HORIZON_GSON_JAR to Gson 2.14.0}
rm -rf /tmp/gta-classes; mkdir -p /tmp/gta-classes
javac --release 25 -cp "$GSON_JAR" -d /tmp/gta-classes demo/Probe.java
java -cp /tmp/gta-classes:"$GSON_JAR" demo.Probe > java25.txt
cat java25.txt
```
