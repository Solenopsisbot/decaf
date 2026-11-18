package demo;
import java.util.*; import java.util.stream.*;
public class Probe {
 static String rev(List<Integer> l){ Collections.reverse(l); return l.toString(); }
 static String err(Runnable r){try{r.run();return "ok";}catch(Exception e){return e.getClass().getSimpleName()+":"+e.getMessage();}}
 public static String rows(){StringBuilder s=new StringBuilder();
  for(List<Integer> l: (List<List<Integer>>)(List)List.of(new ArrayList<Integer>(),new ArrayList<>(List.of(1)),new ArrayList<>(List.of(1,2)),new ArrayList<>(List.of(1,2,3)),new LinkedList<>(List.of(1,2)),new LinkedList<>(List.of(1,2,3)))) s.append(rev(l)).append('|');
  for(List<Integer> l: (List<List<Integer>>)(List)List.of(List.<Integer>of(),List.of(1),List.of(1,2))) s.append(err(()->Collections.reverse(l))).append('|');
  List<Integer> m=new ArrayList<>(List.of(1,2,3)); Iterator<Integer> it=m.iterator(); Collections.reverse(m); s.append(err(()->it.next())).append('|');
  List<Integer> a=Arrays.asList(1,2,3); s.append(rev(a)).append('|');
  List<Integer> u=Collections.unmodifiableList(new ArrayList<>(List.of(1,2))); s.append(err(()->Collections.reverse(u))).append('|');
  Iterator<Integer> si=Stream.of(1,2,3).iterator(); si.next(); s.append(err(si::remove)).append('|');
  List<Integer> ll=new LinkedList<>(List.of(1,2,3)); Iterator<Integer> li=ll.iterator(); Collections.reverse(ll); s.append(err(li::next)).append('|');
  s.append(err(()->Collections.reverse(null))).append('|');
  return s.toString(); }
 public static String subListReverse(){ List<Integer> p=new ArrayList<>(List.of(1,2,3,4)); List<Integer> sub=p.subList(1,3); rev(sub); return p.toString(); }
 public static void main(String[] x){System.out.print(rows());}
}
