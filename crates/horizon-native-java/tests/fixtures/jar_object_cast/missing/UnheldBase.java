package missing;

/**
 * A superclass the probe jar leaves out (only `demo/` is packed): Horizon cannot see that it extends `Number`
 * (#267 review), so a cast of its subclass to a non-final JDK class is counted, never answered.
 */
public class UnheldBase extends Number {
    @Override
    public int intValue() {
        return 0;
    }

    @Override
    public long longValue() {
        return 0;
    }

    @Override
    public float floatValue() {
        return 0;
    }

    @Override
    public double doubleValue() {
        return 0;
    }
}
