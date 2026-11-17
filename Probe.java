import java.net.*;
public class Probe {
  public static void main(String[] a) {
    String[] xs={"https://a.b/c?d#e","file:///tmp/x","jar:file:/x.jar!/y","relative","foo:bar","mailto:a@b"};
    for(String s:xs) try { System.out.println(s+" => "+new URI(s).toURL()); }
    catch(Exception e){ System.out.println(s+" => "+e.getClass().getName()+":"+e.getMessage()); }
  }
}
