public class T {
  public static void main(String[] a) {
    try { new Thread((String) null); System.out.print("ok"); }
    catch (Throwable t) { System.out.print(t.getClass().getName() + ":" + t.getMessage()); }
  }
}
