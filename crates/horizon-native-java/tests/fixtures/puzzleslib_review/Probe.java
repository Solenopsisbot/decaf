package demo;
import java.util.*;
import com.google.common.collect.*;
import net.fabricmc.fabric.api.event.Event;
import net.fabricmc.fabric.api.biome.v1.ModificationPhase;
public final class Probe {
  public static String queue() {
    java.util.concurrent.ConcurrentLinkedQueue<String> q=Queues.newConcurrentLinkedQueue();
    String a; try { q.offer(null); a="ok"; } catch (NullPointerException e) { a="npe"; }
    java.util.LinkedList<String> l=new java.util.LinkedList<>(); boolean accepted=l.offer(null);
    q.offer("x"); return a+":"+accepted+":"+q.poll();
  }
  public static String immutable() {
    ImmutableMap.Builder<String,Integer> b=ImmutableMap.builder();
    try { b.put("a",1).put("a",2).build(); return "ok"; }
    catch (IllegalArgumentException e) {
      String m=e.getMessage(); b=ImmutableMap.<String,Integer>builder().put("a",1);
      java.util.Map<String,Integer> map=b.build(); String u;
      try { map.put("b",2); u="ok"; } catch (UnsupportedOperationException x) { u="uoe"; }
      return "iae:"+m+"|"+u+":"+map.size();
    }
  }
  enum P { Z, A, M }
  public static String enumMap() {
    EnumMap<P,Integer> src=new EnumMap<>(P.class); src.put(P.M,1); src.put(P.Z,2); src.put(P.A,3);
    java.util.Map<P,Integer> out=Maps.immutableEnumMap(src); StringBuilder s=new StringBuilder();
    for (P p: out.keySet()) s.append(p.name()); return s.toString();
  }
  public static String phases() {
    boolean a=Event.DEFAULT_PHASE==Event.DEFAULT_PHASE;
    boolean b=ModificationPhase.ADDITIONS==ModificationPhase.ADDITIONS;
    boolean c=ModificationPhase.ADDITIONS != (Object)"fabric-modification-phase/ADDITIONS";
    return a+"|"+b+"|"+c;
  }
}
