package demo;

import com.google.gson.Gson;
import com.google.gson.reflect.TypeToken;
import java.util.HashMap;
import java.util.Map;
import java.util.function.Supplier;

/** Map.merge on maps Gson reads (#337): one row per check, each in its own try/catch. */
public class Probe {
  private static String row(Supplier<Object> body) {
    try {
      return String.valueOf(body.get());
    } catch (Throwable t) {
      return t.getClass().getName() + ":" + t.getMessage();
    }
  }

  @SuppressWarnings({"rawtypes", "unchecked"})
  public static String rows() {
    StringBuilder s = new StringBuilder();
    Map m = new Gson().fromJson("{\"a\":1,\"b\":2}", Map.class);
    s.append("valueClass=").append(row(() -> m.get("a").getClass().getName())).append('\n');
    s.append("mergePresent=")
        .append(row(() -> m.merge("a", 10.0, (x, y) -> ((Double) x) + ((Double) y))))
        .append('\n');
    s.append("mergeAbsent=")
        .append(row(() -> m.merge("z", 5.0, (x, y) -> ((Double) x) + ((Double) y))))
        .append('\n');
    s.append("mergeToNull=")
        .append(row(() -> m.merge("b", 1.0, (x, y) -> null) + " " + m.containsKey("b")))
        .append('\n');
    s.append("mergeNullValue=").append(row(() -> m.merge("a", null, (x, y) -> x))).append('\n');
    s.append("after=").append(row(() -> m)).append('\n');
    Map<String, Integer> typed =
        new Gson().fromJson("{\"a\":1}", new TypeToken<Map<String, Integer>>() {}.getType());
    s.append("typed=")
        .append(row(() -> typed.merge("a", 5, Integer::sum) + " " + typed))
        .append('\n');
    Map<String, Object> plain = new HashMap<>();
    plain.put("a", 2);
    s.append("hashMap=").append(row(() -> plain.merge("a", 10, (x, y) -> "m") + " " + plain));
    return s.toString();
  }

  public static void main(String[] args) {
    System.out.print(rows());
  }
}
