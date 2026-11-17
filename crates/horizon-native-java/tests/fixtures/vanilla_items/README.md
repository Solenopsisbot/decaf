# Vanilla Items capture

`demo/ItemsCapture.java` bootstraps vanilla (`SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();`)
on Java 25 with the 26.3 server game jar and prints one `FIELD|registry id` row for each of the
1,218 `public static final Item` fields of `net.minecraft.world.item.Items`, in declaration order
(`java25.txt`). The class's other 35 static fields are `WeatheringCopperCollection<Item>` and
`ColorCollection<Item>` collections, not items: a mod reading one is counted.

Run from this folder:

```sh
J=$HOME/ava/jdk/jdk-25.0.5+8/Contents/Home/bin
CP=$HOME/ava/minecraft-26.3-server-game.jar:$(find $HOME/ava/scratch/vanilla-values/oracle/libraries/server -name '*.jar' | paste -sd: -)
O=$HOME/ava/scratch/vanilla-items-classes
$J/javac -cp "$CP" -d "$O" demo/ItemsCapture.java
$J/java -cp "$O:$CP" demo.ItemsCapture
```
