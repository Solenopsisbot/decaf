package demo;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
public final class Probe {
  private static final String[] INPUTS = {"a b", "*-._", "~!'()", "é", "😀", "", "a+b=c&d"};
  public static String rows() {
    String[] out = new String[INPUTS.length];
    for (int i = 0; i < INPUTS.length; i++) out[i] = URLEncoder.encode(INPUTS[i], StandardCharsets.UTF_8);
    return String.join("|", out);
  }
  public static String loneSurrogate() { return URLEncoder.encode("\uD800", StandardCharsets.UTF_8); }
  public static String replacementChar() { return URLEncoder.encode("�", StandardCharsets.UTF_8); }
  public static String nullString() { try { return URLEncoder.encode(null, StandardCharsets.UTF_8); } catch (RuntimeException e) { return e.getClass().getName() + ":" + e.getMessage(); } }
  public static String rowsOtherCharset() { return URLEncoder.encode("a", StandardCharsets.UTF_16); }
  public static void main(String[] args) { System.out.print(rows()); }
}
