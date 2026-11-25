//! Real-jar integration test for block-use -> open-menu against Runes 1.3.2 (`RUNES_JAR`).
//!
//! Right-clicking `runes:crafting_altar` runs `RuneCraftingBlock.useItemOn` (inherited ->
//! `TRY_WITH_EMPTY_HAND`) and `RuneCraftingBlock.useWithoutItem` through Decaf's generic
//! block-use and menu-provider models.

use std::{collections::BTreeMap, path::Path};

use horizon_native_java::{
    BlockHitData, BlockUseContext, BlockUsePlayerData, BlockUseSide, InteractionItemContext,
    InteractionResultValue, MenuEffect, NativeJar, PlayerContainer, StdHost, SwingSource,
};
use serde_json::json;

#[test]
fn runes_crafting_altar_right_click_opens_crafting_menu() {
    let Ok(jar_path) = std::env::var("RUNES_JAR") else {
        eprintln!(
            "skipping runes_crafting_altar_right_click_opens_crafting_menu: set RUNES_JAR to run"
        );
        return;
    };
    std::thread::Builder::new()
        .name("runes-block-use".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || run_runes_crafting_altar_test(&jar_path))
        .expect("spawn test thread")
        .join()
        .unwrap();
}

fn run_runes_crafting_altar_test(jar_path: &str) {
    let jar = NativeJar::from_path(Path::new(jar_path)).expect("load RUNES_JAR");
    let mut host = jar
        .try_seed_host(StdHost::new().with_game_version("26.3"))
        .expect("seed Runes host");
    let entrypoints = jar.discover_entrypoints("main");
    assert!(
        !entrypoints.is_empty(),
        "Runes should have a main entrypoint"
    );
    for entry in entrypoints {
        jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
            .expect("run Runes main");
    }

    // The player just joined: a new `ServerPlayer`, `containerCounter` 0.
    host.server_player_created(1, 1, 0);
    let pos = [12, 65, -7];
    let server_ctx = BlockUseContext {
        side: BlockUseSide::Server,
        dimension: "minecraft:overworld".to_owned(),
        block_id: "runes:crafting_altar".to_owned(),
        state_properties: BTreeMap::from([("facing".to_owned(), "north".to_owned())]),
        pos,
        hit: BlockHitData::block(pos, "UP", [12.5, 66.0, -6.5], false),
        player: BlockUsePlayerData::default(),
        hand: "MAIN_HAND".to_owned(),
        stack: None,
    };

    // 1. useItemOn inherits BlockBehaviour.useItemOn -> TRY_WITH_EMPTY_HAND (0 unknowns).
    let item_on = jar
        .block_use_item_on(&mut host, &server_ctx)
        .expect("run useItemOn");
    assert_eq!(item_on.failure, None);
    assert_eq!(
        item_on.unknown_host_call_count, 0,
        "unexpected unknowns in useItemOn: {:?}",
        item_on.unknown_host_calls
    );
    assert_eq!(
        item_on.result,
        Some(InteractionResultValue::TryEmptyHandInteraction)
    );
    assert!(item_on.menu_effects.is_empty());

    // 2. Server-side useWithoutItem -> CONSUME + typed OpenMenuEffect for `runes:crafting`.
    //    `ServerPlayer.initMenu` (the slot listener and synchronizer) is the one counted call.
    let server = jar
        .block_use_without_item(&mut host, &server_ctx)
        .expect("run server useWithoutItem");
    assert_eq!(server.failure, None);
    assert_eq!(
        server.unknown_host_calls,
        BTreeMap::from([(INIT_MENU.to_owned(), 1)]),
        "unexpected unknowns in server useWithoutItem"
    );
    assert_eq!(
        server.result,
        Some(InteractionResultValue::Success {
            swing_source: SwingSource::None,
            item_context: InteractionItemContext {
                was_item_interaction: true,
                held_item_transformed_to: None,
            },
        })
    );
    assert_eq!(server.menu_effects.len(), 1);
    let effect = server.open_menus().next().unwrap();
    assert_eq!(effect.container_id, 1);
    assert_eq!(effect.menu_type_id.as_deref(), Some("runes:crafting"));
    assert_eq!(
        effect.title_json,
        Some(json!({ "translate": "gui.runes.rune_crafting" }))
    );
    assert_eq!(effect.access_pos, Some(pos));
    assert!(effect.constructor.is_some());
    // Decaf holds it as the player's `containerMenu`.
    assert_eq!(
        host.player_open_menu(1).map(|m| m.menu.clone()),
        Some(effect.menu.clone())
    );

    // A second click while the window shows menu 1 closes it first: `ItemCombinerMenu.removed`
    // hands the inputs back, exactly (`live_menu.rs`; they are empty here), then menu 2 is
    // built. Without the player's facts `initMenu` stays the one counted call.
    let mut showing = server_ctx.clone();
    showing.player.container = PlayerContainer::Open(1);
    let again = jar
        .block_use_without_item(&mut host, &showing)
        .expect("run second server useWithoutItem");
    assert_eq!(again.failure, None);
    assert!(matches!(
        again.menu_effects.as_slice(),
        [MenuEffect::Closed { container_id: 1 }, MenuEffect::Opened(open)] if open.container_id == 2
    ));
    assert_eq!(
        again.unknown_host_calls,
        BTreeMap::from([(INIT_MENU.to_owned(), 1)]),
        "{:?}",
        again.unknown_host_calls
    );
    assert_eq!(host.player_open_menu(1).map(|m| m.container_id), Some(2));

    // 3. Client-side useWithoutItem -> SUCCESS + 0 open-menu effects + 0 unknowns.
    let mut client_ctx = server_ctx;
    client_ctx.side = BlockUseSide::Client;
    let client = jar
        .block_use_without_item(&mut host, &client_ctx)
        .expect("run client useWithoutItem");
    assert_eq!(client.failure, None);
    assert_eq!(
        client.unknown_host_call_count, 0,
        "unexpected unknowns in client useWithoutItem: {:?}",
        client.unknown_host_calls
    );
    assert_eq!(
        client.result,
        Some(InteractionResultValue::Success {
            swing_source: SwingSource::Predicted,
            item_context: InteractionItemContext {
                was_item_interaction: true,
                held_item_transformed_to: None,
            },
        })
    );
    assert!(client.menu_effects.is_empty());
}

const INIT_MENU: &str = "net/minecraft/server/level/ServerPlayer#initMenu(Lnet/minecraft/world/inventory/AbstractContainerMenu;)V";
