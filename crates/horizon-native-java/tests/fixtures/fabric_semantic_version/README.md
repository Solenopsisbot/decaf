# Fabric Loader SemanticVersion Java 25 capture

`java25.txt` is the raw output of `oracle/OracleMain.java` run on a real JDK 25 against the real
Fabric Loader 0.20.1 jar (Apache-2.0), for each text: `Version.parse(text)` and
`SemanticVersion.parse(text)`, then `demo/Probe.java` (`describe`) on what `Version.parse`
returned.

```
/usr/lib/jvm/java-25-openjdk/bin/java -version
  openjdk version "25.0.5.1" 2026-11-17
  OpenJDK Runtime Environment (build 25.0.5.1)
JAR=fabric-loader-0.20.1.jar   # SHA-256 d0c5a082e07410aaff9de7ab196aa6b4236002870f9ba3ad5c1260fcce5110ad
/usr/lib/jvm/java-25-openjdk/bin/javac -cp $JAR -d out demo/Probe.java oracle/OracleMain.java
/usr/lib/jvm/java-25-openjdk/bin/java -cp out:$JAR oracle.OracleMain > java25.txt
```

The capture was first recorded against Fabric Loader 0.19.3 and re-recorded on 0.20.1, the version
Decaf targets; the two outputs are byte-identical.

`stubs/` are compile-only stand-ins for the loader API the Decaf side of the test compiles
`demo/Probe.java` against (never run on Java). Only the texts the oracle parses to a
`SemanticVersion` are answered by the runtime (`src/fabric_version.rs`). Every other text is
counted at `ModMetadata.getVersion()`: the loader answers the texts the oracle shows as
`StringVersion` (`v1.0.0`, `abc`) with a `StringVersion`, which is not modelled, so no plain
`String` stands in for it. Every call without a row (`compareTo`, `equals`, `hashCode`,
`Version.parse`) on a version stays counted.
