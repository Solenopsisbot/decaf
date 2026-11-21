package demo;
import com.google.gson.Gson;
import com.google.gson.JsonElement;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonSyntaxException;
import java.io.StringReader;
/**
 * gson-2.14.0's nesting limit: documents nested to 255 levels parse, and the 256th opening bracket
 * throws JsonSyntaxException(MalformedJsonException "Nesting limit 255 reached at line L column C
 * path P"), through JsonParser.parseString/parseReader and Gson.fromJson into Object and
 * JsonElement. The documents vary what the line, column and path count: whitespace, \r and \n,
 * a byte order mark, non-ASCII and surrogate pairs, newlines in strings and comments, array
 * indices, member names (with a dot, a space, empty, escaped).
 */
public class Depth {
  static String arrays(int n) { return "[".repeat(n) + "]".repeat(n); }
  static String objects(String name, int n) {
    StringBuilder s = new StringBuilder();
    for (int i = 0; i < n; i++) s.append("{\"").append(name).append("\":");
    s.append("1");
    for (int i = 0; i < n; i++) s.append('}');
    return s.toString();
  }
  static final String[] DOCS = {
    arrays(255),
    arrays(256),
    arrays(300),
    objects("a", 256),
    objects("a.b", 256),
    objects("x y", 256),
    objects("", 256),
    "[1,2," + arrays(255) + "]",
    "[[],[1]," + arrays(255) + "]",
    "{\"k\":1,\"m\":" + arrays(255) + "}",
    "{\"k\":1,\"m\":" + arrays(254) + "}",
    "{\"\\u0041\\n\":" + arrays(255) + "}",
    " \n  \t" + arrays(256),
    "\r\n\r\n" + arrays(256),
    "\r\r" + arrays(256),
    "\n" + "[\n".repeat(256),
    "﻿" + arrays(256),
    "[\"é😀\"," + arrays(255) + "]",
    "[\"a\nb\"," + arrays(255) + "]",
    "/* x\ny */" + arrays(256),
    "// c\n" + arrays(256),
    "# c\n" + arrays(256),
    "[" + "{\"a\":[".repeat(127) + "{}",
    "[" + "{\"a\":[".repeat(128),
  };
  static String describe(Throwable e) {
    String s = e.getClass().getName() + ": " + e.getMessage();
    if (e.getCause() != null) s += " | cause " + e.getCause().getClass().getName() + ": " + e.getCause().getMessage();
    s += " | parse=" + (e instanceof JsonParseException) + " syntax=" + (e instanceof JsonSyntaxException) + " runtime=" + (e instanceof RuntimeException);
    return s;
  }
  public static String rows() {
    StringBuilder s = new StringBuilder();
    for (int i = 0; i < DOCS.length; i++) {
      String doc = DOCS[i];
      try { JsonParser.parseString(doc); s.append(i).append(" parseString ok\n"); }
      catch (RuntimeException e) { s.append(i).append(" parseString ").append(describe(e)).append('\n'); }
      try { new Gson().fromJson(doc, Object.class); s.append(i).append(" fromJson(Object) ok\n"); }
      catch (RuntimeException e) { s.append(i).append(" fromJson(Object) ").append(describe(e)).append('\n'); }
      try { new Gson().fromJson(doc, JsonElement.class); s.append(i).append(" fromJson(JsonElement) ok\n"); }
      catch (RuntimeException e) { s.append(i).append(" fromJson(JsonElement) ").append(describe(e)).append('\n'); }
      try { JsonParser.parseReader(new StringReader(doc)); s.append(i).append(" parseReader ok\n"); }
      catch (JsonSyntaxException e) { s.append(i).append(" parseReader caught JsonSyntaxException ").append(describe(e)).append('\n'); }
    }
    for (int depth : new int[] {1, 32, 33, 128, 129, 254, 255, 256, 257, 100000}) {
      try { JsonParser.parseString(arrays(depth)); s.append("depth ").append(depth).append(" ok\n"); }
      catch (JsonParseException e) { s.append("depth ").append(depth).append(" caught JsonParseException ").append(e.getCause().getMessage().length()).append('\n'); }
    }
    return s.toString();
  }
  public static void main(String[] args) { System.out.print(rows()); }
}
