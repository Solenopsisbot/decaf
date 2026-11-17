# live_objects

Oracle for `fixture_live_objects.rs`. A live game object exists only in the running game, so no JVM
run can answer these rows. The test takes its class rows from the 26.3 jar's own declarations.

`javap_facts.txt` is javap output, nothing else: line 2 (the class declaration) of each class, then
the three `Entity` methods the probe calls. Produced on arcweld with JDK 25.0.5+8:

```sh
. ~/ava/env.sh
J=~/ava/minecraft-26.3-server-game.jar
for c in net.minecraft.server.level.ServerPlayer net.minecraft.world.entity.player.Player \
         net.minecraft.world.entity.Avatar net.minecraft.world.entity.LivingEntity \
         net.minecraft.world.entity.Entity net.minecraft.world.inventory.AbstractContainerMenu; do
  javap -cp $J $c | sed -n 2p
done
javap -cp $J net.minecraft.world.entity.Entity \
  | grep -E '^  public final boolean isRemoved\(\);|^  public final double getX\(\);|^  public void setPos\(double, double, double\);'
```

The chain is `ServerPlayer` → `Player` → `Avatar` → `LivingEntity` → `Entity`. `Avatar` is easy to
miss, and the test derives each expected `instanceof` by following these `extends` clauses.

Only these excerpts are committed, never a full listing of a Minecraft class.
