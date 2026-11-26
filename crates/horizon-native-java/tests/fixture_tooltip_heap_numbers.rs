//! Client tooltips followed by a screen open never trip the "missed heap root" check (#609,
//! Runes 1.3.2 round 2 bug 1).
//!
//! In game, after a few minutes of hovering Runes' items, every altar open failed with
//! "the jar's object net/runes/client/RuneCraftingScreen#235 was collected while still
//! reachable (a missed heap root)", until restart. The screen was the object being constructed:
//! nothing had missed a root. Each hover passes the jar a host `Consumer` (the tooltip sink),
//! and its key spelled a jar object's (`tooltip-consumer#<n>`) with `n` from a counter of its
//! own that grows by one per tooltip frame. A hover allocates one varargs array and no
//! object, so after enough hovers the sink's number was far past the heap's next object
//! number. The collection that ran while a sink was live took the sink's number as the heap's
//! limit and freed every number below it, those the heap had not handed out yet included.
//! From then on, each new object's first unwritten-field read was refused.
//!
//! `fixtures/tooltip_heap_numbers/demo/Probe.java` runs unchanged on both sides: its own `main`
//! on Java 25 (1,500 hovers, then the open) and the same calls one by one through one host.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{
    ExecutionBudget, StdHost, Value,
    client_surface::{tooltip_context, tooltip_display, tooltip_flag, tooltip_stack},
};

/// What `java -cp . demo.Probe` printed on Java 25 (Temurin 25.0.5, `javac --release 21`, with
/// the stubs below beside it), the same on two runs: hovers, the last hover's lines, the open.
const ORACLE: &str = include_str!("fixtures/tooltip_heap_numbers/java25.txt");

const PROBE: &str = include_str!("fixtures/tooltip_heap_numbers/demo/Probe.java");

/// The 26.3 API the probe compiles against. Only `demo/` ships in the probe jar, so in Horizon
/// every call below reaches the host's models. The bodies are for the Java 25 oracle run.
const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/ChatFormatting.java",
        "package net.minecraft; public enum ChatFormatting { GRAY, ITALIC }",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat; import net.minecraft.ChatFormatting; public interface Component { static MutableComponent translatable(String key) { return new Stub(key); } final class Stub implements MutableComponent { final String key; Stub(String key) { this.key = key; } public MutableComponent withStyle(ChatFormatting... formats) { return this; } } }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat; import net.minecraft.ChatFormatting; public interface MutableComponent extends Component { MutableComponent withStyle(ChatFormatting... formats); }",
    ),
    (
        "net/minecraft/world/item/ItemStack.java",
        "package net.minecraft.world.item; public class ItemStack {}",
    ),
    (
        "net/minecraft/world/item/TooltipFlag.java",
        "package net.minecraft.world.item; public interface TooltipFlag {}",
    ),
    (
        "net/minecraft/world/item/component/TooltipDisplay.java",
        "package net.minecraft.world.item.component; public final class TooltipDisplay {}",
    ),
    (
        "net/minecraft/world/item/Item.java",
        "package net.minecraft.world.item; import java.util.function.Consumer; import net.minecraft.network.chat.Component; import net.minecraft.world.item.component.TooltipDisplay; public class Item { public static class Properties {} public interface TooltipContext {} public Item(Properties properties) {} public void appendHoverText(ItemStack stack, TooltipContext context, TooltipDisplay display, Consumer<Component> out, TooltipFlag flag) {} }",
    ),
];

/// `Probe.HOVERS`: past the first collection trigger.
const HOVERS: usize = 1500;

fn text(fixture: &javac_fixture::Fixture, host: &mut StdHost, method: &str) -> String {
    match fixture
        .jar
        .invoke_method("demo/Probe", method, "()Ljava/lang/String;", None, host)
    {
        Ok(Value::Ref(Some(text))) => text,
        other => panic!("{method}: {other:?}"),
    }
}

/// 1,500 hovers of a Runes-shaped tooltip collect the heap at least once while a tooltip sink
/// is live; the object built afterwards then reads its unwritten field as Java 25 does
/// (`60|176`) instead of being refused as collected.
#[test]
fn objects_built_after_many_tooltips_are_never_taken_for_collected_ones() {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = javac_fixture::Fixture::build("tooltip-heap-numbers", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let item = fixture
        .jar
        .invoke_method(
            "demo/Probe",
            "item",
            "()Lnet/minecraft/world/item/Item;",
            None,
            &mut host,
        )
        .expect("Probe.item()");
    let mut lines = 0;
    let mut most_arrays = 0;
    for hover in 0..HOVERS {
        let out = fixture
            .jar
            .run_item_tooltip(
                &item,
                tooltip_stack(),
                tooltip_context(),
                tooltip_display(),
                tooltip_flag(false, false),
                &mut host,
                &ExecutionBudget::UNBOUNDED,
            )
            .unwrap_or_else(|error| panic!("hover {hover}: {error}"));
        lines = out.len();
        most_arrays = most_arrays.max(host.resident_heap().arrays());
    }
    // One `ChatFormatting[]` per hover, none kept: a collection ran during the hovers (without
    // one this test would show nothing), and it ran while that hover's sink was live.
    assert!(
        host.resident_heap().arrays() < HOVERS / 2,
        "{} arrays kept after {HOVERS} hovers (at most {most_arrays}): nothing was collected",
        host.resident_heap().arrays()
    );
    let hovered = text(&fixture, &mut host, "hovered");
    let opened = fixture
        .jar
        .invoke_method(
            "demo/Probe",
            "open",
            "()Ljava/lang/String;",
            None,
            &mut host,
        )
        .map_err(|error| error.to_string());
    let opened = match opened {
        Ok(Value::Ref(Some(text))) => text,
        other => panic!("the open after {HOVERS} hovers: {other:?}"),
    };
    assert_eq!(format!("{hovered}|{lines}|{opened}\n"), ORACLE);
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}
