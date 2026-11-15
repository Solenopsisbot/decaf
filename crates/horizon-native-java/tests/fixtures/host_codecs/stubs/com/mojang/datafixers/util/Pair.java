package com.mojang.datafixers.util;

// Compile-only stub of DataFixerUpper 10.0.21's class (the oracle runs the real one).
public class Pair<F, S> implements com.mojang.datafixers.kinds.App<Object, F> {
    public Pair(F first, S second) {}

    public static <F, S> Pair<F, S> of(F first, S second) { return null; }

    public F getFirst() { return null; }

    public S getSecond() { return null; }

    public Pair<S, F> swap() { return null; }
    public <F2> Pair<F2, S> mapFirst(java.util.function.Function<? super F, ? extends F2> f) { return null; }
    public <S2> Pair<F, S2> mapSecond(java.util.function.Function<? super S, ? extends S2> f) { return null; }
}
