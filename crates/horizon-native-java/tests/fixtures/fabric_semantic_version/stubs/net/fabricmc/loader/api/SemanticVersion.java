package net.fabricmc.loader.api;
public interface SemanticVersion extends Version {
    int getVersionComponentCount();
    int getVersionComponent(int index);
    java.util.Optional<String> getPrereleaseKey();
    java.util.Optional<String> getBuildKey();
    boolean hasWildcard();
}
