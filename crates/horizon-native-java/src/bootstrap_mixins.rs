//! Mixin callbacks at the end of a game class's static initializer, or of a static bootstrap
//! method, run before the first entrypoint of any jar.
//!
//! A Mixin `@Inject(method = "<clinit>", at = @At("TAIL"))` into a game class runs once, as the
//! last thing that class's static initializer does. Bundle API registers its data component type
//! that way: `DataComponentTypesMixin` into `DataComponents` does
//! `Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE, ...)`. A `TAIL` into a static `()V`
//! method the bootstrap calls once is the same thing at that method's last `return`: Bundle API's
//! client `NumericPropertiesMixin` adds its item model property with a `TAIL` into
//! `RangeSelectItemModelProperties.bootstrap()`. The runtime models game classes on the host, so
//! their initializers and bootstrap methods never run in it, and nothing else would ever run such
//! a callback. This pass runs them itself ([`run_bootstrap_injectors`]).
//!
//! It is one pass over every loaded jar, as Mixin weaves every mod's callbacks into one game
//! and the game bootstraps once: a loader runs it after seeding every jar's host and before any
//! jar's entrypoint (`mod/src/content.rs`, `mod/src/client.rs`, the probe's `main.rs`). Each
//! jar's callbacks run on its own host (every jar has its own model of the game), and the pass
//! runs once per host.
//!
//! Exactly when, and in which order:
//!
//! * A real Fabric 26.3 game reaches these points before any `main` entrypoint: the server in
//!   `Bootstrap.bootStrap()` ([`SERVER_ORDER`]), the client in its `Main.main` up to
//!   `Minecraft.<init>`, where Fabric runs the first entrypoint ([`CLIENT_ORDER`]: its
//!   `Bootstrap.bootStrap()`, then `ClientBootstrap.bootstrap()`). Both lists were measured with a
//!   JDK 25 agent on the real 26.3 jars (`tools/trace_bootstrap_clinit_order.py`). On the server
//!   the target must also be a class it loads before mods
//!   ([`crate::mixin_class_loading::server_loads_before_mods`], measured on Fabric); on the client,
//!   reaching the point before `Minecraft.<init>` is that.
//! * Callbacks into different points run in the order the side's list reaches them, which is when
//!   a `TAIL` callback runs, across all jars. A point the list reaches more than once, or on
//!   another thread, has no one turn: its callbacks are counted. A pass whose hosts are not all on
//!   one side has no one list: every callback is counted.
//! * Callbacks into one point run in Mixin's apply order (`mixin_plans_in_apply_order`). Two
//!   different mods' callbacks into one class are ordered by Mixin across their configs, which is
//!   not modelled: every callback into that class is counted, in every jar. One mod bundled by two
//!   jars (the same nested mod id) is one mod, whose callbacks run on each of those jars' hosts.
//! * The mixins are those of every mod the loader loads: each jar's own, each nested jar it
//!   selects (`fabric.mod.json` `jars`, JarJar), and the probe's `--link`ed jars (one host), each
//!   through its own configs' side gate. A dependency jar the mod links into another jar's host is
//!   a loaded jar of its own there; its callbacks run on its own host only.
//!
//! Only the shape Mixin gives a `TAIL` callback on a static `void` target runs: a static handler
//! taking one `CallbackInfo`, not cancellable, with no selector modifier, into `<clinit>` or into a
//! static method named with no descriptor or `()V` (Mixin's name-only selector: a mod whose
//! handler takes only a `CallbackInfo` loads on Fabric only if it selects a no-argument `void`
//! method). Anything else that would apply on a real game (another anchor, a config plugin that
//! could veto the mixin, a target the bootstrap does not reach or reaches at no one point) is
//! counted under `<target>#<method>` on the declaring jar's host and not run; a callback into a
//! static method the side's list never reaches is not this pass's (another tier answers or counts
//! it). A callback that throws is the game failing to start: no later callback runs, and the
//! loader starts no jar's entrypoint (every jar is refused with that reason). So is a callback at
//! a point after `BuiltInRegistries.freeze()` that registers into a built-in registry, where the
//! game's `Registry.register` throws "Registry is already frozen" (the client's
//! `RangeSelectItemModelProperties.bootstrap()` is such a point: a `put` into its id mapper is not
//! a registry write).

use super::*;

/// The `TAIL` points the 26.3 server's bootstrap reaches, in order
/// (`tools/trace_bootstrap_clinit_order.py`): a class's internal name where its `<clinit>`
/// returns, `<class>.<name>()V` where a static `()V` method reaches its last `return`.
const SERVER_ORDER: &str = include_str!("bootstrap_clinit_order_26_3.txt");

/// The same for the 26.3 client, up to `Minecraft.<init>`
/// (`tools/trace_bootstrap_clinit_order.py --side client`).
const CLIENT_ORDER: &str = include_str!("bootstrap_clinit_order_26_3_client.txt");

/// `CallbackInfo`'s descriptor as a handler's only parameter.
const TAIL_HANDLER_DESCRIPTOR: &str =
    "(Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo;)V";

/// A line the tool marks as reached on a thread other than `main`.
const OTHER_THREAD: &str = " @other-thread";

/// One side's list: each point with its turn, `None` for a point reached more than once or on
/// another thread (no one turn).
fn parse_order(text: &'static str) -> BTreeMap<&'static str, Option<usize>> {
    let mut order: BTreeMap<&'static str, Option<usize>> = BTreeMap::new();
    for (position, line) in text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .enumerate()
    {
        let (point, main_thread) = match line.strip_suffix(OTHER_THREAD) {
            Some(point) => (point, false),
            None => (line, true),
        };
        order
            .entry(point)
            .and_modify(|turn| *turn = None)
            .or_insert(main_thread.then_some(position));
    }
    order
}

/// `side`'s list ([`SERVER_ORDER`] or [`CLIENT_ORDER`]).
fn side_order(side: MixinSide) -> &'static BTreeMap<&'static str, Option<usize>> {
    static SERVER: std::sync::OnceLock<BTreeMap<&'static str, Option<usize>>> =
        std::sync::OnceLock::new();
    static CLIENT: std::sync::OnceLock<BTreeMap<&'static str, Option<usize>>> =
        std::sync::OnceLock::new();
    match side {
        MixinSide::Client => CLIENT.get_or_init(|| parse_order(CLIENT_ORDER)),
        _ => SERVER.get_or_init(|| parse_order(SERVER_ORDER)),
    }
}

/// The list's name for the point a `TAIL` into `target` runs at: the class for `<clinit>`,
/// `<class>.<name>()V` for a static method selected by name alone or as `()V`, `None` for any
/// other target.
fn tail_point(target: &MethodRef) -> Option<String> {
    if target.name == "<clinit>" {
        return Some(target.owner.clone());
    }
    matches!(target.descriptor.as_str(), "" | "()V")
        .then(|| format!("{}.{}()V", target.owner, target.name))
}

/// Where `BuiltInRegistries.freeze()` ends on `side`: after it, `Registry.register` into a
/// built-in registry throws `IllegalStateException` ("Registry is already frozen").
fn freeze_turn(side: MixinSide) -> Option<usize> {
    point_turn(
        side,
        "net/minecraft/core/registries/BuiltInRegistries.freeze()V",
    )
    .flatten()
}

/// Where `side`'s bootstrap reaches `point`: `None` when it never does, `Some(None)` when it
/// does at no one turn.
fn point_turn(side: MixinSide, point: &str) -> Option<Option<usize>> {
    side_order(side).get(point).copied()
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
    /// Its turn in the side's list ([`point_turn`]), `None` when it is counted for its own shape
    /// or its point.
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
            if !target.starts_with("net/minecraft/") || jar_class(target) {
                continue;
            }
            // Every `<clinit>` callback is this pass's; a callback into a method only when it is
            // into a static `()V` method this side's bootstrap reaches (any anchor but `TAIL`
            // then counts below).
            let point = tail_point(&plan.target);
            let turn = point.as_deref().and_then(|point| point_turn(side, point));
            if plan.target.name != "<clinit>" && turn.is_none() {
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
            // The server's bootstrap loads the class before mods only if Fabric's does; on the
            // client, the list ends where Fabric runs the first entrypoint.
            let loaded = side == MixinSide::Client
                || crate::mixin_class_loading::server_loads_before_mods(target);
            let position = turn.flatten().filter(|_| applies && shape && loaded);
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

/// Runs every applicable Mixin `@Inject(method = "<clinit>", at = @At("TAIL"))` into a game class,
/// and every `TAIL` into a static bootstrap method the side reaches once, of every unit's mods, as the bootstrap would (see the module documentation): one pass over all
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
    // The two sides' lists are not one order: a pass over hosts of both sides runs nothing.
    let on = |side: MixinSide| {
        pending
            .iter()
            .any(|entry| units[entry.unit].host.mixin_side() == side)
    };
    let unordered = on(MixinSide::Client) && on(MixinSide::Server);
    let (mut run, counted): (Vec<Pending>, Vec<Pending>) = pending.into_iter().partition(|entry| {
        entry.position.is_some()
            && !unordered
            && owners_by_target[&entry.plan.target.owner].len() == 1
    });
    for entry in counted {
        *units[entry.unit]
            .host
            .unknown_host_calls
            .entry(format!(
                "{}#{}",
                entry.plan.target.owner, entry.plan.target.name
            ))
            .or_default() += 1;
    }
    // The global finish order; stable, so one class's callbacks keep Mixin's apply order and one
    // mod's callbacks on several hosts run in unit order.
    run.sort_by_key(|entry| entry.position);
    for entry in run {
        let unit = &mut units[entry.unit];
        let info = host_mixin::callback_info(
            &host_mixin::callback_info_key(
                &entry.plan.target.owner,
                &entry.plan.target.name,
                false,
            ),
            &entry.plan.callback,
            false,
        );
        // Past `BuiltInRegistries.freeze()`, a registration is the game failing to start.
        let frozen = freeze_turn(unit.host.mixin_side())
            .is_some_and(|freeze| entry.position.is_some_and(|turn| turn > freeze));
        let registered = unit.host.registrations().len();
        let site = format!(
            "{}#{} at the end of {}.{}",
            entry.plan.callback.owner,
            entry.plan.callback.name,
            entry.plan.target.owner,
            entry.plan.target.name
        );
        if let Err(error) =
            unit.runner
                .invoke_persistent(&entry.plan.callback, None, &[info], unit.host, budget)
        {
            return vec![format!("{site}: {error}")];
        }
        if frozen && unit.host.registrations().len() > registered {
            return vec![format!(
                "{site}: java.lang.IllegalStateException: Registry.register after BuiltInRegistries.freeze() (Registry is already frozen)"
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
