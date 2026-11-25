import java.io.BufferedOutputStream;
import java.io.PrintStream;
import java.util.Base64;

/**
 * Pseudo-random inputs of 0 to 260 bytes through the basic and URL encoders with and without
 * padding, and through MIME encoders of many line lengths and separators (also without padding).
 * One line per case:
 * {@code u<url>|p<padding>|m<line length>|<separator hex or ->|<input hex>|<output hex>|<encodeToString length>}.
 *
 * <pre>java EncodeAll</pre>
 */
public class EncodeAll {
  static String hex(byte[] b) {
    StringBuilder s = new StringBuilder();
    for (byte x : b) {
      s.append(String.format("%02x", x & 0xff));
    }
    return s.toString();
  }

  public static void main(String[] a) throws Exception {
    PrintStream out = new PrintStream(new BufferedOutputStream(System.out, 1 << 20), false, "UTF-8");
    long seed = 12345;
    int[] lineLengths = {-3, 0, 1, 3, 4, 5, 7, 8, 12, 19, 20, 64, 75, 76, 77, 80, 1000};
    byte[][] separators = {{}, {13, 10}, {10}, {'|'}, {'-', '_'}, {(byte) 0x80, (byte) 0xff}, {' ', '\t', '~'}};
    for (int length = 0; length <= 260; length++) {
      for (int repeat = 0; repeat < 3; repeat++) {
        byte[] src = new byte[length];
        for (int i = 0; i < length; i++) {
          seed = seed * 6364136223846793005L + 1442695040888963407L;
          src[i] = (byte) (seed >>> 33);
        }
        for (int url = 0; url < 2; url++) {
          for (int padding = 0; padding < 2; padding++) {
            Base64.Encoder e = url == 0 ? Base64.getEncoder() : Base64.getUrlEncoder();
            if (padding == 0) {
              e = e.withoutPadding();
            }
            out.println("u" + url + "|p" + padding + "|m0|-|" + hex(src) + "|" + hex(e.encode(src)) + "|"
                + e.encodeToString(src).length());
          }
        }
        for (int lineLength : lineLengths) {
          for (byte[] separator : separators) {
            for (int padding = 0; padding < 2; padding++) {
              Base64.Encoder e = Base64.getMimeEncoder(lineLength, separator);
              if (padding == 0) {
                e = e.withoutPadding();
              }
              out.println("u0|p" + padding + "|m" + lineLength + "|" + (separator.length == 0 ? "-" : hex(separator))
                  + "|" + hex(src) + "|" + hex(e.encode(src)) + "|" + e.encodeToString(src).length());
            }
          }
        }
        Base64.Encoder e = Base64.getMimeEncoder();
        out.println("u0|p1|mmime|-|" + hex(src) + "|" + hex(e.encode(src)) + "|0");
        e = e.withoutPadding();
        out.println("u0|p0|mmime|-|" + hex(src) + "|" + hex(e.encode(src)) + "|0");
      }
    }
    out.flush();
  }
}
