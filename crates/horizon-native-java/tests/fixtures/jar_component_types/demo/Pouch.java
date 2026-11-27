package demo;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import java.util.List;

/**
 * A component value shaped like Bundle API's {@code CustomBundleContentsComponent}: a required list
 * (through an {@code xmap}, as its {@code Content}) and an optional int whose default is left out.
 */
public record Pouch(Names names, int multiplier) {
  /** The list wrapper, as {@code CustomBundleContentsComponent.Content} wraps its templates. */
  public record Names(List<String> items) {
    public static final Codec<Names> CODEC =
        Codec.STRING.listOf().xmap(Names::new, Names::items);
  }

  public static final Codec<Pouch> CODEC =
      RecordCodecBuilder.create(
          i ->
              i.group(
                      Names.CODEC.fieldOf("content").forGetter(Pouch::names),
                      Codec.INT.optionalFieldOf("size_multiplier", 1).forGetter(Pouch::multiplier))
                  .apply(i, Pouch::new));
}
