package com.mojang.datafixers;

import com.mojang.datafixers.kinds.App;
import com.mojang.serialization.codecs.RecordCodecBuilder;

public final class Products {
    public static final class P1<O, A> implements App<RecordCodecBuilder.Mu<O>, A> {
        public <R> App<RecordCodecBuilder.Mu<O>, R> apply(RecordCodecBuilder.Instance<O> instance, java.util.function.Function<A, R> constructor) { return null; }
    }
    public static final class P3<O, A, B, C> implements App<RecordCodecBuilder.Mu<O>, Object> {
        public <R> App<RecordCodecBuilder.Mu<O>, R> apply(RecordCodecBuilder.Instance<O> instance, com.mojang.datafixers.util.Function3<A, B, C, R> constructor) { return null; }
    }
    public static final class P8<O, A, B, C, D, E, F, G, H> implements App<RecordCodecBuilder.Mu<O>, Object> {
        public <R> App<RecordCodecBuilder.Mu<O>, R> apply(RecordCodecBuilder.Instance<O> instance, com.mojang.datafixers.util.Function8<A, B, C, D, E, F, G, H, R> constructor) { return null; }
    }
}
