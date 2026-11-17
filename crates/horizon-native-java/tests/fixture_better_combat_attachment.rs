#[path = "support/javac_fixture.rs"]
mod javac_fixture;
const PROBE: &str = include_str!("fixtures/better_combat/Probe.java");
const ORACLE: &str = include_str!("fixtures/better_combat/java25.txt");
const PREDICATE_STUB: &str = r#"package net.fabricmc.fabric.api.attachment.v1; public interface AttachmentSyncPredicate { boolean test(AttachmentTarget t, net.minecraft.server.level.ServerPlayer p); static AttachmentSyncPredicate all(){return null;} static AttachmentSyncPredicate targetOnly(){return null;} static AttachmentSyncPredicate allButTarget(){return null;} }"#;
#[test]
fn attachment_predicate_rows_match_java25() {
    let fixture = javac_fixture::Fixture::build(
        "better-combat-attachment",
        &[
            ("demo/Probe.java", PROBE),
            (
                "net/fabricmc/fabric/api/attachment/v1/AttachmentSyncPredicate.java",
                PREDICATE_STUB,
            ),
            (
                "net/fabricmc/fabric/api/attachment/v1/AttachmentTarget.java",
                "package net.fabricmc.fabric.api.attachment.v1; public interface AttachmentTarget {}",
            ),
            (
                "net/minecraft/server/level/ServerPlayer.java",
                "package net.minecraft.server.level; public class ServerPlayer implements net.fabricmc.fabric.api.attachment.v1.AttachmentTarget {}",
            ),
        ],
    );
    let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "rows",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    let text = match answer {
        horizon_native_java::Value::Ref(Some(v)) => v,
        other => panic!("{other:?}"),
    };
    let expected = ORACLE
        .lines()
        .filter(|line| !line.starts_with("#"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(text, expected.trim());
}
