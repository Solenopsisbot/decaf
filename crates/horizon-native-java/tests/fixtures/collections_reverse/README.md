Java 25 capture for the Collections.reverse and Stream.iterator fixture.

```sh
J="$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin"
S="$(mktemp -d)"
trap "rm -rf \"$S\"" EXIT
"$J/javac" --release 21 -d "$S" demo/Probe.java
"$J/java" -cp "$S" demo.Probe
```
