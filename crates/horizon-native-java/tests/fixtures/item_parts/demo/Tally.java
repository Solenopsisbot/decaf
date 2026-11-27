package demo;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;

/** A jar data component: how many items a tally item took in, and the last one's id. */
public record Tally(int count, String last) {
  public static final Codec<Tally> CODEC =
      RecordCodecBuilder.create(
          i ->
              i.group(
                      Codec.INT.fieldOf("count").forGetter(Tally::count),
                      Codec.STRING.optionalFieldOf("last", "").forGetter(Tally::last))
                  .apply(i, Tally::new));
}
