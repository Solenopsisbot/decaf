package demo;
import net.minecraft.util.RandomSource;
public class Probe {
 public static String rows(){ RandomSource a=RandomSource.create(), b=RandomSource.create(); StringBuilder s=new StringBuilder(); s.append(a.getClass().getName()).append('\n'); s.append(a.nextLong()!=b.nextLong()).append('\n'); RandomSource r=RandomSource.create(42L); s.append(r.nextLong()).append(',').append(r.nextInt()).append(',').append(r.nextFloat()).append(',').append(r.nextDouble()).append('\n'); return s.toString(); }
 public static void main(String[] args){System.out.print(rows());}
}
