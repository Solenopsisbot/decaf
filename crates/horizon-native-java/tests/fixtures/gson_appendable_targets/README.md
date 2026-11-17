Java 25 oracle for `fixture_gson_appendable_targets.rs` (#458).

`java25.txt` is the output of `demo/Probe.java`'s `rows()`, and `java25_counted.txt` the output of
`unmodelled()` (a case Decaf counts, kept as Java's answer). Both come from
`openjdk version "25.0.5" 2026-10-20 LTS` on arcweld, with Gson 2.14.0, run in an empty directory
(the probe writes `game/gson-appendable/`):

```sh
. ~/ava/env.sh
G=~/ava/scratch/vanilla-values/oracle/libraries/server/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar
javac -d out -cp "$G" demo/Probe.java
java -cp "out:$G" demo.Probe > java25.txt
java -cp "out:$G" demo.Probe counted > java25_counted.txt
```
