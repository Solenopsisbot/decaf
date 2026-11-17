Oracle for `fixture_neoforge_extension_point.rs` (#468).

FancyModLoader can't run outside the game (`ModLoadingContext.getActiveContainer` reads
`ModList`), so `javap_facts.txt` holds FML 12.0.12's own bytecode for the calls involved, and the
test derives its expectations from those lines. Produced on arcweld with JDK 25.0.5+8:

```sh
. ~/ava/env.sh
J=~/ava/scratch/neoforge-loader-model/loader-12.0.12.jar
S='public <T extends net.neoforged.fml.IExtensionPoint> void registerExtensionPoint(java.lang.Class<T>, java.util.function.Supplier<T>);'
javap -c -cp $J net.neoforged.fml.ModLoadingContext | sed -n "/$S/,/return/p"
javap -c -p -cp $J net.neoforged.fml.ModContainer | sed -n "/$S/,/return/p"
javap -c -p -cp $J net.neoforged.fml.ModContainer | grep -E 'IdentityHashMap."<init>"|putfield.*extensionPoints'
```
