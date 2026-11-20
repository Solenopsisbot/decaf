//! Exact, generic vanilla menu/container models used by mod-owned menus.
//!
//! Item-combiner slot geometry is data supplied by the mod's own
//! `ItemCombinerMenuSlotDefinition.Builder` bytecode. Nothing here knows a
//! particular mod or menu type.

use crate::{MethodRef, NativeError, Value, expect_ref};

/// `ItemStack.EMPTY`'s key, as the client surface answers the static (`client_surface.rs`).
const EMPTY_STACK: &str = "client/stack/empty";

/// `ItemStack.EMPTY`: the one typed host value every side answers the static with, so a menu's
/// stacks and the client item model agree on it, and a jar `String` spelling the key never
/// passes for it.
pub(crate) fn empty_stack() -> Value {
    crate::game_object::client_value(EMPTY_STACK.to_owned())
}
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct SlotDef {
    pub(crate) index: i32,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) may_place: Value,
    pub(crate) empty: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Definition {
    pub(crate) inputs: Vec<SlotDef>,
    pub(crate) result: SlotDef,
}

#[derive(Clone, Debug)]
pub(crate) struct Slot {
    index: i32,
    container_slot: i32,
    container: Value,
    x: i32,
    y: i32,
    may_place: Option<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct MenuState {
    pub input: String,
    pub result: String,
    pub slots: String,
    pub(crate) definition: Definition,
    pub(crate) container_id: i32,
    pub(crate) access: Value,
    pub(crate) player: Value,
}

#[derive(Clone, Debug)]
pub enum MenuAction {
    SlotsChanged { menu: Value, container: Value },
    CreateResult { menu: Value },
    Predicate { predicate: Value, stack: Value },
}

#[derive(Clone, Debug)]
pub(crate) enum Object {
    Menu(MenuState),
    Inventory {
        player: Value,
    },
    Builder {
        inputs: Vec<SlotDef>,
        result: Option<SlotDef>,
    },
    Definition(Definition),
    SlotDefinition(SlotDef),
    Slot(Slot),
    SlotList {
        slots: Vec<String>,
    },
    Container {
        size: i32,
        items: Vec<Value>,
        result: bool,
        owner: Option<String>,
        recipe_used: Option<Value>,
    },
    Predicate(bool),
    Pending(MenuAction),
}

pub(crate) type States = BTreeMap<String, Object>;

pub(crate) fn is_menu_class(owner: &str) -> bool {
    matches!(
        owner,
        "net/minecraft/world/inventory/AbstractContainerMenu"
            | "net/minecraft/world/inventory/ItemCombinerMenu"
            | "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition"
            | "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder"
            | "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition"
            | "net/minecraft/world/SimpleContainer"
            | "net/minecraft/world/inventory/ResultContainer"
            | "net/minecraft/world/inventory/RecipeCraftingHolder"
            | "net/minecraft/world/Container"
            | "net/minecraft/world/inventory/Slot"
            | "net/minecraft/world/inventory/ContainerLevelAccess"
            | "java/util/function/Predicate"
    )
}

fn marker(key: impl Into<String>) -> Value {
    let key = key.into();
    if crate::jar_object::key_class(&key).is_some() {
        crate::jar_object::stored(&key)
    } else {
        Value::Ref(Some(key))
    }
}

fn next_key(states: &States, prefix: &str) -> String {
    let mut n = states.len();
    loop {
        let key = format!("{prefix}#{n}");
        if !states.contains_key(&key) {
            return key;
        }
        n += 1;
    }
}

/// Resolve fields javac names through a jar subclass to the vanilla declaration.
pub(crate) fn field_owner(owner: &str, name: &str, descriptor: &str) -> Option<&'static str> {
    if owner != "net/minecraft/world/inventory/ItemCombinerMenu" {
        return None;
    }
    Some(match (name, descriptor) {
        ("inputSlots", "Lnet/minecraft/world/Container;")
        | ("resultSlots", "Lnet/minecraft/world/inventory/ResultContainer;")
        | ("access", "Lnet/minecraft/world/inventory/ContainerLevelAccess;")
        | ("player", "Lnet/minecraft/world/entity/player/Player;")
        | ("resultSlotIndex", "I") => "net/minecraft/world/inventory/ItemCombinerMenu",
        ("slots", "Lnet/minecraft/core/NonNullList;") | ("containerId", "I") => {
            "net/minecraft/world/inventory/AbstractContainerMenu"
        }
        _ => return None,
    })
}

pub(crate) fn field(states: &States, receiver: &Value, r: &MethodRef) -> Option<Value> {
    let key = expect_ref(receiver)?;
    let object = states.get(key)?;
    match (object, r.owner.as_str(), r.name.as_str()) {
        (Object::Menu(state), _, "inputSlots") => Some(marker(state.input.clone())),
        (Object::Menu(state), _, "resultSlots") => Some(marker(state.result.clone())),
        (Object::Menu(state), _, "slots") => Some(marker(state.slots.clone())),
        (Object::Menu(state), _, "containerId") => Some(Value::Int(state.container_id)),
        (Object::Menu(state), _, "access") => Some(state.access.clone()),
        (Object::Menu(state), _, "player") => Some(state.player.clone()),
        (Object::Menu(state), _, "resultSlotIndex") => {
            Some(Value::Int(state.definition.inputs.len() as i32))
        }
        (Object::Inventory { player }, "net/minecraft/world/entity/player/Inventory", "player") => {
            Some(player.clone())
        }
        (Object::Slot(slot), "net/minecraft/world/inventory/Slot", "container") => {
            Some(slot.container.clone())
        }
        (Object::Slot(slot), "net/minecraft/world/inventory/Slot", "x") => Some(Value::Int(slot.x)),
        (Object::Slot(slot), "net/minecraft/world/inventory/Slot", "y") => Some(Value::Int(slot.y)),
        (Object::Slot(slot), "net/minecraft/world/inventory/Slot", "index") => {
            Some(Value::Int(slot.index))
        }
        _ => None,
    }
}

pub(crate) fn static_call(
    states: &mut States,
    r: &MethodRef,
    _args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition"
        && r.name == "create"
    {
        let key = next_key(
            states,
            "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
        );
        states.insert(
            key.clone(),
            Object::Builder {
                inputs: Vec::new(),
                result: Some(SlotDef {
                    index: 0,
                    x: 0,
                    y: 0,
                    may_place: marker("menu/predicate/empty-result"),
                    empty: true,
                }),
            },
        );
        states.insert(
            "menu/predicate/empty-result".into(),
            Object::Predicate(true),
        );
        return Some(Ok(marker(key)));
    }
    None
}

pub(crate) fn take_action(states: &mut States) -> Option<MenuAction> {
    let key = states
        .iter()
        .find_map(|(key, object)| matches!(object, Object::Pending(_)).then_some(key.clone()))?;
    match states.remove(&key) {
        Some(Object::Pending(action)) => Some(action),
        _ => None,
    }
}

pub(crate) fn keyed_text(states: &States, key: &str) -> Option<String> {
    states.get(key).map(|object| format!("{object:?}"))
}

pub(crate) fn call(
    states: &mut States,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let key = expect_ref(receiver)?.to_owned();

    if r.owner == "net/minecraft/world/inventory/ContainerLevelAccess"
        && key == "container-level-access/null"
    {
        if r.name == "execute" && r.descriptor == "(Ljava/util/function/BiConsumer;)V" {
            return Some(Ok(Value::Void));
        }
        if r.name == "evaluate"
            && r.descriptor == "(Ljava/util/function/BiFunction;)Ljava/util/Optional;"
        {
            return Some(Ok(crate::empty_optional()));
        }
        if r.name == "evaluate"
            && r.descriptor
                == "(Ljava/util/function/BiFunction;Ljava/lang/Object;)Ljava/lang/Object;"
        {
            // ContainerLevelAccess's default overload is `evaluate(f).orElse(default)`.
            // NULL's Optional is empty, so the default is the exact answer.
            return Some(Ok(args.get(1)?.clone()));
        }
        return None;
    }

    // The builder and immutable definition are ordinary objects in vanilla.
    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition" {
        let Some(Object::Definition(definition)) = states.get(&key).cloned() else {
            return None;
        };
        return Some(match r.name.as_str() {
            "getNumOfInputSlots" => Ok(Value::Int(definition.inputs.len() as i32)),
            "getResultSlotIndex" => Ok(Value::Int(definition.result.index)),
            "getResultSlot" => {
                let out = next_key(
                    states,
                    "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition",
                );
                states.insert(out.clone(), Object::SlotDefinition(definition.result));
                Ok(marker(out))
            }
            "getSlot" => {
                let Some(Value::Int(index)) = args.first() else {
                    return None;
                };
                let slot = definition.inputs.get(*index as usize).cloned()?;
                let out = next_key(
                    states,
                    "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition",
                );
                states.insert(out.clone(), Object::SlotDefinition(slot));
                Ok(marker(out))
            }
            "getSlots" => Ok(marker(format!("menu/{key}/definition-slots"))),
            _ => return None,
        });
    }
    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition" {
        let Some(Object::SlotDefinition(slot)) = states.get(&key) else {
            return None;
        };
        return Some(match r.name.as_str() {
            "slotIndex" => Ok(Value::Int(slot.index)),
            "x" => Ok(Value::Int(slot.x)),
            "y" => Ok(Value::Int(slot.y)),
            "mayPlace" => Ok(slot.may_place.clone()),
            _ => return None,
        });
    }

    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder" {
        match r.name.as_str() {
            "<init>" => {
                states.insert(
                    "menu/predicate/empty-result".into(),
                    Object::Predicate(true),
                );
                states.insert(
                    key,
                    Object::Builder {
                        inputs: Vec::new(),
                        result: Some(SlotDef {
                            index: 0,
                            x: 0,
                            y: 0,
                            may_place: marker("menu/predicate/empty-result"),
                            empty: true,
                        }),
                    },
                );
                return Some(Ok(Value::Void));
            }
            "withSlot" => {
                let Some(Object::Builder { inputs, .. }) = states.get_mut(&key) else {
                    return None;
                };
                let [Value::Int(index), Value::Int(x), Value::Int(y), ..] = args else {
                    return None;
                };
                inputs.push(SlotDef {
                    index: *index,
                    x: *x,
                    y: *y,
                    may_place: args[3].clone(),
                    empty: false,
                });
                return Some(Ok(receiver.clone()));
            }
            "withResultSlot" => {
                let Some(Object::Builder { result, .. }) = states.get_mut(&key) else {
                    return None;
                };
                let [Value::Int(index), Value::Int(x), Value::Int(y), ..] = args else {
                    return None;
                };
                *result = Some(SlotDef {
                    index: *index,
                    x: *x,
                    y: *y,
                    may_place: marker("menu/predicate/result"),
                    empty: false,
                });
                states.insert("menu/predicate/result".into(), Object::Predicate(false));
                return Some(Ok(receiver.clone()));
            }
            "build" => {
                let Some(Object::Builder { inputs, result }) = states.get(&key).cloned() else {
                    return None;
                };
                if inputs
                    .iter()
                    .enumerate()
                    .any(|(i, slot)| slot.index != i as i32)
                {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some("Expected input slots to have continous indexes".into()),
                        frame: None,
                    }));
                }
                let result = result?;
                if result.index != inputs.len() as i32 {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some(
                            "Expected result slot index to follow last input slot".into(),
                        ),
                        frame: None,
                    }));
                }
                if inputs.is_empty() || result.empty {
                    return Some(Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some("Need to define both inputSlots and resultSlot".into()),
                        frame: None,
                    }));
                }
                let definition = Definition { inputs, result };
                let out = next_key(
                    states,
                    "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition",
                );
                states.insert(out.clone(), Object::Definition(definition));
                return Some(Ok(marker(out)));
            }
            _ => {}
        }
    }

    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenu" && r.name == "<init>" {
        let def_key = args.get(4).and_then(expect_ref)?;
        let Some(Object::Definition(definition)) = states.get(def_key).cloned() else {
            return None;
        };
        let inventory = args.get(2)?.clone();
        let player = expect_ref(&inventory)
            .and_then(|inventory| match states.get(inventory) {
                Some(Object::Inventory { player }) => Some(player.clone()),
                _ => None,
            })
            .unwrap_or(Value::Ref(None));
        // 26.3: AbstractContainerMenu first, then ResultContainer, access/player,
        // createContainer(input count), result index, input slots, result slot,
        // addStandardInventorySlots(inventory, 8, 84). The hotbar is 58px lower.
        let slots = next_key(states, "net/minecraft/core/NonNullList");
        states.insert(slots.clone(), Object::SlotList { slots: Vec::new() });
        let result = next_key(states, "net/minecraft/world/inventory/ItemCombinerMenu$1");
        states.insert(
            result.clone(),
            Object::Container {
                size: 1,
                items: vec![empty_stack()],
                result: true,
                owner: Some(key.clone()),
                recipe_used: None,
            },
        );
        let input = next_key(states, "net/minecraft/world/inventory/ItemCombinerMenu$4");
        states.insert(
            input.clone(),
            Object::Container {
                size: definition.inputs.len() as i32,
                items: vec![empty_stack(); definition.inputs.len()],
                result: false,
                owner: Some(key.clone()),
                recipe_used: None,
            },
        );
        let mut layout = definition
            .inputs
            .iter()
            .map(|slot| Slot {
                index: slot.index,
                container_slot: slot.index,
                container: marker(input.clone()),
                x: slot.x,
                y: slot.y,
                may_place: Some(slot.may_place.clone()),
            })
            .collect::<Vec<_>>();
        layout.push(Slot {
            index: definition.inputs.len() as i32,
            container_slot: definition.result.index,
            container: marker(result.clone()),
            x: definition.result.x,
            y: definition.result.y,
            may_place: Some(definition.result.may_place.clone()),
        });
        for row in 0..3 {
            for column in 0..9 {
                layout.push(Slot {
                    index: layout.len() as i32,
                    container_slot: 9 * (row + 1) + column,
                    container: inventory.clone(),
                    x: 8 + 18 * column,
                    y: 84 + 18 * row,
                    may_place: None,
                });
            }
        }
        for column in 0..9 {
            layout.push(Slot {
                index: layout.len() as i32,
                container_slot: column,
                container: inventory.clone(),
                x: 8 + 18 * column,
                y: 142,
                may_place: None,
            });
        }
        let mut slot_keys = Vec::new();
        for (position, slot) in layout.into_iter().enumerate() {
            let class = if position < definition.inputs.len() {
                "net/minecraft/world/inventory/ItemCombinerMenu$2"
            } else if position == definition.inputs.len() {
                "net/minecraft/world/inventory/ItemCombinerMenu$3"
            } else {
                "net/minecraft/world/inventory/Slot"
            };
            let slot_key = next_key(states, class);
            states.insert(slot_key.clone(), Object::Slot(slot));
            slot_keys.push(slot_key);
        }
        states.insert(slots.clone(), Object::SlotList { slots: slot_keys });
        states.insert(
            key.clone(),
            Object::Menu(MenuState {
                input,
                result,
                slots,
                definition,
                container_id: match args.get(1)? {
                    Value::Int(id) => *id,
                    _ => return None,
                },
                access: args.get(3)?.clone(),
                player,
            }),
        );
        return Some(Ok(Value::Void));
    }
    if r.owner == "net/minecraft/world/inventory/ItemCombinerMenu" && r.name == "createContainer" {
        let Some(Object::Menu(state)) = states.get(&key) else {
            return None;
        };
        let input_key = state.input.clone();
        let size = args
            .first()
            .and_then(|v| {
                if let Value::Int(i) = v {
                    Some(*i)
                } else {
                    None
                }
            })
            .unwrap_or(0)
            .max(0);
        states.insert(
            input_key.clone(),
            Object::Container {
                size,
                items: vec![empty_stack(); size as usize],
                result: false,
                owner: Some(key.clone()),
                recipe_used: None,
            },
        );
        return Some(Ok(marker(input_key)));
    }
    if r.owner == "net/minecraft/world/entity/player/Inventory"
        && r.name == "<init>"
        && r.descriptor
            == "(Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/EntityEquipment;)V"
    {
        states.insert(
            key,
            Object::Inventory {
                player: args.first()?.clone(),
            },
        );
        return Some(Ok(Value::Void));
    }
    if r.owner == "net/minecraft/world/inventory/Slot"
        && r.name == "getContainerSlot"
        && r.descriptor == "()I"
        && let Some(Object::Slot(slot)) = states.get(&key)
    {
        return Some(Ok(Value::Int(slot.container_slot)));
    }
    if let Some(Object::Slot(slot)) = states.get(&key)
        && (r.owner == "net/minecraft/world/inventory/Slot"
            || r.owner == "net/minecraft/world/inventory/ItemCombinerMenu$2"
            || r.owner == "net/minecraft/world/inventory/ItemCombinerMenu$3")
        && r.name == "mayPlace"
        && r.descriptor == "(Lnet/minecraft/world/item/ItemStack;)Z"
    {
        if let Some(predicate) = &slot.may_place {
            let pending = next_key(states, "menu/action");
            states.insert(
                pending,
                Object::Pending(MenuAction::Predicate {
                    predicate: predicate.clone(),
                    stack: args.first().cloned().unwrap_or(Value::Ref(None)),
                }),
            );
            return Some(Ok(Value::Int(0)));
        }
        return Some(Ok(Value::Int(1)));
    }
    if r.owner == "net/minecraft/world/inventory/Slot" && r.name == "<init>" {
        let [container, Value::Int(index), Value::Int(x), Value::Int(y)] = args else {
            return None;
        };
        states.insert(
            key,
            Object::Slot(Slot {
                index: 0,
                container_slot: *index,
                container: container.clone(),
                x: *x,
                y: *y,
                may_place: None,
            }),
        );
        return Some(Ok(Value::Void));
    }
    if r.owner == "net/minecraft/world/SimpleContainer" && r.name == "<init>" {
        let size = args
            .first()
            .and_then(|v| {
                if let Value::Int(i) = v {
                    Some(*i)
                } else {
                    None
                }
            })
            .unwrap_or(0)
            .max(0);
        states.insert(
            key,
            Object::Container {
                size,
                items: vec![empty_stack(); size as usize],
                result: false,
                owner: None,
                recipe_used: None,
            },
        );
        return Some(Ok(Value::Void));
    }
    if r.owner == "net/minecraft/world/inventory/ResultContainer" && r.name == "<init>" {
        states.insert(
            key,
            Object::Container {
                size: 1,
                items: vec![empty_stack()],
                result: true,
                owner: None,
                recipe_used: None,
            },
        );
        return Some(Ok(Value::Void));
    }

    if let Some(Object::Menu(state)) = states.get(&key)
        && matches!(
            r.owner.as_str(),
            "net/minecraft/world/inventory/AbstractContainerMenu"
                | "net/minecraft/world/inventory/ItemCombinerMenu"
        )
    {
        return Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("getSlot", "(I)Lnet/minecraft/world/inventory/Slot;") => {
                let Some(Value::Int(index)) = args.first() else {
                    return None;
                };
                let Some(Object::SlotList { slots }) = states.get(&state.slots) else {
                    return None;
                };
                Ok(marker(slots.get(*index as usize)?.clone()))
            }
            ("getResultSlot", "()I") => Ok(Value::Int(state.definition.inputs.len() as i32)),
            // These private vanilla methods are also used by the host-side oracle test.
            ("getInventorySlotStart", "()I") => {
                Ok(Value::Int(state.definition.inputs.len() as i32 + 1))
            }
            ("getInventorySlotEnd" | "getUseRowStart", "()I") => {
                Ok(Value::Int(state.definition.inputs.len() as i32 + 28))
            }
            ("getUseRowEnd" | "getRowEnd", "()I") => {
                Ok(Value::Int(state.definition.inputs.len() as i32 + 37))
            }
            ("stillValid", "(Lnet/minecraft/world/entity/player/Player;)Z")
                if expect_ref(&state.access)
                    .is_some_and(|key| key == "container-level-access/null") =>
            {
                // ItemCombinerMenu passes Boolean.TRUE as the default to
                // ContainerLevelAccess.evaluate(BiFunction, Object).
                Ok(Value::Int(1))
            }
            _ => return None,
        });
    }
    if let Some(Object::SlotList { slots }) = states.get(&key)
        && matches!(
            r.owner.as_str(),
            "java/util/List" | "java/util/Collection" | "net/minecraft/core/NonNullList"
        )
    {
        return Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("size", "()I") => Ok(Value::Int(slots.len() as i32)),
            ("get", "(I)Ljava/lang/Object;") => {
                let Some(Value::Int(index)) = args.first() else {
                    return None;
                };
                Ok(marker(slots.get(*index as usize)?.clone()))
            }
            _ => return None,
        });
    }

    // Containers are shared by SimpleContainer, ResultContainer and the
    // Container interface. The recipe manager remains deliberately counted.
    if let Some(Object::Container {
        size,
        items,
        result,
        owner,
        recipe_used,
    }) = states.get_mut(&key)
        && (r.owner == "net/minecraft/world/Container"
            || r.owner == "net/minecraft/world/SimpleContainer"
            || r.owner == "net/minecraft/world/inventory/ResultContainer"
            || r.owner == "net/minecraft/world/inventory/RecipeCraftingHolder")
    {
        let index = args
            .first()
            .and_then(|v| {
                if let Value::Int(i) = v {
                    Some(*i as usize)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let mut pending_action = None;
        let answer = match r.name.as_str() {
            "getContainerSize" => Ok(Value::Int(*size)),
            "isEmpty" => {
                // Only `ItemStack.EMPTY` is known to be empty. A Java `null` here (a mod's
                // `setItem(i, null)`, which this model refuses below) would make
                // `ItemStack.isEmpty()` throw, so it is never answered as empty.
                if items.iter().all(|v| *v == empty_stack()) {
                    Ok(Value::Int(1))
                } else {
                    // ItemStack.isEmpty is observable per element.  An arbitrary
                    // marker does not tell us whether its count/item is empty.
                    return None;
                }
            }
            "getItem" => {
                let position = if *result { 0 } else { index };
                let Some(item) = items.get(position).cloned() else {
                    if !*result && position >= *size as usize {
                        return Some(Ok(empty_stack()));
                    }
                    return None;
                };
                Ok(item)
            }
            "setItem" => {
                let value = args.get(1)?;
                // A null stack: 26.3's containers store into a `NonNullList`, whose `set`
                // rejects null with a `NullPointerException` (and `SimpleContainer` calls
                // `limitSize` on it first). Counted rather than stored or answered.
                if matches!(value, Value::Ref(None)) {
                    return None;
                }
                // SimpleContainer clamps through getMaxStackSize(stack) and
                // ItemStack.limitSize(max).  Only EMPTY is fully known here;
                // refuse to mutate for an unknown stack so Java's clamp cannot
                // be silently skipped. ResultContainer has no clamp in 26.3.
                if !*result && *value != empty_stack() {
                    return None;
                }
                let item = items.get_mut(index)?;
                *item = value.clone();
                if !*result && let Some(menu) = owner.as_ref() {
                    pending_action = Some(MenuAction::SlotsChanged {
                        menu: marker(menu.clone()),
                        container: marker(key.clone()),
                    });
                }
                Ok(Value::Void)
            }
            "setChanged" => {
                if !*result && let Some(menu) = owner.as_ref() {
                    pending_action = Some(MenuAction::SlotsChanged {
                        menu: marker(menu.clone()),
                        container: marker(key.clone()),
                    });
                }
                Ok(Value::Void)
            }
            "clearContent" => {
                items.clear();
                if !*result && let Some(menu) = owner.as_ref() {
                    pending_action = Some(MenuAction::SlotsChanged {
                        menu: marker(menu.clone()),
                        container: marker(key.clone()),
                    });
                }
                Ok(Value::Void)
            }
            "setRecipeUsed" => {
                *recipe_used = args.first().cloned();
                Ok(Value::Void)
            }
            "getRecipeUsed" => Ok(recipe_used.clone().unwrap_or(Value::Ref(None))),
            _ => {
                return None;
            }
        };
        if let Some(action) = pending_action {
            let pending = next_key(states, "menu/action");
            states.insert(pending, Object::Pending(action));
        }
        return Some(answer);
    }
    if let Some(Object::Predicate(answer)) = states.get(&key)
        && r.owner == "java/util/function/Predicate"
        && r.name == "test"
        && r.descriptor == "(Ljava/lang/Object;)Z"
    {
        return Some(Ok(Value::Int(i32::from(*answer))));
    }
    if let Some(Object::Menu(state)) = states.get(&key)
        && r.owner == "net/minecraft/world/inventory/ItemCombinerMenu"
        && r.name == "slotsChanged"
        && r.descriptor == "(Lnet/minecraft/world/Container;)V"
        && args
            .first()
            .and_then(expect_ref)
            .is_some_and(|input| input == &state.input)
    {
        let pending = next_key(states, "menu/action");
        states.insert(
            pending,
            Object::Pending(MenuAction::CreateResult {
                menu: marker(key.clone()),
            }),
        );
        return Some(Ok(Value::Void));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn method(owner: &str, name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    #[test]
    fn definition_drives_smithing_like_ranges() {
        let mut states = States::new();
        let builder = static_call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition",
                "create",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &[],
        )
        .unwrap()
        .unwrap();
        let receiver = builder.clone();
        for (index, x, y) in [(0, 8, 48), (1, 26, 48), (2, 44, 48)] {
            call(&mut states, &method("net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder", "withSlot", "(IIILjava/util/function/Predicate;)Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;"), &receiver, &[Value::Int(index), Value::Int(x), Value::Int(y), Value::Ref(None)]).unwrap().unwrap();
        }
        call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "withResultSlot",
                "(III)Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &receiver,
            &[Value::Int(3), Value::Int(98), Value::Int(48)],
        )
        .unwrap()
        .unwrap();
        let definition = call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "build",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;",
            ),
            &receiver,
            &[],
        )
        .unwrap()
        .unwrap();
        let menu = Value::Ref(Some("menu/test".into()));
        call(&mut states, &method("net/minecraft/world/inventory/ItemCombinerMenu", "<init>", "(Lnet/minecraft/world/inventory/MenuType;ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/inventory/ContainerLevelAccess;Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;)V"), &menu, &[Value::Ref(None), Value::Int(0), Value::Ref(None), Value::Ref(Some("container-level-access/null".into())), definition]).unwrap().unwrap();
        for (name, expected) in [
            ("getResultSlot", 3),
            ("getInventorySlotStart", 4),
            ("getInventorySlotEnd", 31),
            ("getUseRowStart", 31),
            ("getUseRowEnd", 40),
            ("getRowEnd", 40),
        ] {
            assert_eq!(
                call(
                    &mut states,
                    &method(
                        "net/minecraft/world/inventory/AbstractContainerMenu",
                        name,
                        "()I"
                    ),
                    &menu,
                    &[]
                )
                .unwrap()
                .unwrap(),
                Value::Int(expected)
            );
        }
        assert_eq!(
            match states.get("menu/test") {
                Some(Object::Menu(state)) => match states.get(&state.slots) {
                    Some(Object::SlotList { slots }) => slots.len(),
                    _ => 0,
                },
                _ => 0,
            },
            40
        );

        let Object::Menu(state) = states.get("menu/test").unwrap() else {
            panic!("constructor did not make a menu");
        };
        let Object::SlotList { slots } = states.get(&state.slots).unwrap() else {
            panic!("constructor did not make the slot list");
        };
        let mut observed = String::new();
        writeln!(observed, "inputCount={}", state.definition.inputs.len()).unwrap();
        for key in &slots[..state.definition.inputs.len()] {
            let Object::Slot(slot) = states.get(key).unwrap() else {
                panic!("input is not a slot");
            };
            writeln!(
                observed,
                "slot index={} x={} y={}",
                slot.index, slot.x, slot.y
            )
            .unwrap();
        }
        let Object::Slot(result) = states.get(&slots[state.definition.inputs.len()]).unwrap()
        else {
            panic!("result is not a slot");
        };
        writeln!(
            observed,
            "result index={} x={} y={}",
            result.index, result.x, result.y
        )
        .unwrap();
        for (name, _) in [
            ("resultSlot", 0),
            ("inventoryStart", 0),
            ("inventoryEnd", 0),
            ("useRowStart", 0),
            ("useRowEnd", 0),
        ] {
            let value = call(
                &mut states,
                &method(
                    "net/minecraft/world/inventory/AbstractContainerMenu",
                    match name {
                        "resultSlot" => "getResultSlot",
                        "inventoryStart" => "getInventorySlotStart",
                        "inventoryEnd" => "getInventorySlotEnd",
                        "useRowStart" => "getUseRowStart",
                        _ => "getUseRowEnd",
                    },
                    "()I",
                ),
                &menu,
                &[],
            )
            .unwrap()
            .unwrap();
            let Value::Int(value) = value else {
                panic!("range is not an integer");
            };
            writeln!(observed, "{name}={value}").unwrap();
        }
        assert_eq!(observed, include_str!("menu_models_oracle.txt"));
    }

    #[test]
    fn builder_reproduces_vanilla_validation_messages() {
        let mut states = States::new();
        let builder = static_call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition",
                "create",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &[],
        )
        .unwrap()
        .unwrap();
        let empty = call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "build",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;",
            ),
            &builder,
            &[],
        )
        .unwrap()
        .unwrap_err();
        assert!(
            matches!(empty, NativeError::UncaughtException { message: Some(message), .. } if message == "Need to define both inputSlots and resultSlot")
        );

        call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "withSlot",
                "(IIILjava/util/function/Predicate;)Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &builder,
            &[Value::Int(2), Value::Int(0), Value::Int(0), Value::Ref(None)],
        )
        .unwrap()
        .unwrap();
        call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "withResultSlot",
                "(III)Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder;",
            ),
            &builder,
            &[Value::Int(1), Value::Int(0), Value::Int(0)],
        )
        .unwrap()
        .unwrap();
        let indexes = call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder",
                "build",
                "()Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;",
            ),
            &builder,
            &[],
        )
        .unwrap()
        .unwrap_err();
        assert!(
            matches!(indexes, NativeError::UncaughtException { message: Some(message), .. } if message == "Expected input slots to have continous indexes")
        );
    }

    #[test]
    fn a_null_stack_is_counted_not_stored() {
        // 26.3's containers keep stacks in a `NonNullList`, whose `set` rejects null with a
        // `NullPointerException`; the model counts the call and leaves the container as it was.
        let mut states = States::new();
        for (owner, init, key) in [
            ("net/minecraft/world/SimpleContainer", "(I)V", "simple/null"),
            (
                "net/minecraft/world/inventory/ResultContainer",
                "()V",
                "result/null",
            ),
        ] {
            let container = Value::Ref(Some(key.into()));
            let args: &[Value] = if init == "(I)V" {
                &[Value::Int(1)]
            } else {
                &[]
            };
            call(
                &mut states,
                &method(owner, "<init>", init),
                &container,
                args,
            )
            .unwrap()
            .unwrap();
            assert!(
                call(
                    &mut states,
                    &method(owner, "setItem", "(ILnet/minecraft/world/item/ItemStack;)V"),
                    &container,
                    &[Value::Int(0), Value::Ref(None)],
                )
                .is_none()
            );
            assert_eq!(
                call(
                    &mut states,
                    &method(owner, "isEmpty", "()Z"),
                    &container,
                    &[]
                )
                .unwrap()
                .unwrap(),
                Value::Int(1)
            );
        }
    }

    #[test]
    fn null_access_uses_the_evaluate_default_and_unknown_stacks_count() {
        let mut states = States::new();
        let access = Value::Ref(Some("container-level-access/null".into()));
        let fallback = Value::Int(1);
        assert_eq!(
            call(
                &mut states,
                &method(
                    "net/minecraft/world/inventory/ContainerLevelAccess",
                    "evaluate",
                    "(Ljava/util/function/BiFunction;Ljava/lang/Object;)Ljava/lang/Object;",
                ),
                &access,
                &[Value::Ref(None), fallback.clone()],
            )
            .unwrap()
            .unwrap(),
            fallback
        );

        let simple = Value::Ref(Some("simple/test".into()));
        call(
            &mut states,
            &method("net/minecraft/world/SimpleContainer", "<init>", "(I)V"),
            &simple,
            &[Value::Int(1)],
        )
        .unwrap()
        .unwrap();
        assert!(
            call(
                &mut states,
                &method(
                    "net/minecraft/world/SimpleContainer",
                    "setItem",
                    "(ILnet/minecraft/world/item/ItemStack;)V",
                ),
                &simple,
                &[Value::Int(0), Value::Ref(Some("stack/unknown".into()))],
            )
            .is_none()
        );
        assert_eq!(
            call(
                &mut states,
                &method("net/minecraft/world/SimpleContainer", "isEmpty", "()Z",),
                &simple,
                &[],
            )
            .unwrap()
            .unwrap(),
            Value::Int(1)
        );

        let result = Value::Ref(Some("result/test".into()));
        call(
            &mut states,
            &method(
                "net/minecraft/world/inventory/ResultContainer",
                "<init>",
                "()V",
            ),
            &result,
            &[],
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            call(
                &mut states,
                &method(
                    "net/minecraft/world/inventory/ResultContainer",
                    "setItem",
                    "(ILnet/minecraft/world/item/ItemStack;)V",
                ),
                &result,
                &[Value::Int(0), Value::Ref(Some("stack/unknown".into()))],
            )
            .unwrap()
            .unwrap(),
            Value::Void
        );
        assert!(
            call(
                &mut states,
                &method(
                    "net/minecraft/world/inventory/ResultContainer",
                    "isEmpty",
                    "()Z",
                ),
                &result,
                &[],
            )
            .is_none()
        );
    }
}
