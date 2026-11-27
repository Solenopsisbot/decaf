package net.fabricmc.api;

/** Compile-only stand-in for Fabric's entrypoint interface (the oracle never runs Probe). */
public interface ModInitializer {
  void onInitialize();
}
