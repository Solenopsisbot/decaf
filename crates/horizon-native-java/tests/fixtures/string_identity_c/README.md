Java 25 oracle for `fixture_string_identity` (#544, option c).

`java25.txt` is captured from the real API with Java 25 using the same source:

```sh
. ~/ava/env.sh
javac --release 21 -d out demo/Probe.java
java -cp out demo.Probe > java25.txt
```

The `normal` row is exact. Identity-sensitive rows retain Java's result and require counted calls because the native String value keeps text without allocation provenance.
