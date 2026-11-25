package demo;

/* Probes for String#toUpperCase/toLowerCase (with and without a Locale) and
 * Character#toUpperCase/toLowerCase (char and int).
 *
 * Every row is printed as `<label>=<value>` where `<value>` is the result's
 * length in UTF-16 units followed by each unit as uppercase hexadecimal
 * digits, e.g. `"ß".toUpperCase()` is `l1SharpUpper=2 53 53`.
 *
 * The same source is compiled by `javac --release 21` and run on Java 25 to
 * produce java25.txt (the oracle); the runtime then answers the same static
 * methods from the compiled probe and each answer must match the oracle line
 * of the same label, with nothing counted. Rows the runtime does not model
 * are counted instead and are never compared against the oracle.
 */
public final class Probe {
  private Probe() {}

  static final char[] HEX = "0123456789ABCDEF".toCharArray();

  static String hex4(int unit) {
    return ""
        + HEX[(unit >> 12) & 0xF]
        + HEX[(unit >> 8) & 0xF]
        + HEX[(unit >> 4) & 0xF]
        + HEX[unit & 0xF];
  }

  static String hex(int value) {
    if (value == 0) {
      return "0";
    }
    StringBuilder out = new StringBuilder();
    while (value != 0) {
      out.insert(0, HEX[value & 0xF]);
      value >>>= 4;
    }
    return out.toString();
  }

  /** Length in UTF-16 units, then each unit as four uppercase hex digits. */
  static String row(String text) {
    StringBuilder out = new StringBuilder();
    out.append(text.length());
    for (int i = 0; i < text.length(); i++) {
      out.append(' ');
      out.append(hex4(text.charAt(i)));
    }
    return out.toString();
  }

  /** One code point as a length-1 row. */
  static String rowC(int codePoint) {
    return "1 " + hex(codePoint);
  }

  /* ================= String#toUpperCase() / toLowerCase() ================= */

  public static String emptyUpper() {
    return row("".toUpperCase());
  }

  public static String emptyLower() {
    return row("".toLowerCase());
  }

  public static String asciiLowerUpper() {
    return row("abc".toUpperCase());
  }

  public static String asciiLowerLower() {
    return row("abc".toLowerCase());
  }

  public static String asciiUpperUpper() {
    return row("ABC".toUpperCase());
  }

  public static String asciiUpperLower() {
    return row("ABC".toLowerCase());
  }

  /* The default locale decides `I`/`i`: ASCII with neither letter is answered, any other is counted. */

  public static String asciiCapIUpper() {
    return row("ID".toUpperCase());
  }

  public static String asciiSmallILower() {
    return row("id".toLowerCase());
  }

  public static String asciiNoIUpper() {
    return row("Hello, World_42!".toUpperCase());
  }

  public static String asciiNoIILower() {
    return row("Hello, World_42!".toLowerCase());
  }

  public static String asciiCapILower() {
    return row("I".toLowerCase());
  }

  public static String asciiSmallIUpper() {
    return row("i".toUpperCase());
  }

  public static String asciiTitleLower() {
    return row("TITLE".toLowerCase());
  }

  public static String asciiMiniUpper() {
    return row("Mini".toUpperCase());
  }

  public static String l1EUpper() {
    return row("\u00E9".toUpperCase());
  }

  public static String l1ELower() {
    return row("\u00E9".toLowerCase());
  }

  public static String l1YenUpper() {
    return row("\u00FF".toUpperCase());
  }

  public static String l1YenLower() {
    return row("\u00FF".toLowerCase());
  }

  public static String l1SharpUpper() {
    return row("\u00DF".toUpperCase());
  }

  public static String l1SharpLower() {
    return row("\u00DF".toLowerCase());
  }

  public static String l1MicroUpper() {
    return row("\u00B5".toUpperCase());
  }

  public static String l1MicroLower() {
    return row("\u00B5".toLowerCase());
  }

  public static String leAMacronUpper() {
    return row("\u0100".toUpperCase());
  }

  public static String leAMacronLower() {
    return row("\u0100".toLowerCase());
  }

  public static String greekSigmaLower() {
    return row("A\u03A3".toLowerCase());
  }

  public static String greekSigmaUpper() {
    return row("a\u03C3".toUpperCase());
  }

  public static String turkishIDotUpper() {
    return row("\u0130".toUpperCase());
  }

  public static String turkishIDotLower() {
    return row("\u0130".toLowerCase());
  }

  public static String turkishIDotlessUpper() {
    return row("\u0131".toUpperCase());
  }

  public static String turkishIDotlessLower() {
    return row("\u0131".toLowerCase());
  }

  public static String kelvinUpper() {
    return row("\u212A".toUpperCase());
  }

  public static String kelvinLower() {
    return row("\u212A".toLowerCase());
  }

  public static String romanUpper() {
    return row("\u2160".toUpperCase());
  }

  public static String romanLower() {
    return row("\u2160".toLowerCase());
  }

  public static String deseretUpper() {
    return row("\uD800\uDC00".toUpperCase());
  }

  public static String deseretLower() {
    return row("\uD800\uDC00".toLowerCase());
  }

  public static String loneLowSurrogateUpper() {
    return row("\uDC00".toUpperCase());
  }

  public static String loneLowSurrogateLower() {
    return row("\uDC00".toLowerCase());
  }

  public static String loneHighSurrogateUpper() {
    return row("\uD800".toUpperCase());
  }

  public static String loneHighSurrogateLower() {
    return row("\uD800".toLowerCase());
  }

  /* ================= String#toUpperCase(Locale) / toLowerCase(Locale) ================= */

  public static String asciiRootLower() {
    return row("ABC".toLowerCase(java.util.Locale.ROOT));
  }

  public static String asciiRootUpper() {
    return row("abc".toUpperCase(java.util.Locale.ROOT));
  }

  public static String asciiTrLower() {
    return row("ABC".toLowerCase(java.util.Locale.forLanguageTag("tr")));
  }

  public static String asciiTrUpper() {
    return row("abc".toUpperCase(java.util.Locale.forLanguageTag("tr")));
  }

  public static String asciiCapIRootLower() {
    return row("I".toLowerCase(java.util.Locale.ROOT));
  }

  public static String asciiSmallIRootUpper() {
    return row("i".toUpperCase(java.util.Locale.ROOT));
  }

  public static String asciiCapITrLower() {
    return row("I".toLowerCase(java.util.Locale.forLanguageTag("tr")));
  }

  public static String asciiSmallITrUpper() {
    return row("i".toUpperCase(java.util.Locale.forLanguageTag("tr")));
  }

  public static String idotRootLower() {
    return row("\u0130".toLowerCase(java.util.Locale.ROOT));
  }

  public static String idotTrLower() {
    return row("\u0130".toLowerCase(java.util.Locale.forLanguageTag("tr")));
  }

  public static String idotlessRootUpper() {
    return row("\u0131".toUpperCase(java.util.Locale.ROOT));
  }

  public static String idotlessTrUpper() {
    return row("\u0131".toUpperCase(java.util.Locale.forLanguageTag("tr")));
  }

  public static String sharpRootUpper() {
    return row("\u00DF".toUpperCase(java.util.Locale.ROOT));
  }

  /* A null Locale: Java throws NullPointerException; the runtime must count the call. */

  public static String asciiNullLocaleUpper() {
    return row("abc".toUpperCase((java.util.Locale) null));
  }

  public static String asciiNullLocaleLower() {
    return row("ABC".toLowerCase((java.util.Locale) null));
  }

  /* ================= Character#toUpperCase / toLowerCase (char and int) ================= */

  public static String charUpperSharp() {
    return rowC(Character.toUpperCase('\u00DF'));
  }

  public static String charLowerSharp() {
    return rowC(Character.toLowerCase('\u00DF'));
  }

  public static String charUpperE() {
    return rowC(Character.toUpperCase('\u00E9'));
  }

  public static String charLowerE() {
    return rowC(Character.toLowerCase('\u00E9'));
  }

  public static String charUpperAMacron() {
    return rowC(Character.toUpperCase('\u0100'));
  }

  public static String charLowerAMacron() {
    return rowC(Character.toLowerCase('\u0100'));
  }

  public static String charLowerSigma() {
    return rowC(Character.toLowerCase('\u03A3'));
  }

  public static String charUpperSigma() {
    return rowC(Character.toUpperCase('\u03C3'));
  }

  public static String charUpperIDotless() {
    return rowC(Character.toUpperCase('\u0131'));
  }

  public static String charLowerIDot() {
    return rowC(Character.toLowerCase('\u0130'));
  }

  public static String charUpperIDot() {
    return rowC(Character.toUpperCase('\u0130'));
  }

  public static String charLowerIDotless() {
    return rowC(Character.toLowerCase('\u0131'));
  }

  public static String charLowerKelvin() {
    return rowC(Character.toLowerCase('\u212A'));
  }

  public static String charUpperRoman() {
    return rowC(Character.toUpperCase('\u2160'));
  }

  public static String charLowerRoman() {
    return rowC(Character.toLowerCase('\u2160'));
  }

  public static String intLowerKelvin() {
    return rowC(Character.toLowerCase(0x212A));
  }

  public static String intUpperDeseret() {
    return rowC(Character.toUpperCase(0x10400));
  }

  public static String intLowerDeseret() {
    return rowC(Character.toLowerCase(0x10400));
  }

  public static String intUpperKelvin() {
    return rowC(Character.toUpperCase(0x212A));
  }

  public static String intLowerRoman() {
    return rowC(Character.toLowerCase(0x2160));
  }

  public static String intUpperRoman() {
    return rowC(Character.toUpperCase(0x2160));
  }

  public static void main(String[] ignored) {
    System.out.println("emptyUpper=" + emptyUpper());
    System.out.println("emptyLower=" + emptyLower());
    System.out.println("asciiLowerUpper=" + asciiLowerUpper());
    System.out.println("asciiLowerLower=" + asciiLowerLower());
    System.out.println("asciiUpperUpper=" + asciiUpperUpper());
    System.out.println("asciiUpperLower=" + asciiUpperLower());
    System.out.println("asciiCapIUpper=" + asciiCapIUpper());
    System.out.println("asciiSmallILower=" + asciiSmallILower());
    System.out.println("asciiNoIUpper=" + asciiNoIUpper());
    System.out.println("asciiNoIILower=" + asciiNoIILower());
    System.out.println("asciiCapILower=" + asciiCapILower());
    System.out.println("asciiSmallIUpper=" + asciiSmallIUpper());
    System.out.println("asciiTitleLower=" + asciiTitleLower());
    System.out.println("asciiMiniUpper=" + asciiMiniUpper());
    System.out.println("l1EUpper=" + l1EUpper());
    System.out.println("l1ELower=" + l1ELower());
    System.out.println("l1YenUpper=" + l1YenUpper());
    System.out.println("l1YenLower=" + l1YenLower());
    System.out.println("l1SharpUpper=" + l1SharpUpper());
    System.out.println("l1SharpLower=" + l1SharpLower());
    System.out.println("l1MicroUpper=" + l1MicroUpper());
    System.out.println("l1MicroLower=" + l1MicroLower());
    System.out.println("leAMacronUpper=" + leAMacronUpper());
    System.out.println("leAMacronLower=" + leAMacronLower());
    System.out.println("greekSigmaLower=" + greekSigmaLower());
    System.out.println("greekSigmaUpper=" + greekSigmaUpper());
    System.out.println("turkishIDotUpper=" + turkishIDotUpper());
    System.out.println("turkishIDotLower=" + turkishIDotLower());
    System.out.println("turkishIDotlessUpper=" + turkishIDotlessUpper());
    System.out.println("turkishIDotlessLower=" + turkishIDotlessLower());
    System.out.println("kelvinUpper=" + kelvinUpper());
    System.out.println("kelvinLower=" + kelvinLower());
    System.out.println("romanUpper=" + romanUpper());
    System.out.println("romanLower=" + romanLower());
    System.out.println("deseretUpper=" + deseretUpper());
    System.out.println("deseretLower=" + deseretLower());
    System.out.println("loneLowSurrogateUpper=" + loneLowSurrogateUpper());
    System.out.println("loneLowSurrogateLower=" + loneLowSurrogateLower());
    System.out.println("loneHighSurrogateUpper=" + loneHighSurrogateUpper());
    System.out.println("loneHighSurrogateLower=" + loneHighSurrogateLower());
    System.out.println("asciiRootLower=" + asciiRootLower());
    System.out.println("asciiRootUpper=" + asciiRootUpper());
    System.out.println("asciiTrLower=" + asciiTrLower());
    System.out.println("asciiTrUpper=" + asciiTrUpper());
    System.out.println("asciiCapIRootLower=" + asciiCapIRootLower());
    System.out.println("asciiSmallIRootUpper=" + asciiSmallIRootUpper());
    System.out.println("asciiCapITrLower=" + asciiCapITrLower());
    System.out.println("asciiSmallITrUpper=" + asciiSmallITrUpper());
    System.out.println("idotRootLower=" + idotRootLower());
    System.out.println("idotTrLower=" + idotTrLower());
    System.out.println("idotlessRootUpper=" + idotlessRootUpper());
    System.out.println("idotlessTrUpper=" + idotlessTrUpper());
    System.out.println("sharpRootUpper=" + sharpRootUpper());
    try {
      System.out.println("asciiNullLocaleUpper=" + asciiNullLocaleUpper());
    } catch (NullPointerException e) {
      System.out.println("asciiNullLocaleUpper=NullPointerException");
    }
    try {
      System.out.println("asciiNullLocaleLower=" + asciiNullLocaleLower());
    } catch (NullPointerException e) {
      System.out.println("asciiNullLocaleLower=NullPointerException");
    }
    System.out.println("charUpperSharp=" + charUpperSharp());
    System.out.println("charLowerSharp=" + charLowerSharp());
    System.out.println("charUpperE=" + charUpperE());
    System.out.println("charLowerE=" + charLowerE());
    System.out.println("charUpperAMacron=" + charUpperAMacron());
    System.out.println("charLowerAMacron=" + charLowerAMacron());
    System.out.println("charLowerSigma=" + charLowerSigma());
    System.out.println("charUpperSigma=" + charUpperSigma());
    System.out.println("charUpperIDotless=" + charUpperIDotless());
    System.out.println("charLowerIDot=" + charLowerIDot());
    System.out.println("charUpperIDot=" + charUpperIDot());
    System.out.println("charLowerIDotless=" + charLowerIDotless());
    System.out.println("charLowerKelvin=" + charLowerKelvin());
    System.out.println("charUpperRoman=" + charUpperRoman());
    System.out.println("charLowerRoman=" + charLowerRoman());
    System.out.println("intLowerKelvin=" + intLowerKelvin());
    System.out.println("intUpperDeseret=" + intUpperDeseret());
    System.out.println("intLowerDeseret=" + intLowerDeseret());
    System.out.println("intUpperKelvin=" + intUpperKelvin());
    System.out.println("intLowerRoman=" + intLowerRoman());
    System.out.println("intUpperRoman=" + intUpperRoman());
  }
}
