//! `FabricCreativeModeTab.builder()` and vanilla's `CreativeModeTab.Builder` (#453,
//! `host_creative_tab.rs`).
//!
//! `fixtures/fabric_creative_tab/demo/Probe.java` runs on Java 25 with the real Fabric
//! creative-tab module and the 26.3 jar (command in the README), and here against compile-only
//! stubs: only `demo/` ships, so every call reaches the host. `rows()` must equal `java25.txt`
//! with nothing counted. The other probe methods are cases Decaf counts rather than answers; Java
//! 25's answers are in `java25_counted.txt`, and each one must count exactly the call named here.
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
const ITEM_STACK: &str = r#"package net.minecraft.world.item; public final class ItemStack {}"#;
const TAB: &str = r#"package net.minecraft.world.item; import net.minecraft.network.chat.Component; import java.util.function.Supplier; public class CreativeModeTab { public static class Builder { public Builder title(Component c){throw new AssertionError();} public Builder icon(Supplier<ItemStack> s){throw new AssertionError();} public Builder alignedRight(){throw new AssertionError();} public Builder hideTitle(){throw new AssertionError();} public Builder noScrollBar(){throw new AssertionError();} public CreativeModeTab build(){throw new AssertionError();} } public Component getDisplayName(){throw new AssertionError();} }"#;
const JAVA25_CAPTURE: &str = include_str!("fixtures/fabric_creative_tab/java25.txt");
const JAVA25_COUNTED: &str = include_str!("fixtures/fabric_creative_tab/java25_counted.txt");
const PROBE: &str = include_str!("fixtures/fabric_creative_tab/demo/Probe.java");

const DISPLAY_NAME: &str = "net/minecraft/world/item/CreativeModeTab#getDisplayName()Lnet/minecraft/network/chat/Component;";

fn fixture() -> Fixture {
    Fixture::build(
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
            ("net/minecraft/world/item/ItemStack.java", ITEM_STACK),
            ("net/minecraft/world/item/CreativeModeTab.java", TAB),
            ("demo/Probe.java", PROBE),
        ],
    )
}

#[test]
fn fabric_creative_tab_rows_match_java25() {
    let f = fixture();
    let mut h = f.jar.seed_host(StdHost::new());
    let rows = f.text_on(&mut h, "rows");
    let unknown = h.unknown_host_calls();
    assert_eq!(rows, JAVA25_CAPTURE, "unknown calls: {unknown:?}");
    assert_eq!(unknown, Vec::<(String, usize)>::new());
}

#[test]
fn null_title_is_counted() {
    let f = fixture();
    // (probe method, the one call it must count).
    let cases = [("nullTitle", DISPLAY_NAME)];
    // Every case the capture holds is asserted here, and no other.
    let captured: Vec<&str> = JAVA25_COUNTED
        .lines()
        .map(|line| line.split_once('=').expect("method=answer").0)
        .collect();
    assert_eq!(captured, cases.map(|(method, _)| method));
    for (method, counted) in cases {
        let mut h = f.jar.seed_host(StdHost::new());
        f.text_on(&mut h, method);
        assert_eq!(
            h.unknown_host_calls(),
            vec![(counted.to_owned(), 1)],
            "{method}"
        );
    }
}
