# Collectors.toUnmodifiableMap

```sh
set -eu
J="$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin"
"$J/javac" --release 25 -d out demo/Probe.java
"$J/java" -cp out demo.Probe | head -1 > java25.txt
```
