package net.fabricmc.loader.api;
public interface FabricLoader {
    static FabricLoader getInstance() { throw new AssertionError(); }
    java.util.Optional<ModContainer> getModContainer(String id);
}
