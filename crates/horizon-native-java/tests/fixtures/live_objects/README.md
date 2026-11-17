# Live game object facts

Captured with JDK 25 `javap -public` from `~/ava/minecraft-26.3-server-game.jar`.

```text
ServerPlayer extends net.minecraft.world.entity.player.Player
Player extends net.minecraft.world.entity.Avatar implements net.minecraft.world.entity.ContainerUser
LivingEntity extends net.minecraft.world.entity.Entity
Entity extends net.minecraft.world.entity.EntityAccess implements net.minecraft.world.level.entity.TraceableEntity
AbstractContainerMenu is abstract
```

Methods exercised by the probe:

```text
Entity: public boolean isRemoved();
Entity: public double getX();
Entity: public void setPos(double,double,double);
```

Architectury `dev.architectury.mixin.fabric.MixinServerPlayer#openMenu` (`javap -c -p`, callback body):

```text
getReturnValue().isPresent();
PlayerEvent.OPEN_MENU.invoker().open((Player)this, ((ServerPlayer)this).containerMenu);
```

The expected rows in `javap_facts.txt` are derived from these declarations and callback facts. Unsupported identity/class/position/write operations remain counted.
