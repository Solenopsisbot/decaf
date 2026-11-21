package demo;
import java.nio.charset.StandardCharsets;
import java.util.function.Supplier;
public class Probe {
  static String row(Supplier<Object> f) { try { return String.valueOf(f.get()); } catch (Throwable t) { return t.getClass().getName()+":"+t.getMessage(); } }
  interface Throwing { Object get() throws Throwable; }
  static String rowThrow(Throwing f) { try { return String.valueOf(f.get()); } catch (Throwable t) { return t.getClass().getName()+":"+t.getMessage(); } }
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
      +"chars="+ca+"/"+cas+"\n"+"bytes="+bs+"\n"
      +"common="+commonMethods()+"\n"+"ints="+integrations()+"\n"+"nulls="+nulls()+"\n";
  }
  public static String commonMethods() {
    String c = new String("hello");
    StringBuilder b = new StringBuilder("hello");
    return c.length()+"/"+c.isEmpty()+"/"+c.charAt(1)+"/"+c.codePointAt(1)+"/"+c.compareTo("hello")
      +"/"+c.contains("ell")+"/"+c.startsWith("he")+"/"+c.endsWith("lo")+"/"+c.indexOf("l")+"/"+c.lastIndexOf("l")
      +"/"+c.substring(1,4)+"/"+c.subSequence(1,4)+"/"+c.concat("!")+"/"+c.replace('l','r')
      +"/"+c.toLowerCase()+"/"+c.toUpperCase()+"/"+c.trim()+"/"+c.strip()+"/"+c.repeat(2)
      +"/"+c.equalsIgnoreCase("HELLO")+"/"+c.contentEquals(b);
  }
  public static String integrations() {
    String c = new String("hello");
    StringBuilder b = new StringBuilder().append(c);
    java.util.HashMap<String,Integer> m = new java.util.HashMap<>(); m.put("hello", 7);
    return b.toString()+"/"+String.join(",", c, "x")+"/"+String.format("%s:%d", c, 3)+"/"+switchCopy(c)+"/"+m.get(c);
  }
  public static String nulls() {
    char[] chars = null; byte[] bytes = null;
    return row(() -> new String((String)null))+"|"+row(() -> new String(chars))+"|"+row(() -> new String(chars, 0, 0))
      +"|"+row(() -> new String(bytes))+"|"+row(() -> new String(bytes, StandardCharsets.UTF_8))
      +"|"+rowThrow(() -> new String(bytes, (String)null))+"|"+row(() -> new String((StringBuilder)null))
      +"|"+row(() -> new String((StringBuffer)null));
  }
  /** A constructor Decaf can't model (UTF-16 decoding): every later use counts, `==` included. */
  public static String countedSelf() {
    String s = new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16);
    return String.valueOf(s == s);
  }
  /** Concatenating a String from a counted constructor counts the concatenation. */
  public static String countedConcat() {
    String s = new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16);
    return s + "!";
  }
  static final class CountedText {
    public String toString() {
      return new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16);
    }
  }
  public static String countedObjectConcat() { return new CountedText() + "!"; }
  public static String countedBuilderAppend() {
    String s = new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16);
    return new StringBuilder().append(s).toString();
  }
  public static String countedJoin() {
    String s = new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16);
    return String.join(",", new String[] {s});
  }
  public static String countedFormat() {
    String s = new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16);
    return String.format("%s", s);
  }
  public static String countedConcatLength() {
    String s = new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16);
    return String.valueOf((s + "!").length());
  }
  /** A copy of a counted String is itself counted: `outer.toString()` must count. */
  public static String countedCopy() {
    String inner = new String(new byte[] {0, 65}, java.nio.charset.StandardCharsets.UTF_16);
    String outer = new String(inner);
    return outer.toString();
  }
  public static void main(String[] args) { System.out.print(rows()); }
}
