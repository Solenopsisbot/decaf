package demo;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import net.minecraft.ChatFormatting;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.item.component.TooltipDisplay;

/**
 * Hover an item's tooltip many times, then build a fresh object that reads a field it never
 * wrote (#609, Runes 1.3.2 round 2 bug 1).
 *
 * <p>{@link HintItem} has the shape of Runes' altar item ({@code RuneCraftingBlock$1}): one
 * translatable line styled through {@code withStyle(ChatFormatting...)}, so every hover allocates
 * one varargs array and no object. {@link Screen} has the shape of the altar's screen: its
 * {@code titleLabelX} is read before this class ever writes it ({@code +=}), as
 * {@code RuneCraftingScreen#<init>} reads {@code AbstractContainerScreen#titleLabelX}.
 *
 * <p>{@code main} is the Java 25 oracle: {@code java -cp . demo.Probe} with the stubs beside it.
 */
public final class Probe {
  /** Hovers: past the resident heap's first collection trigger (1,024 new entries). */
  public static final int HOVERS = 1500;

  static int hovers;
  /** The registered item, as a mod's item registry holds it. */
  static Item item;

  /** Runes' {@code RuneCraftingBlock$1.appendHoverText}, offsets 0-42. */
  public static final class HintItem extends Item {
    public HintItem(Item.Properties properties) {
      super(properties);
    }

    @Override
    public void appendHoverText(
        ItemStack stack,
        Item.TooltipContext context,
        TooltipDisplay display,
        Consumer<Component> out,
        TooltipFlag flag) {
      super.appendHoverText(stack, context, display, out, flag);
      hovers++;
      out.accept(
          Component.translatable("demo.hint").withStyle(ChatFormatting.GRAY, ChatFormatting.ITALIC));
    }
  }

  /** A screen's label position: never written before the first read. */
  static final class Screen {
    int titleLabelX;
    int imageWidth = 176;
  }

  public static Item item() {
    item = new HintItem(new Item.Properties());
    return item;
  }

  public static String hovered() {
    return Integer.toString(hovers);
  }

  /** The screen open after the hovers: {@code 60|176} on Java 25. */
  public static String open() {
    Screen screen = new Screen();
    screen.titleLabelX += 60;
    return screen.titleLabelX + "|" + screen.imageWidth;
  }

  public static void main(String[] args) {
    Item hovered = item();
    List<Component> lines = new ArrayList<>();
    for (int i = 0; i < HOVERS; i++) {
      lines.clear();
      hovered.appendHoverText(null, null, null, lines::add, null);
    }
    System.out.println(hovered() + "|" + lines.size() + "|" + open());
  }
}
