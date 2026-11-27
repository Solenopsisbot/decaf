import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.client.gui.Font;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.FormattedCharSequence;
import net.minecraft.util.Mth;

/**
 * Java 25 oracle for cases25.txt: 26.3's own Font.split and Font.width (FontOracle's Font, over
 * the same advance table) of the inputs client_font.rs's unit tests and fixture_client_font.rs's
 * `split_and_width_cases_match_java25` assert, and Mth.ceil of the edge floats `mth_ceil` takes.
 * A text is printed with Java escapes for `\n`; a split's lines are joined by `|`.
 */
public final class FontCasesOracle {
  static final String[][] SPLITS = {
    {"ab cd", "20"}, {"ab cd", "28"}, {"abcd", "13"}, {"a\nb", "100"}, {"a\n", "100"},
    {"a\n\n", "100"}, {"", "100"}, {"ab", "0"}, {"ab", "-5"}, {"qqqqqqq qq", "40"},
    {"  lead", "10"}, {"i i i i i i", "7"}, {"trailing   ", "30"}, {"one\n\ntwo\n", "40"},
    {"Can hold mixed stacks of runes", "96"}, {"Can hold mixed stacks of runes", "40"},
    {"wrap supercalifragilistic words", "40"}, {"\n", "5"}, {" ", "1"}, {"ab  cd", "12"},
  };
  static final String[] WIDTHS = {"a", "q", "qq", "qqq", "Iq6", "", "Pouch Label", "  ", "Full"};
  static final float[] CEILS = {38.5f, 39f, 3e9f, Float.POSITIVE_INFINITY, Float.NaN, -0.5f, -3e9f};

  static String escape(String text) {
    return text.replace("\\", "\\\\").replace("\n", "\\n");
  }

  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    Font font = FontOracle.font();
    for (String[] split : SPLITS) {
      List<FormattedCharSequence> lines =
          font.split(Component.literal(split[0]), Integer.parseInt(split[1]));
      StringBuilder joined = new StringBuilder();
      for (int i = 0; i < lines.size(); i++) {
        if (i > 0) joined.append('|');
        joined.append(escape(FontOracle.text(lines.get(i))));
      }
      System.out.println(
          "split \"" + escape(split[0]) + "\" " + split[1] + " " + lines.size() + " [" + joined + "]");
    }
    for (String text : WIDTHS) {
      System.out.println("width \"" + escape(text) + "\" " + font.width(text));
    }
    for (float value : CEILS) {
      System.out.println("ceil " + value + " " + Mth.ceil(value));
    }
  }
}
