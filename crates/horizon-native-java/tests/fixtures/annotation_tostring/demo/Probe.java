package demo;

import java.lang.annotation.Annotation;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Repeatable;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

@Retention(RetentionPolicy.RUNTIME) @interface PAnno { String value(); }
enum TopE { X, Y }
@Retention(RetentionPolicy.RUNTIME) @interface TopAnno { int n() default 3; }
/**
 * `Annotation.toString()`, `hashCode()` and `equals()` of annotations read
 * back by reflection, run unchanged on both sides: its own `main` on Java 25
 * (output is `java25.txt`), its static probes in Horizon. Each `t_`, `h_` and
 * `x_` method answers one `<label>` row; `main` prints `<label>=<value>`, one
 * line per row, sorted by label. A call that throws prints `EXC <class>:<message>`.
 *
 * `t_` rows are `toString` of an annotation on a class (or a field, method,
 * constructor or parameter); `h_` rows are its `hashCode`; `x_` rows are
 * `equals` both ways and whether the two `hashCode`s agree. Covered: every
 * element type (alone and as arrays), nested and repeated annotations, defaults
 * (their `HashMap` order and explicit overrides), a lone `value`, escaping,
 * `float`/`double` specials, `Class` and enum constants, canonical names.
 */
public final class Probe {
    enum E { A, B, C { int f() { return 1; } }; int f() { return 0; } }
    static class Inner { @Retention(RetentionPolicy.RUNTIME) @interface Deep { String value(); } }
    @Retention(RetentionPolicy.RUNTIME) @interface AStr { String v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface AChar { char v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface AInt { int v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface ALong { long v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface AFloat { float v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface ADouble { double v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface ABool { boolean v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface AByte { byte v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface AShort { short v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface AClass { Class<?> v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface AEnum { E v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface ANest { PAnno v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SStr { String[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SChar { char[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SInt { int[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SLong { long[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SFloat { float[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SDouble { double[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SBool { boolean[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SByte { byte[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SShort { short[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SClass { Class<?>[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SEnum { E[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface SNest { PAnno[] v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface VStr { String value(); }
    @Retention(RetentionPolicy.RUNTIME) @interface VInt { int value(); }
    @Retention(RetentionPolicy.RUNTIME) @interface VArr { String[] value(); }
    @Retention(RetentionPolicy.RUNTIME) @interface VEnum { E value(); }
    @Retention(RetentionPolicy.RUNTIME) @interface VNest { PAnno value(); }
    @Retention(RetentionPolicy.RUNTIME) @interface VDef { String value() default "vd"; }
    @Retention(RetentionPolicy.RUNTIME) @interface VDefArr { String[] value() default {}; }
    @Retention(RetentionPolicy.RUNTIME) @interface OneV { String v() default "ov"; }
    @Retention(RetentionPolicy.RUNTIME) @interface TwoV { String value(); int n() default 1; }
    @Retention(RetentionPolicy.RUNTIME) @interface TwoVd { String value() default "vd"; int n() default 1; }
    @Retention(RetentionPolicy.RUNTIME) @interface Req2 { String a(); int b(); }
    @Retention(RetentionPolicy.RUNTIME) @interface Req3 { String a(); int b(); long c(); }
    @Retention(RetentionPolicy.RUNTIME) @interface NoEl {  }
    @Retention(RetentionPolicy.RUNTIME) @interface Const { String NAME = new String("nm"); int v() default 2; }
    @Retention(RetentionPolicy.RUNTIME) @interface Many { int alpha() default 0;int beta() default 1;int gamma() default 2;int delta() default 3;int epsilon() default 4;int zeta() default 5;int eta() default 6;int theta() default 7;int iota() default 8;int kappa() default 9;int lambda() default 10;int mu() default 11; }
    @Retention(RetentionPolicy.RUNTIME) @interface Mix { String req(); int dflt() default 1; long zed(); String abc() default "abc"; boolean mm(); }
    @Retention(RetentionPolicy.RUNTIME) @Repeatable(Reps.class) @interface Rep { int value(); }
    @Retention(RetentionPolicy.RUNTIME) @interface Reps { Rep[] value(); }
    @Retention(RetentionPolicy.RUNTIME) @interface Dual { String s() default "s"; int[] a() default {1, 2}; Class<?> k() default Object.class; E e() default E.B; }
    @Retention(RetentionPolicy.RUNTIME) @interface Wrap { PAnno one(); PAnno[] many() default {}; ANest deep(); }
    @Retention(RetentionPolicy.RUNTIME) @interface Def2 { int blue() default 0; int five() default 1; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def3 { int indigo() default 0; int blue() default 1; int two() default 2; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def4 { int seven() default 0; int nu() default 1; int mu() default 2; int two() default 3; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def5 { int blue() default 0; int upsilon() default 1; int kappa() default 2; int zeta() default 3; int four() default 4; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def6 { int pi() default 0; int eight() default 1; int beta() default 2; int indigo() default 3; int phi() default 4; int blue() default 5; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def7 { int eight() default 0; int one() default 1; int tau() default 2; int three() default 3; int mu() default 4; int epsilon() default 5; int iota() default 6; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def8 { int gamma() default 0; int sigma() default 1; int eight() default 2; int right() default 3; int east() default 4; int left() default 5; int six() default 6; int psi() default 7; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def12 { int left() default 0; int omega() default 1; int four() default 2; int kappa() default 3; int psi() default 4; int iota() default 5; int delta() default 6; int two() default 7; int indigo() default 8; int pi() default 9; int six() default 10; int nine() default 11; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def13 { int phi() default 0; int xi() default 1; int chi() default 2; int five() default 3; int seven() default 4; int left() default 5; int blue() default 6; int violet() default 7; int nine() default 8; int six() default 9; int omega() default 10; int psi() default 11; int kappa() default 12; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def16 { int zeta() default 0; int yellow() default 1; int chi() default 2; int six() default 3; int west() default 4; int red() default 5; int xi() default 6; int lambda() default 7; int omega() default 8; int eta() default 9; int tau() default 10; int blue() default 11; int upsilon() default 12; int ten() default 13; int iota() default 14; int eight() default 15; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def24 { int east() default 0; int left() default 1; int omega() default 2; int six() default 3; int beta() default 4; int pi() default 5; int seven() default 6; int eight() default 7; int blue() default 8; int alpha() default 9; int right() default 10; int nine() default 11; int sigma() default 12; int up() default 13; int five() default 14; int xi() default 15; int yellow() default 16; int red() default 17; int down() default 18; int four() default 19; int nu() default 20; int chi() default 21; int rho() default 22; int ten() default 23; }
    @Retention(RetentionPolicy.RUNTIME) @interface Def25 { int up() default 0; int chi() default 1; int eight() default 2; int gamma() default 3; int lambda() default 4; int kappa() default 5; int south() default 6; int iota() default 7; int three() default 8; int pi() default 9; int seven() default 10; int right() default 11; int green() default 12; int alpha() default 13; int omicron() default 14; int sigma() default 15; int tau() default 16; int blue() default 17; int nu() default 18; int indigo() default 19; int delta() default 20; int mu() default 21; int rho() default 22; int four() default 23; int one() default 24; }
    @Retention(RetentionPolicy.RUNTIME) @interface ATopEnum { TopE v(); }
    @Retention(RetentionPolicy.RUNTIME) @interface AEnumJ { java.util.concurrent.TimeUnit v(); }

    private static String b(boolean z) { return z ? "true" : "false"; }
    private static String ex(Throwable t) { return t.getClass().getName() + ":" + t.getMessage(); }

    private static String ts(Class<?> c, Class<? extends Annotation> a) {
        try { Annotation x = c.getAnnotation(a); return x == null ? "null" : x.toString(); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    private static String hs(Class<?> c, Class<? extends Annotation> a) {
        try { return Integer.toString(c.getAnnotation(a).hashCode()); } catch (Throwable t) { return "EXC " + ex(t); }
    }

    @Def2 static class C0 {}
    public static String t_dflt2() { return ts(C0.class, Def2.class); }
    public static String h_dflt2() { return hs(C0.class, Def2.class); }
    @Def2(five = 99, blue = 98) static class C1 {}
    public static String t_dflt2_over() { return ts(C1.class, Def2.class); }
    public static String h_dflt2_over() { return hs(C1.class, Def2.class); }
    @Def3 static class C2 {}
    public static String t_dflt3() { return ts(C2.class, Def3.class); }
    public static String h_dflt3() { return hs(C2.class, Def3.class); }
    @Def3(two = 99, indigo = 98) static class C3 {}
    public static String t_dflt3_over() { return ts(C3.class, Def3.class); }
    public static String h_dflt3_over() { return hs(C3.class, Def3.class); }
    @Def4 static class C4 {}
    public static String t_dflt4() { return ts(C4.class, Def4.class); }
    public static String h_dflt4() { return hs(C4.class, Def4.class); }
    @Def4(two = 99, seven = 98) static class C5 {}
    public static String t_dflt4_over() { return ts(C5.class, Def4.class); }
    public static String h_dflt4_over() { return hs(C5.class, Def4.class); }
    @Def5 static class C6 {}
    public static String t_dflt5() { return ts(C6.class, Def5.class); }
    public static String h_dflt5() { return hs(C6.class, Def5.class); }
    @Def5(four = 99, blue = 98) static class C7 {}
    public static String t_dflt5_over() { return ts(C7.class, Def5.class); }
    public static String h_dflt5_over() { return hs(C7.class, Def5.class); }
    @Def6 static class C8 {}
    public static String t_dflt6() { return ts(C8.class, Def6.class); }
    public static String h_dflt6() { return hs(C8.class, Def6.class); }
    @Def6(blue = 99, pi = 98) static class C9 {}
    public static String t_dflt6_over() { return ts(C9.class, Def6.class); }
    public static String h_dflt6_over() { return hs(C9.class, Def6.class); }
    @Def7 static class C10 {}
    public static String t_dflt7() { return ts(C10.class, Def7.class); }
    public static String h_dflt7() { return hs(C10.class, Def7.class); }
    @Def7(iota = 99, eight = 98) static class C11 {}
    public static String t_dflt7_over() { return ts(C11.class, Def7.class); }
    public static String h_dflt7_over() { return hs(C11.class, Def7.class); }
    @Def8 static class C12 {}
    public static String t_dflt8() { return ts(C12.class, Def8.class); }
    public static String h_dflt8() { return hs(C12.class, Def8.class); }
    @Def8(psi = 99, gamma = 98) static class C13 {}
    public static String t_dflt8_over() { return ts(C13.class, Def8.class); }
    public static String h_dflt8_over() { return hs(C13.class, Def8.class); }
    @Def12 static class C14 {}
    public static String t_dflt12() { return ts(C14.class, Def12.class); }
    public static String h_dflt12() { return hs(C14.class, Def12.class); }
    @Def12(nine = 99, left = 98) static class C15 {}
    public static String t_dflt12_over() { return ts(C15.class, Def12.class); }
    public static String h_dflt12_over() { return hs(C15.class, Def12.class); }
    @Def13 static class C16 {}
    public static String t_dflt13() { return ts(C16.class, Def13.class); }
    public static String h_dflt13() { return hs(C16.class, Def13.class); }
    @Def13(kappa = 99, phi = 98) static class C17 {}
    public static String t_dflt13_over() { return ts(C17.class, Def13.class); }
    public static String h_dflt13_over() { return hs(C17.class, Def13.class); }
    @Def16 static class C18 {}
    public static String t_dflt16() { return ts(C18.class, Def16.class); }
    public static String h_dflt16() { return hs(C18.class, Def16.class); }
    @Def16(eight = 99, zeta = 98) static class C19 {}
    public static String t_dflt16_over() { return ts(C19.class, Def16.class); }
    public static String h_dflt16_over() { return hs(C19.class, Def16.class); }
    @Def24 static class C20 {}
    public static String t_dflt24() { return ts(C20.class, Def24.class); }
    public static String h_dflt24() { return hs(C20.class, Def24.class); }
    @Def24(ten = 99, east = 98) static class C21 {}
    public static String t_dflt24_over() { return ts(C21.class, Def24.class); }
    public static String h_dflt24_over() { return hs(C21.class, Def24.class); }
    @Def25 static class C22 {}
    public static String t_dflt25() { return ts(C22.class, Def25.class); }
    public static String h_dflt25() { return hs(C22.class, Def25.class); }
    @Def25(one = 99, up = 98) static class C23 {}
    public static String t_dflt25_over() { return ts(C23.class, Def25.class); }
    public static String h_dflt25_over() { return hs(C23.class, Def25.class); }
    @AStr(v = "marked") static class C24 {}
    public static String t_str_pstring() { return ts(C24.class, AStr.class); }
    public static String h_str_pstring() { return hs(C24.class, AStr.class); }
    @AStr(v = "") static class C25 {}
    public static String t_str_empty() { return ts(C25.class, AStr.class); }
    public static String h_str_empty() { return hs(C25.class, AStr.class); }
    @AStr(v = "q\"b\\s'") static class C26 {}
    public static String t_str_quote() { return ts(C26.class, AStr.class); }
    public static String h_str_quote() { return hs(C26.class, AStr.class); }
    @AStr(v = "l\nb\tt\rr\bb\ff") static class C27 {}
    public static String t_str_ctl() { return ts(C27.class, AStr.class); }
    public static String h_str_ctl() { return hs(C27.class, AStr.class); }
    @AStr(v = "\u0000\u0001\u001f\u007f\u0080\u009f") static class C28 {}
    public static String t_str_ctl2() { return ts(C28.class, AStr.class); }
    public static String h_str_ctl2() { return hs(C28.class, AStr.class); }
    @AStr(v = "é€ñ日本語") static class C29 {}
    public static String t_str_uni() { return ts(C29.class, AStr.class); }
    public static String h_str_uni() { return hs(C29.class, AStr.class); }
    @AStr(v = "\ud83d\ude00x") static class C30 {}
    public static String t_str_supp() { return ts(C30.class, AStr.class); }
    public static String h_str_supp() { return hs(C30.class, AStr.class); }
    @AStr(v = "\u2028\u2029\u00a0\u200b\u0085") static class C31 {}
    public static String t_str_sep() { return ts(C31.class, AStr.class); }
    public static String h_str_sep() { return hs(C31.class, AStr.class); }
    @AStr(v = "a/b\\c") static class C32 {}
    public static String t_str_slash() { return ts(C32.class, AStr.class); }
    public static String h_str_slash() { return hs(C32.class, AStr.class); }
    @AStr(v = "~ !#$%&") static class C33 {}
    public static String t_str_tilde() { return ts(C33.class, AStr.class); }
    public static String h_str_tilde() { return hs(C33.class, AStr.class); }
    @AChar(v = 'a') static class C34 {}
    public static String t_char_a() { return ts(C34.class, AChar.class); }
    public static String h_char_a() { return hs(C34.class, AChar.class); }
    @AChar(v = '\'') static class C35 {}
    public static String t_char_apos() { return ts(C35.class, AChar.class); }
    public static String h_char_apos() { return hs(C35.class, AChar.class); }
    @AChar(v = '"') static class C36 {}
    public static String t_char_dq() { return ts(C36.class, AChar.class); }
    public static String h_char_dq() { return hs(C36.class, AChar.class); }
    @AChar(v = '\\') static class C37 {}
    public static String t_char_bs() { return ts(C37.class, AChar.class); }
    public static String h_char_bs() { return hs(C37.class, AChar.class); }
    @AChar(v = '\n') static class C38 {}
    public static String t_char_nl() { return ts(C38.class, AChar.class); }
    public static String h_char_nl() { return hs(C38.class, AChar.class); }
    @AChar(v = '\t') static class C39 {}
    public static String t_char_tab() { return ts(C39.class, AChar.class); }
    public static String h_char_tab() { return hs(C39.class, AChar.class); }
    @AChar(v = '\r') static class C40 {}
    public static String t_char_cr() { return ts(C40.class, AChar.class); }
    public static String h_char_cr() { return hs(C40.class, AChar.class); }
    @AChar(v = '\b') static class C41 {}
    public static String t_char_bsp() { return ts(C41.class, AChar.class); }
    public static String h_char_bsp() { return hs(C41.class, AChar.class); }
    @AChar(v = '\f') static class C42 {}
    public static String t_char_ff() { return ts(C42.class, AChar.class); }
    public static String h_char_ff() { return hs(C42.class, AChar.class); }
    @AChar(v = '\u0000') static class C43 {}
    public static String t_char_nul() { return ts(C43.class, AChar.class); }
    public static String h_char_nul() { return hs(C43.class, AChar.class); }
    @AChar(v = '\u0001') static class C44 {}
    public static String t_char_c1() { return ts(C44.class, AChar.class); }
    public static String h_char_c1() { return hs(C44.class, AChar.class); }
    @AChar(v = '\u007f') static class C45 {}
    public static String t_char_del() { return ts(C45.class, AChar.class); }
    public static String h_char_del() { return hs(C45.class, AChar.class); }
    @AChar(v = '\u0080') static class C46 {}
    public static String t_char_c80() { return ts(C46.class, AChar.class); }
    public static String h_char_c80() { return hs(C46.class, AChar.class); }
    @AChar(v = 'é') static class C47 {}
    public static String t_char_e() { return ts(C47.class, AChar.class); }
    public static String h_char_e() { return hs(C47.class, AChar.class); }
    @AChar(v = '€') static class C48 {}
    public static String t_char_euro() { return ts(C48.class, AChar.class); }
    public static String h_char_euro() { return hs(C48.class, AChar.class); }
    @AChar(v = '\uffff') static class C49 {}
    public static String t_char_ffff() { return ts(C49.class, AChar.class); }
    public static String h_char_ffff() { return hs(C49.class, AChar.class); }
    @AChar(v = ' ') static class C50 {}
    public static String t_char_sp() { return ts(C50.class, AChar.class); }
    public static String h_char_sp() { return hs(C50.class, AChar.class); }
    @AChar(v = '0') static class C51 {}
    public static String t_char_zero() { return ts(C51.class, AChar.class); }
    public static String h_char_zero() { return hs(C51.class, AChar.class); }
    @AChar(v = '\u2028') static class C52 {}
    public static String t_char_sep() { return ts(C52.class, AChar.class); }
    public static String h_char_sep() { return hs(C52.class, AChar.class); }
    @AInt(v = 0) static class C53 {}
    public static String t_int_zero() { return ts(C53.class, AInt.class); }
    public static String h_int_zero() { return hs(C53.class, AInt.class); }
    @AInt(v = -1) static class C54 {}
    public static String t_int_neg() { return ts(C54.class, AInt.class); }
    public static String h_int_neg() { return hs(C54.class, AInt.class); }
    @AInt(v = Integer.MIN_VALUE) static class C55 {}
    public static String t_int_min() { return ts(C55.class, AInt.class); }
    public static String h_int_min() { return hs(C55.class, AInt.class); }
    @AInt(v = Integer.MAX_VALUE) static class C56 {}
    public static String t_int_max() { return ts(C56.class, AInt.class); }
    public static String h_int_max() { return hs(C56.class, AInt.class); }
    @AInt(v = 123456789) static class C57 {}
    public static String t_int_big() { return ts(C57.class, AInt.class); }
    public static String h_int_big() { return hs(C57.class, AInt.class); }
    @ALong(v = 0L) static class C58 {}
    public static String t_long_zero() { return ts(C58.class, ALong.class); }
    public static String h_long_zero() { return hs(C58.class, ALong.class); }
    @ALong(v = -1L) static class C59 {}
    public static String t_long_neg() { return ts(C59.class, ALong.class); }
    public static String h_long_neg() { return hs(C59.class, ALong.class); }
    @ALong(v = Long.MIN_VALUE) static class C60 {}
    public static String t_long_min() { return ts(C60.class, ALong.class); }
    public static String h_long_min() { return hs(C60.class, ALong.class); }
    @ALong(v = Long.MAX_VALUE) static class C61 {}
    public static String t_long_max() { return ts(C61.class, ALong.class); }
    public static String h_long_max() { return hs(C61.class, ALong.class); }
    @ALong(v = 1234567890123L) static class C62 {}
    public static String t_long_big() { return ts(C62.class, ALong.class); }
    public static String h_long_big() { return hs(C62.class, ALong.class); }
    @AFloat(v = 1.0f) static class C63 {}
    public static String t_float_one() { return ts(C63.class, AFloat.class); }
    public static String h_float_one() { return hs(C63.class, AFloat.class); }
    @AFloat(v = -0.5f) static class C64 {}
    public static String t_float_neg() { return ts(C64.class, AFloat.class); }
    public static String h_float_neg() { return hs(C64.class, AFloat.class); }
    @AFloat(v = 0.0f) static class C65 {}
    public static String t_float_zero() { return ts(C65.class, AFloat.class); }
    public static String h_float_zero() { return hs(C65.class, AFloat.class); }
    @AFloat(v = -0.0f) static class C66 {}
    public static String t_float_nzero() { return ts(C66.class, AFloat.class); }
    public static String h_float_nzero() { return hs(C66.class, AFloat.class); }
    @AFloat(v = 0.1f) static class C67 {}
    public static String t_float_p01() { return ts(C67.class, AFloat.class); }
    public static String h_float_p01() { return hs(C67.class, AFloat.class); }
    @AFloat(v = 1.0E10f) static class C68 {}
    public static String t_float_e10() { return ts(C68.class, AFloat.class); }
    public static String h_float_e10() { return hs(C68.class, AFloat.class); }
    @AFloat(v = 1.0E-10f) static class C69 {}
    public static String t_float_em10() { return ts(C69.class, AFloat.class); }
    public static String h_float_em10() { return hs(C69.class, AFloat.class); }
    @AFloat(v = Float.MIN_VALUE) static class C70 {}
    public static String t_float_min() { return ts(C70.class, AFloat.class); }
    public static String h_float_min() { return hs(C70.class, AFloat.class); }
    @AFloat(v = Float.MIN_NORMAL) static class C71 {}
    public static String t_float_minn() { return ts(C71.class, AFloat.class); }
    public static String h_float_minn() { return hs(C71.class, AFloat.class); }
    @AFloat(v = Float.MAX_VALUE) static class C72 {}
    public static String t_float_max() { return ts(C72.class, AFloat.class); }
    public static String h_float_max() { return hs(C72.class, AFloat.class); }
    @AFloat(v = Float.NaN) static class C73 {}
    public static String t_float_nan() { return ts(C73.class, AFloat.class); }
    public static String h_float_nan() { return hs(C73.class, AFloat.class); }
    @AFloat(v = Float.POSITIVE_INFINITY) static class C74 {}
    public static String t_float_inf() { return ts(C74.class, AFloat.class); }
    public static String h_float_inf() { return hs(C74.class, AFloat.class); }
    @AFloat(v = Float.NEGATIVE_INFINITY) static class C75 {}
    public static String t_float_ninf() { return ts(C75.class, AFloat.class); }
    public static String h_float_ninf() { return hs(C75.class, AFloat.class); }
    @AFloat(v = 100.0f) static class C76 {}
    public static String t_float_h() { return ts(C76.class, AFloat.class); }
    public static String h_float_h() { return hs(C76.class, AFloat.class); }
    @AFloat(v = 1.0E7f) static class C77 {}
    public static String t_float_m() { return ts(C77.class, AFloat.class); }
    public static String h_float_m() { return hs(C77.class, AFloat.class); }
    @AFloat(v = 9999999.0f) static class C78 {}
    public static String t_float_m2() { return ts(C78.class, AFloat.class); }
    public static String h_float_m2() { return hs(C78.class, AFloat.class); }
    @AFloat(v = 0.001f) static class C79 {}
    public static String t_float_m3() { return ts(C79.class, AFloat.class); }
    public static String h_float_m3() { return hs(C79.class, AFloat.class); }
    @AFloat(v = 1.0E-4f) static class C80 {}
    public static String t_float_m4() { return ts(C80.class, AFloat.class); }
    public static String h_float_m4() { return hs(C80.class, AFloat.class); }
    @AFloat(v = (1.0f/3)) static class C81 {}
    public static String t_float_third() { return ts(C81.class, AFloat.class); }
    public static String h_float_third() { return hs(C81.class, AFloat.class); }
    @AFloat(v = 3.1415927f) static class C82 {}
    public static String t_float_pi() { return ts(C82.class, AFloat.class); }
    public static String h_float_pi() { return hs(C82.class, AFloat.class); }
    @AFloat(v = 16777216.0f) static class C83 {}
    public static String t_float_big() { return ts(C83.class, AFloat.class); }
    public static String h_float_big() { return hs(C83.class, AFloat.class); }
    @ADouble(v = 1.0) static class C84 {}
    public static String t_double_one() { return ts(C84.class, ADouble.class); }
    public static String h_double_one() { return hs(C84.class, ADouble.class); }
    @ADouble(v = -0.5) static class C85 {}
    public static String t_double_neg() { return ts(C85.class, ADouble.class); }
    public static String h_double_neg() { return hs(C85.class, ADouble.class); }
    @ADouble(v = 0.0) static class C86 {}
    public static String t_double_zero() { return ts(C86.class, ADouble.class); }
    public static String h_double_zero() { return hs(C86.class, ADouble.class); }
    @ADouble(v = -0.0) static class C87 {}
    public static String t_double_nzero() { return ts(C87.class, ADouble.class); }
    public static String h_double_nzero() { return hs(C87.class, ADouble.class); }
    @ADouble(v = 0.1) static class C88 {}
    public static String t_double_p01() { return ts(C88.class, ADouble.class); }
    public static String h_double_p01() { return hs(C88.class, ADouble.class); }
    @ADouble(v = 1.0E10) static class C89 {}
    public static String t_double_e10() { return ts(C89.class, ADouble.class); }
    public static String h_double_e10() { return hs(C89.class, ADouble.class); }
    @ADouble(v = 1.0E-10) static class C90 {}
    public static String t_double_em10() { return ts(C90.class, ADouble.class); }
    public static String h_double_em10() { return hs(C90.class, ADouble.class); }
    @ADouble(v = Double.MIN_VALUE) static class C91 {}
    public static String t_double_min() { return ts(C91.class, ADouble.class); }
    public static String h_double_min() { return hs(C91.class, ADouble.class); }
    @ADouble(v = Double.MIN_NORMAL) static class C92 {}
    public static String t_double_minn() { return ts(C92.class, ADouble.class); }
    public static String h_double_minn() { return hs(C92.class, ADouble.class); }
    @ADouble(v = Double.MAX_VALUE) static class C93 {}
    public static String t_double_max() { return ts(C93.class, ADouble.class); }
    public static String h_double_max() { return hs(C93.class, ADouble.class); }
    @ADouble(v = Double.NaN) static class C94 {}
    public static String t_double_nan() { return ts(C94.class, ADouble.class); }
    public static String h_double_nan() { return hs(C94.class, ADouble.class); }
    @ADouble(v = Double.POSITIVE_INFINITY) static class C95 {}
    public static String t_double_inf() { return ts(C95.class, ADouble.class); }
    public static String h_double_inf() { return hs(C95.class, ADouble.class); }
    @ADouble(v = Double.NEGATIVE_INFINITY) static class C96 {}
    public static String t_double_ninf() { return ts(C96.class, ADouble.class); }
    public static String h_double_ninf() { return hs(C96.class, ADouble.class); }
    @ADouble(v = 100.0) static class C97 {}
    public static String t_double_h() { return ts(C97.class, ADouble.class); }
    public static String h_double_h() { return hs(C97.class, ADouble.class); }
    @ADouble(v = 1.0E7) static class C98 {}
    public static String t_double_m() { return ts(C98.class, ADouble.class); }
    public static String h_double_m() { return hs(C98.class, ADouble.class); }
    @ADouble(v = 9999999.0) static class C99 {}
    public static String t_double_m2() { return ts(C99.class, ADouble.class); }
    public static String h_double_m2() { return hs(C99.class, ADouble.class); }
    @ADouble(v = 0.001) static class C100 {}
    public static String t_double_m3() { return ts(C100.class, ADouble.class); }
    public static String h_double_m3() { return hs(C100.class, ADouble.class); }
    @ADouble(v = 1.0E-4) static class C101 {}
    public static String t_double_m4() { return ts(C101.class, ADouble.class); }
    public static String h_double_m4() { return hs(C101.class, ADouble.class); }
    @ADouble(v = (1.0/3)) static class C102 {}
    public static String t_double_third() { return ts(C102.class, ADouble.class); }
    public static String h_double_third() { return hs(C102.class, ADouble.class); }
    @ADouble(v = Math.PI) static class C103 {}
    public static String t_double_pi() { return ts(C103.class, ADouble.class); }
    public static String h_double_pi() { return hs(C103.class, ADouble.class); }
    @ADouble(v = 123456789012345680.0) static class C104 {}
    public static String t_double_big() { return ts(C104.class, ADouble.class); }
    public static String h_double_big() { return hs(C104.class, ADouble.class); }
    @ADouble(v = 2.0E-3) static class C105 {}
    public static String t_double_p2() { return ts(C105.class, ADouble.class); }
    public static String h_double_p2() { return hs(C105.class, ADouble.class); }
    @ABool(v = true) static class C106 {}
    public static String t_bool_t() { return ts(C106.class, ABool.class); }
    public static String h_bool_t() { return hs(C106.class, ABool.class); }
    @ABool(v = false) static class C107 {}
    public static String t_bool_f() { return ts(C107.class, ABool.class); }
    public static String h_bool_f() { return hs(C107.class, ABool.class); }
    @AByte(v = (byte) 0) static class C108 {}
    public static String t_byte_zero() { return ts(C108.class, AByte.class); }
    public static String h_byte_zero() { return hs(C108.class, AByte.class); }
    @AByte(v = (byte) 1) static class C109 {}
    public static String t_byte_one() { return ts(C109.class, AByte.class); }
    public static String h_byte_one() { return hs(C109.class, AByte.class); }
    @AByte(v = (byte) -1) static class C110 {}
    public static String t_byte_neg() { return ts(C110.class, AByte.class); }
    public static String h_byte_neg() { return hs(C110.class, AByte.class); }
    @AByte(v = Byte.MAX_VALUE) static class C111 {}
    public static String t_byte_max() { return ts(C111.class, AByte.class); }
    public static String h_byte_max() { return hs(C111.class, AByte.class); }
    @AByte(v = Byte.MIN_VALUE) static class C112 {}
    public static String t_byte_min() { return ts(C112.class, AByte.class); }
    public static String h_byte_min() { return hs(C112.class, AByte.class); }
    @AByte(v = (byte) 16) static class C113 {}
    public static String t_byte_sixteen() { return ts(C113.class, AByte.class); }
    public static String h_byte_sixteen() { return hs(C113.class, AByte.class); }
    @AByte(v = (byte) 0xff) static class C114 {}
    public static String t_byte_ff() { return ts(C114.class, AByte.class); }
    public static String h_byte_ff() { return hs(C114.class, AByte.class); }
    @AShort(v = (short) 0) static class C115 {}
    public static String t_short_zero() { return ts(C115.class, AShort.class); }
    public static String h_short_zero() { return hs(C115.class, AShort.class); }
    @AShort(v = (short) 1) static class C116 {}
    public static String t_short_one() { return ts(C116.class, AShort.class); }
    public static String h_short_one() { return hs(C116.class, AShort.class); }
    @AShort(v = (short) -1) static class C117 {}
    public static String t_short_neg() { return ts(C117.class, AShort.class); }
    public static String h_short_neg() { return hs(C117.class, AShort.class); }
    @AShort(v = Short.MAX_VALUE) static class C118 {}
    public static String t_short_max() { return ts(C118.class, AShort.class); }
    public static String h_short_max() { return hs(C118.class, AShort.class); }
    @AShort(v = Short.MIN_VALUE) static class C119 {}
    public static String t_short_min() { return ts(C119.class, AShort.class); }
    public static String h_short_min() { return hs(C119.class, AShort.class); }
    @AClass(v = String.class) static class C120 {}
    public static String t_class_string() { return ts(C120.class, AClass.class); }
    @AClass(v = int.class) static class C121 {}
    public static String t_class_int() { return ts(C121.class, AClass.class); }
    @AClass(v = void.class) static class C122 {}
    public static String t_class_void() { return ts(C122.class, AClass.class); }
    @AClass(v = int[].class) static class C123 {}
    public static String t_class_arr() { return ts(C123.class, AClass.class); }
    @AClass(v = String[][].class) static class C124 {}
    public static String t_class_arr2() { return ts(C124.class, AClass.class); }
    @AClass(v = Inner.class) static class C125 {}
    public static String t_class_nested() { return ts(C125.class, AClass.class); }
    @AClass(v = Inner.Deep.class) static class C126 {}
    public static String t_class_deep() { return ts(C126.class, AClass.class); }
    @AClass(v = PAnno.class) static class C127 {}
    public static String t_class_top() { return ts(C127.class, AClass.class); }
    @AClass(v = java.util.Map.Entry.class) static class C128 {}
    public static String t_class_entry() { return ts(C128.class, AClass.class); }
    @AClass(v = PAnno.class) static class C129 {}
    public static String t_class_anno() { return ts(C129.class, AClass.class); }
    @AClass(v = boolean.class) static class C130 {}
    public static String t_class_boolean() { return ts(C130.class, AClass.class); }
    @AClass(v = byte[].class) static class C131 {}
    public static String t_class_bytearr() { return ts(C131.class, AClass.class); }
    @AClass(v = E.class) static class C132 {}
    public static String t_class_enum() { return ts(C132.class, AClass.class); }
    @AClass(v = Inner[].class) static class C133 {}
    public static String t_class_innerarr() { return ts(C133.class, AClass.class); }
    @AClass(v = Object.class) static class C134 {}
    public static String t_class_object() { return ts(C134.class, AClass.class); }
    @AEnum(v = E.A) static class C135 {}
    public static String t_enum_a() { return ts(C135.class, AEnum.class); }
    @AEnum(v = E.B) static class C136 {}
    public static String t_enum_b() { return ts(C136.class, AEnum.class); }
    @AEnum(v = E.C) static class C137 {}
    public static String t_enum_body() { return ts(C137.class, AEnum.class); }
    @ATopEnum(v = TopE.X) static class C138 {}
    public static String t_enumtop() { return ts(C138.class, ATopEnum.class); }
    @AEnumJ(v = java.util.concurrent.TimeUnit.SECONDS) static class C139 {}
    public static String t_enum_jdk() { return ts(C139.class, AEnumJ.class); }
    @AStr(v = "a\ufffdb") static class C140 {}
    public static String t_str_fffd() { return ts(C140.class, AStr.class); }
    public static String h_str_fffd() { return hs(C140.class, AStr.class); }
    @AStr(v = "a\ud800b") static class C141 {}
    public static String t_str_lone() { return ts(C141.class, AStr.class); }
    public static String h_str_lone() { return hs(C141.class, AStr.class); }
    @AStr(v = "\udc00\ud800") static class C142 {}
    public static String t_str_lone2() { return ts(C142.class, AStr.class); }
    public static String h_str_lone2() { return hs(C142.class, AStr.class); }
    @AChar(v = '\ud800') static class C143 {}
    public static String t_char_lone() { return ts(C143.class, AChar.class); }
    public static String h_char_lone() { return hs(C143.class, AChar.class); }
    @AChar(v = '\udfff') static class C144 {}
    public static String t_char_lone2() { return ts(C144.class, AChar.class); }
    public static String h_char_lone2() { return hs(C144.class, AChar.class); }
    @AChar(v = '\ufffd') static class C145 {}
    public static String t_char_fffd() { return ts(C145.class, AChar.class); }
    public static String h_char_fffd() { return hs(C145.class, AChar.class); }
    @ANest(v = @PAnno("n")) static class C146 {}
    public static String t_nest_p() { return ts(C146.class, ANest.class); }
    public static String h_nest_p() { return hs(C146.class, ANest.class); }
    @ANest(v = @PAnno("")) static class C147 {}
    public static String t_nest_empty() { return ts(C147.class, ANest.class); }
    public static String h_nest_empty() { return hs(C147.class, ANest.class); }
    @ANest(v = @PAnno("a\"b")) static class C148 {}
    public static String t_nest_esc() { return ts(C148.class, ANest.class); }
    public static String h_nest_esc() { return hs(C148.class, ANest.class); }
    @SStr(v = {}) static class C149 {}
    public static String t_sstr_empty() { return ts(C149.class, SStr.class); }
    public static String h_sstr_empty() { return hs(C149.class, SStr.class); }
    @SStr(v = {"a"}) static class C150 {}
    public static String t_sstr_one() { return ts(C150.class, SStr.class); }
    public static String h_sstr_one() { return hs(C150.class, SStr.class); }
    @SStr(v = {"a", "b"}) static class C151 {}
    public static String t_sstr_two() { return ts(C151.class, SStr.class); }
    public static String h_sstr_two() { return hs(C151.class, SStr.class); }
    @SStr(v = {"q\"", "\n", "é"}) static class C152 {}
    public static String t_sstr_esc() { return ts(C152.class, SStr.class); }
    public static String h_sstr_esc() { return hs(C152.class, SStr.class); }
    @SStr(v = {"x", "", "z"}) static class C153 {}
    public static String t_sstr_three() { return ts(C153.class, SStr.class); }
    public static String h_sstr_three() { return hs(C153.class, SStr.class); }
    @SChar(v = {}) static class C154 {}
    public static String t_schar_empty() { return ts(C154.class, SChar.class); }
    public static String h_schar_empty() { return hs(C154.class, SChar.class); }
    @SChar(v = {'a'}) static class C155 {}
    public static String t_schar_one() { return ts(C155.class, SChar.class); }
    public static String h_schar_one() { return hs(C155.class, SChar.class); }
    @SChar(v = {'a', '\n'}) static class C156 {}
    public static String t_schar_two() { return ts(C156.class, SChar.class); }
    public static String h_schar_two() { return hs(C156.class, SChar.class); }
    @SChar(v = {'\'', '"'}) static class C157 {}
    public static String t_schar_q() { return ts(C157.class, SChar.class); }
    public static String h_schar_q() { return hs(C157.class, SChar.class); }
    @SInt(v = {}) static class C158 {}
    public static String t_sint_empty() { return ts(C158.class, SInt.class); }
    public static String h_sint_empty() { return hs(C158.class, SInt.class); }
    @SInt(v = {1}) static class C159 {}
    public static String t_sint_one() { return ts(C159.class, SInt.class); }
    public static String h_sint_one() { return hs(C159.class, SInt.class); }
    @SInt(v = {1, 2}) static class C160 {}
    public static String t_sint_two() { return ts(C160.class, SInt.class); }
    public static String h_sint_two() { return hs(C160.class, SInt.class); }
    @SInt(v = {-1, 0, Integer.MIN_VALUE}) static class C161 {}
    public static String t_sint_neg() { return ts(C161.class, SInt.class); }
    public static String h_sint_neg() { return hs(C161.class, SInt.class); }
    @SLong(v = {}) static class C162 {}
    public static String t_slong_empty() { return ts(C162.class, SLong.class); }
    public static String h_slong_empty() { return hs(C162.class, SLong.class); }
    @SLong(v = {1L}) static class C163 {}
    public static String t_slong_one() { return ts(C163.class, SLong.class); }
    public static String h_slong_one() { return hs(C163.class, SLong.class); }
    @SLong(v = {1L, -2L}) static class C164 {}
    public static String t_slong_two() { return ts(C164.class, SLong.class); }
    public static String h_slong_two() { return hs(C164.class, SLong.class); }
    @SFloat(v = {}) static class C165 {}
    public static String t_sfloat_empty() { return ts(C165.class, SFloat.class); }
    public static String h_sfloat_empty() { return hs(C165.class, SFloat.class); }
    @SFloat(v = {1.5f}) static class C166 {}
    public static String t_sfloat_one() { return ts(C166.class, SFloat.class); }
    public static String h_sfloat_one() { return hs(C166.class, SFloat.class); }
    @SFloat(v = {1.5f, Float.NaN}) static class C167 {}
    public static String t_sfloat_two() { return ts(C167.class, SFloat.class); }
    public static String h_sfloat_two() { return hs(C167.class, SFloat.class); }
    @SFloat(v = {Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY}) static class C168 {}
    public static String t_sfloat_inf() { return ts(C168.class, SFloat.class); }
    public static String h_sfloat_inf() { return hs(C168.class, SFloat.class); }
    @SDouble(v = {}) static class C169 {}
    public static String t_sdouble_empty() { return ts(C169.class, SDouble.class); }
    public static String h_sdouble_empty() { return hs(C169.class, SDouble.class); }
    @SDouble(v = {1.5}) static class C170 {}
    public static String t_sdouble_one() { return ts(C170.class, SDouble.class); }
    public static String h_sdouble_one() { return hs(C170.class, SDouble.class); }
    @SDouble(v = {1.5, Double.NaN}) static class C171 {}
    public static String t_sdouble_two() { return ts(C171.class, SDouble.class); }
    public static String h_sdouble_two() { return hs(C171.class, SDouble.class); }
    @SDouble(v = {Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY}) static class C172 {}
    public static String t_sdouble_inf() { return ts(C172.class, SDouble.class); }
    public static String h_sdouble_inf() { return hs(C172.class, SDouble.class); }
    @SBool(v = {}) static class C173 {}
    public static String t_sbool_empty() { return ts(C173.class, SBool.class); }
    public static String h_sbool_empty() { return hs(C173.class, SBool.class); }
    @SBool(v = {true}) static class C174 {}
    public static String t_sbool_one() { return ts(C174.class, SBool.class); }
    public static String h_sbool_one() { return hs(C174.class, SBool.class); }
    @SBool(v = {true, false}) static class C175 {}
    public static String t_sbool_two() { return ts(C175.class, SBool.class); }
    public static String h_sbool_two() { return hs(C175.class, SBool.class); }
    @SByte(v = {}) static class C176 {}
    public static String t_sbyte_empty() { return ts(C176.class, SByte.class); }
    public static String h_sbyte_empty() { return hs(C176.class, SByte.class); }
    @SByte(v = {1}) static class C177 {}
    public static String t_sbyte_one() { return ts(C177.class, SByte.class); }
    public static String h_sbyte_one() { return hs(C177.class, SByte.class); }
    @SByte(v = {1, -1}) static class C178 {}
    public static String t_sbyte_two() { return ts(C178.class, SByte.class); }
    public static String h_sbyte_two() { return hs(C178.class, SByte.class); }
    @SByte(v = {(byte) 0x7f, (byte) 0x80}) static class C179 {}
    public static String t_sbyte_ex() { return ts(C179.class, SByte.class); }
    public static String h_sbyte_ex() { return hs(C179.class, SByte.class); }
    @SShort(v = {}) static class C180 {}
    public static String t_sshort_empty() { return ts(C180.class, SShort.class); }
    public static String h_sshort_empty() { return hs(C180.class, SShort.class); }
    @SShort(v = {1}) static class C181 {}
    public static String t_sshort_one() { return ts(C181.class, SShort.class); }
    public static String h_sshort_one() { return hs(C181.class, SShort.class); }
    @SShort(v = {1, -1}) static class C182 {}
    public static String t_sshort_two() { return ts(C182.class, SShort.class); }
    public static String h_sshort_two() { return hs(C182.class, SShort.class); }
    @SClass(v = {}) static class C183 {}
    public static String t_sclass_empty() { return ts(C183.class, SClass.class); }
    @SClass(v = {String.class}) static class C184 {}
    public static String t_sclass_one() { return ts(C184.class, SClass.class); }
    @SClass(v = {String.class, int.class}) static class C185 {}
    public static String t_sclass_two() { return ts(C185.class, SClass.class); }
    @SClass(v = {int[].class, Inner.class, void.class}) static class C186 {}
    public static String t_sclass_mix() { return ts(C186.class, SClass.class); }
    @SEnum(v = {}) static class C187 {}
    public static String t_senum_empty() { return ts(C187.class, SEnum.class); }
    @SEnum(v = {E.A}) static class C188 {}
    public static String t_senum_one() { return ts(C188.class, SEnum.class); }
    @SEnum(v = {E.A, E.B}) static class C189 {}
    public static String t_senum_two() { return ts(C189.class, SEnum.class); }
    @SNest(v = {}) static class C190 {}
    public static String t_snest_empty() { return ts(C190.class, SNest.class); }
    public static String h_snest_empty() { return hs(C190.class, SNest.class); }
    @SNest(v = {@PAnno("a")}) static class C191 {}
    public static String t_snest_one() { return ts(C191.class, SNest.class); }
    public static String h_snest_one() { return hs(C191.class, SNest.class); }
    @SNest(v = {@PAnno("a"), @PAnno("b")}) static class C192 {}
    public static String t_snest_two() { return ts(C192.class, SNest.class); }
    public static String h_snest_two() { return hs(C192.class, SNest.class); }
    @SStr(v = "solo") static class C193 {}
    public static String t_sstr_bare() { return ts(C193.class, SStr.class); }
    public static String h_sstr_bare() { return hs(C193.class, SStr.class); }
    @SInt(v = 4) static class C194 {}
    public static String t_sint_bare() { return ts(C194.class, SInt.class); }
    public static String h_sint_bare() { return hs(C194.class, SInt.class); }
    @SClass(v = String.class) static class C195 {}
    public static String t_sclass_bare() { return ts(C195.class, SClass.class); }
    @VStr("marked") static class C196 {}
    public static String t_val_str() { return ts(C196.class, VStr.class); }
    public static String h_val_str() { return hs(C196.class, VStr.class); }
    @VStr(value = "marked") static class C197 {}
    public static String t_val_strkw() { return ts(C197.class, VStr.class); }
    public static String h_val_strkw() { return hs(C197.class, VStr.class); }
    @VInt(7) static class C198 {}
    public static String t_val_int() { return ts(C198.class, VInt.class); }
    public static String h_val_int() { return hs(C198.class, VInt.class); }
    @VArr("a") static class C199 {}
    public static String t_val_arr1() { return ts(C199.class, VArr.class); }
    public static String h_val_arr1() { return hs(C199.class, VArr.class); }
    @VArr({"a", "b"}) static class C200 {}
    public static String t_val_arr2() { return ts(C200.class, VArr.class); }
    public static String h_val_arr2() { return hs(C200.class, VArr.class); }
    @VArr({}) static class C201 {}
    public static String t_val_arr0() { return ts(C201.class, VArr.class); }
    public static String h_val_arr0() { return hs(C201.class, VArr.class); }
    @VEnum(E.A) static class C202 {}
    public static String t_val_enum() { return ts(C202.class, VEnum.class); }
    @VNest(@PAnno("z")) static class C203 {}
    public static String t_val_nest() { return ts(C203.class, VNest.class); }
    public static String h_val_nest() { return hs(C203.class, VNest.class); }
    @VDef static class C204 {}
    public static String t_val_def() { return ts(C204.class, VDef.class); }
    public static String h_val_def() { return hs(C204.class, VDef.class); }
    @VDef("over") static class C205 {}
    public static String t_val_def2() { return ts(C205.class, VDef.class); }
    public static String h_val_def2() { return hs(C205.class, VDef.class); }
    @VDefArr static class C206 {}
    public static String t_val_defarr() { return ts(C206.class, VDefArr.class); }
    public static String h_val_defarr() { return hs(C206.class, VDefArr.class); }
    @VDefArr({"p", "q"}) static class C207 {}
    public static String t_val_defarr2() { return ts(C207.class, VDefArr.class); }
    public static String h_val_defarr2() { return hs(C207.class, VDefArr.class); }
    @OneV static class C208 {}
    public static String t_onev_def() { return ts(C208.class, OneV.class); }
    public static String h_onev_def() { return hs(C208.class, OneV.class); }
    @OneV(v = "set") static class C209 {}
    public static String t_onev_set() { return ts(C209.class, OneV.class); }
    public static String h_onev_set() { return hs(C209.class, OneV.class); }
    @TwoV(value = "t") static class C210 {}
    public static String t_twov() { return ts(C210.class, TwoV.class); }
    public static String h_twov() { return hs(C210.class, TwoV.class); }
    @TwoV(value = "t", n = 2) static class C211 {}
    public static String t_twov2() { return ts(C211.class, TwoV.class); }
    public static String h_twov2() { return hs(C211.class, TwoV.class); }
    @TwoVd static class C212 {}
    public static String t_twovd() { return ts(C212.class, TwoVd.class); }
    public static String h_twovd() { return hs(C212.class, TwoVd.class); }
    @TwoVd(n = 9) static class C213 {}
    public static String t_twovd2() { return ts(C213.class, TwoVd.class); }
    public static String h_twovd2() { return hs(C213.class, TwoVd.class); }
    @TwoVd(value = "w") static class C214 {}
    public static String t_twovd3() { return ts(C214.class, TwoVd.class); }
    public static String h_twovd3() { return hs(C214.class, TwoVd.class); }
    @Req2(a = "hi", b = 3) static class C215 {}
    public static String t_req2() { return ts(C215.class, Req2.class); }
    public static String h_req2() { return hs(C215.class, Req2.class); }
    @Req2(b = 3, a = "hi") static class C216 {}
    public static String t_req2r() { return ts(C216.class, Req2.class); }
    public static String h_req2r() { return hs(C216.class, Req2.class); }
    @Req3(a = "x", b = 1, c = 2L) static class C217 {}
    public static String t_req3() { return ts(C217.class, Req3.class); }
    public static String h_req3() { return hs(C217.class, Req3.class); }
    @Req3(c = 2L, b = 1, a = "x") static class C218 {}
    public static String t_req3r() { return ts(C218.class, Req3.class); }
    public static String h_req3r() { return hs(C218.class, Req3.class); }
    @Req3(b = 1, c = 2L, a = "x") static class C219 {}
    public static String t_req3m() { return ts(C219.class, Req3.class); }
    public static String h_req3m() { return hs(C219.class, Req3.class); }
    @NoEl static class C220 {}
    public static String t_noel() { return ts(C220.class, NoEl.class); }
    public static String h_noel() { return hs(C220.class, NoEl.class); }
    @Const static class C221 {}
    public static String t_const() { return ts(C221.class, Const.class); }
    public static String h_const() { return hs(C221.class, Const.class); }
    @Const(v = 3) static class C222 {}
    public static String t_const2() { return ts(C222.class, Const.class); }
    public static String h_const2() { return hs(C222.class, Const.class); }
    @Many static class C223 {}
    public static String t_many0() { return ts(C223.class, Many.class); }
    public static String h_many0() { return hs(C223.class, Many.class); }
    @Many(mu = 100) static class C224 {}
    public static String t_many1() { return ts(C224.class, Many.class); }
    public static String h_many1() { return hs(C224.class, Many.class); }
    @Many(zeta = 100, alpha = 200) static class C225 {}
    public static String t_many2() { return ts(C225.class, Many.class); }
    public static String h_many2() { return hs(C225.class, Many.class); }
    @Many(lambda = 1, kappa = 2, iota = 3, theta = 4, eta = 5, zeta = 6, epsilon = 7, delta = 8, gamma = 9, beta = 10, alpha = 11, mu = 12) static class C226 {}
    public static String t_many3() { return ts(C226.class, Many.class); }
    public static String h_many3() { return hs(C226.class, Many.class); }
    @Mix(req = "r", zed = 1L, mm = true) static class C227 {}
    public static String t_mix0() { return ts(C227.class, Mix.class); }
    public static String h_mix0() { return hs(C227.class, Mix.class); }
    @Mix(mm = false, zed = 2L, req = "q") static class C228 {}
    public static String t_mix1() { return ts(C228.class, Mix.class); }
    public static String h_mix1() { return hs(C228.class, Mix.class); }
    @Mix(zed = 3L, abc = "A", mm = true, req = "x", dflt = 4) static class C229 {}
    public static String t_mix2() { return ts(C229.class, Mix.class); }
    public static String h_mix2() { return hs(C229.class, Mix.class); }
    @Reps({@Rep(1), @Rep(2)}) static class C230 {}
    public static String t_rep() { return ts(C230.class, Reps.class); }
    public static String h_rep() { return hs(C230.class, Reps.class); }
    @Reps({@Rep(1)}) static class C231 {}
    public static String t_rep1() { return ts(C231.class, Reps.class); }
    public static String h_rep1() { return hs(C231.class, Reps.class); }
    @Reps({}) static class C232 {}
    public static String t_rep0() { return ts(C232.class, Reps.class); }
    public static String h_rep0() { return hs(C232.class, Reps.class); }
    @Dual static class C233 {}
    public static String t_dual0() { return ts(C233.class, Dual.class); }
    @Dual(s = "z") static class C234 {}
    public static String t_dual1() { return ts(C234.class, Dual.class); }
    @Dual(a = {3}, e = E.A) static class C235 {}
    public static String t_dual2() { return ts(C235.class, Dual.class); }
    @Wrap(one = @PAnno("o"), deep = @ANest(v = @PAnno("d"))) static class C236 {}
    public static String t_wrap0() { return ts(C236.class, Wrap.class); }
    public static String h_wrap0() { return hs(C236.class, Wrap.class); }
    @Wrap(many = {@PAnno("m1"), @PAnno("m2")}, one = @PAnno("o"), deep = @ANest(v = @PAnno("d"))) static class C237 {}
    public static String t_wrap1() { return ts(C237.class, Wrap.class); }
    public static String h_wrap1() { return hs(C237.class, Wrap.class); }

    // ---- hand-written rows ----
    @Inner.Deep("x") static class D1 {}
    @TopAnno static class D2 {}
    @TopAnno(n = -4) static class D2b {}
    @Rep(1) @Rep(2) static class D3 {}
    @Rep(7) static class D4 {}
    @PAnno("m") @MAnno2(a = "k", b = 2) @AInt(v = 5) static class D5 {}
    @MAnno2(a = "hi", b = 3) static class D6 {}
    @MAnno2(b = 3, a = "hi") static class D6r {}
    @Retention(RetentionPolicy.RUNTIME) @interface MAnno2 { String a(); int b(); }

    @AStr(v = "field") public int fld;
    @AStr(v = "method") public void meth(@AStr(v = "param") int p, int q, @PAnno("two") @AInt(v = 1) int r) {}
    @AStr(v = "ctor") public Probe() {}

    public static String t_deep() { return ts(D1.class, Inner.Deep.class); }
    public static String h_deep() { return hs(D1.class, Inner.Deep.class); }
    public static String t_top() { return ts(D2.class, TopAnno.class); }
    public static String h_top() { return hs(D2.class, TopAnno.class); }
    public static String t_top2() { return ts(D2b.class, TopAnno.class); }
    public static String h_top2() { return hs(D2b.class, TopAnno.class); }
    public static String t_repeat() { return ts(D3.class, Reps.class); }
    public static String h_repeat() { return hs(D3.class, Reps.class); }
    public static String t_repeat_one() { return ts(D4.class, Rep.class); }
    public static String h_repeat_one() { return hs(D4.class, Rep.class); }
    public static String t_repeat_none() { return ts(D4.class, Reps.class); }
    public static String t_multi1() { return ts(D5.class, PAnno.class); }
    public static String t_multi2() { return ts(D5.class, MAnno2.class); }
    public static String t_multi3() { return ts(D5.class, AInt.class); }
    public static String t_all() {
        try {
            StringBuilder sb = new StringBuilder();
            for (Annotation a : D5.class.getAnnotations()) sb.append(a.toString()).append(" ; ");
            return sb.toString();
        } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String t_declared() {
        try {
            Annotation[] arr = D5.class.getDeclaredAnnotations();
            return arr.length + ":" + arr[0].toString() + ":" + arr[1].toString() + ":" + arr[2].toString();
        } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String t_field() {
        try { return Probe.class.getField("fld").getAnnotation(AStr.class).toString(); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String t_method() {
        try { return Probe.class.getMethod("meth", int.class, int.class, int.class).getAnnotation(AStr.class).toString(); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String t_ctor() {
        try { return Probe.class.getConstructor().getAnnotation(AStr.class).toString(); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String t_params() {
        try {
            Annotation[][] p = Probe.class.getMethod("meth", int.class, int.class, int.class).getParameterAnnotations();
            return p.length + ":" + p[0].length + ":" + p[0][0].toString() + ":" + p[1].length + ":" + p[2].length + ":" + p[2][0].toString() + ":" + p[2][1].toString();
        } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String t_type() {
        try { return D6.class.getAnnotation(MAnno2.class).annotationType().getName(); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String t_member_order_a() { return ts(D6.class, MAnno2.class); }
    public static String t_member_order_b() { return ts(D6r.class, MAnno2.class); }
    public static String t_valueof() {
        try { return String.valueOf(D6.class.getAnnotation(MAnno2.class)); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String t_concat() {
        try { return "x" + D6.class.getAnnotation(MAnno2.class) + "y"; } catch (Throwable t) { return "EXC " + ex(t); }
    }

    @Deprecated(since = "9") static class J1 {}
    public static String t_jdk_deprecated() { return ts(J1.class, Deprecated.class); }
    @FunctionalInterface interface J2 { void f(); }
    public static String t_jdk_functional() { return ts(J2.class, FunctionalInterface.class); }

    // ---- equals / hashCode ----
    @PAnno("same") static class Q1 {}
    @PAnno("same") static class Q2 {}
    @PAnno("other") static class Q3 {}
    @VDef static class Q4 {}
    @VDef("vd") static class Q5 {}
    @AFloat(v = Float.NaN) static class Q6 {}
    @AFloat(v = Float.NaN) static class Q7 {}
    @AFloat(v = 0.0f) static class Q8 {}
    @AFloat(v = -0.0f) static class Q9 {}
    @ADouble(v = Double.NaN) static class Q10 {}
    @ADouble(v = Double.NaN) static class Q11 {}
    @ADouble(v = 0.0) static class Q12 {}
    @ADouble(v = -0.0) static class Q13 {}
    @SInt(v = {1, 2}) static class Q14 {}
    @SInt(v = {1, 2}) static class Q15 {}
    @SInt(v = {2, 1}) static class Q16 {}
    @SInt(v = {1}) static class Q17 {}
    @ANest(v = @PAnno("n")) static class Q18 {}
    @ANest(v = @PAnno("n")) static class Q19 {}
    @ANest(v = @PAnno("m")) static class Q20 {}
    @AClass(v = String.class) static class Q21 {}
    @AClass(v = String.class) static class Q22 {}
    @AClass(v = Integer.class) static class Q23 {}
    @AEnum(v = E.A) static class Q24 {}
    @AEnum(v = E.A) static class Q25 {}
    @AEnum(v = E.B) static class Q26 {}
    @SStr(v = {"a", "b"}) static class Q27 {}
    @SStr(v = {"a", "b"}) static class Q28 {}
    @SStr(v = {"a", "c"}) static class Q29 {}
    @AStr(v = "same") static class Q30 {}
    @Many(mu = 100) static class Q31 {}
    @Many(mu = 100, alpha = 0) static class Q32 {}
    @Many static class Q33 {}
    @NoEl static class Q34 {}
    @Req2(a = "hi", b = 3) static class Q35 {}
    @Req2(b = 3, a = "hi") static class Q36 {}

    private static Annotation an(Class<?> c, Class<? extends Annotation> a) { return c.getAnnotation(a); }
    private static String eq(Class<?> c1, Class<? extends Annotation> a1, Class<?> c2, Class<? extends Annotation> a2) {
        try {
            Annotation x = an(c1, a1), y = an(c2, a2);
            return b(x.equals(y)) + "/" + b(y.equals(x)) + "/" + b(x.hashCode() == y.hashCode());
        } catch (Throwable t) { return "EXC " + ex(t); }
    }
    private static String eqNoHash(Class<?> c1, Class<? extends Annotation> a1, Class<?> c2, Class<? extends Annotation> a2) {
        try {
            Annotation x = an(c1, a1), y = an(c2, a2);
            return b(x.equals(y)) + "/" + b(y.equals(x));
        } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String x_same() { return eq(Q1.class, PAnno.class, Q2.class, PAnno.class); }
    public static String x_diff() { return eq(Q1.class, PAnno.class, Q3.class, PAnno.class); }
    public static String x_default() { return eq(Q4.class, VDef.class, Q5.class, VDef.class); }
    public static String x_nan_f() { return eq(Q6.class, AFloat.class, Q7.class, AFloat.class); }
    public static String x_zero_f() { return eq(Q8.class, AFloat.class, Q9.class, AFloat.class); }
    public static String x_nan_d() { return eq(Q10.class, ADouble.class, Q11.class, ADouble.class); }
    public static String x_zero_d() { return eq(Q12.class, ADouble.class, Q13.class, ADouble.class); }
    public static String x_arr_same() { return eq(Q14.class, SInt.class, Q15.class, SInt.class); }
    public static String x_arr_perm() { return eq(Q14.class, SInt.class, Q16.class, SInt.class); }
    public static String x_arr_len() { return eq(Q14.class, SInt.class, Q17.class, SInt.class); }
    public static String x_nest_same() { return eq(Q18.class, ANest.class, Q19.class, ANest.class); }
    public static String x_nest_diff() { return eq(Q18.class, ANest.class, Q20.class, ANest.class); }
    public static String x_class_same() { return eqNoHash(Q21.class, AClass.class, Q22.class, AClass.class); }
    public static String x_class_diff() { return eqNoHash(Q21.class, AClass.class, Q23.class, AClass.class); }
    public static String x_enum_same() { return eqNoHash(Q24.class, AEnum.class, Q25.class, AEnum.class); }
    public static String x_enum_diff() { return eqNoHash(Q24.class, AEnum.class, Q26.class, AEnum.class); }
    public static String x_strarr_same() { return eq(Q27.class, SStr.class, Q28.class, SStr.class); }
    public static String x_strarr_diff() { return eq(Q27.class, SStr.class, Q29.class, SStr.class); }
    public static String x_type() { return eqNoHash(Q1.class, PAnno.class, Q30.class, AStr.class); }
    public static String x_many() { return eq(Q31.class, Many.class, Q32.class, Many.class); }
    public static String x_many_def() { return eq(Q31.class, Many.class, Q33.class, Many.class); }
    public static String x_noel() { return eq(Q34.class, NoEl.class, Q34.class, NoEl.class); }
    public static String x_order() { return eq(Q35.class, Req2.class, Q36.class, Req2.class); }
    public static String x_null() {
        try { return b(an(Q1.class, PAnno.class).equals(null)); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String x_string() {
        try { return b(an(Q1.class, PAnno.class).equals("@demo.PAnno(\"same\")")); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String x_self() {
        try { Annotation a = an(Q1.class, PAnno.class); return b(a.equals(a)); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String x_hash_noel() { return hs(Q34.class, NoEl.class); }
    public static String x_hash_eq_str() {
        try { return b(an(Q30.class, AStr.class).hashCode() == an(Q1.class, PAnno.class).hashCode()); } catch (Throwable t) { return "EXC " + ex(t); }
    }
    public static String x_hash_many0() { return hs(Q33.class, Many.class); }
    public static String x_hash_req2() { return hs(Q35.class, Req2.class); }

    /** Prints every `t_`/`h_`/`x_` probe as `label=value`, sorted by label. */
    public static void main(String[] args) throws Exception {
        List<Method> methods = new ArrayList<>();
        for (Method m : Probe.class.getDeclaredMethods()) {
            if (m.getReturnType() == String.class && m.getParameterCount() == 0 && m.getName().matches("[thx]_.*")) methods.add(m);
        }
        methods.sort((p, q) -> p.getName().compareTo(q.getName()));
        for (Method m : methods) {
            System.out.println(m.getName() + "=" + m.invoke(null));
        }
    }
}
