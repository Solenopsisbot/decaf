package demo;
import java.nio.charset.*; import java.util.*;
public class Probe {
 static String row(String n, java.util.function.Supplier<String> f){try{return n+"="+f.get();}catch(Throwable e){return n+"="+e.getClass().getName()+":"+e.getMessage();}}
 static String b(byte[] x){return Arrays.toString(x);}
 public static String rows(){ String s="hé😀"; StringBuilder o=new StringBuilder();
  o.append(row("utf8",()->b(s.getBytes(StandardCharsets.UTF_8)))).append('\n');
  o.append(row("ascii",()->b(s.getBytes(StandardCharsets.US_ASCII)))).append('\n');
  o.append(row("latin1",()->b(s.getBytes(StandardCharsets.ISO_8859_1)))).append('\n');
  o.append(row("utf16",()->b(s.getBytes(StandardCharsets.UTF_16)))).append('\n');
  o.append(row("utf16be",()->b(s.getBytes(StandardCharsets.UTF_16BE)))).append('\n');
  o.append(row("utf16le",()->b(s.getBytes(StandardCharsets.UTF_16LE)))).append('\n');
  o.append(row("default",()->b(s.getBytes()))).append('\n');
  o.append(row("null",()->b(s.getBytes((Charset)null)))).append('\n');
  o.append(row("custom",()->b(s.getBytes(Charset.forName("UTF-8"))))).append('\n');
  String u="a\uD800b";
  o.append(row("u8",()->b(u.getBytes(StandardCharsets.UTF_8)))).append("\n");
  o.append(row("ua",()->b(u.getBytes(StandardCharsets.US_ASCII)))).append("\n");
  o.append(row("ul",()->b(u.getBytes(StandardCharsets.ISO_8859_1)))).append("\n");
  o.append(row("ube",()->b(u.getBytes(StandardCharsets.UTF_16BE)))).append("\n");
  o.append(row("ule",()->b(u.getBytes(StandardCharsets.UTF_16LE)))).append("\n");
  String g="a\uFFFDb";
  o.append(row("g8",()->b(g.getBytes(StandardCharsets.UTF_8))));
  return o.toString(); }
 public static String nameOverload(){String s="hé"; StringBuilder o=new StringBuilder(); try{o.append("name="+b(s.getBytes("UTF-8"))+"\n");}catch(Throwable e){o.append("name="+e.getClass().getName()+":"+e.getMessage()+"\n");} try{o.append("bad="+b(s.getBytes("NOPE")));}catch(Throwable e){o.append("bad="+e.getClass().getName()+":"+e.getMessage());} return o.toString();}
}
