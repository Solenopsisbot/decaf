# Gson separator rows (#211)

`separator_rows.java25.txt` is the output of `separatorRows()` (the probe method `fixture_gson_builder.rs` builds
into its demo class; the same source is `Sep.java` here) on Java 25 with the real Gson 2.14.0, captured on arcweld:

```sh
J=~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
G=.../libraries/server/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar   # the 26.3 server's own Gson
$J/javac -d out -cp $G demo/Sep.java
$J/java -cp out:$G demo.Sep > separator_rows.java25.txt
```

`java -version`: openjdk 25.0.5 2026-10-20 LTS (Temurin-25.0.5+8). The file has no trailing newline.
