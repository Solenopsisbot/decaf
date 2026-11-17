package demo;
import net.minecraft.network.codec.StreamCodec;
public final class Probe {
 public static String rows(){
  StreamCodec<Object,Integer> base=StreamCodec.unit(1);
  StreamCodec<?,?> m1=base.map(x->x,x->x), m2=base.map(x->x,x->x);
  return "mapDistinct="+(m1!=m2)+"\nmapCodec="+(m1 instanceof StreamCodec)+"\n";
 }
 public static String counted(){
  StreamCodec<Object,Integer> base=StreamCodec.unit(1);
  base.apply(c->c.map(x->x,x->x));
  return "applied";
 }
 public static void main(String[]x){System.out.print(rows());}
}
