package com.mojang.serialization;

public interface DataResult<A> {
  static <R> DataResult<R> success(R value) { throw new AssertionError(); }
  java.util.Optional<A> result();
  boolean isSuccess();
}
