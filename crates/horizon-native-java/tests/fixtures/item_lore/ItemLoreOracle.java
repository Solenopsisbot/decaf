import com.google.gson.JsonElement;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.JsonOps;
import java.util.List;
import net.minecraft.ChatFormatting;
import net.minecraft.SharedConstants;
import net.minecraft.core.component.DataComponents;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.component.ItemLore;

/**
 * Captures Minecraft 26.3's real {@code DataComponents.LORE} codec ({@code ItemLore.CODEC}) +
 * {@link JsonOps#INSTANCE} output for the lores {@code fixture_item_lore.rs}'s demo mod builds, on
 * OpenJDK 25. Each line is {@code <case>=<JSON>}, the value the item declaration's
 * {@code components."minecraft:lore"} must hold.
 */
public final class ItemLoreOracle {
  private static String encode(ItemLore lore) {
    DataResult<JsonElement> result =
        DataComponents.LORE.codecOrThrow().encodeStart(JsonOps.INSTANCE, lore);
    return result
        .result()
        .map(JsonElement::toString)
        .orElseThrow(
            () ->
                new IllegalStateException(
                    result.error().map(DataResult.Error::message).orElse("unknown codec error")));
  }

  public static void main(String[] args) {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    Object[][] cases = {
      {
        "wand",
        new ItemLore(
            List.of(Component.translatable("item.fixture.wand.hint").withStyle(ChatFormatting.GRAY)))
      },
      {"rod", new ItemLore(List.of(Component.literal("First"), Component.literal("Second")))},
      {
        "staff",
        new ItemLore(
            List.of(
                Component.translatable("item.fixture.staff.uses", 3)
                    .withStyle(ChatFormatting.AQUA, ChatFormatting.BOLD),
                Component.literal("x").append(Component.literal("y").withStyle(ChatFormatting.RED))))
      },
      {"blank", new ItemLore(List.of())},
    };
    for (Object[] row : cases) {
      System.out.println("CASE:" + row[0] + "=" + encode((ItemLore) row[1]));
    }
    // A lore over the constructor's 256-line limit is refused there, before any item holds it.
    try {
      new ItemLore(java.util.Collections.nCopies(257, Component.literal("x")));
      System.out.println("CASE:over_limit=accepted");
    } catch (IllegalArgumentException error) {
      System.out.println("CASE:over_limit=" + error.getClass().getName() + ": " + error.getMessage());
    }
  }
}
