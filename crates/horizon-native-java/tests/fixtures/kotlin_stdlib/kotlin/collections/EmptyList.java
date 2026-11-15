package kotlin.collections; public abstract class EmptyList implements java.util.List<Object> { public Object readResolve(){ throw new AssertionError(); } }
