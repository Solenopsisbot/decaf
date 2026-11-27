package demo;

import net.minecraft.world.inventory.tooltip.TooltipComponent;

/** A tooltip image's data: which text the label wraps (see {@link LabelTooltip#text}). */
public record Label(int kind) implements TooltipComponent {}
