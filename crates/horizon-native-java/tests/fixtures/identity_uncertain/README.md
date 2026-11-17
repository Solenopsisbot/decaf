Java 25 oracle for `fixture_identity_uncertain.rs` (#464).

`java25.txt` is the output of `demo/Probe.java` on `openjdk version "25.0.5" 2026-10-20 LTS` on
arcweld, with the real Guava 33.6.0:

```sh
. ~/ava/env.sh
G=$(find ~/ava/scratch/vanilla-values/oracle/libraries/server -name 'guava-33.6.0-jre.jar')
javac -d out -cp "$G" demo/Probe.java
java -cp "out:$G" demo.Probe > java25.txt
```

`exact` must equal its row with nothing counted. The other rows are Java's answers for cases Decaf
counts (two equal strings that are different objects); the test asserts the counted calls.
