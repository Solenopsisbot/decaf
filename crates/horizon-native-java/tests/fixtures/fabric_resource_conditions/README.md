Java 25 oracle for `fabric_resource_conditions.rs` (#454).

`java25.txt` is the output of `demo/Probe.java`'s `rows()`, and `java25_counted.txt` the output
of its other methods: the cases Decaf counts, kept here as Java's answers. Both come from
`openjdk version "25.0.5" 2026-10-20 LTS` on arcweld. The class path is the real Fabric API
resource-conditions module (`fabric-resource-conditions-api-v1-6.2.1+fd3b1d565d.jar`, nested in
`fabric-api-0.166.2+26.3.jar` under `META-INF/jars/`), the 26.3 server jar and its libraries:

```sh
. ~/ava/env.sh
unzip -p ~/ava/hj-corpus/deps-pool/fabric-api-0.166.2+26.3.jar \
  'META-INF/jars/fabric-resource-conditions-api-v1-6.2.1+fd3b1d565d.jar' > rc.jar
L=$HOME/ava/scratch/vanilla-values/oracle/libraries/server
CP=rc.jar:$HOME/ava/minecraft-26.3-server-game.jar:$(find $L -name '*.jar' | tr '\n' ':')
javac -nowarn -d out -cp "$CP" demo/Probe.java
java -cp "out:$CP" demo.Probe > java25.txt
java -cp "out:$CP" demo.Probe counted > java25_counted.txt
```

`fabricDefault=ok` in the counted file is the reason that case is counted: Fabric's own
`DefaultResourceConditionTypes` are registered by the module's `main` entrypoint, which this run
didn't execute. In the game it would normally have run first, and then the same call throws.
