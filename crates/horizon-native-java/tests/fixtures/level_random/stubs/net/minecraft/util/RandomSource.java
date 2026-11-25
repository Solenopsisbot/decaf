package net.minecraft.util;
import net.minecraft.world.level.levelgen.PositionalRandomFactory;
// Compile-only stub: the method set and descriptors of 26.3's `RandomSource` (javap).
public interface RandomSource {
  RandomSource fork();
  PositionalRandomFactory forkPositional();
  void setSeed(long seed);
  int nextInt();
  int nextInt(int bound);
  default int nextIntBetweenInclusive(int min, int max) { return 0; }
  long nextLong();
  boolean nextBoolean();
  float nextFloat();
  double nextDouble();
  double nextGaussian();
  default double triangle(double mean, double spread) { return 0; }
  default float triangle(float mean, float spread) { return 0; }
  default void consumeCount(int count) {}
  default int nextInt(int origin, int bound) { return 0; }
}
