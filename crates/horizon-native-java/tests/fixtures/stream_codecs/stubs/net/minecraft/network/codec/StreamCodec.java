package net.minecraft.network.codec;

import java.util.function.BiFunction;
import java.util.function.Function;

public interface StreamCodec<B, V> extends StreamEncoder<B, V>, StreamDecoder<B, V> {
  static <B, V> StreamCodec<B, V> of(StreamEncoder<B, V> encoder, StreamDecoder<B, V> decoder) {
    return null;
  }
  static <B, V> StreamCodec<B, V> ofMember(StreamMemberEncoder<B, V> encoder, StreamDecoder<B, V> decoder) {
    return null;
  }
  static <B, C, T1> StreamCodec<B, C> composite(
      StreamCodec<? super B, T1> codec1, Function<C, T1> getter1, Function<T1, C> constructor) {
    return null;
  }

  <O> StreamCodec<B, O> map(java.util.function.Function<? super V, ? extends O> a, java.util.function.Function<? super O, ? extends V> b);
  StreamCodec<?, ?> apply(CodecOperation operation);
  StreamCodec<?, ?> dispatch(java.util.function.Function<?, ?> a, java.util.function.Function<?, ?> b);
  interface CodecOperation {}

  static <B, C, T1, T2> StreamCodec<B, C> composite(
      StreamCodec<? super B, T1> codec1, Function<C, T1> getter1,
      StreamCodec<? super B, T2> codec2, Function<C, T2> getter2,
      BiFunction<T1, T2, C> constructor) {
    return null;
  }
}
