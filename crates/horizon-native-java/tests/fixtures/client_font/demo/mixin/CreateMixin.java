package demo.mixin;

import demo.Label;
import demo.LabelTooltip;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipComponent;
import net.minecraft.world.inventory.tooltip.TooltipComponent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Maps a {@link Label} to its client component, before vanilla's `create` refuses it. */
@Mixin(ClientTooltipComponent.class)
public interface CreateMixin {
  @Inject(
      method =
          "create(Lnet/minecraft/world/inventory/tooltip/TooltipComponent;)Lnet/minecraft/client/gui/screens/inventory/tooltip/ClientTooltipComponent;",
      at = @At("HEAD"),
      cancellable = true)
  private static void demo$create(
      TooltipComponent data, CallbackInfoReturnable<ClientTooltipComponent> cir) {
    if (data instanceof Label label) {
      cir.setReturnValue(new LabelTooltip(label));
    }
  }
}
