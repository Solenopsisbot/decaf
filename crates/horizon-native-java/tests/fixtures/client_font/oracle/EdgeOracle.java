import net.minecraft.util.ARGB;
import net.minecraft.util.Mth;

/** ARGB.white(float) and Mth.ceil(float) of the rows the Decaf unit tests assert. */
public final class EdgeOracle {
  public static void main(String[] args) {
    float[] alphas = {0.5f, 1.0f, 0.0f, -1e10f, Float.NEGATIVE_INFINITY, -9e6f, 2.0f, Float.NaN};
    for (float a : alphas) {
      System.out.println("white " + a + " " + String.format("%08x", ARGB.white(a)));
    }
    float[] ceils = {38.5f, 39f, 3e9f, Float.POSITIVE_INFINITY, Float.NaN, -0.5f, -3e9f};
    for (float c : ceils) {
      System.out.println("ceil " + c + " " + Mth.ceil(c));
    }
  }
}
