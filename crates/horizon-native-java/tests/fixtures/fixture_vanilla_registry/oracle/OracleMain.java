package oracle;

import demo.Probe;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;

public final class OracleMain {
    public static void main(String[] args) {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.out.println(Probe.rows());
    }
}
