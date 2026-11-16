package demo;
import java.util.*;
public class Probe {
 static String s(List<?> x){return x.toString();}
 public static String all(){
  StringBuilder b=new StringBuilder();
  List<Integer> a=new ArrayList<>(Arrays.asList(1,2,3)); Collections.reverse(a); b.append("array=").append(a);
  Integer[] raw={1,2,3}; List<Integer> av=Arrays.asList(raw); Collections.reverse(av); b.append(" av=").append(Arrays.toString(raw));
  List<Integer> l=new LinkedList<>(Arrays.asList(1,2,3)); Collections.reverse(l); b.append(" linked=").append(l);
  List<Integer> sub=new ArrayList<>(Arrays.asList(0,1,2,3,4)).subList(1,4); Collections.reverse(sub); b.append(" sub=").append(sub);
  List<Integer> n=new ArrayList<>(Arrays.asList(1,null,3)); Collections.reverse(n); b.append(" null=").append(n);
  for(List<?> z: List.of(Collections.emptyList(), List.of(1))) { try { Collections.reverse(z); b.append(" imm=ok"); } catch(Exception e){b.append(" imm=").append(e.getClass().getSimpleName()); }}
  try { Collections.reverse(null); } catch(Exception e){b.append(" npe=").append(e.getClass().getSimpleName()); }
  try { Collections.reverse(List.of(1,2)); } catch(Exception e){b.append(" imm2=").append(e.getClass().getSimpleName()); }
  return b.toString();
 }
}
