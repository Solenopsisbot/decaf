//! Mixin callbacks at the end of a game class's static initializer, run before the first
//! entrypoint.
//!
//! A Mixin `@Inject(method = "<clinit>", at = @At("TAIL"))` into a game class runs once, as the
//! last thing that class's static initializer does. Bundle API registers its data component type
//! that way: `DataComponentTypesMixin` into `DataComponents` does
//! `Registry.register(BuiltInRegistries.DATA_COMPONENT_TYPE, ...)`. The runtime models game classes
//! on the host, so their initializers never run in it, and nothing else would ever run such a
//! callback. This pass runs them itself, once per host, before the jar's first entrypoint
//! ([`NativeJar::run_bootstrap_static_injectors`]).
//!
//! Exactly when, and in which order:
//!
//! * A real Fabric 26.3 server initializes these classes in `Bootstrap.bootStrap()`, before any
//!   `main` entrypoint: the target must be a class it loads before mods
//!   ([`crate::runtime::mixin_class_loading::server_loads_before_mods`], measured on Fabric) whose
//!   initializer returns during the server's bootstrap ([`CLINIT_ORDER`], measured with a JDK 25
//!   agent: `tools/trace_bootstrap_clinit_order.py`).
//! * Callbacks into different classes run in the order those classes' initializers return
//!   ([`CLINIT_ORDER`]), which is when a `TAIL` callback runs. The list is the server's; on a
//!   client the order of two classes is not measured, so callbacks into more than one class are
//!   counted there instead of run.
//! * Callbacks into one class run in Mixin's apply order (`mixin_plans_in_apply_order`). Two mods'
//!   callbacks into one class are ordered by Mixin across their configs, which is not modelled:
//!   those are counted.
//! * The mixins are those of every mod the loader loads from this jar: its own and each nested
//!   jar it selects (`fabric.mod.json` `jars`, JarJar), each through its own configs' side gate.
//!
//! Only the shape Mixin gives a `TAIL` callback on a static `void` target runs: a static handler
//! taking one `CallbackInfo`, not cancellable, with no selector modifier. Anything else that would
//! apply on a real server (another anchor, a config plugin that could veto the mixin, a target the
//! bootstrap does not initialize or initializes at an unmeasured point) is counted under
//! `<target>#<clinit>` and not run. A callback that throws is the game failing to start: it is
//! reported, and no later callback runs.

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

/// One callback that would run on a real server, with what decides its turn.
struct Pending {
    /// [`clinit_position`] of its target.
    position: usize,
    /// Which loaded jar declares it: 0 for this jar, then its nested jars in load order.
    jar: usize,
    plan: MixinInjectionPlan,
}

impl NativeJar {
    /// Runs every applicable Mixin `@Inject(method = "<clinit>", at = @At("TAIL"))` into a game
    /// class, of this jar's mods, as the bootstrap would (see the module documentation), once per
    /// host. Returns the failures: a callback that threw, after which none runs.
    pub fn run_bootstrap_static_injectors(
        &self,
        host: &mut StdHost,
        budget: &ExecutionBudget,
    ) -> Vec<String> {
        if std::mem::replace(&mut host.bootstrap_injectors_ran, true) {
            return Vec::new();
        }
        let side = host.mixin_side();
        // The loaded mods' jars: this one, then the nested jars the loader selects. A resolution
        // failure is already the load's error (`seed_host`); its nested jars then load none.
        let nested = nested_jars::select_checked_for_side(self, host.environment_type.as_str())
            .unwrap_or_default();
        let jars: Vec<&NativeJar> = std::iter::once(self)
            .chain(nested.iter().map(|candidate| &candidate.jar))
            .collect();
        let jar_class = |owner: &str| jars.iter().any(|jar| jar.classes.contains_key(owner));
        let mut pending: Vec<Pending> = Vec::new();
        let mut counted: Vec<String> = Vec::new();
        for (index, jar) in jars.iter().enumerate() {
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
                match mixin_config_gate(&configs, &config_sides, &plan.callback.owner, side) {
                    Ok(()) => {}
                    // A config plugin's `shouldApplyMixin` could veto the mixin: whether it
                    // applies is not known.
                    Err(MixinSkipReason::ConfigPlugin) => {
                        counted.push(target.to_owned());
                        continue;
                    }
                    // Mixin never applies it on this side.
                    Err(_) => continue,
                }
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
                    .filter(|_| crate::runtime::mixin_class_loading::server_loads_before_mods(target));
                match (shape, position) {
                    (true, Some(position)) => pending.push(Pending {
                        position,
                        jar: index,
                        plan,
                    }),
                    _ => counted.push(target.to_owned()),
                }
            }
        }
        // Two jars' callbacks into one class: Mixin orders them across their configs.
        let mut jars_by_target: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
        for entry in &pending {
            jars_by_target
                .entry(entry.plan.target.owner.as_str())
                .or_default()
                .insert(entry.jar);
        }
        let shared: BTreeSet<String> = jars_by_target
            .iter()
            .filter(|(_, jars)| jars.len() > 1)
            .map(|(target, _)| (*target).to_owned())
            .collect();
        // On a client, the order of two classes' initializers is not measured.
        let unordered = side == MixinSide::Client && jars_by_target.len() > 1;
        let (run, refused): (Vec<Pending>, Vec<Pending>) = pending
            .into_iter()
            .partition(|entry| !unordered && !shared.contains(entry.plan.target.owner.as_str()));
        counted.extend(refused.into_iter().map(|entry| entry.plan.target.owner));
        for target in counted {
            *host
                .unknown_host_calls
                .entry(format!("{target}#<clinit>"))
                .or_default() += 1;
        }
        let mut run = run;
        // Stable: one class's callbacks keep Mixin's apply order.
        run.sort_by_key(|entry| entry.position);
        let mut failures = Vec::new();
        for entry in run {
            let info = host_mixin::callback_info(
                &host_mixin::callback_info_key(&entry.plan.target.owner, "<clinit>", false),
                &entry.plan.callback,
                false,
            );
            if let Err(error) =
                self.invoke_persistent(&entry.plan.callback, None, &[info], host, budget)
            {
                failures.push(format!(
                    "{}#{} at the end of {}.<clinit>: {error}",
                    entry.plan.callback.owner, entry.plan.callback.name, entry.plan.target.owner
                ));
                break;
            }
        }
        failures
    }
}
