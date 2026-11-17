package demo;
public class Probe {
  static Object service() throws Exception { Class<?> c=Class.forName("net.fabricmc.loader.impl.launch.knot.MixinServiceKnot"); return c.getDeclaredConstructor().newInstance(); }
  static String call(String n) { try { Object s=service(); return String.valueOf(s.getClass().getMethod(n).invoke(s)); } catch (Throwable t) { return "COUNTED"; } }
  public static String rows() { return "name="+call("getName")+"\nside="+call("getSideName")+"\n"; }
  public static void main(String[] a) { System.out.print(rows()); }
  public static String provider() { return call("getBytecodeProvider"); }
}
