package demo;
import com.mojang.serialization.*;
import com.mojang.serialization.codecs.RecordCodecBuilder;
public class Probe {
 record Three(int a, String b, boolean c) {}
 static Codec<Three> build() {
  return RecordCodecBuilder.create(i -> i.group(
   Codec.INT.fieldOf("a").forGetter(Three::a),
   Codec.STRING.fieldOf("b").forGetter(Three::b),
   Codec.BOOL.fieldOf("c").forGetter(Three::c)).apply(i, Three::new));
 }
 public static String rows() {
  Codec<Three> a=build(), b=build();
  Codec<String> x=Codec.INT.xmap(Object::toString,Integer::valueOf);
  Codec<?> list=Codec.INT.listOf();
  return "record="+(a!=null)+"\nidentity="+(a==a)+"|"+(a!=b)+"\nxmap="+(x!=null)+"\nlist="+(list!=null)+"\n";
 }
 public static String encode() { build().encodeStart(null,new Three(1,"b",true)); return "called"; }
 public static String parse() { build().parse(null,(Object)null); return "called"; }
 public static void main(String[] args) { System.out.print(rows()); }
}
