package demo;
import java.util.*;
public final class Probe {
 static String t(Object x){try{return String.valueOf(x);}catch(Throwable e){return e.getClass().getName();}}
 static String held(Map<String,Integer> m){m.put("a",1); Map.Entry<String,Integer> e=m.entrySet().iterator().next(); m.remove("a"); return t(e.getValue())+"|"+t(e);}
 static String re(Map<String,Integer> m){m.put("a",1); Map.Entry<String,Integer> e=m.entrySet().iterator().next(); m.remove("a"); m.put("a",5); return t(e.getValue())+"|"+t(e);}
 static String it(Map<String,Integer> m){m.put("a",1); Iterator<Map.Entry<String,Integer>> i=m.entrySet().iterator(); Map.Entry<String,Integer> e=i.next(); i.remove(); return t(e.getValue())+"|"+t(e);}
 static String keys(){Hashtable<String,Integer> h=new Hashtable<>();h.put("a",1);Enumeration<String> k=h.keys(); try{k.nextElement();((Iterator)k).remove();return "removed";}catch(Throwable e){return e.getClass().getName();}}
 static String elems(){Hashtable<String,Integer> h=new Hashtable<>();h.put("a",1);Enumeration<Integer> k=h.elements(); try{k.nextElement();((Iterator)k).remove();return "removed";}catch(Throwable e){return e.getClass().getName();}}
 public static String rows(){return "ht="+held(new Hashtable<>())+"|"+re(new Hashtable<>())+"|"+it(new Hashtable<>())+";props="+held((Map)new Properties())+"|"+re((Map)new Properties())+"|"+it((Map)new Properties())+";hm="+held(new HashMap<>())+"|"+re(new HashMap<>())+"|"+it(new HashMap<>())+";keys="+keys()+";elems="+elems();}
 public static void main(String[]a){System.out.println(rows());}
}
