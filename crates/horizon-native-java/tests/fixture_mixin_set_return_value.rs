//! A cancellable Mixin `@Inject` whose callback calls `CallbackInfoReturnable.setReturnValue`.
//!
//! sponge-mixin 0.17.5 (the Mixin Fabric Loader ships): `setReturnValue` calls `super.cancel()`
//! and then stores the value, and the woven `HEAD` injection returns `cir.getReturnValue()` when
//! the callback info is cancelled (unboxed by `getReturnValueI()` and its siblings, which answer
//! 0 for `null`). So `@Inject(method = "number()I", at = @At("HEAD"), cancellable = true)` setting
//! 7 on `int number()` (whose body returns 3) answers 7, through every entry point that runs a
//! `HEAD` plan (`execute_mixin_at_head`, `execute_mixin_chain`, `execute_discovered_mixins`); a
//! bare `cancel()` answers the descriptor's default, 0, and so does `setReturnValue(null)`. A
//! plain `CallbackInfo` handler on a valued target is refused, as Mixin's
//! `Callback.checkDescriptor` refuses it, unless a name-only selector picked the target (Mixin
//! skips a method the handler doesn't fit). At an `@At("INVOKE")` site any cancel returns from
//! the whole target, which the runtime's suppressed-call model doesn't express: counted.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{MethodRef, MixinInjectionPlan, StdHost, Value};

const STUBS: &[(&str, &str)] = &[
    (
        "org/spongepowered/asm/mixin/Mixin.java",
        "package org.spongepowered.asm.mixin;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.CLASS) @Target(ElementType.TYPE)\npublic @interface Mixin { Class<?>[] value() default {}; }",
    ),
    (
        "org/spongepowered/asm/mixin/injection/At.java",
        "package org.spongepowered.asm.mixin.injection;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.RUNTIME) @Target({})\npublic @interface At { String value() default \"\"; String target() default \"\"; }",
    ),
    (
        "org/spongepowered/asm/mixin/injection/Inject.java",
        "package org.spongepowered.asm.mixin.injection;\nimport java.lang.annotation.*;\n@Retention(RetentionPolicy.RUNTIME) @Target(ElementType.METHOD)\npublic @interface Inject { String[] method(); At[] at(); boolean cancellable() default false; }",
    ),
    (
        "org/spongepowered/asm/mixin/injection/callback/CallbackInfo.java",
        "package org.spongepowered.asm.mixin.injection.callback; public class CallbackInfo { public void cancel() {} }",
    ),
    (
        "org/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable.java",
        "package org.spongepowered.asm.mixin.injection.callback; public class CallbackInfoReturnable<R> extends CallbackInfo { public void setReturnValue(R value) {} }",
    ),
    (
        "demo/Target.java",
        "package demo;\npublic class Target {\n  public static int number() { return 3; }\n  public static int plain() { return 4; }\n  public static int helper() { return 5; }\n  public static int viaCall() { return helper() + 1; }\n  public static int nothing() { return 6; }\n  public static int plainInfo() { return 8; }\n  public static int byName() { return 9; }\n}",
    ),
    (
        "demo/mixin/SetMixin.java",
        r#"package demo.mixin;
import demo.Target;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;
@Mixin(Target.class)
public class SetMixin {
  @Inject(method = "number()I", at = @At("HEAD"), cancellable = true)
  private static void seven(CallbackInfoReturnable<Integer> cir) { cir.setReturnValue(7); }

  @Inject(method = "plain()I", at = @At("HEAD"), cancellable = true)
  private static void stop(CallbackInfoReturnable<Integer> cir) { cir.cancel(); }

  @Inject(method = "viaCall", at = @At(value = "INVOKE", target = "Ldemo/Target;helper()I"), cancellable = true)
  private static void atCall(CallbackInfoReturnable<Integer> cir) { cir.setReturnValue(9); }

  @Inject(method = "nothing()I", at = @At("HEAD"), cancellable = true)
  private static void none(CallbackInfoReturnable<Integer> cir) { cir.setReturnValue(null); }
}
"#,
    ),
    (
        "demo/mixin/InfoMixin.java",
        r#"package demo.mixin;
import demo.Target;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
@Mixin(Target.class)
public class InfoMixin {
  @Inject(method = "plainInfo()I", at = @At("HEAD"), cancellable = true)
  private static void exact(CallbackInfo ci) { ci.cancel(); }

  @Inject(method = "byName", at = @At("HEAD"), cancellable = true)
  private static void loose(CallbackInfo ci) { ci.cancel(); }
}
"#,
    ),
];

const MOD_JSON: &str =
    r#"{"schemaVersion":1,"id":"setreturn","version":"1.0.0","mixins":["setreturn.mixins.json"]}"#;
const MIXINS_JSON: &str =
    r#"{"required":true,"package":"demo.mixin","mixins":["SetMixin","InfoMixin"]}"#;

fn fixture() -> javac_fixture::Fixture {
    javac_fixture::Fixture::build_with_mod_json_and_extra(
        "mixin_set_return_value",
        STUBS,
        MOD_JSON,
        &[("setreturn.mixins.json", MIXINS_JSON.as_bytes())],
    )
}

fn target(name: &str) -> MethodRef {
    MethodRef {
        owner: "demo/Target".into(),
        name: name.into(),
        descriptor: "()I".into(),
    }
}

fn plan(fixture: &javac_fixture::Fixture, name: &str) -> MixinInjectionPlan {
    fixture
        .jar
        .mixin_plans_in_apply_order()
        .into_iter()
        .find(|plan| plan.target.name == name)
        .unwrap_or_else(|| panic!("a plan into {name}"))
}

#[test]
fn a_head_set_return_value_is_the_answer_on_every_head_path() {
    let fixture = fixture();
    let jar = &fixture.jar;
    let number = plan(&fixture, "number");
    let plain = plan(&fixture, "plain");

    let mut host = StdHost::new();
    let head = jar
        .execute_mixin_at_head(&number, None, &[], &mut host)
        .unwrap();
    assert!(head.cancelled && head.return_value_overridden);
    assert_eq!(head.target_result, Value::Int(7));

    let mut host = StdHost::new();
    let chain = jar
        .execute_mixin_chain(std::slice::from_ref(&number), None, &[], &mut host)
        .unwrap();
    assert!(chain.cancelled && chain.return_value_overridden);
    assert_eq!(chain.target_result, Value::Int(7));

    let mut host = StdHost::new();
    let discovered = jar
        .execute_discovered_mixins(&target("number"), None, &[], &mut host)
        .unwrap();
    assert!(discovered.cancelled && discovered.return_value_overridden);
    assert_eq!(discovered.target_result, Value::Int(7));

    // A bare `cancel()`: no value was set, the woven method returns `int`'s default.
    for report in [
        jar.execute_mixin_at_head(&plain, None, &[], &mut StdHost::new())
            .unwrap(),
        jar.execute_discovered_mixins(&target("plain"), None, &[], &mut StdHost::new())
            .unwrap(),
    ] {
        assert!(report.cancelled && !report.return_value_overridden);
        assert_eq!(report.target_result, Value::Int(0));
    }
}

#[test]
fn a_set_return_value_at_an_invoke_site_counts() {
    let fixture = fixture();
    let mut host = StdHost::new();
    let report = fixture
        .jar
        .execute_discovered_mixins(&target("viaCall"), None, &[], &mut host)
        .unwrap();
    assert!(report.cancelled);
    assert_eq!(
        host.unknown_host_calls()
            .into_iter()
            .filter(|(key, _)| key.contains("<setReturnValue at INVOKE>"))
            .count(),
        1,
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn set_return_value_null_on_a_primitive_target_answers_zero() {
    let fixture = fixture();
    let plan = plan(&fixture, "nothing");
    let report = fixture
        .jar
        .execute_mixin_at_head(&plan, None, &[], &mut StdHost::new())
        .unwrap();
    assert!(report.cancelled && report.return_value_overridden);
    assert_eq!(report.target_result, Value::Int(0));
}

/// Mixin's `Callback.checkDescriptor`: a valued target's `HEAD` callback takes a
/// `CallbackInfoReturnable`. One that names the target exactly fails; one selected by name alone
/// is skipped (the body runs).
#[test]
fn a_plain_callback_info_on_a_valued_target_is_refused_or_skipped() {
    let fixture = fixture();
    let refused =
        fixture
            .jar
            .execute_discovered_mixins(&target("plainInfo"), None, &[], &mut StdHost::new());
    assert!(
        matches!(&refused, Err(error) if error.to_string().contains("Mixin expects a CallbackInfoReturnable")),
        "{refused:?}"
    );
    let skipped = fixture
        .jar
        .execute_discovered_mixins(&target("byName"), None, &[], &mut StdHost::new())
        .unwrap();
    assert!(!skipped.cancelled);
    assert_eq!(skipped.target_result, Value::Int(9));
}
