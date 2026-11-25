package demo;

import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.SemanticVersion;
import net.fabricmc.loader.api.Version;

/**
 * What a mod reads off a Fabric `Version`: whether it is a `SemanticVersion`, and then every
 * `SemanticVersion` accessor. The same `describe` runs on Java 25 against the real Fabric Loader
 * (`oracle/OracleMain.java` hands it `Version.parse(text)`) and in Decaf (a jar whose
 * `fabric.mod.json` carries the version, read through `ModMetadata.getVersion()`).
 */
public final class Probe {
    private Probe() {}

    /** Decaf: the running mod's own version, read the way a mod reads it. */
    public static String mod() {
        return describe(FabricLoader.getInstance().getModContainer("probe-fixture").orElseThrow()
                .getMetadata().getVersion());
    }

    /** Decaf: the built-in `minecraft` mod's version (the game version the host was seeded with). */
    public static String minecraft() {
        return describe(FabricLoader.getInstance().getModContainer("minecraft").orElseThrow()
                .getMetadata().getVersion());
    }

    /** Decaf: the built-in `java` mod's version. */
    public static String java() {
        return describe(FabricLoader.getInstance().getModContainer("java").orElseThrow()
                .getMetadata().getVersion());
    }

    /** Decaf: `compareTo`, which no oracle row covers (counted). */
    public static String compared() {
        Version version = FabricLoader.getInstance().getModContainer("probe-fixture").orElseThrow()
                .getMetadata().getVersion();
        return String.valueOf(version.compareTo(version));
    }

    /** Decaf: `getVersionComponentCount` through a cast, which only a `SemanticVersion` answers. */
    public static String count() {
        Object version = FabricLoader.getInstance().getModContainer("probe-fixture").orElseThrow()
                .getMetadata().getVersion();
        return String.valueOf(((SemanticVersion) version).getVersionComponentCount());
    }

    public static String describe(Version version) {
        StringBuilder out = new StringBuilder();
        boolean semantic = version instanceof SemanticVersion;
        out.append("instanceof=").append(semantic);
        out.append(" version=").append(version instanceof Version);
        out.append(" comparable=").append(version instanceof Comparable);
        out.append(" friendly=").append(version.getFriendlyString());
        if (!semantic) {
            return out.toString();
        }
        SemanticVersion sv = (SemanticVersion) version;
        int count = sv.getVersionComponentCount();
        out.append(" count=").append(count);
        out.append(" components=");
        for (int i = 0; i < count; i++) {
            out.append(i == 0 ? "" : ",").append(sv.getVersionComponent(i));
        }
        out.append(" at-count=").append(component(sv, count));
        out.append(" at-count+1=").append(component(sv, count + 1));
        out.append(" at-count+7=").append(component(sv, count + 7));
        out.append(" at-max=").append(component(sv, Integer.MAX_VALUE));
        out.append(" at-minus1=").append(component(sv, -1));
        out.append(" at-min=").append(component(sv, Integer.MIN_VALUE));
        out.append(" prerelease=").append(sv.getPrereleaseKey());
        out.append(" build=").append(sv.getBuildKey());
        out.append(" wildcard=").append(sv.hasWildcard());
        out.append(" toString=").append(sv.toString());
        return out.toString();
    }

    private static String component(SemanticVersion sv, int index) {
        try {
            return String.valueOf(sv.getVersionComponent(index));
        } catch (RuntimeException e) {
            return e.getClass().getName() + ": " + e.getMessage();
        }
    }
}
