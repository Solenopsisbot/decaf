package demo;

import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;

/** The client side of a {@link Meter}: rows of four 18-pixel cells and a bar under them. */
public final class MeterTooltip implements ClientTooltipComponent {
  private final Meter meter;

  public MeterTooltip(Meter meter) {
    this.meter = meter;
  }

  private int rows() {
    return (meter.segments() + 3) / 4;
  }

  @Override
  public int getHeight(Font font) {
    return rows() * 18 + 4;
  }

  @Override
  public int getWidth(Font font) {
    return Math.min(meter.segments(), 4) * 18;
  }

  @Override
  public void extractImage(
      Font font, int x, int y, int width, int height, GuiGraphicsExtractor graphics) {
    for (int i = 0; i < meter.segments(); i++) {
      int left = x + (i % 4) * 18 + 1;
      int top = y + (i / 4) * 18 + 1;
      graphics.fill(left, top, left + 16, top + 16, 0xFF8B8B8B);
    }
    int bar = y + rows() * 18;
    graphics.fill(x, bar, x + Math.min(width, getWidth(font)), bar + 4, 0xFF20C020);
  }
}
