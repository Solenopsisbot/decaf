import demo.LevelRandoms;
import net.minecraft.world.level.levelgen.LegacyRandomSource;

/** `LevelRandoms.all` on a real 26.3 `LegacyRandomSource`, which `Level.random` is. */
public final class LevelRandomsOracle {
  public static void main(String[] args) {
    long seed = Long.parseLong(args[0]);
    LegacyRandomSource random = new LegacyRandomSource(seed);
    System.out.println(LevelRandoms.all(random, random));
  }
}
