# Mixin service probe

This capture invokes the real Fabric Loader `MixinServiceKnot` reflectively from `fabric-loader-0.19.5.jar`. `javap -c -p` shows `getName()` and `getSideName()` call `FabricLauncherBase.getLauncher()`, so without a launcher both are recorded as `COUNTED`; no Decaf default is claimed. Bytecode-provider access is counted separately.

```sh
javac --release 25 -cp "$HOME/ava/hj-tools/fabric-loader-0.19.5.jar" -d /tmp/mixin-probe-classes demo/Probe.java
java -cp /tmp/mixin-probe-classes:"$HOME/ava/hj-tools/fabric-loader-0.19.5.jar" demo.Probe > java25.txt
cat java25.txt
```
