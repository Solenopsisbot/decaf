import demo.BenchMenu;
import demo.Probe;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundSetPlayerInventoryPacket;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.stats.Stat;
import net.minecraft.util.Prediction;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityEquipment;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Abilities;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.ContainerLevelAccess;
import net.minecraft.world.inventory.ContainerListener;
import net.minecraft.world.inventory.ContainerSynchronizer;
import net.minecraft.world.inventory.RemoteSlot;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import sun.misc.Unsafe;

/**
 * Java 25 oracle for tests/fixture_live_menu.rs: the demo bench menu (demo/*.java, compiled
 * against the real 26.3 server jar) driven by script.txt with the real AbstractContainerMenu /
 * ItemCombinerMenu / Slot / SimpleContainer / Inventory / ItemStack classes.
 *
 * The server player, its level and connection are built with Unsafe.allocateInstance (no
 * constructor runs) and only answer what a running server would: the inventory, abilities,
 * position, block interaction range, the block at the bench, and the packets and stats it is
 * sent, which are recorded as the same effect lines Decaf reports. The player's container
 * listener and synchronizer are written out as ServerPlayer$2 / ServerPlayer$1 are, printing
 * instead of sending. A click runs what ServerGamePacketListenerImpl.handleContainerClick
 * runs, without the client's predicted slots (Decaf takes none).
 */
public final class LiveMenuOracle {
  static final List<String> EFFECTS = new ArrayList<>();
  static Unsafe unsafe;
  static FakePlayer player;
  static BenchMenu menu;
  static int nextContainerId = 1;
  static final BlockPos POS = new BlockPos(3, 64, 5);
  /** Whether the step is an open: only then is the client's stateId Decaf's to know too. */
  static boolean opening;

  static String stack(ItemStack stack) {
    return stack.toString();
  }

  static final class FakeState extends BlockState {
    FakeState() {
      super(null, null, null);
    }

    @Override
    public boolean is(Block block) {
      return block == Probe.BENCH;
    }
  }

  static final class FakeLevel extends ServerLevel {
    FakeLevel() {
      super(null, null, null, null, null, null, false, 0L, null, false);
    }

    @Override
    public net.minecraft.world.flag.FeatureFlagSet enabledFeatures() {
      return net.minecraft.world.flag.FeatureFlags.VANILLA_SET;
    }

    @Override
    public BlockState getBlockState(BlockPos pos) {
      if (pos.equals(POS)) {
        return (BlockState) allocate(FakeState.class);
      }
      throw new AssertionError("getBlockState " + pos);
    }
  }

  static final class FakeConnection extends ServerGamePacketListenerImpl {
    FakeConnection() {
      super(null, null, null, null);
    }

    @Override
    public void send(Packet<?> packet) {
      if (packet instanceof ClientboundSetPlayerInventoryPacket inv) {
        EFFECTS.add("inventory_sent " + inv.slot() + " " + stack(inv.contents()));
      } else {
        throw new AssertionError("packet " + packet);
      }
    }
  }

  static final class FakePlayer extends ServerPlayer {
    double range = 4.5;

    FakePlayer() {
      super(null, null, null, null);
    }

    @Override
    public ItemEntity drop(ItemStack stack, boolean retainOwnership, Prediction prediction) {
      if (stack.isEmpty()) {
        return null;
      }
      EFFECTS.add("drop " + stack(stack) + " retain=" + retainOwnership + " " + prediction);
      return null;
    }

    @Override
    public void awardStat(Stat<?> stat, int amount) {
      EFFECTS.add(
          "stat "
              + BuiltInRegistries.STAT_TYPE.getKey(stat.getType())
              + " "
              + BuiltInRegistries.ITEM.getKey((net.minecraft.world.item.Item) stat.getValue())
              + " "
              + amount);
    }

    @Override
    public double blockInteractionRange() {
      return range;
    }

    @Override
    public boolean hasDisconnected() {
      return false;
    }
  }

  /** ServerPlayer$2. */
  static final class Listener implements ContainerListener {
    @Override
    public void slotChanged(AbstractContainerMenu menu, int index, ItemStack stack) {
      Slot slot = menu.getSlot(index);
      if (slot instanceof net.minecraft.world.inventory.ResultSlot) {
        return;
      }
      if (slot.container == player.getInventory()) {
        EFFECTS.add("criterion " + stack(stack));
      }
    }

    @Override
    public void dataChanged(AbstractContainerMenu menu, int index, int value) {}
  }

  /** ServerPlayer$1, printing its packets. */
  static final class Synchronizer implements ContainerSynchronizer {
    @Override
    public void sendInitialData(
        AbstractContainerMenu menu, List<ItemStack> items, ItemStack carried, int[] data) {
      int state = menu.incrementStateId();
      StringBuilder line =
          new StringBuilder("initial " + menu.containerId + " state=" + (opening ? state : "?") + " [");
      for (int i = 0; i < items.size(); i++) {
        if (!items.get(i).isEmpty()) {
          line.append(" ").append(i).append(":").append(stack(items.get(i)));
        }
      }
      EFFECTS.add(line.append(" ] carried=").append(stack(carried)).toString());
      for (int i = 0; i < data.length; i++) {
        EFFECTS.add("data " + menu.containerId + " " + i + "=" + data[i]);
      }
    }

    @Override
    public void sendSlotChange(AbstractContainerMenu menu, int slot, ItemStack stack) {
      menu.incrementStateId();
      EFFECTS.add("slot " + menu.containerId + " " + slot + " " + stack(stack));
    }

    @Override
    public void sendCarriedChange(AbstractContainerMenu menu, ItemStack stack) {
      EFFECTS.add("carried " + stack(stack));
    }

    @Override
    public void sendDataChange(AbstractContainerMenu menu, int index, int value) {
      EFFECTS.add("data " + menu.containerId + " " + index + "=" + value);
    }

    @Override
    public RemoteSlot createSlot() {
      return new RemoteSlot.Synchronized(component -> 0);
    }
  }

  static Object allocate(Class<?> type) {
    try {
      return unsafe.allocateInstance(type);
    } catch (InstantiationException e) {
      throw new RuntimeException(e);
    }
  }

  static void set(Class<?> owner, Object target, String name, Object value) throws Exception {
    Field field = owner.getDeclaredField(name);
    field.setAccessible(true);
    long offset = unsafe.objectFieldOffset(field);
    unsafe.putObject(target, offset, value);
  }

  static void setFloat(Class<?> owner, Object target, String name, float value) throws Exception {
    Field field = owner.getDeclaredField(name);
    long offset = unsafe.objectFieldOffset(field);
    unsafe.putFloat(target, offset, value);
  }

  static void printState() {
    System.out.println("carried=" + stack(menu.getCarried()));
    for (int i = 0; i < menu.slots.size(); i++) {
      ItemStack item = menu.slots.get(i).getItem();
      if (!item.isEmpty()) {
        System.out.println("slot " + i + "=" + stack(item));
      }
    }
  }

  static void flush() {
    for (String effect : EFFECTS) {
      System.out.println("effect: " + effect);
    }
    EFFECTS.clear();
  }

  /** ServerGamePacketListenerImpl.handleContainerClick offsets 68-352, without predictions. */
  static void click(int slot, int button, ContainerInput input, boolean stateMatches) {
    if (!menu.stillValid(player)) {
      System.out.println("dropped: invalid menu");
      return;
    }
    if (!menu.isValidSlotIndex(slot)) {
      System.out.println("dropped: invalid slot");
      return;
    }
    menu.suppressRemoteUpdates();
    menu.clicked(slot, button, input, player);
    menu.resumeRemoteUpdates();
    if (stateMatches) {
      menu.broadcastChanges();
    } else {
      menu.broadcastFullState();
    }
  }

  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    // Bind every item's default components as a server's reload does
    // (ReloadableServerResources: DATA_COMPONENT_INITIALIZERS.build(lookup), apply), over the
    // vanilla registries the data generator builds.
    for (var pending :
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(
            net.minecraft.data.registries.VanillaRegistries.createReloadableLookup(
                net.minecraft.data.registries.VanillaRegistries.createWorldLookup()))) {
      pending.apply();
    }
    Field theUnsafe = Unsafe.class.getDeclaredField("theUnsafe");
    theUnsafe.setAccessible(true);
    unsafe = (Unsafe) theUnsafe.get(null);

    Probe.BENCH = (Block) allocate(demo.Bench.class);
    FakeLevel level = (FakeLevel) allocate(FakeLevel.class);
    player = (FakePlayer) allocate(FakePlayer.class);
    player.range = 4.5;
    Inventory inventory = new Inventory(player, new EntityEquipment());
    set(Player.class, player, "inventory", inventory);
    set(Player.class, player, "abilities", new Abilities());
    set(Entity.class, player, "level", level);
    set(Entity.class, player, "position", new Vec3(3.5, 65.0, 7.5));
    setFloat(Entity.class, player, "eyeHeight", 1.62f);
    player.connection = (ServerGamePacketListenerImpl) allocate(FakeConnection.class);
    inventory.setItem(0, new ItemStack(Items.COBBLESTONE, 10));
    inventory.setItem(1, new ItemStack(Items.AMETHYST_SHARD, 3));
    inventory.setItem(2, new ItemStack(Items.DIRT, 5));
    if (args.length > 1 && args[1].equals("item_click")) {
      // script_item_click.txt: the demo grab item in hotbar slot 3, registered as a mod's
      // item is (the built-in item registry opened again), its components bound as the
      // reload above binds every item's.
      MappedRegistry<?> items = (MappedRegistry<?>) BuiltInRegistries.ITEM;
      Field frozen = MappedRegistry.class.getDeclaredField("frozen");
      frozen.setAccessible(true);
      frozen.setBoolean(items, false);
      Field intrusive = MappedRegistry.class.getDeclaredField("unregisteredIntrusiveHolders");
      intrusive.setAccessible(true);
      intrusive.set(items, new java.util.IdentityHashMap<>());
      demo.GrabProbe.register();
      for (var pending :
          BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(
              net.minecraft.data.registries.VanillaRegistries.createReloadableLookup(
                  net.minecraft.data.registries.VanillaRegistries.createWorldLookup()))) {
        pending.apply();
      }
      inventory.setItem(3, new ItemStack(demo.GrabProbe.GRAB, 1));
    }

    for (String raw : Files.readAllLines(Path.of(args[0]))) {
      String line = raw.strip();
      if (line.isEmpty() || line.startsWith("#")) {
        continue;
      }
      System.out.println("== " + line);
      String[] parts = line.split(" ");
      switch (parts[0]) {
        case "open" -> {
          opening = true;
          // ServerPlayer.openMenu: createMenu, then initMenu (addSlotListener,
          // setSynchronizer).
          menu = new BenchMenu(nextContainerId++, inventory, ContainerLevelAccess.create(level, POS));
          menu.addSlotListener(new Listener());
          menu.setSynchronizer(new Synchronizer());
          opening = false;
        }
        case "click", "mismatch" ->
            click(
                Integer.parseInt(parts[1]),
                Integer.parseInt(parts[2]),
                ContainerInput.valueOf(parts[3]),
                parts[0].equals("click"));
        case "creative" -> player.getAbilities().instabuild = Boolean.parseBoolean(parts[1]);
        case "tick" -> {
          // ServerPlayer.tick offsets 38-65.
          menu.broadcastChanges();
          if (!menu.stillValid(player)) {
            EFFECTS.add("closed " + menu.containerId);
            menu.removed(player);
            menu = null;
          }
        }
        case "close" -> {
          // handleContainerClose -> doCloseContainer -> removed.
          menu.removed(player);
          menu = null;
        }
        case "far" -> set(Entity.class, player, "position", new Vec3(3.5, 65.0, 15.5));
        default -> throw new AssertionError(line);
      }
      flush();
      if (menu != null) {
        printState();
      }
      StringBuilder inv = new StringBuilder("inventory:");
      for (int i = 0; i < inventory.getContainerSize(); i++) {
        if (!inventory.getItem(i).isEmpty()) {
          inv.append(" ").append(i).append("=").append(stack(inventory.getItem(i)));
        }
      }
      System.out.println(inv);
    }
  }
}
