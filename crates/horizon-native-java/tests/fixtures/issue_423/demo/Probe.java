package demo;
import java.nio.charset.StandardCharsets;
import java.util.function.Supplier;
public class Probe {
  static String row(Supplier<Object> f) { try { return String.valueOf(f.get()); } catch (Throwable t) { return t.getClass().getName()+":"+t.getMessage(); } }
  public static String rows() {
    String x="hello";
    String c=new String(x);
    char[] chars={'w','o','r','l','d'};
    String ca=new String(chars);
    String cas=new String(chars,1,3);
    String bs=new String(new byte[]{65,66,67}, StandardCharsets.UTF_8);
    return "copy="+c.length()+"/"+x.equals(c)+"/"+c.equals(x)+"/"+(c.hashCode()==x.hashCode())+"/"+(c==x)+"\n"
      +"chars="+ca+"/"+cas+"\n"+"bytes="+bs+"\n";
  }
  public static void main(String[] args) { System.out.print(rows()); }
}
