package com.mojang.serialization;

public interface MapCodec<A> {
    static <A> MapCodec<A> unit(A value) { return null; }
    static <A> MapCodec<A> unit(java.util.function.Supplier<A> value) { return null; }
    <O> com.mojang.serialization.codecs.RecordCodecBuilder<O, A> forGetter(java.util.function.Function<O, A> getter);
    <S> MapCodec<S> xmap(java.util.function.Function<? super A, ? extends S> to,
                         java.util.function.Function<? super S, ? extends A> from);
    <S> MapCodec<S> flatXmap(java.util.function.Function<? super A, ? extends DataResult<? extends S>> to,
                             java.util.function.Function<? super S, ? extends DataResult<? extends A>> from);
    Codec<A> codec();
    <T> DataResult<A> decode(DynamicOps<T> ops, MapLike<T> input);
    <T> RecordBuilder<T> encode(A value, DynamicOps<T> ops, RecordBuilder<T> prefix);
    <T> java.util.stream.Stream<T> keys(DynamicOps<T> ops);
}
