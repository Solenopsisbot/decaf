package demo;
import com.google.gson.Gson;
import java.io.*;
import java.nio.file.*;
import java.util.*;
public final class Probe {
  public static final class Config { public int n; public String s; public List<Integer> xs; public Nested nested; }
  public static final class Nested { public String k; }
  static String show(Config c) { return c==null?"null":c.n+"|"+c.s+"|"+c.xs+"|"+(c.nested==null?"null":c.nested.k); }
  static String one(Gson g, Reader r) { try { return show(g.fromJson(r, Config.class)); } catch (Throwable t) { return t.getClass().getName()+":"+t.getMessage(); } }
  public static void main(String[] args) throws Exception { System.out.print(rows()); }
  public static String rows() throws Exception {
    Gson g=new Gson(); StringBuilder b=new StringBuilder();
    b.append("string=").append(one(g,new StringReader("{\"n\":3,\"s\":\"ok\",\"xs\":[1,2],\"nested\":{\"k\":\"v\"}}"))).append('\n');
    Path p=Path.of("reader-config.json"); Files.writeString(p,"{\"n\":4,\"s\":\"file\",\"xs\":[3],\"nested\":{\"k\":\"f\"}}");
    b.append("file=").append(one(g,Files.newBufferedReader(p))).append('\n');
    Reader r=new InputStreamReader(Probe.class.getResourceAsStream("/resource-config.json")); b.append("resource=").append(one(g,r)).append('\n');
    b.append("missing=").append(one(g,new StringReader("{\"n\":1}"))).append('\n');
    b.append("empty=").append(one(g,new StringReader(""))).append('\n');
    return b.toString();
  }
  public static String trailing() { return one(new Gson(), new StringReader("{\"n\":1} garbage")); }
  public static String twice() { Gson g=new Gson(); StringReader r=new StringReader("{\"n\":8}"); return one(g,r)+"|"+one(g,r); }
}
