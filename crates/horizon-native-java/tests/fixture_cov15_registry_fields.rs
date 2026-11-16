//! Captured Java 25 reflection census for vanilla registry static fields.
use std::path::Path;

#[test]
fn captured_field_table_has_expected_complete_counts() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cov15_items/fields.txt");
    let text = std::fs::read_to_string(path).unwrap();
    let items = text.lines().filter(|l| l.starts_with("Items ")).count();
    let blocks = text.lines().filter(|l| l.starts_with("Blocks ")).count();
    assert_eq!(items, 1218);
    assert_eq!(blocks, 878);
    assert!(text.contains("class net.minecraft.world.item.Items count=1218"));
    assert!(text.contains("class net.minecraft.world.level.block.Blocks count=878"));
}
