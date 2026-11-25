//! Generic Sunrise `BlockPart` bridge for Java mod block right-clicks (`useItemOn`,
//! `useWithoutItem`) and `player.openMenu(MenuProvider)`.
//!
//! Nothing here names a specific mod. Every `InteractionResult` maps 1-to-1 to Sunrise's
//! [`mod_api::InteractionResult`], and every Java `openMenu` effect either opens a registered
//! Sunrise mod menu via [`mod_api::PlayerRef::open_mod_menu`] or records and counts a named
//! refusal without opening a stand-in window.
//!
//! The jar's host holds at most one Java menu per player, the one vanilla would have as the
//! `ServerPlayer`'s `containerMenu`, and only while Sunrise shows it under the same window id:
//! a refused open drops it at once, a close (`menu_closed`) runs its `removed` and drops it, and
//! logout and a new server drop it ([`crate::runtime::StdHost::player_open_menu`]). Sunrise's
//! window ids are the real `containerCounter`, so every open Sunrise reports
//! (`player_container_open`) keeps each jar's counter in step.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use mod_api::{
    BlockClick, BlockPart, Hand, InteractionResult, LogicalSide, MenuReplace, ModMenuOpen,
    OpenMenuError, PlayerRef, ServerHost, UseItemOnEvent, UseWithoutItemEvent,
};
use serde_json::json;

use crate::runtime::{
    BlockHitData, BlockUseContext, BlockUseOutcome, BlockUsePlayerData, BlockUseSide,
    ExecutionBudget, InteractionResultValue, MenuEffect, OpenMenuEffect, PlayerContainer,
    SwingSource,
};
use crate::{JAR_BUDGET, LOG, LoadedJar, State, caught, dimension_id};

/// Per-jar block-use and open-menu metrics included in `java-runtime.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    pub(crate) uses_ran: usize,
    pub(crate) uses_failed: usize,
    pub(crate) unknown_host_calls: usize,
    pub(crate) menus_opened: usize,
    pub(crate) menus_refused: usize,
    pub(crate) refusals: BTreeMap<String, usize>,
    /// Live-menu calls (clicks, ticks, validity checks, closes) run on a held Java menu.
    pub(crate) menu_calls: usize,
}

impl Tally {
    pub(crate) fn note_refusal(&mut self, reason: impl Into<String>) {
        self.menus_refused += 1;
        let reason = reason.into();
        if self.refusals.len() < 32 || self.refusals.contains_key(&reason) {
            *self.refusals.entry(reason).or_default() += 1;
        }
    }

    pub(crate) fn json(&self) -> serde_json::Value {
        json!({
            "uses_ran": self.uses_ran,
            "uses_failed": self.uses_failed,
            "unknown_host_call_count": self.unknown_host_calls,
            "menus_opened": self.menus_opened,
            "menus_refused": self.menus_refused,
            "refusals": self.refusals,
            "menu_calls": self.menu_calls,
        })
    }
}

/// Scoped Sunrise [`BlockPart`] attached to a Java mod block at `Hooks::register`.
pub(crate) struct JavaBlockUsePart {
    state: Arc<Mutex<State>>,
    block_id: String,
}

impl JavaBlockUsePart {
    pub(crate) fn new(state: Arc<Mutex<State>>, block_id: String) -> Self {
        Self { state, block_id }
    }

    pub(crate) fn dispatch(
        &self,
        host: &mut dyn ServerHost,
        state_val: mod_api::BlockState,
        click: &BlockClick<'_>,
        with_item: bool,
    ) -> Option<InteractionResult> {
        // The Java run holds the state lock; Sunrise's menu calls run without it, since
        // opening and closing a window fires `player_container_open` and `menu_closed`, which
        // this mod listens to and which take the lock themselves.
        let (jar_index, jar_id, outcome) =
            self.run_java(&mut *host, state_val, click, with_item)?;
        let hook_name = if with_item {
            "useItemOn"
        } else {
            "useWithoutItem"
        };
        let openings: Vec<Opening> = match &outcome {
            Ok(outcome) => {
                let mut guard = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard
                    .jars
                    .get(jar_index)
                    .is_some_and(|jar| jar.row.id == jar_id)
                    .then(|| {
                        crate::live_menu::opening_check(
                            &mut guard,
                            jar_index,
                            click.player,
                            &outcome.live_effects,
                        )
                    })
                    .flatten()
                    .into_iter()
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        let applied = match &outcome {
            Ok(outcome) => apply_menu_effects(host, click.player, &outcome.menu_effects, &openings),
            Err(_) => Vec::new(),
        };
        let opening_counted =
            self.show_opened_menu(host, jar_index, &jar_id, click.player, &applied);

        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let jar = guard
            .jars
            .get_mut(jar_index)
            .filter(|jar| jar.row.id == jar_id)?;
        match outcome {
            Ok(outcome) => {
                jar.row.block_use.uses_ran += 1;
                jar.row.block_use.unknown_host_calls += outcome.unknown_host_call_count;
                if outcome.unknown_host_call_count > 0 {
                    log::warn!(
                        "{LOG}: {}: {} {} leaned on {} unmodelled host call(s): {:?}",
                        jar.row.id,
                        self.block_id,
                        hook_name,
                        outcome.unknown_host_call_count,
                        outcome.unknown_host_calls
                    );
                }
                for applied in applied {
                    record_menu_outcome(jar, click.player, applied);
                }
                for tag in opening_counted {
                    jar.host.note_unknown_game_hook(tag);
                }
                if let Some(failure) = &outcome.failure {
                    // The hook threw (vanilla's use would fail too); what it did to menus before
                    // that went out above.
                    jar.row.block_use.uses_failed += 1;
                    log::warn!(
                        "{LOG}: {}: {} {} failed: {failure}",
                        jar.row.id,
                        self.block_id,
                        hook_name
                    );
                    return None;
                }
                match outcome.result {
                    Some(ref result) => {
                        if let InteractionResultValue::Success { item_context, .. } = result {
                            if item_context.held_item_transformed_to.is_some() {
                                jar.host.note_unknown_game_hook(
                                    "sunrise#block_use:held_item_transformed_to",
                                );
                                log::warn!(
                                    "{LOG}: {}: {} {} returned heldItemTransformedTo stack; Sunrise BlockPart::use_* cannot rewrite the held stack",
                                    jar.row.id,
                                    self.block_id,
                                    hook_name
                                );
                            }
                            if !item_context.was_item_interaction {
                                jar.host.note_unknown_game_hook(
                                    "sunrise#block_use:without_item_context",
                                );
                                log::warn!(
                                    "{LOG}: {}: {} {} returned InteractionResult.Success.withoutItem(); Sunrise InteractionResult does not carry ItemContext.NONE",
                                    jar.row.id,
                                    self.block_id,
                                    hook_name
                                );
                            }
                        }
                        Some(map_interaction_result(result))
                    }
                    None => {
                        let reason = format!(
                            "{}: {} returned null or unmodelled InteractionResult",
                            self.block_id, hook_name
                        );
                        jar.row.block_use.uses_failed += 1;
                        jar.host
                            .note_unknown_game_hook("block-use:unmodelled-interaction-result");
                        log::warn!("{LOG}: {}: {reason}", jar.row.id);
                        None
                    }
                }
            }
            Err(error) => {
                jar.row.block_use.uses_failed += 1;
                log::warn!(
                    "{LOG}: {}: {} {} {error}",
                    jar.row.id,
                    self.block_id,
                    hook_name
                );
                None
            }
        }
    }

    /// A window Sunrise shows, after a use's effects, for the Java menu the jar now holds:
    /// check Sunrise's layout is the Java menu's (the bake declared it from the same
    /// constructor; a mismatch closes the window, whose close then runs the Java close), then
    /// show what the Java menu holds (`ServerPlayer$1.sendInitialData`). The hook tags to count.
    fn show_opened_menu(
        &self,
        host: &mut dyn ServerHost,
        jar_index: usize,
        jar_id: &str,
        uuid: u128,
        applied: &[MenuApplied],
    ) -> Vec<&'static str> {
        let Some(MenuApplied::Shown(PlayerContainer::Open(window))) = applied.last() else {
            return Vec::new();
        };
        let window = *window;
        let prepared = {
            let mut guard = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(jar) = guard
                .jars
                .get_mut(jar_index)
                .filter(|jar| jar.row.id == jar_id)
            else {
                return Vec::new();
            };
            crate::live_menu::opening_contents(jar, &*host, jar_index, uuid, window)
        };
        let Some((prepared, roles)) = prepared else {
            return Vec::new();
        };
        let menu = host
            .player(uuid)
            .and_then(|player| player.container_menu(&*host))
            .filter(|menu| menu.container_id(&*host) == Some(window));
        if !menu.is_some_and(|menu| crate::live_menu::window_matches(&*host, menu, &roles)) {
            log::warn!(
                "{LOG}: {jar_id}: Sunrise's window {window} does not lay out the Java menu's {} slots as Java does; closing it",
                roles.len()
            );
            if let Some(menu) = menu {
                menu.close_container(host);
            }
            return vec!["sunrise#openModMenu:layout_mismatch"];
        }
        let mut counted = prepared.counted.clone();
        counted.extend(crate::live_menu::apply(host, &prepared).counted);
        counted
    }

    /// Run the jar's `useItemOn`/`useWithoutItem` for this click under the state lock: the
    /// jar's index and id, and its outcome or why it has none (an error or a panic).
    fn run_java(
        &self,
        host: &mut dyn ServerHost,
        state_val: mod_api::BlockState,
        click: &BlockClick<'_>,
        with_item: bool,
    ) -> Option<(usize, String, Result<BlockUseOutcome, String>)> {
        let state_str = host.block_state_string(state_val);
        let (resolved_block_id, state_properties) = match state_str.as_deref() {
            Some(raw) => {
                let (id, props) = parse_block_state_str(raw);
                (id.to_owned(), props)
            }
            None => (self.block_id.clone(), BTreeMap::new()),
        };
        if resolved_block_id != self.block_id {
            return None;
        }

        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Which `ServerPlayer` object this is (a login's session, a respawn's life); `None`
        // for a player whose login this mod never heard, whose object it can't name.
        let incarnation = guard.lives.get(&click.player).copied();
        let facade_name = guard
            .players
            .iter()
            .find(|p| p.uuid == click.player)
            .map(|p| p.name.clone());

        let player_ref = host.player(click.player);
        let player_name = player_ref
            .and_then(|p| p.name(&*host).map(str::to_owned))
            .or(facade_name)
            .unwrap_or_else(|| "Player".to_owned());
        let sneaking = player_ref
            .and_then(|p| p.entity().is_shift_key_down(&*host))
            .unwrap_or(false);
        let spectator = player_ref
            .and_then(|p| p.is_spectator(&*host))
            .unwrap_or(false);
        let container = player_container(&*host, player_ref);

        let jar_index = guard
            .jars
            .iter()
            .position(|jar| jar.host.registered_block(&self.block_id).is_some())?;

        let jar = &mut guard.jars[jar_index];

        // `ServerPlayer.containerCounter` is Sunrise's (`PlayerRef::container_counter`), so the
        // id Java hands `createMenu` is the one Sunrise's open takes next: the two can't drift.
        if let Some(counter) = player_ref.and_then(|player| player.container_counter(&*host)) {
            jar.host
                .server_player_container_opened(click.player, Some(counter));
        }
        // The player as Sunrise has them, so a menu this use opens runs `initMenu` exactly
        // (its listener and synchronizer read the player's inventory slots).
        let facts = player_ref.map(|player| {
            crate::live_menu::read_player(
                &*host,
                &mut jar.host,
                incarnation,
                player,
                None,
                container,
                None,
            )
            .facts
        });
        jar.host.set_player_menu_facts(click.player, facts);

        // A held stack whose item the host cannot name has no Java form: the use fails and is
        // counted rather than run with an empty hand Java would not see.
        let clicked_stack = match click.stack {
            None => None,
            Some(s) => {
                match crate::client::runtime_stack_from_api(&*host, &mut jar.host, None, s) {
                    Some(value) => Some(value),
                    None => {
                        jar.host
                            .note_unknown_game_hook("block-use:held item has no registry id");
                        let reason = "the held stack's item has no registry id".to_owned();
                        return Some((jar_index, jar.row.id.clone(), Err(reason)));
                    }
                }
            }
        };
        let stack = if with_item {
            clicked_stack.clone()
        } else {
            None
        };
        let hand_name = match click.hand {
            Hand::Main => "MAIN_HAND",
            Hand::Off => "OFF_HAND",
        };
        let (main_hand, off_hand) = match (click.hand, clicked_stack) {
            (Hand::Main, Some(s)) => (Some(s), None),
            (Hand::Off, Some(s)) => (None, Some(s)),
            _ => (None, None),
        };

        let context = BlockUseContext {
            side: match click.side {
                LogicalSide::Server => BlockUseSide::Server,
                LogicalSide::Client => BlockUseSide::Client,
            },
            dimension: dimension_id(click.dimension).to_owned(),
            block_id: self.block_id.clone(),
            state_properties,
            pos: click.pos,
            hit: BlockHitData::block(
                click.pos,
                face_to_direction(click.hit.face),
                click.hit.point,
                click.hit.inside,
            ),
            player: BlockUsePlayerData {
                uuid: click.player,
                name: player_name,
                entity_id: Some(click.player_entity_id),
                incarnation,
                container,
                sneaking,
                spectator,
                main_hand,
                off_hand,
            },
            hand: hand_name.to_owned(),
            stack,
        };

        let hook_name = if with_item {
            "useItemOn"
        } else {
            "useWithoutItem"
        };
        let budget = ExecutionBudget::within(JAR_BUDGET);
        let label = format!("{} block {} {}", jar.row.id, self.block_id, hook_name);
        let outcome = caught(&label, || {
            if with_item {
                jar.jar
                    .block_use_item_on_with_budget(&mut jar.host, &context, &budget)
            } else {
                jar.jar
                    .block_use_without_item_with_budget(&mut jar.host, &context, &budget)
            }
        });

        let stdout = jar.host.take_stdout();
        for line in stdout.lines() {
            log::info!("{LOG}: {}: {line}", jar.row.id);
        }
        let outcome = match outcome {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(error)) => Err(format!("failed: {error}")),
            Err(panic) => Err(format!("panicked: {panic}")),
        };
        Some((jar_index, jar.row.id.clone(), outcome))
    }
}

/// The player's open container as Sunrise reports it: the inventory menu is container 0. A
/// menu Sunrise has no handle for (crafting tables, merchants, workstations) is unknown.
fn player_container(host: &dyn ServerHost, player: Option<PlayerRef>) -> PlayerContainer {
    match player
        .and_then(|player| player.container_menu(host))
        .and_then(|menu| menu.container_id(host))
    {
        Some(0) => PlayerContainer::Inventory,
        Some(id) => PlayerContainer::Open(id),
        None => PlayerContainer::Unknown,
    }
}

#[mod_api::sunrise_part]
impl BlockPart for JavaBlockUsePart {
    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        self.dispatch(host, event.state, event.click, true)
    }

    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        self.dispatch(host, event.state, event.click, false)
    }
}

/// Exact 1-to-1 mapping from MC 26.3's sealed `InteractionResult` to Sunrise's
/// [`mod_api::InteractionResult`] (`ServerPlayerGameMode.useItemOn` +
/// `ServerGamePacketListenerImpl.handleUseItemOn`).
pub(crate) fn map_interaction_result(value: &InteractionResultValue) -> InteractionResult {
    match value {
        InteractionResultValue::Success {
            swing_source: SwingSource::Predicted,
            ..
        } => InteractionResult::Success,
        InteractionResultValue::Success {
            swing_source: SwingSource::ServerOnly,
            ..
        } => InteractionResult::SuccessServer,
        InteractionResultValue::Success {
            swing_source: SwingSource::None,
            ..
        } => InteractionResult::Consume,
        InteractionResultValue::Fail => InteractionResult::Fail,
        InteractionResultValue::Pass => InteractionResult::Pass,
        InteractionResultValue::TryEmptyHandInteraction => InteractionResult::TryWithEmptyHand,
    }
}

/// Parse Sunrise's canonical `/setblock` block-state string (`ns:path` or `ns:path[k=v,...]`).
pub(crate) fn parse_block_state_str(raw: &str) -> (&str, BTreeMap<String, String>) {
    let Some((id, rest)) = raw.split_once('[') else {
        return (raw, BTreeMap::new());
    };
    let mut props = BTreeMap::new();
    let inner = rest.strip_suffix(']').unwrap_or(rest);
    for pair in inner.split(',') {
        if let Some((k, v)) = pair.split_once('=')
            && !k.is_empty()
        {
            props.insert(k.to_owned(), v.to_owned());
        }
    }
    (id, props)
}

/// Map Sunrise's unit face normal (`BlockHit::face`) to vanilla `Direction` enum constant name.
pub(crate) fn face_to_direction(face: [i32; 3]) -> &'static str {
    match face {
        [0, -1, 0] => "DOWN",
        [0, 1, 0] => "UP",
        [0, 0, -1] => "NORTH",
        [0, 0, 1] => "SOUTH",
        [-1, 0, 0] => "WEST",
        [1, 0, 0] => "EAST",
        _ => "UP",
    }
}

/// What Sunrise made of one of a use's [`MenuEffect`]s.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MenuApplied {
    /// Java opened menu `container_id`; Sunrise opened it or refused.
    Opened {
        container_id: i32,
        attempt: SunriseMenuAttempt,
    },
    /// Java closed menu `container_id` and opened nothing after it: whether Sunrise closed the
    /// window too.
    Closed { container_id: i32, closed: bool },
    /// Java's server discarded menu `container_id` without closing it, while its window stays:
    /// Sunrise has no way to forget a menu it shows.
    Orphaned { container_id: i32 },
    /// After all of a call's effects: the window Sunrise shows the player.
    Shown(PlayerContainer),
    /// Java advanced `containerCounter` to `container_id` for a `createMenu` that answered
    /// `null`; Sunrise advanced its own to `sunrise` (`PlayerRef::next_container_counter`).
    CounterAdvanced {
        container_id: i32,
        sunrise: Option<i32>,
    },
}

/// A Java menu a call left held, checked before its window opens (`live_menu::opening_check`):
/// the data slot values it opens with, or why it must not open (its live layout is not its
/// type's declared one, its initial data isn't known).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Opening {
    pub(crate) container_id: i32,
    pub(crate) data: Result<Vec<i32>, String>,
}

/// Carry a use's menu effects out on Sunrise, in order, without the state lock. An open only
/// goes out for a menu `openings` checked; any other is refused, never a window whose layout
/// nothing compared with its type's.
pub(crate) fn apply_menu_effects(
    host: &mut dyn ServerHost,
    player_uuid: u128,
    effects: &[MenuEffect],
    openings: &[Opening],
) -> Vec<MenuApplied> {
    deliver_menu_effects(
        &mut SunriseMenus {
            host,
            player: player_uuid,
        },
        effects,
        openings,
    )
}

/// Where a call's menu effects go: Sunrise ([`SunriseMenus`]), or a test's recorder.
pub(crate) trait MenuSink {
    /// Open Java's menu as a Sunrise mod menu with its initial data slot values, replacing the
    /// open one ([`MenuReplace::CloseFirst`]: its close, then the open, as vanilla's packets go).
    fn open(&mut self, effect: &OpenMenuEffect, data: Vec<i32>) -> SunriseMenuAttempt;
    /// Close window `container_id`, if it is the one shown.
    fn close(&mut self, container_id: i32) -> bool;
    /// The window shown now.
    fn shown(&mut self) -> PlayerContainer;
    /// Advance the player's container counter with no window, as `ServerPlayer.openMenu` does
    /// before a `createMenu` that answers `null`: the new counter.
    fn advance_counter(&mut self) -> Option<i32>;
}

/// The player's menus on Sunrise.
struct SunriseMenus<'a> {
    host: &'a mut dyn ServerHost,
    player: u128,
}

impl MenuSink for SunriseMenus<'_> {
    fn open(&mut self, effect: &OpenMenuEffect, data: Vec<i32>) -> SunriseMenuAttempt {
        try_open_sunrise_menu(self.host, self.player, effect, data)
    }

    fn close(&mut self, container_id: i32) -> bool {
        let host = &mut *self.host;
        let window = host
            .player(self.player)
            .and_then(|player| player.container_menu(&*host))
            .filter(|menu| menu.container_id(&*host) == Some(container_id));
        // Sunrise refuses this during the player's own action (a block use is one): the
        // caller then counts the window it couldn't close.
        window.is_some_and(|menu| menu.close_container(host))
    }

    fn shown(&mut self) -> PlayerContainer {
        let player = self.host.player(self.player);
        player_container(&*self.host, player)
    }

    fn advance_counter(&mut self) -> Option<i32> {
        let player = self.host.player(self.player)?;
        player.next_container_counter(self.host)
    }
}

/// The delivery order. Java's `closeContainer` sends its close packet before anything of the
/// next open is built, unconditionally, so every [`MenuEffect::Closed`] closes its window:
/// - followed by an open Sunrise carries out, the open's [`MenuReplace::CloseFirst`] does it,
///   in one call that sends the close before the open, as vanilla's packets go; closing first
///   ourselves instead isn't possible inside a use (Sunrise refuses `close_container` for the
///   acting player) and would double the close;
/// - followed by an open Sunrise refuses or a mod cancels (which leaves Sunrise's old window
///   as it was), it is closed right after that attempt, which itself changed nothing;
/// - followed by no open, it is closed then.
fn deliver_menu_effects(
    sink: &mut dyn MenuSink,
    effects: &[MenuEffect],
    openings: &[Opening],
) -> Vec<MenuApplied> {
    let mut applied = Vec::new();
    for (index, effect) in effects.iter().enumerate() {
        match effect {
            MenuEffect::Opened(open) => {
                let checked = openings
                    .iter()
                    .find(|opening| opening.container_id == open.container_id)
                    .map(|opening| opening.data.clone())
                    .unwrap_or_else(|| {
                        Err("the menu's layout was not compared with its type's".to_owned())
                    });
                let attempt = match checked {
                    Ok(data) => sink.open(open, data),
                    Err(reason) => SunriseMenuAttempt::Refused {
                        hook_tag: "sunrise#openModMenu:layout_unverified",
                        reason: format!("openMenu: not opened: {reason}"),
                    },
                };
                let replaced = matches!(attempt, SunriseMenuAttempt::Opened(_));
                applied.push(MenuApplied::Opened {
                    container_id: open.container_id,
                    attempt,
                });
                if !replaced
                    && let Some(MenuEffect::Closed { container_id }) =
                        index.checked_sub(1).and_then(|before| effects.get(before))
                {
                    applied.push(MenuApplied::Closed {
                        container_id: *container_id,
                        closed: sink.close(*container_id),
                    });
                }
            }
            MenuEffect::Orphaned { container_id } => applied.push(MenuApplied::Orphaned {
                container_id: *container_id,
            }),
            MenuEffect::CounterAdvanced { container_id } => {
                applied.push(MenuApplied::CounterAdvanced {
                    container_id: *container_id,
                    sunrise: sink.advance_counter(),
                })
            }
            MenuEffect::Closed { container_id } => {
                // Carried out by (or after) the open that follows.
                if matches!(effects.get(index + 1), Some(MenuEffect::Opened(_))) {
                    continue;
                }
                applied.push(MenuApplied::Closed {
                    container_id: *container_id,
                    closed: sink.close(*container_id),
                });
            }
        }
    }
    if !effects.is_empty() {
        applied.push(MenuApplied::Shown(sink.shown()));
    }
    applied
}

/// Record what Sunrise did with one effect on the jar: tallies, counted refusals, and the
/// jar's hold on the menu, which it keeps only while Sunrise shows it under the id Java gave it.
pub(crate) fn record_menu_outcome(jar: &mut LoadedJar, player_uuid: u128, applied: MenuApplied) {
    match applied {
        MenuApplied::Opened {
            container_id,
            attempt: SunriseMenuAttempt::Opened(window),
        } => {
            jar.row.block_use.menus_opened += 1;
            log::info!(
                "{LOG}: {}: opened Sunrise menu window {window} for container {container_id}",
                jar.row.id
            );
            if window != container_id {
                // Java built its menu with an id the client won't use: nothing can bridge it.
                jar.host
                    .note_unknown_game_hook("sunrise#openModMenu:container_id_mismatch");
                log::warn!(
                    "{LOG}: {}: Sunrise opened window {window} for Java container {container_id}",
                    jar.row.id
                );
                jar.host.refuse_open_menu(player_uuid, container_id);
            }
            // Sunrise's window id is the `ServerPlayer`'s `containerCounter` now.
            jar.host
                .server_player_container_opened(player_uuid, Some(window));
        }
        MenuApplied::Opened {
            container_id,
            attempt: SunriseMenuAttempt::Refused { hook_tag, reason },
        } => {
            jar.row.block_use.note_refusal(reason.clone());
            jar.host.note_unknown_game_hook(hook_tag);
            log::warn!("{LOG}: {}: {reason}", jar.row.id);
            // Nothing shows the menu: the jar stops holding it, and its counter (Java advanced
            // it, Sunrise didn't) is no longer known.
            jar.host.refuse_open_menu(player_uuid, container_id);
        }
        MenuApplied::Closed {
            container_id,
            closed,
        } => {
            if !closed {
                jar.host
                    .note_unknown_game_hook("sunrise#closeContainer:window_not_closed");
                log::warn!(
                    "{LOG}: {}: Java closed container {container_id}, but Sunrise did not close its window",
                    jar.row.id
                );
            }
        }
        MenuApplied::Orphaned { container_id } => {
            jar.host
                .note_unknown_game_hook("sunrise#containerMenu:orphaned");
            log::warn!(
                "{LOG}: {}: Java discarded container {container_id} without closing it; Sunrise keeps its window",
                jar.row.id
            );
        }
        MenuApplied::CounterAdvanced {
            container_id,
            sunrise,
        } => {
            if sunrise != Some(container_id) {
                jar.host
                    .note_unknown_game_hook("sunrise#nextContainerCounter:mismatch");
                log::warn!(
                    "{LOG}: {}: Java advanced the container counter to {container_id}, Sunrise to {sunrise:?}",
                    jar.row.id
                );
            }
            // Sunrise's counter is the `ServerPlayer`'s now.
            jar.host
                .server_player_container_opened(player_uuid, sunrise);
        }
        MenuApplied::Shown(shown) => {
            // The jar holds a menu vanilla still has installed (its `removed` threw after the
            // close packet went out) while Sunrise shows no window for it.
            if let Some(held) = jar
                .host
                .player_open_menu(player_uuid)
                .map(|open| open.container_id)
                && shown != PlayerContainer::Open(held)
            {
                jar.host
                    .note_unknown_game_hook("sunrise#containerMenu:installed_without_window");
                log::warn!(
                    "{LOG}: {}: Java still has container {held} installed, but Sunrise shows {shown:?}",
                    jar.row.id
                );
            }
        }
    }
}

/// `menu_closed`: a window a mod opened closed (the player closed it, something replaced it, or
/// the player left). A jar that holds its Java menu runs the menu's close
/// (`ServerPlayer.doCloseContainer`: its `removed(player)`) under the state lock; whatever that
/// `removed` opened or closed then goes out on Sunrise without the lock (which those calls'
/// own hooks take), and is recorded under it again.
pub(crate) fn menu_closed(
    java: &crate::HorizonJava,
    host: &mut dyn ServerHost,
    player_uuid: u128,
    window: i32,
) {
    // A player Sunrise can still read: the live close, with their facts.
    if crate::live_menu::close_held(java, host, player_uuid, window) {
        return;
    }
    let closes = {
        let mut state = java.lock();
        run_menu_closes(&mut state, &*host, player_uuid, window)
    };
    let delivered: Vec<(usize, String, Vec<MenuApplied>)> = closes
        .into_iter()
        .map(|(index, id, effects)| {
            let applied = apply_menu_effects(host, player_uuid, &effects, &[]);
            (index, id, applied)
        })
        .collect();
    if delivered.iter().all(|(_, _, applied)| applied.is_empty()) {
        return;
    }
    let mut state = java.lock();
    for (index, id, applied) in delivered {
        let Some(jar) = state.jars.get_mut(index).filter(|jar| jar.row.id == id) else {
            continue;
        };
        for applied in applied {
            record_menu_outcome(jar, player_uuid, applied);
        }
    }
}

/// Run the close of menu `window` in every jar that holds it for the player: each jar's index,
/// id and the menu effects its `removed` made.
fn run_menu_closes(
    state: &mut crate::State,
    host: &dyn ServerHost,
    player_uuid: u128,
    window: i32,
) -> Vec<(usize, String, Vec<MenuEffect>)> {
    let holders: Vec<usize> = state
        .jars
        .iter()
        .enumerate()
        .filter(|(_, jar)| {
            jar.host
                .player_open_menu(player_uuid)
                .is_some_and(|open| open.container_id == window)
        })
        .map(|(index, _)| index)
        .collect();
    if holders.is_empty() {
        return Vec::new();
    }
    let player_ref = host.player(player_uuid);
    let facade = state.players.iter().find(|p| p.uuid == player_uuid);
    let dimension = player_ref
        .and_then(|player| player.entity().level(host))
        .map(|level| level.dimension_id().to_owned())
        .or_else(|| facade.and_then(|p| p.level.clone()));
    let player = BlockUsePlayerData {
        uuid: player_uuid,
        name: player_ref
            .and_then(|p| p.name(host).map(str::to_owned))
            .or_else(|| facade.map(|p| p.name.clone()))
            .unwrap_or_else(|| "Player".to_owned()),
        entity_id: player_ref
            .map(|p| p.entity().id() as i32)
            .or_else(|| facade.and_then(|p| p.entity_id)),
        incarnation: state.lives.get(&player_uuid).copied(),
        container: PlayerContainer::Inventory,
        sneaking: player_ref
            .and_then(|p| p.entity().is_shift_key_down(host))
            .unwrap_or(false),
        spectator: player_ref
            .and_then(|p| p.is_spectator(host))
            .unwrap_or(false),
        main_hand: None,
        off_hand: None,
    };
    let mut effects = Vec::new();
    for index in holders {
        let jar = &mut state.jars[index];
        let Some(dimension) = dimension.as_deref() else {
            // No level to hand `removed`: the close can't run here, so it is counted. The jar
            // keeps the menu until the player's next block use counts the missed close, or
            // their logout drops it.
            jar.host
                .note_unknown_game_hook("sunrise#menu_closed:unknown_level");
            continue;
        };
        let budget = ExecutionBudget::within(JAR_BUDGET);
        let label = format!("{} menu {window} close", jar.row.id);
        let closed = caught(&label, || {
            jar.jar
                .close_player_menu(&mut jar.host, &player, dimension, window, &budget)
        });
        match closed {
            Ok(Ok(Some(outcome))) => {
                jar.row.block_use.unknown_host_calls += outcome.unknown_host_call_count;
                if outcome.unknown_host_call_count > 0 {
                    log::warn!(
                        "{LOG}: {}: closing menu {window} leaned on {} unmodelled host call(s): {:?}",
                        jar.row.id,
                        outcome.unknown_host_call_count,
                        outcome.unknown_host_calls
                    );
                }
                if let Some(failure) = &outcome.failure {
                    // Vanilla's `handleContainerClose` would throw too; the menu stays
                    // installed, and the player's next `openMenu` closes it again.
                    jar.row.block_use.uses_failed += 1;
                    log::warn!(
                        "{LOG}: {}: closing menu {window} threw: {failure}",
                        jar.row.id
                    );
                }
                effects.push((index, jar.row.id.clone(), outcome.menu_effects));
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                jar.row.block_use.uses_failed += 1;
                log::warn!(
                    "{LOG}: {}: closing menu {window} failed: {error}",
                    jar.row.id
                );
            }
            Err(panic) => {
                jar.row.block_use.uses_failed += 1;
                log::warn!(
                    "{LOG}: {}: closing menu {window} panicked: {panic}",
                    jar.row.id
                );
            }
        }
    }
    effects
}

/// `player_container_open`: a menu opened for the player (any opener). Its container id is the
/// `ServerPlayer`'s `containerCounter` now, in every jar; a menu Sunrise has no handle for
/// leaves the counter unknown.
pub(crate) fn container_opened(
    state: &mut crate::State,
    player_uuid: u128,
    container_id: Option<i32>,
) {
    for jar in &mut state.jars {
        jar.host
            .server_player_container_opened(player_uuid, container_id);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SunriseMenuAttempt {
    Opened(i32),
    Refused {
        hook_tag: &'static str,
        reason: String,
    },
}

fn try_open_sunrise_menu(
    host: &mut dyn ServerHost,
    player_uuid: u128,
    effect: &OpenMenuEffect,
    data: Vec<i32>,
) -> SunriseMenuAttempt {
    let Some(menu_id) = effect.menu_type_id.as_deref() else {
        return SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:unknown_menu_type",
            reason: "openMenu: MenuType is not registered in minecraft:menu".to_owned(),
        };
    };
    let menu_key = host
        .menu_keys()
        .into_iter()
        .find(|key| key.id(&*host).is_some_and(|id| id == menu_id));
    let Some(menu_key) = menu_key else {
        return SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:unregistered_menu_type",
            reason: format!(
                "openMenu: menu type `{menu_id}` is not registered with Sunrise \
                 (the bake declares a jar's menu types it can lay out exactly)"
            ),
        };
    };
    let Some(title_json) = &effect.title_json else {
        return SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:unconvertible_title",
            reason: format!("openMenu: title Component for `{menu_id}` has no vanilla JSON form"),
        };
    };
    let title_str = title_json.to_string();
    let title_text = match host.text_from_json(&title_str) {
        Ok(Some(text)) => text,
        Ok(None) => {
            return SunriseMenuAttempt::Refused {
                hook_tag: "sunrise#openModMenu:rejected_title",
                reason: format!("openMenu: Sunrise rejected title JSON for `{menu_id}`"),
            };
        }
        Err(error) => {
            return SunriseMenuAttempt::Refused {
                hook_tag: "sunrise#openModMenu:title_error",
                reason: format!("openMenu: title conversion failed for `{menu_id}`: {error:?}"),
            };
        }
    };
    let Some(player_ref) = host.player(player_uuid) else {
        return SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:player_unavailable",
            reason: format!("openMenu: player {player_uuid:032x} unavailable on ServerHost"),
        };
    };

    // No opening bytes and no opening hand (a block opened it), the Java menu's initial data
    // slot values, and the old window closed first.
    let mut request = ModMenuOpen::new(menu_key, title_text);
    request.data = data;
    request.pos = effect.access_pos;
    request.replace = MenuReplace::CloseFirst;
    let result = player_ref.open_mod_menu(host, request);
    classify_open_mod_menu_result(menu_id, result)
}

fn classify_open_mod_menu_result(
    menu_id: &str,
    result: Result<Option<i32>, OpenMenuError>,
) -> SunriseMenuAttempt {
    match result {
        Ok(Some(window_id)) => SunriseMenuAttempt::Opened(window_id),
        Ok(None) => SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:cancelled",
            reason: format!("openMenu: Sunrise cancelled opening `{menu_id}`"),
        },
        Err(OpenMenuError::NotModMenu) => SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:not_mod_menu",
            reason: format!("openMenu: `{menu_id}` is not a Sunrise content mod menu (NotModMenu)"),
        },
        Err(OpenMenuError::PlayerUnavailable) => SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:player_unavailable",
            reason: format!("openMenu: player unavailable for `{menu_id}`"),
        },
        Err(OpenMenuError::TooLarge) => SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:extra_too_large",
            reason: format!("openMenu: extra menu payload too large for `{menu_id}`"),
        },
        Err(OpenMenuError::DataCountMismatch) => SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:data_count_mismatch",
            reason: format!("openMenu: `{menu_id}` declares another number of data slots"),
        },
        Err(OpenMenuError::Title(error)) => SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:title_error",
            reason: format!("openMenu: title rejected for `{menu_id}`: {error:?}"),
        },
        Err(other) => SunriseMenuAttempt::Refused {
            hook_tag: "sunrise#openModMenu:error",
            reason: format!("openMenu: Sunrise refused `{menu_id}`: {other:?}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::runtime::{InteractionItemContext, NativeJar, StdHost};
    use mod_api::{
        Backend, BackendMut, BlockHit, BlockKey, BlockState, EntityRef, Host, LevelDimension,
        PlayerRef, Text, WorldEpoch,
    };

    struct BlockUseTestHost {
        state_string: Option<String>,
        player_uuid: u128,
        player_entity_id: u32,
        player_name: String,
    }

    impl BlockUseTestHost {
        fn new(state_string: Option<&str>) -> Self {
            Self {
                state_string: state_string.map(str::to_owned),
                player_uuid: 0x0000_0000_0000_0000_0000_0000_0000_002a,
                player_entity_id: 7,
                player_name: "Alex".to_owned(),
            }
        }
    }

    impl Host for BlockUseTestHost {
        fn backend(&self) -> Option<mod_api::BackendRef<'_>> {
            Some(mod_api::__private::backend_ref(self))
        }
    }
    impl mod_api::prelude::BlockStatesHost for BlockUseTestHost {
        fn block_state_string(&self, _state: BlockState) -> Option<String> {
            self.state_string.clone()
        }
    }
    impl mod_api::prelude::PlayersHost for BlockUseTestHost {
        fn player(&self, uuid: u128) -> Option<PlayerRef> {
            (uuid == self.player_uuid).then(|| {
                let entity = EntityRef::new(WorldEpoch::new(1), self.player_entity_id);
                mod_api::__private::game::player_ref(entity, uuid)
            })
        }
    }
    impl mod_api::prelude::KeysHost for BlockUseTestHost {
        fn menu_keys(&self) -> Vec<mod_api::MenuKey> {
            Vec::new()
        }
    }
    impl mod_api::prelude::TextHost for BlockUseTestHost {
        fn text_from_json(&self, json: &str) -> Result<Option<Text>, mod_api::TextFormError> {
            Ok(Some(Text::literal(json)))
        }
    }
    mod_api::impl_host_areas!(
        BlockUseTestHost,
        except(block_states, keys, living, players, text)
    );

    impl ServerHost for BlockUseTestHost {
        fn backend_mut(&mut self) -> Option<mod_api::BackendMutRef<'_>> {
            Some(mod_api::__private::parts_installation::backend_mut_ref(
                self,
            ))
        }
    }
    mod_api::impl_server_host_areas!(BlockUseTestHost);

    impl mod_api::prelude::PlayerSessionBackend for BlockUseTestHost {
        fn player_name(&self, player: PlayerRef) -> Option<&str> {
            (player.uuid() == self.player_uuid).then_some(self.player_name.as_str())
        }
    }
    impl mod_api::prelude::EntityStanceBackend for BlockUseTestHost {
        fn entity_is_shift_key_down(&self, _entity: EntityRef) -> Option<bool> {
            Some(false)
        }
    }
    impl mod_api::prelude::PlayerModesBackend for BlockUseTestHost {
        fn player_is_spectator(&self, _player: PlayerRef) -> Option<bool> {
            Some(false)
        }
        fn player_has_infinite_materials(&self, _player: PlayerRef) -> Option<bool> {
            Some(false)
        }
    }
    /// The player has only their inventory menu open (container 0), and opened five windows
    /// before.
    impl mod_api::prelude::PlayerMenusBackend for BlockUseTestHost {
        fn player_container_menu(&self, player: PlayerRef) -> Option<mod_api::MenuRef> {
            (player.uuid() == self.player_uuid)
                .then(|| mod_api::__private::game::menu_ref(player, 0, 1))
        }
        fn player_container_counter(&self, player: PlayerRef) -> Option<i32> {
            (player.uuid() == self.player_uuid).then_some(5)
        }
    }
    /// An empty inventory, the first hotbar slot selected.
    impl mod_api::prelude::PlayerInventoryBackend for BlockUseTestHost {
        fn player_selected_slot(&self, _player: PlayerRef) -> Option<usize> {
            Some(0)
        }
        fn player_inventory_slots(
            &self,
            _player: PlayerRef,
            read: &mut dyn for<'a> FnMut(mod_api::InventoryView<'a>),
        ) -> Option<()> {
            let metadata = |_slot: u32| None;
            let project =
                |_slot: u32, _: (mod_api::ItemKey, std::num::NonZeroU32, Option<u32>)| None;
            read(mod_api::InventoryView::new(0..43, &metadata, &project));
            Some(())
        }
    }
    /// What the live-menu facts read of the player's body: no level, no living handle, so
    /// only the inventory is known.
    impl mod_api::prelude::EntitySightBackend for BlockUseTestHost {
        fn entity_eye_position(&self, _entity: EntityRef) -> Option<[f64; 3]> {
            None
        }
    }
    impl mod_api::prelude::EntityLifetimeBackend for BlockUseTestHost {
        fn entity_tick_count(&self, _entity: EntityRef) -> Option<i32> {
            None
        }
    }
    impl mod_api::prelude::EntityIdentityBackend for BlockUseTestHost {
        fn entity_level(&self, _entity: EntityRef) -> Option<mod_api::LevelRef> {
            None
        }
    }
    impl mod_api::prelude::EntityPositionBackend for BlockUseTestHost {
        fn entity_position(&self, _entity: EntityRef) -> Option<[f64; 3]> {
            None
        }
    }
    impl mod_api::prelude::LivingHost for BlockUseTestHost {
        fn living(&self, _entity: EntityRef) -> Option<mod_api::LivingRef> {
            None
        }
    }
    impl mod_api::prelude::MenusBackend for BlockUseTestHost {
        fn menu_container_id(&self, _menu: mod_api::MenuRef) -> Option<i32> {
            Some(0)
        }
    }
    impl mod_api::prelude::MenusClientBackend for BlockUseTestHost {}
    mod_api::impl_backend_areas!(
        BlockUseTestHost,
        except(
            player_session,
            entity_identity,
            entity_lifetime,
            entity_position,
            entity_sight,
            entity_stance,
            player_inventory,
            player_modes,
            player_menus,
            menus
        )
    );
    impl Backend for BlockUseTestHost {}
    mod_api::impl_backend_mut_areas!(BlockUseTestHost);
    impl BackendMut for BlockUseTestHost {}

    fn test_block_state() -> BlockState {
        let key: BlockKey = mod_api::__private::parts_installation::block_key(0);
        mod_api::__private::block_state_from_ordinal(key, 0)
    }

    fn test_click<'a>(pos: [i32; 3], player: u128) -> BlockClick<'a> {
        mod_api::__private::built::BlockClick {
            side: LogicalSide::Server,
            dimension: LevelDimension::Overworld,
            pos,
            hit: BlockHit::new(
                [
                    pos[0] as f64 + 0.5,
                    pos[1] as f64 + 1.0,
                    pos[2] as f64 + 0.5,
                ],
                [0, 1, 0],
                false,
            ),
            hand: Hand::Main,
            stack: None,
            player,
            player_entity_id: 7,
        }
        .into()
    }

    #[test]
    fn parses_block_state_strings_and_faces() {
        assert_eq!(
            parse_block_state_str("runes:crafting_altar"),
            ("runes:crafting_altar", BTreeMap::new())
        );
        assert_eq!(
            parse_block_state_str("runes:crafting_altar[facing=north,waterlogged=false]"),
            (
                "runes:crafting_altar",
                BTreeMap::from([
                    ("facing".to_owned(), "north".to_owned()),
                    ("waterlogged".to_owned(), "false".to_owned()),
                ])
            )
        );
        assert_eq!(face_to_direction([0, -1, 0]), "DOWN");
        assert_eq!(face_to_direction([0, 1, 0]), "UP");
        assert_eq!(face_to_direction([0, 0, -1]), "NORTH");
        assert_eq!(face_to_direction([0, 0, 1]), "SOUTH");
        assert_eq!(face_to_direction([-1, 0, 0]), "WEST");
        assert_eq!(face_to_direction([1, 0, 0]), "EAST");
    }

    #[test]
    fn maps_every_mc263_interaction_result_variant_exactly() {
        let default_ctx = InteractionItemContext {
            was_item_interaction: true,
            held_item_transformed_to: None,
        };
        assert_eq!(
            map_interaction_result(&InteractionResultValue::Success {
                swing_source: SwingSource::Predicted,
                item_context: default_ctx.clone(),
            }),
            InteractionResult::Success
        );
        assert_eq!(
            map_interaction_result(&InteractionResultValue::Success {
                swing_source: SwingSource::ServerOnly,
                item_context: default_ctx.clone(),
            }),
            InteractionResult::SuccessServer
        );
        assert_eq!(
            map_interaction_result(&InteractionResultValue::Success {
                swing_source: SwingSource::None,
                item_context: default_ctx,
            }),
            InteractionResult::Consume
        );
        assert_eq!(
            map_interaction_result(&InteractionResultValue::Fail),
            InteractionResult::Fail
        );
        assert_eq!(
            map_interaction_result(&InteractionResultValue::Pass),
            InteractionResult::Pass
        );
        assert_eq!(
            map_interaction_result(&InteractionResultValue::TryEmptyHandInteraction),
            InteractionResult::TryWithEmptyHand
        );
    }

    #[test]
    fn open_mod_menu_outcome_classification_counts_every_refusal_by_name() {
        assert_eq!(
            classify_open_mod_menu_result("demo:bench", Ok(Some(4))),
            SunriseMenuAttempt::Opened(4)
        );
        assert!(matches!(
            classify_open_mod_menu_result("demo:bench", Ok(None)),
            SunriseMenuAttempt::Refused {
                hook_tag: "sunrise#openModMenu:cancelled",
                ..
            }
        ));
        assert!(matches!(
            classify_open_mod_menu_result("runes:crafting", Err(OpenMenuError::NotModMenu)),
            SunriseMenuAttempt::Refused {
                hook_tag: "sunrise#openModMenu:not_mod_menu",
                ..
            }
        ));
        assert!(matches!(
            classify_open_mod_menu_result("demo:bench", Err(OpenMenuError::PlayerUnavailable)),
            SunriseMenuAttempt::Refused {
                hook_tag: "sunrise#openModMenu:player_unavailable",
                ..
            }
        ));
        assert!(matches!(
            classify_open_mod_menu_result("demo:bench", Err(OpenMenuError::TooLarge)),
            SunriseMenuAttempt::Refused {
                hook_tag: "sunrise#openModMenu:extra_too_large",
                ..
            }
        ));
    }

    /// Effects go out in order: a close followed by another close is carried out (nothing of
    /// Sunrise's shows container 1 here, so it is reported unclosed), a discarded menu is
    /// passed on to be counted, and the window shown at the end is read for the jar's check.
    #[test]
    fn menu_effects_go_out_in_bytecode_order() {
        let _ = mod_api::__private::initialize();
        let mut host = BlockUseTestHost::new(None);
        let uuid = host.player_uuid;
        let applied = apply_menu_effects(
            &mut host,
            uuid,
            &[
                MenuEffect::Closed { container_id: 1 },
                MenuEffect::Closed { container_id: 1 },
                MenuEffect::Orphaned { container_id: 2 },
            ],
            &[],
        );
        assert_eq!(
            applied,
            vec![
                MenuApplied::Closed {
                    container_id: 1,
                    closed: false
                },
                MenuApplied::Closed {
                    container_id: 1,
                    closed: false
                },
                MenuApplied::Orphaned { container_id: 2 },
                MenuApplied::Shown(PlayerContainer::Inventory),
            ]
        );
        assert!(apply_menu_effects(&mut host, uuid, &[], &[]).is_empty());
    }

    /// A Sunrise stand-in that records each call and opens or refuses as told.
    struct RecordingSink {
        calls: Vec<String>,
        accept: bool,
        shown: PlayerContainer,
    }

    impl MenuSink for RecordingSink {
        fn open(&mut self, effect: &OpenMenuEffect, data: Vec<i32>) -> SunriseMenuAttempt {
            self.calls
                .push(format!("open {} data {data:?}", effect.container_id));
            if self.accept {
                // `CloseFirst`: the shown window closes, then this one opens.
                self.shown = PlayerContainer::Open(effect.container_id);
                SunriseMenuAttempt::Opened(effect.container_id)
            } else {
                SunriseMenuAttempt::Refused {
                    hook_tag: "sunrise#openModMenu:unregistered_menu_type",
                    reason: "refused".to_owned(),
                }
            }
        }

        fn close(&mut self, container_id: i32) -> bool {
            self.calls.push(format!("close {container_id}"));
            let shown = self.shown == PlayerContainer::Open(container_id);
            if shown {
                self.shown = PlayerContainer::Inventory;
            }
            shown
        }

        fn shown(&mut self) -> PlayerContainer {
            self.shown
        }

        fn advance_counter(&mut self) -> Option<i32> {
            self.calls.push("advance".to_owned());
            None
        }
    }

    fn opened_effect(container_id: i32) -> MenuEffect {
        use crate::runtime::Value;
        MenuEffect::Opened(OpenMenuEffect {
            container_id,
            menu_type: Value::Ref(None),
            menu_type_id: Some("demo:bench".to_owned()),
            title: Value::Ref(None),
            title_json: Some(serde_json::json!({ "text": "Bench" })),
            access_pos: None,
            constructor: None,
            provider: Value::Ref(None),
            menu: Value::Ref(None),
        })
    }

    /// #629 review round 3: `[Closed 1, Opened 2]` where Sunrise refuses the open. Java's
    /// `closeContainer` closed window 1 regardless, so it is closed once the refusal leaves it
    /// standing.
    #[test]
    fn a_refused_replacement_still_closes_the_old_window() {
        let mut sink = RecordingSink {
            calls: Vec::new(),
            accept: false,
            shown: PlayerContainer::Open(1),
        };
        let applied = deliver_menu_effects(
            &mut sink,
            &[MenuEffect::Closed { container_id: 1 }, opened_effect(2)],
            &[checked(2)],
        );
        assert_eq!(sink.calls, ["open 2 data [3]", "close 1"]);
        assert_eq!(
            applied,
            vec![
                MenuApplied::Opened {
                    container_id: 2,
                    attempt: SunriseMenuAttempt::Refused {
                        hook_tag: "sunrise#openModMenu:unregistered_menu_type",
                        reason: "refused".to_owned(),
                    },
                },
                MenuApplied::Closed {
                    container_id: 1,
                    closed: true
                },
                MenuApplied::Shown(PlayerContainer::Inventory),
            ]
        );
    }

    /// The same sequence where Sunrise opens the replacement: its `CloseFirst` closed window 1
    /// before opening 2, so nothing closes it a second time.
    #[test]
    fn a_successful_replacement_closes_the_old_window_once() {
        let mut sink = RecordingSink {
            calls: Vec::new(),
            accept: true,
            shown: PlayerContainer::Open(1),
        };
        let applied = deliver_menu_effects(
            &mut sink,
            &[MenuEffect::Closed { container_id: 1 }, opened_effect(2)],
            &[checked(2)],
        );
        assert_eq!(sink.calls, ["open 2 data [3]"]);
        assert_eq!(
            applied,
            vec![
                MenuApplied::Opened {
                    container_id: 2,
                    attempt: SunriseMenuAttempt::Opened(2),
                },
                MenuApplied::Shown(PlayerContainer::Open(2)),
            ]
        );
        // A close with no open after it closes its window at once.
        let mut sink = RecordingSink {
            calls: Vec::new(),
            accept: true,
            shown: PlayerContainer::Open(2),
        };
        deliver_menu_effects(&mut sink, &[MenuEffect::Closed { container_id: 2 }], &[]);
        assert_eq!(sink.calls, ["close 2"]);
        assert_eq!(sink.shown, PlayerContainer::Inventory);
    }

    /// A checked opening: the window opens with the menu's initial data slot values.
    fn checked(container_id: i32) -> Opening {
        Opening {
            container_id,
            data: Ok(vec![3]),
        }
    }

    /// An open nothing checked (or whose check failed: a layout not its type's, data unknown) is
    /// refused before Sunrise is asked, and the window it would have replaced still closes.
    #[test]
    fn an_unchecked_or_mismatched_open_is_refused_without_a_window() {
        for openings in [
            Vec::new(),
            vec![Opening {
                container_id: 2,
                data: Err("the open menu's layout is not its type's declared one".to_owned()),
            }],
        ] {
            let mut sink = RecordingSink {
                calls: Vec::new(),
                accept: true,
                shown: PlayerContainer::Open(1),
            };
            let applied = deliver_menu_effects(
                &mut sink,
                &[MenuEffect::Closed { container_id: 1 }, opened_effect(2)],
                &openings,
            );
            assert_eq!(sink.calls, ["close 1"]);
            assert!(matches!(
                &applied[0],
                MenuApplied::Opened {
                    container_id: 2,
                    attempt: SunriseMenuAttempt::Refused {
                        hook_tag: "sunrise#openModMenu:layout_unverified",
                        ..
                    },
                }
            ));
        }
    }

    #[test]
    fn runes_crafting_altar_block_part_records_open_menu_and_counts_sunrise_refusal() {
        let Ok(jar_path) = std::env::var("RUNES_JAR") else {
            eprintln!(
                "skipping runes_crafting_altar_block_part_records_open_menu_and_counts_sunrise_refusal: \
                 set RUNES_JAR to run"
            );
            return;
        };
        std::thread::Builder::new()
            .name("mod-runes-block-use".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || run_runes_block_part_test(&jar_path))
            .expect("spawn test thread")
            .join()
            .unwrap();
    }

    fn run_runes_block_part_test(jar_path: &str) {
        let _ = mod_api::__private::initialize();
        let jar = Arc::new(NativeJar::from_path(Path::new(jar_path)).expect("load RUNES_JAR"));
        let mut host = jar
            .try_seed_host(StdHost::new().with_game_version("26.3"))
            .expect("seed Runes host");
        for entry in jar.discover_entrypoints("main") {
            jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
                .expect("run Runes main");
        }

        // The player joined: this mod counted their session, and the jar's host knows their
        // new `ServerPlayer` (`HorizonJava::next_life`).
        let player_uuid = 0x2a_u128;
        host.server_player_created(player_uuid, 1, 0);
        let baseline = host.retained_count();
        let mut state = State::default();
        state.lives.insert(player_uuid, (1, 0));
        state.jars.push(LoadedJar {
            row: crate::JarRow::refused("runes.jar", "runes".to_owned(), String::new()),
            jar,
            host,
            listeners: Vec::new(),
            orderings: Vec::new(),
            commands: Vec::new(),
            native_commands: Vec::new(),
            commands_registered: false,
            stages_ran: 1,
            failures: Vec::new(),
            hooks: crate::runtime::GameHookRegistrations::default(),
            hook_state: crate::runtime::GameHookState::default(),
        });
        let shared = Arc::new(Mutex::new(state));
        let part = JavaBlockUsePart::new(Arc::clone(&shared), "runes:crafting_altar".to_owned());

        let mut server_host = BlockUseTestHost::new(Some("runes:crafting_altar[facing=north]"));
        let click = test_click([8, 65, -3], server_host.player_uuid);
        let block_state = test_block_state();

        // 1. use_item_on -> TryWithEmptyHand, 0 unknowns, 0 menus.
        let item_answer = part.dispatch(&mut server_host, block_state, &click, true);
        assert_eq!(item_answer, Some(InteractionResult::TryWithEmptyHand));

        // 2. use_without_item -> Consume. Java opened `runes:crafting`, and its `initMenu` ran
        //    exactly on the player Sunrise describes (an empty inventory); this host declares no
        //    such content menu, so the named refusal is counted and the jar stops holding the
        //    menu at once.
        let empty_answer = part.dispatch(&mut server_host, block_state, &click, false);
        assert_eq!(empty_answer, Some(InteractionResult::Consume));
        {
            let guard = shared.lock().unwrap();
            let loaded = &guard.jars[0];
            assert_eq!(loaded.row.block_use.uses_ran, 2);
            assert_eq!(loaded.row.block_use.uses_failed, 0);
            assert_eq!(loaded.row.block_use.unknown_host_calls, 0);
            assert_eq!(loaded.row.block_use.menus_opened, 0);
            assert_eq!(loaded.row.block_use.menus_refused, 1);
            let (reason, count) = loaded
                .row
                .block_use
                .refusals
                .iter()
                .next()
                .expect("one refusal");
            assert_eq!(*count, 1);
            assert!(
                reason.contains("runes:crafting") && reason.contains("not registered with Sunrise"),
                "unexpected refusal: {reason}"
            );
            assert!(loaded.host.player_open_menu(player_uuid).is_none());
            assert_eq!(loaded.host.retained_count(), baseline);
        }

        // 3. N more right-clicks: every one refused, and still no menu held and no retain hold
        //    left behind (#629 review P1 1). Java advanced its counter at each refusal while
        //    Sunrise's didn't move, but each use starts from Sunrise's (`container_counter`), so
        //    the counter is never unknown and `nextContainerCounter` never counts (#629's gap).
        for _ in 0..8 {
            let answer = part.dispatch(&mut server_host, block_state, &click, false);
            assert_eq!(answer, Some(InteractionResult::Consume));
        }
        let guard = shared.lock().unwrap();
        let loaded = &guard.jars[0];
        assert_eq!(loaded.row.block_use.menus_refused, 9);
        assert!(loaded.host.player_open_menu(player_uuid).is_none());
        assert_eq!(loaded.host.retained_count(), baseline);
        assert_eq!(loaded.row.block_use.unknown_host_calls, 0);
        assert_eq!(
            loaded
                .host
                .unknown_host_calls()
                .into_iter()
                .find(|(key, _)| key.ends_with("#nextContainerCounter()V")),
            None
        );
    }
}
