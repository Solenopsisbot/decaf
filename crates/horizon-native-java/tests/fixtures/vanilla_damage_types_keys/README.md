Java 25 oracle for the `DamageTypes` key constants (`vanilla_registry_keys.rs`).

`java25.txt` is the output of `demo/Probe.java` run against the real 26.3 server jar on
`openjdk version "25.0.5" 2026-10-20 LTS`, from the repository root on arcweld:

```sh
. ~/ava/env.sh
L=$HOME/ava/scratch/vanilla-values/oracle/libraries/server
CP=$HOME/ava/minecraft-26.3-server-game.jar:$(find $L -name '*.jar' | tr '\n' ':')
F=crates/horizon-native-java/tests/fixtures/vanilla_damage_types_keys
javac -d out -cp "$CP" $F/demo/Probe.java
java -cp "out:$CP" demo.Probe > $F/java25.txt
```

The table `src/vanilla_damage_types_capture.txt` (`NAME|location`, declaration order) was checked
against the same jar by reflecting over `DamageTypes`' static fields: all 51 match, in order.
