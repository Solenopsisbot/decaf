#[path = "support/javac_fixture.rs"] mod javac_fixture;
const PROBE: &str = include_str!("fixtures/nbtio_read/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/nbtio_read/java25.txt");
#[test]
fn nbtio_rows_match_java() {
 let fixture=javac_fixture::Fixture::build("nbtio-read", &[("demo/Probe.java",PROBE)]);
 let mut host=fixture.jar.seed_host(horizon_native_java::StdHost::new());
 let answer=fixture.jar.invoke_method_with_args("demo/Probe","rows","()Ljava/lang/String;",None,&[],&mut host).unwrap();
 let text=match answer { horizon_native_java::Value::Ref(Some(s))=>s, other=>format!("{other:?}") };
 assert_eq!(text, ORACLE.trim());
 assert_eq!(host.unknown_host_calls(), vec![("net/minecraft/nbt/NbtIo#read(Ljava/nio/file/Path;)Lnet/minecraft/nbt/CompoundTag;".into(),1)]);
}
