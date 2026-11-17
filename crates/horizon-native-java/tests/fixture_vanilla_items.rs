//! `Items.<FIELD>` through the host (#484, `vanilla_items.rs`): the id each constant answers is the
//! one Java 25 reflection over the bootstrapped 26.3 `Items` captured (`fixtures/vanilla_items/`,
//! command in the README), and a field 26.3 lacks is counted, never a made-up id. The unit test in
//! `vanilla_items.rs` checks every captured row; this one goes through a mod's `getstatic`.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use horizon_native_java::client_surface::ClientSnapshot;
use javac_fixture::Fixture;
use std::sync::Arc;

const CAPTURE: &str = include_str!("fixtures/vanilla_items/java25.txt");

/// Compile-only stubs: only `demo/` ships, so every `Items` read reaches the host. `CHAIN` is not
/// a 26.3 item (it isn't in the capture).
const ITEM: &str = "package net.minecraft.world.item; public class Item {}";
const ITEMS: &str = "package net.minecraft.world.item; public final class Items { public static final Item STONE = new Item(); public static final Item DRY_SHORT_GRASS = new Item(); public static final Item CHAIN = new Item(); }";
const PROBE: &str = "package demo; import net.minecraft.world.item.Items; public final class Probe { public static Object stone() { return Items.STONE; } public static Object dryShortGrass() { return Items.DRY_SHORT_GRASS; } public static Object chain() { return Items.CHAIN; } }";

fn fixture() -> Fixture {
    Fixture::build(
        "vanilla-items",
        &[
            ("net/minecraft/world/item/Item.java", ITEM),
            ("net/minecraft/world/item/Items.java", ITEMS),
            ("demo/Probe.java", PROBE),
        ],
    )
}

/// The id the capture gives `field`.
fn captured_id(field: &str) -> &'static str {
    CAPTURE
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix('|'))
        .unwrap_or_else(|| panic!("{field} is not in the capture"))
}

/// Run `method` inside a client frame (the `Items` constants answer only there) and return its
/// value and the calls it counted.
fn read(fixture: &Fixture, method: &str) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    host.begin_client_frame(Arc::new(ClientSnapshot::default()), &[]);
    let value = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/Object;",
            None,
            &[],
            &mut host,
        )
        .unwrap_or_else(|error| panic!("{method} failed: {error}"));
    host.end_client_frame();
    (format!("{value:?}"), host.unknown_host_calls())
}

#[test]
fn items_constants_answer_their_captured_ids() {
    let fixture = fixture();
    // STONE follows the lower-case rule; DRY_SHORT_GRASS is one of the three that don't
    // (`minecraft:short_dry_grass`), which main answered wrongly before.
    for (method, field) in [("stone", "STONE"), ("dryShortGrass", "DRY_SHORT_GRASS")] {
        let (value, unknown) = read(&fixture, method);
        let expected = format!("client/item/{}", captured_id(field));
        assert!(
            value.contains(&expected),
            "{field}: {value} lacks {expected}"
        );
        assert_eq!(unknown, Vec::<(String, usize)>::new(), "{field}");
    }
}

#[test]
fn a_field_26_3_lacks_is_counted() {
    assert!(CAPTURE.lines().all(|line| !line.starts_with("CHAIN|")));
    let (value, unknown) = read(&fixture(), "chain");
    assert_eq!(value, "Ref(None)");
    assert_eq!(
        unknown,
        vec![(
            "net/minecraft/world/item/Items#CHAINLnet/minecraft/world/item/Item;".to_owned(),
            1
        )]
    );
}
