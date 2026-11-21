//! Adversarial identity provenance fixture: every fresh allocation gets one exact identity token,
//! and every ambiguous case (`String.intern()`, loop allocation, whole-string `substring`, Mixin
//! `@ModifyArg` / `@ModifyVariable` reference swaps) is counted instead of guessed.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::{MethodRef, StdHost, Value};
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/adversarial_identity/Probe.java");

#[test]
fn adversarial_identity_review() {
    let fixture = Fixture::build("adversarial-identity-review", &[("demo/Probe.java", PROBE)]);
    // Every expected string is captured from JDK 25.0.5 on `fixtures/adversarial_identity/Probe.java`.
    let cases = [
        ("sameClassCallsOut", "2:-1"),
        ("computeCallsOut", "2:-1"),
        ("classCallsOut", "2"),
        ("recursionOut", "2:-1"),
        ("overloadCallsOut", "2"),
        ("staticInitOut", "2:-1"),
        ("fieldCaseOut", "1:2:2"),
        ("listCaseOut", "1:2:2"),
        ("lambdaCaseOut", "2:-1"),
        ("identityCmpHelperOut", "false"),
        ("lambdaIdentityCmpOut", "false"),
        ("recCmpOut", "false"),
        ("loopAllocOut", "2:-1"),
        ("stringValueOfOut", "1"),
        ("wholeSubstringOut", "true:1"),
        ("callbackMutationOut", "3:2:3:2"),
        ("computeValueOverwriteOut", "false"),
        (
            "dynamicInternOut",
            "A intern self=true|B lookup=1 same=true",
        ),
        // Integer > 127 caching depends on -XX:AutoBoxCacheMax /
        // -Djava.lang.Integer.IntegerCache.high (forwarded via JAVA_TOOL_OPTIONS /
        // _JAVA_OPTIONS / JDK_JAVA_OPTIONS):
        //   JDK 25.0.5 default:               integerAbove127EqOut="false", integerAbove127MapOut="2:null"
        //   JDK 25.0.5 -XX:AutoBoxCacheMax=1000 (and -Djava.lang.Integer.IntegerCache.high=1000):
        //                                     integerAbove127EqOut="true",  integerAbove127MapOut="1:b"
        // Because the JVM flag changes the answer, Decaf counts Integer > 127.
        ("integerAbove127EqOut", "false"),
        ("integerAbove127MapOut", "2:null"),
        // Live Map.Entry.setValue, entrySet().toArray() detached SimpleEntry,
        // replaceAll, and entrySet().iterator().remove() on IdentityHashMap:
        ("entrySetValueOut", "false:v"),
        ("entrySetValueOpaqueOut", "false:v"),
        ("entryToArraySetValueOut", "true:null"),
        ("replaceAllValueOut", "false:v"),
        ("entryIterRemoveOut", "0:null"),
        // Unmodifiable / synchronized chains over IdentityHashMap: entrySet().toArray(IntFunction)
        // hands out detached AbstractMap$SimpleEntry copies (not live nodes), while toArray(T[])
        // keeps UnmodifiableEntry wrappers.
        ("unmodEntryToArrayFnOut", "no:true:null"),
        ("unmodEntryToArrayTypedOut", "uoe:true:null"),
        ("unmodSyncEntryToArrayFnOut", "no:true:null"),
        ("unmodUnmodEntryToArrayFnOut", "no:true:null"),
        // Counted reflective mutators taint every reachable IdentityHashMap so
        // later reads and removals are counted rather than trusting stale contents.
        ("reflectPutThenLookupOut", "false:v"),
        ("reflectPutViaSyncViewOut", "false:v"),
        ("reflectSetValueOnEntryOut", "false:v"),
        ("reflectPutOnEmptyMapOut", "1:v"),
        ("reflectPutThenReadViaUnmodOut", "v:1"),
        // Reviewer's TaintProbe (static map + 600-link deep chain) and
        // MutatingKey.equals via Objects.deepEquals (no method-name allow-list):
        ("staticReachOut", "2"),
        ("deepReachOut", "2"),
        ("mutatingKeyDeepEqualsOut", "false:2"),
        ("reflectStaticUncachedTaintOut", "1"),
        ("reflectDeepChain600TaintOut", "1"),
        // Round-5 findings: exact IdentityHashMap equals/hashCode at every size
        // (including size 0/1 and cross-direction HashMap.equals(IdentityHashMap)),
        // plus derived handles (iterator/stream/filtered stream/spliterator/StreamSupport)
        // invalidated by the identity epoch after a reflective put.
        ("idMapOneEntryDistinctEqOut", "false:false"),
        ("idMapOneEntrySameKDistinctVEqOut", "false"),
        ("idMapOneEntrySameKVEqOut", "true:true:0:0"),
        ("idMapOneEntryHashCodeOut", "false:false:false"),
        (
            "idMapVsHashMapJarKeysOut",
            "true:true|false:true|false:false|false:false|true:true",
        ),
        ("idMapVsHashMapStringKeysOut", "false:false"),
        ("idMapKeySetEntrySetEqOut", "false:true:false:false"),
        ("idMapKeySetHashCodeOut", "true"),
        ("reflectPutThenIteratorOut", "CME"),
        ("reflectPutThenStreamOut", "n=2"),
        ("reflectPutThenFilteredStreamOut", "n=2"),
        ("reflectPutThenSpliteratorOut", "n=2"),
        ("reflectPutThenStreamSupportOut", "n=2"),
        ("reflectPutThenIterSpliteratorOut", "CME"),
        // Round-6 findings: live entries escaped through `entrySet().forEach(...)`
        // (and unmodifiable wrappers), enumerations, and stream iterators resolve
        // their root directly from host/runtime state (no `derived_roots` side map).
        ("forEachSavedEntryToStringOut", "k=2"),
        ("forEachSavedEntryEqualsOut", "false:true:false:true"),
        ("forEachSavedEntryConcatOut", "e=k=2"),
        ("unmodForEachSavedEntryToStringOut", "k=2"),
        ("reflectPutThenEnumerationOut", "true"),
        ("reflectPutThenStreamIteratorOut", "true"),
    ];
    let exact = [
        "sameClassCallsOut",
        "computeCallsOut",
        "classCallsOut",
        "recursionOut",
        "overloadCallsOut",
        "staticInitOut",
        "lambdaCaseOut",
        "stringValueOfOut",
        "callbackMutationOut",
        "entrySetValueOut",
        "entryToArraySetValueOut",
        "entryIterRemoveOut",
        "unmodEntryToArrayFnOut",
        "unmodEntryToArrayTypedOut",
        "unmodSyncEntryToArrayFnOut",
        "unmodUnmodEntryToArrayFnOut",
        "idMapOneEntryDistinctEqOut",
        "idMapOneEntrySameKDistinctVEqOut",
        "idMapOneEntrySameKVEqOut",
        "idMapVsHashMapJarKeysOut",
        "idMapKeySetEntrySetEqOut",
    ];
    let must_count = [
        "loopAllocOut",
        "wholeSubstringOut",
        "computeValueOverwriteOut",
        "dynamicInternOut",
        "integerAbove127EqOut",
        "integerAbove127MapOut",
        "entrySetValueOpaqueOut",
        "replaceAllValueOut",
        "reflectPutThenLookupOut",
        "reflectPutViaSyncViewOut",
        "reflectSetValueOnEntryOut",
        "reflectPutOnEmptyMapOut",
        "reflectPutThenReadViaUnmodOut",
        "staticReachOut",
        "deepReachOut",
        "mutatingKeyDeepEqualsOut",
        "reflectStaticUncachedTaintOut",
        "reflectDeepChain600TaintOut",
        "idMapOneEntryHashCodeOut",
        "idMapVsHashMapStringKeysOut",
        "idMapKeySetHashCodeOut",
        "reflectPutThenIteratorOut",
        "reflectPutThenStreamOut",
        "reflectPutThenFilteredStreamOut",
        "reflectPutThenSpliteratorOut",
        "reflectPutThenStreamSupportOut",
        "reflectPutThenIterSpliteratorOut",
        "forEachSavedEntryToStringOut",
        "forEachSavedEntryEqualsOut",
        "forEachSavedEntryConcatOut",
        "unmodForEachSavedEntryToStringOut",
        "reflectPutThenEnumerationOut",
        "reflectPutThenStreamIteratorOut",
    ];
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let value = fixture
            .jar
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap();
        let actual = match value {
            Value::Ref(Some(s)) => s,
            Value::Ref(None) if must_count.contains(&method) => "null".to_owned(),
            other => panic!("{method}: {other:?}"),
        };
        let counted = host.unknown_host_call_count();
        if exact.contains(&method) {
            assert_eq!(
                counted,
                0,
                "{method}: counted {:?}",
                host.unknown_host_calls()
            );
            assert_eq!(actual, expected, "{method}");
        } else if must_count.contains(&method) {
            assert!(
                counted > 0,
                "{method} answered {actual:?} (JDK 25: {expected:?}) without counting"
            );
            if method == "dynamicInternOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#get(Ljava/lang/Object;)Ljava/lang/Object;"),
                    "dynamicInternOut must count IdentityHashMap#get: {calls:?}"
                );
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/lang/String#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z"),
                    "dynamicInternOut must count String#if_acmp: {calls:?}"
                );
            }
            if method == "integerAbove127EqOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/lang/Object#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z"),
                    "integerAbove127EqOut must count Object#if_acmp: {calls:?}"
                );
            }
            if method == "integerAbove127MapOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
                    "integerAbove127MapOut must count IdentityHashMap#put: {calls:?}"
                );
            }
            if method == "entrySetValueOpaqueOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#remove(Ljava/lang/Object;Ljava/lang/Object;)Z"),
                    "entrySetValueOpaqueOut must count IdentityHashMap#remove(k, v): {calls:?}"
                );
            }
            if method == "replaceAllValueOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#replaceAll(Ljava/util/function/BiFunction;)V"),
                    "replaceAllValueOut must count IdentityHashMap#replaceAll: {calls:?}"
                );
            }
            if matches!(
                method,
                "reflectPutThenLookupOut"
                    | "reflectPutViaSyncViewOut"
                    | "reflectSetValueOnEntryOut"
            ) {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/lang/reflect/Method#invoke(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;"),
                    "{method} must count Method#invoke: {calls:?}"
                );
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#remove(Ljava/lang/Object;Ljava/lang/Object;)Z"),
                    "{method} must taint map and count IdentityHashMap#remove: {calls:?}"
                );
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#get(Ljava/lang/Object;)Ljava/lang/Object;"),
                    "{method} must taint map and count IdentityHashMap#get: {calls:?}"
                );
            }
            if method == "reflectPutOnEmptyMapOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/IdentityHashMap#size()I"),
                    "reflectPutOnEmptyMapOut must taint empty map and count size(): {calls:?}"
                );
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#get(Ljava/lang/Object;)Ljava/lang/Object;"),
                    "reflectPutOnEmptyMapOut must taint empty map and count get(): {calls:?}"
                );
            }
            if method == "reflectPutThenReadViaUnmodOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/Map#get(Ljava/lang/Object;)Ljava/lang/Object;"),
                    "reflectPutThenReadViaUnmodOut must count Map#get via unmodifiable view: {calls:?}"
                );
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/Map#size()I"),
                    "reflectPutThenReadViaUnmodOut must count Map#size via unmodifiable view: {calls:?}"
                );
            }
            if matches!(
                method,
                "staticReachOut"
                    | "deepReachOut"
                    | "reflectStaticUncachedTaintOut"
                    | "reflectDeepChain600TaintOut"
            ) {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#get(Ljava/lang/Object;)Ljava/lang/Object;"),
                    "{method} must bump identity epoch and count IdentityHashMap#get: {calls:?}"
                );
            }
            if method == "mutatingKeyDeepEqualsOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/Objects#deepEquals(Ljava/lang/Object;Ljava/lang/Object;)Z"),
                    "mutatingKeyDeepEqualsOut must count Objects#deepEquals: {calls:?}"
                );
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/IdentityHashMap#get(Ljava/lang/Object;)Ljava/lang/Object;"),
                    "mutatingKeyDeepEqualsOut must bump identity epoch and count IdentityHashMap#get: {calls:?}"
                );
            }
            if method == "idMapOneEntryHashCodeOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/IdentityHashMap#hashCode()I"),
                    "idMapOneEntryHashCodeOut must count IdentityHashMap#hashCode: {calls:?}"
                );
            }
            if method == "idMapVsHashMapStringKeysOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call
                            == "java/util/IdentityHashMap#equals(Ljava/lang/Object;)Z"),
                    "idMapVsHashMapStringKeysOut must count IdentityHashMap#equals: {calls:?}"
                );
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/HashMap#equals(Ljava/lang/Object;)Z"),
                    "idMapVsHashMapStringKeysOut must count HashMap#equals: {calls:?}"
                );
            }
            if method == "idMapKeySetHashCodeOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/Set#hashCode()I"),
                    "idMapKeySetHashCodeOut must count Set#hashCode: {calls:?}"
                );
            }
            if method == "reflectPutThenIteratorOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/Iterator#hasNext()Z"),
                    "reflectPutThenIteratorOut must count Iterator#hasNext after epoch bump: {calls:?}"
                );
            }
            if matches!(
                method,
                "reflectPutThenStreamOut" | "reflectPutThenStreamSupportOut"
            ) {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/stream/Stream#count()J"),
                    "{method} must count Stream#count after epoch bump: {calls:?}"
                );
            }
            if method == "reflectPutThenFilteredStreamOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/stream/Stream#toList()Ljava/util/List;"),
                    "reflectPutThenFilteredStreamOut must count Stream#toList after epoch bump: {calls:?}"
                );
            }
            if matches!(
                method,
                "reflectPutThenSpliteratorOut" | "reflectPutThenIterSpliteratorOut"
            ) {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| call
                        == "java/util/Spliterator#forEachRemaining(Ljava/util/function/Consumer;)V"),
                    "{method} must count Spliterator#forEachRemaining after epoch bump: {calls:?}"
                );
            }
            if matches!(
                method,
                "forEachSavedEntryToStringOut" | "unmodForEachSavedEntryToStringOut"
            ) {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/Map$Entry#toString()Ljava/lang/String;"),
                    "{method} must count Map$Entry#toString after reflective put: {calls:?}"
                );
            }
            if method == "forEachSavedEntryEqualsOut" {
                let calls = host.unknown_host_calls();
                let equals_count: usize = calls
                    .iter()
                    .filter(|(call, _)| call == "java/util/Map$Entry#equals(Ljava/lang/Object;)Z")
                    .map(|(_, count)| *count)
                    .sum();
                assert!(
                    equals_count >= 4,
                    "forEachSavedEntryEqualsOut must count all four Map$Entry#equals calls (both directions), got {equals_count}: {calls:?}"
                );
            }
            if method == "forEachSavedEntryConcatOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls.iter().any(|(call, _)| {
                        call == "java/lang/String#valueOf(Ljava/lang/Object;)Ljava/lang/String;"
                            || call.starts_with(
                                "java/lang/invoke/StringConcatFactory#makeConcatWithConstants(",
                            )
                    }),
                    "forEachSavedEntryConcatOut must count String#valueOf or StringConcatFactory#makeConcatWithConstants: {calls:?}"
                );
            }
            if method == "reflectPutThenEnumerationOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/Enumeration#hasMoreElements()Z"),
                    "reflectPutThenEnumerationOut must count Enumeration#hasMoreElements: {calls:?}"
                );
            }
            if method == "reflectPutThenStreamIteratorOut" {
                let calls = host.unknown_host_calls();
                assert!(
                    calls
                        .iter()
                        .any(|(call, _)| call == "java/util/Iterator#hasNext()Z"),
                    "reflectPutThenStreamIteratorOut must count Iterator#hasNext on stream iterator: {calls:?}"
                );
            }
        } else if counted == 0 {
            assert_eq!(actual, expected, "{method}");
        } else {
            assert!(
                counted > 0,
                "{method}: identity answer was neither exact nor counted"
            );
        }
    }
}

#[test]
fn dynamic_intern_aliasing_in_identity_set_is_counted() {
    let sets_stub = r#"package com.google.common.collect;
public final class Sets {
  public static <E> java.util.Set<E> newIdentityHashSet() { throw new AssertionError(); }
}"#;
    let probe = r#"package demo;
import com.google.common.collect.Sets;
import java.util.Set;
public final class Probe {
  public static String run() {
    Set<String> set = Sets.newIdentityHashSet();
    String s = new StringBuilder("xy").append("zq").toString();
    s.intern();
    set.add(s);
    return String.valueOf(set.contains("xyzq"));
  }
}"#;
    let fixture = Fixture::build(
        "dynamic-intern-identity-set",
        &[
            ("com/google/common/collect/Sets.java", sets_stub),
            ("demo/Probe.java", probe),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.text_on(&mut host, "run");
    let calls = host.unknown_host_calls();
    assert!(
        calls
            .iter()
            .any(|(call, _)| call == "java/util/Set#contains(Ljava/lang/Object;)Z"),
        "interned fresh key vs literal in IdentityHashSet must count Set#contains: {calls:?}"
    );
}

#[test]
fn guava_identity_set_equals_hash_code_and_derived_handles() {
    let sets_stub = r#"package com.google.common.collect;
public final class Sets {
  public static <E> java.util.Set<E> newIdentityHashSet() { throw new AssertionError(); }
}"#;
    let probe = r#"package demo;
import com.google.common.collect.Sets;
import java.lang.reflect.Method;
import java.util.*;
import java.util.stream.*;
public final class Probe {
  public static String exactEqAndEmptyHash() {
    Set<String> s1 = Sets.newIdentityHashSet();
    Set<String> s2 = Sets.newIdentityHashSet();
    Set<String> sSame = Sets.newIdentityHashSet();
    String k = new String("k");
    s1.add(k);
    s2.add(new String("k"));
    sSame.add(k);
    Set<String> empty = Sets.newIdentityHashSet();
    return s1.equals(s2) + ":" + s1.equals(sSame) + ":" + empty.hashCode();
  }
  public static String nonEmptyHash() {
    Set<String> s = Sets.newIdentityHashSet();
    s.add(new String("k"));
    return String.valueOf(s.hashCode());
  }
  public static String reflectAddThenDerived() throws Exception {
    Set<String> s = Sets.newIdentityHashSet();
    s.add("a");
    Iterator<String> it = s.iterator();
    Stream<String> st = s.stream().filter(x -> x != null);
    Spliterator<String> sp = Spliterators.spliterator(s, 0);
    Method add = Set.class.getMethod("add", Object.class);
    add.invoke(s, "b");
    boolean hn = it.hasNext();
    long c = st.count();
    int[] n = { 0 };
    sp.forEachRemaining(x -> n[0]++);
    return hn + ":" + c + ":" + n[0];
  }
}"#;
    let fixture = Fixture::build(
        "guava-identity-set-round5",
        &[
            ("com/google/common/collect/Sets.java", sets_stub),
            ("demo/Probe.java", probe),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let actual = fixture.text_on(&mut host, "exactEqAndEmptyHash");
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "exactEqAndEmptyHash counted: {:?}",
        host.unknown_host_calls()
    );
    assert_eq!(actual, "false:true:0");

    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.text_on(&mut host, "nonEmptyHash");
    let calls = host.unknown_host_calls();
    assert!(
        calls
            .iter()
            .any(|(call, _)| call == "java/util/Set#hashCode()I"),
        "non-empty IdentityHashSet#hashCode must be counted: {calls:?}"
    );

    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.text_on(&mut host, "reflectAddThenDerived");
    let calls = host.unknown_host_calls();
    for expected in [
        "java/lang/reflect/Method#invoke(Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;",
        "java/util/Iterator#hasNext()Z",
        "java/util/stream/Stream#count()J",
        "java/util/Spliterator#forEachRemaining(Ljava/util/function/Consumer;)V",
    ] {
        assert!(
            calls.iter().any(|(call, _)| call == expected),
            "reflectAddThenDerived must count {expected}: {calls:?}"
        );
    }
}

#[test]
fn mixin_modified_identity_arguments_are_counted() {
    let mixin_ann = r#"package org.spongepowered.asm.mixin;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface Mixin {
    Class<?>[] value() default {};
    String[] targets() default {};
}"#;
    let at_ann = r#"package org.spongepowered.asm.mixin.injection;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
@Target({})
public @interface At {
    String value() default "";
    String target() default "";
    int ordinal() default -1;
}"#;
    let modify_arg_ann = r#"package org.spongepowered.asm.mixin.injection;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
public @interface ModifyArg {
    String[] method();
    At at();
    int index() default 0;
    boolean cancellable() default false;
}"#;
    let modify_var_ann = r#"package org.spongepowered.asm.mixin.injection;
import java.lang.annotation.*;
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
public @interface ModifyVariable {
    String[] method();
    At at();
    int index() default -1;
    int ordinal() default -1;
    String[] name() default {};
    boolean argsOnly() default false;
    boolean print() default false;
}"#;
    let target = r#"package demo;
import java.util.IdentityHashMap;
public final class Target {
    public static String B;
    public static int unmodified() {
        String a = new String("x");
        String b = new String("x");
        IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
        m.put(a, 1);
        m.put(b, 2);
        Integer v = m.get(a);
        return v == null ? -1 : v;
    }
    public static int lookupWithModifyArg() {
        String a = new String("x");
        String b = new String("x");
        B = b;
        IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
        m.put(a, 1);
        m.put(b, 2);
        Integer v = m.get(a);
        return v == null ? -1 : v;
    }
    public static int lookupWithModifyVarLoad() {
        String a = new String("x");
        String b = new String("x");
        B = b;
        IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
        m.put(a, 1);
        m.put(b, 2);
        String k = a;
        Integer v = m.get(k);
        return v == null ? -1 : v;
    }
}"#;
    let mixin = r#"package demo;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.ModifyVariable;

@Mixin(targets = {"Ldemo/Target;"})
public abstract class IdentityMixin {
    @ModifyArg(
        method = "lookupWithModifyArg()I",
        at = @At(value = "INVOKE", target = "Ljava/util/IdentityHashMap;get(Ljava/lang/Object;)Ljava/lang/Object;"),
        index = 0
    )
    private static Object swapArgToB(Object arg) {
        return Target.B;
    }

    @ModifyVariable(
        method = "lookupWithModifyVarLoad()I",
        at = @At("LOAD"),
        index = 3
    )
    private static String swapLoadedLocalToB(String value) {
        return Target.B;
    }
}"#;
    let fixture = Fixture::build(
        "mixin-identity-modify",
        &[
            ("org/spongepowered/asm/mixin/Mixin.java", mixin_ann),
            ("org/spongepowered/asm/mixin/injection/At.java", at_ann),
            (
                "org/spongepowered/asm/mixin/injection/ModifyArg.java",
                modify_arg_ann,
            ),
            (
                "org/spongepowered/asm/mixin/injection/ModifyVariable.java",
                modify_var_ann,
            ),
            ("demo/Target.java", target),
            ("demo/IdentityMixin.java", mixin),
        ],
    );

    // Baseline without modifier: exact `1`, zero counted calls.
    let mut host = fixture.jar.seed_host(StdHost::new());
    let baseline = fixture
        .jar
        .invoke_method_with_args("demo/Target", "unmodified", "()I", None, &[], &mut host)
        .unwrap();
    assert_eq!(baseline, Value::Int(1));
    assert_eq!(host.unknown_host_call_count(), 0);

    // `@ModifyArg` swaps `m.get(a)` to `m.get(b)` (JDK 25: `2`): must drop static provenance
    // and count `IdentityHashMap#get` rather than answering `1` from pre-modifier `a`.
    let mut host = fixture.jar.seed_host(StdHost::new());
    let report = fixture
        .jar
        .execute_discovered_mixins(
            &MethodRef {
                owner: "demo/Target".into(),
                name: "lookupWithModifyArg".into(),
                descriptor: "()I".into(),
            },
            None,
            &[],
            &mut host,
        )
        .unwrap();
    assert!(
        !report.callback_results.is_empty(),
        "expected @ModifyArg to run: {report:?}"
    );
    let calls = host.unknown_host_calls();
    assert!(
        calls.iter().any(|(call, _)| call
            == "java/util/IdentityHashMap#get(Ljava/lang/Object;)Ljava/lang/Object;"),
        "@ModifyArg swapping equal-text String must count IdentityHashMap#get: {calls:?}"
    );

    // `@ModifyVariable(LOAD)` swaps `aload_3` (`k = a`) to `b` (JDK 25: `2`): must drop static
    // provenance and count `IdentityHashMap#get`.
    let mut host = fixture.jar.seed_host(StdHost::new());
    let report = fixture
        .jar
        .execute_discovered_mixins(
            &MethodRef {
                owner: "demo/Target".into(),
                name: "lookupWithModifyVarLoad".into(),
                descriptor: "()I".into(),
            },
            None,
            &[],
            &mut host,
        )
        .unwrap();
    assert!(
        !report.callback_results.is_empty(),
        "expected @ModifyVariable(LOAD) to run: {report:?}"
    );
    let calls = host.unknown_host_calls();
    assert!(
        calls.iter().any(|(call, _)| call
            == "java/util/IdentityHashMap#get(Ljava/lang/Object;)Ljava/lang/Object;"),
        "@ModifyVariable(LOAD) swapping equal-text String must count IdentityHashMap#get: {calls:?}"
    );
}

#[test]
fn hundred_thousand_iterators_and_streams_do_not_grow_identity_tables() {
    let probe = r#"package demo;
import java.lang.reflect.Method;
import java.util.*;
import java.util.stream.*;
public final class Probe {
    static IdentityHashMap<Object, Integer> LIVE_AFTER_BUMP;
    static final Object LIVE_KEY = new Object();
    static int runLoop(int iterations) {
        IdentityHashMap<String, Integer> m = new IdentityHashMap<>();
        m.put("k", 1);
        Set<String> ks = m.keySet();
        int touched = 0;
        for (int i = 0; i < iterations; i++) {
            Iterator<String> it = ks.iterator();
            Stream<String> st = ks.stream();
            if (it != null && st != null) {
                touched++;
            }
        }
        return touched;
    }
    public static int cleanLoop100k() {
        return runLoop(100_000);
    }
    public static int postBumpLoop() throws Exception {
        Method gc = System.class.getMethod("gc");
        gc.invoke(null);
        return runLoop(2_000);
    }
    public static int bumpAndAllocateMany() throws Exception {
        Method gc = System.class.getMethod("gc");
        gc.invoke(null);
        LIVE_AFTER_BUMP = new IdentityHashMap<>();
        LIVE_AFTER_BUMP.put(LIVE_KEY, 42);
        int sum = 0;
        for (int i = 0; i < 2_000; i++) {
            IdentityHashMap<String, Integer> tmp = new IdentityHashMap<>();
            tmp.put("k", i);
            sum += tmp.size();
        }
        return sum;
    }
    public static int triggerGc() {
        int s = 0;
        for (int i = 0; i < 6_000; i++) {
            s += new int[1].length;
        }
        return s;
    }
    public static String checkLive() {
        return String.valueOf(LIVE_AFTER_BUMP.get(LIVE_KEY));
    }
}"#;
    let fixture = Fixture::build("identity-loop-bounded", &[("demo/Probe.java", probe)]);

    // 100,000 iterators and 100,000 streams in a single invocation before any epoch bump:
    // identity tables stay bounded (no per-handle derived_roots table exists).
    let mut host = fixture.jar.seed_host(StdHost::new());
    let result = fixture
        .jar
        .invoke_method_with_args("demo/Probe", "cleanLoop100k", "()I", None, &[], &mut host)
        .unwrap();
    assert_eq!(result, Value::Int(100_000));
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "cleanLoop100k counted: {:?}",
        host.unknown_host_calls()
    );
    assert!(
        host.identity_epoch_count() <= 1,
        "expected <= 1 identity_epochs entry, got {}",
        host.identity_epoch_count()
    );
    assert!(
        host.peak_identity_token_count() <= 1,
        "expected <= 1 identity token entry, got {}",
        host.peak_identity_token_count()
    );

    // Same iterator+stream loop AFTER an epoch bump so `CollectionStore::identity_epochs`
    // records the freshly constructed map: still bounded at 1 entry.
    let mut host = fixture.jar.seed_host(StdHost::new());
    let result = fixture
        .jar
        .invoke_method_with_args("demo/Probe", "postBumpLoop", "()I", None, &[], &mut host)
        .unwrap();
    assert_eq!(result, Value::Int(2_000));
    assert!(
        host.identity_epoch_count() <= 1,
        "expected <= 1 identity_epochs entry after bump + loop, got {}",
        host.identity_epoch_count()
    );
    assert!(
        host.peak_identity_token_count() <= 1,
        "expected <= 1 identity token entry after bump + loop, got {}",
        host.peak_identity_token_count()
    );

    // 2,000 transient IdentityHashMaps allocated after an epoch bump alongside one
    // live static IdentityHashMap: once the call log is rewound and GC runs without
    // another epoch bump, `CollectionStore::retain_markers` prunes every unreachable
    // map's `identity_epochs` entry down to the 1 live map.
    let mut host = fixture.jar.seed_host(StdHost::new());
    let mark = host.call_log_mark();
    let sum = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "bumpAndAllocateMany",
            "()I",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    assert_eq!(sum, Value::Int(2_000));
    host.rewind_call_log(mark);
    let _ = fixture
        .jar
        .invoke_method_with_args("demo/Probe", "triggerGc", "()I", None, &[], &mut host)
        .unwrap();
    assert!(
        host.freed_object_ranges() > 0,
        "expected resident-heap GC to run after allocating 2,000 transient IdentityHashMaps"
    );
    assert_eq!(
        host.identity_epoch_count(),
        1,
        "expected GC to prune all 2,000 transient identity_epochs down to the 1 live static map"
    );
    let counted_after_alloc = host.unknown_host_call_count();
    let live = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "checkLive",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    assert_eq!(live, Value::Ref(Some("42".into())));
    assert_eq!(
        host.unknown_host_call_count(),
        counted_after_alloc,
        "live map should stay readable with zero extra counted calls: {:?}",
        host.unknown_host_calls()
    );
}
