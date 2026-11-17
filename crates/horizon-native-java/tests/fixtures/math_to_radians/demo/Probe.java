package demo;
public final class Probe {
  public static String rows() {
    double sub = Double.MIN_VALUE;
    return String.join("|", bits(Math.toRadians(0.0)), bits(Math.toRadians(-0.0)), bits(Math.toRadians(180.0)), bits(Math.toRadians(1e308)), Boolean.toString(Double.isNaN(Math.toRadians(Double.NaN))), bits(Math.toRadians(Double.POSITIVE_INFINITY)), bits(Math.toRadians(Double.NEGATIVE_INFINITY)), bits(Math.toRadians(sub)));
  }
  public static String nanBits() { return bits(Math.toRadians(Double.NaN)); }
  private static String bits(double x) { return Long.toHexString(Double.doubleToRawLongBits(x)); }
  public static void main(String[] args) { System.out.println(rows()); }
}
