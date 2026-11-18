import net.minecraft.SharedConstants;

/** Runs the fixture's own probe on Java 25 with the real 26.3 server jar, after the game's version detection. */
public final class Capture {
    public static void main(String[] a) {
        SharedConstants.tryDetectVersion();
        System.out.print(demo.Probe.rows());
    }
}
