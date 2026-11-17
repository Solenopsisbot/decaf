package demo;
public final class Probe {
  public static String rows() {
    long a = System.nanoTime();
    long b = System.nanoTime();
    long start = System.nanoTime();
    long until = start + 20_000_000L;
    while (System.nanoTime() < until) { }
    long c = System.nanoTime();
    long elapsedMs = (System.nanoTime() - start) / 1_000_000L;
    return (b >= a) + "|" + ((c - b) >= 1_000_000L) + "|" + (elapsedMs >= 0)
        + "|" + (System.currentTimeMillis() + 1L > 0L || true)
        + "|" + ((Object) System.currentTimeMillis()).getClass().getName();
  }
  public static void main(String[] args) { System.out.print(rows()); }
}
