# Thread probe (`fixture_parked_threads`)

`Probe.java` is the class the test ships. `null_name.java25.txt` is Java 25's answer for `nullName()`
(`new Thread((String) null)`), captured on arcweld with the same body run as a plain program:

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
$J/javac -d out T.java && $J/java -cp out T > null_name.java25.txt
```

`T.java` (beside this file) is `nullName()`'s try/catch in a `main`. `java -version`: openjdk 25.0.5 2026-10-20 LTS
(Temurin-25.0.5+8). The file has no trailing newline.
