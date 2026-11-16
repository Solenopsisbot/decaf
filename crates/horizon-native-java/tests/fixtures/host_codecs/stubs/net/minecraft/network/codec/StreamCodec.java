package net.minecraft.network.codec;

// Compile-only stub of Minecraft 26.3's interface.
public interface StreamCodec<B, V> {
    static <B, V> StreamCodec<B, V> unit(V value) { return null; }
    default V decode(B input) { return null; }
    default void encode(B output, V value) {}
}
