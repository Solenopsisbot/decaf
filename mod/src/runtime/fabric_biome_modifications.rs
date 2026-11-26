//! Fabric's `BiomeModifications.create` and `BiomeModification.add` (#476).
//!
//! What `fabric-biome-api-v1` 20.2.0 (Fabric API 0.166.2) does, read from its bytecode:
//!
//! * `BiomeModifications.create(id)` is `new BiomeModification(id)`: a new object on every call,
//!   and the id is stored without a check, so `create(null)` returns one too. `BiomeModification`
//!   overrides nothing of `Object`: identity `equals`, and `hashCode`/`toString` print the
//!   identity hash (counted).
//! * Both `add(phase, selector, modifier)` overloads (a `Consumer<BiomeModificationContext>` or a
//!   `BiConsumer<BiomeSelectionContext, BiomeModificationContext>`) call
//!   `BiomeModificationImpl.INSTANCE.addModifier(id, phase, selector, modifier)`, which
//!   `requireNonNull`s the selector and the modifier, appends a `ModifierRecord` and returns, and
//!   then `add` returns `this`. The phase is stored unread.
//!
//! The records take effect only when a server loads its biomes, long after a mod's init: nothing
//! a mod can read back during init depends on them, so the host keeps them (rooted, so the
//! collector never frees a selector or modifier lambda) and answers `this`.
//!
//! Counted rather than answered: a null selector or modifier. Fabric throws
//! `NullPointerException`, but a counted call answers null too, so the null may stand for a value
//! Java never saw as null (`open_options.rs` counts a null operand the same way).

use crate::runtime::host_ref::{HostKind, HostRef};
use crate::runtime::{MethodRef, NativeError, Value};
use std::collections::BTreeMap;

pub(crate) const MODIFICATION: &str = "net/fabricmc/fabric/api/biome/v1/BiomeModification";
const MODIFICATIONS: &str = "net/fabricmc/fabric/api/biome/v1/BiomeModifications";
const CREATE: &str =
    "(Lnet/minecraft/resources/Identifier;)Lnet/fabricmc/fabric/api/biome/v1/BiomeModification;";
const ADD: &str = "(Lnet/fabricmc/fabric/api/biome/v1/ModificationPhase;Ljava/util/function/Predicate;Ljava/util/function/Consumer;)Lnet/fabricmc/fabric/api/biome/v1/BiomeModification;";
const ADD_CONTEXT: &str = "(Lnet/fabricmc/fabric/api/biome/v1/ModificationPhase;Ljava/util/function/Predicate;Ljava/util/function/BiConsumer;)Lnet/fabricmc/fabric/api/biome/v1/BiomeModification;";

/// One modification `create` made: its id and, in `add` order, each modifier's
/// (phase, selector, modifier).
#[derive(Debug, Clone)]
struct Modification {
    id: Value,
    modifiers: Vec<[Value; 3]>,
}

/// One host's biome modifications (Fabric's `BiomeModificationImpl.INSTANCE.modifiers`, grouped
/// by the modification that added them).
#[derive(Debug, Default, Clone)]
pub(crate) struct State {
    /// Handle key → the modification.
    modifications: BTreeMap<String, Modification>,
    next: u64,
}

impl State {
    /// The values the host holds: collector roots (a selector or modifier is a jar lambda).
    pub(crate) fn values(&self) -> impl Iterator<Item = &Value> {
        self.modifications
            .values()
            .flat_map(|m| std::iter::once(&m.id).chain(m.modifiers.iter().flatten()))
    }
}

/// `BiomeModifications.create`; `None` leaves any other call to the rest of the host.
pub(crate) fn static_call(
    state: &mut State,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str())
        != (MODIFICATIONS, "create", CREATE)
    {
        return None;
    }
    let [id] = args else { return None };
    state.next += 1;
    // `/`, not `#`: a handle of its own kind, numbered by this model; `<text>#<digits>` spells a jar
    // object's allocation key, whose number the collector reads as the heap's (#669 review).
    let key = format!("bm/{}", state.next);
    state.modifications.insert(
        key.clone(),
        Modification {
            id: id.clone(),
            modifiers: Vec::new(),
        },
    );
    Some(Ok(Value::Host(HostRef::new(
        HostKind::FabricBiomeModification,
        key,
    ))))
}

/// Both `add` overloads on a modification `create` made; `None` leaves the call counted.
pub(crate) fn instance_call(
    state: &mut State,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let Value::Host(handle) = receiver else {
        return None;
    };
    if handle.kind() != HostKind::FabricBiomeModification
        || r.name != "add"
        || !matches!(r.descriptor.as_str(), ADD | ADD_CONTEXT)
    {
        return None;
    }
    let [phase, selector, modifier] = args else {
        return None;
    };
    if matches!(selector, Value::Ref(None)) || matches!(modifier, Value::Ref(None)) {
        return None;
    }
    let modification = state.modifications.get_mut(handle.key())?;
    modification
        .modifiers
        .push([phase.clone(), selector.clone(), modifier.clone()]);
    Some(Ok(receiver.clone()))
}
