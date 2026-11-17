package demo;
import java.util.concurrent.atomic.AtomicBoolean;
public class Probe {
  public static String rows() {
    AtomicBoolean a = new AtomicBoolean();
    AtomicBoolean b = new AtomicBoolean(true);
    StringBuilder s = new StringBuilder();
    s.append(a.get()).append('|').append(b.get()).append('|');
    a.set(true); s.append(a.get()).append('|');
    a.lazySet(false); s.append(a.get()).append('|');
    s.append(a.compareAndSet(false, true)).append(':').append(a.get()).append('|');
    s.append(a.compareAndSet(false, true)).append(':').append(a.get()).append('|');
    s.append(a.getAndSet(false)).append(':').append(a.get()).append('|');
    s.append(a.toString()).append('|');
    s.append(new AtomicBoolean().getPlain()).append(':').append(new AtomicBoolean(true).getAcquire());
    return s.toString();
  }
  public static void main(String[] args) { System.out.print(rows()); }
}
