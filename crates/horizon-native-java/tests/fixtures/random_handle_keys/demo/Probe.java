package demo;

import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import java.util.function.LongFunction;

/**
 * Seeded {@code java.util.Random}s made by {@code new} and by a constructor reference are
 * distinct objects with their own sequences (#667 review).
 *
 * <p>{@code new Random(seed)} is the host's model, made at the {@code new} opcode; {@code
 * Random::new} reaches the same class through the runtime's allocator. When the two numbered
 * their objects with separate counters, the constructor reference's {@code java/util/Random#0}
 * was the seeded one's key: its {@code <init>(J)} reseeded that object, and both read one
 * sequence ({@code true|-1154715079|1260042744} instead of Java's answer).
 *
 * <p>{@code main} is the Java 25 oracle: {@code java -cp . demo.Probe}.
 */
public final class Probe {
  public static String run() {
    Random seeded = new Random(1L);
    LongFunction<Random> make = Random::new;
    Random referenced = make.apply(2L);
    StringBuilder out = new StringBuilder();
    out.append(seeded == referenced)
        .append('|')
        .append(seeded.nextInt())
        .append('|')
        .append(referenced.nextInt());
    // Both kinds interleaved, all made before any draws: none may share another's state.
    List<Random> all = new ArrayList<>();
    for (int i = 0; i < 4; i++) {
      all.add(new Random(10L + i));
      all.add(make.apply(20L + i));
    }
    for (Random random : all) {
      out.append('|').append(random.nextInt(1000));
    }
    return out.toString();
  }

  public static void main(String[] args) {
    System.out.println(run());
  }
}
