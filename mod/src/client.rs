//! Client-side resident jar loading, registration inventory, and creative-tab / tooltip dispatch.
//!
//! This phase owns one interpreter host per jar on the client. It runs client entrypoints and
//! NeoForge's client loading stages, records registrations and listeners, and dispatches
//! Sunrise's build-scoped creative-tab and item-tooltip hooks into each resident jar's runtime.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{JAR_BUDGET, LOG, caught, jar_id};
use runtime::events::{GameListener, PhaseOrdering};
use runtime::{ExecutionBudget, NativeJar, StdHost, Value};

use crate::runtime;

#[path = "menu_screen.rs"]
pub(crate) mod menu_screen;

#[path = "client_item_parts.rs"]
pub(crate) mod item_models;

#[derive(Default)]
pub(crate) struct State {
    loaded: bool,
    world_loaded: bool,
    jars: Vec<LoadedJar>,
    refused: Vec<Row>,
    retired: Vec<Row>,
    /// The Java screen open on a Sunrise mod menu window (`menu_screen.rs`), if one is.
    screen: Option<menu_screen::Opening>,
    /// The last opening's serial: a reused window id is still a new opening.
    screen_serial: u64,
    /// The layout the open screen's Java side had when Sunrise took it (`MenuScreenLayout` is
    /// fixed at open): a later `init` that moves a label is counted.
    screen_layout: Option<runtime::screen_models::ScreenLayout>,
    #[cfg(test)]
    fixture_files: Option<Vec<(String, Vec<u8>)>>,
}

struct LoadedJar {
    row: Row,
    jar: Arc<NativeJar>,
    host: StdHost,
    listeners: Vec<GameListener>,
    orderings: Vec<PhaseOrdering>,
}

impl LoadedJar {
    /// The jar's row with its counted calls as its host has them now: the one counter every
    /// counted call of this jar lands in, whichever bridge counted it.
    fn synced_row(&self) -> Row {
        let mut row = self.row.clone();
        row.unknown_host_calls = self.host.unknown_host_call_count();
        row.counted = self.host.unknown_host_calls().into_iter().collect();
        row
    }
}

#[derive(Clone, Debug, Default)]
struct StageCounts {
    attempted: usize,
    failed: usize,
    disabled: usize,
    calls: usize,
}

#[derive(Clone, Debug, Default)]
struct Row {
    file: String,
    id: String,
    verdict: &'static str,
    host_calls: usize,
    unknown_host_calls: usize,
    reason: Option<String>,
    entrypoints_ran: usize,
    entrypoints_failed: usize,
    stages: BTreeMap<String, StageCounts>,
    keys: usize,
    hud_layers: usize,
    listeners: usize,
    unsupported: BTreeMap<String, usize>,
    refused_registrations: Vec<String>,
    /// Every call the jar's host counted, by name (`unknown_host_calls` is their total): the
    /// client entrypoints' and every bridge's since (the menu screen's
    /// `sunrise#menu_screen:...` tags among them). Read from the host when the report is
    /// written ([`LoadedJar::synced_row`]), so no call site can leave it behind.
    counted: BTreeMap<String, usize>,
    /// The menu screen's Java calls that failed (`menu_screen.rs`): an open's `MenuType.create`,
    /// constructor, `init` or slot read, a frame's data sync or draw, a close's `removed()`.
    menu_screen_failed: usize,
    /// What those failures said, each with the calls the failed call counted (distinct ones, as
    /// the server row's `block_use.failures`).
    menu_screen_failures: Vec<crate::block_use::FailedCall>,
}

impl Row {
    fn refused(file: &str, id: String, reason: impl Into<String>) -> Self {
        Self {
            file: file.to_owned(),
            id,
            verdict: "refused",
            reason: Some(reason.into()),
            ..Self::default()
        }
    }

    /// Count one failed menu-screen call (`failure` names the call and says what it threw or
    /// why the runtime refused it) with the calls it counted. Answers whether the failure is new
    /// to the row, so a draw failing every frame is logged once.
    pub(crate) fn note_menu_screen_failure(
        &mut self,
        failure: String,
        counted: &BTreeMap<String, usize>,
    ) -> bool {
        self.menu_screen_failed += 1;
        crate::block_use::note_failed_call(&mut self.menu_screen_failures, failure, counted)
    }

    /// The verdict, qualified by the failures since (as the server row's summary is): a jar
    /// whose screens failed never reads as an unqualified clean run.
    fn summary(&self) -> String {
        match self.menu_screen_failed {
            0 => self.verdict.to_owned(),
            1 => format!("{}, 1 menu-screen failure", self.verdict),
            failed => format!("{}, {failed} menu-screen failures", self.verdict),
        }
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "file": self.file,
            "id": self.id,
            "verdict": self.verdict,
            "summary": self.summary(),
            "host_calls": self.host_calls,
            "unknown_host_call_count": self.unknown_host_calls,
            "counted": self.counted,
            "reason": self.reason,
            "client": {
                "entrypoints_ran": self.entrypoints_ran,
                "entrypoints_failed": self.entrypoints_failed,
                "stages": self.stages.iter().map(|(stage, counts)| (stage.clone(), serde_json::json!({
                    "attempted": counts.attempted,
                    "failed": counts.failed,
                    "disabled": counts.disabled,
                    "calls": counts.calls,
                }))).collect::<serde_json::Map<_, _>>(),
                "client_keys": self.keys,
                "client_hud_layers": self.hud_layers,
                "client_listeners": self.listeners,
                "unfired_registrations": self.unsupported.iter().map(|(source, count)| serde_json::json!({"source": source, "count": count})).collect::<Vec<_>>(),
                "refused": self.refused_registrations,
                "menu_screen": {
                    "failed": self.menu_screen_failed,
                    "failures": crate::block_use::failed_calls_json(&self.menu_screen_failures),
                },
            }
        })
    }
}

/// The row's log line at client stop: the summary, then each counted call by name and each
/// failure's text, so the log never shows a jar whose screens failed as a clean run.
impl std::fmt::Display for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "client {} ({}) {} ({} host calls, {} unknown host calls",
            self.file,
            self.id,
            self.summary(),
            self.host_calls,
            self.unknown_host_calls
        )?;
        if let Some(reason) = &self.reason {
            write!(f, ", reason: {reason}")?;
        }
        if !self.counted.is_empty() {
            f.write_str("; counted:")?;
            for (index, (name, count)) in self.counted.iter().enumerate() {
                let separator = if index == 0 { " " } else { ", " };
                write!(f, "{separator}{name} x{count}")?;
            }
        }
        for failure in &self.menu_screen_failures {
            write!(f, "; failed x{}: {}", failure.times, failure.failure)?;
        }
        f.write_str(")")
    }
}

impl State {
    #[cfg(test)]
    pub(crate) fn set_fixture_files(&mut self, files: Vec<(String, Vec<u8>)>) {
        self.fixture_files = Some(files);
    }

    /// Load the client role once, on the client setup thread, and retain every host for later G1 dispatch.
    pub(crate) fn ensure_loaded_with_version(&mut self, game_version: Option<&str>) {
        if self.loaded {
            return;
        }
        self.retired.clear();
        self.world_loaded = false;
        self.loaded = true;
        let mut parsed = Vec::new();
        let files = {
            #[cfg(test)]
            if let Some(files) = &self.fixture_files {
                files.clone()
            } else {
                crate::content::read_jar_files()
            }
            #[cfg(not(test))]
            {
                crate::content::read_jar_files()
            }
        };
        for (file, bytes) in files {
            match NativeJar::from_bytes(&bytes) {
                Ok(jar) => parsed.push((file, Arc::new(jar))),
                Err(error) => self.refused.push(Row::refused(
                    &file,
                    file.strip_suffix(".jar").unwrap_or(&file).to_owned(),
                    format!("not a readable jar: {error}"),
                )),
            }
        }
        let refs: Vec<_> = parsed
            .iter()
            .map(|(file, jar)| (file.as_str(), jar.as_ref()))
            .collect();
        let plan = runtime::dependencies::plan(&refs);
        let imc = runtime::neoforge_imc::ImcBus::new();
        let mut prepared = Vec::new();
        for &index in &plan.order {
            let (file, jar) = parsed[index].clone();
            if let Some(missing) = plan.missing.get(&index) {
                self.refused.push(Row::refused(
                    &file,
                    jar_id(&jar, &file),
                    format!(
                        "requires {} (not in config/horizon_java/java-mods)",
                        missing.join(", ")
                    ),
                ));
                continue;
            }
            let links: Vec<_> = plan
                .links_for(index)
                .iter()
                .map(|&dependency| {
                    let (name, jar) = &parsed[dependency];
                    (name.clone(), Arc::clone(jar))
                })
                .collect();
            match prepare_jar(file, jar, &links, &imc, game_version) {
                Ok(jar) => prepared.push(jar),
                Err(row) => self.refused.push(row),
            }
        }
        // The client bootstraps once before any mod's entrypoint: every jar's Mixin callbacks
        // into game classes' static initializers run in one pass (`runtime::bootstrap_mixins`),
        // each on its jar's client host. One that throws means the game does not start: every
        // jar is refused with it.
        let boot = {
            let mut units: Vec<runtime::BootstrapUnit<'_>> = prepared
                .iter_mut()
                .map(|jar| runtime::BootstrapUnit {
                    runner: jar.jar.as_ref(),
                    host: &mut jar.host,
                    sources: vec![jar.jar.as_ref()],
                })
                .collect();
            match caught("client bootstrap Mixin callbacks", || {
                runtime::run_bootstrap_injectors(&mut units, &ExecutionBudget::within(JAR_BUDGET))
            }) {
                Ok(failures) => failures.into_iter().next(),
                Err(panic) => Some(format!("bootstrap Mixin callbacks panicked: {panic}")),
            }
        };
        for jar in prepared {
            match run_prepared(jar, &imc, boot.as_deref()) {
                Ok(jar) => self.jars.push(jar),
                Err(row) => self.refused.push(row),
            }
        }
        for jar in &mut self.jars {
            if let Some(host) = imc.reclaim(&jar.row.file) {
                jar.host = host;
            }
        }
        for jar in &self.jars {
            log_registration(&jar.row);
        }
        for row in &self.refused {
            log::info!(
                "{LOG}: client {} ({}) refused: {}",
                row.file,
                row.id,
                row.reason.as_deref().unwrap_or("unknown reason")
            );
        }
        log::info!(
            "{LOG}: client registration inventory: {} jar(s), {} loaded, {} refused",
            self.jars.len() + self.refused.len(),
            self.jars.len(),
            self.refused.len()
        );
    }

    #[cfg(test)]
    pub(crate) fn resident_count(&self) -> usize {
        self.jars.len()
    }

    #[cfg(test)]
    pub(crate) fn world_loaded(&self) -> bool {
        self.world_loaded
    }

    #[cfg(test)]
    pub(crate) fn with_first_jar_mut<T>(
        &mut self,
        f: impl FnOnce(&NativeJar, &mut StdHost) -> T,
    ) -> T {
        let first = self
            .jars
            .first_mut()
            .expect("at least one client jar loaded");
        f(&first.jar, &mut first.host)
    }

    /// A resident jar and its host, for tests that set up state a client callback then reads.
    #[cfg(test)]
    pub(crate) fn resident(&mut self, index: usize) -> (&NativeJar, &mut StdHost) {
        let loaded = &mut self.jars[index];
        (&*loaded.jar, &mut loaded.host)
    }

    /// Each row's log line ([`Row`]'s `Display`), as the report has it now.
    pub(crate) fn row_lines(&self) -> Vec<String> {
        self.retired
            .iter()
            .cloned()
            .chain(self.jars.iter().map(LoadedJar::synced_row))
            .chain(self.refused.iter().cloned())
            .map(|row| row.to_string())
            .collect()
    }

    pub(crate) fn report_rows(&self) -> impl Iterator<Item = serde_json::Value> + '_ {
        self.retired
            .iter()
            .map(Row::json)
            .chain(self.jars.iter().map(|jar| jar.synced_row().json()))
            .chain(self.refused.iter().map(Row::json))
    }

    pub(crate) fn level_loaded(&mut self) {
        self.world_loaded = true;
        self.forget_client_sound_streams();
    }

    pub(crate) fn level_unloaded(&mut self) {
        self.world_loaded = false;
        self.forget_menu_screen();
        for jar in &mut self.jars {
            jar.host.forget_all_game_objects();
        }
        self.forget_client_sound_streams();
    }

    pub(crate) fn player_logged_out(&mut self) {
        self.forget_menu_screen();
        for jar in &mut self.jars {
            jar.host.forget_all_game_objects();
        }
    }

    pub(crate) fn player_cloned(&mut self) {
        for jar in &mut self.jars {
            jar.host
                .forget_game_object_prefix(runtime::client_surface::PLAYER);
        }
    }

    /// A client level comes with a fresh `random` and `soundSeedGenerator`: no stream position
    /// the last level left unknown carries over.
    fn forget_client_sound_streams(&mut self) {
        for loaded in &mut self.jars {
            loaded.host.forget_unknown_client_sound_streams();
        }
    }

    /// Deliver the client sounds the resident jars' Java code recorded to Sunrise
    /// (`sound::apply_client_sounds`), so each is played or counted on its jar, never dropped.
    ///
    /// Runs after every client path that executes Java code: client setup (entrypoints and
    /// loading stages), creative-tab builds and item tooltips. The client owner opens no
    /// `ClientSnapshot` frame, so a jar's client sounds land in its host's standalone record.
    /// The level sound streams any jar left unknown are shared with every jar afterwards. They
    /// are gathered after the Java code ran, so a stream one call left unknown also counts that
    /// callback's earlier deferred sounds: conservative, and moot while the client defers none.
    pub(crate) fn drain_client_sounds<H: crate::sound::LocalSounds + ?Sized>(&mut self, host: &H) {
        let mut streams =
            crate::sound::sync_unknown_sound_streams(&mut self.jars, |jar| &mut jar.host);
        for loaded in &mut self.jars {
            let sounds = loaded.host.take_sound_records();
            if sounds.is_empty() {
                continue;
            }
            crate::sound::apply_client_sounds(host, &sounds, &mut streams, |reason| {
                loaded.host.note_unknown_game_hook(reason);
            });
            loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
        }
        crate::sound::share_unknown_sound_streams(
            self.jars.iter_mut().map(|jar| &mut jar.host),
            &streams,
        );
    }

    /// Dispatch Java Fabric creative-tab output into the build-scoped Sunrise edit.  Each jar's
    /// collector is seeded with the tab's current Sunrise parent/search stacks so `insertBefore`,
    /// `insertAfter`, and `prepend` see real vanilla and earlier-mod anchors; the interpreter owns
    /// Fabric's phase order, visibility and feature checks, and this adapter reconciles the
    /// resulting lists back onto `event.edit`.
    pub(crate) fn build_creative_tab(
        &mut self,
        host: &mut dyn mod_api::ClientHost,
        event: &mut mod_api::BuildCreativeModeTabContentsEvent<'_>,
    ) {
        if self.jars.is_empty() {
            return;
        }
        let tab = event.tab.to_owned();
        let permissions = event.permissions == mod_api::CreativeTabPermissions::Operator;
        let Some(enabled_features) = runtime::creative_feature_mask(&event.features) else {
            for loaded in &mut self.jars {
                loaded
                    .host
                    .note_unknown_game_hook("creative tab feature flag");
                loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
            }
            return;
        };
        for loaded in &mut self.jars {
            let (Ok(parent_entries), Ok(search_entries)) = (
                event.edit.entries(host, mod_api::CreativeTabList::Parent),
                event.edit.entries(host, mod_api::CreativeTabList::Search),
            ) else {
                loaded.host.note_unknown_game_hook("creative tab entries");
                loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                continue;
            };
            // A stack whose item the host cannot name has no Java form: the tab's contents
            // would be incomplete, so the jar's build is counted like unreadable entries.
            let (Some(seeded_parent), Some(seeded_search)) = (
                seed_creative_stacks(host, &mut loaded.host, &parent_entries),
                seed_creative_stacks(host, &mut loaded.host, &search_entries),
            ) else {
                loaded.host.note_unknown_game_hook("creative tab entries");
                loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                continue;
            };
            let context = runtime::CreativeTabBuildContext {
                enabled_features,
                has_permissions: permissions,
                initial_display_stacks: seeded_parent.iter().map(|(v, _)| v.clone()).collect(),
                initial_search_tab_stacks: seeded_search.iter().map(|(v, _)| v.clone()).collect(),
                ..Default::default()
            };
            let contents =
                match loaded
                    .jar
                    .creative_tab_contents_with(&mut loaded.host, &tab, context)
                {
                    Ok(contents) => contents,
                    Err(error) => {
                        loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                        log::warn!(
                            "{LOG}: client {} creative tab {} failed: {error}",
                            loaded.row.id,
                            tab
                        );
                        continue;
                    }
                };
            apply_creative_contents(
                host,
                event,
                &mut loaded.host,
                &contents,
                &seeded_parent,
                &seeded_search,
            );
            // Creative registrations are initially reported as unfired. Re-read the inventory
            // after this tab's hook so the JSON row reflects the actual registration state.
            let registered = loaded.host.game_listeners(&loaded.jar, false);
            loaded.row.unsupported = registered.unsupported;
            loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
        }
        self.drain_client_sounds(host);
    }

    /// Run each resident jar's Item appendHoverText implementation for the item in a Sunrise
    /// tooltip.  Components are converted through Sunrise's text boundary, retaining translation
    /// keys and style; a failed conversion is counted on the jar and does not silently vanish.
    pub(crate) fn append_java_tooltip(
        &mut self,
        host: &dyn mod_api::ClientHost,
        event: &mut mod_api::ItemTooltipEvent<'_>,
    ) {
        if self.jars.is_empty() || !event.display.name_visible(event.creative) {
            return;
        }
        // `None` when the host has no name for the item's slot. Then this bridge cannot tell which
        // jar's item it is, yet any jar that registered an item may own it and would run its
        // `appendHoverText` in Java: each such jar counts the tooltip it could not run, as a
        // creative tab counts a stack it cannot name, instead of the tooltip skipping it silently.
        let Some(id) = event.stack.item().id(host).map(str::to_owned) else {
            for loaded in &mut self.jars {
                let registers_items = loaded
                    .host
                    .registrations()
                    .iter()
                    .any(|entry| entry.registry == "minecraft:item");
                if registers_items {
                    loaded
                        .host
                        .note_unknown_game_hook("tooltip item has no registry id");
                    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                }
            }
            return;
        };
        // The hovered stack is lent as a view and copied out once, for the first jar that owns
        // its item (`ItemStackView::project`); `None` inside means it has no owned form.
        let mut projected: Option<Option<mod_api::ItemStack>> = None;
        // Each line with the jar that produced it, so a line Sunrise refuses is counted there.
        let mut additions: Vec<(usize, mod_api::Text)> = Vec::new();
        for (index, loaded) in self.jars.iter_mut().enumerate() {
            let Some(entry) = loaded.host.observed_registry_entry("minecraft:item", &id) else {
                continue;
            };
            let item = entry.value.clone();
            let Some(api_stack) = projected
                .get_or_insert_with(|| event.stack.project())
                .as_ref()
            else {
                // Java's `appendHoverText` would see this stack; without it the call is counted.
                loaded
                    .host
                    .note_unknown_game_hook("tooltip stack has no owned form");
                loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                continue;
            };
            let Some(stack) =
                runtime_stack_from_api(host, &mut loaded.host, Some(item.clone()), api_stack)
            else {
                loaded
                    .host
                    .note_unknown_game_hook("tooltip stack item has no registry id");
                loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                continue;
            };
            let (display, malformed_display) =
                build_tooltip_display(&mut loaded.host, event, api_stack);
            if malformed_display {
                loaded
                    .host
                    .note_unknown_game_hook("tooltip display component");
            }
            let lines = match loaded.jar.run_item_tooltip(
                &item,
                stack,
                runtime::client_surface::tooltip_context(),
                display,
                runtime::client_surface::tooltip_flag(event.advanced, event.creative),
                &mut loaded.host,
                &ExecutionBudget::within(JAR_BUDGET),
            ) {
                Ok(lines) => lines,
                Err(error) => {
                    log::warn!(
                        "{LOG}: client {} item tooltip {} failed: {error}",
                        loaded.row.id,
                        id
                    );
                    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                    continue;
                }
            };
            for line in lines {
                let (converted, refused) = convert_tooltip_component(host, &loaded.host, &line);
                if refused {
                    loaded
                        .host
                        .note_unknown_game_hook("tooltip component conversion");
                } else if let Some(text) = converted {
                    additions.push((index, text));
                }
            }
            loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
        }
        // The first line is the item's name. Java appendHoverText runs immediately after it.
        // `TooltipLines` validates each line on its own: one Sunrise refuses is left out and
        // counted on its jar, and the rest still go in, in order.
        let mut at = event.lines.len().min(1);
        for (index, text) in additions {
            match event.lines.insert(at, text, u32::MAX) {
                Ok(_) => at += 1,
                Err(_) => {
                    let loaded = &mut self.jars[index];
                    loaded
                        .host
                        .note_unknown_game_hook("tooltip line refused by Sunrise");
                    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
                }
            }
        }
        self.drain_client_sounds(host);
    }

    /// `isBarVisible` / `getBarWidth` / `getBarColor` of the jar item `item_id` on `stack`, run
    /// on the client's resident jar (`item_parts.rs`): `bar` arrives holding vanilla's bar and
    /// takes the item's own answers for the methods it overrides. A call that counted leaves
    /// vanilla's bar.
    pub(crate) fn java_item_bar(
        &mut self,
        host: &dyn mod_api::ClientHost,
        item_id: &str,
        stack: &mod_api::ItemStack,
        bar: &mut Option<(i32, u32)>,
    ) {
        for loaded in &mut self.jars {
            let Some(entry) = loaded
                .host
                .observed_registry_entry("minecraft:item", item_id)
            else {
                continue;
            };
            let item = entry.value.clone();
            let overrides = loaded.jar.item_overrides(&item, &loaded.host);
            let jar_types = crate::item_parts::persistent_jar_types(&loaded.host);
            let jar = &loaded.jar;
            // Drawn every frame the stack shows: what the calls make is garbage once the bar is
            // known, and the stack is the one the last draw of this game stack used while
            // nothing changed it.
            let outcome = loaded.host.with_transient_rows(|jar_host| {
                let value = crate::item_parts::java_draw_stack(
                    host,
                    jar,
                    jar_host,
                    Some(item.clone()),
                    stack,
                    &jar_types,
                )?;
                Some(jar.item_bar(
                    jar_host,
                    &item,
                    &overrides,
                    &value,
                    bar.is_some(),
                    &ExecutionBudget::within(JAR_BUDGET),
                ))
            });
            let Some(outcome) = outcome else {
                loaded
                    .host
                    .note_unknown_game_hook("item_bar:stack unreadable");
                return;
            };
            let Ok(outcome) = outcome else {
                loaded.host.note_unknown_game_hook("item_bar:failed");
                return;
            };
            if !outcome.report.exact() {
                return;
            }
            // `GuiGraphicsExtractor.itemDecorations` draws `ARGB.opaque(getBarColor())`: the rgb.
            let visible = outcome.visible.unwrap_or(bar.is_some());
            if !visible {
                *bar = None;
                return;
            }
            let (width, color) = bar.unwrap_or((0, 0));
            let width = outcome.width.unwrap_or(width);
            let color = outcome
                .color
                .map_or(color, |argb| (argb as u32) & 0x00FF_FFFF);
            *bar = Some((width, color));
            return;
        }
    }

    /// Count a client-side item-part ask the bridge leaves to vanilla (a click prediction), on
    /// the resident jar that registered `item_id`.
    pub(crate) fn note_item_part_unanswered(&mut self, item_id: &str, what: &str) {
        if let Some(loaded) = self.jars.iter_mut().find(|loaded| {
            loaded
                .host
                .observed_registry_entry("minecraft:item", item_id)
                .is_some()
        }) {
            loaded.host.note_unknown_game_hook(what);
        }
    }

    /// Drop resident hosts, listeners, and jar heaps at client shutdown. Rows stay available
    /// until the next client setup so the final report can be rewritten after teardown.
    pub(crate) fn teardown(&mut self) -> usize {
        let mut rows = std::mem::take(&mut self.refused);
        rows.extend(self.jars.drain(..).map(|jar| jar.synced_row()));
        let count = rows.len();
        self.retired.extend(rows);
        self.forget_menu_screen();
        self.loaded = false;
        self.world_loaded = false;
        count
    }
}

/// Every entry as its runtime stack, or `None` if any entry's item has no name on the host.
pub(crate) fn seed_creative_stacks(
    host: &dyn mod_api::Host,
    runtime_host: &mut StdHost,
    entries: &[mod_api::ItemStack],
) -> Option<Vec<(Value, mod_api::ItemStack)>> {
    entries
        .iter()
        .map(|stack| {
            let value = runtime_stack_from_api(host, runtime_host, None, stack)?;
            Some((value, stack.clone()))
        })
        .collect()
}

fn java_string_utf8(s: &mod_api::JavaString) -> Option<String> {
    String::from_utf16(&s.0).ok()
}

fn parse_tooltip_display_tag(tag: &mod_api::Tag) -> Option<(bool, Vec<String>)> {
    let mod_api::Tag::Compound(fields) = tag else {
        return None;
    };
    let mut hide_tooltip = false;
    let mut hidden_components = Vec::new();
    for (raw_key, val) in fields {
        let key = java_string_utf8(raw_key)?;
        match key.as_str() {
            "hide_tooltip" => match val {
                mod_api::Tag::Byte(b) => hide_tooltip = *b != 0,
                _ => return None,
            },
            "hidden_components" => {
                let mod_api::Tag::List(items) = val else {
                    return None;
                };
                for item in items {
                    let mod_api::Tag::String(raw_id) = item else {
                        return None;
                    };
                    let raw = java_string_utf8(raw_id)?;
                    // Match MC 26.3 `DataComponentType.CODEC` (`Identifier.CODEC` +
                    // `ReferenceLinkedOpenHashSet`): normalize unqualified ids (`"damage"` ->
                    // `"minecraft:damage"`) and deduplicate preserving first-occurrence order.
                    let id = mod_api::Identifier::parse(&raw).ok()?.to_string();
                    if !hidden_components.iter().any(|existing| existing == &id) {
                        hidden_components.push(id);
                    }
                }
            }
            _ => return None,
        }
    }
    Some((hide_tooltip, hidden_components))
}

fn build_tooltip_display(
    runtime_host: &mut StdHost,
    event: &mod_api::ItemTooltipEvent<'_>,
    stack: &mod_api::ItemStack,
) -> (Value, bool) {
    let hide_tooltip = !event.display.name_visible(false);
    let mut hidden = Vec::new();
    let mut hidden_known = true;
    let mut malformed = false;

    let stack_td = stack.components().and_then(|components| {
        components
            .as_nbt()
            .iter()
            .find(|(id, _)| {
                id.0.iter()
                    .copied()
                    .eq("minecraft:tooltip_display".encode_utf16())
            })
            .map(|(_, tag)| tag)
    });
    if let Some(tag) = stack_td {
        if let Some((tag_hide, ids)) = parse_tooltip_display_tag(tag) {
            if hide_tooltip {
                if tag_hide {
                    hidden.extend(ids);
                } else {
                    hidden_known = false;
                }
            } else {
                for id in ids {
                    if !event.display.shows(&id) && !hidden.iter().any(|existing| existing == &id) {
                        hidden.push(id);
                    }
                }
            }
        } else {
            hidden_known = false;
            malformed = true;
        }
    } else if hide_tooltip {
        // `event.display` hid everything without an explicit stack tag, so `shows(...)` returns
        // false for every id and cannot reveal the underlying `hidden_components` set.
        hidden_known = false;
    }

    if !hide_tooltip && !malformed {
        for id in runtime::all_vanilla_data_component_ids() {
            if !event.display.shows(id) && !hidden.iter().any(|existing| existing == id) {
                hidden.push(id.to_owned());
            }
        }
        for id in runtime_host.registered_data_component_ids() {
            if !event.display.shows(&id) && !hidden.iter().any(|existing| existing == &id) {
                hidden.push(id);
            }
        }
    }

    let handle = runtime_host.make_tooltip_display(hide_tooltip, hidden, hidden_known);
    (handle, malformed)
}

fn json_to_nbt(value: &serde_json::Value) -> Option<mod_api::Tag> {
    match value {
        serde_json::Value::String(s) => Some(mod_api::Tag::String(s.as_str().into())),
        serde_json::Value::Bool(b) => Some(mod_api::Tag::Byte(i8::from(*b))),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if let Ok(i32_val) = i32::try_from(i) {
                    Some(mod_api::Tag::Int(i32_val))
                } else {
                    Some(mod_api::Tag::Long(i))
                }
            } else {
                n.as_f64()
                    .filter(|f| f.is_finite())
                    .map(|f| mod_api::Tag::Double(f.to_bits()))
            }
        }
        serde_json::Value::Array(items) => Some(mod_api::Tag::List(
            items.iter().map(json_to_nbt).collect::<Option<Vec<_>>>()?,
        )),
        serde_json::Value::Object(map) => {
            let mut compound = mod_api::Compound::new();
            for (k, v) in map {
                compound.insert(k.as_str().into(), json_to_nbt(v)?);
            }
            Some(mod_api::Tag::Compound(compound))
        }
        serde_json::Value::Null => None,
    }
}

fn nbt_to_json_value(tag: &mod_api::Tag, bool_hint: bool) -> Option<serde_json::Value> {
    match tag {
        mod_api::Tag::String(s) => Some(serde_json::Value::String(java_string_utf8(s)?)),
        mod_api::Tag::Byte(b) => {
            if bool_hint {
                Some(serde_json::Value::Bool(*b != 0))
            } else {
                Some(serde_json::Value::from(*b))
            }
        }
        mod_api::Tag::Short(s) => Some(serde_json::Value::from(*s)),
        mod_api::Tag::Int(i) => Some(serde_json::Value::from(*i)),
        mod_api::Tag::Long(l) => Some(serde_json::Value::from(*l)),
        mod_api::Tag::Float(bits) => {
            let f = f32::from_bits(*bits);
            serde_json::Number::from_f64(f64::from(f)).map(serde_json::Value::Number)
        }
        mod_api::Tag::Double(bits) => {
            let d = f64::from_bits(*bits);
            serde_json::Number::from_f64(d).map(serde_json::Value::Number)
        }
        mod_api::Tag::List(items) => Some(serde_json::Value::Array(
            items
                .iter()
                .map(|item| nbt_to_json_value(item, false))
                .collect::<Option<Vec<_>>>()?,
        )),
        mod_api::Tag::Compound(fields) => {
            let mut obj = serde_json::Map::new();
            for (raw_key, val) in fields {
                let key = java_string_utf8(raw_key)?;
                let is_bool_field = matches!(
                    key.as_str(),
                    "bold" | "italic" | "underlined" | "strikethrough" | "obfuscated"
                );
                obj.insert(key, nbt_to_json_value(val, is_bool_field)?);
            }
            Some(serde_json::Value::Object(obj))
        }
        _ => None,
    }
}

fn nbt_component_to_json(tag: &mod_api::Tag) -> Option<String> {
    let val = nbt_to_json_value(tag, false)?;
    match &val {
        serde_json::Value::String(_) => {}
        serde_json::Value::Object(map)
            if map
                .get("text")
                .and_then(serde_json::Value::as_str)
                .is_some()
                || map
                    .get("translate")
                    .and_then(serde_json::Value::as_str)
                    .is_some() => {}
        _ => return None,
    }
    serde_json::to_string(&val).ok()
}

pub(crate) fn runtime_stack_from_api(
    host: &dyn mod_api::Host,
    runtime_host: &mut StdHost,
    item_override: Option<Value>,
    stack: &mod_api::ItemStack,
) -> Option<Value> {
    let parts = stack_parts(host, stack, &[])?;
    Some(runtime_host.make_host_item_stack_with_components(
        item_override,
        &parts.id,
        parts.count,
        parts.patch,
        parts.opaque_components,
        parts.opaque_keys,
    ))
}

/// A mod API stack as the runtime's stack model takes it.
pub(crate) struct StackParts {
    pub(crate) id: String,
    pub(crate) count: i32,
    pub(crate) patch: Vec<(String, Option<runtime::RuntimeComponentValue>)>,
    pub(crate) opaque_components: bool,
    pub(crate) opaque_keys: Vec<String>,
}

/// `stack`'s item id, count and component patch, each patch entry as the runtime models it;
/// a jar data component type in `jar_types` crosses as its NBT
/// ([`runtime::RuntimeComponentValue::Nbt`], decoded by `NativeJar::make_item_stack`), and any
/// other value the boundary can't carry is opaque (a Java read of it counts).
pub(crate) fn stack_parts(
    host: &dyn mod_api::Host,
    stack: &mod_api::ItemStack,
    jar_types: &[String],
) -> Option<StackParts> {
    // `None` when the host has no name for the item's slot: no Java stack can stand for it, so
    // the caller counts the call instead of handing Java a made-up item.
    let id = stack.item().id(host)?.to_owned();
    let count = stack.count().min(i32::MAX as u32) as i32;
    let mut patch: Vec<(String, Option<runtime::RuntimeComponentValue>)> = Vec::new();
    let mut opaque_keys: Vec<String> = Vec::new();
    let mut opaque_components = false;

    if let Some(damage) = stack.damage_component() {
        if let Ok(dmg) = i32::try_from(damage) {
            patch.push((
                "minecraft:damage".to_owned(),
                Some(runtime::RuntimeComponentValue::Int(dmg)),
            ));
        } else {
            opaque_components = true;
            opaque_keys.push("minecraft:damage".to_owned());
        }
    }

    if let Some(components) = stack.components() {
        for (raw_key, tag) in components.as_nbt() {
            let Some(key) = java_string_utf8(raw_key) else {
                opaque_components = true;
                continue;
            };
            if let Some(removed) = key.strip_prefix('!') {
                if matches!(tag, mod_api::Tag::Compound(fields) if fields.is_empty()) {
                    patch.push((removed.to_owned(), None));
                } else {
                    opaque_components = true;
                    opaque_keys.push(removed.to_owned());
                }
                continue;
            }
            if jar_types.contains(&key) {
                match crate::attachments::tag_to_runtime(tag, 0) {
                    Ok(nbt) => patch.push((key, Some(runtime::RuntimeComponentValue::Nbt(nbt)))),
                    Err(_) => {
                        opaque_components = true;
                        opaque_keys.push(key);
                    }
                }
                continue;
            }
            let converted = match key.as_str() {
                "minecraft:damage" => match tag {
                    mod_api::Tag::Int(n) if *n >= 0 => {
                        Some(runtime::RuntimeComponentValue::Int(*n))
                    }
                    _ => None,
                },
                "minecraft:max_stack_size" => match tag {
                    mod_api::Tag::Int(n) if (1..=99).contains(n) => {
                        Some(runtime::RuntimeComponentValue::Int(*n))
                    }
                    _ => None,
                },
                "minecraft:max_damage" => match tag {
                    mod_api::Tag::Int(n) if *n >= 1 => {
                        Some(runtime::RuntimeComponentValue::Int(*n))
                    }
                    _ => None,
                },
                "minecraft:repair_cost" => match tag {
                    mod_api::Tag::Int(n) if *n >= 0 => {
                        Some(runtime::RuntimeComponentValue::Int(*n))
                    }
                    _ => None,
                },
                "minecraft:unbreakable" => match tag {
                    mod_api::Tag::Compound(fields) if fields.is_empty() => {
                        Some(runtime::RuntimeComponentValue::Unit)
                    }
                    _ => None,
                },
                "minecraft:custom_data" => match tag {
                    mod_api::Tag::Compound(fields) => crate::attachments::to_runtime(fields)
                        .ok()
                        .map(runtime::RuntimeComponentValue::CustomData),
                    _ => None,
                },
                "minecraft:custom_name" | "minecraft:item_name" => {
                    nbt_component_to_json(tag).map(runtime::RuntimeComponentValue::ComponentJson)
                }
                "minecraft:tooltip_display" => {
                    parse_tooltip_display_tag(tag).map(|(hide_tooltip, hidden_components)| {
                        runtime::RuntimeComponentValue::TooltipDisplay {
                            hide_tooltip,
                            hidden_components,
                            hidden_known: true,
                        }
                    })
                }
                _ => None,
            };
            if let Some(val) = converted {
                patch.push((key, Some(val)));
            } else {
                opaque_components = true;
                opaque_keys.push(key);
            }
        }
    }

    Some(StackParts {
        id,
        count,
        patch,
        opaque_components,
        opaque_keys,
    })
}

pub(crate) fn api_stack(
    host: &dyn mod_api::Host,
    runtime_host: &StdHost,
    value: &Value,
) -> Option<mod_api::ItemStack> {
    api_stack_from_components(host, runtime_host.item_stack_components(value)?)
}

/// [`api_stack`] of a runtime stack's extracted parts: `None` for one the mod API can't carry.
pub(crate) fn api_stack_from_components(
    host: &dyn mod_api::Host,
    extracted: runtime::StackComponents,
) -> Option<mod_api::ItemStack> {
    if extracted.opaque_components {
        return None;
    }
    let key = host.item_key(&mod_api::Identifier::parse(&extracted.item_id).ok()?)?;
    let count: u32 = extracted.count.try_into().ok()?;
    let mut stack = mod_api::ItemStack::new(key, count)?;
    if extracted.patch.is_empty() {
        return Some(stack);
    }
    let mut nbt = mod_api::Compound::new();
    for (comp_id, entry) in extracted.patch {
        mod_api::ComponentId::new(&comp_id)?;
        match entry {
            None => {
                nbt.insert(
                    format!("!{comp_id}").as_str().into(),
                    mod_api::Tag::Compound(mod_api::Compound::new()),
                );
            }
            Some(runtime::RuntimeComponentValue::Int(n)) => {
                let valid = match comp_id.as_str() {
                    "minecraft:damage" | "minecraft:repair_cost" => n >= 0,
                    "minecraft:max_damage" => n >= 1,
                    "minecraft:max_stack_size" => (1..=99).contains(&n),
                    _ => false,
                };
                if !valid {
                    return None;
                }
                nbt.insert(comp_id.as_str().into(), mod_api::Tag::Int(n));
            }
            Some(runtime::RuntimeComponentValue::Unit) if comp_id == "minecraft:unbreakable" => {
                nbt.insert(
                    comp_id.as_str().into(),
                    mod_api::Tag::Compound(mod_api::Compound::new()),
                );
            }
            Some(runtime::RuntimeComponentValue::CustomData(data))
                if comp_id == "minecraft:custom_data" =>
            {
                nbt.insert(
                    comp_id.as_str().into(),
                    mod_api::Tag::Compound(crate::attachments::to_game(&data)),
                );
            }
            Some(runtime::RuntimeComponentValue::ComponentJson(json))
                if matches!(
                    comp_id.as_str(),
                    "minecraft:custom_name" | "minecraft:item_name"
                ) =>
            {
                let parsed: serde_json::Value = serde_json::from_str(&json).ok()?;
                let tag = json_to_nbt(&parsed)?;
                nbt.insert(comp_id.as_str().into(), tag);
            }
            Some(runtime::RuntimeComponentValue::TooltipDisplay {
                hide_tooltip,
                hidden_components,
                hidden_known: true,
            }) if comp_id == "minecraft:tooltip_display" => {
                let mut fields = mod_api::Compound::new();
                if hide_tooltip {
                    fields.insert("hide_tooltip".into(), mod_api::Tag::Byte(1));
                }
                if !hidden_components.is_empty() {
                    fields.insert(
                        "hidden_components".into(),
                        mod_api::Tag::List(
                            hidden_components
                                .into_iter()
                                .map(|id| mod_api::Tag::String(id.as_str().into()))
                                .collect(),
                        ),
                    );
                }
                nbt.insert(comp_id.as_str().into(), mod_api::Tag::Compound(fields));
            }
            // A jar component type's value, as its own codec encodes it (`item_parts.rs`).
            Some(runtime::RuntimeComponentValue::Nbt(tag)) => {
                nbt.insert(
                    comp_id.as_str().into(),
                    crate::attachments::tag_to_game(&tag),
                );
            }
            _ => return None,
        }
    }
    stack.apply_components(&nbt).ok()?;
    Some(stack)
}

/// Indices of a longest strictly-increasing subsequence of `seq` (returns a boolean mask over
/// `0..seq.len()`). Used to keep the maximal subset of seeded entries already in `desired` order
/// untouched in `CreativeTabEdit`.
fn longest_increasing_mask(seq: &[usize]) -> Vec<bool> {
    let n = seq.len();
    if n == 0 {
        return Vec::new();
    }
    let mut tails: Vec<usize> = Vec::with_capacity(n);
    let mut prev: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        let pos = tails.partition_point(|&idx| seq[idx] < seq[i]);
        if pos > 0 {
            prev[i] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
    }
    let mut mask = vec![false; n];
    let mut cur = tails.last().copied();
    while let Some(idx) = cur {
        mask[idx] = true;
        cur = prev[idx];
    }
    mask
}

fn apply_creative_list(
    host: &mut dyn mod_api::ClientHost,
    event: &mut mod_api::BuildCreativeModeTabContentsEvent<'_>,
    runtime_host: &mut StdHost,
    resulting: &[Value],
    seeded: &[(Value, mod_api::ItemStack)],
    visibility: mod_api::CreativeTabVisibility,
) {
    // 1. Convert every entry (retaining untouched seeded stacks verbatim so Sunrise components
    //    never downgrade, and counting/skipping any stack whose components cannot be converted),
    //    then deduplicate in FIRST-occurrence order exactly like Fabric's
    //    `this.displayItems.clear(); this.displayItems.addAll(displayList);` (ItemStackLinkedSet).
    let mut desired: Vec<(Value, mod_api::ItemStack, Option<usize>)> =
        Vec::with_capacity(resulting.len());
    let mut claimed_seed = vec![false; seeded.len()];
    for value in resulting {
        let (stack, exact_seed) = if let Some((idx, (_, original))) = seeded
            .iter()
            .enumerate()
            .find(|(_, (seed_value, _))| seed_value == value)
        {
            (original.clone(), Some(idx))
        } else {
            let Some(stack) = api_stack(host, runtime_host, value) else {
                runtime_host.note_unknown_game_hook("creative tab stack conversion");
                continue;
            };
            (stack, None)
        };
        if desired.iter().any(|(existing_val, existing_stack, _)| {
            existing_stack.is_stackable_with(&stack)
                || matches!(
                    runtime_host.stacks_same_item_same_components(existing_val, value),
                    Ok(Some(true))
                )
        }) {
            continue;
        }
        let seed_idx = exact_seed.filter(|&idx| !claimed_seed[idx]).or_else(|| {
            seeded
                .iter()
                .enumerate()
                .find(|(idx, (_, orig))| !claimed_seed[*idx] && orig.is_stackable_with(&stack))
                .map(|(idx, _)| idx)
        });
        if let Some(idx) = seed_idx {
            claimed_seed[idx] = true;
        }
        desired.push((value.clone(), stack, seed_idx));
    }

    // 2. Keep the longest subsequence of already-present seeded entries that remain in their
    //    existing relative order, and remove only entries that moved earlier or were dropped.
    let present_in_desired: Vec<(usize, usize)> = desired
        .iter()
        .enumerate()
        .filter_map(|(desired_idx, (_, _, seed_idx))| seed_idx.map(|s| (desired_idx, s)))
        .collect();
    let seq: Vec<usize> = present_in_desired.iter().map(|&(_, s)| s).collect();
    let lis_mask = longest_increasing_mask(&seq);
    let mut kept_seed = vec![false; seeded.len()];
    for (&(_, seed_idx), &keep) in present_in_desired.iter().zip(&lis_mask) {
        if keep {
            kept_seed[seed_idx] = true;
        }
    }
    for (idx, (_, orig)) in seeded.iter().enumerate() {
        if !kept_seed[idx]
            && let Err(error) = event.edit.remove(host, orig, visibility)
        {
            runtime_host.note_unknown_game_hook("creative tab edit");
            log::warn!(
                "{LOG}: creative tab {} remove refused: {error:?}",
                event.tab
            );
        }
    }

    // 3. Walk `desired` left-to-right, inserting every non-kept entry immediately after `prev`
    //    (or `First` at index 0) and advancing `prev` through every position in `desired`.
    let mut prev: Option<mod_api::ItemStack> = None;
    for (_, stack, seed_idx) in desired {
        if seed_idx.is_some_and(|idx| kept_seed[idx]) {
            prev = Some(stack);
            continue;
        }
        let placement = match &prev {
            Some(anchor) => mod_api::CreativeTabPlacement::After(anchor.clone()),
            None => mod_api::CreativeTabPlacement::First,
        };
        match event
            .edit
            .insert(host, placement.clone(), stack.clone(), visibility)
        {
            Ok(()) => prev = Some(stack),
            Err(mod_api::CreativeTabEditError::Duplicate(list)) => {
                // Sunrise may normalize components (e.g. dropping a patch entry equal to a
                // prototype default) so two distinct raw patches collapse to one creative key.
                // If that existing equivalent entry sits AFTER `prev`, move it here to preserve
                // Fabric's first-occurrence order; if it sits BEFORE `prev`, keep the earlier
                // occurrence in place.
                if let Ok(current) = event.edit.entries(host, list) {
                    let prev_pos = prev.as_ref().and_then(|anchor| {
                        current
                            .iter()
                            .rposition(|existing| existing.is_stackable_with(anchor))
                    });
                    let dup_pos = current
                        .iter()
                        .position(|existing| existing.is_stackable_with(&stack));
                    let sits_after_prev = match (prev_pos, dup_pos) {
                        (None, Some(d)) => d > 0,
                        (Some(p), Some(d)) => d > p,
                        _ => false,
                    };
                    if sits_after_prev
                        && let Some(existing) = dup_pos.map(|d| current[d].clone())
                        && event.edit.remove(host, &existing, visibility).is_ok()
                        && event
                            .edit
                            .insert(host, placement, stack.clone(), visibility)
                            .is_ok()
                    {
                        prev = Some(stack);
                    }
                }
            }
            Err(error) => {
                runtime_host.note_unknown_game_hook("creative tab edit");
                log::warn!(
                    "{LOG}: creative tab {} insert refused: {error:?}",
                    event.tab
                );
            }
        }
    }
}

pub(crate) fn apply_creative_contents(
    host: &mut dyn mod_api::ClientHost,
    event: &mut mod_api::BuildCreativeModeTabContentsEvent<'_>,
    runtime_host: &mut StdHost,
    contents: &runtime::CreativeTabContents,
    seeded_parent: &[(Value, mod_api::ItemStack)],
    seeded_search: &[(Value, mod_api::ItemStack)],
) {
    apply_creative_list(
        host,
        event,
        runtime_host,
        &contents.display_stacks,
        seeded_parent,
        mod_api::CreativeTabVisibility::Parent,
    );
    apply_creative_list(
        host,
        event,
        runtime_host,
        &contents.search_tab_stacks,
        seeded_search,
        mod_api::CreativeTabVisibility::Search,
    );
}

pub(crate) fn convert_tooltip_component(
    host: &dyn mod_api::ClientHost,
    runtime_host: &StdHost,
    component: &Value,
) -> (Option<mod_api::Text>, bool) {
    let Some(json) = runtime_host.component_json(component) else {
        return (None, true);
    };
    match host.text_from_json(&json) {
        Ok(Some(text)) => (Some(text), false),
        Ok(None) | Err(_) => (None, true),
    }
}

fn log_registration(row: &Row) {
    log::info!(
        "{LOG}: client registration {} ({}): keys={}, hud_layers={}, listeners={}, unfired={}, refused={}",
        row.file,
        row.id,
        row.keys,
        row.hud_layers,
        row.listeners,
        row.unsupported.values().sum::<usize>(),
        row.refused_registrations.len()
    );
}

/// A client jar whose host is seeded and linked, before the bootstrap and its entrypoints.
struct PreparedJar {
    file: String,
    id: String,
    jar: Arc<NativeJar>,
    host: StdHost,
}

/// Seeds one client jar's host and links its dependencies; refused without a client entrypoint.
#[allow(clippy::result_large_err)]
fn prepare_jar(
    file: String,
    jar: Arc<NativeJar>,
    links: &[(String, Arc<NativeJar>)],
    imc: &runtime::neoforge_imc::ImcBus,
    game_version: Option<&str>,
) -> Result<PreparedJar, Row> {
    let id = jar_id(&jar, &file);
    let entrypoints = jar.role_entrypoints(true);
    if entrypoints.is_empty() {
        return Err(Row::refused(&file, id, "no client entrypoint"));
    }
    let mut seed = StdHost::new().with_environment_type("CLIENT");
    if let Some(version) = game_version {
        seed = seed.with_game_version(version);
    }
    let mut host = match jar.try_seed_host(seed) {
        Ok(host) => host,
        Err(error) => return Err(Row::refused(&file, id, error)),
    };
    imc.join(&file, &jar, &mut host);
    for (link, dependency) in links {
        if let Err(error) = host.link_jar(link.clone(), dependency) {
            return Err(Row::refused(
                &file,
                id,
                format!("linking {link} failed: {error}"),
            ));
        }
    }
    Ok(PreparedJar {
        file,
        id,
        jar,
        host,
    })
}

/// Runs a prepared client jar's entrypoints once the bootstrap ran. `boot` is the bootstrap's
/// failure: the game did not start, so the jar runs nothing and is refused with that reason.
#[allow(clippy::result_large_err)]
fn run_prepared(
    prepared: PreparedJar,
    imc: &runtime::neoforge_imc::ImcBus,
    boot: Option<&str>,
) -> Result<LoadedJar, Row> {
    let PreparedJar {
        file,
        id,
        jar,
        mut host,
    } = prepared;
    if let Some(failure) = boot {
        return Err(Row::refused(
            &file,
            id,
            format!("the game did not start: {failure}"),
        ));
    }
    let entrypoints = jar.role_entrypoints(true);
    let mut row = Row {
        file,
        id: id.clone(),
        verdict: "ran-clean",
        ..Row::default()
    };
    let mut failures = Vec::new();
    let mut failed_classes = Vec::new();
    for (kind, entry) in &entrypoints {
        let counts = row.stages.entry((*kind).to_owned()).or_default();
        counts.attempted += 1;
        if let Some(adapter) = entry.adapter.as_deref() {
            row.entrypoints_failed += 1;
            counts.failed += 1;
            counts.disabled += 1;
            failures.push(format!(
                "{}: entrypoint adapter {adapter} is not supported",
                entry.class
            ));
            continue;
        }
        match caught(
            &format!("{} client entrypoint {} ({kind})", row.id, entry.class),
            || {
                jar.execute_with_budget(
                    &entry.class,
                    entry.method.as_deref(),
                    kind,
                    &mut host,
                    &ExecutionBudget::within(JAR_BUDGET),
                )
            },
        ) {
            Ok(Ok(report)) => {
                row.entrypoints_ran += 1;
                row.host_calls += report.calls;
                if let Some(counts) = row.stages.get_mut(*kind) {
                    counts.calls += report.calls;
                }
            }
            Ok(Err(error)) => {
                row.entrypoints_failed += 1;
                if let Some(counts) = row.stages.get_mut(*kind) {
                    counts.failed += 1;
                    counts.disabled += 1;
                }
                failed_classes.push(entry.class.as_str());
                failures.push(format!("{}: {error}", entry.class));
            }
            Err(panic) => {
                row.entrypoints_failed += 1;
                if let Some(counts) = row.stages.get_mut(*kind) {
                    counts.failed += 1;
                    counts.disabled += 1;
                }
                failed_classes.push(entry.class.as_str());
                failures.push(format!("{}: panicked: {panic}", entry.class));
            }
        }
    }
    if row.entrypoints_ran == 0 {
        row.verdict = "refused";
        row.reason = Some(failures.join("; "));
        row.unknown_host_calls = host.unknown_host_call_count();
        return Err(row);
    }
    for problem in jar.inject_event_subscribers(
        &mut host,
        &ExecutionBudget::within(JAR_BUDGET),
        &failed_classes,
    ) {
        failures.push(problem);
    }
    imc.lend(&row.file, Arc::clone(&jar), host);
    let mut loaded = LoadedJar {
        row,
        jar,
        host: StdHost::new().with_environment_type("CLIENT"),
        listeners: Vec::new(),
        orderings: Vec::new(),
    };
    for stage in runtime::neoforge_lifecycle::LoadingStage::ALL {
        if let Some(host) = imc.reclaim(&loaded.row.file) {
            loaded.host = host;
        }
        let stage_name = stage.name().to_owned();
        let counts = loaded.row.stages.entry(stage_name.clone()).or_default();
        counts.attempted += 1;
        let before_calls = loaded.host.calls().len();
        let budget = ExecutionBudget::within(JAR_BUDGET);
        let (jar, host) = (&loaded.jar, &mut loaded.host);
        let stage_failures = caught(&format!("{} {}", loaded.row.id, stage.name()), || {
            jar.run_loading_stage(stage, host, &budget)
        })
        .unwrap_or_else(|panic| vec![format!("{}: panicked: {panic}", stage.name())]);
        let deferred_failures = caught(
            &format!("{} {} deferred work", loaded.row.id, stage.name()),
            || jar.run_deferred_work(host, &budget),
        )
        .unwrap_or_else(|panic| vec![format!("{} deferred work panicked: {panic}", stage.name())]);
        let stage_failed = !stage_failures.is_empty() || !deferred_failures.is_empty();
        let calls = loaded.host.calls().len().saturating_sub(before_calls);
        let counts = loaded.row.stages.get_mut(&stage_name).expect("stage count");
        counts.calls += calls;
        if stage_failed {
            counts.failed += 1;
            counts.disabled += 1;
        }
        loaded.row.host_calls += calls;
        failures.extend(stage_failures);
        failures.extend(deferred_failures);
        imc.lend(
            &loaded.row.file,
            Arc::clone(&loaded.jar),
            std::mem::take(&mut loaded.host),
        );
    }
    loaded.host = imc
        .reclaim(&loaded.row.file)
        .unwrap_or_else(|| StdHost::new().with_environment_type("CLIENT"));
    let registered = loaded.host.game_listeners(&loaded.jar, false);
    loaded.listeners = registered.listeners;
    loaded.orderings = registered.phase_orderings;
    let (layers, refused) = loaded.host.client_hud_layers(&loaded.jar);
    loaded.row.keys = loaded.host.client_key_mappings().len();
    loaded.row.hud_layers = layers.len();
    loaded.row.listeners = loaded.listeners.len();
    loaded.row.unsupported = registered.unsupported;
    loaded.row.refused_registrations = loaded
        .host
        .client_refused_registrations()
        .iter()
        .cloned()
        .chain(refused)
        .collect();
    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
    if !failures.is_empty() {
        loaded.row.verdict = "ran-partial";
        loaded.row.reason = Some(failures.join("; "));
    } else if loaded.row.unknown_host_calls > 0 {
        loaded.row.verdict = "ran-hollow";
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn jar(resources: &[(&str, &[u8])]) -> NativeJar {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut bytes);
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in resources {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        NativeJar::from_bytes(&bytes.into_inner()).unwrap()
    }

    #[test]
    fn fabric_client_entrypoint_fixture_is_selected_for_client_owner() {
        let jar = jar(&[(
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"fixture","version":"1","entrypoints":{"client":["fixture.Client"]}}"#,
        )]);
        let entries = jar.role_entrypoints(true);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "client");
        assert_eq!(entries[0].1.class, "fixture.Client");
        assert!(jar.role_entrypoints(false).is_empty());
    }

    #[test]
    fn neoforge_client_mod_fixture_is_selected_for_client_owner() {
        let class = client_mod_class();
        let jar = jar(&[
            (
                "META-INF/neoforge.mods.toml",
                br#"modLoader = "javafml"
loaderVersion = "[1,)"
license = "MIT"
[[mods]]
modId = "fixture"
version = "1"
"#,
            ),
            ("fixture/ClientMod.class", &class),
        ]);
        let entries = jar.role_entrypoints(true);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "neoforge");
        assert_eq!(entries[0].1.class, "fixture.ClientMod");
        assert!(jar.role_entrypoints(false).is_empty());
    }

    fn client_mod_class() -> Vec<u8> {
        fn u2(out: &mut Vec<u8>, value: u16) {
            out.extend_from_slice(&value.to_be_bytes());
        }
        fn utf8(out: &mut Vec<u8>, value: &str) {
            out.push(1);
            u2(out, value.len() as u16);
            out.extend_from_slice(value.as_bytes());
        }
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52];
        u2(&mut out, 12);
        utf8(&mut out, "fixture/ClientMod");
        out.extend_from_slice(&[7, 0, 1]);
        utf8(&mut out, "java/lang/Object");
        out.extend_from_slice(&[7, 0, 3]);
        utf8(&mut out, "Lnet/neoforged/fml/common/Mod;");
        utf8(&mut out, "RuntimeVisibleAnnotations");
        utf8(&mut out, "value");
        utf8(&mut out, "fixture");
        utf8(&mut out, "dist");
        utf8(&mut out, "Lnet/neoforged/api/distmarker/Dist;");
        utf8(&mut out, "CLIENT");
        out.extend_from_slice(&[0, 0x21, 0, 2, 0, 4, 0, 0, 0, 0, 0, 0]);
        u2(&mut out, 1);
        u2(&mut out, 6);
        let mut annotation = Vec::new();
        u2(&mut annotation, 1);
        u2(&mut annotation, 5);
        u2(&mut annotation, 2);
        u2(&mut annotation, 7);
        annotation.extend_from_slice(&[b's', 0, 8]);
        u2(&mut annotation, 9);
        annotation.push(b'[');
        u2(&mut annotation, 1);
        annotation.push(b'e');
        u2(&mut annotation, 10);
        u2(&mut annotation, 11);
        out.extend_from_slice(&(annotation.len() as u32).to_be_bytes());
        out.extend_from_slice(&annotation);
        out
    }
}
