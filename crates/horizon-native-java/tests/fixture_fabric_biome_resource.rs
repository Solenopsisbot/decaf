#[test]
fn captured_java25_rows_match_fabric_biome_contract() {
    let rows = include_str!("fixtures/fabric_biome_resource/java25.txt");
    assert_eq!(
        rows,
        "net.fabricmc.fabric.api.biome.v1.BiomeModification\ntrue\ntrue\ndup=allowed\n"
    );
}
