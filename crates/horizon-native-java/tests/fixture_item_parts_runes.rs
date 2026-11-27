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
//! - `overrideStackedOnOther` offsets 0-153: a SECONDARY click with the carried pouch on a
//!   non-empty slot asks `slot.getItem().getItem().canFitInsideContainerItems()` (offset 95) and
//!   then `is(#runes:runes)` (offset 114); either `false` skips the insert, and the pouch writes
//!   its unchanged contents back (`set`, offset 150) and answers `true`: nothing moves. A PRIMARY
//!   click answers `false` (offsets 0-8).
//! - `use` offsets 0-52: `dropAllBundledItems` (offsets 0-69) clears the contents
//!   (`Builder.clear().build()`, `set`) and, on a `ServerPlayer`, drops each stack of the
//!   contents' list in order through `Player.drop(stack, true, Prediction.PREDICTED)`; then
//!   `playDropContentsSound` plays `BUNDLE_DROP_CONTENTS` at `0.8f` volume and pitch
//!   `0.8f + level.getRandom().nextFloat() * 0.4f` (`fmul`, `fadd`) through
//!   `Entity.playSound`, then `awardStat(ITEM_USED.get(this))`, and on a server level answers
//!   `SUCCESS_SERVER`.

use std::path::Path;

use horizon_native_java::{
    BlockUsePlayerData, ExecutionBudget, GAME_PROTOTYPE_COMPONENTS, InteractionResultValue,
    ItemClick, ItemClickDirection, ItemHook, ItemUse, LevelRandomStream, LiveMenuEffect,
    MenuPlayerFacts, NativeJar, Nbt, NbtCompound, PlayerContainer, RuntimeComponentValue, StdHost,
    SwingSource, Value, level_sound::SoundRecord, player_drop_plan,
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
        // The server's runtime, as the mod seeds a server jar's host (`mod/src/lib.rs`).
        .try_seed_host(
            StdHost::new()
                .with_game_version("26.3")
                .with_environment_type("SERVER"),
        )
        .expect("seed Runes host");
    // The game's bootstrap runs Bundle API's Mixin at the tail of `DataComponents.<clinit>`
    // (`DataComponentTypesMixin`), which registers its contents type, before any entrypoint.
    assert_eq!(
        jar.run_bootstrap_static_injectors(&mut host, &ExecutionBudget::UNBOUNDED),
        Vec::<String>::new()
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
    for entry in jar.discover_entrypoints("main") {
        jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
            .expect("run Runes main");
    }
    host.server_player_created(UUID, 1, 0);

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
        host.jar_component_types()
            .iter()
            .any(|kind| kind.id == CONTENTS && kind.persistent),
        "the contents type has a persistent codec"
    );

    // The game's prototype values for the components the façade asks it about: none of the
    // vanilla container components on either item. The pouch's contents default is its own
    // `Item.Properties.component` object, which the runtime recorded and which answers its
    // prototype ahead of any game copy: none is handed over here, so the insert below works
    // only through that recorded object.
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
            defaults(None),
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

/// The jar booted as the mod boots a server jar: Bundle API's bootstrap Mixin, then Runes'
/// `main` entrypoints, and the test player's `ServerPlayer`.
fn boot(jar_path: &Path) -> (NativeJar, StdHost) {
    let jar = NativeJar::from_path(jar_path).expect("load RUNES_JAR");
    let mut host = jar
        .try_seed_host(
            StdHost::new()
                .with_game_version("26.3")
                .with_environment_type("SERVER"),
        )
        .expect("seed Runes host");
    assert_eq!(
        jar.run_bootstrap_static_injectors(&mut host, &ExecutionBudget::UNBOUNDED),
        Vec::<String>::new()
    );
    for entry in jar.discover_entrypoints("main") {
        jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
            .expect("run Runes main");
    }
    host.server_player_created(UUID, 1, 0);
    (jar, host)
}

/// The game's prototype values for the vanilla container components (none on these items) and
/// the pouch's contents type, as `mod/src/item_parts.rs` `java_stack` hands them.
fn game_defaults() -> Vec<(String, Option<Nbt>)> {
    let mut out: Vec<(String, Option<Nbt>)> = GAME_PROTOTYPE_COMPONENTS
        .iter()
        .map(|id| ((*id).to_owned(), None))
        .collect();
    out.push((CONTENTS.to_owned(), None));
    out
}

/// A pouch's `bundleapi:custom_bundle_contents` as the type's codec stores it.
fn contents_nbt(stacks: &[(&str, i32)], size_multiplier: i32) -> Nbt {
    Nbt::Compound(NbtCompound::from([
        (
            "content".to_owned(),
            Nbt::List(
                stacks
                    .iter()
                    .map(|(id, count)| {
                        Nbt::Compound(NbtCompound::from([
                            ("id".to_owned(), Nbt::String((*id).to_owned())),
                            ("count".to_owned(), Nbt::Int(*count)),
                        ]))
                    })
                    .collect(),
            ),
        ),
        ("size_multiplier".to_owned(), Nbt::Int(size_multiplier)),
    ]))
}

/// A stack of `id`: a jar item (its `Item` object) or a vanilla one (`None`), with `patch`.
fn stack(
    jar: &NativeJar,
    host: &mut StdHost,
    id: &str,
    count: i32,
    patch: Vec<(String, Option<RuntimeComponentValue>)>,
) -> Value {
    let item = host
        .registrations()
        .iter()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == id)
        .map(|entry| entry.value.clone());
    jar.make_item_stack(
        host,
        item,
        id,
        count,
        patch,
        game_defaults(),
        false,
        Vec::new(),
    )
    .unwrap_or_else(|error| panic!("{id} stack: {error}"))
}

/// A small pouch holding `stacks`. Each rune the contents name gets the game's prototype
/// values first, as the mod seeds them (`java_stack`), since the codec reads them as it decodes.
fn filled_pouch(jar: &NativeJar, host: &mut StdHost, stacks: &[(&str, i32)]) -> Value {
    for (id, _) in stacks {
        jar.record_item_prototype(host, id, game_defaults())
            .expect("rune prototype");
    }
    let patch = vec![(
        CONTENTS.to_owned(),
        Some(RuntimeComponentValue::Nbt(contents_nbt(stacks, 4))),
    )];
    stack(jar, host, POUCH, 1, patch)
}

/// BUGS-3d34-round4 #3: the carried pouch right-clicked onto a slot of a vanilla item that isn't
/// a rune. Fabric's `overrideStackedOnOther` reads the vanilla item's
/// `canFitInsideContainerItems()` (26.3: `Item`'s `true`; only the 17 shulker boxes, a
/// `BlockItem` of a `ShulkerBoxBlock`, answer `false`: `vanilla_can_fit_inside_container_items`'s
/// Java 25 capture), finds it outside `#runes:runes`, writes its contents back unchanged and
/// answers `true`: nothing moves. The runtime failed that read (`Item#canFitInsideContainerItems`
/// on a vanilla item was no host call it answered), and the mod turned the failure into
/// vanilla's swap.
#[test]
fn rune_pouch_onto_cobblestone_moves_nothing() {
    let Some(jar_path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes pouch-onto-cobblestone test skipped");
        return;
    };
    std::thread::Builder::new()
        .name("runes-pouch-onto-cobblestone".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let (jar, mut host) = boot(Path::new(&jar_path));
            let pouch_item = item(&host, POUCH);
            // (the slot's item and count, PRIMARY, the override's answer)
            for (target, count, primary, handled) in [
                ("minecraft:cobblestone", 10, false, true),
                // A shulker box stacks to 1; its `false` skips the insert the same way.
                ("minecraft:shulker_box", 1, false, true),
                // PRIMARY: `false` at offset 8, before anything is read.
                ("minecraft:cobblestone", 10, true, false),
            ] {
                let pouch = filled_pouch(&jar, &mut host, &[(RUNE, 5)]);
                let mut inventory = vec![StdHost::empty_item_stack(); 36];
                inventory[7] = stack(&jar, &mut host, target, count, Vec::new());
                let before = host.unknown_host_call_count();
                let out = jar
                    .item_click_override(
                        &mut host,
                        &player(),
                        "minecraft:overworld",
                        &facts(&inventory),
                        &pouch_item,
                        BUNDLE_ITEM,
                        &ItemClick {
                            direction: ItemClickDirection::CarriedOnOther,
                            inventory_slot: 7,
                            carried: pouch,
                            primary,
                        },
                        &ExecutionBudget::UNBOUNDED,
                    )
                    .expect("click");
                let what = format!("{target} primary={primary}");
                assert!(out.report.exact(), "{what}: {:?}", out.report);
                assert_eq!(host.unknown_host_call_count(), before, "{what}");
                assert_eq!(out.handled, handled, "{what}");
                assert_eq!(out.takes, 0, "{what}");
                assert!(out.effects.is_empty(), "{what}: {:?}", out.effects);
                // The slot keeps its stack.
                let slot = host.item_stack_components(&out.slot).expect("slot stack");
                assert_eq!(
                    (slot.item_id.as_str(), slot.count),
                    (target, count),
                    "{what}"
                );
                // The cursor keeps the pouch and its five runes.
                let carried = jar
                    .item_stack_components(&mut host, &out.carried)
                    .expect("encode")
                    .expect("modelled pouch");
                assert_eq!(
                    (carried.item_id.as_str(), carried.count),
                    (POUCH, 1),
                    "{what}"
                );
                assert_eq!(
                    carried.patch,
                    vec![(
                        CONTENTS.to_owned(),
                        Some(RuntimeComponentValue::Nbt(contents_nbt(&[(RUNE, 5)], 4)))
                    )],
                    "{what}"
                );
                // No insert, so no insert sound.
                let sounds = host.take_sound_records();
                assert!(sounds.is_empty(), "{what}: {sounds:?}");
            }
        })
        .expect("spawn test thread")
        .join()
        .unwrap();
}

/// BUGS-3d34-round4 #2: using a filled pouch drops its contents in their list order, plays the
/// drop-contents sound and counts the use. The sound's pitch draws the level's random after the
/// drops are recorded; a drop draws nothing of the level stream in 26.3 (the player's own and
/// the item entity's own randoms: `live_menu::draws_no_level_random`), so that draw is served
/// exactly from the lent stream, not counted.
#[test]
fn rune_pouch_use_drops_its_contents_in_order() {
    let Some(jar_path) = std::env::var_os("RUNES_JAR") else {
        eprintln!("RUNES_JAR is unset; real Runes pouch use test skipped");
        return;
    };
    std::thread::Builder::new()
        .name("runes-pouch-use".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            const FROST: &str = "runes:frost_stone";
            let (jar, mut host) = boot(Path::new(&jar_path));
            let pouch_item = item(&host, POUCH);
            let pouch = filled_pouch(&jar, &mut host, &[(FROST, 10), (RUNE, 40)]);
            let mut inventory = vec![StdHost::empty_item_stack(); 36];
            inventory[0] = pouch.clone();
            let holder = BlockUsePlayerData {
                main_hand: Some(pouch),
                ..player()
            };
            host.set_host_deferred_server_sound_seeds(true);
            let mut stream = FixedStream(0.25);
            let used = host
                .with_lent_level_random("minecraft:overworld", "test/1", &mut stream, |host| {
                    jar.item_use(
                        host,
                        &holder,
                        "minecraft:overworld",
                        &facts(&inventory),
                        &pouch_item,
                        BUNDLE_ITEM,
                        &ItemUse {
                            hand: "MAIN_HAND".to_owned(),
                        },
                        &ExecutionBudget::UNBOUNDED,
                    )
                })
                .expect("use");
            host.set_host_deferred_server_sound_seeds(false);
            assert!(used.report.exact(), "{:?}", used.report);
            assert_eq!(
                host.unknown_host_call_count(),
                0,
                "{:?}",
                host.unknown_host_calls()
            );
            assert!(
                matches!(
                    used.result,
                    Some(InteractionResultValue::Success {
                        swing_source: SwingSource::ServerOnly,
                        ..
                    })
                ),
                "SUCCESS_SERVER: {:?}",
                used.result
            );
            // The drops in the contents' order, each `Player.drop(stack, true, PREDICTED)` (its
            // plan: thrown by the player, the dropped stats once the entity is in the world),
            // then `ITEM_USED` of the pouch.
            let mut lines = Vec::new();
            for effect in &used.effects {
                match effect {
                    LiveMenuEffect::Drop {
                        stack,
                        retain_ownership,
                        prediction,
                    } => {
                        let plan = player_drop_plan(stack, *retain_ownership, *prediction);
                        lines.push(format!(
                            "drop {} {} thrower={} updateSelf={}",
                            stack.count, stack.item_id, plan.thrower, plan.swing_updates_self
                        ));
                        for (stat_type, value, amount) in &plan.stats_when_dropped {
                            lines.push(format!("stat {stat_type} {value} {amount}"));
                        }
                    }
                    LiveMenuEffect::AwardStat {
                        stat_type,
                        value,
                        amount,
                    } => lines.push(format!("stat {stat_type} {value} {amount}")),
                    other => panic!("unexpected effect {other:?}"),
                }
            }
            assert_eq!(
                lines,
                [
                    "drop 10 runes:frost_stone thrower=true updateSelf=false",
                    "stat minecraft:dropped runes:frost_stone 10",
                    "stat minecraft:custom minecraft:drop 1",
                    "drop 40 runes:fire_stone thrower=true updateSelf=false",
                    "stat minecraft:dropped runes:fire_stone 40",
                    "stat minecraft:custom minecraft:drop 1",
                    "stat minecraft:used runes:small_rune_pouch 1",
                ]
            );
            // The pouch is empty again: its cleared contents equal the prototype, so the patch
            // holds nothing (`PatchedDataComponentMap.set`).
            let held = jar
                .item_stack_components(&mut host, used.held.as_ref().expect("held"))
                .expect("encode")
                .expect("modelled pouch");
            assert_eq!((held.item_id.as_str(), held.count), (POUCH, 1));
            assert_eq!(held.patch, Vec::new());
            // `Entity.playSound` on the player is `Player.playSound`: the level's sound at the
            // player, except them, in their sound source, pitch `0.8f + 0.25f * 0.4f`.
            let sounds = host.take_sound_records();
            assert_eq!(sounds.len(), 1, "{sounds:?}");
            let SoundRecord::Play(sound) = &sounds[0] else {
                panic!("a seed drawn without a sound: {sounds:?}");
            };
            assert_eq!(sound.sound_id, "minecraft:item.bundle.drop_contents");
            assert_eq!(sound.except_player, Some(UUID));
            assert_eq!(sound.volume, 0.8f32);
            assert_eq!(sound.pitch, 0.8f32 + 0.25f32 * 0.4f32);
        })
        .expect("spawn test thread")
        .join()
        .unwrap();
}
