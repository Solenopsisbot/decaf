//! Whether a jar `MenuType`'s slot layout is fixed by its structure, so that one construction
//! (the bake's, with a synthetic player) gives the layout of every menu it ever builds.
//!
//! An embedder declares a menu type's layout before any menu opens (Sunrise's
//! `data/<ns>/sunrise/menu/*.json`), but a `MenuSupplier` is code: it may choose its slots from
//! the inventory it is handed, the player's state, the level, or a mod's own static state. So
//! the layout is declared only when an allow-list proves it can't vary, read off the bytecode
//! (never off one run):
//!
//! - the supplier is a constructor reference (`X::new`) or a lambda whose body only constructs
//!   one jar class;
//! - that constructor chain is straight-line jar code (no branch, no exception handler, no
//!   `athrow`) that reaches 26.3's `ItemCombinerMenu.<init>(MenuType, int, Inventory,
//!   ContainerLevelAccess, ItemCombinerMenuSlotDefinition)` exactly once, through `this(...)`
//!   and `super(...)` calls only;
//! - before that call the chain runs no jar code but a straight-line slot-definition builder
//!   (`ItemCombinerMenuSlotDefinition.create().withSlot(...).withResultSlot(...).build()`, in a
//!   private static helper or inline) whose slot indices are constants, plus lambdas,
//!   constants, static reads and vanilla calls that build arguments;
//! - after it, no jar code runs and no menu method but `addDataSlot` (whose straight-line calls
//!   give a fixed data slot count);
//! - no jar class in the chain overrides a method `ItemCombinerMenu.<init>` lays slots out with
//!   (`addSlot`, `addStandardInventorySlots`, `addInventoryExtendedSlots`,
//!   `addInventoryHotbarSlots`) or `addDataSlot` / `addDataSlots`.
//!
//! `ItemCombinerMenu.<init>` itself (26.3 offsets 0-75) lays out `getNumOfInputSlots` input slots
//! and the result slot from the definition, then the player's 36 inventory slots, through its
//! private `createInputSlots` / `createResultSlot` / `createContainer` and the methods above: with
//! a constant definition and none of those overridden, its layout is the same for every player,
//! inventory and level. Anything else is refused with a reason, never guessed.

use crate::runtime::{ClassFile, Host, MethodRef, NativeError, Runtime, Value, callback_marker, expect_ref};

const ICM: &str = "net/minecraft/world/inventory/ItemCombinerMenu";
const ACM: &str = "net/minecraft/world/inventory/AbstractContainerMenu";
const ICM_INIT: &str = "(Lnet/minecraft/world/inventory/MenuType;ILnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/world/inventory/ContainerLevelAccess;Lnet/minecraft/world/inventory/ItemCombinerMenuSlotDefinition;)V";
const DEFINITION: &str = "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition";
const BUILDER: &str = "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$Builder";

/// Methods `ItemCombinerMenu.<init>` lays slots out with, and the data slot adders: a jar
/// override of any of them changes the layout.
const LAYOUT_METHODS: &[(&str, &str)] = &[
    (
        "addSlot",
        "(Lnet/minecraft/world/inventory/Slot;)Lnet/minecraft/world/inventory/Slot;",
    ),
    (
        "addStandardInventorySlots",
        "(Lnet/minecraft/world/Container;II)V",
    ),
    (
        "addInventoryExtendedSlots",
        "(Lnet/minecraft/world/Container;II)V",
    ),
    (
        "addInventoryHotbarSlots",
        "(Lnet/minecraft/world/Container;II)V",
    ),
    (
        "addDataSlot",
        "(Lnet/minecraft/world/inventory/DataSlot;)Lnet/minecraft/world/inventory/DataSlot;",
    ),
    (
        "addDataSlots",
        "(Lnet/minecraft/world/inventory/ContainerData;)V",
    ),
];

/// How deep `this(...)` / `super(...)` chains are followed.
const MAX_CHAIN: usize = 16;

/// Whether the layout of the menus `menu_type`'s supplier builds is fixed by their structure
/// (module docs): `Ok(())`, or why it can't be declared.
pub(crate) fn check<H: Host>(rt: &Runtime<'_, H>, menu_type: &Value) -> Result<(), String> {
    let supplier = rt
        .host
        .as_std_host()
        .and_then(
            |host| match expect_ref(menu_type).and_then(|key| host.menu_states.get(key)) {
                Some(crate::runtime::menu_models::Object::MenuType { constructor, .. }) => {
                    Some(constructor.clone())
                }
                _ => None,
            },
        )
        .ok_or("it is not a MenuType Decaf models")?;
    let lambda = callback_marker::lambda_of(&supplier)
        .ok_or("its MenuSupplier is not a constructor reference or a lambda")?;
    if !lambda.captures.is_empty() {
        return Err("its MenuSupplier lambda captures values".to_owned());
    }
    let (class, descriptor) = match lambda.ref_kind {
        // `X::new`.
        Some(8) => (
            lambda.target.owner.clone(),
            lambda.target.descriptor.clone(),
        ),
        // A lambda body that constructs one class.
        Some(6) => constructed_by_lambda(rt, &lambda.target)?,
        _ => {
            return Err(
                "its MenuSupplier is not a constructor reference or a static lambda".to_owned(),
            );
        }
    };
    check_overrides(rt, &class)?;
    if constructor(rt, &class, &descriptor, 0)? {
        Ok(())
    } else {
        Err(format!(
            "{class}'s constructor does not reach ItemCombinerMenu's"
        ))
    }
}

/// The jar class a static lambda body constructs and the constructor it calls: the body may only
/// load its arguments, push constants, construct that one object and return it.
fn constructed_by_lambda<H: Host>(
    rt: &Runtime<'_, H>,
    target: &MethodRef,
) -> Result<(String, String), String> {
    let (file, code) = method_code(rt, &target.owner, &target.name, &target.descriptor)?;
    let mut built = None;
    let mut news = 0;
    for (pc, op) in instructions(&code)? {
        match op {
            0xbb => news += 1,
            0xb7 => {
                let r = cp_method(file, &code, pc)?;
                if r.name != "<init>" || rt.class(&r.owner).is_none() || built.is_some() {
                    return Err(format!(
                        "its MenuSupplier lambda calls {}.{} besides one jar constructor",
                        r.owner, r.name
                    ));
                }
                built = Some((r.owner, r.descriptor));
            }
            0xb0 | 0x59 | 0x01..=0x11 | 0x15..=0x2d => {}
            _ => {
                return Err(format!(
                    "its MenuSupplier lambda does more than construct a menu (opcode {op:#04x})"
                ));
            }
        }
    }
    match (built, news) {
        (Some(built), 1) => Ok(built),
        _ => Err("its MenuSupplier lambda does not construct exactly one menu".to_owned()),
    }
}

/// No jar class from `class` up to `ItemCombinerMenu` overrides a layout method.
fn check_overrides<H: Host>(rt: &Runtime<'_, H>, class: &str) -> Result<(), String> {
    let mut current = class.to_owned();
    for _ in 0..MAX_CHAIN {
        let Some(file) = rt.class(&current) else {
            return if current == ICM {
                Ok(())
            } else {
                Err(format!(
                    "{class} is not an ItemCombinerMenu ({current} is not a jar class)"
                ))
            };
        };
        for (name, descriptor) in LAYOUT_METHODS {
            if file.method(name, descriptor).is_some() {
                return Err(format!("{current} overrides {name}, which lays slots out"));
            }
        }
        current = file.super_name.clone();
    }
    Err(format!("{class}'s superclass chain is too deep"))
}

/// Check constructor `class.<init>descriptor`: `Ok(true)` when it reaches the combiner's
/// constructor, `Ok(false)` when it returns without, `Err` for anything not allowed.
fn constructor<H: Host>(
    rt: &Runtime<'_, H>,
    class: &str,
    descriptor: &str,
    depth: usize,
) -> Result<bool, String> {
    if depth > MAX_CHAIN {
        return Err(format!("{class}'s constructor chain is too deep"));
    }
    let (file, code) = method_code(rt, class, "<init>", descriptor)?;
    let mut reached = false;
    // `new` instructions whose `<init>` hasn't run yet: an `invokespecial <init>` with none
    // pending is this object's own `this(...)` / `super(...)`.
    let mut pending = 0usize;
    let ops = instructions(&code)?;
    for (index, &(pc, op)) in ops.iter().enumerate() {
        match op {
            0xbb => pending += 1,
            0xb7 => {
                let r = cp_method(file, &code, pc)?;
                if r.name == "<init>" && pending > 0 {
                    pending -= 1;
                    if rt.class(&r.owner).is_some() {
                        return Err(format!(
                            "{class}'s constructor constructs jar class {}",
                            r.owner
                        ));
                    }
                    if is_menu_class(&r.owner) {
                        return Err(format!("{class}'s constructor constructs another menu"));
                    }
                } else if r.name == "<init>" {
                    if reached {
                        return Err(format!("{class}'s constructor initializes itself twice"));
                    }
                    if r.owner != class && r.owner != file.super_name {
                        return Err(format!("{class}'s constructor calls {}.<init>", r.owner));
                    }
                    reached = if r.owner == ICM {
                        if r.descriptor != ICM_INIT {
                            return Err(format!(
                                "{class} calls an ItemCombinerMenu constructor other than 26.3's"
                            ));
                        }
                        // The definition (the last argument, pushed last) comes straight from
                        // a checked builder chain or helper, not from a field or a local that
                        // static state may have set.
                        let definition = index
                            .checked_sub(1)
                            .map(|at| ops[at])
                            .map(|(at, op)| (op, cp_method(file, &code, at)));
                        let from_builder = match definition {
                            Some((0xb6, Ok(call)))
                                if call.owner == BUILDER && call.name == "build" =>
                            {
                                builder_chain(file, &code, &ops, index - 1)?.is_some()
                            }
                            Some((0xb8, Ok(call))) => {
                                rt.class(&call.owner).is_some()
                                    && call.descriptor.ends_with(&format!("L{DEFINITION};"))
                            }
                            _ => false,
                        };
                        if !from_builder {
                            return Err(format!(
                                "{class}'s slot definition doesn't come straight from a builder"
                            ));
                        }
                        true
                    } else if rt.class(&r.owner).is_some() {
                        constructor(rt, &r.owner, &r.descriptor, depth + 1)?
                    } else {
                        return Err(format!(
                            "{class}'s superclass {} is not ItemCombinerMenu",
                            r.owner
                        ));
                    };
                    if !reached {
                        return Err(format!(
                            "{}'s constructor does not reach ItemCombinerMenu's",
                            r.owner
                        ));
                    }
                } else {
                    // `super.m()` or a private method of the menu.
                    return Err(format!(
                        "{class}'s constructor calls {}.{}",
                        r.owner, r.name
                    ));
                }
            }
            0xb6 | 0xb9 | 0xb8 => {
                let r = cp_method(file, &code, pc)?;
                // javac names the receiver's static type: `this.addDataSlot(...)` is
                // `demo/X.addDataSlot`, which a jar class may only inherit from vanilla.
                let declared = declaring_jar_class(rt, &r);
                let jar = declared.is_some();
                let inherited_menu_method = !jar && rt.class(&r.owner).is_some();
                if !reached {
                    if is_builder_call(&r) {
                        check_builder_call(&r, &ops, index, &code)?;
                    } else if op == 0xb8
                        && jar
                        && r.descriptor.ends_with(&format!("L{DEFINITION};"))
                    {
                        definition_helper(rt, &r)?;
                    } else if jar || inherited_menu_method || is_menu_class(&r.owner) {
                        return Err(format!(
                            "{class}'s constructor calls {}.{} before its slots are laid out",
                            r.owner, r.name
                        ));
                    }
                } else if jar {
                    return Err(format!(
                        "{class}'s constructor calls {}.{} after its slots are laid out",
                        r.owner, r.name
                    ));
                } else if (inherited_menu_method || is_menu_class(&r.owner))
                    && r.name != "addDataSlot"
                {
                    return Err(format!(
                        "{class}'s constructor calls menu method {}",
                        r.name
                    ));
                }
            }
            0xba => {}
            0x99..=0xab | 0xc6..=0xc9 | 0xbf => {
                return Err(format!("{class}'s constructor branches or throws"));
            }
            _ => {}
        }
    }
    Ok(reached)
}

/// A jar static method returning a slot definition: its whole body is one builder chain,
/// `create()...build()` then `areturn` (no field, local or other call can supply the definition).
fn definition_helper<H: Host>(rt: &Runtime<'_, H>, r: &MethodRef) -> Result<(), String> {
    let (file, code) = method_code(rt, &r.owner, &r.name, &r.descriptor)?;
    let ops = instructions(&code)?;
    let shaped = ops.len() >= 3 && ops[ops.len() - 1].1 == 0xb0;
    let start = if shaped {
        builder_chain(file, &code, &ops, ops.len() - 2)?
    } else {
        None
    };
    if start != Some(0) {
        return Err(format!(
            "{}.{} is not one slot-definition builder chain",
            r.owner, r.name
        ));
    }
    Ok(())
}

/// Check the builder chain ending with the `build()` at `ops[build]`: back to its
/// `ItemCombinerMenuSlotDefinition.create()`, nothing but builder calls (each slot's index a
/// constant), constants, and each `withSlot`'s predicate (a lambda, a static, `null` or a local
/// right before the call), so the builder's receiver is only ever `create()`'s. The index of
/// that `create()`, or `None` when `ops[build]` is no `build()`.
fn builder_chain(
    file: &ClassFile,
    code: &[u8],
    ops: &[(usize, u8)],
    build: usize,
) -> Result<Option<usize>, String> {
    let call = |index: usize| -> Option<MethodRef> {
        let (pc, op) = ops[index];
        matches!(op, 0xb6..=0xb9)
            .then(|| cp_method(file, code, pc).ok())
            .flatten()
    };
    if call(build).is_none_or(|r| r.owner != BUILDER || r.name != "build") {
        return Ok(None);
    }
    let mut index = build;
    loop {
        if index == 0 {
            return Err("a slot-definition builder chain has no create()".to_owned());
        }
        index -= 1;
        let (_, op) = ops[index];
        match op {
            0xb8 => {
                return match call(index) {
                    Some(r) if r.owner == DEFINITION && r.name == "create" => Ok(Some(index)),
                    _ => Err("a slot-definition builder chain calls a static method".to_owned()),
                };
            }
            0xb6 => match call(index) {
                Some(r) if r.owner == BUILDER && r.name != "build" => {
                    check_builder_call(&r, ops, index, code)?;
                }
                _ => {
                    return Err("a slot-definition builder chain calls another method".to_owned());
                }
            },
            0x02..=0x13 => {}
            0xba | 0xb2 | 0x01 | 0x19 | 0x2a..=0x2d => {
                let predicate =
                    call(index + 1).is_some_and(|r| r.owner == BUILDER && r.name == "withSlot");
                if !predicate {
                    return Err(
                        "a slot-definition builder chain reads a value that isn't a slot predicate"
                            .to_owned(),
                    );
                }
            }
            _ => {
                return Err(format!(
                    "a slot-definition builder chain does more than build (opcode {op:#04x})"
                ));
            }
        }
    }
}

fn is_builder_call(r: &MethodRef) -> bool {
    (r.owner == DEFINITION && r.name == "create")
        || (r.owner == BUILDER
            && matches!(r.name.as_str(), "withSlot" | "withResultSlot" | "build"))
}

/// A slot's index (`withSlot(index, x, y, predicate)`, `withResultSlot(index, x, y)`) must be a
/// constant: the three ints are the instructions right before the call (and the predicate's,
/// for `withSlot`), each a constant push.
fn check_builder_call(
    r: &MethodRef,
    ops: &[(usize, u8)],
    index: usize,
    code: &[u8],
) -> Result<(), String> {
    let ints = match r.name.as_str() {
        "withSlot" => {
            let predicate = index
                .checked_sub(1)
                .and_then(|at| ops.get(at))
                .map(|(_, op)| *op);
            // `invokedynamic`, `getstatic`, `aconst_null` or a local.
            if !matches!(predicate, Some(0xba | 0xb2 | 0x01 | 0x19 | 0x2a..=0x2d)) {
                return Err("a slot predicate isn't a lambda, a constant or a local".to_owned());
            }
            index.checked_sub(4)
        }
        "withResultSlot" => index.checked_sub(3),
        _ => return Ok(()),
    };
    let Some(first) = ints else {
        return Err("a slot's position is not constant".to_owned());
    };
    for &(pc, op) in &ops[first..first + 3] {
        let _ = (pc, code);
        // `iconst_*`, `bipush`, `sipush`, `ldc` / `ldc_w` (an int, as the descriptor says).
        if !matches!(op, 0x02..=0x08 | 0x10..=0x13) {
            return Err("a slot's index or position is not a constant".to_owned());
        }
    }
    Ok(())
}

/// The jar class that declares the method `r` resolves to, walking up from `r.owner`; `None`
/// when no jar class declares it (a vanilla method, inherited or not).
fn declaring_jar_class<H: Host>(rt: &Runtime<'_, H>, r: &MethodRef) -> Option<String> {
    let mut current = r.owner.clone();
    for _ in 0..MAX_CHAIN {
        let file = rt.class(&current)?;
        if file.method(&r.name, &r.descriptor).is_some() {
            return Some(current);
        }
        current = file.super_name.clone();
    }
    Some(current)
}

fn is_menu_class(owner: &str) -> bool {
    owner == ICM || owner == ACM
}

/// A jar method's class file and code, with no exception handler.
fn method_code<'a, H: Host>(
    rt: &'a Runtime<'_, H>,
    class: &str,
    name: &str,
    descriptor: &str,
) -> Result<(&'a ClassFile, Vec<u8>), String> {
    let file = rt
        .class(class)
        .ok_or_else(|| format!("{class} is not a jar class"))?;
    let code = file
        .method(name, descriptor)
        .and_then(|method| method.code.clone())
        .ok_or_else(|| format!("{class}.{name}{descriptor} has no code"))?;
    let handlers = file
        .exception_handlers(name, descriptor)
        .map_err(|error| error.to_string())?;
    if !handlers.is_empty() {
        return Err(format!("{class}.{name} has exception handlers"));
    }
    Ok((file, code))
}

/// Every instruction's offset and opcode.
fn instructions(code: &[u8]) -> Result<Vec<(usize, u8)>, String> {
    let mut out = Vec::new();
    let mut pc = 0;
    while pc < code.len() {
        out.push((pc, code[pc]));
        pc +=
            crate::runtime::reference_identity::instruction_length(code, pc).ok_or("unreadable bytecode")?;
    }
    Ok(out)
}

/// The method an invoke instruction at `pc` names.
fn cp_method(file: &ClassFile, code: &[u8], pc: usize) -> Result<MethodRef, String> {
    let index = code
        .get(pc + 1..pc + 3)
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
        .ok_or("truncated invoke")?;
    file.method_ref(index)
        .map_err(|error: NativeError| error.to_string())
}
