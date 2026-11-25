package demo;

import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.List;
import java.util.function.BiFunction;
import java.util.function.Function;

public class Probe {
  static final String HEX = "0123456789abcdef";

  static String hex(byte[] b) {
    if (b == null) {
      return "null";
    }
    StringBuilder s = new StringBuilder();
    for (int i = 0; i < b.length; i++) {
      s.append(HEX.charAt((b[i] >> 4) & 15)).append(HEX.charAt(b[i] & 15));
    }
    return s.toString();
  }

  /** Printable ASCII as is, every other char as a backslash-u escape (a backslash too). */
  static String show(String s) {
    StringBuilder out = new StringBuilder();
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      if (c >= 0x20 && c <= 0x7e && c != '\\') {
        out.append(c);
      } else {
        out.append("\\u").append(HEX.charAt((c >> 12) & 15)).append(HEX.charAt((c >> 8) & 15))
            .append(HEX.charAt((c >> 4) & 15)).append(HEX.charAt(c & 15));
      }
    }
    return out.toString();
  }

  static String oops(Throwable t) {
    return t.getClass().getName() + ": " + t.getMessage();
  }

  static byte[] pattern(int n) {
    byte[] b = new byte[n];
    for (int i = 0; i < n; i++) {
      b[i] = (byte) (i * 31 + 7 + (i >>> 8));
    }
    return b;
  }

  static byte[] allBytes() {
    byte[] b = new byte[256];
    for (int i = 0; i < 256; i++) {
      b[i] = (byte) i;
    }
    return b;
  }

  static boolean same(byte[] a, byte[] b) {
    if (a.length != b.length) {
      return false;
    }
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) {
        return false;
      }
    }
    return true;
  }

  static String bits(boolean b) {
    return b ? "1" : "0";
  }

  static final byte[] CRLF = {13, 10};

  // ---- identity -------------------------------------------------------------------------

  static Base64.Encoder[] encoders() {
    return new Base64.Encoder[] {
      Base64.getEncoder(),
      Base64.getUrlEncoder(),
      Base64.getMimeEncoder(),
      Base64.getEncoder().withoutPadding(),
      Base64.getUrlEncoder().withoutPadding(),
      Base64.getMimeEncoder().withoutPadding(),
      Base64.getMimeEncoder(76, CRLF),
      Base64.getMimeEncoder(4, CRLF),
      Base64.getMimeEncoder(3, CRLF),
      Base64.getMimeEncoder(0, CRLF),
      Base64.getMimeEncoder(-1, CRLF),
      Base64.getMimeEncoder(8, new byte[0]),
      Base64.getMimeEncoder(Integer.MIN_VALUE, new byte[0]),
    };
  }

  static final String[] ENCODER_NAMES = {
    "std", "url", "mime", "std-nopad", "url-nopad", "mime-nopad", "mime76", "mime4", "mime3", "mime0",
    "mime-1", "mime8-empty", "mimemin-empty"
  };

  static void identity(StringBuilder out) {
    Base64.Encoder[] a = encoders();
    Base64.Encoder[] b = encoders();
    for (int i = 0; i < a.length; i++) {
      StringBuilder same = new StringBuilder();
      StringBuilder eq = new StringBuilder();
      StringBuilder self = new StringBuilder();
      for (int j = 0; j < b.length; j++) {
        same.append(bits(a[i] == b[j]));
        eq.append(bits(a[i].equals(b[j])));
      }
      out.append("idm|").append(ENCODER_NAMES[i]).append("|==|").append(same).append("|equals|").append(eq)
          .append("|nopad-same|").append(bits(a[i].withoutPadding() == a[i]))
          .append("|nopad-twice|").append(bits(a[i].withoutPadding() == a[i].withoutPadding()))
          .append("|self|").append(bits(a[i] == a[i])).append(bits(a[i].equals(a[i]))).append('\n');
    }
    Base64.Decoder[] d = {Base64.getDecoder(), Base64.getUrlDecoder(), Base64.getMimeDecoder()};
    Base64.Decoder[] e = {Base64.getDecoder(), Base64.getUrlDecoder(), Base64.getMimeDecoder()};
    for (int i = 0; i < d.length; i++) {
      StringBuilder same = new StringBuilder();
      for (int j = 0; j < e.length; j++) {
        same.append(bits(d[i] == e[j])).append(bits(d[i].equals(e[j])));
      }
      out.append("idd|").append(i).append('|').append(same).append('\n');
    }
    Object o = Base64.getEncoder();
    Object p = Base64.getDecoder();
    out.append("types|").append(bits(o instanceof Base64.Encoder)).append(bits(p instanceof Base64.Decoder))
        .append(bits(o instanceof Object)).append(bits(o.equals(p))).append(bits(o.equals(null)))
        .append(bits(o.equals("x"))).append('\n');
    Base64.Encoder cast = (Base64.Encoder) o;
    out.append("cast|").append(bits(cast == o)).append('\n');
  }

  // ---- encoding -------------------------------------------------------------------------

  static void encoding(StringBuilder out) {
    Base64.Encoder[] e = encoders();
    int[] lengths = {
      0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 55, 56, 57, 58, 59,
      60, 61, 62, 63, 64, 70
    };
    for (int i = 0; i < 6; i++) {
      for (int n : lengths) {
        byte[] src = pattern(n);
        String text = e[i].encodeToString(src);
        String again = new String(e[i].encode(src), StandardCharsets.ISO_8859_1);
        out.append("enc|").append(ENCODER_NAMES[i]).append('|').append(n).append('|').append(show(text))
            .append('|').append(bits(text.equals(again))).append('\n');
      }
    }
    // Every byte value, so every 6-bit group lands in every position of a unit.
    byte[] all = allBytes();
    for (int i = 0; i < 6; i++) {
      for (int cut = 255; cut <= 256; cut++) {
        byte[] src = new byte[cut];
        System.arraycopy(all, 0, src, 0, cut);
        out.append("all|").append(ENCODER_NAMES[i]).append('|').append(cut).append('|')
            .append(show(e[i].encodeToString(src))).append('\n');
      }
    }
    // Lengths round the MIME line: 57 bytes is one full line, 58 starts the second.
    for (int i = 2; i < 6; i += 3) {
      for (int n = 112; n <= 118; n++) {
        out.append("mimeline|").append(ENCODER_NAMES[i]).append('|').append(n).append('|')
            .append(show(e[i].encodeToString(pattern(n)))).append('\n');
      }
    }
    // The input is not changed, and the output is a new array each time.
    byte[] src = pattern(10);
    byte[] before = src.clone();
    byte[] one = Base64.getEncoder().encode(src);
    byte[] two = Base64.getEncoder().encode(src);
    out.append("fresh|").append(bits(one != two)).append(bits(same(src, before)))
        .append(bits(one.length == 16)).append('\n');
    // A big input.
    byte[] big = pattern(3000);
    String bigText = Base64.getMimeEncoder().encodeToString(big);
    int lines = 1;
    for (int i = 0; i < bigText.length(); i++) {
      if (bigText.charAt(i) == '\n') {
        lines++;
      }
    }
    out.append("big|").append(bigText.length()).append('|').append(lines).append('|')
        .append(show(bigText.substring(0, 80))).append('|').append(show(bigText.substring(bigText.length() - 30)))
        .append('\n');
  }

  static void lines(StringBuilder out) {
    int[] lens = {-1, 3, 4, 7, 8, 76, Integer.MAX_VALUE, Integer.MIN_VALUE};
    byte[][] seps = {new byte[0], CRLF, {'|'}, {'-', '_', ' '}, {(byte) 0x80, (byte) 0xff}};
    String[] sepNames = {"empty", "crlf", "bar", "dash-us-space", "high"};
    int[] ns = {0, 1, 2, 10, 31, 60};
    for (int len : lens) {
      for (int s = 0; s < seps.length; s++) {
        for (int n : ns) {
          Base64.Encoder e = Base64.getMimeEncoder(len, seps[s]);
          byte[] src = pattern(n);
          out.append("line|").append(len).append('|').append(sepNames[s]).append('|').append(n).append('|')
              .append(show(e.encodeToString(src))).append('|')
              .append(show(e.withoutPadding().encodeToString(src))).append('\n');
        }
      }
    }
  }

  static void separators(StringBuilder out) {
    byte[][] seps = {
      {'A'}, {'a'}, {'Z'}, {'z'}, {'0'}, {'9'}, {'+'}, {'/'}, {'='}, {'-'}, {'_'}, {' '}, {'\t'}, {13, 10}, {0},
      {(byte) 0x80}, {(byte) 0xff}, {'<', 'z', '='}, {'<', '=', 'z'}, {'|', 10, 'A'}, {'$', '$'}, {'~'}
    };
    int[] lens = {0, 3, 4, 76};
    for (byte[] sep : seps) {
      for (int len : lens) {
        out.append("sep|").append(hex(sep)).append('|').append(len).append('|');
        try {
          Base64.Encoder e = Base64.getMimeEncoder(len, sep);
          out.append("ok|").append(bits(e == Base64.getEncoder())).append('|')
              .append(show(e.encodeToString(pattern(40)))).append('\n');
        } catch (Exception x) {
          out.append(oops(x)).append('\n');
        }
      }
    }
    for (int len : lens) {
      try {
        Base64.getMimeEncoder(len, null);
        out.append("sep-null|").append(len).append("|ok\n");
      } catch (Exception x) {
        out.append("sep-null|").append(len).append('|').append(oops(x)).append('\n');
      }
    }
    // The encoder keeps the caller's separator array, not a copy.
    byte[] sep = {'|'};
    Base64.Encoder e = Base64.getMimeEncoder(8, sep);
    Base64.Encoder np = e.withoutPadding();
    byte[] src = pattern(12);
    out.append("keep|before|").append(show(e.encodeToString(src))).append('|')
        .append(show(np.encodeToString(src))).append('\n');
    sep[0] = '#';
    out.append("keep|after|").append(show(e.encodeToString(src))).append('|').append(show(np.encodeToString(src)))
        .append('|').append(show(e.withoutPadding().encodeToString(src))).append('\n');
    sep[0] = '=';
    out.append("keep|later-illegal|").append(show(e.encodeToString(src))).append('\n');
    // A built-in MIME encoder's separator is nobody's to change.
    out.append("keep|mime|").append(show(Base64.getMimeEncoder().withoutPadding().encodeToString(pattern(58))))
        .append('\n');
  }

  static void destinations(StringBuilder out) {
    Base64.Encoder[] e = encoders();
    int[] ns = {0, 1, 2, 3, 4, 5, 57, 58};
    for (int i = 0; i < 7; i++) {
      Base64.Encoder enc = i == 6 ? Base64.getMimeEncoder(8, new byte[] {'<', '>'}) : e[i];
      String name = i == 6 ? "mime8" : ENCODER_NAMES[i];
      for (int n : ns) {
        byte[] src = pattern(n);
        int len = enc.encode(src).length;
        int[] caps = {0, len - 1, len, len + 3};
        for (int cap : caps) {
          if (cap < 0) {
            continue;
          }
          byte[] dst = new byte[cap];
          java.util.Arrays.fill(dst, (byte) '#');
          out.append("dst|").append(name).append('|').append(n).append('|').append(cap).append('|');
          try {
            int count = enc.encode(src, dst);
            out.append(count).append('|').append(hex(dst)).append('\n');
          } catch (Exception x) {
            out.append(oops(x)).append('|').append(hex(dst)).append('\n');
          }
        }
      }
    }
    byte[] dst = new byte[8];
    Object[][] nulls = {{null, dst}, {new byte[3], null}, {null, null}, {new byte[0], null}, {null, new byte[0]}};
    for (Object[] c : nulls) {
      out.append("dst-null|");
      try {
        out.append(Base64.getEncoder().encode((byte[]) c[0], (byte[]) c[1])).append('\n');
      } catch (Exception x) {
        out.append(oops(x)).append('\n');
      }
    }
  }

  // ---- decoding -------------------------------------------------------------------------

  static String decodeS(Base64.Decoder d, String s) {
    try {
      return hex(d.decode(s));
    } catch (Exception e) {
      return oops(e);
    }
  }

  static String decodeB(Base64.Decoder d, byte[] s) {
    try {
      return hex(d.decode(s));
    } catch (Exception e) {
      return oops(e);
    }
  }

  static void decoding(StringBuilder out) {
    Base64.Decoder[] ds = {Base64.getDecoder(), Base64.getUrlDecoder(), Base64.getMimeDecoder()};
    String[] named = {
      "", "A", "AA", "AAA", "AAAA", "AAAAA", "AAAAAA", "AAAAAAA", "AAAAAAAA", "A=", "A==", "A===", "AA=", "AA==",
      "AA===", "AAA=", "AAA==", "AAAA=", "AAAA==", "AAAA===", "AAAA====", "====", "=", "==", "===", "A=A", "AA=A",
      "AAA=A", "AA==AAAA", "AAAA=AAA", "AAAA==AA", "AAAAA=", "AAAAA==", "AAAAA===", "AAAAAA=", "AAAAAA==",
      "AAAAAAA=", "AAAAAAA==", "AB==", "AAB=", "AAAB", "QR==", "QQ==", "QUE=", "QUF=", "QUFB", "Q===", "=AAA",
      "A=AA", "AA=AA", "AAAA\n", "AAAA ", "AA AA", "A\nAAA", "AAA\r\nA", "AA==\n", "AA==  ", "AA==\nAA==", "AA=\n=",
      "AA=\r\n=", "AA= =", "AA==AA", "AA==A", "AA==AAA", "+/+/", "-_-_", "+/-_", "AA-_", "AA+/", "A-", "A_", "AA-",
      "A+==", "AA==A=", "AAA=AAA=", "AAAA\n\nAAAA", "!!!!", "AAA!", "AA!", "A!", "!", "!!", "!A", "A!A", "\n", "\n\n",
      " ", "\r\n", "\tAA==", "SGVsbG8=", "SGVsbG8", "SGVsbG8==", "SGVsbG8!", "SGVs bG8=", "SGVs\nbG8=", "SGVsbG8=\n",
      "SGVsbG8=\r\n", "  SGVsbG8=  ", "SGVs\r\nbG8=\r\n", "aGVsbG8gd29ybGQ=", "aGVsbG8gd29ybGQ", "////", "++++",
      "----", "____", "AAAA/AAA", "AA-_AA-_", "Zm9v", "Zm9vYg==", "Zm9vYmE=", "Zm9vYmFy", "Zg==", "Zm8=", "Zg", "Zm8",
      "Zm9vYmFy\r\nZm9vYmFy", "Zm9v\n\nYmFy\n", "-", "_", "+", "/", "=A", "==A", "A=\n", "A\n=", "AA\n=\n=",
      "AAAA\n=", "AAAA\n==", "AAAA\n====", "AA==\n====", "AAA=\n=", "AAA\n=\nA", "AéAA", "AAAé", "ÿAAA",
      "AAĀA", "AAA中", "�AAA", "AAAA😀", "AA😀AA", "A\u0080AA", "AAAA ",
      " AAAA", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
      "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\r\nAAAA",
    };
    for (String s : named) {
      out.append("dec|").append(show(s));
      for (Base64.Decoder d : ds) {
        out.append('|').append(show(decodeS(d, s)));
      }
      out.append('\n');
    }
    // Every string of up to three of these five characters, and of four of the first three.
    char[] alpha = {'A', '+', '=', '-', '!'};
    for (int len = 1; len <= 4; len++) {
      int symbols = len == 4 ? 3 : 5;
      int total = 1;
      for (int i = 0; i < len; i++) {
        total *= symbols;
      }
      for (int k = 0; k < total; k++) {
        char[] cs = new char[len];
        int t = k;
        for (int i = len - 1; i >= 0; i--) {
          cs[i] = alpha[t % symbols];
          t /= symbols;
        }
        String s = new String(cs);
        out.append("dec4|").append(s);
        for (Base64.Decoder d : ds) {
          out.append('|').append(decodeS(d, s));
        }
        out.append('\n');
      }
    }
    // Bytes: the same text as bytes, and bytes no string has.
    byte[][] raw = {
      {'A', 'A', 'A', 'A'}, {(byte) 0xff, 'A', 'A', 'A'}, {(byte) 0x80, 'A', 'A', 'A', 'A'}, {'A', (byte) 0xe9, 'A', 'A'},
      {(byte) 0xff}, {(byte) 0x80}, {0}, {0, 0, 0, 0}, {'A', 'A', 0, '=', '='}, {'A', 'A', '=', '=', (byte) 0xff},
      {'A', 'A', '=', '=', 'A'}, {'A', 'A', '=', (byte) 0xe9, '='}, {'A', 'A', '=', 0, '='}, {}, {'='},
      {'+', '/', '-', '_'}, {127, 'A', 'A', 'A'}, {'A', 'A', 'A', (byte) 0xa0},
    };
    for (byte[] b : raw) {
      out.append("decb|").append(hex(b));
      for (Base64.Decoder d : ds) {
        out.append('|').append(decodeB(d, b));
      }
      out.append('\n');
    }
    // Decoding does not change its input, and gives a new array.
    byte[] in = "SGVsbG8=".getBytes(StandardCharsets.ISO_8859_1);
    byte[] cp = in.clone();
    byte[] r1 = Base64.getDecoder().decode(in);
    byte[] r2 = Base64.getDecoder().decode(in);
    out.append("decb-fresh|").append(bits(r1 != r2)).append(bits(same(in, cp)))
        .append(bits(r1.length == 5)).append('\n');
  }

  static void roundTrips(StringBuilder out) {
    Base64.Encoder[] e = encoders();
    Base64.Decoder[] d = {Base64.getDecoder(), Base64.getUrlDecoder(), Base64.getMimeDecoder()};
    for (int n = 0; n <= 130; n += (n < 70 ? 1 : 13)) {
      byte[] src = pattern(n);
      StringBuilder row = new StringBuilder();
      // std through basic and mime, url through url, mime through mime, no padding through each.
      row.append(bits(same(src, d[0].decode(e[0].encodeToString(src)))));
      row.append(bits(same(src, d[2].decode(e[0].encodeToString(src)))));
      row.append(bits(same(src, d[1].decode(e[1].encodeToString(src)))));
      row.append(bits(same(src, d[2].decode(e[2].encodeToString(src)))));
      row.append(bits(same(src, d[0].decode(e[3].encodeToString(src)))));
      row.append(bits(same(src, d[1].decode(e[4].encodeToString(src)))));
      row.append(bits(same(src, d[2].decode(e[5].encodeToString(src)))));
      row.append(bits(same(src, d[2].decode(e[6].encodeToString(src)))));
      row.append(bits(same(src, d[2].decode(e[7].encodeToString(src)))));
      out.append("rt|").append(n).append('|').append(row).append('\n');
    }
    // The URL alphabet is not the standard one.
    byte[] src = {(byte) 0xfb, (byte) 0xff, (byte) 0xbf, (byte) 0xfb, (byte) 0xef};
    out.append("alphabets|").append(Base64.getEncoder().encodeToString(src)).append('|')
        .append(Base64.getUrlEncoder().encodeToString(src)).append('|')
        .append(Base64.getUrlEncoder().withoutPadding().encodeToString(src)).append('\n');
  }

  static void nulls(StringBuilder out) {
    Base64.Encoder[] e = encoders();
    for (int i = 0; i < 6; i++) {
      try {
        e[i].encode((byte[]) null);
        out.append("null|").append(ENCODER_NAMES[i]).append("|encode|ok\n");
      } catch (Exception x) {
        out.append("null|").append(ENCODER_NAMES[i]).append("|encode|").append(oops(x)).append('\n');
      }
      try {
        e[i].encodeToString(null);
        out.append("null|").append(ENCODER_NAMES[i]).append("|encodeToString|ok\n");
      } catch (Exception x) {
        out.append("null|").append(ENCODER_NAMES[i]).append("|encodeToString|").append(oops(x)).append('\n');
      }
    }
    Base64.Decoder[] d = {Base64.getDecoder(), Base64.getUrlDecoder(), Base64.getMimeDecoder()};
    for (int i = 0; i < 3; i++) {
      try {
        d[i].decode((byte[]) null);
        out.append("null|dec").append(i).append("|bytes|ok\n");
      } catch (Exception x) {
        out.append("null|dec").append(i).append("|bytes|").append(oops(x)).append('\n');
      }
      try {
        d[i].decode((String) null);
        out.append("null|dec").append(i).append("|string|ok\n");
      } catch (Exception x) {
        out.append("null|dec").append(i).append("|string|").append(oops(x)).append('\n');
      }
    }
  }

  // ---- how a mod calls it ---------------------------------------------------------------

  static void usage(StringBuilder out) {
    String text = "Hello, wörld 中";
    String enc = Base64.getEncoder().encodeToString(text.getBytes(StandardCharsets.UTF_8));
    out.append("use|round|").append(enc).append('|')
        .append(show(new String(Base64.getDecoder().decode(enc), StandardCharsets.UTF_8))).append('\n');
    Function<byte[], String> bound = Base64.getEncoder()::encodeToString;
    out.append("use|bound-encode|").append(bound.apply("Hello".getBytes(StandardCharsets.UTF_8))).append('\n');
    Function<String, byte[]> boundDecode = Base64.getDecoder()::decode;
    out.append("use|bound-decode|").append(hex(boundDecode.apply("SGVsbG8="))).append('\n');
    BiFunction<Base64.Encoder, byte[], String> unbound = Base64.Encoder::encodeToString;
    out.append("use|unbound-encode|").append(unbound.apply(Base64.getUrlEncoder(), new byte[] {-5, -1, -65})).append('\n');
    BiFunction<Base64.Decoder, String, byte[]> unboundDecode = Base64.Decoder::decode;
    out.append("use|unbound-decode|").append(hex(unboundDecode.apply(Base64.getUrlDecoder(), "-_-_"))).append('\n');
    List<String> decoded = List.of("SGk=", "SGVsbG8=", "d29ybGQ").stream().map(Base64.getDecoder()::decode)
        .map(b -> new String(b, StandardCharsets.UTF_8)).toList();
    out.append("use|stream|").append(decoded).append('\n');
    List<String> encoded = List.of("a", "bc", "def").stream().map(s -> s.getBytes(StandardCharsets.UTF_8))
        .map(Base64.getEncoder()::encodeToString).toList();
    out.append("use|stream-encode|").append(encoded).append('\n');
    java.util.Map<String, Base64.Encoder> byName = new java.util.HashMap<>();
    byName.put("std", Base64.getEncoder());
    byName.put("url", Base64.getUrlEncoder());
    out.append("use|map|").append(byName.get("url").encodeToString(new byte[] {-1, -2, -3, -4})).append('|')
        .append(bits(byName.get("std") == Base64.getEncoder())).append('\n');
    // A shared encoder in a static field and an instance's field.
    out.append("use|field|").append(SHARED.encodeToString(new byte[] {1, 2, 3})).append('|')
        .append(new Holder().encode(new byte[] {4, 5})).append('\n');
    // A mod that decodes a value it read: the error is a plain IllegalArgumentException.
    try {
      Base64.getDecoder().decode("not base64!");
      out.append("use|bad|ok\n");
    } catch (IllegalArgumentException e) {
      out.append("use|bad|").append(e.getMessage()).append('\n');
    }
    try {
      Base64.getDecoder().decode("not base64!");
    } catch (RuntimeException e) {
      out.append("use|bad-runtime|").append(e.getClass().getName()).append('\n');
    }
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < 1000; i++) {
      byte[] b = new byte[] {(byte) i, (byte) (i >> 8)};
      sb.append(Base64.getEncoder().withoutPadding().encodeToString(b).length());
    }
    out.append("use|many|").append(sb.length()).append('\n');
  }

  static final Base64.Encoder SHARED = Base64.getUrlEncoder().withoutPadding();

  static class Holder {
    final Base64.Encoder enc = Base64.getMimeEncoder(4, new byte[] {'.'});

    String encode(byte[] b) {
      return enc.encodeToString(b);
    }
  }

  public static String rows() {
    StringBuilder out = new StringBuilder();
    identity(out);
    encoding(out);
    lines(out);
    separators(out);
    destinations(out);
    decoding(out);
    roundTrips(out);
    nulls(out);
    usage(out);
    return out.toString();
  }

  public static void main(String[] args) {
    System.out.print(rows());
  }
}
