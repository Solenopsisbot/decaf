#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.lang.annotation.Annotation;
import java.lang.annotation.Inherited;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.reflect.Method;

@Retention(RetentionPolicy.RUNTIME)
@interface N { int x(); }

enum E { A, B }

@Retention(RetentionPolicy.RUNTIME)
@interface Marker {}

@Retention(RetentionPolicy.RUNTIME)
@interface C { Class<?>[] value(); }

@Retention(RetentionPolicy.RUNTIME)
@interface A {
  byte b() default 1; char c() default 'q'; double d() default 2.0; float f() default 1.0f;
  int i() default 3; E en() default E.A; Class<?> cls() default String.class; long l() default 4L;
  String[] sa() default {}; N n() default @N(x=7); N[] na() default {}; String s() default "def";
  short sh() default 2; int[] ia() default {}; boolean z() default false; E[] ea() default {};
  Class<?>[] ca() default {};
}

@Retention(RetentionPolicy.RUNTIME)
@Inherited
@interface I { String value(); }

@I("parent")
class Parent {}

final class Child extends Parent {}

@C({String.class})
class ArrayEqualLeft {}

@C({String.class})
class ArrayEqualRight {}

@C({String.class, Integer.class})
class ArrayUnequalLength {}

@Marker
@A(b=(byte)3, c='x', d=2.5, f=1.5f, i=5, en=E.B, cls=String.class, l=6L,
   sa={"x", "y"}, n=@N(x=4), na={@N(x=4), @N(x=8)}, s="hi", sh=(short)4,
   ia={1, 2}, z=true, ea={E.A, E.B}, ca={String.class, int.class})
public final class Probe {
  public static String objects() {
    A a = Probe.class.getAnnotation(A.class);
    return a.toString() + "|" + a.equals(a);
  }

  public static String specials() {
    return "" + Special.class.getAnnotation(A.class).toString();
  }

  @A(f=Float.NaN, d=Double.POSITIVE_INFINITY)
  static final class Special {}

  static final class FakeMarker implements Marker {
    @Override public Class<? extends java.lang.annotation.Annotation> annotationType() {
      return Marker.class;
    }
  }

  public static String classArrayEquals() {
    C left = ArrayEqualLeft.class.getAnnotation(C.class);
    C right = ArrayEqualRight.class.getAnnotation(C.class);
    return Boolean.toString(left.equals(right));
  }

  public static String classArrayUnequal() {
    C left = ArrayEqualLeft.class.getAnnotation(C.class);
    C right = ArrayUnequalLength.class.getAnnotation(C.class);
    return Boolean.toString(left.equals(right));
  }

  public static String stringForgery() {
    Marker marker = Probe.class.getAnnotation(Marker.class);
    return Boolean.toString(marker.equals("annotation/Ldemo/Marker;/0"));
  }

  public static String jarAnnotationImplementation() {
    Marker marker = Probe.class.getAnnotation(Marker.class);
    return Boolean.toString(marker.equals(new FakeMarker()));
  }

  public static String inherited() {
    I value = Child.class.getAnnotation(I.class);
    return value.value() + "|" + Child.class.getDeclaredAnnotations().length + "|"
        + Child.class.isAnnotationPresent(I.class);
  }

  public static String unheld() throws Exception {
    Class<?> type = String.class;
    Method method = String.class.getDeclaredMethod("length");
    return (type.getAnnotation(A.class) == null) + ":" + (method.getAnnotation(A.class) == null);
  }
}
"#;

// `A` has 17 defaulted elements, so the JDK prints them in the bucket order of the
// `HashMap` it keeps the defaults in, and three pairs of them (`en`/`i`, `n`/`sa`,
// `na`/`s`) share a bucket: their relative order is the order `getDeclaredMethods()`
// listed them in, which the JVM does not define. Java 25.0.5.1 printed
//
//   @demo.A(b=(byte)0x03, c='x', d=2.5, f=1.5f, i=5, en=B, cls=java.lang.String.class,
//   l=6L, sa={"x", "y"}, n=@demo.N(x=4), s="hi", na={@demo.N(x=4), @demo.N(x=8)}, sh=4,
//   ia={1, 2}, z=true, ea={A, B}, ca={java.lang.String.class, int.class})
//
// so the print is counted rather than answered in an order that is only sometimes
// right. `annotation_tostring` holds the exact rows (no shared bucket).
#[test]
fn annotation_object_with_shared_default_buckets_is_counted() {
    let fixture = Fixture::build("field-annotations-review", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.text_on(&mut host, "objects");
    assert!(host.unknown_host_call_count() > 0);
}

#[test]
fn special_float_annotation_values_are_counted_with_shared_default_buckets() {
    let fixture = Fixture::build("field-annotations-specials", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    // Java 25.0.5.1: `@demo.A(b=(byte)0x01, c='q', d=1.0/0.0, f=0.0f/0.0f, i=3, en=A,
    // cls=java.lang.String.class, l=4L, sa={}, n=@demo.N(x=7), s="def", na={}, sh=2,
    // ia={}, z=false, ea={}, ca={})`; the same shared buckets as above.
    let _ = fixture.text_on(&mut host, "specials");
    assert!(host.unknown_host_call_count() > 0);
}

#[test]
fn class_member_array_equal_row_is_counted() {
    let fixture = Fixture::build(
        "field-annotations-class-array-equal",
        &[("demo/Probe.java", PROBE)],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    // Java 25 oracle: true. Class members compare by VM identity; this runtime
    // deliberately counts the unsupported equal-length element comparison.
    let _ = fixture.text_on(&mut host, "classArrayEquals");
    assert!(host.unknown_host_call_count() > 0);
}

#[test]
fn class_member_array_unequal_row_is_exact() {
    let fixture = Fixture::build(
        "field-annotations-class-array-unequal",
        &[("demo/Probe.java", PROBE)],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    // Java 25 oracle: false. Arrays.equals checks length before member identity.
    assert_eq!(fixture.text_on(&mut host, "classArrayUnequal"), "false");
    assert_eq!(host.unknown_host_call_count(), 0);
}

#[test]
fn string_forgery_row_is_false() {
    let fixture = Fixture::build(
        "field-annotations-string-forgery",
        &[("demo/Probe.java", PROBE)],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    // Java 25 oracle: false. A String spelling the proxy key is still a String.
    assert_eq!(fixture.text_on(&mut host, "stringForgery"), "false");
}

#[test]
fn jar_annotation_implementation_row_is_counted() {
    let fixture = Fixture::build(
        "field-annotations-jar-annotation",
        &[("demo/Probe.java", PROBE)],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    // Java 25 oracle: true. A jar class implementing the annotation interface
    // needs type.isInstance and is counted until that dynamic identity is held.
    let _ = fixture.text_on(&mut host, "jarAnnotationImplementation");
    assert!(host.unknown_host_call_count() > 0);
}

#[test]
fn inherited_class_annotations_match_java_25() {
    let fixture = Fixture::build("field-annotations-inherited", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "inherited"), "parent|0|true");
}

#[test]
fn unheld_class_and_method_queries_are_counted() {
    let fixture = Fixture::build("field-annotations-unheld", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "unheld"), "true:true");
    let unknown = host.unknown_host_calls();
    assert!(
        unknown
            .iter()
            .any(|(call, _)| call.contains("java/lang/Class#getAnnotation")),
        "{unknown:?}"
    );
    assert!(
        unknown
            .iter()
            .any(|(call, _)| call.contains("java/lang/reflect/Method#getAnnotation")),
        "{unknown:?}"
    );
}
