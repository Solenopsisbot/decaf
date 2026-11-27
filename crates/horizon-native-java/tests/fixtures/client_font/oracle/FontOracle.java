import com.mojang.blaze3d.font.GlyphInfo;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import demo.Label;
import demo.LabelTooltip;
import java.lang.reflect.Constructor;
import java.lang.reflect.Proxy;
import java.util.Map;
import net.minecraft.SharedConstants;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GlyphSource;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.font.glyphs.BakedGlyph;
import net.minecraft.client.gui.font.glyphs.EffectGlyph;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.resources.language.ClientLanguage;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.util.RandomSource;
import sun.misc.Unsafe;

/**
 * Java 25 oracle for tests/fixture_client_font.rs: demo.LabelTooltip (compiled against the real
 * 26.3 client jar) measured and drawn with the real client Font, StringSplitter, ClientLanguage
 * and GuiGraphicsExtractor, printing the lines the Decaf fixture prints.
 *
 * The Font is 26.3's own (`new Font(Provider)`), over a glyph provider whose every glyph's advance
 * is the fixture's table (`advance` below, the same rule as the test's `advances()`), so
 * Font.width and Font.split run vanilla's code on known metrics. The language is a real
 * ClientLanguage (made through its private constructor) holding the fixture's two translations.
 * Draws are recorded by a GuiGraphicsExtractor subclass made without its constructor: it
 * overrides the two leaves every overload the demo calls ends in, `text(Font,
 * FormattedCharSequence, int, int, int, boolean)` and `blitSprite(RenderPipeline, Identifier, int,
 * int, int, int, int)`; textWithWordWrap, centeredText and the other blitSprite overloads run
 * their real bodies. Nothing here opens a window or a connection (run it under `unshare -rn`).
 */
public final class FontOracle {
  /** The fixture's advance table: GUI pixels per glyph, for chars 0..127. */
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

  /** Prints the draws `extractImage` makes. */
  static final class Recording extends GuiGraphicsExtractor {
    Recording() {
      super(null, null, 0, 0);
    }

    @Override
    public void text(Font font, FormattedCharSequence line, int x, int y, int color, boolean shadow) {
      System.out.println(
          "  text \"" + FontOracle.text(line) + "\" " + x + " " + y + " " + color + " " + shadow);
    }

    @Override
    public void blitSprite(
        RenderPipeline pipeline, Identifier sprite, int x, int y, int w, int h, int color) {
      String which = pipeline == RenderPipelines.GUI_TEXTURED ? "gui_textured" : "other";
      System.out.println(
          "  sprite " + which + " " + sprite + " " + x + " " + y + " " + w + " " + h + " " + color);
    }
  }

  public static void main(String[] args) throws Exception {
    java.lang.reflect.Field theUnsafe = Unsafe.class.getDeclaredField("theUnsafe");
    theUnsafe.setAccessible(true);
    Unsafe unsafe = (Unsafe) theUnsafe.get(null);
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    Constructor<ClientLanguage> make =
        ClientLanguage.class.getDeclaredConstructor(Map.class, boolean.class);
    make.setAccessible(true);
    Language.inject(
        make.newInstance(
            Map.of(
                "demo.label.description", "Can hold mixed stacks of runes",
                "demo.label.title", "Pouch Label"),
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

    for (int kind = 1; kind <= 5; kind++) {
      ClientTooltipComponent component = new LabelTooltip(new Label(kind));
      int width = component.getWidth(font);
      int height = component.getHeight(font);
      System.out.println("label kind=" + kind + " width=" + width + " height=" + height);
      System.out.println("  draw at 10,20 box " + width + "x" + height);
      component.extractImage(
          font, 10, 20, width, height, (Recording) unsafe.allocateInstance(Recording.class));
    }
  }
}
