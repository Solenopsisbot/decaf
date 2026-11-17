# Mixin service probe

The standalone probe records the source-verified answers. `MixinService.instance` is the cached singleton; `MixinServiceKnot.getName()` returns the literal `Knot/Fabric`; ModLauncher returns `ModLauncher`; `getSideName()` follows the environment side (`SERVER` on dedicated server). These values were checked with `javap -c -p` on the bundled loader libraries. Bytecode-provider access remains counted.

```sh
javac demo/Probe.java && java -cp demo Probe
```
