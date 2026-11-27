package demo;

import java.util.List;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.util.FormattedCharSequence;

/**
 * The client side of a {@link Label}: its text wrapped to 40 pixels (Font.split,
 * textWithWordWrap), GUI sprites on GUI_TEXTURED in each blitSprite overload (a zero-width one
 * among them), and centred text measured three ways (a split line, a component, a String).
 */
public final class LabelTooltip implements ClientTooltipComponent {
  static final Identifier SLOT = Identifier.withDefaultNamespace("container/bundle/slot_background");
  static final Identifier BAR = Identifier.fromNamespaceAndPath("demo", "bar");
  static final Component TITLE = Component.translatable("demo.label.title");
  private static final int WRAP = 40;

  private final Label label;

  public LabelTooltip(Label label) {
    this.label = label;
  }

  private Component text() {
    return switch (label.kind()) {
      case 1 -> Component.translatable("demo.label.description");
      case 2 -> Component.literal("wrap supercalifragilistic words");
      case 3 -> Component.literal("one\n\ntwo\n");
      case 4 -> Component.literal("qqqqqqq qq");
      default -> Component.literal("");
    };
  }

  @Override
  public int getHeight(Font font) {
    return font.split(text(), WRAP).size() * 9 + 24;
  }

  @Override
  public int getWidth(Font font) {
    return Math.max(WRAP, font.width(TITLE));
  }

  @Override
  public void extractImage(
      Font font, int x, int y, int width, int height, GuiGraphicsExtractor graphics) {
    int bottom = graphics.textWithWordWrap(font, text(), x, y, WRAP, 0xFFAAAAAA);
    List<FormattedCharSequence> lines = font.split(text(), WRAP);
    int widest = 0;
    for (FormattedCharSequence line : lines) {
      widest = Math.max(widest, font.width(line));
    }
    graphics.blitSprite(RenderPipelines.GUI_TEXTURED, SLOT, x, bottom, 24, 24);
    graphics.blitSprite(RenderPipelines.GUI_TEXTURED, BAR, x + 1, bottom, 0, 13);
    graphics.blitSprite(RenderPipelines.GUI_TEXTURED, BAR, x + 24, bottom, widest, 13, 0x80FF0000);
    graphics.blitSprite(RenderPipelines.GUI_TEXTURED, BAR, x + 24, bottom + 13, 30, 11, 0.5f);
    if (!lines.isEmpty()) {
      graphics.centeredText(font, lines.get(0), x + 48, bottom + 2, -1);
    }
    graphics.centeredText(font, TITLE, x + width / 2, y + height - 9, 0xFFFFFF00);
    graphics.centeredText(font, "Iq" + lines.size(), x, y + height, -1);
  }
}
