package demo;

import net.minecraft.util.RandomSource;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.PositionalRandomFactory;

/**
 * Every `RandomSource` call Decaf answers on a level's lent `Level.random`, in a fixed order.
 * `all(random, again)` is what the oracle runs on a real `LegacyRandomSource`; `run(level)` is
 * the same on `level.getRandom()`, which is what Decaf runs.
 */
public final class LevelRandoms {
  private static RandomSource kept;

  private LevelRandoms() {}

  public static String run(Level level) {
    return all(level.getRandom(), level.getRandom());
  }

  public static String all(RandomSource random, RandomSource again) {
    return draws(random, again) + ";" + errors(random);
  }

  static String draws(RandomSource random, RandomSource again) {
    StringBuilder out = new StringBuilder(random == again ? "same" : "different");
    out.append('|').append(random.nextInt());
    for (int bound : new int[] {1, 2, 7, 16, 100, 1 << 30, Integer.MAX_VALUE}) {
      out.append('|').append(random.nextInt(bound));
    }
    out.append('|').append(random.nextInt(-5, 5));
    out.append('|').append(random.nextInt(Integer.MIN_VALUE, -1));
    out.append('|').append(random.nextIntBetweenInclusive(3, 9));
    out.append('|').append(random.nextIntBetweenInclusive(-4, -4));
    out.append('|').append(random.nextLong());
    out.append('|').append(random.nextBoolean());
    out.append('|').append(random.nextFloat());
    out.append('|').append(random.nextDouble());
    out.append('|').append(random.nextGaussian());
    out.append('|').append(random.nextGaussian());
    out.append('|').append(random.triangle(10.0, 2.5));
    out.append('|').append(random.triangle(1.0f, 0.5f));
    random.consumeCount(3);
    random.consumeCount(0);
    random.consumeCount(-2);
    out.append('|').append(random.nextInt());
    RandomSource fork = random.fork();
    out.append('|').append(fork.nextLong()).append('|').append(fork.nextInt(10));
    out.append('|').append(random.nextInt());
    PositionalRandomFactory positional = random.forkPositional();
    out.append('|').append(positional.at(1, 64, -3).nextInt());
    out.append('|').append(positional.fromSeed(99L).nextLong());
    out.append('|').append(random.nextInt());
    return out.toString();
  }

  /** Java's argument checks throw before any draw: the stream is where it was afterwards. */
  static String errors(RandomSource random) {
    StringBuilder out = new StringBuilder();
    for (int i = 0; i < 7; i++) {
      try {
        switch (i) {
          case 0 -> random.nextInt(0);
          case 1 -> random.nextInt(-3);
          case 2 -> random.nextInt(5, 5);
          case 3 -> random.nextInt(6, 2);
          case 4 -> random.nextInt(Integer.MIN_VALUE, Integer.MAX_VALUE);
          case 5 -> random.nextIntBetweenInclusive(5, 4);
          default -> random.nextIntBetweenInclusive(0, Integer.MAX_VALUE);
        }
        out.append("no throw|");
      } catch (IllegalArgumentException e) {
        out.append(e.getClass().getName()).append(':').append(e.getMessage()).append('|');
      }
    }
    return out.append(random.nextInt()).toString();
  }

  /** Keep the level's handle and draw from it. */
  public static int keep(Level level) {
    kept = level.getRandom();
    return kept.nextInt();
  }

  /** Draw from the kept handle: with no lend, nothing answers it. */
  public static int drawKept() {
    return kept.nextInt();
  }

  /** The kept handle is the object `getRandom()` answers in a later call. */
  public static boolean keptIsCurrent(Level level) {
    return kept == level.getRandom();
  }

  /** `consumeCount(count)`, then one draw. */
  public static int consume(Level level, int count) {
    RandomSource random = level.getRandom();
    random.consumeCount(count);
    return random.nextInt();
  }

  /** The level's stream can't be reseeded: `setSeed` is counted, and the next draw goes on. */
  public static int reseed(Level level) {
    RandomSource random = level.getRandom();
    random.setSeed(42L);
    return random.nextInt();
  }
}
