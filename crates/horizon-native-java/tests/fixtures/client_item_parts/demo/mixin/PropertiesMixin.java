package demo.mixin;

import com.mojang.serialization.MapCodec;
import demo.FillProperty;
import net.minecraft.client.renderer.item.properties.numeric.RangeSelectItemModelProperties;
import net.minecraft.client.renderer.item.properties.numeric.RangeSelectItemModelProperty;
import net.minecraft.resources.Identifier;
import net.minecraft.util.ExtraCodecs;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Adds `demo:fill` to the numeric item model properties, at the end of their bootstrap. */
@Mixin(RangeSelectItemModelProperties.class)
public abstract class PropertiesMixin {
  @Shadow @Final
  private static ExtraCodecs.LateBoundIdMapper<
          Identifier, MapCodec<? extends RangeSelectItemModelProperty>>
      ID_MAPPER;

  @Inject(method = "bootstrap", at = @At("TAIL"))
  private static void demo$bootstrap(CallbackInfo ci) {
    ID_MAPPER.put(Identifier.fromNamespaceAndPath("demo", "fill"), FillProperty.CODEC);
  }
}
