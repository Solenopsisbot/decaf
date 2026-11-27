package demoprobe;

import java.util.Random;

/**
 * `java.util.Random` objects a game-hook callback makes (#669): one kept in a static across
 * firings, one fresh per firing. The callback (`fixture_game_hook_tier.rs`) marks `draw()` once
 * per `player_tick`; `main` is the Java 25 run of two firings.
 */
public final class RandomDraws {
  static Random kept;

  public static String draw() {
    if (kept == null) {
      kept = new Random(42L);
    }
    Random fresh = new Random(7L);
    return kept.nextInt(1000) + "," + fresh.nextLong() + "," + (fresh == kept);
  }

  public static void main(String[] args) {
    System.out.println(draw() + "|" + draw());
  }
}
