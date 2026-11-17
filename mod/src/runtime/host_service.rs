//! Host singletons the runtime answers by their owner's calls, as handles
//! (#167's review). They used to be plain text (`system-out`, `fml-loader`...)
//! that `is_plain_string` read as a `String`, so a cast of one named
//! `java.lang.String` where Java names the object's class.
//!
//! * [`HostKind::StdStream`]: `System.out` and `System.err`, keyed
//!   `system-out`/`system-err` (the in-memory sink a modelled writer
//!   appends to). In the game they are vanilla's `LoggedPrintStream`s
//!   (`Bootstrap.bootStrap()`'s `wrapStreams`, before any mod runs), or its
//!   `DebugLoggedPrintStream` subclass with debug logging on, so the class is
//!   `ClassRule::Unknown` with `PrintStream` declared: `instanceof` and casts
//!   to `PrintStream` and its supertypes are exact (`(PrintStream) (Object)
//!   System.out` passes), anything that needs the exact class is counted.
//! * [`HostKind::LoaderSingleton`]: NeoForge's (and legacy Forge's) `ModList`,
//!   `FMLLoader`, `LoadingModList` and `ConfigTracker`, keyed by the declared
//!   class. `ModLoadingContext` adds the native thread identity to its key,
//!   matching FML's `ThreadLocal`; its runtime class is still unknown, so a
//!   cast's message is counted (`ClassRule::Unknown`).

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::Value;
use crate::runtime::host_path::identity_equals;
use crate::runtime::host_ref::{HostKind, HostRef, Methods, instance_of_class};

/// `System.out` (`err` false) or `System.err`.
pub(crate) fn std_stream(err: bool) -> Value {
    let key = if err { "system-err" } else { "system-out" };
    Value::Host(HostRef::new(HostKind::StdStream, key))
}

/// The sink key a standard stream's handle names (`system-out`), for the
/// writer model that reads receivers by text.
pub(crate) fn stream_marker(value: &Value) -> Option<&String> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::StdStream => Some(handle.key_text()),
        _ => None,
    }
}

/// The loader classes a [`HostKind::LoaderSingleton`] is keyed by.
const LOADER_CLASSES: &[&str] = &[
    "net/neoforged/fml/ModLoadingContext",
    "net/neoforged/fml/ModList",
    "net/minecraftforge/fml/ModList",
    "net/neoforged/fml/loading/FMLLoader",
    "net/neoforged/fml/loading/LoadingModList",
    "net/neoforged/fml/config/ConfigTracker",
];

/// The loader's one object of `class` (one of [`LOADER_CLASSES`]).
pub(crate) fn loader_singleton(class: &str) -> Value {
    debug_assert!(LOADER_CLASSES.contains(&class), "{class}");
    Value::Host(HostRef::new(HostKind::LoaderSingleton, class))
}

static NEXT_CONTEXT_THREAD: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static CONTEXT_THREAD: Cell<Option<u64>> = const { Cell::new(None) };
}

/// The native thread's stable identity for FML's `ModLoadingContext.context`.
///
/// The bounded interpreter parks Java-created threads instead of running them,
/// so its Rust execution thread is the only thread whose context it can know.
/// A caller that asks from another Rust thread receives a different handle.
pub(crate) fn context_thread_id() -> u64 {
    CONTEXT_THREAD.with(|slot| {
        if let Some(id) = slot.get() {
            return id;
        }
        let id = NEXT_CONTEXT_THREAD.fetch_add(1, Ordering::Relaxed);
        slot.set(Some(id));
        id
    })
}

/// FML's `ThreadLocal.withInitial` context for the current native thread.
pub(crate) fn thread_local_loader_singleton(class: &str) -> Value {
    debug_assert_eq!(class, "net/neoforged/fml/ModLoadingContext");
    Value::Host(HostRef::new(
        HostKind::LoaderSingleton,
        format!("{class}|thread:{}", context_thread_id()),
    ))
}

/// The class a loader singleton's handle is keyed by, for the loader model
/// that reads receivers by text.
pub(crate) fn loader_key(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::LoaderSingleton => Some(
            handle
                .key()
                .split_once("|thread:")
                .map_or(handle.key(), |(class, _)| class),
        ),
        _ => None,
    }
}

/// Whether a `ModLoadingContext` handle belongs to this native thread. A
/// context crossing an unknown thread boundary must stay counted.
pub(crate) fn is_current_context(value: &Value) -> Option<bool> {
    let Value::Host(handle) = value else {
        return None;
    };
    if handle.kind() != HostKind::LoaderSingleton
        || !handle
            .key()
            .starts_with("net/neoforged/fml/ModLoadingContext|thread:")
    {
        return None;
    }
    let id = handle.key().split_once("|thread:")?.1.parse::<u64>().ok()?;
    Some(id == context_thread_id())
}

/// The type a standard stream surely is: a `PrintStream` (in the game,
/// vanilla's `LoggedPrintStream` subclass).
pub(crate) fn std_stream_types(_: &str) -> &'static [&'static str] {
    &["java/io/PrintStream"]
}

/// The type a loader singleton surely is: its key's class.
pub(crate) fn loader_singleton_types(key: &str) -> &'static [&'static str] {
    let key = key.split_once("|thread:").map_or(key, |(class, _)| class);
    LOADER_CLASSES
        .iter()
        .position(|class| *class == key)
        .map_or(&[], |at| &LOADER_CLASSES[at..=at])
}

/// One object per loader key: identity `equals`; `hashCode` and `toString`
/// are `Object`'s identity ones (counted).
pub(crate) const METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| Ok(identity_equals(handle, other)),
    instance_of: instance_of_class,
    crosses_jars: |_, _| true,
    compare_to: None,
};
