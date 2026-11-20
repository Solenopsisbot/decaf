package demo;
import java.nio.charset.StandardCharsets;
import java.util.function.Supplier;
public class Probe {
  static String row(Supplier<Object> f) { try { return String.valueOf(f.get()); } catch (Throwable t) { return t.getClass().getName()+":"+t.getMessage(); } }
  static String switchCopy(String value) { return switch (value) { case "hello" -> "hit"; default -> "miss"; }; }
  public static String rows() {
    String x="hello";
    String c=new String(x);
    char[] chars={'w','o','r','l','d'};
    String ca=new String(chars);
    String cas=new String(chars,1,3);
    String bs=new String(new byte[]{65,66,67}, StandardCharsets.UTF_8);
    StringBuilder builder = new StringBuilder("ab");
    String builderCopy = new String(builder);
    builder.append("cd");
    return "copy="+c.length()+"/"+x.equals(c)+"/"+c.equals(x)+"/"+(c.hashCode()==x.hashCode())+"/"+(c==x)+"\n"
      +"methods="+(c.toString()==c)+"/"+c.charAt(1)+"/"+c.substring(1,4)+"/"+c.concat("!")+"/"+c.compareTo("hello")+"/"+(c.intern()=="hello")+"\n"
      +"switch="+switchCopy(c)+"\n"
      +"null="+row(() -> new String((String)null))+"\n"
      +"builder="+builderCopy+"/"+builder+"\n"
      +"chars="+ca+"/"+cas+"\n"+"bytes="+bs+"\n";
  }
  public static void main(String[] args) { System.out.print(rows()); }
}
