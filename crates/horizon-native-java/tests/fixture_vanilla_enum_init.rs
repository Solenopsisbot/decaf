#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const PROBE: &str = include_str!("fixtures/vanilla_enum/InitProbe.java");
const ENUM: &str = include_str!("fixtures/vanilla_enum/SoundSource.java");
#[test]
fn static_initializer_reads_enum_constant() {
    let f = Fixture::build(
        "vanilla-enum-init",
        &[
            ("demo/InitProbe.java", PROBE),
            ("net/minecraft/sounds/SoundSource.java", ENUM),
        ],
    );
    let mut h = f.jar.seed_host(StdHost::new());
    let v = f
        .jar
        .invoke_method_with_args(
            "demo/InitProbe",
            "rows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut h,
        )
        .unwrap();
    assert_eq!(v, horizon_native_java::Value::Ref(Some("player".into())));
}
