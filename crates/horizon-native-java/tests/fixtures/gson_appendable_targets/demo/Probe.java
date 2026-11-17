package demo;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import java.io.IOException;
import java.io.Writer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** Gson.toJson(Object, Appendable) into the targets mods save their configs through. */
public final class Probe {
  static Map<String, Object> config() {
    Map<String, Object> m = new LinkedHashMap<>();
    m.put("name", "probe");
    m.put("count", 3);
    m.put("enabled", true);
    List<Object> list = new ArrayList<>();
    list.add("a");
    list.add(2);
    m.put("list", list);
    m.put("missing", null);
    return m;
  }

  static String writeAndRead(Gson gson, String file) throws IOException {
    Path dir = Path.of("game/gson-appendable");
    Files.createDirectories(dir);
    Path path = dir.resolve(file);
    try (Writer writer = Files.newBufferedWriter(path)) {
      gson.toJson(config(), writer);
    }
    return Files.readString(path);
  }

  /** Rows Decaf answers exactly. */
  public static String rows() throws IOException {
    StringBuilder b = new StringBuilder();
    StringBuilder sb = new StringBuilder("prefix:");
    new Gson().toJson(config(), sb);
    b.append("stringBuilder=").append(sb).append('\n');
    b.append("bufferedWriter=").append(writeAndRead(new Gson(), "plain.json")).append('\n');
    b.append("pretty=")
        .append(writeAndRead(new GsonBuilder().setPrettyPrinting().create(), "pretty.json"))
        .append('\n');
    b.append("serializeNulls=")
        .append(writeAndRead(new GsonBuilder().serializeNulls().create(), "nulls.json"))
        .append('\n');
    Map<String, Object> allNull = new LinkedHashMap<>();
    allNull.put("a", null);
    StringBuilder an = new StringBuilder();
    new Gson().toJson(allNull, an);
    b.append("allNull=").append(an).append('\n');
    StringBuilder anp = new StringBuilder();
    new GsonBuilder().setPrettyPrinting().create().toJson(allNull, anp);
    b.append("allNullPretty=").append(anp).append('\n');
    StringBuilder nul = new StringBuilder();
    new Gson().toJson(null, nul);
    b.append("nullSource=").append(nul).append('\n');
    return b.toString();
  }

  /** An Appendable Decaf doesn't model (a jar class): counted. */
  public static String unmodelled() {
    final StringBuilder seen = new StringBuilder();
    Appendable own =
        new Appendable() {
          public Appendable append(CharSequence s) {
            seen.append(s);
            return this;
          }

          public Appendable append(CharSequence s, int start, int end) {
            seen.append(s, start, end);
            return this;
          }

          public Appendable append(char c) {
            seen.append(c);
            return this;
          }
        };
    new Gson().toJson(config(), own);
    return seen.toString();
  }

  public static void main(String[] args) throws IOException {
    System.out.print(args.length > 0 ? "unmodelled=" + unmodelled() + "\n" : rows());
  }
}
