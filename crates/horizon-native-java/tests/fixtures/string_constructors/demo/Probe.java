package demo;
import java.nio.charset.StandardCharsets;
public final class Probe {
  public static String rows() throws Exception {
    String x="hello"; String c=new String(x);
    return c.length()+"|"+x.equals(c)+"|"+c.equals(x)+"|"+(x.hashCode()==c.hashCode())+"|"+(c!=x)+"|"+(c.intern()==x)
      +"|"+new String(new char[]{'a','b'})+"|"+new String(new char[]{'a','b','c'},1,2)
      +"|"+new String("é".getBytes(StandardCharsets.UTF_8),StandardCharsets.UTF_8)
      +"|"+new String("é".getBytes(StandardCharsets.ISO_8859_1),"ISO-8859-1")
      +"|"+new String(new StringBuilder("sb"))+"|"+new String();
  }
}
