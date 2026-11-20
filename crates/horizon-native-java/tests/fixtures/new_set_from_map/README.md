```sh
J="$JAVA_HOME/bin"   # a JDK 25.0.5
mkdir -p out
"$J/javac" -d out demo/Probe.java
"$J/java" -cp out demo.Probe
```
