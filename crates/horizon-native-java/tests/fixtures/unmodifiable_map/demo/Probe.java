package demo;
import java.util.*; import java.util.stream.*;
public class Probe {
 public static void main(String[] a){System.out.print(rows());}
 static String err(Runnable r){try{r.run();return "ok";}catch(Throwable e){return e.getClass().getName();}}
 static Map<String,Integer> map(){return Stream.of("a","b").collect(Collectors.toUnmodifiableMap(x->x,x->x.length()));}
 public static String rows(){Map<String,Integer> m=map(); StringBuilder s=new StringBuilder();
  try{Stream.of("x").collect(Collectors.toUnmodifiableMap(x->(String)null,x->1));s.append("no");}catch(Throwable e){s.append(e.getClass().getName());}
  s.append("|").append(err(()->m.put("x",1))).append("|").append(err(()->m.remove("a"))).append("|").append(err(m::clear));
  s.append("|").append(err(()->m.keySet().remove("a"))).append("|").append(err(()->m.values().remove(1))).append("|").append(err(()->m.entrySet().clear()));
  s.append("|").append(m.get("a")).append("|").append(m.size()); return s.toString();}
 public static String counted(){Map<String,Integer> m=map(); return m.toString()+"|"+m.keySet()+"|"+m.values()+"|"+m.entrySet();}
}
