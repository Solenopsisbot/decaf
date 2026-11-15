package com.mojang.datafixers.util;

public abstract class Either<L, R> {
    public static <L, R> Either<L, R> left(L value) { return null; }
    public static <L, R> Either<L, R> right(R value) { return null; }
    public abstract <T> T map(java.util.function.Function<? super L, ? extends T> left,
                              java.util.function.Function<? super R, ? extends T> right);
    public abstract <C, D> Either<C, D> mapBoth(java.util.function.Function<? super L, ? extends C> left,
                                                java.util.function.Function<? super R, ? extends D> right);
    public abstract Either<L, R> ifLeft(java.util.function.Consumer<? super L> consumer);
    public abstract Either<L, R> ifRight(java.util.function.Consumer<? super R> consumer);
    public abstract java.util.Optional<L> left();
    public abstract java.util.Optional<R> right();
    public Object orThrow() { return null; }
    public Either<R, L> swap() { return null; }
    public <T> Either<T, R> mapLeft(java.util.function.Function<? super L, ? extends T> f) { return null; }
    public <T> Either<L, T> mapRight(java.util.function.Function<? super R, ? extends T> f) { return null; }
}
