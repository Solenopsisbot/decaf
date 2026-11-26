import demo.Probe;
import demo.Tally;
import java.lang.reflect.Field;
import java.util.IdentityHashMap;
import java.util.TreeMap;
import net.minecraft.SharedConstants;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.stats.Stat;
import net.minecraft.util.Prediction;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityEquipment;
import net.minecraft.world.entity.SlotAccess;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.ClickAction;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.phys.Vec3;
import sun.misc.Unsafe;

/**
 * Java 25 oracle for tests/fixture_item_parts.rs: the demo tally item (demo/*.java, compiled
 * against the real 26.3 server jar) run through its own overrides on the real ItemStack, Slot,
 * Inventory and player classes, printing the lines the Decaf fixture prints.
 *
 * The built-in registries are opened again after bootstrap (as a mod's registration runs before
 * they freeze) so the demo's onInitialize registers its component type and item, then every
 * item's components are bound as a server's reload binds them. The server player and its level
 * are made with Unsafe.allocateInstance and answer only what the overrides read: the inventory,
 * position and level; drops, stats and sounds are printed instead of carried out.
 */
public final class ItemPartsOracle {
  static Unsafe unsafe;
  static FakePlayer player;
  static ItemStack carried = ItemStack.EMPTY;
  static int takes;

  static final class FakeLevel extends ServerLevel {
    FakeLevel() {
      super(null, null, null, null, null, null, false, 0L, null, false);
    }

    @Override
    public void playSound(
        Entity except,
        double x,
        double y,
        double z,
        SoundEvent sound,
        SoundSource source,
        float volume,
        float pitch) {
      System.out.println(
          "sound "
              + sound.location()
              + " "
              + source
              + " "
              + volume
              + " "
              + pitch
              + " except="
              + (except == player));
    }
  }

  static final class FakePlayer extends ServerPlayer {
    FakePlayer() {
      super(null, null, null, null);
    }

    @Override
    public ItemEntity drop(ItemStack stack, boolean retainOwnership, Prediction prediction) {
      if (stack.isEmpty()) {
        return null;
      }
      System.out.println(
          "drop " + stack + " " + tally(stack) + " retain=" + retainOwnership + " " + prediction);
      return null;
    }

    @Override
    public void awardStat(Stat<?> stat, int amount) {
      System.out.println(
          "stat "
              + BuiltInRegistries.STAT_TYPE.getKey(stat.getType())
              + " "
              + BuiltInRegistries.ITEM.getKey((net.minecraft.world.item.Item) stat.getValue())
              + " "
              + amount);
    }
  }

  /** The stack's tally as NbtOps encodes it: each field `name=TYPE:value`, by name. */
  static String tally(ItemStack stack) {
    Tally tally = stack.get(Probe.TALLY);
    if (tally == null) {
      return "tally=none";
    }
    CompoundTag tag = (CompoundTag) Tally.CODEC.encodeStart(NbtOps.INSTANCE, tally).getOrThrow();
    TreeMap<String, Tag> fields = new TreeMap<>();
    for (String key : tag.keySet()) {
      fields.put(key, tag.get(key));
    }
    StringBuilder out = new StringBuilder("tally={");
    fields.forEach(
        (key, value) ->
            out.append(' ')
                .append(key)
                .append('=')
                .append(value.getType().getName())
                .append(':')
                .append(value));
    return out.append(" }").toString();
  }

  static Object allocate(Class<?> type) throws Exception {
    return unsafe.allocateInstance(type);
  }

  static void set(Class<?> owner, Object target, String name, Object value) throws Exception {
    Field field = owner.getDeclaredField(name);
    unsafe.putObject(target, unsafe.objectFieldOffset(field), value);
  }

  /** Unfreeze `registry`; one whose values hold their own holder takes intrusive ones again. */
  static void open(MappedRegistry<?> registry, boolean intrusiveHolders) throws Exception {
    Field frozen = MappedRegistry.class.getDeclaredField("frozen");
    frozen.setAccessible(true);
    frozen.setBoolean(registry, false);
    if (!intrusiveHolders) {
      return;
    }
    Field intrusive = MappedRegistry.class.getDeclaredField("unregisteredIntrusiveHolders");
    intrusive.setAccessible(true);
    intrusive.set(registry, new IdentityHashMap<>());
  }

  /** A plain `Slot` over the inventory, counting `onTake` (the body is `Slot`'s own). */
  static Slot slot(Inventory inventory, int index) {
    return new Slot(inventory, index, 0, 0) {
      @Override
      public void onTake(Player p, ItemStack stack) {
        takes++;
        super.onTake(p, stack);
      }
    };
  }

  static void click(String name, boolean carriedOnOther, int index, ClickAction action) {
    Inventory inventory = player.getInventory();
    Slot slot = slot(inventory, index);
    takes = 0;
    SlotAccess access = SlotAccess.of(() -> carried, stack -> carried = stack);
    boolean handled =
        carriedOnOther
            ? carried.overrideStackedOnOther(slot, action, player)
            : slot.getItem().overrideOtherStackedOnMe(carried, slot, action, player, access);
    System.out.println(
        "click "
            + name
            + " handled="
            + handled
            + " slot="
            + inventory.getItem(index)
            + " "
            + tally(inventory.getItem(index))
            + " carried="
            + carried
            + " "
            + tally(carried)
            + " takes="
            + takes);
  }

  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    open((MappedRegistry<?>) BuiltInRegistries.DATA_COMPONENT_TYPE, false);
    open((MappedRegistry<?>) BuiltInRegistries.ITEM, true);
    new Probe().onInitialize();
    for (var pending :
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(
            net.minecraft.data.registries.VanillaRegistries.createReloadableLookup(
                net.minecraft.data.registries.VanillaRegistries.createWorldLookup()))) {
      pending.apply();
    }
    Field theUnsafe = Unsafe.class.getDeclaredField("theUnsafe");
    theUnsafe.setAccessible(true);
    unsafe = (Unsafe) theUnsafe.get(null);

    FakeLevel level = (FakeLevel) allocate(FakeLevel.class);
    player = (FakePlayer) allocate(FakePlayer.class);
    EntityEquipment equipment = new EntityEquipment();
    Inventory inventory = new Inventory(player, equipment);
    set(Player.class, player, "inventory", inventory);
    set(net.minecraft.world.entity.LivingEntity.class, player, "equipment", equipment);
    set(Entity.class, player, "level", level);
    set(Entity.class, player, "position", new Vec3(1.5, 64.0, -2.5));

    inventory.setItem(0, new ItemStack(Probe.TALLY_ITEM));
    inventory.setItem(1, new ItemStack(Items.DIAMOND, 5));
    carried = new ItemStack(Items.STONE, 10);
    click("primary", false, 0, ClickAction.PRIMARY);
    click("insert", false, 0, ClickAction.SECONDARY);

    carried = inventory.getItem(0);
    inventory.setItem(0, ItemStack.EMPTY);
    click("take", true, 1, ClickAction.SECONDARY);

    ItemStack tally = carried;
    System.out.println(
        "bar visible="
            + tally.isBarVisible()
            + " width="
            + tally.getBarWidth()
            + " color="
            + tally.getBarColor());

    // The main hand: `LivingEntity.getItemInHand` reads the player's equipment.
    equipment.set(net.minecraft.world.entity.EquipmentSlot.MAINHAND, tally);
    InteractionResult result = Probe.TALLY_ITEM.use(level, player, InteractionHand.MAIN_HAND);
    System.out.println(
        "use result="
            + (result == InteractionResult.SUCCESS_SERVER ? "SUCCESS_SERVER" : result)
            + " held="
            + tally
            + " "
            + tally(tally));
  }
}
