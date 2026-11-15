package demo;

import java.util.*;
import kotlin.Lazy;
import kotlin.LazyKt;
import kotlin.enums.EnumEntries;
import kotlin.enums.EnumEntriesKt;
import kotlin.jvm.functions.Function0;
import kotlin.jvm.internal.Intrinsics;
import kotlin.collections.CollectionsKt;
import kotlin.collections.MapsKt;
import kotlin.collections.SetsKt;

public final class Probe {
    enum E { A, B }
    private static Lazy<String> kept;
    private static int keepCalls;
    private static Lazy<String> retry;
    private static int retryCalls;

    private static String exception(int which) {
        try {
            switch (which) {
                case 0: Intrinsics.checkNotNull(null); break;
                case 1: Intrinsics.checkNotNull(null, "hello"); break;
                case 2: Intrinsics.checkNotNullParameter(null, "arg"); break;
                case 3: Intrinsics.checkParameterIsNotNull(null, "arg"); break;
                case 4: Intrinsics.checkNotNullExpressionValue(null, "expr"); break;
                case 5: Intrinsics.checkExpressionValueIsNotNull(null, "expr"); break;
                case 6: Intrinsics.throwUninitializedPropertyAccessException("foo"); break;
                default: return "bad";
            }
            return "ok";
        } catch (Throwable t) {
            return t.getClass().getName() + "|" + t.getMessage();
        }
    }

    public static String enumEntries() {
        EnumEntries<E> a = EnumEntriesKt.enumEntries(E.values());
        EnumEntries<E> b = EnumEntriesKt.enumEntries(E.values());
        return a.getClass().getName() + "|" + a.size() + "|" + a.get(0).name() + a.get(1).name()
            + "|" + (a == b) + "|" + a.equals(b) + "|" + exceptionAdd(a) + "|" + a.toString();
    }

    private static String exceptionAdd(EnumEntries<E> a) {
        try { a.add(E.A); return "ok"; }
        catch (Throwable t) { return t.getClass().getName() + "|" + t.getMessage(); }
    }

    public static String intrinsics() {
        Intrinsics.checkNotNull("ok");
        Intrinsics.checkNotNull("ok", "message");
        Intrinsics.checkNotNullParameter("ok", "arg");
        Intrinsics.checkParameterIsNotNull("ok", "arg");
        Intrinsics.checkNotNullExpressionValue("ok", "expr");
        Intrinsics.checkExpressionValueIsNotNull("ok", "expr");
        return Intrinsics.areEqual((Object)null, (Object)null) + "|"
            + Intrinsics.areEqual((Object)null, (Object)"x") + "|"
            + Intrinsics.areEqual((Object)"x", (Object)"x") + "|"
            + exception(0) + "|" + exception(1) + "|" + exception(2) + "|" + exception(3)
            + "|" + exception(4) + "|" + exception(5) + "|" + exception(6);
    }

    public static String lazy() {
        final int[] calls = { 0 };
        Lazy<String> value = LazyKt.lazy(new Function0<String>() {
            public String invoke() { calls[0]++; return "v"; }
        });
        String before = value.toString();
        boolean initialized = value.isInitialized();
        String first = value.getValue();
        String second = value.getValue();
        return before + "|" + initialized + "|" + first + "|" + second + "|" + calls[0]
            + "|" + value.isInitialized() + "|" + value.toString();
    }

    public static String crossCollectionEquality() {
        List<Object> kotlinList = CollectionsKt.emptyList();
        List<Object> jdkList = java.util.List.of();
        Set<Object> kotlinSet = SetsKt.emptySet();
        Set<Object> jdkSet = java.util.Set.of();
        Map<Object,Object> kotlinMap = MapsKt.emptyMap();
        Map<Object,Object> jdkMap = java.util.Map.of();
        EnumEntries<E> entries = EnumEntriesKt.enumEntries(E.values());
        List<E> ordinary = java.util.Arrays.asList(E.values());
        return kotlinList.equals(jdkList) + "|" + jdkList.equals(kotlinList)
            + "|" + kotlinSet.equals(jdkSet) + "|" + jdkSet.equals(kotlinSet)
            + "|" + kotlinMap.equals(jdkMap) + "|" + jdkMap.equals(kotlinMap)
            + "|" + entries.equals(ordinary) + "|" + kotlinList.hashCode()
            + "|" + kotlinSet.hashCode() + "|" + kotlinMap.hashCode();
    }

    public static String lazyOf() {
        Lazy<String> value = LazyKt.lazyOf("x");
        return value.getClass().getName() + "|" + value.isInitialized() + "|"
            + value.toString() + "|" + value.getValue();
    }

    public static String lazyAcrossCallbacks() {
        kept = LazyKt.lazy(new Function0<String>() {
            public String invoke() { keepCalls++; return "kept"; }
        });
        return kept.toString() + "|" + kept.isInitialized();
    }

    public static String lazyAfterCollection() {
        for (int i = 0; i < 70000; i++) { Object[] trash = new Object[1]; }
        return kept.getValue() + "|" + keepCalls + "|" + kept.isInitialized();
    }

    public static String lazyRetriesAfterThrow() {
        retry = LazyKt.lazy(new Function0<String>() {
            public String invoke() {
                retryCalls++;
                if (retryCalls == 1) throw new IllegalStateException("boom");
                return "retry";
            }
        });
        String first;
        try { retry.getValue(); first = "ok"; }
        catch (Throwable t) { first = t.getClass().getName() + "|" + t.getMessage(); }
        return first + "|" + retry.getValue() + "|" + retryCalls + "|" + retry.isInitialized();
    }

    public static String droppedLazy() {
        Lazy<String> value = LazyKt.lazy(new Function0<String>() {
            public String invoke() { return "dropped"; }
        });
        return value.getValue();
    }

    public static String forceCollectionOnly() {
        for (int i = 0; i < 70000; i++) { Object[] trash = new Object[1]; }
        return "forced";
    }

    public static String readResolveIdentity() {
        List<Object> list = CollectionsKt.emptyList();
        return Boolean.toString(
            ((kotlin.collections.EmptyList) (Object) list).readResolve() == (Object) list
        );
    }

    public static String collections() {
        List<Object> list = CollectionsKt.emptyList();
        List<Object> list2 = CollectionsKt.listOf();
        List<Object> mutable = CollectionsKt.mutableListOf();
        Set<Object> set = SetsKt.emptySet();
        Map<Object,Object> map = MapsKt.emptyMap();
        return list.getClass().getName() + "|" + (list == list2) + "|" + (list == CollectionsKt.emptyList())
            + "|" + mutable.getClass().getName() + "|" + set.getClass().getName() + "|" + map.getClass().getName()
            + "|" + exceptionAddList(list);
    }

    private static String exceptionAddList(List<Object> list) {
        try { list.add("x"); return "ok"; }
        catch (Throwable t) { return t.getClass().getName() + "|" + t.getMessage(); }
    }
}
