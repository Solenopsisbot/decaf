package demo;

import net.fabricmc.api.ModInitializer;

public final class Throwing implements ModInitializer {
    public Throwing() {
        throw new IllegalStateException("ctor boom");
    }

    @Override
    public void onInitialize() {
        // The constructor fails before this method can run.
    }
}
