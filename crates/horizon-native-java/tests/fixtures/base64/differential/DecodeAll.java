import java.io.BufferedOutputStream;
import java.io.PrintStream;
import java.util.Base64;

/**
 * Every byte string up to a length over an alphabet, through the basic, URL and MIME decoders.
 * One line per (string, decoder): {@code <input hex>|<0 basic, 1 url, 2 mime>|OK:<output hex>}
 * or {@code E:<message>}.
 *
 * <pre>java DecodeAll &lt;max length&gt; &lt;alphabet as hex bytes&gt;    e.g. java DecodeAll 6 4151512b2d5f3d210aff80</pre>
 */
public class DecodeAll {
  static String hex(byte[] b) {
    StringBuilder s = new StringBuilder();
    for (byte x : b) {
      s.append(String.format("%02x", x & 0xff));
    }
    return s.toString();
  }

  public static void main(String[] a) throws Exception {
    int maxLen = Integer.parseInt(a[0]);
    int[] alphabet = new int[a[1].length() / 2];
    for (int i = 0; i < alphabet.length; i++) {
      alphabet[i] = Integer.parseInt(a[1].substring(2 * i, 2 * i + 2), 16);
    }
    Base64.Decoder[] decoders = {Base64.getDecoder(), Base64.getUrlDecoder(), Base64.getMimeDecoder()};
    PrintStream out = new PrintStream(new BufferedOutputStream(System.out, 1 << 20), false, "UTF-8");
    for (int length = 0; length <= maxLen; length++) {
      long total = 1;
      for (int i = 0; i < length; i++) {
        total *= alphabet.length;
      }
      for (long k = 0; k < total; k++) {
        byte[] input = new byte[length];
        long rest = k;
        for (int i = length - 1; i >= 0; i--) {
          input[i] = (byte) alphabet[(int) (rest % alphabet.length)];
          rest /= alphabet.length;
        }
        for (int d = 0; d < 3; d++) {
          String result;
          try {
            result = "OK:" + hex(decoders[d].decode(input));
          } catch (IllegalArgumentException e) {
            result = "E:" + e.getMessage();
          } catch (Throwable e) {
            result = "X:" + e.getClass().getName() + ":" + e.getMessage();
          }
          out.println(hex(input) + "|" + d + "|" + result);
        }
      }
    }
    out.flush();
  }
}
