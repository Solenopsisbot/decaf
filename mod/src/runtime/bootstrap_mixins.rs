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
//!   `Bootstrap.bootStrap()` ([`SERVER_ORDER`]), the client in its `Main.main` and the start of
//!   `Minecraft.<init>`, up to where Fabric Loader 0.20.1 runs the first entrypoint
//!   ([`CLIENT_ORDER`]: its `Bootstrap.bootStrap()`, `ClientBootstrap.bootstrap()`, then the
//!   constructor until `EntrypointPatch`'s hook, just before the last `Thread.currentThread()`
//!   call ahead of the "Backend library: {}" log line, offset 555 in 26.3). Both lists were measured with a
//!   JDK 25 agent on the real 26.3 jars (`tools/trace_bootstrap_clinit_order.py`). On the server
//!   the target must also be a class it loads before mods
//!   ([`crate::runtime::mixin_class_loading::server_loads_before_mods`], measured on Fabric); on the client,
//!   reaching the point before that hook is that.
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
//! static method selected as `()V`, or by name alone when its class declares that name once
//! (`@overloaded` in the list: Mixin's name-only selector selects every overload, and a bare
//! `(CallbackInfo)V` handler fits any target, `Callback.checkDescriptor` with `captureArgs`
//! false). Anything else that would apply on a real game (another anchor, a config plugin that
//! could veto the mixin, a target the bootstrap does not reach or reaches at no one point, a `TAIL`
//! on a static handler into a method with arguments or one the list never reaches, unless a
//! game-hook site binds it) is counted under `<target>#<method>` on the declaring jar's host and
//! not run ([`decide`]). A callback that throws is the game failing to start: no later callback runs, and the
//! loader starts no jar's entrypoint (every jar is refused with that reason). A callback at a point
//! after `BuiltInRegistries.freeze()` that registers into a built-in registry counts: vanilla's
//! `Registry.register` throws "Registry is already frozen" there, but Fabric API's registry sync
//! delays the freeze until after mod initialization, and which game it is is not modelled (the
//! client's `RangeSelectItemModelProperties.bootstrap()` is such a point: a `put` into its id
//! mapper is not a registry write).

use super::*;

/// The `TAIL` points the 26.3 server's bootstrap reaches, in order
/// (`tools/trace_bootstrap_clinit_order.py`): a class's internal name where its `<clinit>`
/// returns, `<class>.<name>()V` where a static `()V` method reaches its last `return`.
const SERVER_ORDER: &str = crate::runtime::vendored_bootstrap_clinit_order_26_3_txt::VENDORED_DATA;

/// The same for the 26.3 client, up to where Fabric runs its first entrypoint in
/// `Minecraft.<init>` (offset 555; `tools/trace_bootstrap_clinit_order.py --side client`).
const CLIENT_ORDER: &str = crate::runtime::vendored_bootstrap_clinit_order_26_3_client_txt::VENDORED_DATA;

/// `CallbackInfo`'s descriptor as a handler's only parameter.
const TAIL_HANDLER_DESCRIPTOR: &str =
    "(Lorg/spongepowered/asm/mixin/injection/callback/CallbackInfo;)V";

/// A line the tool marks as reached on a thread other than `main`.
const OTHER_THREAD: &str = " @other-thread";

/// A method point whose name its class declares more than once.
const OVERLOADED: &str = " @overloaded";

/// One point of a side's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Point {
    /// Its turn; `None` for a point reached more than once or on another thread (no one turn).
    turn: Option<usize>,
    /// A method whose name its class declares more than once.
    overloaded: bool,
}

/// One side's list, by point.
fn parse_order(text: &'static str) -> BTreeMap<&'static str, Point> {
    let mut order: BTreeMap<&'static str, Point> = BTreeMap::new();
    for (position, line) in text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .enumerate()
    {
        let (line, main_thread) = match line.strip_suffix(OTHER_THREAD) {
            Some(point) => (point, false),
            None => (line, true),
        };
        let (point, overloaded) = match line.strip_suffix(OVERLOADED) {
            Some(point) => (point, true),
            None => (line, false),
        };
        order
            .entry(point)
            .and_modify(|seen| seen.turn = None)
            .or_insert(Point {
                turn: main_thread.then_some(position),
                overloaded,
            });
    }
    order
}

/// `side`'s list ([`SERVER_ORDER`] or [`CLIENT_ORDER`]).
fn side_order(side: MixinSide) -> &'static BTreeMap<&'static str, Point> {
    static SERVER: std::sync::OnceLock<BTreeMap<&'static str, Point>> = std::sync::OnceLock::new();
    static CLIENT: std::sync::OnceLock<BTreeMap<&'static str, Point>> = std::sync::OnceLock::new();
    match side {
        MixinSide::Client => CLIENT.get_or_init(|| parse_order(CLIENT_ORDER)),
        _ => SERVER.get_or_init(|| parse_order(SERVER_ORDER)),
    }
}

/// Where `BuiltInRegistries.freeze()` ends on `side`: after it, vanilla's `Registry.register`
/// into a built-in registry throws `IllegalStateException` ("Registry is already frozen").
fn freeze_turn(side: MixinSide) -> Option<usize> {
    side_order(side)
        .get("net/minecraft/core/registries/BuiltInRegistries.freeze()V")
        .and_then(|point| point.turn)
}

/// What the pass does with one callback into a game class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    /// Not a bootstrap callback: another tier answers or counts it.
    NotOurs,
    /// It would run on a real game, at no turn this pass can give it: counted.
    Count,
    /// It runs at this turn.
    Run(usize),
}

/// The pass's decision for `plan` against `order`. `runnable` is everything but the point:
/// Mixin's `TAIL` shape on a static handler taking one `CallbackInfo`, a config that applies it
/// for sure, a class the side loads before mods. `static_handler` alone says the target is a
/// static method (Mixin requires a static handler for one). `hook_site` is whether a game-hook
/// tier binds the target.
///
/// * `<clinit>`: runs at its point's one turn, counted otherwise.
/// * A method selected by name alone or as `()V` whose point the list holds: runs at its one
///   turn, counted otherwise, and when selected by name alone on a name the class declares more
///   than once (Mixin's name-only selector selects every overload, and a `(CallbackInfo)V`
///   handler fits any of them: `Callback.checkDescriptor`, `captureArgs` false).
/// * Any other `TAIL` on a static handler into a method no game-hook site binds (an argument
///   descriptor, a name the list never reaches): it runs on a real game, never here: counted.
fn decide(
    order: &BTreeMap<&'static str, Point>,
    plan: &MixinInjectionPlan,
    runnable: bool,
    static_handler: bool,
    hook_site: bool,
) -> Decision {
    let target = &plan.target;
    if target.name == "<clinit>" {
        return match order.get(target.owner.as_str()) {
            Some(Point {
                turn: Some(turn), ..
            }) if runnable => Decision::Run(*turn),
            _ => Decision::Count,
        };
    }
    let name_only = target.descriptor.is_empty();
    let point = (name_only || target.descriptor == "()V")
        .then(|| order.get(format!("{}.{}()V", target.owner, target.name).as_str()))
        .flatten();
    match point {
        Some(point) => match point.turn {
            Some(turn) if runnable && !(name_only && point.overloaded) => Decision::Run(turn),
            _ => Decision::Count,
        },
        None if plan.at == "TAIL" && static_handler && !hook_site => Decision::Count,
        None => Decision::NotOurs,
    }
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
    /// Its turn in the side's list ([`decide`]), `None` when it is counted.
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
                || crate::runtime::mixin_class_loading::server_loads_before_mods(target);
            let hook_site = crate::runtime::hook_shapes::game_hook_site_at(&plan.target, &plan.at).is_some();
            let position = match decide(
                side_order(side),
                &plan,
                applies && shape && loaded,
                handler,
                hook_site,
            ) {
                Decision::NotOurs => continue,
                Decision::Count => None,
                Decision::Run(turn) => Some(turn),
            };
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
        // Past `BuiltInRegistries.freeze()`, vanilla refuses a registration into a built-in
        // registry, but Fabric API's registry sync (`BootstrapMixin.delayRegistryFreeze`,
        // fabric-registry-sync-v0) moves the freeze to after mod initialization: which one the
        // game has is not modelled, so such a callback counts (its rows stay, as with Fabric API).
        let frozen = freeze_turn(unit.host.mixin_side())
            .is_some_and(|freeze| entry.position.is_some_and(|turn| turn > freeze));
        let built_in_rows = built_in_registrations(unit.host);
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
        if frozen && built_in_registrations(unit.host) > built_in_rows {
            *unit
                .host
                .unknown_host_calls
                .entry(format!(
                    "{}#{} <registration after BuiltInRegistries.freeze()>",
                    entry.plan.target.owner, entry.plan.target.name
                ))
                .or_default() += 1;
        }
    }
    Vec::new()
}

/// How many registrations `host` holds in a built-in registry: every `BuiltInRegistries`
/// registry is keyed `minecraft:`, and a mod's own registry (which is never frozen) its own
/// namespace.
fn built_in_registrations(host: &StdHost) -> usize {
    host.registrations()
        .iter()
        .filter(|entry| entry.registry.starts_with("minecraft:"))
        .count()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A side's list with a class finished once, a method ended once, one ended twice, one ended
    /// off the main thread, and one whose name its class declares twice.
    const ORDER: &str = "# header\n\
        demo/Once\n\
        demo/A.once()V\n\
        demo/A.twice()V\n\
        demo/A.twice()V\n\
        demo/A.elsewhere()V @other-thread\n\
        demo/A.overloaded()V @overloaded\n";

    fn plan(name: &str, descriptor: &str, at: &str) -> MixinInjectionPlan {
        MixinInjectionPlan {
            target: MethodRef {
                owner: if name == "<clinit>" {
                    "demo/Once"
                } else {
                    "demo/A"
                }
                .into(),
                name: name.into(),
                descriptor: descriptor.into(),
            },
            callback: MethodRef {
                owner: "demo/mixin/M".into(),
                name: "tail".into(),
                descriptor: TAIL_HANDLER_DESCRIPTOR.into(),
            },
            at: at.into(),
            cancellable: false,
            invoke_target: None,
            ordinal: None,
            slice: None,
            shift: None,
        }
    }

    fn decided(name: &str, descriptor: &str) -> Decision {
        decide(
            &parse_order(ORDER),
            &plan(name, descriptor, "TAIL"),
            true,
            true,
            false,
        )
    }

    #[test]
    fn a_point_reached_twice_or_off_the_main_thread_has_no_turn() {
        let order = parse_order(ORDER);
        assert_eq!(order["demo/A.once()V"].turn, Some(1));
        assert_eq!(order["demo/A.twice()V"].turn, None);
        assert_eq!(order["demo/A.elsewhere()V"].turn, None);
        assert!(order["demo/A.overloaded()V"].overloaded);
        assert_eq!(decided("<clinit>", ""), Decision::Run(0));
        assert_eq!(decided("once", ""), Decision::Run(1));
        assert_eq!(decided("once", "()V"), Decision::Run(1));
        assert_eq!(decided("twice", "()V"), Decision::Count);
        assert_eq!(decided("elsewhere", "()V"), Decision::Count);
    }

    /// Mixin's name-only selector selects every overload; `()V` selects the one.
    #[test]
    fn a_name_only_selector_on_an_overloaded_name_counts() {
        assert_eq!(decided("overloaded", ""), Decision::Count);
        assert_eq!(decided("overloaded", "()V"), Decision::Run(5));
    }

    /// A `(CallbackInfo)V` handler fits any target: a `TAIL` into a static method with
    /// arguments, or one the bootstrap never reaches, would run on a real game; here it counts,
    /// unless a game-hook site binds it. An instance handler's target is not the pass's.
    #[test]
    fn a_tail_no_turn_can_answer_counts() {
        assert_eq!(decided("withArgs", "(I)V"), Decision::Count);
        assert_eq!(decided("neverReached", ""), Decision::Count);
        let order = parse_order(ORDER);
        let tail = plan("withArgs", "(I)V", "TAIL");
        assert_eq!(decide(&order, &tail, true, true, true), Decision::NotOurs);
        assert_eq!(decide(&order, &tail, true, false, false), Decision::NotOurs);
        let head = plan("withArgs", "(I)V", "HEAD");
        assert_eq!(decide(&order, &head, false, true, false), Decision::NotOurs);
        // Any other anchor at a measured point is the pass's, and counts.
        let head = plan("once", "()V", "HEAD");
        assert_eq!(decide(&order, &head, false, true, false), Decision::Count);
    }
}
