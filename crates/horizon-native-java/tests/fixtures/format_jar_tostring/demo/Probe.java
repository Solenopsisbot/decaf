package demo;
import java.util.*;
public class Probe {
  public static final class Named { static int calls; static String order=""; final String text; Named(String t){text=t;} public String toString(){ calls++; order += (order.isEmpty()?"":",")+text; return text; } }
  public static final class Nully { public String toString(){ return null; } }
  public static final class Boom { static int calls; public String toString(){ calls++; throw new IllegalStateException("boom"); } }
  public static final class Fmt implements Formattable { public void formatTo(Formatter f,int flags,int width,int precision){ f.format("fmt"); } }
  public static String rows() {
    Named n = new Named("named");
    return String.join("\n",
      "[".concat("%s".formatted(n)).concat("]"),
      String.format("%-8s|%.3s|%S", n, n, n),
      "%s %s".formatted("host", n),
      "[%s]".formatted((Object)null),
      "[%s]".formatted(new Nully()),
      lazyUnused(), repeated(), explicitOrder(), previous());
  }
  static String lazyUnused(){ Boom.calls=0; String s=String.format("%s", "ok", new Boom()); return s+"|"+Boom.calls; }
  static String lazyThrow(){ try { String.format(Locale.ROOT, "%s %d", new Boom(), 3); return "ok"; } catch(Throwable t){ return t.getClass().getName()+":"+t.getMessage(); } }
  static String repeated(){ Named.calls=0; String.format("%1$s %1$s", new Named("count")); return "count|"+Named.calls; }
  static String explicitOrder(){ Named.calls=0; Named.order=""; return String.format("%2$s %1$s", new Named("one"), new Named("two"))+"|"+Named.order; }
  static String previous(){ Named.calls=0; String.format("%s %<s", new Named("prev")); return "prev|"+Named.calls; }
  static String countedNonString(){ try { return String.format("%d", new Named("counted")); } catch(Throwable t){ return t.getClass().getName()+":"+t.getMessage(); } }
  public static String lazyThrowLocale() { try { String.format(Locale.ROOT, "%s %d", new Boom(), 3); return "ok"; } catch(Throwable t){ return t.getClass().getName()+":"+t.getMessage(); } }
  public static String lazyThrowDefaultLocale() { try { String.format("%s %d", new Boom(), 3); return "ok"; } catch(Throwable t){ return t.getClass().getName()+":"+t.getMessage(); } }
  public static String boom() { return lazyThrowDefaultLocale(); }
  public static String fmt() { try { return "[%s]".formatted(new Fmt()); } catch (Throwable t) { return t.getClass().getName()+":"+t.getMessage(); } }
  public static String mixed() { return countedNonString(); }
  public static void main(String[] a) { if (a.length > 0) System.out.print(fmt()+"\n"+mixed()); else System.out.print(rows()); }
}
