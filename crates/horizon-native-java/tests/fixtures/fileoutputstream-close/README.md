# FileOutputStream.close fixture

On arcweld, from a clean copy of this folder:
```sh
J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
rm -rf fos-scratch *.class demo/*.class
"$J/javac" --release 21 demo/Probe.java
"$J/java" -cp . demo.Probe
```
Java version: `openjdk 25.0.5 2025-10-21`
