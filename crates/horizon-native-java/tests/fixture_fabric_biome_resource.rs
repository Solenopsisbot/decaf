#[test]
fn captured_java25_rows_match_fabric_biome_contract() {
    let rows = include_str!("fixtures/fabric_biome_resource/java25.txt");
    assert_eq!(
        rows,
        "net.fabricmc.fabric.api.biome.v1.BiomeModification\ntrue\ntrue\ndup=allowed\n"
    );
}

#[test]
fn interpreter_matches_captured_create_rows() {
    use horizon_native_java::{Host, MethodRef, StdHost, Value};
    let r = MethodRef { owner: "net/fabricmc/fabric/api/biome/v1/BiomeModifications".into(), name: "create".into(), descriptor: "(Lnet/minecraft/resources/Identifier;)Lnet/fabricmc/fabric/api/biome/v1/BiomeModification;".into() };
    let mut host = StdHost::new();
    let id = Value::Ref(Some("identifier/x:y".into()));
    let a = host.invoke(&r, std::slice::from_ref(&id)).unwrap();
    let b = host.invoke(&r, std::slice::from_ref(&id)).unwrap();
    assert_ne!(a, b);
    assert_eq!(host.invoke(&r, &[Value::Ref(None)]).is_ok(), true);
}
