//! `FabricCreativeModeTab.builder()` (#453, `host_creative_tab.rs`).
//!
//! `fixtures/fabric_creative_tab/demo/Probe.java` runs on Java 25 with the real Fabric
//! creative-tab module and the 26.3 jar (`java25.txt`, command in the README), and here against
//! compile-only stubs: only `demo/` ships. Two builders are distinct, each `build()` makes a new
//! tab, and a tab's display name is its builder's title. Equal to the capture, nothing counted.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;
const FABRIC: &str = r#"package net.fabricmc.fabric.api.creativetab.v1; public final class FabricCreativeModeTab { public static net.minecraft.world.item.CreativeModeTab.Builder builder(){ throw new AssertionError(); }}"#;
// Vanilla's `Component.literal` returns `MutableComponent`, the descriptor the host's component
// model answers.
const COMPONENT: &str = r#"package net.minecraft.network.chat; public interface Component { static MutableComponent literal(String s){ throw new AssertionError(); } String getString(); }"#;
const MUTABLE_COMPONENT: &str =
    r#"package net.minecraft.network.chat; public interface MutableComponent extends Component {}"#;
const TAB: &str = r#"package net.minecraft.world.item; import net.minecraft.network.chat.Component; public class CreativeModeTab { public static class Builder { public Builder title(Component c){throw new AssertionError();} public CreativeModeTab build(){throw new AssertionError();} } public Component getDisplayName(){throw new AssertionError();} }"#;
const JAVA25_CAPTURE: &str = include_str!("fixtures/fabric_creative_tab/java25.txt");
const PROBE: &str = include_str!("fixtures/fabric_creative_tab/demo/Probe.java");
#[test]
fn fabric_creative_tab_fixture() {
    let f = Fixture::build(
        "fabric-creative-tab",
        &[
            (
                "net/fabricmc/fabric/api/creativetab/v1/FabricCreativeModeTab.java",
                FABRIC,
            ),
            ("net/minecraft/network/chat/Component.java", COMPONENT),
            (
                "net/minecraft/network/chat/MutableComponent.java",
                MUTABLE_COMPONENT,
            ),
            ("net/minecraft/world/item/CreativeModeTab.java", TAB),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut h = f.jar.seed_host(StdHost::new());
    assert_eq!(f.text_on(&mut h, "rows"), JAVA25_CAPTURE);
    assert!(
        h.unknown_host_calls().is_empty(),
        "{:?}",
        h.unknown_host_calls()
    );
}
