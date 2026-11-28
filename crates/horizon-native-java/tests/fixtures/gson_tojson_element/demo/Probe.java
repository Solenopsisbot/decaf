package demo;

import com.google.gson.ExclusionStrategy;
import com.google.gson.FieldAttributes;
import com.google.gson.FieldNamingPolicy;
import com.google.gson.FieldNamingStrategy;
import com.google.gson.FormattingStyle;
import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import com.google.gson.JsonSerializationContext;
import com.google.gson.JsonSerializer;
import com.google.gson.LongSerializationPolicy;
import com.google.gson.ToNumberPolicy;
import com.google.gson.TypeAdapter;
import com.google.gson.TypeAdapterFactory;
import com.google.gson.reflect.TypeToken;
import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.io.StringReader;
import java.io.StringWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Type;

/**
 * Gson.toJson(JsonElement) and its Appendable / JsonWriter forms. Every row
 * writes a tree and prints the text; rows named {@code adapter*}, {@code
 * strategy*} and so on build the Gson with an option the tree writer must
 * ignore (a tree is written by {@code JsonElement}'s own adapter, never by the
 * adapter list).
 */
public final class Probe {
  /* ---------------- trees ---------------- */

  /** The shape a config manager writes: booleans, enum names, a string set. */
  static JsonObject config() {
    JsonObject o = new JsonObject();
    o.addProperty("sort_mode", "alphabetical");
    o.addProperty("count_libraries", Boolean.TRUE);
    o.addProperty("compact_list", Boolean.FALSE);
    JsonArray hidden = new JsonArray();
    hidden.add("alpha");
    hidden.add("beta");
    o.add("hidden_mods", hidden);
    o.add("empty_set", new JsonArray());
    return o;
  }

  /** Escapes, numbers, nesting, nulls. */
  static JsonObject rich() {
    JsonObject o = new JsonObject();
    o.addProperty("quote\"key", "a\"b\\c/d");
    o.addProperty("html", "<tag a='1'>&=</tag>");
    o.addProperty("ctl", "\u0000\u0001\b\t\n\f\r\u001f\u007f\u0080");
    o.addProperty("sep", "\u2028\u2029");
    o.addProperty("uni", "\u00e9\u4e2d\ud83d\ude00");
    o.addProperty("<k>", "v");
    o.addProperty("int", 7);
    o.addProperty("min", Integer.MIN_VALUE);
    o.addProperty("long", 9000000000L);
    o.addProperty("lmin", Long.MIN_VALUE);
    o.addProperty("dbl", 1.0);
    o.addProperty("dbl2", 100.0);
    o.addProperty("dbl3", 0.001);
    o.addProperty("dbl4", 1.0E-5);
    o.addProperty("dbl5", 1.0E7);
    o.addProperty("dbl6", 9999999.0);
    o.addProperty("dbl7", 123456789.123456789);
    o.addProperty("dbl8", -0.0);
    o.addProperty("flt", 1.1f);
    o.addProperty("sh", Short.valueOf((short) -3));
    o.addProperty("by", Byte.valueOf((byte) 5));
    o.addProperty("chr", 'x');
    o.addProperty("yes", true);
    o.addProperty("no", false);
    o.add("nul", JsonNull.INSTANCE);
    o.add("nested", config());
    JsonArray a = new JsonArray();
    a.add("s");
    a.add(1);
    a.add(2.5);
    a.add(true);
    a.add('c');
    a.add(JsonNull.INSTANCE);
    a.add(new JsonObject());
    a.add(new JsonArray());
    JsonArray inner = new JsonArray();
    inner.add(new JsonPrimitive("deep"));
    a.add(inner);
    JsonObject member = new JsonObject();
    member.addProperty("k", "v");
    member.add("n", JsonNull.INSTANCE);
    a.add(member);
    o.add("arr", a);
    o.add("emptyObj", new JsonObject());
    return o;
  }

  /* ---------------- adapters / strategies the tree ignores ---------------- */

  static final class Ser implements JsonSerializer<Object> {
    public JsonElement serialize(Object src, Type typeOfSrc, JsonSerializationContext context) {
      return new JsonPrimitive("SERIALIZER-RAN");
    }
  }

  static final class SkipAll implements ExclusionStrategy {
    public boolean shouldSkipField(FieldAttributes f) {
      return true;
    }

    public boolean shouldSkipClass(Class<?> clazz) {
      return true;
    }
  }

  static final class Names implements FieldNamingStrategy {
    public String translateName(Field f) {
      return "NAMED";
    }
  }

  static final class Factory implements TypeAdapterFactory {
    public <T> TypeAdapter<T> create(Gson gson, TypeToken<T> type) {
      return null;
    }
  }

  /* ---------------- helpers ---------------- */

  /** toJson(JsonElement), then toJson(JsonElement, Appendable). */
  static String both(Gson g, JsonElement e) {
    StringBuilder sb = new StringBuilder();
    g.toJson(e, sb);
    return g.toJson(e) + "\n--\n" + sb;
  }

  static String esc(String s) {
    return s.replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t");
  }

  /* ---------------- default Gson ---------------- */

  public static String defaultConfig() {
    return both(new Gson(), config());
  }

  public static String defaultRich() {
    return both(new Gson(), rich());
  }

  public static String prettyConfig() {
    return both(new GsonBuilder().setPrettyPrinting().create(), config());
  }

  public static String prettyRich() {
    return both(new GsonBuilder().setPrettyPrinting().create(), rich());
  }

  public static String nullsRich() {
    return both(new GsonBuilder().serializeNulls().create(), rich());
  }

  public static String noHtmlRich() {
    return both(new GsonBuilder().disableHtmlEscaping().create(), rich());
  }

  public static String prettyNullsNoHtmlRich() {
    return both(new GsonBuilder().setPrettyPrinting().serializeNulls().disableHtmlEscaping().create(), rich());
  }

  public static String nonExecutable() {
    return both(new GsonBuilder().generateNonExecutableJson().create(), config());
  }

  public static String longsAsStrings() {
    return both(new GsonBuilder().setLongSerializationPolicy(LongSerializationPolicy.STRING).create(), rich());
  }

  public static String specialFloatsOption() {
    return both(new GsonBuilder().serializeSpecialFloatingPointValues().create(), rich());
  }

  public static String formatTabs() {
    FormattingStyle s = FormattingStyle.PRETTY.withIndent("\t");
    return both(new GsonBuilder().setFormattingStyle(s).create(), rich());
  }

  public static String formatCrlf() {
    FormattingStyle s = FormattingStyle.PRETTY.withNewline("\r\n").withIndent("    ");
    return both(new GsonBuilder().setFormattingStyle(s).create(), rich());
  }

  public static String formatCompactSpaced() {
    FormattingStyle s = FormattingStyle.COMPACT.withSpaceAfterSeparators(true);
    return both(new GsonBuilder().setFormattingStyle(s).create(), rich());
  }

  public static String formatPrettyNoSpace() {
    FormattingStyle s = FormattingStyle.PRETTY.withSpaceAfterSeparators(false);
    return both(new GsonBuilder().setFormattingStyle(s).create(), rich());
  }

  /* ---------------- single-primitive and null documents ---------------- */

  public static String topLevels() {
    Gson g = new Gson();
    return g.toJson(new JsonPrimitive("a<b")) + "|" + g.toJson(new JsonPrimitive(7)) + "|"
        + g.toJson(new JsonPrimitive(true)) + "|" + g.toJson(JsonNull.INSTANCE) + "|" + g.toJson(new JsonObject()) + "|"
        + g.toJson(new JsonArray());
  }

  public static String nullElement() {
    Gson g = new Gson();
    StringBuilder sb = new StringBuilder();
    g.toJson((JsonElement) null, sb);
    return g.toJson((JsonElement) null) + "|" + sb;
  }

  public static String nullElementNulls() {
    Gson g = new GsonBuilder().serializeNulls().setPrettyPrinting().create();
    StringBuilder sb = new StringBuilder();
    g.toJson((JsonElement) null, sb);
    return g.toJson((JsonElement) null) + "|" + sb;
  }

  public static String nullMemberTop() {
    Gson g = new Gson();
    JsonObject o = new JsonObject();
    o.add("only", JsonNull.INSTANCE);
    JsonArray a = new JsonArray();
    a.add(JsonNull.INSTANCE);
    return g.toJson(o) + "|" + g.toJson(a);
  }

  /* ---------------- appendable targets ---------------- */

  public static String stringWriter() {
    StringWriter w = new StringWriter();
    new GsonBuilder().setPrettyPrinting().create().toJson(config(), w);
    return w.toString();
  }

  public static String stringBuilderAppends() {
    StringBuilder sb = new StringBuilder("<<");
    Gson g = new Gson();
    g.toJson(new JsonPrimitive("one"), sb);
    sb.append("::");
    g.toJson(config(), sb);
    return sb.toString();
  }

  public static String jsonWriterCompact() throws IOException {
    StringWriter w = new StringWriter();
    JsonWriter jw = new JsonWriter(w);
    new Gson().toJson(rich(), jw);
    jw.flush();
    return w.toString();
  }

  public static String jsonWriterOptions() throws IOException {
    StringWriter w = new StringWriter();
    JsonWriter jw = new JsonWriter(w);
    new GsonBuilder().disableHtmlEscaping().serializeNulls().create().toJson(rich(), jw);
    jw.flush();
    return w.toString();
  }

  /* ---------------- options the tree ignores ---------------- */

  public static String modMenuStyle() {
    return both(
        new GsonBuilder()
            .registerTypeHierarchyAdapter(Enum.class, new Ser())
            .setFieldNamingPolicy(FieldNamingPolicy.LOWER_CASE_WITH_UNDERSCORES)
            .setPrettyPrinting()
            .create(),
        config());
  }

  public static String adapterJsonObject() {
    return both(new GsonBuilder().registerTypeAdapter(JsonObject.class, new Ser()).create(), rich());
  }

  public static String adapterJsonArray() {
    return both(new GsonBuilder().registerTypeAdapter(JsonArray.class, new Ser()).create(), rich());
  }

  public static String adapterJsonPrimitive() {
    return both(new GsonBuilder().registerTypeAdapter(JsonPrimitive.class, new Ser()).create(), rich());
  }

  public static String adapterJsonElement() {
    return both(new GsonBuilder().registerTypeAdapter(JsonElement.class, new Ser()).create(), rich());
  }

  public static String hierarchyJsonElement() {
    return both(new GsonBuilder().registerTypeHierarchyAdapter(JsonElement.class, new Ser()).create(), rich());
  }

  public static String adapterString() {
    return both(new GsonBuilder().registerTypeAdapter(String.class, new Ser()).create(), rich());
  }

  public static String adapterBoolean() {
    return both(new GsonBuilder().registerTypeAdapter(Boolean.class, new Ser()).create(), rich());
  }

  public static String adapterNumber() {
    return both(new GsonBuilder().registerTypeAdapter(Integer.class, new Ser()).create(), rich());
  }

  public static String adapterFactory() {
    return both(new GsonBuilder().registerTypeAdapterFactory(new Factory()).create(), rich());
  }

  public static String namingStrategy() {
    return both(new GsonBuilder().setFieldNamingStrategy(new Names()).create(), rich());
  }

  public static String exclusionStrategies() {
    return both(new GsonBuilder().setExclusionStrategies(new SkipAll()).create(), rich());
  }

  public static String exclusionSerialization() {
    return both(new GsonBuilder().addSerializationExclusionStrategy(new SkipAll()).create(), rich());
  }

  public static String exclusionDeserialization() {
    return both(new GsonBuilder().addDeserializationExclusionStrategy(new SkipAll()).create(), rich());
  }

  public static String exposeOnly() {
    return both(new GsonBuilder().excludeFieldsWithoutExposeAnnotation().create(), rich());
  }

  public static String complexMapKeys() {
    return both(new GsonBuilder().enableComplexMapKeySerialization().create(), rich());
  }

  public static String dateFormat() {
    return both(new GsonBuilder().setDateFormat("yyyy-MM-dd").create(), rich());
  }

  public static String version() {
    return both(new GsonBuilder().setVersion(1.0).create(), rich());
  }

  public static String objectToNumber() {
    return both(new GsonBuilder().setObjectToNumberStrategy(ToNumberPolicy.DOUBLE).create(), rich());
  }

  public static String numberToNumber() {
    return both(new GsonBuilder().setNumberToNumberStrategy(ToNumberPolicy.BIG_DECIMAL).create(), rich());
  }

  public static String innerClasses() {
    return both(new GsonBuilder().disableInnerClassSerialization().create(), rich());
  }

  public static String everythingAtOnce() {
    return both(
        new GsonBuilder()
            .registerTypeHierarchyAdapter(JsonElement.class, new Ser())
            .registerTypeAdapterFactory(new Factory())
            .setFieldNamingStrategy(new Names())
            .setExclusionStrategies(new SkipAll())
            .excludeFieldsWithoutExposeAnnotation()
            .enableComplexMapKeySerialization()
            .setDateFormat("yyyy")
            .setVersion(2.0)
            .setObjectToNumberStrategy(ToNumberPolicy.DOUBLE)
            .disableInnerClassSerialization()
            .setPrettyPrinting()
            .serializeNulls()
            .disableHtmlEscaping()
            .generateNonExecutableJson()
            .create(),
        rich());
  }

  public static String adapterJsonWriter() throws IOException {
    StringWriter w = new StringWriter();
    JsonWriter jw = new JsonWriter(w);
    new GsonBuilder().registerTypeAdapter(JsonObject.class, new Ser()).setExclusionStrategies(new SkipAll()).create().toJson(rich(), jw);
    jw.flush();
    return w.toString();
  }

  /* ---------------- toJson(Object) of a tree: only the default Gson is answered ---------------- */

  public static String objectDefault() {
    return new Gson().toJson((Object) config());
  }

  public static String objectWithAdapter() {
    return new GsonBuilder().registerTypeAdapter(JsonObject.class, new Ser()).create().toJson((Object) config());
  }

  public static String nullElementAdapter() {
    Gson g = new GsonBuilder().registerTypeAdapter(JsonObject.class, new Ser()).create();
    StringBuilder sb = new StringBuilder();
    g.toJson((JsonElement) null, sb);
    return g.toJson((JsonElement) null) + "|" + sb;
  }

  /** Write the config, read it back (a config manager's save then load). */
  public static String roundTrip() {
    Gson g =
        new GsonBuilder()
            .registerTypeHierarchyAdapter(Enum.class, new Ser())
            .setFieldNamingPolicy(FieldNamingPolicy.LOWER_CASE_WITH_UNDERSCORES)
            .setPrettyPrinting()
            .create();
    String text = g.toJson(config());
    JsonObject back = JsonParser.parseReader(new StringReader(text)).getAsJsonObject();
    return back.get("sort_mode").getAsString() + "|" + back.get("count_libraries").getAsBoolean() + "|"
        + back.getAsJsonArray("hidden_mods").size() + "|" + back.has("empty_set");
  }

  /* ---------------- non-finite numbers (stay counted) ---------------- */

  public static String nanDefault() {
    return new Gson().toJson(new JsonPrimitive(Double.NaN));
  }

  public static String infinityInArray() {
    JsonArray a = new JsonArray();
    a.add(1);
    a.add(Double.POSITIVE_INFINITY);
    return new Gson().toJson(a);
  }

  /* ---------------- the rows ---------------- */

  interface Row {
    String run() throws Exception;
  }

  static void row(StringBuilder b, String label, Row r) {
    String text;
    try {
      text = r.run();
    } catch (Throwable t) {
      text = "THROWS " + t.getClass().getName() + ": " + t.getMessage();
    }
    b.append(label).append('=').append(esc(text)).append('\n');
  }

  public static String rows() {
    StringBuilder b = new StringBuilder();
    row(b, "defaultConfig", Probe::defaultConfig);
    row(b, "defaultRich", Probe::defaultRich);
    row(b, "prettyConfig", Probe::prettyConfig);
    row(b, "prettyRich", Probe::prettyRich);
    row(b, "nullsRich", Probe::nullsRich);
    row(b, "noHtmlRich", Probe::noHtmlRich);
    row(b, "prettyNullsNoHtmlRich", Probe::prettyNullsNoHtmlRich);
    row(b, "nonExecutable", Probe::nonExecutable);
    row(b, "longsAsStrings", Probe::longsAsStrings);
    row(b, "specialFloatsOption", Probe::specialFloatsOption);
    row(b, "formatTabs", Probe::formatTabs);
    row(b, "formatCrlf", Probe::formatCrlf);
    row(b, "formatCompactSpaced", Probe::formatCompactSpaced);
    row(b, "formatPrettyNoSpace", Probe::formatPrettyNoSpace);
    row(b, "topLevels", Probe::topLevels);
    row(b, "nullElement", Probe::nullElement);
    row(b, "nullElementNulls", Probe::nullElementNulls);
    row(b, "nullMemberTop", Probe::nullMemberTop);
    row(b, "stringWriter", Probe::stringWriter);
    row(b, "stringBuilderAppends", Probe::stringBuilderAppends);
    row(b, "jsonWriterCompact", Probe::jsonWriterCompact);
    row(b, "jsonWriterOptions", Probe::jsonWriterOptions);
    row(b, "modMenuStyle", Probe::modMenuStyle);
    row(b, "adapterJsonObject", Probe::adapterJsonObject);
    row(b, "adapterJsonArray", Probe::adapterJsonArray);
    row(b, "adapterJsonPrimitive", Probe::adapterJsonPrimitive);
    row(b, "adapterJsonElement", Probe::adapterJsonElement);
    row(b, "hierarchyJsonElement", Probe::hierarchyJsonElement);
    row(b, "adapterString", Probe::adapterString);
    row(b, "adapterBoolean", Probe::adapterBoolean);
    row(b, "adapterNumber", Probe::adapterNumber);
    row(b, "adapterFactory", Probe::adapterFactory);
    row(b, "namingStrategy", Probe::namingStrategy);
    row(b, "exclusionStrategies", Probe::exclusionStrategies);
    row(b, "exclusionSerialization", Probe::exclusionSerialization);
    row(b, "exclusionDeserialization", Probe::exclusionDeserialization);
    row(b, "exposeOnly", Probe::exposeOnly);
    row(b, "complexMapKeys", Probe::complexMapKeys);
    row(b, "dateFormat", Probe::dateFormat);
    row(b, "version", Probe::version);
    row(b, "objectToNumber", Probe::objectToNumber);
    row(b, "numberToNumber", Probe::numberToNumber);
    row(b, "innerClasses", Probe::innerClasses);
    row(b, "everythingAtOnce", Probe::everythingAtOnce);
    row(b, "adapterJsonWriter", Probe::adapterJsonWriter);
    row(b, "objectDefault", Probe::objectDefault);
    row(b, "objectWithAdapter", Probe::objectWithAdapter);
    row(b, "nullElementAdapter", Probe::nullElementAdapter);
    row(b, "roundTrip", Probe::roundTrip);
    row(b, "nanDefault", Probe::nanDefault);
    row(b, "infinityInArray", Probe::infinityInArray);
    return b.toString();
  }

  public static void main(String[] args) {
    System.out.print(rows());
  }
}
