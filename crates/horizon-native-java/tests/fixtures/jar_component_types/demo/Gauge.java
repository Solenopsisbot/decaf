package demo;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;

/** One field of each other primitive codec, for the tag each becomes under {@code NbtOps}. */
public record Gauge(boolean sealed, long big, double ratio, byte flags, short level, float weight) {
  public static final Codec<Gauge> CODEC =
      RecordCodecBuilder.create(
          i ->
              i.group(
                      Codec.BOOL.fieldOf("sealed").forGetter(Gauge::sealed),
                      Codec.LONG.fieldOf("big").forGetter(Gauge::big),
                      Codec.DOUBLE.fieldOf("ratio").forGetter(Gauge::ratio),
                      Codec.BYTE.fieldOf("flags").forGetter(Gauge::flags),
                      Codec.SHORT.fieldOf("level").forGetter(Gauge::level),
                      Codec.FLOAT.fieldOf("weight").forGetter(Gauge::weight))
                  .apply(i, Gauge::new));
}
