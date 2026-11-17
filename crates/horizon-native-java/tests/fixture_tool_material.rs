//! Java 25 oracle fixture for vanilla 26.3 `ToolMaterial` constants and accessors
//! (`tool_material.rs`; Wizards, Paladins and Archers read them).
//!
//! `fixtures/tool_material/demo/Probe.java` is one source run two ways: on Java 25 against
//! the real game jar after bootstrap (`java25.txt`, command in the README), and here,
//! compiled against the stub below. Only `demo/` ships, so every `ToolMaterial` read reaches
//! the host model, and `rows()` must equal the capture byte for byte.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const TOOL_MATERIAL: &str = r#"package net.minecraft.world.item;
public final class ToolMaterial {
 public static final ToolMaterial WOOD = null, STONE = null, COPPER = null, IRON = null,
     DIAMOND = null, GOLD = null, NETHERITE = null;
 public int durability(){throw new AssertionError();}
 public float speed(){throw new AssertionError();}
 public float attackDamageBonus(){throw new AssertionError();}
 public int enchantmentValue(){throw new AssertionError();}
}"#;
const PROBE: &str = include_str!("fixtures/tool_material/demo/Probe.java");
const ORACLE: &str = include_str!("fixtures/tool_material/java25.txt");

#[test]
fn tool_material_rows_match_java25() {
    let fixture = Fixture::build(
        "tool-material",
        &[
            ("net/minecraft/world/item/ToolMaterial.java", TOOL_MATERIAL),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows"), ORACLE);
}
