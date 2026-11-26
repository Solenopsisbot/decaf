//! Real-jar item parts: Runes 1.3.2's rune pouches are Bundle API's `CustomBundleItem` (in the
//! nested `bundle-api-5.0.0+26.3-fabric.jar`), whose own `overrideOtherStackedOnMe`, bar hooks
//! and `use` run here on Decaf's item-part façades (`item_parts.rs`).
//!
//! `RUNES_JAR` names the jar; the test is skipped without it (CI has no corpus).
//!
//! Expected values are derived from the jars' bytecode (`javap -c -p`, JDK 25):
//! - `CustomBundleItem.overrideOtherStackedOnMe` offsets 0-136: a SECONDARY click on an
//!   unmodified slot with a stack in `#runes:runes` adds it through `Builder.add(ItemStack)` and
//!   writes `Builder.build()` back to the pouch, answering `true`; a PRIMARY click answers
//!   `false` (offsets 0-5, 137-138) and changes nothing.
//! - `Builder.add(ItemStack)` takes `min(count, getMaxAllowed(itemOccupancy))` where an item's
//!   occupancy is `1/maxStackSize` (`CustomBundleContentsComponent.getOccupancy(ItemInstance,
//!   int)` offsets 32-111 for a stack with neither contents component nor bees) and
//!   `getMaxAllowed(f) = max((1 * size_multiplier - occupancy) / f, 0)` in `Fraction`s.
//! - `getBarWidth` is `min(1 + Mth.mulAndTruncate(occupancy, 12), 13)`; `getBarColor` is the
//!   static `ARGB.colorFromFloat(1.0f, 0.4f, 0.4f, 1.0f)` (`<clinit>` offsets 0-11).

use std::path::Path;

use horizon_native_java::{
    BlockUsePlayerData, ExecutionBudget, GAME_PROTOTYPE_COMPONENTS, ItemClick, ItemClickDirection,
    ItemHook, LevelRandomStream, MenuPlayerFacts, NativeJar, Nbt, NbtCompound, PlayerContainer,
    RuntimeComponentValue, StdHost, Value,
};

const UUID: u128 = 0x00c0_ffee_0000_0000_0000_0000_0000_0077;
const POUCH: &str = "runes:small_rune_pouch";
const RUNE: &str = "runes:fire_stone";
const CONTENTS: &str = "bundleapi:custom_bundle_contents";
const BUNDLE_ITEM: &str = "com/github/theredbrain/bundleapi/item/CustomBundleItem";

#[test]
fn rune_pouch_takes_runes_through_its_own_override() {
    let Some(jar_path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes item-part test skipped");
        return;
    };
    std::thread::Builder::new()
        .name("runes-item-parts".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || run(Path::new(&jar_path)))
        .expect("spawn test thread")
        .join()
        .unwrap();
}

/// A lent level stream whose every `nextFloat()` is the same draw.
struct FixedStream(f32);

impl LevelRandomStream for FixedStream {
    fn next_int(&mut self) -> i32 {
        0
    }
    fn next_int_bound(&mut self, _bound: i32) -> i32 {
        0
    }
    fn next_long(&mut self) -> i64 {
        0
    }
    fn next_boolean(&mut self) -> bool {
        false
    }
    fn next_float(&mut self) -> f32 {
        self.0
    }
    fn next_double(&mut self) -> f64 {
        f64::from(self.0)
    }
    fn next_gaussian(&mut self) -> f64 {
        0.0
    }
}

fn player() -> BlockUsePlayerData {
    BlockUsePlayerData {
        uuid: UUID,
        name: "Pouchkeeper".to_owned(),
        entity_id: Some(11),
        incarnation: Some((1, 0)),
        container: PlayerContainer::Inventory,
        sneaking: false,
        spectator: false,
        main_hand: None,
        off_hand: None,
    }
}

fn facts(inventory: &[Value]) -> MenuPlayerFacts {
    MenuPlayerFacts {
        inventory: Some(inventory.to_vec()),
        selected_slot: 0,
        instabuild: Some(false),
        position: Some([1.5, 64.0, -2.5]),
        ..MenuPlayerFacts::default()
    }
}

/// The pouch's prototype value, as the item's `Item.Properties` sets it and the game stores it
/// (`{content: [], size_multiplier: 4}` for the small pouch, `CustomBundleContentsComponent.CODEC`).
fn small_pouch_default() -> Nbt {
    Nbt::Compound(NbtCompound::from([
        ("content".to_owned(), Nbt::List(Vec::new())),
        ("size_multiplier".to_owned(), Nbt::Int(4)),
    ]))
}

fn item(host: &StdHost, id: &str) -> Value {
    host.registrations()
        .iter()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == id)
        .unwrap_or_else(|| panic!("{id} is registered"))
        .value
        .clone()
}

fn run(jar_path: &Path) {
    let jar = NativeJar::from_path(jar_path).expect("load RUNES_JAR");
    let mut host = jar
        .try_seed_host(StdHost::new().with_game_version("26.3"))
        .expect("seed Runes host");
    for entry in jar.discover_entrypoints("main") {
        jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
            .expect("run Runes main");
    }
    host.server_player_created(UUID, 1, 0);
    // Bundle API registers its contents type from a Mixin at the tail of
    // `DataComponents.<clinit>` (`DataComponentTypesMixin.static_init_TAIL_BundleAPI`), which
    // the game runs at bootstrap. Registering jar component types with the game is the bridge's
    // step 1 (`docs/java-item-parts.md`); here the handler runs directly, as the weave would.
    jar.invoke_method_with_args(
        "com/github/theredbrain/bundleapi/mixin/registry/DataComponentTypesMixin",
        "static_init_TAIL_BundleAPI",
        "(Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo;)V",
        None,
        &[Value::Ref(None)],
        &mut host,
    )
    .expect("register the contents type");

    let pouch_item = item(&host, POUCH);
    let overrides = jar.item_overrides(&pouch_item, &host);
    for hook in ItemHook::ALL {
        assert_eq!(
            overrides.owners.get(&hook).map(String::as_str),
            Some(BUNDLE_ITEM),
            "{hook:?} is CustomBundleItem's own"
        );
    }
    assert_eq!(
        overrides.vanilla_base.as_deref(),
        Some("net/minecraft/world/item/Item")
    );
    assert!(
        jar.jar_component_types(&mut host)
            .contains(&CONTENTS.to_owned()),
        "the contents type has a persistent codec"
    );

    // The game's prototype values for the components the façade asks it about: the pouch's
    // contents default, and none of the vanilla container components on either item.
    let defaults = |contents: Option<Nbt>| {
        let mut out: Vec<(String, Option<Nbt>)> = GAME_PROTOTYPE_COMPONENTS
            .iter()
            .map(|id| ((*id).to_owned(), None))
            .collect();
        out.push((CONTENTS.to_owned(), contents));
        out
    };
    let pouch = jar
        .make_item_stack(
            &mut host,
            Some(pouch_item.clone()),
            POUCH,
            1,
            Vec::new(),
            defaults(Some(small_pouch_default())),
            false,
            Vec::new(),
        )
        .expect("pouch stack");
    let rune_item = item(&host, RUNE);
    let runes = jar
        .make_item_stack(
            &mut host,
            Some(rune_item),
            RUNE,
            10,
            Vec::new(),
            defaults(None),
            false,
            Vec::new(),
        )
        .expect("rune stack");
    let mut inventory = vec![StdHost::empty_item_stack(); 36];
    inventory[3] = pouch;

    // A PRIMARY click: the pouch's override answers false and changes nothing.
    let primary = jar
        .item_click_override(
            &mut host,
            &player(),
            "minecraft:overworld",
            &facts(&inventory),
            &pouch_item,
            BUNDLE_ITEM,
            &ItemClick {
                direction: ItemClickDirection::OtherStackedOnMe,
                inventory_slot: 3,
                carried: runes.clone(),
                primary: true,
            },
            &ExecutionBudget::UNBOUNDED,
        )
        .expect("primary click");
    assert!(primary.report.exact(), "{:?}", primary.report);
    assert!(!primary.handled);

    // A SECONDARY click puts the runes in. The insert sound's pitch draws the level's
    // `Level.getRandom()`, which the embedder lends for the call.
    host.set_host_deferred_server_sound_seeds(true);
    let mut stream = FixedStream(0.5);
    let secondary = host
        .with_lent_level_random("minecraft:overworld", "test/1", &mut stream, |host| {
            jar.item_click_override(
                host,
                &player(),
                "minecraft:overworld",
                &facts(&inventory),
                &pouch_item,
                BUNDLE_ITEM,
                &ItemClick {
                    direction: ItemClickDirection::OtherStackedOnMe,
                    inventory_slot: 3,
                    carried: runes.clone(),
                    primary: false,
                },
                &ExecutionBudget::UNBOUNDED,
            )
        })
        .expect("secondary click");
    assert!(secondary.report.exact(), "{:?}", secondary.report);
    assert!(
        secondary.handled,
        "a SECONDARY click with a rune is handled"
    );
    assert_eq!(secondary.takes, 0);
    // `Builder.add(ItemStack)`: occupancy per rune `1/(64 * 4)` (a plain `new Item(..)` stacks
    // to 64; the small pouch's `size_multiplier` is 4), so `getMaxAllowed` is 256 and all ten
    // move in: the cursor's stack is shrunk to nothing.
    let carried_after = host
        .item_stack_components(&secondary.carried)
        .expect("modelled cursor");
    assert_eq!(carried_after.count, 0, "{carried_after:?}");
    let pouch_after = jar
        .item_stack_components(&mut host, &secondary.slot)
        .expect("encode")
        .expect("modelled pouch");
    assert_eq!(pouch_after.item_id, POUCH);
    assert_eq!(pouch_after.count, 1);
    assert!(!pouch_after.opaque_components);
    let ten_runes = Nbt::Compound(NbtCompound::from([
        (
            "content".to_owned(),
            Nbt::List(vec![Nbt::Compound(NbtCompound::from([
                ("id".to_owned(), Nbt::String(RUNE.to_owned())),
                ("count".to_owned(), Nbt::Int(10)),
            ]))]),
        ),
        ("size_multiplier".to_owned(), Nbt::Int(4)),
    ]));
    assert_eq!(
        pouch_after.patch,
        vec![(
            CONTENTS.to_owned(),
            Some(RuntimeComponentValue::Nbt(ten_runes))
        )]
    );

    // `isBarVisible`: occupancy `10/256 > 0`; `getBarWidth`: `min(1 + 10 * 12 / 256, 13)` in
    // `Mth.mulAndTruncate`'s int arithmetic on the reduced `5/128` (`5 * 12 / 128 = 0`), so 1;
    // `getBarColor`: `ARGB.colorFromFloat(1, 0.4, 0.4, 1)` = `0xFF6666FF`.
    let bar = jar
        .item_bar(
            &mut host,
            &pouch_item,
            &overrides,
            &secondary.slot,
            false,
            &ExecutionBudget::UNBOUNDED,
        )
        .expect("bar");
    assert!(bar.report.exact(), "{:?}", bar.report);
    assert_eq!(bar.visible, Some(true));
    assert_eq!(bar.width, Some(1));
    assert_eq!(bar.color, Some(0xFF66_66FF_u32 as i32));

    // The full pouch on the cursor, right-clicked onto an empty slot: its
    // `overrideStackedOnOther` (offsets 45-87) takes the first stack out (`removeFirst`) into
    // the slot (`Slot.safeInsert`), and the emptied contents equal the prototype's again, so
    // the pouch's patch is empty (`PatchedDataComponentMap.set` removes an entry equal to the
    // prototype).
    inventory = secondary.inventory.clone().expect("inventory after");
    let full_pouch = inventory[3].clone();
    inventory[3] = StdHost::empty_item_stack();
    let mut stream = FixedStream(0.25);
    let out = host
        .with_lent_level_random("minecraft:overworld", "test/1", &mut stream, |host| {
            jar.item_click_override(
                host,
                &player(),
                "minecraft:overworld",
                &facts(&inventory),
                &pouch_item,
                BUNDLE_ITEM,
                &ItemClick {
                    direction: ItemClickDirection::CarriedOnOther,
                    inventory_slot: 5,
                    carried: full_pouch.clone(),
                    primary: false,
                },
                &ExecutionBudget::UNBOUNDED,
            )
        })
        .expect("take out");
    assert!(out.report.exact(), "{:?}", out.report);
    assert!(out.handled);
    let slot_after = host.item_stack_components(&out.slot).expect("slot stack");
    assert_eq!((slot_after.item_id.as_str(), slot_after.count), (RUNE, 10));
    let pouch_after = jar
        .item_stack_components(&mut host, &out.carried)
        .expect("encode")
        .expect("modelled pouch");
    assert_eq!(pouch_after.patch, Vec::new());
}
