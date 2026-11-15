package demo;
import com.google.common.collect.Lists;
import com.google.common.collect.Maps;
import com.google.common.collect.Sets;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentMap;
public final class Probe {
  public static String identity() {
    String a = new String("x");
    String b = new String("x");
    Map<String,Integer> map = Maps.newIdentityHashMap();
    map.put(a, 1); map.put(b, 2);
    Set<String> set = Sets.newIdentityHashSet();
    set.add(a); set.add(b);
    return map.size() + ":" + map.get(new String("x")) + ":" + set.size() + ":" + set.contains(new String("x"));
  }
  public static String concurrent() {
    ConcurrentMap<String,Integer> map = Maps.newConcurrentMap();
    map.put("x", 1);
    String error;
    try { map.put(null, 2); error = "no"; }
    catch (NullPointerException e) { error = "npe"; }
    return map.get("x") + ":" + error + ":" + map.size();
  }
  public static String capacities() {
    StringBuilder out = new StringBuilder();
    try { Maps.newHashMapWithExpectedSize(-1); out.append("no"); }
    catch (IllegalArgumentException e) { out.append(e.getMessage()); }
    out.append('|');
    try { Lists.newArrayListWithCapacity(-1); out.append("no"); }
    catch (IllegalArgumentException e) { out.append(e.getMessage()); }
    out.append('|');
    try { Lists.newArrayListWithExpectedSize(-1); out.append("no"); }
    catch (IllegalArgumentException e) { out.append(e.getMessage()); }
    return out.toString();
  }
}
