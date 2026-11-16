//! Public bridge seam for generated Sunrise game-point listeners.
//!
//! Generated listeners translate a typed record into [`GameDispatch`] and apply the returned
//! answer. This module owns one global Mixin order and keeps Java callback semantics in the
//! existing resident runtime. Java-to-engine re-entry is deliberately refused in PR 1: no
//! Sunrise dispatch token is available inside a Java callback yet. Argument-mutating Mixin plans
//! are filtered into the load-time body-weave tables (`@ModifyArg`/`@ModifyVariable`) and never
//! enter these point registrations; generated record copy-back is deferred to PR 2.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, MutexGuard};

use mod_api::ServerHost;

use crate::runtime::{GameHookContext, GameHookEffect, GameObject, NativeError, Value};
use crate::{HorizonJava, JAR_BUDGET, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MethodId<'a>(pub &'a str);
impl<'a> From<&'a str> for MethodId<'a> {
    fn from(value: &'a str) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Head,
    Return,
    Tail,
}

/// Stable host-owned identity; its marker is derived from the game's key, never a Rust address.
pub type GameObjectHandle = GameObject;

/// One generated Sunrise point firing.
pub struct GameDispatch<'a> {
    pub method: MethodId<'a>,
    pub phase: Phase,
    pub receiver: Option<GameObjectHandle>,
    pub args: &'a mut [Value],
    /// The real result at RETURN or TAIL. HEAD passes `None`.
    pub actual: Option<Value>,
    pub host: &'a mut dyn ServerHost,
}

impl<'a> GameDispatch<'a> {
    pub fn new(
        method: MethodId<'a>,
        phase: Phase,
        receiver: Option<GameObjectHandle>,
        args: &'a mut [Value],
        actual: Option<Value>,
        host: &'a mut dyn ServerHost,
    ) -> Self {
        Self {
            method,
            phase,
            receiver,
            args,
            actual,
            host,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DispatchAnswer {
    pub cancelled: bool,
    pub returned: Option<Value>,
    pub callbacks: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispatchError(String);
impl DispatchError {
    fn from_error(error: impl std::fmt::Display) -> Self {
        Self(error.to_string())
    }
    pub fn is_reentry_refusal(&self) -> bool {
        self.0.starts_with("Java game-hook re-entry is unsupported")
    }
}
impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DispatchError {}

struct PlanSnapshot {
    words: [u64; 4],
}
/// Immutable snapshots are intentionally leaked: an in-flight dispatch may hold any old pointer.
/// Refresh publishes one complete replacement with Release; it never clears live words in place.
#[derive(Debug)]
pub(crate) struct PlanBits {
    ptr: AtomicPtr<PlanSnapshot>,
}
impl Default for PlanBits {
    fn default() -> Self {
        Self {
            ptr: AtomicPtr::new(Box::into_raw(Box::new(PlanSnapshot { words: [0; 4] }))),
        }
    }
}
impl PlanBits {
    fn publish(&self, words: [u64; 4]) {
        self.ptr.store(
            Box::into_raw(Box::new(PlanSnapshot { words })),
            Ordering::Release,
        );
    }
    fn maybe_contains(&self, id: &str) -> bool {
        let (word, bit) = bit_for(id);
        let snapshot = self.ptr.load(Ordering::Acquire);
        // SAFETY: snapshots are never reclaimed, so every acquired pointer remains valid.
        unsafe { (*snapshot).words[word] & (1_u64 << bit) != 0 }
    }
}
fn bit_for(id: &str) -> (usize, u32) {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (((hash >> 6) as usize) & 3, (hash & 63) as u32)
}
fn set_bit(words: &mut [u64; 4], id: &str) {
    let (word, bit) = bit_for(id);
    words[word] |= 1_u64 << bit;
}
fn set_target_bit(words: &mut [u64; 4], owner: &str, name: &str, descriptor: &str) {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in owner
        .bytes()
        .chain(std::iter::once(b'#'))
        .chain(name.bytes())
        .chain(std::iter::once(b' ').chain(descriptor.bytes()))
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    words[((hash >> 6) as usize) & 3] |= 1_u64 << (hash & 63);
}
pub(crate) fn matches_method(
    id: &str,
    hook: &str,
    owner: &str,
    name: &str,
    descriptor: &str,
) -> bool {
    if id == hook {
        return true;
    }
    let Some(rest) = id.strip_prefix(owner).and_then(|r| r.strip_prefix('#')) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(name).and_then(|r| r.strip_prefix(' ')) else {
        return false;
    };
    rest == descriptor
}

struct ActiveFrame {
    owner: *const HorizonJava,
    state: std::cell::UnsafeCell<State>,
    nested_entries: std::cell::Cell<u32>,
    refused_reentries: std::cell::Cell<u32>,
}
struct ActiveFrameSlot(Box<ActiveFrame>);
thread_local! { static ACTIVE_FRAMES: RefCell<Vec<ActiveFrameSlot>> = const { RefCell::new(Vec::new()) }; }
#[cfg(test)]
thread_local! { static FORCE_CALLBACK_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
static BLOCK_CALLBACK: std::sync::Mutex<
    Option<(Arc<std::sync::Barrier>, Arc<std::sync::Barrier>)>,
> = std::sync::Mutex::new(None);
#[cfg(test)]
static NESTED_DISPATCH_TARGET: std::sync::Mutex<Option<Arc<HorizonJava>>> =
    std::sync::Mutex::new(None);

pub(crate) enum StateGuard<'a> {
    Locked {
        _gate: MutexGuard<'a, ()>,
        state: MutexGuard<'a, State>,
    },
    /// A raw view into the active frame's state. A nested Hooks call can alias the outer
    /// dispatcher's live `&mut State` and re-enter a jar interpreter; PR2 replaces this with
    /// Sunrise's explicit re-entry contract. `lock_state` counts every such view for the census.
    Active {
        state: *mut State,
        _marker: std::marker::PhantomData<&'a mut State>,
    },
}

impl Deref for StateGuard<'_> {
    type Target = State;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Locked { state, .. } => state,
            Self::Active { state, .. } => {
                // SAFETY: the active frame is owned by this thread and remains on its TLS stack
                // for the guard's lifetime.
                unsafe { &**state }
            }
        }
    }
}

impl DerefMut for StateGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Locked { state, .. } => state,
            Self::Active { state, .. } => {
                // SAFETY: the active frame is owned by this thread and remains on its TLS stack
                // for the guard's lifetime.
                unsafe { &mut **state }
            }
        }
    }
}

pub(crate) fn lock_state(owner: &HorizonJava) -> StateGuard<'_> {
    let owner_ptr = owner as *const HorizonJava;
    let active = ACTIVE_FRAMES.with(|frames| {
        let frames = frames.borrow();
        let frame = frames.last().filter(|frame| frame.0.owner == owner_ptr)?;
        frame
            .0
            .nested_entries
            .set(frame.0.nested_entries.get().saturating_add(1));
        Some(frame.0.state.get())
    });
    if let Some(state) = active {
        return StateGuard::Active {
            state,
            _marker: std::marker::PhantomData,
        };
    }
    let gate = owner
        .dispatch_gate
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let state = owner
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    StateGuard::Locked { _gate: gate, state }
}

fn finish_frame(slot: ActiveFrameSlot) -> State {
    let ActiveFrameSlot(frame) = slot;
    let ActiveFrame {
        state,
        nested_entries,
        refused_reentries,
        ..
    } = *frame;
    let nested_entries = nested_entries.get();
    let refused_reentries = refused_reentries.get();
    let mut state = state.into_inner();
    if nested_entries > 0 || refused_reentries > 0 {
        for loaded in &mut state.jars {
            if nested_entries > 0 {
                loaded
                    .host
                    .note_unknown_game_hook("nested-hooks-entry-during-dispatch");
                loaded.row.game_hooks.failed += nested_entries as usize;
            }
            if refused_reentries > 0 {
                loaded
                    .host
                    .note_unknown_game_hook("game-hook-reentry-during-dispatch");
                loaded.row.game_hooks.failed += refused_reentries as usize;
            }
        }
    }
    state
}

impl HorizonJava {
    /// Dispatch one point. The relaxed/immutable snapshot check precedes every lock or allocation.
    pub fn dispatch_game_hook<'a>(
        &self,
        dispatch: GameDispatch<'a>,
    ) -> Result<DispatchAnswer, DispatchError> {
        if !self.plan_bits.maybe_contains(dispatch.method.0) {
            return Ok(DispatchAnswer::default());
        }
        let owner = self as *const HorizonJava;
        let refused = ACTIVE_FRAMES.with(|frames| {
            let frames = frames.borrow();
            let Some(frame) = frames.last().filter(|frame| frame.0.owner == owner) else {
                return false;
            };
            frame
                .0
                .refused_reentries
                .set(frame.0.refused_reentries.get().saturating_add(1));
            true
        });
        if refused {
            return Err(DispatchError::from_error(
                "Java game-hook re-entry is unsupported in PR 1 (Sunrise dispatch token not available inside Java callbacks)",
            ));
        }
        let _gate = self
            .dispatch_gate
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let state = {
            let mut locked = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *locked)
        };
        ACTIVE_FRAMES.with(|frames| {
            frames
                .borrow_mut()
                .push(ActiveFrameSlot(Box::new(ActiveFrame {
                    owner,
                    state: std::cell::UnsafeCell::new(state),
                    nested_entries: std::cell::Cell::new(0),
                    refused_reentries: std::cell::Cell::new(0),
                })))
        });
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let state = ACTIVE_FRAMES
                .with(|frames| frames.borrow().last().expect("active frame").0.state.get());
            // SAFETY: the frame stays on this thread's stack for the closure.
            unsafe { dispatch_state(&mut *state, dispatch) }
        }));
        let frame = ACTIVE_FRAMES.with(|frames| frames.borrow_mut().pop().expect("active frame"));
        let state = finish_frame(frame);
        *self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = state;
        match outcome {
            Ok(result) => result,
            Err(panic) => resume_unwind(panic),
        }
    }

    pub(crate) fn refresh_game_hook_plan_bits(&self) {
        let state = self.lock();
        let mut words = [0; 4];
        for loaded in &state.jars {
            for hook in loaded.hooks.hooks() {
                set_bit(&mut words, hook);
                for entry in loaded.hooks.entries(hook) {
                    set_target_bit(
                        &mut words,
                        &entry.target.owner,
                        &entry.target.name,
                        &entry.target.descriptor,
                    );
                }
            }
        }
        self.plan_bits.publish(words);
    }
}

struct HookTarget {
    jar: usize,
    hook: String,
    priority: i32,
    order: usize,
}
#[cfg(test)]
static STATE_DISPATCHES: AtomicUsize = AtomicUsize::new(0);
fn dispatch_state(
    state: &mut State,
    dispatch: GameDispatch<'_>,
) -> Result<DispatchAnswer, DispatchError> {
    #[cfg(test)]
    STATE_DISPATCHES.fetch_add(1, Ordering::Relaxed);
    let facts = Arc::clone(&state.server);
    let dedicated = state.dedicated;
    let anchor = match dispatch.phase {
        Phase::Head => "HEAD",
        Phase::Return => "RETURN",
        Phase::Tail => "TAIL",
    };
    let mut targets = Vec::new();
    for (jar, loaded) in state.jars.iter().enumerate() {
        for hook in loaded.hooks.hooks() {
            let entries = loaded.hooks.entries(hook);
            if let Some(entry) = entries.iter().find(|entry| {
                matches_method(
                    dispatch.method.0,
                    hook,
                    &entry.target.owner,
                    &entry.target.name,
                    &entry.target.descriptor,
                ) && (if entry.anchor.is_empty() {
                    "HEAD"
                } else {
                    entry.anchor.as_str()
                }) == anchor
            }) {
                targets.push(HookTarget {
                    jar,
                    hook: hook.to_owned(),
                    priority: entry.priority,
                    order: entry.order,
                });
            }
        }
    }
    // Lower Mixin priority first, then load order, then declaration order. Hook names never enter
    // this comparator, so the BTreeMap cannot silently reorder callbacks.
    targets.sort_by_key(|target| (target.priority, target.jar, target.order));
    let mut answer = DispatchAnswer::default();
    for target in targets {
        let loaded = &mut state.jars[target.jar];
        let registrations = loaded.hooks.for_anchor(&target.hook, anchor);
        if registrations.is_empty() {
            continue;
        }
        let mut context = GameHookContext::new();
        context.set_server_facts(Arc::clone(&facts));
        context.insert(GameObject::server(dedicated));
        let receiver = dispatch
            .receiver
            .clone()
            .map(|object| context.insert(object));
        let result = crate::caught(
            &format!("{} game hook {}", loaded.row.id, target.hook),
            || {
                let callback = loaded.jar.run_game_hook_returning(
                    &registrations,
                    &mut loaded.hook_state,
                    &mut context,
                    &target.hook,
                    receiver,
                    dispatch.args,
                    dispatch.actual.clone(),
                    &mut loaded.host,
                    &crate::runtime::ExecutionBudget::within(JAR_BUDGET),
                );
                #[cfg(test)]
                if let Some((entered, release)) = BLOCK_CALLBACK.lock().unwrap().take() {
                    entered.wait();
                    release.wait();
                }
                #[cfg(test)]
                let nested_target = NESTED_DISPATCH_TARGET.lock().unwrap().take();
                #[cfg(test)]
                if let Some(target) = nested_target {
                    tests::dispatch_nested_for_test(&target);
                }
                callback.inspect(|_| {
                    #[cfg(test)]
                    if FORCE_CALLBACK_PANIC.with(std::cell::Cell::get) {
                        panic!("dispatcher callback panic fixture");
                    }
                })
            },
        );
        let run = match result {
            Ok(Ok(run)) => run,
            Ok(Err(error)) => {
                loaded.row.game_hooks.failed += 1;
                return Err(DispatchError::from_error(error));
            }
            Err(panic) => {
                loaded.row.game_hooks.failed += 1;
                return Err(DispatchError::from_error(format!("panicked: {panic}")));
            }
        };
        loaded.row.game_hooks.ran += run.ran;
        answer.callbacks += run.ran;
        answer.cancelled |= run.cancelled;
        if run.returned.is_some() {
            answer.returned = run.returned;
        }
        for effect in context.take_effects() {
            apply_effect(dispatch.host, effect);
        }
        if run.cancelled {
            break;
        }
    }
    Ok(answer)
}
fn apply_effect(host: &mut dyn ServerHost, effect: GameHookEffect) {
    match effect {
        GameHookEffect::SystemMessage { player, text } => {
            if let Some(target) = host.player(player) {
                let _ = target.tell(host, mod_api::Text::literal(text));
            }
        }
    }
}
impl From<NativeError> for DispatchError {
    fn from(error: NativeError) -> Self {
        Self::from_error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_hooks::tests::{PlayerTickHost, load, stdout};
    use crate::runtime::GameObject;
    use crate::runtime::Value;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Barrier};
    use std::thread;

    struct CountingAllocator;
    static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
    thread_local! { static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) }; }
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            COUNT_ALLOCATIONS.with(|enabled| {
                if enabled.get() {
                    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
                }
            });
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe {
                System.dealloc(ptr, layout);
            }
        }
    }
    #[global_allocator]
    static TEST_ALLOCATOR: CountingAllocator = CountingAllocator;
    fn stop_dispatch(java: &HorizonJava, phase: Phase) -> Result<DispatchAnswer, DispatchError> {
        let mut args = [];
        let mut host = PlayerTickHost {
            spectator: BTreeMap::new(),
        };
        java.dispatch_game_hook(GameDispatch::new(
            MethodId("java_server_stopped"),
            phase,
            Some(GameObject::server(false)),
            &mut args,
            None,
            &mut host,
        ))
    }

    pub(super) fn dispatch_nested_for_test(java: &HorizonJava) {
        stop_dispatch(java, Phase::Tail).unwrap();
    }

    fn destroy_speed_dispatch(
        java: &HorizonJava,
        actual: f64,
    ) -> Result<DispatchAnswer, DispatchError> {
        let mut args = [Value::Ref(Some("game/blockstate/x".into()))];
        let mut host = PlayerTickHost {
            spectator: BTreeMap::new(),
        };
        java.dispatch_game_hook(GameDispatch::new(
            MethodId("java_destroy_speed"),
            Phase::Return,
            Some(GameObject::player(1, "Alex")),
            &mut args,
            Some(Value::Float(actual)),
            &mut host,
        ))
    }

    fn player_tick_dispatch(java: &HorizonJava) -> Result<DispatchAnswer, DispatchError> {
        let mut args = [];
        let mut host = PlayerTickHost {
            spectator: BTreeMap::new(),
        };
        java.dispatch_game_hook(GameDispatch::new(
            MethodId("java_base_player_tick_head"),
            Phase::Head,
            Some(GameObject::player(1, "Alex")),
            &mut args,
            None,
            &mut host,
        ))
    }

    fn stdout_trace() -> Vec<String> {
        crate::runtime::take_test_stdout()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[derive(Debug)]
    struct DispatchObservation {
        callbacks: Option<usize>,
        error: Option<String>,
        failed: usize,
        unknown: usize,
    }

    fn only_lines(trace: Vec<String>, wanted: &[&str]) -> Vec<String> {
        trace
            .into_iter()
            .filter(|line| wanted.contains(&line.as_str()))
            .collect()
    }

    #[test]
    fn method_bitset_has_no_false_negative_for_target_spelling() {
        let bits = PlanBits::default();
        let mut words = [0; 4];
        set_target_bit(&mut words, "a/b/C", "m", "(I)V");
        bits.publish(words);
        assert!(bits.maybe_contains("a/b/C#m (I)V"));
    }
    #[test]
    fn phase_matching_is_exact() {
        assert_eq!(Phase::Head, Phase::Head);
        assert_ne!(Phase::Return, Phase::Tail);
    }
    #[test]
    fn no_plan_returns_before_state_dispatch() {
        struct NoopHost;
        impl mod_api::Host for NoopHost {}
        mod_api::impl_host_areas!(NoopHost);
        impl mod_api::ServerHost for NoopHost {}
        mod_api::impl_server_host_areas!(NoopHost);
        let java = HorizonJava::default();
        let before = STATE_DISPATCHES.load(Ordering::Relaxed);
        let mut args = [Value::Int(7)];
        let mut host = NoopHost;
        let answer = java
            .dispatch_game_hook(GameDispatch::new(
                MethodId("unplanned_method"),
                Phase::Head,
                None,
                &mut args,
                None,
                &mut host,
            ))
            .unwrap();
        assert_eq!(answer, DispatchAnswer::default());
        assert_eq!(STATE_DISPATCHES.load(Ordering::Relaxed), before);
    }
    #[test]
    fn java_reentry_is_explicitly_refused() {
        let error = DispatchError::from_error(
            "Java game-hook re-entry is unsupported in PR 1 (Sunrise dispatch token not available inside Java callbacks)",
        );
        assert!(error.is_reentry_refusal());
    }
    #[test]
    fn refresh_publishes_complete_snapshots() {
        let bits = PlanBits::default();
        let mut words = [0; 4];
        set_bit(&mut words, "live");
        bits.publish(words);
        let old = bits.maybe_contains("live");
        let mut words = [0; 4];
        set_bit(&mut words, "new");
        bits.publish(words);
        assert!(old);
        assert!(bits.maybe_contains("new"));
    }
    #[test]
    fn cross_jar_priority_load_and_declaration_order() {
        let _ = crate::runtime::take_test_stdout();
        let java = HorizonJava::default();
        java.lock().jars = vec![
            load(
                "high",
                "High",
                "@Mixin(value=Player.class, priority=1500) public class High { @Inject(method=\"tick\", at=@At(\"HEAD\")) private void z(CallbackInfo ci) { System.out.println(\"high\"); } }",
            ),
            load(
                "low",
                "Low",
                "@Mixin(value=Player.class, priority=500) public class Low { @Inject(method=\"tick\", at=@At(\"HEAD\")) private void z(CallbackInfo ci) { System.out.println(\"low\"); } }",
            ),
        ];
        java.refresh_game_hook_plan_bits();
        player_tick_dispatch(&java).unwrap();
        assert_eq!(
            only_lines(stdout_trace(), &["low", "high"]),
            ["low", "high"]
        );

        java.lock().jars = vec![
            load(
                "first",
                "First",
                "@Mixin(Player.class) public class First { @Inject(method=\"tick\", at=@At(\"HEAD\")) private void z(CallbackInfo ci) { System.out.println(\"first\"); } }",
            ),
            load(
                "second",
                "Second",
                "@Mixin(Player.class) public class Second { @Inject(method=\"tick\", at=@At(\"HEAD\")) private void z(CallbackInfo ci) { System.out.println(\"second\"); } }",
            ),
        ];
        java.refresh_game_hook_plan_bits();
        player_tick_dispatch(&java).unwrap();
        assert_eq!(
            only_lines(stdout_trace(), &["first", "second"]),
            ["first", "second"]
        );

        java.lock().jars = vec![load(
            "decl",
            "Decl",
            "@Mixin(Player.class) public class Decl { @Inject(method=\"tick\", at=@At(\"HEAD\")) private void a(CallbackInfo ci) { System.out.println(\"a\"); } @Inject(method=\"tick\", at=@At(\"HEAD\")) private void b(CallbackInfo ci) { System.out.println(\"b\"); } }",
        )];
        java.refresh_game_hook_plan_bits();
        player_tick_dispatch(&java).unwrap();
        assert_eq!(only_lines(stdout_trace(), &["a", "b"]), ["a", "b"]);
    }

    #[test]
    fn return_and_tail_phases_are_exact() {
        let java = HorizonJava::default();
        java.lock().jars.push(load("phases", "Phases", "@Mixin(MinecraftServer.class) public class Phases { @Inject(method=\"stopServer\", at=@At(\"RETURN\")) private void r(CallbackInfo ci) { System.out.println(\"return\"); } @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"tail\"); } }"));
        java.refresh_game_hook_plan_bits();
        stop_dispatch(&java, Phase::Return).unwrap();
        stop_dispatch(&java, Phase::Return).unwrap();
        stop_dispatch(&java, Phase::Tail).unwrap();
        assert_eq!(stdout(&mut java.lock(), 0), ["return", "return", "tail"]);
    }

    #[test]
    fn head_cancel_default_and_set_return_value() {
        let java = HorizonJava::default();
        java.lock().jars.push(load("cancel", "Cancel", "@Mixin(BoneMealItem.class) public class Cancel { @Inject(method=\"growCrop\", at=@At(\"HEAD\"), cancellable=true) private void c(ItemStack s, Level l, BlockPos p, CallbackInfoReturnable<Boolean> ci) { ci.cancel(); } }"));
        java.refresh_game_hook_plan_bits();
        let mut args = [
            Value::Ref(Some("game/itemstack/x".into())),
            Value::Ref(Some("game/level/x".into())),
            Value::Ref(Some("game/blockpos/x".into())),
        ];
        let mut host = PlayerTickHost {
            spectator: BTreeMap::new(),
        };
        let answer = java
            .dispatch_game_hook(GameDispatch::new(
                MethodId("java_grow_crop"),
                Phase::Head,
                None,
                &mut args,
                None,
                &mut host,
            ))
            .unwrap();
        assert!(answer.cancelled);
        assert_eq!(answer.returned, Some(Value::Int(0)));
        java.lock().jars.clear();
        java.lock().jars.push(load("set", "Set", "@Mixin(BoneMealItem.class) public class Set { @Inject(method=\"growCrop\", at=@At(\"HEAD\"), cancellable=true) private void c(ItemStack s, Level l, BlockPos p, CallbackInfoReturnable<Boolean> ci) { ci.setReturnValue(true); } }"));
        java.refresh_game_hook_plan_bits();
        let answer = java
            .dispatch_game_hook(GameDispatch::new(
                MethodId("java_grow_crop"),
                Phase::Head,
                None,
                &mut args,
                None,
                &mut host,
            ))
            .unwrap();
        assert!(answer.cancelled);
        assert_eq!(answer.returned, Some(Value::Int(1)));
    }

    #[test]
    fn callback_exception_is_counted_and_stops_the_chain() {
        let java = HorizonJava::default();
        java.lock().jars = vec![
            load(
                "throws",
                "Throws",
                "@Mixin(MinecraftServer.class) public class Throws { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { Integer.parseInt(\"1\", 40); } }",
            ),
            load(
                "after",
                "After",
                "@Mixin(MinecraftServer.class) public class After { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"after\"); } }",
            ),
        ];
        java.refresh_game_hook_plan_bits();
        let error = stop_dispatch(&java, Phase::Tail).unwrap_err();
        assert!(error.to_string().contains("NumberFormatException"));
        let mut state = java.lock();
        assert_eq!(state.jars[0].row.game_hooks.failed, 1);
        assert_eq!(state.jars[1].row.game_hooks.ran, 0);
        assert!(stdout(&mut state, 1).is_empty());
    }

    #[test]
    fn callback_panic_is_counted_after_the_real_jar_callback() {
        let java = HorizonJava::default();
        java.lock().jars.push(load(
            "panic",
            "Panic",
            "@Mixin(MinecraftServer.class) public class Panic { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"before-panic\"); } }",
        ));
        java.refresh_game_hook_plan_bits();
        FORCE_CALLBACK_PANIC.with(|panic| panic.set(true));
        let error = stop_dispatch(&java, Phase::Tail).unwrap_err();
        FORCE_CALLBACK_PANIC.with(|panic| panic.set(false));
        assert!(
            error
                .to_string()
                .contains("dispatcher callback panic fixture")
        );
        let mut state = java.lock();
        assert_eq!(state.jars[0].row.game_hooks.failed, 1);
        assert!(stdout(&mut state, 0).contains(&"before-panic".to_owned()));
    }

    #[test]
    fn non_cancellable_cancel_is_a_counted_cancellation_exception() {
        let java = HorizonJava::default();
        java.lock().jars = vec![
            load(
                "badcancel",
                "BadCancel",
                "@Mixin(Player.class) public class BadCancel { @Inject(method=\"getDestroySpeed\", at=@At(\"RETURN\")) private void r(BlockState s, CallbackInfoReturnable<Float> ci) { ci.setReturnValue(2.0f); } }",
            ),
            load(
                "aftercancel",
                "AfterCancel",
                "@Mixin(Player.class) public class AfterCancel { @Inject(method=\"getDestroySpeed\", at=@At(\"RETURN\")) private void r(BlockState s, CallbackInfoReturnable<Float> ci) { System.out.println(\"after\"); } }",
            ),
        ];
        java.refresh_game_hook_plan_bits();
        let error = destroy_speed_dispatch(&java, 1.5).unwrap_err();
        assert!(error.to_string().contains("CancellationException"));
        let mut state = java.lock();
        assert_eq!(state.jars[0].row.game_hooks.failed, 1);
        assert_eq!(state.jars[1].row.game_hooks.ran, 0);
        assert!(stdout(&mut state, 1).is_empty());
    }

    #[test]
    fn return_tail_real_result_and_last_override_win() {
        let java = HorizonJava::default();
        java.lock().jars = vec![
            load(
                "ret",
                "Ret",
                "@Mixin(Player.class) public class Ret { @Inject(method=\"getDestroySpeed\", at=@At(\"RETURN\"), cancellable=true) private void r(BlockState s, CallbackInfoReturnable<Float> ci) { System.out.println(\"seen\"+ci.getReturnValue()); ci.setReturnValue(2.0f); } }",
            ),
            load(
                "tail",
                "Tail",
                "@Mixin(Player.class) public class Tail { @Inject(method=\"getDestroySpeed\", at=@At(\"TAIL\"), cancellable=true) private void t(BlockState s, CallbackInfoReturnable<Float> ci) { ci.setReturnValue(3.0f); } }",
            ),
        ];
        java.refresh_game_hook_plan_bits();
        let mut args = [Value::Ref(Some("game/blockstate/x".into()))];
        let mut host = PlayerTickHost {
            spectator: BTreeMap::new(),
        };
        let answer = java
            .dispatch_game_hook(GameDispatch::new(
                MethodId("java_destroy_speed"),
                Phase::Return,
                Some(GameObject::player(1, "Alex")),
                &mut args,
                Some(Value::Float(1.5)),
                &mut host,
            ))
            .unwrap();
        assert_eq!(answer.returned, Some(Value::Float(2.0)));
        let answer = java
            .dispatch_game_hook(GameDispatch::new(
                MethodId("java_destroy_speed"),
                Phase::Tail,
                Some(GameObject::player(1, "Alex")),
                &mut args,
                Some(Value::Float(1.5)),
                &mut host,
            ))
            .unwrap();
        assert_eq!(answer.returned, Some(Value::Float(3.0)));
        assert!(
            stdout(&mut java.lock(), 0)
                .iter()
                .any(|line| line.contains("seen1.5"))
        );
    }

    #[test]
    fn unload_clears_plan_snapshot_and_stops_callbacks() {
        let java = HorizonJava::default();
        java.lock().jars.push(load("unload", "Unload", "@Mixin(MinecraftServer.class) public class Unload { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"ran\"); } }"));
        java.refresh_game_hook_plan_bits();
        stop_dispatch(&java, Phase::Tail).unwrap();
        java.lock().jars.clear();
        java.refresh_game_hook_plan_bits();
        assert_eq!(stop_dispatch(&java, Phase::Tail).unwrap().callbacks, 0);
    }

    #[test]
    fn dispatch_state_is_not_visible_empty_to_other_hooks() {
        let java = Arc::new(HorizonJava::default());
        java.lock().jars.push(load(
            "blocked",
            "Blocked",
            "@Mixin(MinecraftServer.class) public class Blocked { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"ran\"); } }",
        ));
        java.refresh_game_hook_plan_bits();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        *BLOCK_CALLBACK.lock().unwrap() = Some((Arc::clone(&entered), Arc::clone(&release)));
        let java_a = Arc::clone(&java);
        let dispatch = thread::spawn(move || stop_dispatch(&java_a, Phase::Tail).unwrap());
        entered.wait();

        let started = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let java_b = Arc::clone(&java);
        let started_b = Arc::clone(&started);
        let finished_b = Arc::clone(&finished);
        let entry = thread::spawn(move || {
            started_b.store(true, Ordering::Release);
            java_b.ensure_jars("26.3");
            let mut state = java_b.lock();
            assert_eq!(state.jars.len(), 1);
            state.jars[0].row.game_hooks.unlisted += 1;
            finished_b.store(true, Ordering::Release);
        });
        while !started.load(Ordering::Acquire) {
            thread::yield_now();
        }
        assert!(!finished.load(Ordering::Acquire));
        release.wait();
        assert_eq!(dispatch.join().unwrap().callbacks, 1);
        entry.join().unwrap();
        assert!(finished.load(Ordering::Acquire));
        let state = java.lock();
        assert_eq!(state.jars.len(), 1);
        assert_eq!(state.jars[0].row.game_hooks.unlisted, 1);
    }

    #[test]
    fn active_frame_serves_hooks_and_refresh_without_deadlock() {
        let java = HorizonJava::default();
        java.lock().jars.push(load(
            "nested_entry",
            "NestedEntry",
            "@Mixin(MinecraftServer.class) public class NestedEntry { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"ran\"); } }",
        ));
        java.refresh_game_hook_plan_bits();
        let state = {
            let mut locked = java.lock();
            std::mem::take(&mut *locked)
        };
        let owner = &java as *const HorizonJava;
        ACTIVE_FRAMES.with(|frames| {
            frames
                .borrow_mut()
                .push(ActiveFrameSlot(Box::new(ActiveFrame {
                    owner,
                    state: std::cell::UnsafeCell::new(state),
                    nested_entries: std::cell::Cell::new(0),
                    refused_reentries: std::cell::Cell::new(0),
                })))
        });
        java.ensure_jars("26.3");
        java.refresh_game_hook_plan_bits();
        let frame = ACTIVE_FRAMES.with(|frames| frames.borrow_mut().pop().unwrap());
        let mut state = finish_frame(frame);
        assert_eq!(state.jars.len(), 1);
        assert!(state.jars[0].host.unknown_host_call_count() >= 1);
        assert!(state.jars[0].row.game_hooks.failed >= 1);
        state.jars[0].row.game_hooks.unlisted += 1;
        *java.lock() = state;
        assert!(java.plan_bits.maybe_contains("java_server_stopped"));
        assert_eq!(stop_dispatch(&java, Phase::Tail).unwrap().callbacks, 1);
    }

    #[test]
    fn different_instance_nested_dispatch_keeps_frame_pointer_stable() {
        let first = Arc::new(HorizonJava::default());
        first.lock().jars.push(load(
            "outer_instance",
            "OuterInstance",
            "@Mixin(MinecraftServer.class) public class OuterInstance { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"outer\"); } }",
        ));
        first.refresh_game_hook_plan_bits();
        let second = Arc::new(HorizonJava::default());
        second.lock().jars.push(load(
            "inner_instance",
            "InnerInstance",
            "@Mixin(MinecraftServer.class) public class InnerInstance { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"inner\"); } }",
        ));
        second.refresh_game_hook_plan_bits();
        *NESTED_DISPATCH_TARGET.lock().unwrap() = Some(Arc::clone(&second));
        assert_eq!(stop_dispatch(&first, Phase::Tail).unwrap().callbacks, 1);
        assert_eq!(stdout(&mut first.lock(), 0), ["outer"]);
        assert_eq!(stdout(&mut second.lock(), 0), ["inner"]);
    }

    #[test]
    fn refusal_is_counted_for_reentry_and_live_plans_survive_refresh_races() {
        let java = HorizonJava::default();
        java.lock().jars.push(load("reentry", "Reentry", "@Mixin(MinecraftServer.class) public class Reentry { @Inject(method=\"stopServer\", at=@At(\"TAIL\")) private void t(CallbackInfo ci) { System.out.println(\"ran\"); } }"));
        java.refresh_game_hook_plan_bits();
        let state = {
            let mut locked = java.lock();
            std::mem::take(&mut *locked)
        };
        let owner = &java as *const HorizonJava;
        ACTIVE_FRAMES.with(|frames| {
            frames
                .borrow_mut()
                .push(ActiveFrameSlot(Box::new(ActiveFrame {
                    owner,
                    state: std::cell::UnsafeCell::new(state),
                    nested_entries: std::cell::Cell::new(0),
                    refused_reentries: std::cell::Cell::new(0),
                })))
        });
        let refused = stop_dispatch(&java, Phase::Tail).unwrap_err();
        let frame = ACTIVE_FRAMES.with(|frames| frames.borrow_mut().pop().unwrap());
        let state = finish_frame(frame);
        assert!(refused.is_reentry_refusal());
        assert_eq!(state.jars[0].host.unknown_host_call_count(), 1);
        assert_eq!(state.jars[0].row.game_hooks.failed, 1);
        *java.lock() = state;
        let java = Arc::new(java);
        let refresh = Arc::clone(&java);
        let refresher = thread::spawn(move || {
            for _ in 0..20 {
                refresh.refresh_game_hook_plan_bits();
            }
        });
        let workers = (0..2)
            .map(|_| {
                let java = Arc::clone(&java);
                thread::spawn(move || {
                    (0..10)
                        .map(|_| {
                            let outcome = stop_dispatch(&java, Phase::Tail);
                            let (callbacks, error) = match outcome {
                                Ok(answer) => (Some(answer.callbacks), None),
                                Err(error) => (None, Some(error.to_string())),
                            };
                            let (failed, unknown) = java
                                .lock()
                                .jars
                                .first()
                                .map(|loaded| {
                                    (
                                        loaded.row.game_hooks.failed,
                                        loaded.host.unknown_host_call_count(),
                                    )
                                })
                                .unwrap_or_default();
                            DispatchObservation {
                                callbacks,
                                error,
                                failed,
                                unknown,
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        refresher.join().unwrap();
        let observations = workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        let counts = observations
            .iter()
            .map(|observation| observation.callbacks)
            .collect::<Vec<_>>();
        if observations
            .iter()
            .any(|observation| observation.callbacks != Some(1))
        {
            let mismatches = observations
                .iter()
                .filter(|observation| observation.callbacks != Some(1))
                .map(|observation| {
                    format!(
                        "callbacks={:?}, error={:?}, failed={}, unknown={}",
                        observation.callbacks,
                        observation.error,
                        observation.failed,
                        observation.unknown
                    )
                })
                .collect::<Vec<_>>();
            panic!("dispatch counts={counts:?}; mismatches={mismatches:?}");
        }
    }

    #[test]
    fn no_plan_dispatch_is_allocation_free() {
        struct NoopHost;
        impl mod_api::Host for NoopHost {}
        mod_api::impl_host_areas!(NoopHost);
        impl mod_api::ServerHost for NoopHost {}
        mod_api::impl_server_host_areas!(NoopHost);
        let java = HorizonJava::default();
        let mut args = [Value::Int(1)];
        let mut host = NoopHost;
        ALLOCATIONS.store(0, Ordering::Relaxed);
        COUNT_ALLOCATIONS.with(|enabled| enabled.set(true));
        let answer = java
            .dispatch_game_hook(GameDispatch::new(
                MethodId("missing"),
                Phase::Head,
                None,
                &mut args,
                None,
                &mut host,
            ))
            .unwrap();
        COUNT_ALLOCATIONS.with(|enabled| enabled.set(false));
        assert_eq!(answer, DispatchAnswer::default());
        assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    }
}
