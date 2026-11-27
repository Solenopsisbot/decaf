import com.github.theredbrain.bundleapi.client.gui.tooltip.CustomBundleTooltipComponent;
import com.github.theredbrain.bundleapi.component.type.CustomBundleContentsComponent;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.font.GlyphInfo;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.serialization.JsonOps;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Proxy;
import java.util.IdentityHashMap;
import java.util.Map;
import net.minecraft.SharedConstants;
import net.minecraft.client.ClientBootstrap;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GlyphSource;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.font.glyphs.BakedGlyph;
import net.minecraft.client.gui.font.glyphs.EffectGlyph;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.resources.language.ClientLanguage;
import net.minecraft.core.MappedRegistry;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.util.RandomSource;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import sun.misc.Unsafe;

/**
 * Java 25 oracle for tests/fixture_runes_client_items.rs: Bundle API 5.0.0's own
 * CustomBundleTooltipComponent (the jar Runes 1.3.2 nests, on the classpath; its Mixins don't
 * run) measured and drawn on the real 26.3 client: vanilla's Font, StringSplitter, ClientLanguage
 * and GuiGraphicsExtractor.
 *
 * - The pouch is Runes' small rune pouch: its tooltip component is
 *   `new CustomBundleTooltipComponent(contents, Component.translatable(
 *   "item.runes.rune_pouch.empty.description"))`, as the jar's TooltipComponentMixin builds it from
 *   CustomBundleItem.getTooltipImage's data (the item's emptyDescription field, set by Runes'
 *   RuneCraftingItems), and its contents decode through Bundle API's own CODEC from the stored
 *   `{"content": [...], "size_multiplier": 4}`.
 * - `runes:fire_stone` is registered as a plain Item under that id (the registry opened as a mod's
 *   registration finds it; its components bound as a reload binds them): what the tooltip reads
 *   of a rune is its id, count and max stack size (64), as a plain item's.
 * - The Font is 26.3's own over a glyph provider whose advances are the test's table (`advance`,
 *   the same as FontOracle's), the language a real ClientLanguage holding the three translations
 *   the tooltip names (Runes' en_us, and vanilla 26.3's en_us for the bundle labels).
 * - A GuiGraphicsExtractor subclass made without its constructor records the leaves the
 *   component's calls end in: text(Font, FormattedCharSequence, int, int, int, boolean),
 *   blitSprite(RenderPipeline, Identifier, int, int, int, int, int), and item(ItemStack, int, int,
 *   int) and itemDecorations(Font, ItemStack, int, int) themselves (their bodies read the
 *   Minecraft instance, which a tooltip draws through and this oracle doesn't make).
 *
 * Nothing here opens a window or a connection; run it under `unshare -rn`, and it stops before
 * Minecraft.<init> (no Minecraft instance is made at all).
 */
public final class PouchTooltipOracle {
  static float advance(int c) {
    if (c < 32 || c >= 128) return 0f;
    if (c == ' ') return 4f;
    if ("i!.,:;'|".indexOf(c) >= 0) return 2f;
    if (c == 'l') return 3f;
    if ("tI[]".indexOf(c) >= 0) return 4f;
    if ("fk<>{}()\"*".indexOf(c) >= 0) return 5f;
    if (c == 'q') return 5.5f;
    if (c == '@' || c == '~') return 7f;
    return 6f;
  }

  static String text(FormattedCharSequence line) {
    StringBuilder out = new StringBuilder();
    line.accept((index, style, codePoint) -> {
      out.appendCodePoint(codePoint);
      return true;
    });
    return out.toString();
  }

  static final class Recording extends GuiGraphicsExtractor {
    Recording() {
      super(null, null, 0, 0);
    }

    @Override
    public void text(Font font, FormattedCharSequence line, int x, int y, int color, boolean shadow) {
      System.out.println(
          "  text \"" + PouchTooltipOracle.text(line) + "\" " + x + " " + y + " " + color + " " + shadow);
    }

    @Override
    public void blitSprite(
        RenderPipeline pipeline, Identifier sprite, int x, int y, int w, int h, int color) {
      String which = pipeline == RenderPipelines.GUI_TEXTURED ? "gui_textured" : "other";
      System.out.println(
          "  sprite " + which + " " + sprite + " " + x + " " + y + " " + w + " " + h + " " + color);
    }

    @Override
    public void item(ItemStack stack, int x, int y, int seed) {
      System.out.println(
          "  item " + BuiltInRegistries.ITEM.getKey(stack.getItem()) + " " + stack.getCount() + " "
              + x + " " + y + " " + seed);
    }

    @Override
    public void itemDecorations(Font font, ItemStack stack, int x, int y) {
      System.out.println(
          "  decorations " + BuiltInRegistries.ITEM.getKey(stack.getItem()) + " "
              + stack.getCount() + " " + x + " " + y);
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

  public static void main(String[] args) throws Exception {
    Field theUnsafe = Unsafe.class.getDeclaredField("theUnsafe");
    theUnsafe.setAccessible(true);
    Unsafe unsafe = (Unsafe) theUnsafe.get(null);
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    ClientBootstrap.bootstrap();
    open((MappedRegistry<?>) BuiltInRegistries.ITEM);
    Identifier rune = Identifier.fromNamespaceAndPath("runes", "fire_stone");
    Registry.register(
        BuiltInRegistries.ITEM,
        rune,
        new Item(new Item.Properties().setId(ResourceKey.create(Registries.ITEM, rune))));
    for (var pending :
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(
            net.minecraft.data.registries.VanillaRegistries.createReloadableLookup(
                net.minecraft.data.registries.VanillaRegistries.createWorldLookup()))) {
      pending.apply();
    }
    Constructor<ClientLanguage> make =
        ClientLanguage.class.getDeclaredConstructor(Map.class, boolean.class);
    make.setAccessible(true);
    Language.inject(
        make.newInstance(
            Map.of(
                "item.runes.rune_pouch.empty.description", "Can hold mixed stacks of runes",
                "item.minecraft.bundle.empty", "Empty",
                "item.minecraft.bundle.full", "Full"),
            false));

    GlyphSource glyphs =
        new GlyphSource() {
          @Override
          public BakedGlyph getGlyph(int codePoint) {
            GlyphInfo info = GlyphInfo.simple(advance(codePoint));
            return (BakedGlyph)
                Proxy.newProxyInstance(
                    BakedGlyph.class.getClassLoader(),
                    new Class<?>[] {BakedGlyph.class},
                    (proxy, method, margs) -> {
                      if (method.getName().equals("info")) return info;
                      throw new AssertionError("a glyph was drawn: " + method);
                    });
          }

          @Override
          public BakedGlyph getRandomGlyph(RandomSource random, int width) {
            throw new AssertionError("no obfuscated text here");
          }
        };
    Font font =
        new Font(
            new Font.Provider() {
              @Override
              public GlyphSource glyphs(FontDescription description) {
                return glyphs;
              }

              @Override
              public EffectGlyph effect() {
                throw new AssertionError("no effect glyph here");
              }
            });

    String[][] pouches = {
      {"empty", "[]"},
      {"partly", "[{\"id\":\"runes:fire_stone\",\"count\":10}]"},
      {
        "full",
        "[{\"id\":\"runes:fire_stone\",\"count\":64},{\"id\":\"runes:fire_stone\",\"count\":64},"
            + "{\"id\":\"runes:fire_stone\",\"count\":64},{\"id\":\"runes:fire_stone\",\"count\":64}]"
      },
    };
    for (String[] pouch : pouches) {
      CustomBundleContentsComponent contents =
          CustomBundleContentsComponent.CODEC
              .parse(
                  JsonOps.INSTANCE,
                  JsonParser.parseString(
                      "{\"content\":" + pouch[1] + ",\"size_multiplier\":4}"))
              .getOrThrow();
      ClientTooltipComponent component =
          new CustomBundleTooltipComponent(
              contents, Component.translatable("item.runes.rune_pouch.empty.description"));
      int width = component.getWidth(font);
      int height = component.getHeight(font);
      System.out.println("pouch " + pouch[0] + " width=" + width + " height=" + height);
      System.out.println("  draw at 8,8 box " + width + "x" + height);
      component.extractImage(
          font, 8, 8, width, height, (Recording) unsafe.allocateInstance(Recording.class));
    }
  }
}
