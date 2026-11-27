package demo;

import net.minecraft.world.inventory.tooltip.TooltipComponent;

/** A tooltip image's data: how many segments to draw. */
public record Meter(int segments) implements TooltipComponent {}
