//! Mixin callbacks at the end of a game class's static initializer, run before the first
//! entrypoint of any jar.
//!
//! A Mixin `@Inject(method = "<clinit>", at = @At("TAIL"))` into a game class runs once, as the
//! last thing that class's static initializer does. Bundle API registers its data component type
//! that way: `DataComponentTypesMixin` into `DataComponents` does
//! `Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE, ...)`. The runtime models game classes
//! on the host, so their initializers never run in it, and nothing else would ever run such a
//! callback. This pass runs them itself ([`run_bootstrap_injectors`]).
//!
//! It is one pass over every loaded jar, as Mixin weaves every mod's callbacks into one game
//! and the game bootstraps once: a loader runs it after seeding every jar's host and before any
//! jar's entrypoint (`mod/src/content.rs`, `mod/src/client.rs`, the probe's `main.rs`). Each
//! jar's callbacks run on its own host (every jar has its own model of the game), and the pass
//! runs once per host.
//!
//! Exactly when, and in which order:
//!
//! * A real Fabric 26.3 server initializes these classes in `Bootstrap.bootStrap()`, before any
//!   `main` entrypoint: the target must be a class it loads before mods
//!   ([`crate::runtime::mixin_class_loading::server_loads_before_mods`], measured on Fabric) whose
//!   initializer returns during the server's bootstrap ([`CLINIT_ORDER`], measured with a JDK 25
//!   agent: `tools/trace_bootstrap_clinit_order.py`).
//! * Callbacks into different classes run in the order those classes' initializers return
//!   ([`CLINIT_ORDER`]), which is when a `TAIL` callback runs, across all jars. The list is the
//!   server's; on a client the order of two classes is not measured, so when callbacks of the
//!   pass target more than one class there, every one is counted instead of run.
//! * Callbacks into one class run in Mixin's apply order (`mixin_plans_in_apply_order`). Two
//!   different mods' callbacks into one class are ordered by Mixin across their configs, which is
//!   not modelled: every callback into that class is counted, in every jar. One mod bundled by two
//!   jars (the same nested mod id) is one mod, whose callbacks run on each of those jars' hosts.
//! * The mixins are those of every mod the loader loads: each jar's own, each nested jar it
//!   selects (`fabric.mod.json` `jars`, JarJar), and the probe's `--link`ed jars (one host), each
//!   through its own configs' side gate. A dependency jar the mod links into another jar's host is
//!   a loaded jar of its own there; its callbacks run on its own host only.
//!
//! Only the shape Mixin gives a `TAIL` callback on a static `void` target runs: a static handler
//! taking one `CallbackInfo`, not cancellable, with no selector modifier. Anything else that would
//! apply on a real server (another anchor, a config plugin that could veto the mixin, a target the
//! bootstrap does not initialize or initializes at an unmeasured point) is counted under
//! `<target>#<clinit>` on the declaring jar's host and not run. A callback that throws is the game
//! failing to start: no later callback runs, and the loader starts no jar's entrypoint (every jar
//! is refused with that reason).

use super::*;

/// The game classes whose `<clinit>` returns during the 26.3 server's bootstrap, in the order they
/// return (`tools/trace_bootstrap_clinit_order.py`).
const CLINIT_ORDER: &str = crate::runtime::vendored_bootstrap_clinit_order_26_3_txt::VENDORED_DATA;

/// `CallbackInfo`'s descriptor as a handler's only parameter.
const TAIL_HANDLER_DESCRIPTOR: &str =
    "(Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo;)V";

/// Where `class`'s `<clinit>` returns in the server's bootstrap, if it does.
fn clinit_position(class: &str) -> Option<usize> {
    static ORDER: std::sync::OnceLock<BTreeMap<&'static str, usize>> = std::sync::OnceLock::new();
    ORDER
        .get_or_init(|| {
            CLINIT_ORDER
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .enumerate()
                .map(|(position, class)| (class, position))
                .collect()
        })
        .get(class)
        .copied()
}

/// One jar of the pass: the jar whose runtime runs the callbacks, its host, and the jars whose
/// mixins apply on that host (the jar itself, then any jar linked into the same host as a loaded
/// mod, as the probe's `--link`). Each source's nested jars join it.
pub struct BootstrapUnit<'a> {
    pub runner: &'a NativeJar,
    pub host: &'a mut StdHost,
    pub sources: Vec<&'a NativeJar>,
}

/// One callback that would run on a real server, with what decides its turn.
struct Pending {
    /// [`clinit_position`] of its target, `None` when it is counted for its own shape.
    position: Option<usize>,
    /// The unit whose host it runs on.
    unit: usize,
    /// The mod that declares it: its loader id, or a unique key for a jar without one.
    owner: String,
    plan: MixinInjectionPlan,
}

/// The callbacks into game classes' static initializers of one unit's mods, each with its turn or
/// `None` when its shape, its config plugin or its target keeps it from running exactly.
fn unit_callbacks(index: usize, unit: &BootstrapUnit<'_>) -> Vec<Pending> {
    let side = unit.host.mixin_side();
    let environment = unit.host.environment_type.as_str();
    // Each source's loaded mods: the jar, then the nested jars the loader selects. A resolution
    // failure is already the load's error (`seed_host`); its nested jars then load none.
    let mut jars: Vec<(String, NativeJar)> = Vec::new();
    let mut borrowed: Vec<(String, &NativeJar)> = Vec::new();
    for (source_index, source) in unit.sources.iter().enumerate() {
        let key = |jar: &NativeJar, nested: usize| {
            jar.fabric_id
                .clone()
                .or_else(|| jar.neoforge_id.clone())
                .unwrap_or_else(|| format!("unit {index} jar {source_index} nested {nested}"))
        };
        borrowed.push((key(source, 0), source));
        let nested = nested_jars::select_checked_for_side(source, environment).unwrap_or_default();
        for (nested_index, candidate) in nested.into_iter().enumerate() {
            jars.push((key(&candidate.jar, nested_index + 1), candidate.jar));
        }
    }
    let all: Vec<(&str, &NativeJar)> = borrowed
        .iter()
        .map(|(key, jar)| (key.as_str(), *jar))
        .chain(jars.iter().map(|(key, jar)| (key.as_str(), jar)))
        .collect();
    let jar_class = |owner: &str| all.iter().any(|(_, jar)| jar.classes.contains_key(owner));
    let mut pending = Vec::new();
    for (owner, jar) in &all {
        let configs = jar.mixin_configs();
        let config_sides = jar.mixin_config_sides();
        for plan in jar.mixin_plans_in_apply_order() {
            let target = plan.target.owner.as_str();
            if plan.target.name != "<clinit>"
                || !target.starts_with("net/minecraft/")
                || jar_class(target)
            {
                continue;
            }
            let applies =
                match mixin_config_gate(&configs, &config_sides, &plan.callback.owner, side) {
                    Ok(()) => true,
                    // A config plugin's `shouldApplyMixin` could veto the mixin: whether it applies
                    // is not known.
                    Err(MixinSkipReason::ConfigPlugin) => false,
                    // Mixin never applies it on this side.
                    Err(_) => continue,
                };
            let handler = jar
                .classes
                .get(&plan.callback.owner)
                .and_then(|class| class.method(&plan.callback.name, &plan.callback.descriptor))
                .is_some_and(|method| method.is_static());
            let shape = plan.at == "TAIL"
                && !plan.cancellable
                && plan.ordinal.is_none()
                && plan.slice.is_none()
                && plan.shift.is_none()
                && plan.callback.descriptor == TAIL_HANDLER_DESCRIPTOR
                && handler;
            let position = clinit_position(target)
                .filter(|_| applies && shape)
                .filter(|_| crate::runtime::mixin_class_loading::server_loads_before_mods(target));
            pending.push(Pending {
                position,
                unit: index,
                owner: (*owner).to_owned(),
                plan,
            });
        }
    }
    pending
}

/// Runs every applicable Mixin `@Inject(method = "<clinit>", at = @At("TAIL"))` into a game class
/// of every unit's mods, as the bootstrap would (see the module documentation): one pass over all
/// the loaded jars, before any of them runs an entrypoint. A unit whose host already had its pass
/// takes no part. Returns the failures: a callback that threw, after which none runs and the
/// game does not start.
pub fn run_bootstrap_injectors(
    units: &mut [BootstrapUnit<'_>],
    budget: &ExecutionBudget,
) -> Vec<String> {
    let fresh: Vec<bool> = units
        .iter_mut()
        .map(|unit| !std::mem::replace(&mut unit.host.bootstrap_injectors_ran, true))
        .collect();
    let mut pending: Vec<Pending> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        if fresh[index] {
            pending.extend(unit_callbacks(index, unit));
        }
    }
    // Two different mods' callbacks into one class: Mixin orders them across their configs.
    let mut owners_by_target: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in &pending {
        owners_by_target
            .entry(entry.plan.target.owner.clone())
            .or_default()
            .insert(entry.owner.clone());
    }
    // On a client, the order of two classes' initializers is not measured: callbacks that would
    // run into more than one class are all counted.
    let client = pending
        .iter()
        .any(|entry| units[entry.unit].host.mixin_side() == MixinSide::Client);
    let runnable_targets: BTreeSet<&str> = pending
        .iter()
        .filter(|entry| entry.position.is_some())
        .map(|entry| entry.plan.target.owner.as_str())
        .collect();
    let unordered = client && runnable_targets.len() > 1;
    let (mut run, counted): (Vec<Pending>, Vec<Pending>) = pending.into_iter().partition(|entry| {
        entry.position.is_some()
            && !unordered
            && owners_by_target[&entry.plan.target.owner].len() == 1
    });
    for entry in counted {
        *units[entry.unit]
            .host
            .unknown_host_calls
            .entry(format!("{}#<clinit>", entry.plan.target.owner))
            .or_default() += 1;
    }
    // The global finish order; stable, so one class's callbacks keep Mixin's apply order and one
    // mod's callbacks on several hosts run in unit order.
    run.sort_by_key(|entry| entry.position);
    for entry in run {
        let unit = &mut units[entry.unit];
        let info = host_mixin::callback_info(
            &host_mixin::callback_info_key(&entry.plan.target.owner, "<clinit>", false),
            &entry.plan.callback,
            false,
        );
        if let Err(error) =
            unit.runner
                .invoke_persistent(&entry.plan.callback, None, &[info], unit.host, budget)
        {
            return vec![format!(
                "{}#{} at the end of {}.<clinit>: {error}",
                entry.plan.callback.owner, entry.plan.callback.name, entry.plan.target.owner
            )];
        }
    }
    Vec::new()
}

impl NativeJar {
    /// [`run_bootstrap_injectors`] for a game with this jar alone (its nested jars included): a
    /// loader with several jars runs the pass once over all of them instead.
    pub fn run_bootstrap_static_injectors(
        &self,
        host: &mut StdHost,
        budget: &ExecutionBudget,
    ) -> Vec<String> {
        run_bootstrap_injectors(
            &mut [BootstrapUnit {
                runner: self,
                host,
                sources: vec![self],
            }],
            budget,
        )
    }
}
