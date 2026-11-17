package demo;
import java.util.*;
public class Probe {
 static void row(StringBuilder s, String n, Runnable r){s.append(n).append('=');try{r.run();s.append('\n');}catch(Throwable e){s.append(e.getClass().getName()).append(':').append(e.getMessage()).append('\n');}}
 public static String rows(){StringBuilder s=new StringBuilder();
  row(s,"boolean",()->{boolean[] a={false,false};Arrays.fill(a,true);s.append(Arrays.toString(a));});
  row(s,"byte",()->{byte[] a={0,0};Arrays.fill(a,(byte)3);s.append(Arrays.toString(a));});
  row(s,"char",()->{char[] a={0,0};Arrays.fill(a,'x');s.append(Arrays.toString(a));});
  row(s,"short",()->{short[] a={0,0};Arrays.fill(a,(short)3);s.append(Arrays.toString(a));});
  row(s,"int",()->{int[] a={0,0};Arrays.fill(a,3);s.append(Arrays.toString(a));});
  row(s,"long",()->{long[] a={0,0};Arrays.fill(a,3L);s.append(Arrays.toString(a));});
  row(s,"float",()->{float[] a={1,-1};Arrays.fill(a,-0.0F);s.append(Arrays.toString(a));});
  row(s,"double",()->{double[] a={1,-1};Arrays.fill(a,Double.NaN);s.append(Arrays.toString(a));});
  row(s,"object",()->{Object[] a={null,null};Arrays.fill(a,"x");s.append(Arrays.toString(a));});
  row(s,"object-nonstr",()->{Object[] a={new Object()};s.append(Arrays.toString(a));});
  row(s,"object-boxed",()->{Object[] a={true,'x',1};s.append(Arrays.toString(a));});
  row(s,"char-surrogate",()->{char[] a={'x'};Arrays.fill(a,'\uD800');s.append(Arrays.toString(a));});
  row(s,"range",()->{int[] a={1,2,3,4};Arrays.fill(a,1,3,7);s.append(Arrays.toString(a));});
  row(s,"from-negative",()->Arrays.fill(new int[2],-1,1,4));
  row(s,"to-large",()->Arrays.fill(new int[2],0,9,4));
  row(s,"from-after-to",()->Arrays.fill(new int[2],5,3,4));
  row(s,"store",()->{Object[] a=new String[1];Arrays.fill(a,Integer.valueOf(1));});
  return s.toString(); }
 public static void main(String[] x){System.out.print(rows());}
}
