# DFU construction, Java 25 / 26.3 libraries

```sh
set -eu
J="$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin"
L="$HOME/ava/scratch/vanilla-values/oracle/libraries/server"
CP=$(find "$L" -name '*.jar' | paste -sd ':' -)
OUT=$(mktemp -d)
"$J/javac" -cp "$CP" -d "$OUT" demo/Probe.java
"$J/java" -cp "$OUT:$CP" demo.Probe
```
