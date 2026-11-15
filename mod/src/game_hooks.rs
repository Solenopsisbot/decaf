//! Game-class Mixin callbacks at the moments the pinned mod API names (#156 P16e2).
//!
//! A jar's `@Inject` into a Minecraft class cannot be woven: the class is the game's, not the
//! jar's. The runtime keeps such a callback when a game hook site stands for its target method
//! and anchor (`register_game_hooks`), and this module runs it where the game fires a
//! [`mod_api::Hooks`] hook at **exactly** that moment, with the receiver and arguments the target
//! method has, built from what the hook passes (`GameObject`, keyed as the event tier keys the
//! same object: a callback's `this` is that object).
//!
//! A site is bound only where the hook fires at the site's own moment and passes what the
//! callback needs ([`BOUND`]); every other site stays unbound and is listed with why
//! ([`UNBOUND`]), never run at a nearby moment. A Mixin `cancel()` reaches the game only where
//! the hook's cancel does what the Mixin's would; elsewhere a jar with a cancellable callback at
//! the site is not run there.
//!
//! What ran, failed or was refused is counted per jar (`JarRow::game_hooks`), for the report and
//! the in-game survey.

use std::collections::BTreeMap;

use crate::runtime::events::FacadePlayer;
use crate::runtime::{
    ExecutionBudget, GameHookContext, GameHookEffect, GameObject, MixinSide, NativeJar, Value,
};
use crate::{JAR_BUDGET, LOG, LoadedJar, State};

/// The sites this mod runs, and the mod API hook that fires at each one's moment.
pub(crate) const BOUND: &[(&str, &str)] = &[
    (
        "player_logged_in",
        "player_logged_in: PlayerList.placeNewPlayer TAIL",
    ),
    (
        "player_clone",
        "player_clone: ServerPlayer.restoreFrom TAIL",
    ),
    ("player_died", "player_died: ServerPlayer.die HEAD"),
    (
        "living_death",
        "living_death: LivingEntity.die HEAD (mobs; listen only)",
    ),
    (
        "server_stopping",
        "server_stopping: MinecraftServer.stopServer HEAD",
    ),
    (
        "java_server_stopped",
        "server_stopped: MinecraftServer.stopServer TAIL",
    ),
    (
        "java_server_tick_head",
        "server_tick ServerTickStart: tickServer HEAD (single player only: a dedicated server's empty pause runs HEAD every tick, Horizon fires nothing then)",
    ),
    (
        "java_server_tick_tail",
        "server_tick ServerTickEnd: tickServer TAIL (a RETURN callback, single player only: vanilla's empty pause returns early)",
    ),
    (
        "java_base_player_tick_head",
        "player_tick_pre: NeoForge PlayerTickEvent.Pre at Player.tick HEAD; server-side player state is acting (client-side Mixin plans are counted as server-only)",
    ),
    (
        "java_base_player_tick_tail",
        "player_tick_post: NeoForge PlayerTickEvent.Post at Player.tick TAIL; server-side player state is acting (client-side Mixin plans are counted as server-only)",
    ),
    ("attack_entity", "attack_entity: Player.attack HEAD"),
];

/// The bound sites whose hook carries a Mixin `cancel()`: `Player.attack` HEAD's cancel is the
/// hook's (the blow does nothing). The runtime refuses a cancellable `@Inject` at a listen-only
/// hook point when it registers it (counted `refused`), so a listen site has no cancellable
/// callback to run; at `living_death` (a cancel hook) the hook's cancel keeps the mob alive
/// while a Mixin's skips only `die`'s body, so a jar with a cancellable callback there is
/// skipped and counted, never run with its `cancel()` dropped or mistranslated.
pub(crate) const CANCELS: &[&str] = &["attack_entity"];

/// The sites the runtime knows that no pinned hook fires at exactly, and why.
pub(crate) const UNBOUND: &[(&str, &str)] = &[
    (
        "level_unload",
        "Horizon closes an idle level mid-game (vanilla closes levels only in stopServer), and a level it closed earlier gets no close at stop",
    ),
    (
        "java_level_tick_head",
        "Horizon ticks only levels with players or forced chunks; vanilla's tickChildren ticks every level each tick",
    ),
    (
        "java_level_tick_tail",
        "Horizon ticks only levels with players or forced chunks; vanilla's tickChildren ticks every level each tick",
    ),
    (
        "player_start_tracking",
        "the hook is a deferred batch after the tick (the entity may be gone), not addPairing's TAIL",
    ),
    (
        "player_tick",
        "no hook at ServerPlayer.tick's moment; Player.tick's pair is a different moment (a native Mixin escape hatch would be needed)",
    ),
    (
        "java_player_tick_head",
        "no hook at ServerPlayer.tick's moment; Player.tick's pair is a different moment (a native Mixin escape hatch would be needed)",
    ),
    (
        "java_player_list_remove",
        "player_logged_out fires after the player left the list; PlayerList.remove HEAD is before",
    ),
    (
        "java_living_hurt",
        "living_damage_pre is actuallyHurt after armor, not hurtServer HEAD",
    ),
    (
        "java_living_died",
        "living_death_post is at die's broadcastEntityEvent, not its TAIL",
    ),
    (
        "java_explosion_explode",
        "explosion_detonate is after toBlow is computed, not explode HEAD",
    ),
    (
        "java_destroy_speed",
        "player_break_speed is not getDestroySpeed's RETURN",
    ),
    (
        "java_drop_death_loot_head",
        "living_drops is after the drops are collected, not dropAllDeathLoot HEAD",
    ),
    (
        "java_drop_death_loot_tail",
        "no hook at dropAllDeathLoot TAIL",
    ),
    (
        "java_block_player_destroy",
        "no hook at Block.playerDestroy HEAD",
    ),
    (
        "java_block_player_destroyed",
        "player_destroyed_block is Block.destroy, not playerDestroy TAIL",
    ),
    (
        "java_grow_crop",
        "crop_grow is the random-tick growth, not BoneMealItem.growCrop",
    ),
    (
        "java_server_constructed",
        "no hook at the server's constructor",
    ),
    (
        "java_dedicated_server_constructed",
        "no hook at the server's constructor",
    ),
    ("java_bootstrap_head", "no hook at Bootstrap.bootStrap"),
    ("java_bootstrap_tail", "no hook at Bootstrap.bootStrap"),
    (
        "item_picked_up",
        "the hook names no item entity id, the callback's `this`",
    ),
    (
        "living_jump",
        "one hook for LivingEntity and MagmaCube targets, and no Host in its loop",
    ),
    ("living_knockback", "no living_knockback hook at the pin"),
    ("entity_mount", "no entity_mount hook at the pin"),
    ("item_damaged", "no item_damaged hook at the pin"),
    (
        "left_click_block",
        "the hook is after the reach and protection checks, START only, not HEAD",
    ),
    (
        "player_container_open",
        "the hook is heard once a tick, not at openHorseInventory's TAIL",
    ),
    (
        "living_entity_use_item_stop",
        "the hook is after the use check, not releaseUsingItem HEAD",
    ),
    (
        "living_entity_use_item_tick",
        "its cancel finishes the use; a Mixin cancel at HEAD skips the tick",
    ),
];

/// The bound and unbound sites, for the report: which game-class Mixin targets run here, and why
/// the others do not.
pub(crate) fn sites_json() -> serde_json::Value {
    serde_json::json!({
        "bound": BOUND.iter().map(|(site, moment)| serde_json::json!({ "site": site, "moment": moment })).collect::<Vec<_>>(),
        "unbound": UNBOUND.iter().map(|(site, why)| serde_json::json!({ "site": site, "why": why })).collect::<Vec<_>>(),
    })
}

/// What one jar's game-class callbacks did, for the report and the survey.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tally {
    /// Callbacks registered at a bound site, and at a site this mod does not run.
    pub bound: usize,
    pub unbound: usize,
    /// Callbacks that ran (callbacks, not firings), firings that failed (an error names the call
    /// the model refused), and firings skipped because this run cannot give a jar's callbacks
    /// their exact moment: a cancellable callback whose `cancel()` would mean something else
    /// here, or a server-tick callback on a dedicated server (`exact_here`).
    pub ran: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Firings not run because a player they hand over is not on the bridge's list with an
    /// entity id and that life: the object would not be the event tier's (#246's review).
    pub unlisted: usize,
    /// Game-class `@Inject` plans a hook point refused when they registered (a cancellable
    /// callback at a listen-only point, an anchor the point has no call site for): they never
    /// run.
    pub refused: usize,
    /// Failure reasons, first line, with counts (at most 16 kept).
    pub failures: BTreeMap<String, usize>,
}

impl Tally {
    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "bound": self.bound,
            "unbound": self.unbound,
            "ran": self.ran,
            "failed": self.failed,
            "skipped": self.skipped,
            "unlisted": self.unlisted,
            "refused": self.refused,
            "failures": self.failures,
        })
    }
}

/// The game-hook registrations of one jar: its `@Inject`s a game hook site stands for.
pub(crate) fn register(
    jar: &NativeJar,
    tally: &mut Tally,
) -> crate::runtime::GameHookRegistrations {
    let weave = jar.plan_load_time_weave_with_game_hooks(MixinSide::Server);
    tally.refused += weave
        .skipped
        .iter()
        .filter(|plan| {
            matches!(
                plan.reason,
                crate::runtime::MixinSkipReason::GameHookMismatch(_)
            ) || matches!(plan.reason, crate::runtime::MixinSkipReason::Unresolved)
                && crate::runtime::every_game_hook_site().any(|site| {
                    site.target_owner == plan.target.owner
                        && site.target_name == plan.target.name
                        && (plan.target.descriptor.is_empty()
                            || site.target_descriptor == plan.target.descriptor)
                })
        })
        .count();
    let registrations = jar.register_game_hooks(&weave).unwrap_or_default();
    for hook in registrations.hooks() {
        let count = registrations.entries(hook).len();
        if BOUND.iter().any(|(site, _)| *site == hook) {
            tally.bound += count;
        } else {
            tally.unbound += count;
        }
    }
    registrations
}

/// One argument of a target method: a game object, or a value (a `boolean`).
pub(crate) enum Arg {
    Object(GameObject),
    Value(Value),
}

/// A player as the bridge counts them: their session, life and entity id (`State::players`),
/// so the handle is the event tier's object for them. `None` when the player is not listed, has
/// no entity id, or has no life `life_back` lives ago: such an object would be a second key
/// for the same `ServerPlayer`, so the firing is not run (`Tally::unlisted`).
pub(crate) fn player(state: &State, uuid: u128, life_back: u32) -> Option<GameObject> {
    player_of(state.players.iter().find(|p| p.uuid == uuid)?, life_back)
}

/// [`player`] from the bridge's entry for them (a leaver's last entry too).
pub(crate) fn player_of(listed: &FacadePlayer, life_back: u32) -> Option<GameObject> {
    let uuid = listed.uuid;
    let life = listed.life.checked_sub(life_back)?;
    let mut object = GameObject::player(uuid, &listed.name)
        .with_incarnation(listed.session, life)
        .with_entity_id(listed.entity_id?);
    if let Some(level) = &listed.level {
        object = object.with_dimension(level);
    }
    Some(object)
}

/// `Player.attack`'s target: a player is the listed player's object, and `None` when they are
/// not listed (the firing is counted `unlisted`, never handed a second key for the same
/// `ServerPlayer`); any other entity is an `Entity` whose livingness the hook does not say, so
/// a callback's `instanceof LivingEntity` on it is counted, never answered.
pub(crate) fn attack_target(state: &State, id: i32, kind: &str) -> Option<GameObject> {
    if kind == "minecraft:player" {
        let listed = state.players.iter().find(|p| p.entity_id == Some(id))?;
        return player_of(listed, 0);
    }
    Some(GameObject::entity(id, kind, false))
}

/// Count a firing of `site` that was not run because a player it hands over is not listed
/// (`player` answered `None`), on every jar with callbacks there.
pub(crate) fn unlisted(state: &mut State, site: &str) {
    for loaded in &mut state.jars {
        if !loaded.hooks.entries(site).is_empty() {
            loaded.row.game_hooks.unlisted += 1;
            log::debug!(
                "{LOG}: {}: {site} not fired: a player it hands over is not listed",
                loaded.row.id
            );
        }
    }
}

/// Count a spectator firing while the pinned game predates ari #4520. The bridge cannot know
/// whether this particular spectator touches an unloaded chunk, so it runs the callback and names
/// the possible mismatch instead of suppressing valid callbacks.
pub(crate) fn note_spectator(state: &mut State, site: &str) {
    for loaded in &mut state.jars {
        if !loaded.hooks.entries(site).is_empty() {
            note_failure(
                &mut loaded.row,
                site,
                "fork gap: pinned game may fire PlayerTickEvent for spectator touching an unloaded chunk; ari #4520 narrows this after a pin bump",
            );
        }
    }
}

/// Drop what every jar's callbacks kept on `object` (its `@Unique` fields, its initialised
/// Mixins): a player's previous life after a respawn, a player who left. Java's object is
/// gone; a later object is a new one.
pub(crate) fn forget(state: &mut State, object: &GameObject) {
    let marker = object.marker();
    for loaded in &mut state.jars {
        loaded.hook_state.forget_object(&marker);
    }
}

/// Whether this run gives every callback in `entries` at `site` its vanilla moment. A
/// dedicated server's empty pause (`pause-when-empty-seconds`) runs `tickServer`'s HEAD and
/// its early RETURN every tick while Horizon fires no server tick then: there only TAIL is
/// exact. A single player server skips `tickServer` while paused, as Horizon does.
fn exact_here(site: &str, entries: &[crate::runtime::RegisteredGameHook], dedicated: bool) -> bool {
    match site {
        "java_server_tick_head" => !dedicated,
        "java_server_tick_tail" => !dedicated || entries.iter().all(|entry| entry.anchor == "TAIL"),
        _ => true,
    }
}

/// What a firing of one site did across the jars.
#[derive(Default)]
pub(crate) struct Fired {
    pub cancelled: bool,
    pub effects: Vec<GameHookEffect>,
}

/// Run every jar's callbacks at `site`, against `receiver` and `args` (the target method's).
/// Where the hook's cancel is not the Mixin's ([`CANCELS`]), a jar with a cancellable callback
/// at the site is skipped, so no `cancel()` is dropped or mistranslated.
/// Once a jar's callback cancels, the target method returns: no later jar's callback at that
/// point runs (jars run in load order; Mixin orders them by priority across jars, a departure).
pub(crate) fn fire(
    state: &mut State,
    site: &str,
    receiver: Option<GameObject>,
    args: Vec<Arg>,
) -> Fired {
    let mut fired = Fired::default();
    let cancels = CANCELS.contains(&site);
    let facts = std::sync::Arc::clone(&state.server);
    let dedicated = state.dedicated;
    for loaded in &mut state.jars {
        let LoadedJar {
            jar,
            host,
            hooks,
            hook_state,
            row,
            ..
        } = loaded;
        let entries = hooks.entries(site);
        if entries.is_empty() {
            continue;
        }
        if (!cancels && entries.iter().any(|entry| entry.cancellable))
            || !exact_here(site, entries, dedicated)
        {
            row.game_hooks.skipped += 1;
            continue;
        }
        let mut context = GameHookContext::new();
        context.set_server_facts(std::sync::Arc::clone(&facts));
        context.insert(GameObject::server(dedicated));
        let receiver = receiver.clone().map(|object| context.insert(object));
        let values: Vec<Value> = args
            .iter()
            .map(|arg| match arg {
                Arg::Object(object) => context.insert(object.clone()),
                Arg::Value(value) => value.clone(),
            })
            .collect();
        let budget = ExecutionBudget::within(JAR_BUDGET);
        let outcome = crate::caught(&format!("{} game hook {site}", row.id), || {
            jar.run_game_hook_in(
                hooks,
                hook_state,
                &mut context,
                site,
                receiver,
                &values,
                host,
                &budget,
            )
        });
        match outcome {
            Ok(Ok(run)) => {
                row.game_hooks.ran += run.ran;
                fired.cancelled |= run.cancelled;
                if site == "java_base_player_tick_tail" {
                    note_late_player_tick_post(row);
                }
            }
            Ok(Err(error)) => note_failure(row, site, &error.to_string()),
            Err(panic) => note_failure(row, site, &format!("panicked: {panic}")),
        }
        fired.effects.extend(context.take_effects());
        if fired.cancelled {
            break;
        }
    }
    fired
}

/// Drop every object's Mixin state: the server stopped, so its levels, player list, players,
/// mobs and connections are gone, and the next world's are new Java objects (single player
/// keeps the jars across worlds).
pub(crate) fn forget_world(state: &mut State) {
    for loaded in &mut state.jars {
        loaded.hook_state = crate::runtime::GameHookState::default();
    }
}

/// A `DamageSource` of its own for one firing: Java makes one per hurt, never shared (#156
/// P16b), so two deaths by `lava` are two objects.
pub(crate) fn damage_source(state: &mut State, cause: &str) -> GameObject {
    state.damage_sources += 1;
    GameObject::opaque(
        "net/minecraft/world/damagesource/DamageSource",
        &format!("{cause}#{}", state.damage_sources),
    )
}

fn note_failure(row: &mut crate::JarRow, site: &str, reason: &str) {
    row.game_hooks.failed += 1;
    let line = format!("{site}: {}", reason.lines().next().unwrap_or_default());
    log::warn!("{LOG}: {}: game hook callback failed: {line}", row.id);
    let failures = &mut row.game_hooks.failures;
    if failures.len() < 16 || failures.contains_key(&line) {
        *failures.entry(line).or_default() += 1;
    }
}

fn note_late_player_tick_post(row: &mut crate::JarRow) {
    note_failure(
        row,
        "java_base_player_tick_tail",
        "fork gap: player_tick_post fires after ServerPlayer.doTick's pickups, advancements, scores and death handling; vanilla Player.tick TAIL is before that work",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::{
        Backend, EntityRef, Hooks, Host, HostSide, PlayerRef, PlayerTickPostEvent,
        PlayerTickPreEvent, ServerHost, WorldEpoch,
    };
    use std::collections::BTreeMap;

    struct PlayerTickHost {
        spectator: BTreeMap<u128, Option<bool>>,
    }

    impl Host for PlayerTickHost {
        fn backend(&self) -> Option<mod_api::BackendRef<'_>> {
            Some(mod_api::__private::backend_ref(self))
        }
    }
    mod_api::impl_host_areas!(PlayerTickHost);
    impl ServerHost for PlayerTickHost {}
    mod_api::impl_server_host_areas!(PlayerTickHost);
    impl mod_api::prelude::PlayersBackend for PlayerTickHost {
        fn player_is_spectator(&self, player: PlayerRef) -> Option<bool> {
            self.spectator.get(&player.uuid()).copied().flatten()
        }
    }
    impl mod_api::prelude::PlayersClientBackend for PlayerTickHost {}
    mod_api::impl_backend_areas!(PlayerTickHost, except(players));
    impl Backend for PlayerTickHost {}
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// Compile-only stubs, and the jar's own classes: a Fabric entrypoint and a `ServerPlayer.die`
    /// HEAD Mixin that keeps a field on the player and prints its UUID.
    const SOURCES: &[(&str, &str)] = &[
        (
            "org/spongepowered/asm/mixin/Mixin.java",
            "package org.spongepowered.asm.mixin; import java.lang.annotation.*; \
             @Retention(RetentionPolicy.RUNTIME) @Target(ElementType.TYPE) \
             public @interface Mixin { Class<?>[] value() default {}; }",
        ),
        (
            "org/spongepowered/asm/mixin/injection/At.java",
            "package org.spongepowered.asm.mixin.injection; import java.lang.annotation.*; \
             @Retention(RetentionPolicy.RUNTIME) @Target({}) public @interface At { String value() default \"\"; }",
        ),
        (
            "org/spongepowered/asm/mixin/injection/Inject.java",
            "package org.spongepowered.asm.mixin.injection; import java.lang.annotation.*; \
             @Retention(RetentionPolicy.RUNTIME) @Target(ElementType.METHOD) \
             public @interface Inject { String[] method(); At[] at(); boolean cancellable() default false; \
             int require() default 0; int expect() default 0; String locals() default \"NO_CAPTURE\"; }",
        ),
        (
            "org/spongepowered/asm/mixin/injection/callback/CallbackInfo.java",
            "package org.spongepowered.asm.mixin.injection.callback; public class CallbackInfo { \
             public void cancel() {} }",
        ),
        (
            "net/minecraft/world/damagesource/DamageSource.java",
            "package net.minecraft.world.damagesource; public class DamageSource {}",
        ),
        (
            "net/minecraft/server/level/ServerPlayer.java",
            "package net.minecraft.server.level; public class ServerPlayer { \
             public java.util.UUID getUUID() { return null; } }",
        ),
        (
            "demo/Main.java",
            "package demo; public final class Main { public void onInitialize() {} }",
        ),
        (
            "demo/DeathMixin.java",
            "package demo; \
             import net.minecraft.server.level.ServerPlayer; \
             import net.minecraft.world.damagesource.DamageSource; \
             import org.spongepowered.asm.mixin.Mixin; \
             import org.spongepowered.asm.mixin.injection.*; \
             import org.spongepowered.asm.mixin.injection.callback.CallbackInfo; \
             @Mixin(ServerPlayer.class) public class DeathMixin { \
               private int demo$deaths; \
               @Inject(method = \"die\", at = @At(\"HEAD\")) \
               private void onDie(DamageSource source, CallbackInfo ci) { \
                 demo$deaths++; \
                 System.out.println(((ServerPlayer) (Object) this).getUUID() + \" \" + demo$deaths); \
               } }",
        ),
    ];

    fn jar_bytes() -> Vec<u8> {
        jar_bytes_with("deathdemo", &[], &["DeathMixin"])
    }

    /// A Fabric jar `id` holding `demo.Main` and the Mixins `mixins`, compiled from
    /// [`SOURCES`] and `extra` (compile-only stubs outside `demo/` are left out).
    fn jar_bytes_with(id: &str, extra: &[(&str, &str)], mixins: &[&str]) -> Vec<u8> {
        let root = std::env::temp_dir().join(format!(
            "horizon-java-mod-hooks-{id}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let (src, classes) = (root.join("src"), root.join("classes"));
        let mut paths = Vec::new();
        for (path, body) in SOURCES.iter().chain(extra) {
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
        let files = [
            (
                "fabric.mod.json".to_owned(),
                format!(
                    r#"{{"id":"{id}","version":"1","entrypoints":{{"main":["demo.Main"]}},"mixins":["{id}.mixins.json"]}}"#
                ),
            ),
            (
                format!("{id}.mixins.json"),
                serde_json::json!({ "package": "demo", "mixins": mixins }).to_string(),
            ),
        ];
        for (path, body) in files {
            zip.start_file(path, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        for name in std::iter::once(&"Main").chain(mixins) {
            let path: &Path = &classes.join(format!("demo/{name}.class"));
            zip.start_file(format!("demo/{name}.class"), options)
                .unwrap();
            zip.write_all(&std::fs::read(path).unwrap()).unwrap();
        }
        zip.finish().unwrap();
        let _ = std::fs::remove_dir_all(&root);
        bytes
    }

    /// The mod runs a jar's `ServerPlayer.die` HEAD Mixin where `player_died` fires: `this` is
    /// the player (its UUID), the field the Mixin adds stays on that player across deaths of one
    /// life, and starts again on the new `ServerPlayer` a respawn builds.
    #[test]
    fn a_bound_mixin_callback_runs_on_the_player_at_its_hook() {
        let jar = Arc::new(NativeJar::from_bytes(&jar_bytes()).unwrap());
        let imc = crate::runtime::neoforge_imc::ImcBus::default();
        let Ok(loaded) = crate::load_jar("deathdemo.jar".into(), jar, &[], &imc, None) else {
            panic!("the jar did not load")
        };
        assert_eq!(
            (loaded.row.game_hooks.bound, loaded.row.game_hooks.unbound),
            (1, 0)
        );
        let uuid = 0x0123_4567_89ab_cdef_0011_2233_4455_6677_u128;
        let mut state = State::default();
        state.jars.push(loaded);
        let die = |state: &mut State, life: u32| {
            state.players = vec![FacadePlayer {
                uuid,
                name: "Alex".into(),
                level: Some("minecraft:overworld".into()),
                entity_id: Some(7),
                session: 1,
                life,
            }];
            let player = player(state, uuid, 0).expect("listed with an id and a life");
            let source =
                GameObject::opaque("net/minecraft/world/damagesource/DamageSource", "lava");
            fire(
                state,
                "player_died",
                Some(player),
                vec![Arg::Object(source)],
            );
        };
        die(&mut state, 0);
        die(&mut state, 0);
        die(&mut state, 1);
        // A forgotten object (a previous life, a player who left) keeps nothing: the same key
        // would be a new `ServerPlayer`, whose field starts again.
        die(&mut state, 1);
        let gone = player(&state, uuid, 0).unwrap();
        forget(&mut state, &gone);
        die(&mut state, 1);
        let loaded = &mut state.jars[0];
        assert_eq!(
            (loaded.row.game_hooks.ran, loaded.row.game_hooks.failed),
            (5, 0)
        );
        assert_eq!(
            loaded.host.take_stdout().lines().collect::<Vec<_>>(),
            [
                "01234567-89ab-cdef-0011-223344556677 1",
                "01234567-89ab-cdef-0011-223344556677 2",
                "01234567-89ab-cdef-0011-223344556677 1",
                "01234567-89ab-cdef-0011-223344556677 2",
                "01234567-89ab-cdef-0011-223344556677 1",
            ]
        );

        // A player with no entity id, or no life that far back, is no object the event tier
        // has: no firing, counted on the jar with callbacks at the site.
        state.players[0].entity_id = None;
        assert!(player(&state, uuid, 0).is_none());
        state.players[0].entity_id = Some(7);
        assert!(player(&state, uuid, 2).is_none());
        assert!(player(&state, 0xff, 0).is_none());
        unlisted(&mut state, "player_died");
        unlisted(&mut state, "player_tick");
        assert_eq!(state.jars[0].row.game_hooks.unlisted, 1);
    }

    /// Every site the runtime knows is either bound to a pinned hook at its moment or listed as
    /// unbound with why, and never both.
    #[test]
    fn every_site_is_bound_or_listed_unbound() {
        fn named(list: &'static [(&'static str, &'static str)]) -> Vec<&'static str> {
            list.iter().map(|(site, _)| *site).collect()
        }
        let (bound, unbound) = (named(BOUND), named(UNBOUND));
        for site in crate::runtime::every_game_hook_site() {
            let listed = bound.contains(&site.hook) as u8 + unbound.contains(&site.hook) as u8;
            assert_eq!(listed, 1, "{} is listed {listed} times", site.hook);
        }
        for site in bound.iter().chain(&unbound) {
            assert!(
                crate::runtime::every_game_hook_site().any(|known| known.hook == *site),
                "{site} is no runtime site"
            );
        }
    }

    /// Game classes for the attack and server-tick Mixins below, compile-only.
    const GAME: &[(&str, &str)] = &[
        (
            "net/minecraft/world/entity/Entity.java",
            "package net.minecraft.world.entity; public class Entity { public int getId() { return 0; } }",
        ),
        (
            "net/minecraft/world/entity/LivingEntity.java",
            "package net.minecraft.world.entity; public class LivingEntity extends Entity {}",
        ),
        (
            "net/minecraft/world/entity/player/Player.java",
            "package net.minecraft.world.entity.player; \
             public class Player extends net.minecraft.world.entity.LivingEntity { \
             public void attack(net.minecraft.world.entity.Entity target) {} \
             public void tick() {} public java.util.UUID getUUID() { return null; } }",
        ),
        (
            "net/minecraft/server/MinecraftServer.java",
            "package net.minecraft.server; public class MinecraftServer { \
             public void tickServer(java.util.function.BooleanSupplier hasTime) {} }",
        ),
    ];

    const MIXIN_IMPORTS: &str = "package demo; \
        import net.minecraft.world.entity.*; \
        import net.minecraft.world.entity.player.Player; \
        import net.minecraft.server.MinecraftServer; \
        import org.spongepowered.asm.mixin.Mixin; \
        import org.spongepowered.asm.mixin.injection.*; \
        import org.spongepowered.asm.mixin.injection.callback.CallbackInfo; ";

    fn load(id: &str, mixin: &str, body: &str) -> LoadedJar {
        let path = format!("demo/{mixin}.java");
        let source = format!("{MIXIN_IMPORTS}{body}");
        let extra: Vec<(&str, &str)> = GAME
            .iter()
            .copied()
            .chain([(path.as_str(), source.as_str())])
            .collect();
        let jar = Arc::new(NativeJar::from_bytes(&jar_bytes_with(id, &extra, &[mixin])).unwrap());
        let imc = crate::runtime::neoforge_imc::ImcBus::default();
        let Ok(loaded) = crate::load_jar(format!("{id}.jar"), jar, &[], &imc, None) else {
            panic!("{id} did not load")
        };
        loaded
    }

    fn stdout(state: &mut State, jar: usize) -> Vec<String> {
        state.jars[jar]
            .host
            .take_stdout()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn player_tick_head_and_tail_run_once_per_player_in_order() {
        let java = crate::HorizonJava::default();
        java.lock().jars.push(load(
            "playertick",
            "PlayerTickMixin",
            "@Mixin(Player.class) public class PlayerTickMixin { \
               @Inject(method = \"tick\", at = @At(\"HEAD\")) \
               private void pre(CallbackInfo ci) { System.out.println(\"pre/\" + ((Player)(Object)this).getUUID()); } \
               @Inject(method = \"tick\", at = @At(\"TAIL\")) \
               private void post(CallbackInfo ci) { System.out.println(\"post/\" + ((Player)(Object)this).getUUID()); } }",
        ));
        let alice = 0xa1_u128;
        let bob = 0xb2_u128;
        java.lock().players = vec![
            FacadePlayer {
                uuid: alice,
                name: "Alice".into(),
                level: None,
                entity_id: Some(7),
                session: 1,
                life: 0,
            },
            FacadePlayer {
                uuid: bob,
                name: "Bob".into(),
                level: None,
                entity_id: Some(8),
                session: 1,
                life: 0,
            },
        ];
        let mut host = PlayerTickHost {
            spectator: BTreeMap::from([(alice, Some(false)), (bob, Some(false))]),
        };
        for (uuid, entity) in [(alice, 7), (bob, 8)] {
            let player = mod_api::__private::game::player_ref(
                EntityRef::new(WorldEpoch::new(1), entity),
                uuid,
            );
            crate::HorizonJava::player_tick_pre(
                &java,
                HostSide::Server(&mut host),
                &mut PlayerTickPreEvent { player },
            );
            crate::HorizonJava::player_tick_post(
                &java,
                HostSide::Server(&mut host),
                &mut PlayerTickPostEvent { player },
            );
        }
        let mut state = java.lock();
        assert_eq!(
            stdout(&mut state, 0),
            [
                "pre/00000000-0000-0000-0000-0000000000a1",
                "post/00000000-0000-0000-0000-0000000000a1",
                "pre/00000000-0000-0000-0000-0000000000b2",
                "post/00000000-0000-0000-0000-0000000000b2",
            ]
        );
        assert_eq!(state.jars[0].row.game_hooks.ran, 4);
        assert_eq!(state.jars[0].row.game_hooks.failed, 2);
    }

    #[test]
    fn player_tick_cancellable_injection_is_refused_and_counted() {
        let mut state = State::default();
        state.jars.push(load(
            "cancelplayertick",
            "CancelPlayerTickMixin",
            "@Mixin(Player.class) public class CancelPlayerTickMixin { \
               @Inject(method = \"tick\", at = @At(\"HEAD\"), cancellable = true) \
               private void pre(CallbackInfo ci) { ci.cancel(); } }",
        ));
        assert_eq!(state.jars[0].row.game_hooks.refused, 1);
        assert_eq!(state.jars[0].row.game_hooks.bound, 0);
        assert!(
            state.jars[0]
                .hooks
                .entries("java_base_player_tick_head")
                .is_empty()
        );
    }

    #[test]
    fn player_tick_injection_metadata_is_refused_and_counted() {
        let mut state = State::default();
        state.jars.push(load(
            "metadata_playertick",
            "MetadataPlayerTickMixin",
            "@Mixin(Player.class) public class MetadataPlayerTickMixin { \
               @Inject(method = \"tick\", at = @At(\"HEAD\"), require = 2, expect = 1, locals = \"CAPTURE_FAILHARD\") \
               private void pre(CallbackInfo ci) {} }",
        ));
        assert_eq!(state.jars[0].row.game_hooks.refused, 1);
        assert_eq!(state.jars[0].row.game_hooks.bound, 0);
    }

    #[test]
    fn player_tick_spectator_statuses_are_counted_through_hook_dispatch() {
        let mut state = State::default();
        state.jars.push(load(
            "spectatortick",
            "SpectatorTickMixin",
            "@Mixin(Player.class) public class SpectatorTickMixin { \
               @Inject(method = \"tick\", at = @At(\"HEAD\")) \
               private void pre(CallbackInfo ci) { System.out.println(\"pre/\" + ((Player)(Object)this).getUUID()); } \
               @Inject(method = \"tick\", at = @At(\"TAIL\")) \
               private void post(CallbackInfo ci) { System.out.println(\"post/\" + ((Player)(Object)this).getUUID()); } }",
        ));
        let java = crate::HorizonJava::default();
        java.lock().jars.push(state.jars.remove(0));
        let players = [
            (0xa1_u128, 7_u32, "Spectator"),
            (0xb2, 8_u32, "Survival"),
            (0xc3, 9_u32, "Unavailable"),
        ];
        java.lock().players = players
            .iter()
            .map(|&(uuid, entity_id, name)| FacadePlayer {
                uuid,
                name: name.into(),
                level: None,
                entity_id: Some(entity_id as i32),
                session: 1,
                life: 0,
            })
            .collect();
        let mut host = PlayerTickHost {
            spectator: BTreeMap::from([(0xa1, Some(true)), (0xb2, Some(false)), (0xc3, None)]),
        };
        for &(uuid, entity_id, _) in &players {
            let player = mod_api::__private::game::player_ref(
                EntityRef::new(WorldEpoch::new(1), entity_id),
                uuid,
            );
            crate::HorizonJava::player_tick_pre(
                &java,
                HostSide::Server(&mut host),
                &mut PlayerTickPreEvent { player },
            );
            crate::HorizonJava::player_tick_post(
                &java,
                HostSide::Server(&mut host),
                &mut PlayerTickPostEvent { player },
            );
        }
        let mut state = java.lock();
        assert_eq!(state.jars[0].row.game_hooks.skipped, 0);
        assert_eq!(state.jars[0].row.game_hooks.ran, 6);
        assert_eq!(state.jars[0].row.game_hooks.failed, 7);
        assert_eq!(
            state.jars[0]
                .row
                .game_hooks
                .failures
                .get("java_base_player_tick_head: fork gap: pinned game may fire PlayerTickEvent for spectator touching an unloaded chunk; ari #4520 narrows this after a pin bump"),
            Some(&2)
        );
        assert_eq!(
            stdout(&mut state, 0),
            [
                "pre/00000000-0000-0000-0000-0000000000a1",
                "post/00000000-0000-0000-0000-0000000000a1",
                "pre/00000000-0000-0000-0000-0000000000b2",
                "post/00000000-0000-0000-0000-0000000000b2",
                "pre/00000000-0000-0000-0000-0000000000c3",
                "post/00000000-0000-0000-0000-0000000000c3",
            ]
        );
    }

    /// `Player.attack` returns once a HEAD callback cancels: a later jar's callback at that
    /// HEAD never runs. A mob target's livingness is not in the hook, so `instanceof
    /// LivingEntity` on it is counted (Java: `true` for a zombie), and a player target that
    /// is not listed is no object at all (`unlisted`).
    #[test]
    fn a_cancel_stops_the_later_jars_and_an_unknown_target_is_counted() {
        let mut state = State::default();
        state.jars.push(load(
            "attacka",
            "CancelAttack",
            "@Mixin(Player.class) public class CancelAttack { \
               @Inject(method = \"attack\", at = @At(\"HEAD\"), cancellable = true) \
               private void onAttack(Entity target, CallbackInfo ci) { \
                 System.out.println(\"a/\" + target.getId()); \
                 if (target.getId() == 42) ci.cancel(); } }",
        ));
        state.jars.push(load(
            "attackb",
            "SeeAttack",
            "@Mixin(Player.class) public class SeeAttack { \
               @Inject(method = \"attack\", at = @At(\"HEAD\")) \
               private void onAttack(Entity target, CallbackInfo ci) { \
                 System.out.println(\"b/\" + (target instanceof LivingEntity)); } }",
        ));
        let alex = 0xa1_u128;
        state.players = vec![FacadePlayer {
            uuid: alex,
            name: "Alex".into(),
            level: None,
            entity_id: Some(7),
            session: 1,
            life: 0,
        }];
        let attacker = player(&state, alex, 0).unwrap();
        let attack = |state: &mut State, id: i32| {
            let target = attack_target(state, id, "minecraft:zombie").unwrap();
            fire(
                state,
                "attack_entity",
                Some(attacker.clone()),
                vec![Arg::Object(target)],
            )
        };
        assert!(attack(&mut state, 42).cancelled);
        assert!(!attack(&mut state, 5).cancelled);
        assert_eq!(stdout(&mut state, 0), ["a/42", "a/5"]);
        assert_eq!(stdout(&mut state, 1), ["b/false"]);
        assert_eq!(state.jars[1].row.game_hooks.ran, 1);
        let counted = state.jars[1].host.unknown_host_calls();
        assert!(
            counted.iter().any(|(call, _)| call
                == "net/minecraft/world/entity/LivingEntity#<instanceof>(Ljava/lang/Object;)Z"),
            "{counted:?}"
        );
        // A player target: the listed player's own object, or nothing when unlisted.
        assert_eq!(
            attack_target(&state, 7, "minecraft:player").map(|o| o.marker()),
            Some(attacker.marker())
        );
        assert!(attack_target(&state, 9, "minecraft:player").is_none());
    }

    /// On a dedicated server Horizon fires no server tick during the empty pause, where
    /// vanilla still runs `tickServer`'s HEAD: a HEAD callback is skipped there, a TAIL one
    /// runs. A stopped server's Mixin state goes with it.
    #[test]
    fn dedicated_tick_heads_are_skipped_and_a_stop_forgets_the_world() {
        let mut state = State::default();
        let ticks = |at: &str, class: &str| {
            format!(
                "@Mixin(MinecraftServer.class) public class {class} {{ private int demo$n; \
                   @Inject(method = \"tickServer\", at = @At(\"{at}\")) \
                   private void onTick(java.util.function.BooleanSupplier hasTime, CallbackInfo ci) {{ \
                     demo$n++; System.out.println(\"{at}/\" + demo$n); }} }}"
            )
        };
        state
            .jars
            .push(load("tickhead", "TickHead", &ticks("HEAD", "TickHead")));
        state
            .jars
            .push(load("ticktail", "TickTail", &ticks("TAIL", "TickTail")));
        let tick = |state: &mut State| {
            let server = GameObject::server(state.dedicated);
            for site in ["java_server_tick_head", "java_server_tick_tail"] {
                fire(
                    state,
                    site,
                    Some(server.clone()),
                    vec![crate::has_time_supplier()],
                );
            }
        };
        state.dedicated = true;
        tick(&mut state);
        assert_eq!(state.jars[0].row.game_hooks.skipped, 1);
        assert!(stdout(&mut state, 0).is_empty());
        assert_eq!(stdout(&mut state, 1), ["TAIL/1"]);
        state.dedicated = false;
        tick(&mut state);
        assert_eq!(stdout(&mut state, 0), ["HEAD/1"]);
        assert_eq!(stdout(&mut state, 1), ["TAIL/2"]);
        // The next world's server is a new object: its fields start at their initialisers.
        forget_world(&mut state);
        tick(&mut state);
        assert_eq!(stdout(&mut state, 0), ["HEAD/1"]);
        assert_eq!(stdout(&mut state, 1), ["TAIL/1"]);
    }

    /// A cancellable `ServerPlayer.die` HEAD callback (baguettelib's, architectury's) would keep
    /// the player alive in Java; `player_died` only hears, so the runtime refuses it when it
    /// registers and the jar counts it `refused`: it never runs with its `cancel()` dropped
    /// (#248's re-check). A non-cancellable one runs.
    #[test]
    fn a_cancel_the_hook_cannot_carry_is_refused_and_counted() {
        let mut state = State::default();
        let die = |cancellable: bool| {
            format!(
                "@Mixin(net.minecraft.server.level.ServerPlayer.class) public class Dies {{ \
                   @Inject(method = \"die\", at = @At(\"HEAD\"), cancellable = {cancellable}) \
                   private void onDie(net.minecraft.world.damagesource.DamageSource s, CallbackInfo ci) {{ \
                     System.out.println(\"died\"); }} }}"
            )
        };
        state.jars.push(load("keepalive", "Dies", &die(true)));
        state.jars.push(load("listens", "Dies", &die(false)));
        state.players = vec![FacadePlayer {
            uuid: 0xa1,
            name: "Alex".into(),
            level: None,
            entity_id: Some(7),
            session: 1,
            life: 0,
        }];
        let player = player(&state, 0xa1, 0).unwrap();
        let source = damage_source(&mut State::default(), "lava");
        fire(
            &mut state,
            "player_died",
            Some(player),
            vec![Arg::Object(source)],
        );
        let tally = |jar: &LoadedJar| {
            let t = &jar.row.game_hooks;
            (t.refused, t.bound, t.ran)
        };
        assert_eq!(tally(&state.jars[0]), (1, 0, 0));
        assert!(stdout(&mut state, 0).is_empty());
        assert_eq!(tally(&state.jars[1]), (0, 1, 1));
        assert_eq!(stdout(&mut state, 1), ["died"]);
    }
}
