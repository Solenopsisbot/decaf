//! Milestone 3: a jar's recorded registrations become content before the game freezes its
//! registries.
//!
//! The game asks native mods for blocks and items at the **Registration** phase
//! (`mod_api::Hooks::blocks`/`items`, NeoForge's `RegisterEvent`), before any resource or world
//! loads and while the content root can still grow. M1 ran jars at `server_about_to_start`, which
//! is too late for that, so the jars run here instead: once per process, in this module, and the
//! remaining hooks fire the listeners they registered.
//!
//! Reading the jars needs the game's `config/` folder, and the scoped config service
//! (`Host::config_files`) first exists in the Config phase. This module reads
//! `config/horizon_java/java-mods` with `std::fs` instead: the game folder is the process's working
//! directory (`--game-dir` is a `set_current_dir`), and native mods are trusted code. The report
//! still goes through the config service, where it exists.
//!
//! Transcription is deliberately narrow. `runtime::content` maps a typed
//! `RegistryDeclaration::{Block, Item}` into the content vocabulary (the template and the property
//! keys the fork reads); this module only applies the event fields, skips ids the game's
//! namespace fence does not let the mod own, and leaves the rest in the jar's row as
//! `docs/m3-content-design.md` maps it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mod_api::{
    BlocksEvent, ComponentTypesEvent, ItemsEvent, PackageKind, Registrar, RegistrationContext,
};

use crate::runtime;
use crate::{
    JARS, JarRow, LOG, LoadedJar, State, bootstrap_jars, jar_id, prepare_jar, run_prepared,
};

/// Run a registered jar block's vanilla placement override. The game hook
/// supplies a resolved Sunrise placement snapshot; the runtime owns the
/// `BlockPlaceContext` façade and returns the canonical Java `BlockState`.
/// `None` is vanilla's `null` refusal. This helper deliberately does not
/// inspect a mod name or a block-specific property.
pub(crate) fn state_for_placement(
    jar: &mut LoadedJar,
    block_id: &str,
    context: runtime::BlockPlaceContextData,
) -> Result<Option<runtime::Value>, runtime::NativeError> {
    let block = jar
        .host
        .registrations()
        .iter()
        .rev()
        .find(|entry| entry.registry == "minecraft:block" && entry.id == block_id)
        .map(|entry| entry.value.clone());
    let Some(block) = block else { return Ok(None) };
    jar.jar
        .block_state_for_placement(block, context, &mut jar.host)
}

pub(crate) enum PlacementContentOutcome {
    /// Java returned `null` (vanilla placement refusal) with no unknown calls.
    Refused,
    /// Java returned a decoded `(block_id, properties)` state with no unknown calls.
    State(String, BTreeMap<String, String>),
    /// Java returned a non-null state whose block could not be decoded (e.g. unregistered).
    UnknownState,
    /// One or more unmodelled / unknown-placer calls were hit during execution. This wins over
    /// every other outcome: a counted call's placeholder answer may have steered the result.
    CountedUnknown(usize),
    /// Java threw out of `getStateForPlacement` (the thrown class's binary name) with no counted
    /// call: a real Java throw, such as the NullPointerException a `DirectionalPlaceContext`'s
    /// `getNearestLookingVerticalDirection` raises because it has no player.
    Threw(String),
}

pub(crate) fn state_for_placement_content(
    jar: &mut LoadedJar,
    block_id: &str,
    context: runtime::BlockPlaceContextData,
) -> Result<PlacementContentOutcome, runtime::NativeError> {
    let unknown_before = jar.host.unknown_host_call_count();
    let result = state_for_placement(jar, block_id, context);
    let unknown_delta = jar
        .host
        .unknown_host_call_count()
        .saturating_sub(unknown_before);
    jar.row.callback_unknown_host_calls += unknown_delta;
    if unknown_delta > 0 {
        return Ok(PlacementContentOutcome::CountedUnknown(unknown_delta));
    }
    let state = match result {
        Ok(state) => state,
        Err(runtime::NativeError::UncaughtException { class, .. }) => {
            return Ok(PlacementContentOutcome::Threw(class));
        }
        Err(error) => return Err(error),
    };
    let Some(state) = state else {
        return Ok(PlacementContentOutcome::Refused);
    };
    Ok(match jar.host.block_state_content(&state) {
        Some((id, props)) => PlacementContentOutcome::State(id, props),
        None => PlacementContentOutcome::UnknownState,
    })
}

/// This mod's manifest id (`mod/sunrise-mod.toml`): the package whose namespaces the game's
/// declaration fence checks.
pub(crate) const MOD_ID: &str = "horizon_java";

/// Attaches [`crate::JavaPlacementPart`] (the jar block's `getStateForPlacement`) to every jar
/// block a part may be attached to ([`attachable_registrations`]): exactly the blocks
/// [`register_block_parts`] gives a block-use part, so a block of a baked content package (Runes'
/// altar, in the `runes` package) gets its placement as it gets its block use.
pub(crate) fn register_placement_parts(
    state: &mut State,
    context: &RegistrationContext,
    registrar: &mut dyn mod_api::Registrar,
    shared: Arc<Mutex<State>>,
) {
    state.ensure_loaded(context, registration_game_version(context));
    for id in attachable_registrations(&state.jars, context, "minecraft:block") {
        let key = registrar.block_key(&id);
        registrar.block_part(
            key,
            crate::JavaPlacementPart {
                state: Arc::clone(&shared),
            },
        );
    }
}

impl State {
    pub(crate) fn has_registered_block(&self, block_id: &str) -> bool {
        self.jars.iter().any(|jar| {
            jar.host
                .registrations()
                .iter()
                .any(|entry| entry.registry == "minecraft:block" && entry.id == block_id)
        })
    }

    /// Placement hook adapter used by the Sunrise `state_for_placement` part.
    /// The lookup is by the game's registered block id; no block-specific
    /// behavior is embedded in the native mod.
    pub(crate) fn state_for_placement_content(
        &mut self,
        block_id: &str,
        context: runtime::BlockPlaceContextData,
    ) -> Result<Option<PlacementContentOutcome>, runtime::NativeError> {
        for jar in &mut self.jars {
            if jar
                .host
                .registrations()
                .iter()
                .any(|entry| entry.registry == "minecraft:block" && entry.id == block_id)
            {
                return state_for_placement_content(jar, block_id, context).map(Some);
            }
        }
        Ok(None)
    }

    /// Runs every jar once, the first time content is asked for, and keeps its runtime for the
    /// server lifecycle hooks. The game asks for blocks before items, so this guard also makes
    /// the two hooks one pass.
    ///
    /// `game_version` is the running game's Minecraft version as the game reports it
    /// ([`registration_game_version`], or `ConfigHost::game_version` where a host exists). Every
    /// jar's host is seeded with it, so `minecraft`'s mod container (Fabric's `getAllMods` and
    /// `getModContainer`, FML's `ModList`) and `DetectedVersion`/`SharedConstants
    /// .getCurrentVersion()` answer the game's own value. `None` leaves both unanswered (counted),
    /// as the runtime does for a host that was not told, rather than inventing a release.
    pub(crate) fn ensure_loaded(
        &mut self,
        context: &RegistrationContext,
        game_version: Option<&str>,
    ) {
        let owned = owned_namespaces(context);
        self.ensure_loaded_with(read_jar_files, &owned, game_version);
    }

    /// [`State::ensure_loaded`] with the folder read injected, so a test can count how often the
    /// content phase reaches for the jars: blocks and items both ask, but a jar runs once.
    fn ensure_loaded_with(
        &mut self,
        read: impl FnOnce() -> Vec<(String, Vec<u8>)>,
        owned: &[String],
        game_version: Option<&str>,
    ) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        // Parse every jar before running any: a declared dependency is a sibling
        // jar's metadata, so the graph is only knowable once the whole folder
        // has been read.
        let mut parsed: Vec<(String, Arc<runtime::NativeJar>)> = Vec::new();
        for (file, bytes) in read() {
            match runtime::NativeJar::from_bytes(&bytes) {
                Ok(jar) => parsed.push((file, Arc::new(jar))),
                Err(error) => {
                    let id = file.strip_suffix(".jar").unwrap_or(&file).to_owned();
                    let row = JarRow::refused(&file, id, format!("not a readable jar: {error}"));
                    log::info!("{LOG}: {row}");
                    self.refused.push(row);
                }
            }
        }
        let refs: Vec<(&str, &runtime::NativeJar)> = parsed
            .iter()
            .map(|(file, jar)| (file.as_str(), jar.as_ref()))
            .collect();
        let plan = runtime::dependencies::plan(&refs);
        // FML's `InterModComms` queues, one per game (`runtime::neoforge_imc`): every jar that
        // loads is in `ModList` before any constructor runs, so any of them can be sent to.
        let imc = runtime::neoforge_imc::ImcBus::new();
        for &index in &plan.order {
            if !plan.missing.contains_key(&index) {
                imc.add_mods(&parsed[index].1);
            }
        }
        let mut prepared = Vec::new();
        for &index in &plan.order {
            let (file, jar) = parsed[index].clone();
            if let Some(missing) = plan.missing.get(&index) {
                let row = JarRow::refused(
                    &file,
                    jar_id(&jar, &file),
                    format!("requires {} (not in {JARS})", missing.join(", ")),
                );
                log::info!("{LOG}: {row}");
                self.refused.push(row);
                continue;
            }
            let links: Vec<_> = plan
                .links_for(index)
                .iter()
                .map(|&dependency| {
                    let (file, jar) = &parsed[dependency];
                    (file.clone(), Arc::clone(jar))
                })
                .collect();
            match prepare_jar(file, jar, &links, &imc, game_version) {
                Ok(jar) => prepared.push(jar),
                Err(row) => {
                    log::info!("{LOG}: {row}");
                    self.refused.push(row);
                }
            }
        }
        // The game bootstraps once, before any mod's entrypoint: every jar's Mixin callbacks
        // into game classes' static initializers run in one pass, in the game's order
        // (`runtime::bootstrap_mixins`). One that throws means the game does not start, and every
        // jar is refused with it.
        let boot = bootstrap_jars(&mut prepared);
        if let Some(failure) = &boot {
            log::warn!("{LOG}: the game's bootstrap failed in a jar's Mixin callback: {failure}");
        }
        for jar in prepared {
            match run_prepared(jar, boot.as_deref()) {
                Ok(loaded) => {
                    log::info!("{LOG}: {}", loaded.row);
                    self.jars.push(loaded);
                }
                Err(row) => {
                    log::info!("{LOG}: {row}");
                    self.refused.push(row);
                }
            }
        }
        // NeoForge's loading stages after the constructors (`runtime::neoforge_lifecycle`): each
        // stage posts its event to every jar's containers, then runs every jar's deferred work, as
        // FML runs a step's `DeferredWorkQueue` once every mod's event was posted.
        // While the stages run, every jar's host but the running one's is lent to the IMC bus, so
        // a jar reading another's message can run that jar's supplier.
        for jar in &mut self.jars {
            imc.lend(
                &jar.row.file,
                Arc::clone(&jar.jar),
                std::mem::take(&mut jar.host),
            );
        }
        for stage in runtime::neoforge_lifecycle::LoadingStage::ALL {
            for jar in &mut self.jars {
                on_own_host(&imc, jar, |jar| jar.run_loading_stage(stage));
            }
            for jar in &mut self.jars {
                on_own_host(&imc, jar, |jar| jar.run_deferred_work(stage));
            }
            // NeoForge's `RegisterEvent` for `neoforge:attachment_types`, in load order
            // (`runtime::neoforge_attachments`): the registry step that follows construction.
            if stage == runtime::neoforge_lifecycle::LoadingStage::Construct {
                for jar in &mut self.jars {
                    on_own_host(&imc, jar, |jar| {
                        let budget = runtime::ExecutionBudget::within(crate::JAR_BUDGET);
                        for problem in jar.jar.register_attachment_types(&mut jar.host, &budget) {
                            log::warn!(
                                "{LOG}: {}: attachment type not registered: {problem}",
                                jar.row.id
                            );
                        }
                    });
                }
            }
        }
        for jar in &mut self.jars {
            if let Some(host) = imc.reclaim(&jar.row.file) {
                jar.host = host;
            }
        }
        for jar in &mut self.jars {
            jar.finish_loading();
        }
        // The items' default components of the jars' own types, encoded with each type's codec
        // now that every stage has registered what it will (`runtime::jar_components`).
        for jar in &mut self.jars {
            jar.jar.declare_jar_component_defaults(&mut jar.host);
        }
        for jar in &mut self.jars {
            let declarations = runtime::content::declarations(jar.host.registrations());
            jar.row.content_skipped = unowned_counts(
                owned,
                declarations
                    .iter()
                    .map(|declaration| (declaration.id(), declaration.is_block())),
            );
        }
        for jar in &self.jars {
            for (namespace, (blocks, items)) in &jar.row.content_skipped {
                log::info!(
                    "{LOG}: {}: content: {blocks} block(s), {items} item(s) in namespace \
                     {namespace} not declared: not owned by {MOD_ID}; run tools/bake.py",
                    jar.row.file
                );
            }
        }
    }
}

/// Runs `step` on `jar` with its own host taken back from the IMC bus, and lends it again after.
fn on_own_host(
    imc: &runtime::neoforge_imc::ImcBus,
    jar: &mut LoadedJar,
    step: impl FnOnce(&mut LoadedJar),
) {
    if let Some(host) = imc.reclaim(&jar.row.file) {
        jar.host = host;
    }
    step(jar);
    imc.lend(
        &jar.row.file,
        Arc::clone(&jar.jar),
        std::mem::take(&mut jar.host),
    );
}

/// The namespaces this mod may declare content in, exactly as the game's fence checks them
/// (`sunrise-mod-api/src/parts/owned_namespaces.rs`): its own id, then the installed package's
/// manifest `namespaces`. A jar's namespace is learned at install time, so it is not here unless
/// the manifest was built with it; its content belongs in a baked content package instead.
pub(crate) fn owned_namespaces(context: &RegistrationContext) -> Vec<String> {
    match context.mods().get(MOD_ID) {
        Some(package) => package.namespaces().map(str::to_owned).collect(),
        None => vec![MOD_ID.to_owned()],
    }
}

/// The game's Minecraft version from a registration context (`RegistrationContext::game_version`,
/// which the game captures before the first registration hook).
///
/// That accessor panics on a context made without the game's registration data (the fallback in
/// `HorizonJava::ensure_jars` makes one at server start). The instance folder comes from the same
/// data (`RegistrationContext::new` takes both from it), so its presence says the version may be
/// asked; without it the caller has a host to ask instead.
pub(crate) fn registration_game_version(context: &RegistrationContext) -> Option<&str> {
    context
        .instance_dir()
        .is_some()
        .then(|| context.game_version())
}

/// Whether `id` (`namespace:path`) is in one of `owned`'s namespaces.
fn owns(owned: &[String], id: &str) -> bool {
    id.split_once(':')
        .is_some_and(|(namespace, _)| owned.iter().any(|owned| owned == namespace))
}

/// The typed declarations `owned` does not cover, per namespace: (blocks, items). The game's
/// dispatch would refuse every one of them, so none may be pushed; the counts become the jar's
/// `content.skipped` row and one log line.
fn unowned_counts<'a>(
    owned: &[String],
    declarations: impl Iterator<Item = (&'a str, bool)>,
) -> BTreeMap<String, (usize, usize)> {
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (id, block) in declarations {
        if !usable_id(id) || owns(owned, id) {
            continue;
        }
        let (namespace, _) = id.split_once(':').expect("usable id has a namespace");
        let entry = counts.entry(namespace.to_owned()).or_default();
        if block {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    counts
}

/// Registers, as Sunrise NBT component types, the data component types the jars built and
/// registered into `minecraft:data_component_type` with a persistent codec
/// (`StdHost::jar_component_types`), each id once. A type is this mod's to register when its
/// namespace is one this mod owns ([`owned_namespaces`]) or one a content package serves through it
/// ([`served_namespace`]: the bake writes `served_by = "horizon_java"` into a jar's packages for
/// that). A transient type (no codec) is no NBT type and is left out; Sunrise refuses a repeated
/// id at start, so a second jar's registration of an id is logged and left out.
pub(crate) fn declare_component_types(
    state: &mut State,
    context: &RegistrationContext,
    event: &mut ComponentTypesEvent<'_>,
) {
    state.ensure_loaded(context, registration_game_version(context));
    let owned = owned_namespaces(context);
    let mut declared = BTreeSet::new();
    for jar in &state.jars {
        for kind in jar.host.jar_component_types() {
            if !kind.persistent {
                continue;
            }
            if !usable_id(&kind.id) {
                log::warn!(
                    "{LOG}: {}: component type {} is not an id this mod can register; left out",
                    jar.row.id,
                    kind.id
                );
                continue;
            }
            if !owns(&owned, &kind.id) && !served_namespace(context, &kind.id) {
                log::info!(
                    "{LOG}: {}: component type {} not registered: its namespace is neither owned \
                     nor served by {MOD_ID}; run tools/bake.py",
                    jar.row.id,
                    kind.id
                );
                continue;
            }
            if !declared.insert(kind.id.clone()) {
                log::warn!(
                    "{LOG}: {}: component type {} is registered by another jar too; only the \
                     first is used",
                    jar.row.id,
                    kind.id
                );
                continue;
            }
            event.register(kind.id);
        }
    }
    if !declared.is_empty() {
        log::info!("{LOG}: content: {} component type(s)", declared.len());
    }
}

/// Whether an installed content package that names this mod in `served_by` owns the id's
/// namespace: the game's namespace authority then gives it to this mod
/// (`sunrise-mod-vocab/src/parts/owned_namespaces.rs` `NamespaceAuthority::from_packages`).
fn served_namespace(context: &RegistrationContext, id: &str) -> bool {
    let Some((namespace, _)) = id.split_once(':') else {
        return false;
    };
    context.mods().iter().any(|package| {
        package.kind() == PackageKind::Content
            && package.served_by() == Some(MOD_ID)
            && package.namespaces().any(|owned| owned == namespace)
    })
}

/// Pushes one `content::Block` per typed block declaration. A block keeps the runtime's inferred
/// template (`minecraft:stone`) and the properties the content vocabulary has a key for.
///
/// It does **not** bring an item. Vanilla registers block items one by one in `Items`
/// (`Items.STONE`, `Items.DIRT`...), not by walking the block registry, and a Fabric/NeoForge mod
/// registers its own `BlockItem` or none at all. The runtime's inferred `item` flag says only
/// that a block was built from `BlockBehaviour.Properties`, which is no evidence of an item. An
/// item this bridge declares is one the jar registered through the item registry
/// (`declare_items`); a block's `item` stays false and `Block.block_item`/`pick_item` stay unset
/// unless the vocabulary carries the mod's own registration later.
pub(crate) fn declare_blocks(
    state: &mut State,
    context: &RegistrationContext,
    event: &mut BlocksEvent<'_>,
) {
    state.ensure_loaded(context, registration_game_version(context));
    let owned = owned_namespaces(context);
    let mut declared = BTreeSet::new();
    let mut count = 0;
    for jar in &mut state.jars {
        for declaration in runtime::content::declarations(jar.host.registrations()) {
            let runtime::content::ContentDeclaration::Block(declaration) = declaration else {
                continue;
            };
            let id = declaration.id.clone();
            if !usable_id(&id) {
                log::warn!(
                    "{LOG}: {}: block {} is not an id this mod can declare; left out",
                    jar.row.id,
                    id
                );
                continue;
            }
            if !owns(&owned, &id) {
                // The game's declaration fence would refuse it, and a refusal is fatal at start.
                // It is in the row's `content.skipped`; a baked content package declares it.
                continue;
            }
            if !declared.insert(id.clone()) {
                log::warn!(
                    "{LOG}: {}: block {} is declared more than once; only the first is used",
                    jar.row.id,
                    id
                );
                continue;
            }
            if let Err(error) = event.register(block_entry(&declaration, &id)) {
                log::warn!("{LOG}: {}: block {id} refused by game: {error}", jar.row.id);
                continue;
            }
            jar.row.content_blocks += 1;
            count += 1;
        }
    }
    log::info!(
        "{LOG}: content: {count} block declaration(s) from {} jar(s)",
        state.jars.len()
    );
}

/// Pushes one `content::Item` per typed item declaration. The template and the keys
/// `item_declaration_properties` exports carry over.
///
/// A block's own `BlockItem` registration is exactly one of these rows, so it is declared with the
/// properties the jar gave it. Nothing is skipped because a block has the same id: the block no
/// longer invents an item (`declare_blocks`), so the item registry is the only source of items.
pub(crate) fn declare_items(
    state: &mut State,
    context: &RegistrationContext,
    event: &mut ItemsEvent<'_>,
) {
    state.ensure_loaded(context, registration_game_version(context));
    let owned = owned_namespaces(context);
    let mut declared = BTreeSet::new();
    let mut count = 0;
    for jar in &mut state.jars {
        for declaration in runtime::content::declarations(jar.host.registrations()) {
            let runtime::content::ContentDeclaration::Item(declaration) = declaration else {
                continue;
            };
            let id = declaration.id.clone();
            if !usable_id(&id) {
                log::warn!(
                    "{LOG}: {}: item {} is not an id this mod can declare; left out",
                    jar.row.id,
                    id
                );
                continue;
            }
            if !owns(&owned, &id) {
                // See the block counterpart: refused by the game's fence, counted in the row,
                // and declared by a baked content package instead.
                continue;
            }
            if !declared.insert(id.clone()) {
                log::warn!(
                    "{LOG}: {}: item {} is declared more than once; only the first is used",
                    jar.row.id,
                    id
                );
                continue;
            }
            if let Err(error) =
                event.register(item_entry(&declaration, &id, !owns_assets(context, &id)))
            {
                log::warn!("{LOG}: {}: item {id} refused by game: {error}", jar.row.id);
                continue;
            }
            jar.row.content_items += 1;
            count += 1;
        }
    }
    log::info!(
        "{LOG}: content: {count} item declaration(s) from {} jar(s)",
        state.jars.len()
    );
}

/// Attaches a generic [`crate::block_use::JavaBlockUsePart`] to every registered Java mod block
/// whose namespace is owned by this mod or by an installed content package (so Sunrise's block
/// key validation always succeeds).
pub(crate) fn register_block_parts(
    state: &Arc<Mutex<State>>,
    registrar: &mut dyn Registrar,
    context: &RegistrationContext,
    client: &Arc<Mutex<crate::client::State>>,
) {
    let mut guard = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.ensure_loaded(context, registration_game_version(context));
    let attached = attachable_registrations(&guard.jars, context, "minecraft:block");
    for id in &attached {
        let key = registrar.block_key(id);
        registrar.block_part(
            key,
            crate::block_use::JavaBlockUsePart::new(Arc::clone(state), id.clone()),
        );
    }
    if !attached.is_empty() {
        log::info!(
            "{LOG}: attached Java block-use part(s) to {} block(s)",
            attached.len()
        );
    }
    // Each menu type a jar registered: its clicks run the Java menu (`live_menu.rs`). The bake
    // declares the Sunrise menu type from the same registration; a type Sunrise doesn't know is
    // never opened, so its part is never asked.
    let menus = attachable_registrations(&guard.jars, context, "minecraft:menu");
    for id in &menus {
        registrar.menu_part(id, crate::live_menu::JavaMenuPart::new(Arc::clone(state)));
        // Its client screen: the one the jar registers with `MenuScreens.register`, found
        // when the client opens the menu (client entrypoints run later, at client setup).
        // A menu type no jar gave a screen opens none, as vanilla's `MenuScreens.create`.
        crate::client::menu_screen::register_screen(registrar, id, client);
    }
    if !menus.is_empty() {
        log::info!(
            "{LOG}: attached the Java live-menu part to {} menu type(s)",
            menus.len()
        );
    }
}

/// The ids every jar registered in `registry` (`minecraft:block`, `minecraft:menu`) that this
/// mod attaches its Sunrise parts to, each once, in jar then registration order. An id qualifies
/// when it is usable and its namespace is one this mod owns ([`owned_namespaces`]) or one an
/// installed content package owns ([`owns_assets`]: a baked jar package such as `runes`, whose
/// content the package declares and this mod runs), so Sunrise's key validation succeeds.
///
/// Every part registration (placement, block use, menus) takes its ids from here, so two parts
/// can't disagree about which blocks are the jar's: #659's placement part once read the typed
/// declarations and checked this mod's own namespaces alone, so a baked package's block got its
/// block use but never its placement (BUGS-3d34 #1).
fn attachable_registrations(
    jars: &[LoadedJar],
    context: &RegistrationContext,
    registry: &str,
) -> Vec<String> {
    let owned = owned_namespaces(context);
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    for jar in jars {
        for entry in jar.host.registrations() {
            let id = entry.id.as_str();
            if entry.registry == registry
                && usable_id(id)
                && (owns(&owned, id) || owns_assets(context, id))
                && seen.insert(id.to_owned())
            {
                ids.push(id.to_owned());
            }
        }
    }
    ids
}

/// Whether an installed content package owns the id's namespace, and so supplies that content's
/// assets (models, item definitions, textures). A runtime declaration then leaves the template's
/// `minecraft:item_model` alone; the package's files decide what is drawn.
fn owns_assets(context: &RegistrationContext, id: &str) -> bool {
    let Some((namespace, _)) = id.split_once(':') else {
        return false;
    };
    context.mods().iter().any(|package| {
        package.kind() == PackageKind::Content
            && package.namespaces().any(|owned| owned == namespace)
    })
}

/// One block declaration as the content vocabulary's block. `item` is false: an item exists only
/// when the jar registered one, and `components` is the item's patch (the vocabulary says so), so
/// a block without an item carries none.
fn block_entry(declaration: &runtime::content::Block, id: &str) -> mod_api::Block {
    let mut block = mod_api::Block::default();
    block.id = id.to_owned();
    block.template = declaration.template.clone();
    block.item = false;
    block.properties.hardness = declaration.hardness;
    block.properties.explosion_resistance = declaration.explosion_resistance;
    block.properties.friction = declaration.friction;
    block.properties.requires_tool = declaration.requires_tool;
    block.properties.occludes = declaration.occludes;
    if let Some(shapes) = &declaration.shapes {
        block.shapes = Some(mod_api::BlockShapes {
            properties: shapes.properties.clone(),
            values: shapes.values.clone(),
            default: shapes.default.clone(),
        });
    }
    if let Some(collision_shapes) = &declaration.collision_shapes {
        block.collision_shapes = Some(mod_api::BlockShapes {
            properties: collision_shapes.properties.clone(),
            values: collision_shapes.values.clone(),
            default: collision_shapes.default.clone(),
        });
    }
    // The vocabulary has `Boolean` and `Enum`: an integer property is an enum of the same value
    // strings, in its `getPossibleValues()` order (as blockstate JSON spells `age=3`). The runtime
    // records only these three kinds; any other is left out rather than guessed.
    block.state_properties = declaration
        .state_properties
        .iter()
        .filter_map(|(name, property)| {
            let state = match property.kind.as_str() {
                "boolean" => mod_api::BlockStateProperty::Boolean,
                "integer" | "enum" => mod_api::BlockStateProperty::Enum {
                    values: property.values.clone(),
                },
                _ => return None,
            };
            Some((name.clone(), state))
        })
        .collect();
    block.default_state = declaration.default_state.clone();
    block
}

/// One item declaration as the content vocabulary's item, with the template's model when no
/// content package of the item's namespace supplies its own.
fn item_entry(
    declaration: &runtime::content::Item,
    id: &str,
    keep_template_model: bool,
) -> mod_api::Item {
    let mut item = mod_api::Item::default();
    item.id = id.to_owned();
    item.template = declaration.template.clone();
    item.max_stack_size = declaration.max_stack_size;
    item.rarity = declaration.rarity.map(|rarity| match rarity {
        runtime::content::Rarity::Common => mod_api::Rarity::Common,
        runtime::content::Rarity::Uncommon => mod_api::Rarity::Uncommon,
        runtime::content::Rarity::Rare => mod_api::Rarity::Rare,
        runtime::content::Rarity::Epic => mod_api::Rarity::Epic,
    });
    item.crafting_remainder = declaration.crafting_remainder.clone();
    // The content plan renames the template's `minecraft:item_model` to the new id, and nothing
    // then defines that id. A content package of the same namespace (the bake in
    // docs/m3-content-design.md) supplies the real definition; without one, keep the template's
    // model so the item draws with the vanilla art.
    if keep_template_model {
        item.components.push((
            "minecraft:item_model".to_owned(),
            Some(mod_api::Tag::String(declaration.template.as_str().into())),
        ));
    }
    item
}

/// A content id this bridge may declare: `namespace:path` in the characters a content namespace
/// and path accept (`Content::namespaced_id`). `minecraft:` is vanilla's and is never redefined by
/// a mod; registering one is recorded in the jar's row, but not declared as content.
fn usable_id(id: &str) -> bool {
    let Some((namespace, path)) = id.split_once(':') else {
        return false;
    };
    let ok = |byte: u8, extra: &[u8]| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || extra.contains(&byte)
    };
    !namespace.is_empty()
        && namespace != "minecraft"
        && !path.is_empty()
        && namespace.bytes().all(|b| ok(b, b"_.-"))
        && path.bytes().all(|b| ok(b, b"_./-"))
}

/// The jars under the game's `config/horizon_java/java-mods/`, sorted by file name so one run's
/// report reads the same as the next. An absent folder is no jars; an unreadable one is a warning,
/// not a failed start.
pub(crate) fn read_jar_files() -> Vec<(String, Vec<u8>)> {
    let Ok(game_dir) = std::env::current_dir() else {
        log::warn!("{LOG}: the game folder is unknown; no jars are read");
        return Vec::new();
    };
    let folder = game_dir.join("config").join(JARS);
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            log::warn!("{LOG}: config/{JARS} could not be listed: {error}");
            return Vec::new();
        }
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(".jar").then(|| (name, entry.path()))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
        .into_iter()
        .filter_map(|(name, path)| match std::fs::read(&path) {
            Ok(bytes) => Some((name, bytes)),
            Err(error) => {
                log::warn!("{LOG}: config/{JARS}/{name} could not be read: {error}");
                None
            }
        })
        .collect()
}

impl LoadedJar {
    /// One loading stage's posts to this jar's containers. A listener that throws is a failure of
    /// the jar's load, as it fails NeoForge's (departure L1: the later stages still run).
    fn run_loading_stage(&mut self, stage: runtime::neoforge_lifecycle::LoadingStage) {
        let budget = runtime::ExecutionBudget::within(crate::JAR_BUDGET);
        let (jar, host) = (&self.jar, &mut self.host);
        match crate::caught(&format!("{} {}", self.row.id, stage.name()), || {
            jar.run_loading_stage(stage, host, &budget)
        }) {
            Ok(problems) => self.failures.extend(problems),
            Err(panic) => self
                .failures
                .push(format!("{}: panicked: {panic}", stage.name())),
        }
    }

    /// The deferred work this jar's containers queued during `stage`.
    fn run_deferred_work(&mut self, stage: runtime::neoforge_lifecycle::LoadingStage) {
        let budget = runtime::ExecutionBudget::within(crate::JAR_BUDGET);
        let (jar, host) = (&self.jar, &mut self.host);
        match crate::caught(
            &format!("{} {}: deferred work", self.row.id, stage.name()),
            || jar.run_deferred_work(host, &budget),
        ) {
            Ok(problems) => self.failures.extend(problems),
            Err(panic) => self
                .failures
                .push(format!("{}: deferred work panicked: {panic}", stage.name())),
        }
    }

    /// After the loading stages: the listeners the jar now holds (a setup listener may register
    /// more), and the row's verdict over everything its load ran. The log line is written again
    /// when the stages changed it.
    fn finish_loading(&mut self) {
        let budget = runtime::ExecutionBudget::within(crate::JAR_BUDGET);
        let (jar, host) = (&self.jar, &mut self.host);
        if let Err(panic) = crate::caught(&format!("{} block shapes", self.row.id), || {
            jar.populate_block_shapes(host, &budget);
        }) {
            self.failures
                .push(format!("block shapes: panicked: {panic}"));
        }
        let registered = self.host.game_listeners(&self.jar, true);
        self.listeners = registered.listeners;
        self.orderings = registered.phase_orderings;
        // The pinned mod API has no payload channel: a NeoForge jar's `PayloadRegistrar`
        // registrations ran, but no packet reaches their handlers, so each one is counted. (A
        // Fabric jar's `PayloadTypeRegistry` rows are the same gap, not yet counted.)
        if self.jar.fabric_id.is_none() && !self.jar.neoforge_entrypoints.is_empty() {
            self.host
                .count_unrouted_payloads("the game's mod API has no payload channel");
        }
        let unknown = self.host.unknown_host_call_count();
        let verdict = runtime::verdict::verdict(self.stages_ran, self.failures.len(), unknown);
        let reason = (!self.failures.is_empty())
            .then(|| format!("entrypoint(s) failed: {}", self.failures.join("; ")));
        let changed = verdict != self.row.verdict
            || unknown != self.row.unknown_host_calls
            || reason != self.row.reason;
        self.row.verdict = verdict;
        self.row.unknown_host_calls = unknown;
        self.row.reason = reason;
        // The loading stages were the last of the jar's start.
        self.row.start_counting(&self.host);
        if changed {
            log::info!("{LOG}: after the loading stages: {}", self.row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A jar built by `javac` from `sources` with `main` as its Fabric `main` entrypoint, its
    /// `demo/` classes packed. The game classes stay stubs, so every call reaches the runtime.
    fn javac_jar(test: &str, sources: &[(&str, &str)], main: &str) -> runtime::NativeJar {
        let mod_json =
            format!(r#"{{"id":"typesdemo","version":"1","entrypoints":{{"main":["{main}"]}}}}"#);
        runtime::NativeJar::from_bytes(&javac_jar_bytes(test, sources, &mod_json, &[])).unwrap()
    }

    /// [`javac_jar`]'s bytes, with `mod_json` as `fabric.mod.json`, `extra` `(entry, text)` files,
    /// and every class under `demo/` (sub-packages included) packed.
    fn javac_jar_bytes(
        test: &str,
        sources: &[(&str, &str)],
        mod_json: &str,
        extra: &[(&str, &str)],
    ) -> Vec<u8> {
        use std::io::Write;
        use std::path::PathBuf;
        let root = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("horizon-java-mod-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (src, classes) = (root.join("src"), root.join("classes"));
        let mut paths = Vec::new();
        for (path, body) in sources {
            let file = src.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, body).unwrap();
            paths.push(file);
        }
        std::fs::create_dir_all(&classes).unwrap();
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        let status = std::process::Command::new(javac)
            .args(["--release", "17", "-nowarn", "-d"])
            .arg(&classes)
            .args(&paths)
            .status()
            .expect("javac runs");
        assert!(status.success(), "javac failed");
        let mut bytes = Vec::new();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("fabric.mod.json", options).unwrap();
        zip.write_all(mod_json.as_bytes()).unwrap();
        for (entry, text) in extra {
            zip.start_file(*entry, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        let mut folders = vec![classes.join("demo")];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    folders.push(path);
                    continue;
                }
                let name = path.strip_prefix(&classes).unwrap().to_string_lossy();
                zip.start_file(name.replace('\\', "/"), options).unwrap();
                zip.write_all(&std::fs::read(&path).unwrap()).unwrap();
            }
        }
        zip.finish().unwrap();
        let _ = std::fs::remove_dir_all(&root);
        bytes
    }

    /// Review of #609 steps 1-2: the game bootstraps once, before any jar's entrypoint, so the
    /// content phase seeds every jar, runs every jar's bootstrap Mixin callbacks in one pass, and
    /// only then their entrypoints. A callback that throws (here the second jar's) is the game not
    /// starting: neither jar runs an entrypoint, and both are refused with that reason, the first
    /// jar (which loads first and has no callback of its own) included.
    #[test]
    fn a_bootstrap_callback_that_throws_refuses_every_jar() {
        let stubs: &[(&str, &str)] = &[
            (
                "net/minecraft/core/component/DataComponents.java",
                "package net.minecraft.core.component; public final class DataComponents {}",
            ),
            (
                "org/spongepowered/asm/mixin/Mixin.java",
                "package org.spongepowered.asm.mixin; import java.lang.annotation.*; @Retention(RetentionPolicy.CLASS) @Target(ElementType.TYPE) public @interface Mixin { Class<?>[] value() default {}; }",
            ),
            (
                "org/spongepowered/asm/mixin/injection/At.java",
                "package org.spongepowered.asm.mixin.injection; import java.lang.annotation.*; @Retention(RetentionPolicy.RUNTIME) @Target({}) public @interface At { String value() default \"\"; }",
            ),
            (
                "org/spongepowered/asm/mixin/injection/Inject.java",
                "package org.spongepowered.asm.mixin.injection; import java.lang.annotation.*; @Retention(RetentionPolicy.RUNTIME) @Target(ElementType.METHOD) public @interface Inject { String[] method(); At[] at(); boolean cancellable() default false; }",
            ),
            (
                "org/spongepowered/asm/mixin/injection/callback/CallbackInfo.java",
                "package org.spongepowered.asm.mixin.injection.callback; public class CallbackInfo {}",
            ),
        ];
        let main = (
            "demo/Main.java",
            "package demo; public final class Main { public void onInitialize() { System.out.println(\"entrypoint ran\"); } }",
        );
        let mut plain = stubs.to_vec();
        plain.push(main);
        let first = javac_jar_bytes(
            "bootstrap-plain",
            &plain,
            r#"{"id":"plain","version":"1","entrypoints":{"main":["demo.Main"]}}"#,
            &[],
        );
        let mut throwing = plain.clone();
        throwing.push((
            "demo/mixin/Refuse.java",
            "package demo.mixin; import net.minecraft.core.component.DataComponents; import org.spongepowered.asm.mixin.Mixin; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.injection.callback.CallbackInfo; @Mixin(DataComponents.class) public class Refuse { @Inject(method = \"<clinit>\", at = @At(\"TAIL\")) private static void tail(CallbackInfo ci) { if (true) throw new IllegalStateException(\"no\"); } }",
        ));
        let second = javac_jar_bytes(
            "bootstrap-throwing",
            &throwing,
            r#"{"id":"throwing","version":"1","entrypoints":{"main":["demo.Main"]},"mixins":["throwing.mixins.json"]}"#,
            &[(
                "throwing.mixins.json",
                r#"{"required":true,"package":"demo.mixin","mixins":["Refuse"]}"#,
            )],
        );
        let mut state = State::default();
        state.ensure_loaded_with(
            || {
                vec![
                    ("a-plain.jar".to_owned(), first),
                    ("b-throwing.jar".to_owned(), second),
                ]
            },
            &[],
            None,
        );
        assert!(state.jars.is_empty(), "no jar runs");
        let reasons: Vec<(&str, &str)> = state
            .refused
            .iter()
            .map(|row| (row.file.as_str(), row.reason.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(reasons.len(), 2, "{reasons:?}");
        for (file, reason) in &reasons {
            assert!(
                reason.starts_with(
                    "the game did not start: demo/mixin/Refuse#tail at the end of \
                     net/minecraft/core/component/DataComponents.<clinit>"
                ),
                "{file}: {reason}"
            );
        }
    }

    /// docs/java-item-parts.md step 1: `Hooks::component_types` registers every data component
    /// type the jars registered with a persistent codec, when its namespace is this mod's own or
    /// one a content package serves through it (`served_by = "horizon_java"`, as the bake writes
    /// it). A transient type, a type of a namespace a package owns without naming this mod, and a
    /// repeated id are left out (Sunrise stops at a repeated or unowned one).
    #[test]
    fn component_types_registers_the_jars_persistent_types_in_owned_and_served_namespaces() {
        use mod_api::{ModEntry, PackageInfo, Side};
        let sources: &[(&str, &str)] = &[
            (
                "com/mojang/serialization/Codec.java",
                "package com.mojang.serialization; public interface Codec<A> { Codec<Integer> INT = null; }",
            ),
            (
                "net/minecraft/resources/Identifier.java",
                "package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { return null; } }",
            ),
            (
                "net/minecraft/core/Registry.java",
                "package net.minecraft.core; import net.minecraft.resources.Identifier; public interface Registry<T> { static <V, T extends V> T register(Registry<V> r, Identifier id, T v) { return v; } }",
            ),
            (
                "net/minecraft/core/registries/BuiltInRegistries.java",
                "package net.minecraft.core.registries; import net.minecraft.core.Registry; import net.minecraft.core.component.DataComponentType; public final class BuiltInRegistries { public static final Registry<DataComponentType<?>> DATA_COMPONENT_TYPE = null; }",
            ),
            (
                "net/minecraft/core/component/DataComponentType.java",
                "package net.minecraft.core.component; import com.mojang.serialization.Codec; public interface DataComponentType<T> { static <T> Builder<T> builder() { return null; } class Builder<T> { public Builder<T> persistent(Codec<T> codec) { return this; } public DataComponentType<T> build() { return null; } } }",
            ),
            (
                "demo/Main.java",
                r#"package demo;
import com.mojang.serialization.Codec;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
public final class Main {
  static void type(String namespace, String path, boolean persistent) {
    DataComponentType.Builder<Integer> builder = DataComponentType.<Integer>builder();
    if (persistent) builder = builder.persistent(Codec.INT);
    Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE,
        Identifier.fromNamespaceAndPath(namespace, path), builder.build());
  }
  public void onInitialize() {
    type("served_api", "pouch", true);
    type("horizon_java", "own", true);
    type("served_api", "charge", false);
    type("plain_api", "thing", true);
    type("minecraft", "stolen", true);
  }
}
"#,
            ),
        ];
        let jar = javac_jar("component-types", sources, "demo.Main");
        let imc = runtime::neoforge_imc::ImcBus::new();
        let loaded = crate::load_jar("types.jar".into(), Arc::new(jar), &[], &imc, None)
            .unwrap_or_else(|row| panic!("{row}"));
        let package = |id: &str, kind: PackageKind, served_by: Option<&str>| {
            PackageInfo::new(
                id.to_owned(),
                "1".to_owned(),
                kind,
                Side::Both,
                vec![id.to_owned()],
                vec![],
            )
            .with_served_by(served_by.map(str::to_owned))
        };
        let java = crate::HorizonJava::default();
        {
            let mut guard = java.state.lock().unwrap();
            guard.jars = vec![loaded];
            // The jars ran at the content phase already; the hook reads no folder.
            guard.loaded = true;
        }
        let mods: mod_api::Mods = vec![ModEntry::new(MOD_ID, Box::new(java))].into();
        let types = mod_api::__private::with_test_package_inventory(
            vec![
                package(MOD_ID, PackageKind::Code, None),
                package("served_api", PackageKind::Content, Some(MOD_ID)),
                package("plain_api", PackageKind::Content, None),
            ],
            || {
                mod_api::testing::with_mods(mods.clone(), || {
                    use mod_api::__private::game::RegistrationContextNew;
                    let mut types = Vec::new();
                    mod_api::dispatch_component_types(&RegistrationContext::new(), &mut types);
                    types
                })
            },
        );
        assert_eq!(types, ["served_api:pouch", "horizon_java:own"]);
    }

    /// The game asks for blocks and items in two hook calls. Each jar's entrypoints must run once:
    /// a second run would register every listener and command twice and load the jar's classes
    /// into a second host.
    #[test]
    fn the_content_phase_reaches_for_the_jars_once() {
        let mut state = State::default();
        let mut reads = 0;
        state.ensure_loaded_with(
            || {
                reads += 1;
                Vec::new()
            },
            &[],
            None,
        );
        state.ensure_loaded_with(
            || {
                reads += 1;
                Vec::new()
            },
            &[],
            None,
        );
        assert_eq!(reads, 1);
        assert!(state.loaded);
    }

    /// A server that opens a second world reuses the jars: its start resets the per-server
    /// counters, not the content pass.
    #[test]
    fn a_second_server_start_does_not_reload_the_jars() {
        let mut state = State::default();
        let mut reads = 0;
        state.ensure_loaded_with(
            || {
                reads += 1;
                Vec::new()
            },
            &[],
            None,
        );
        state.reset_server();
        state.ensure_loaded_with(
            || {
                reads += 1;
                Vec::new()
            },
            &[],
            None,
        );
        assert_eq!(reads, 1);
        assert!(state.loaded);
    }

    /// The content pass puts every jar on one `InterModComms` bus (`runtime::neoforge_imc`): what
    /// one jar sends in `InterModEnqueueEvent` another reads in `InterModProcessEvent`, and the
    /// receiver's `get()` runs the sender's supplier in the sender's runtime, with its state (the
    /// `count` supplier's second call answers 20). Lines from `neoforge_imc/oracle.jsonl`.
    #[test]
    fn the_content_pass_shares_imc_between_jars() {
        let (send, receive) = runtime::neoforge_imc::tests::jar_bytes();
        let mut state = State::default();
        state.ensure_loaded_with(
            || {
                vec![
                    ("imc-a-send.jar".to_owned(), send.clone()),
                    ("imc-b-receive.jar".to_owned(), receive.clone()),
                ]
            },
            &[],
            None,
        );
        assert_eq!(state.jars.len(), 2);
        let sent = state.jars[0].host.take_stdout();
        let received = state.jars[1].host.take_stdout();
        for line in [
            "S got receiver sender back from-receiver",
            "S supplier count runs 2",
        ] {
            assert!(sent.lines().any(|out| out == line), "{line}: {sent}");
        }
        assert!(!sent.contains("S supplier unread runs"), "{sent}");
        for line in [
            "R value hello",
            "R again 20",
            "R then sender deferred from-work",
            "R left minecraft late",
        ] {
            assert!(
                received.lines().any(|out| out == line),
                "{line}: {received}"
            );
        }
        assert!(state.jars.iter().all(|jar| jar.failures.is_empty()));
    }

    /// A NeoForge jar's loading stages run in the content pass, after every jar's constructors:
    /// its lifecycle listeners and deferred work run (the jar's own two failures, a listener and a
    /// deferred task that throw, make it `ran-partial`), and a game-bus listener its setup added is
    /// one the game holds (`runtime::neoforge_lifecycle`).
    #[test]
    fn the_content_pass_runs_neoforges_loading_stages() {
        let bytes = runtime::neoforge_lifecycle::tests::fixture_jar_bytes().to_vec();
        let mut state = State::default();
        state.ensure_loaded_with(|| vec![("life.jar".to_owned(), bytes)], &[], None);
        let jar = &mut state.jars[0];
        let printed = jar.host.take_stdout();
        for line in [
            "common",
            "common-work-supply",
            "load-complete-work",
            "payloads",
        ] {
            assert!(printed.lines().any(|out| out == line), "{line}: {printed}");
        }
        assert_eq!(jar.row.verdict, "ran-partial");
        assert_eq!(jar.failures.len(), 2, "{:?}", jar.failures);
        assert_eq!(
            jar.listeners
                .iter()
                .map(|listener| listener.event)
                .collect::<Vec<_>>(),
            [runtime::events::GameEvent::RegisterCommands]
        );
    }

    /// The game's fence (`sunrise-mod-api/src/parts/owned_namespaces.rs`) lets a native mod
    /// declare only in its id and its manifest `namespaces`; every other id is a fatal refusal at
    /// start, so the bridge must filter them before they reach the dispatcher.
    #[test]
    fn only_manifest_owned_namespaces_are_declared() {
        let owned = ["horizon_java".to_owned(), "m3_fixture".to_owned()];
        assert!(owns(&owned, "horizon_java:thing"));
        assert!(owns(&owned, "m3_fixture:ruby"));
        assert!(!owns(&owned, "other_mod:ruby"));
        assert!(!owns(&owned, "minecraft:stone"));
        assert!(!owns(&owned, "bare"));
    }

    /// A block declaration never invents an item: the runtime's inferred `item` flag only says the
    /// block was built from `BlockBehaviour.Properties`, while vanilla and mods register block
    /// items one by one in the item registry.
    #[test]
    fn a_block_declaration_brings_no_item_of_its_own() {
        let declaration = runtime::content::Block {
            id: "horizon_java:proof_block".to_owned(),
            template: "minecraft:stone".to_owned(),
            // What the runtime infers for any block built from a property chain.
            item: true,
            hardness: Some(2.0),
            explosion_resistance: Some(6.0),
            friction: None,
            requires_tool: Some(true),
            state_properties: BTreeMap::new(),
            default_state: BTreeMap::new(),
            occludes: None,
            shapes: None,
            collision_shapes: None,
        };
        let block = block_entry(&declaration, &declaration.id);
        assert!(!block.item);
        assert!(
            block.components.is_empty(),
            "an item's component patch has no item to belong to"
        );
        assert_eq!(block.template, "minecraft:stone");
        assert_eq!(block.properties.hardness, Some(2.0));
        assert_eq!(block.properties.explosion_resistance, Some(6.0));
        assert_eq!(block.properties.requires_tool, Some(true));
        assert!(block.shapes.is_none());
        assert!(block.collision_shapes.is_none());
    }

    #[test]
    fn a_block_declaration_carries_evaluated_shapes_into_mod_api() {
        let declaration = runtime::content::Block {
            id: "horizon_java:shaped_block".to_owned(),
            template: "minecraft:stone".to_owned(),
            item: false,
            hardness: None,
            explosion_resistance: None,
            friction: None,
            requires_tool: None,
            state_properties: BTreeMap::from([(
                "facing".to_owned(),
                runtime::content::StateProperty {
                    kind: "enum".to_owned(),
                    values: vec!["north".into(), "south".into(), "west".into(), "east".into()],
                },
            )]),
            default_state: BTreeMap::from([("facing".to_owned(), "north".to_owned())]),
            occludes: None,
            shapes: Some(runtime::content::BlockShapes {
                properties: vec!["facing".to_owned()],
                values: BTreeMap::from([
                    (
                        "east".to_owned(),
                        vec![[0.625, 0.0, 0.125, 1.0, 0.75, 0.875]],
                    ),
                    (
                        "north".to_owned(),
                        vec![[0.125, 0.0, 0.0, 0.875, 0.75, 0.375]],
                    ),
                    (
                        "south".to_owned(),
                        vec![[0.125, 0.0, 0.625, 0.875, 0.75, 1.0]],
                    ),
                    (
                        "west".to_owned(),
                        vec![[0.0, 0.0, 0.125, 0.375, 0.75, 0.875]],
                    ),
                ]),
                default: None,
            }),
            collision_shapes: Some(runtime::content::BlockShapes {
                properties: Vec::new(),
                values: BTreeMap::new(),
                default: Some(Vec::new()),
            }),
        };
        let block = block_entry(&declaration, &declaration.id);
        let shapes = block.shapes.expect("shapes mapped to mod_api::BlockShapes");
        assert_eq!(shapes.properties, vec!["facing".to_owned()]);
        assert_eq!(shapes.default, None);
        assert_eq!(
            shapes.values.get("north"),
            Some(&vec![[0.125, 0.0, 0.0, 0.875, 0.75, 0.375]])
        );
        assert_eq!(shapes.values.len(), 4);
        let collision = block
            .collision_shapes
            .expect("collision_shapes mapped to mod_api::BlockShapes");
        assert_eq!(collision.default, Some(Vec::new()));
    }

    /// A mod's own `BlockItem` is an item registration and is declared with its properties even
    /// when a block shares the id; nothing is dropped as a "duplicate" of an invented item.
    #[test]
    fn an_item_declaration_is_kept_even_when_a_block_shares_its_id() {
        let declaration = runtime::content::Item {
            id: "horizon_java:proof_block".to_owned(),
            template: "minecraft:stick".to_owned(),
            max_stack_size: Some(1),
            rarity: Some(runtime::content::Rarity::Rare),
            crafting_remainder: None,
            components: Default::default(),
        };
        let item = item_entry(&declaration, &declaration.id, true);
        assert_eq!(item.id, "horizon_java:proof_block");
        assert_eq!(item.template, "minecraft:stick");
        assert_eq!(item.max_stack_size, Some(1));
        assert_eq!(item.rarity, Some(mod_api::Rarity::Rare));
        assert!(
            item.components
                .iter()
                .any(|(name, value)| name == "minecraft:item_model" && value.is_some())
        );
        // A content package of the namespace supplies the model: no template model is pinned.
        let packaged = item_entry(&declaration, &declaration.id, false);
        assert!(packaged.components.is_empty());
    }

    /// A jar's own namespace is not in the manifest, so its declarations are skipped and counted
    /// per namespace; a bare or malformed id is not a declaration this bridge counts either.
    #[test]
    fn unowned_declarations_are_counted_per_namespace() {
        let owned = ["horizon_java".to_owned()];
        let counts = unowned_counts(
            &owned,
            [
                ("m3_fixture:ruby", false),
                ("m3_fixture:ruby_block", true),
                ("m3_fixture:ruby_ore", true),
                ("horizon_java:own", true),
                ("minecraft:stone", true),
                ("bare", false),
            ]
            .into_iter(),
        );
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("m3_fixture"), Some(&(2, 1)));
    }
}
