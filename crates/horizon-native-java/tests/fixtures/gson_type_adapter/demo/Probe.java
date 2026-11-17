package demo;
import com.google.gson.*; import com.google.gson.stream.*; import java.io.*;
public class Probe {
 static class P { String value; P(String v){value=v;} }
 static class A extends TypeAdapter<String> { public void write(JsonWriter w,String v)throws IOException{w.value(v.toString());} public String read(JsonReader r){return null;} }
 static class O extends TypeAdapter<P> { public void write(JsonWriter w,P v)throws IOException{w.beginObject().name("text").value(v.value).endObject();} public P read(JsonReader r){return null;} }
 static class U extends TypeAdapter<P> { public void write(JsonWriter w,P v)throws IOException{w.setIndent("x");w.value(v.value);} public P read(JsonReader r){return null;} }
 public static void main(String[]x)throws Exception { P p=new P("hi"); Gson c=new GsonBuilder().registerTypeAdapter(String.class,new A()).create(); Gson o=new GsonBuilder().registerTypeAdapter(P.class,new O()).setPrettyPrinting().create(); Gson u=new GsonBuilder().registerTypeAdapter(P.class,new U()).create(); System.out.println(c.toJson(p)); System.out.println(o.toJson(p)); System.out.println(o.toJson(new P(null))); System.out.println(new Gson().toJson(p)); try { System.out.println(u.toJson(p)); } catch (Exception e) { System.out.println("COUNTED"); } }
}
