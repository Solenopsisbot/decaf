# Dependency prelude fixture

Oracle command (Java 25):

```sh
javac --release 25 -d ~/ava/scratch/issue-457-prelude/classes oracle/OracleMain.java
~/ava/jdk/jdk-25.0.5+8/Contents/Home/bin/java -cp ~/ava/scratch/issue-457-prelude/classes OracleMain > java25.txt
```

Fabric runs dependency `main` before the dependent `main`; the second line is the dependent observation after initialization.
