package demo;
import com.google.gson.*;
import com.google.gson.reflect.TypeToken;
import com.google.gson.stream.JsonReader;
import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.lang.reflect.Type;

public final class Probe {
  static final class P {
    String value;
    P(String value) { this.value = value; }
  }
  static class Prefix extends TypeAdapter<P> {
    public void write(JsonWriter out, P value) throws IOException { out.value("FOO:" + value.value); }
    public P read(JsonReader in) throws IOException { return null; }
  }
  static class StringPrefix extends TypeAdapter<String> {
    public void write(JsonWriter out, String value) throws IOException { out.value("FOO:" + value); }
    public String read(JsonReader in) throws IOException { return in.nextString(); }
  }
  static class Hierarchy extends TypeAdapter<Object> {
    public void write(JsonWriter out, Object value) throws IOException {
      out.value("HIER" + ((P) value).value);
    }
    public Object read(JsonReader in) throws IOException { return null; }
  }
  static class Factory implements TypeAdapterFactory {
    final String prefix;
    Factory(String prefix) { this.prefix = prefix; }
    public <T> TypeAdapter<T> create(Gson gson, TypeToken<T> type) {
      if (type.getRawType() != String.class) return null;
      return (TypeAdapter<T>) new TypeAdapter<String>() {
        public void write(JsonWriter out, String value) throws IOException { out.value(prefix + value); }
        public String read(JsonReader in) throws IOException { return in.nextString(); }
      };
    }
  }
  static String normal() { return new Gson().toJson(new P("x")); }
  static String exact() {
    return new GsonBuilder().registerTypeAdapter(P.class, new Prefix()).create().toJson(new P("x"));
  }
  static String nested() {
    Gson gson = new GsonBuilder().registerTypeAdapter(String.class, new StringPrefix()).create();
    return gson.toJson(new P("x"));
  }
  static String isolation() {
    Gson foo = new GsonBuilder().registerTypeAdapter(P.class, new Prefix()).create();
    return foo.toJson(new P("x")) + "|" + new Gson().toJson(new P("x"));
  }
  static String nullSafe() {
    try {
      Gson gson = new GsonBuilder().registerTypeAdapter(P.class, new Prefix().nullSafe()).create();
      return gson.toJson(new P("x")) + "|" + gson.toJson((P) null);
    } catch (Throwable e) {
      return "COUNTED";
    }
  }
  static String countedNullSafe() {
    TypeAdapter<P> adapter = new Prefix().nullSafe();
    return new GsonBuilder().registerTypeAdapter(P.class, adapter).create().toJson(new P("x"));
  }
  static String exactVsHierarchy() {
    Gson gson = new GsonBuilder()
        .registerTypeHierarchyAdapter(Object.class, new Hierarchy())
        .registerTypeAdapter(P.class, new Prefix()).create();
    return gson.toJson(new P("x"));
  }
  static String hierarchyVsExact() {
    Gson gson = new GsonBuilder()
        .registerTypeAdapter(P.class, new Prefix())
        .registerTypeHierarchyAdapter(Object.class, new Hierarchy()).create();
    return gson.toJson(new P("x"));
  }
  static String factoryAB() {
    Gson gson = new GsonBuilder().registerTypeAdapterFactory(new Factory("A:"))
        .registerTypeAdapterFactory(new Factory("B:")).create();
    return gson.toJson("x");
  }
  static String factoryBA() {
    Gson gson = new GsonBuilder().registerTypeAdapterFactory(new Factory("B:"))
        .registerTypeAdapterFactory(new Factory("A:")).create();
    return gson.toJson("x");
  }
  static String lambdaSerializer() {
    JsonSerializer<String> serializer = (value, type, context) -> new JsonPrimitive("SER:" + value);
    return new GsonBuilder().registerTypeAdapter(String.class, serializer).create().toJson("x");
  }
  static String lambdaDeserializer() {
    JsonDeserializer<String> deserializer = (json, type, context) -> "DES:" + json.getAsString();
    return new GsonBuilder().registerTypeAdapter(String.class, deserializer).create()
        .fromJson("\"x\"", String.class);
  }
  public static void main(String[] args) {
    System.out.println("normal=" + normal());
    System.out.println("exact=" + exact());
    System.out.println("nested=" + nested());
    System.out.println("isolation=" + isolation());
    System.out.println("nullSafe=" + nullSafe());
    System.out.println("exactVsHierarchy=" + exactVsHierarchy());
    System.out.println("hierarchyVsExact=" + hierarchyVsExact());
    System.out.println("factoryAB=" + factoryAB());
    System.out.println("factoryBA=" + factoryBA());
    System.out.println("lambdaSerializer=" + lambdaSerializer());
    System.out.println("lambdaDeserializer=" + lambdaDeserializer());
  }
}
