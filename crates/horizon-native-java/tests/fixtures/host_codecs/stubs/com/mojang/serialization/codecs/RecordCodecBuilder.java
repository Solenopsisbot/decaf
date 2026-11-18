package com.mojang.serialization.codecs;

import com.mojang.datafixers.kinds.App;
import com.mojang.datafixers.Products;
import com.mojang.serialization.Codec;
import com.mojang.serialization.MapCodec;
import com.mojang.serialization.Lifecycle;

public final class RecordCodecBuilder<O, F> implements App<RecordCodecBuilder.Mu<O>, F> {
    public static final class Mu<O> {}
    public static final class Instance<O> {
        public <A> Products.P1<O, A> group(App<RecordCodecBuilder.Mu<O>, A> a) { return null; }
        public <A, B, C> Products.P3<O, A, B, C> group(App<RecordCodecBuilder.Mu<O>, A> a,
                App<RecordCodecBuilder.Mu<O>, B> b, App<RecordCodecBuilder.Mu<O>, C> c) { return null; }
        public <A, B, C, D, E, F, G, H> Products.P8<O, A, B, C, D, E, F, G, H> group(
                App<RecordCodecBuilder.Mu<O>, A> a, App<RecordCodecBuilder.Mu<O>, B> b,
                App<RecordCodecBuilder.Mu<O>, C> c, App<RecordCodecBuilder.Mu<O>, D> d,
                App<RecordCodecBuilder.Mu<O>, E> e, App<RecordCodecBuilder.Mu<O>, F> f,
                App<RecordCodecBuilder.Mu<O>, G> g, App<RecordCodecBuilder.Mu<O>, H> h) { return null; }
        public <A> App<RecordCodecBuilder.Mu<O>, A> stable(A value) { return null; }
        public <A> App<RecordCodecBuilder.Mu<O>, A> point(A value) { return null; }
        public <A> App<RecordCodecBuilder.Mu<O>, A> point(A value, Lifecycle lifecycle) { return null; }
    }
    public static <O> Instance<O> instance() { return null; }
    public static <O> Codec<O> create(java.util.function.Function<Instance<O>, ? extends App<Mu<O>, O>> f) { return null; }
    public static <O> MapCodec<O> mapCodec(java.util.function.Function<Instance<O>, ? extends App<Mu<O>, O>> f) { return null; }
    public static <O, F> RecordCodecBuilder<O, F> point(F value) { return null; }
    public static <O, F> RecordCodecBuilder<O, F> stable(F value) { return null; }
}
