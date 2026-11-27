import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import com.mojang.serialization.MapCodec;
import demo.FillProperty;
import demo.Meter;
import demo.MeterTooltip;
import demo.Probe;
import java.lang.reflect.Field;
import java.util.IdentityHashMap;
import java.util.Optional;
import net.minecraft.SharedConstants;
import net.minecraft.client.ClientBootstrap;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.item.properties.numeric.RangeSelectItemModelProperties;
import net.minecraft.client.renderer.item.properties.numeric.RangeSelectItemModelProperty;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ExtraCodecs;
import net.minecraft.world.entity.ItemOwner;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.inventory.tooltip.TooltipComponent;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import sun.misc.Unsafe;

/**
 * Java 25 oracle for tests/fixture_client_item_parts.rs: the demo (demo/*.java, compiled against
 * the real 26.3 client jar) on the real client classes, printing the lines the Decaf fixture
 * prints.
 *
 * The client bootstraps as its Main does (Bootstrap.bootStrap(), then ClientBootstrap.bootstrap(),
 * which ends with RangeSelectItemModelProperties.bootstrap()); PropertiesMixin's TAIL put is then
 * made on the same ID_MAPPER through reflection, as Mixin would merge it. The item registry is
 * opened again for the demo item (a mod registers before it freezes) and every item's components
 * are bound as a reload binds them. A node is decoded through
 * vanilla's RangeSelectItemModelProperties.MAP_CODEC (the "property" dispatch Decaf's embedder
 * makes by id). CreateMixin's mapping is its handler's body; vanilla's own create is checked to
 * refuse a Meter. Draws are recorded by a GuiGraphicsExtractor subclass made without its
 * constructor.
 */
public final class ClientItemPartsOracle {
  static Unsafe unsafe;

  /** Prints the draws `extractImage` makes. */
  static final class Recording extends GuiGraphicsExtractor {
    Recording() {
      super(null, null, 0, 0);
    }

    @Override
    public void fill(int x0, int y0, int x1, int y1, int color) {
      System.out.println("  fill " + x0 + " " + y0 + " " + x1 + " " + y1 + " " + color);
    }
  }

  static void open(MappedRegistry<?> registry) throws Exception {
    Field frozen = MappedRegistry.class.getDeclaredField("frozen");
    frozen.setAccessible(true);
    frozen.setBoolean(registry, false);
    Field intrusive = MappedRegistry.class.getDeclaredField("unregisteredIntrusiveHolders");
    intrusive.setAccessible(true);
    intrusive.set(registry, new IdentityHashMap<>());
  }

  @SuppressWarnings("unchecked")
  public static void main(String[] args) throws Exception {
    Field theUnsafe = Unsafe.class.getDeclaredField("theUnsafe");
    theUnsafe.setAccessible(true);
    unsafe = (Unsafe) theUnsafe.get(null);
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    ClientBootstrap.bootstrap();
    Field mapper = RangeSelectItemModelProperties.class.getDeclaredField("ID_MAPPER");
    mapper.setAccessible(true);
    ((ExtraCodecs.LateBoundIdMapper<Identifier, MapCodec<? extends RangeSelectItemModelProperty>>)
            mapper.get(null))
        .put(Identifier.fromNamespaceAndPath("demo", "fill"), FillProperty.CODEC);
    System.out.println("registered demo:fill");
    // The demo item registers as a mod's would, before the registries freeze; then every item's
    // components are bound as a reload binds them.
    open((MappedRegistry<?>) BuiltInRegistries.ITEM);
    new Probe().onInitialize();
    for (var pending :
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(
            net.minecraft.data.registries.VanillaRegistries.createReloadableLookup(
                net.minecraft.data.registries.VanillaRegistries.createWorldLookup()))) {
      pending.apply();
    }

    ClientLevel level = (ClientLevel) unsafe.allocateInstance(ClientLevel.class);
    ItemOwner owner = (ItemOwner) unsafe.allocateInstance(ArmorStand.class);
    String[] nodes = {
      "{\"type\":\"minecraft:range_dispatch\",\"property\":\"demo:fill\",\"entries\":[]}",
      "{\"type\":\"minecraft:range_dispatch\",\"property\":\"demo:fill\",\"divisor\":16,\"entries\":[]}",
    };
    for (String node : nodes) {
      RangeSelectItemModelProperty property =
          RangeSelectItemModelProperties.MAP_CODEC
              .codec()
              .parse(JsonOps.INSTANCE, JsonParser.parseString(node))
              .getOrThrow();
      System.out.println("decode " + node + " ok");
      int[][] cases = {{1, 0, 0, 0}, {16, 0, 0, 0}, {64, 0, 0, 0}, {48, 1, 0, 0}, {48, 0, 1, 0}, {8, 1, 1, 7}};
      for (int[] c : cases) {
        float value =
            property.get(
                new ItemStack(Items.DIAMOND, c[0]),
                c[1] == 1 ? level : null,
                c[2] == 1 ? owner : null,
                c[3]);
        System.out.println(
            "  get count=" + c[0] + " level=" + (c[1] == 1) + " owner=" + (c[2] == 1)
                + " seed=" + c[3] + " -> " + value);
      }
    }

    String refused =
        "{\"type\":\"minecraft:range_dispatch\",\"property\":\"demo:fill\",\"divisor\":\"many\",\"entries\":[]}";
    if (RangeSelectItemModelProperties.MAP_CODEC
        .codec()
        .parse(JsonOps.INSTANCE, JsonParser.parseString(refused))
        .isError()) {
      System.out.println("decode " + refused + " rejected");
    }
    for (int count : new int[] {1, 3, 6}) {
      Optional<TooltipComponent> image = Probe.METER.getTooltipImage(new ItemStack(Probe.METER, count));
      if (image.isEmpty()) {
        System.out.println("tooltip_image count=" + count + " -> empty");
        continue;
      }
      System.out.println("tooltip_image count=" + count + " -> present");
      Meter meter = (Meter) image.get();
      try {
        ClientTooltipComponent.create(meter);
        throw new AssertionError("vanilla create took a Meter");
      } catch (IllegalArgumentException expected) {
        // Vanilla's body: only CreateMixin's HEAD callback maps it.
      }
      ClientTooltipComponent component = new MeterTooltip(meter);
      System.out.println(
          "  component width=" + component.getWidth(null) + " height=" + component.getHeight(null));
      System.out.println("  draw at 10,20 box 80x50");
      component.extractImage(null, 10, 20, 80, 50, (Recording) unsafe.allocateInstance(Recording.class));
    }
  }
}
