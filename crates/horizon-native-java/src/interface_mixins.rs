//! Interface-injection `@Mixin` classes on host-modelled game objects.
//!
//! Real Mixin merges a `@Mixin(Target.class) class M implements I` class onto
//! `Target`: `Target` implements `I` (and its superinterfaces), `M`'s added
//! (`@Unique` or shadowless) fields live on each `Target` instance for that
//! object's lifetime, `M`'s concrete methods dispatch on `Target` with `this`
//! bound to the `Target` instance, and `M`'s no-argument `<init>()V` field
//! initialisers run once per `Target` instance.
//!
//! For host-modelled game classes (`Player`, `ServerPlayer`, `LivingEntity`,
//! `Entity`, `Level`, `ItemStack`, etc.) there is no target bytecode to weave,
//! so this module plans those interface-injection mixins and applies them at
//! runtime over the host's game handles (`game_object.rs`), keeping per-object
//! fields in the resident heap keyed by the handle's canonical identity so they
//! survive calls and GC and are cleared when the object is forgotten.
//!
//! Anything this module cannot apply exactly — targets outside the modelled
//! host surface, unlisted/wrong-side/plugin-gated configs, ambiguous overrides,
//! unrunnable non-`Object` field initialisers, and `@Shadow` reads/writes to
//! unmodelled game members — stays skipped in the weave plan or is counted /
//! refused loudly at runtime, never silently guessed.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;

use crate::checkcast::class_access;
use crate::game_hooks::{
    MIXIN_SHADOW_DESCRIPTOR, MemberAnnotationValues, member_annotation_values, member_annotations,
};
use crate::hook_shapes::mixin_config_gate_with_plugins;
use crate::mixin_plugins::ConfigPlugins;
use crate::{
    Annotation, AnnotationValue, ClassFile, Host, LoadTimeMixinWeave, MethodRef, MixinConfig,
    MixinExtrasKind, MixinSide, MixinSkipReason, NativeError, NativeJar, Runtime, SkippedMixinPlan,
    Value, instance_field_key, mixin_config_gate, mixin_target_owners, resolve_mixin_accessor,
    return_desc,
};

const ACC_PUBLIC: u16 = 0x0001;
const ACC_PRIVATE: u16 = 0x0002;
const ACC_STATIC: u16 = 0x0008;
const ACC_FINAL: u16 = 0x0010;
const ACC_INTERFACE: u16 = 0x0200;
const ACC_ABSTRACT: u16 = 0x0400;
const ACC_SYNTHETIC: u16 = 0x1000;
const ACC_ENUM: u16 = 0x4000;

/// `@Mutable`: on a `@Shadow` field, `MixinApplicatorStandard.mergeShadowFields`
/// clears `ACC_FINAL` from the target field (sponge-mixin 0.17.5, `isDecoratedMutable`).
/// `@Final` has no runtime effect at all: `MixinTargetContext.checkFinal` only logs
/// (and throws only under the `mixin.debug.verify` option), so it is not read here.
const MIXIN_MUTABLE_DESCRIPTOR: &str = "Lorg/spongepowered/asm/mixin/Mutable;";

/// `@Unique`, which an enum-extension mixin's enum constant may not carry.
const MIXIN_UNIQUE_DESCRIPTOR: &str = "Lorg/spongepowered/asm/mixin/Unique;";

/// `@Shadow`'s `prefix()` default (`AnnotationDefault` of `Shadow.prefix` in
/// sponge-mixin 0.17.5).
const DEFAULT_SHADOW_PREFIX: &str = "shadow$";

/// The first class-file major version whose `putstatic` of a `final` field is
/// limited to `<clinit>` (HotSpot `LinkResolver::resolve_field`, JDK 9+).
const FINAL_PUT_CLINIT_ONLY_VERSION: u16 = 53;

const PLANNED_INJECTOR_ANNOTATIONS: [&str; 5] = [
    "Lorg/spongepowered/asm/mixin/injection/Inject;",
    "Lorg/spongepowered/asm/mixin/injection/Redirect;",
    "Lorg/spongepowered/asm/mixin/injection/ModifyArg;",
    "Lorg/spongepowered/asm/mixin/injection/ModifyVariable;",
    "Lorg/spongepowered/asm/mixin/Overwrite;",
];

/// Vanilla classes represented by live host game handles (`game_object::declared_types`
/// and `game_hooks::GameObject::hierarchy`). Interface-injection mixins targeting any
/// class outside this allow-list are skipped as `target-outside-jar` (or
/// `unsupported-family` when jar-local) rather than falsely claimed as applied.
const HOST_MODELLED_GAME_TARGETS: &[&str] = &[
    "com/mojang/authlib/GameProfile",
    "com/mojang/blaze3d/platform/InputConstants$Key",
    "com/mojang/renderpearl/api/pipeline/RenderPipeline",
    "net/minecraft/client/KeyMapping$Category",
    "net/minecraft/client/Minecraft",
    "net/minecraft/client/Options",
    "net/minecraft/client/gui/Font",
    "net/minecraft/client/gui/Gui",
    "net/minecraft/client/multiplayer/ClientLevel",
    "net/minecraft/client/player/AbstractClientPlayer",
    "net/minecraft/client/player/LocalPlayer",
    "net/minecraft/commands/Commands",
    "net/minecraft/core/BlockPos",
    "net/minecraft/core/component/DataComponentGetter",
    "net/minecraft/core/component/DataComponentHolder",
    "net/minecraft/core/component/DataComponentType",
    "net/minecraft/network/chat/Style",
    "net/minecraft/network/chat/TextColor",
    "net/minecraft/server/MinecraftServer",
    "net/minecraft/server/dedicated/DedicatedServer",
    "net/minecraft/server/level/ServerLevel",
    "net/minecraft/server/level/ServerPlayer",
    "net/minecraft/server/players/PlayerList",
    "net/minecraft/sounds/SoundEvent",
    "net/minecraft/world/Container",
    "net/minecraft/world/Nameable",
    "net/minecraft/world/damagesource/DamageSource",
    "net/minecraft/world/effect/MobEffect",
    "net/minecraft/world/entity/Avatar",
    "net/minecraft/world/entity/Entity",
    "net/minecraft/world/entity/LivingEntity",
    "net/minecraft/world/entity/ai/attributes/Attribute",
    "net/minecraft/world/entity/item/ItemEntity",
    "net/minecraft/world/entity/player/Inventory",
    "net/minecraft/world/entity/player/Player",
    "net/minecraft/world/item/Item",
    "net/minecraft/world/item/ItemInstance",
    "net/minecraft/world/item/ItemStack",
    "net/minecraft/world/item/crafting/RecipeAccess",
    "net/minecraft/world/item/crafting/RecipeManager",
    "net/minecraft/world/level/ItemLike",
    "net/minecraft/world/level/Level",
    "net/minecraft/world/level/block/Block",
    "net/minecraft/world/level/block/entity/BlockEntityType",
    "net/minecraft/world/level/block/state/BlockState",
    "net/minecraft/world/level/border/WorldBorder",
    "net/minecraft/world/level/gamerules/GameRule",
    "net/minecraft/world/level/storage/LevelResource",
    "net/minecraft/world/phys/Vec3",
];

/// Whether `target` (internal name) is a host-modelled game class that can carry
/// runtime interface-injection Mixin state.
pub(crate) fn is_host_modelled_game_target(target: &str) -> bool {
    HOST_MODELLED_GAME_TARGETS.binary_search(&target).is_ok()
}

/// One interface-injection `@Mixin` plan applied natively to a host-modelled game class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinInterfacePlan {
    /// Internal name of the `@Mixin` class.
    pub mixin: String,
    /// Internal name of the host-modelled target class.
    pub target: String,
    /// Direct interfaces implemented by the mixin class (internal names).
    pub interfaces: Vec<String>,
    /// Non-`@Shadow`, non-synthetic fields declared on the mixin class.
    pub added_fields: Vec<MethodRef>,
    /// Non-`@Shadow`, non-synthetic concrete non-static methods declared on the mixin class
    /// (excluding `<init>`, `<clinit>`, injectors and `@Accessor`/`@Invoker`).
    pub added_methods: Vec<MethodRef>,
    /// Whether the mixin class has a runnable no-argument `<init>()V` field initialiser.
    pub has_initializer: bool,
}

/// What [`NativeJar::plan_interface_mixins`] reports back to `unwoven_mixin_members`.
#[derive(Debug, Default)]
pub(crate) struct InterfaceWeaveSummary {
    /// Mixin classes with at least one applied [`MixinInterfacePlan`].
    pub(crate) applied_mixins: BTreeSet<String>,
    /// Pure interface-injection mixin classes that were already rejected with an explicit
    /// skip reason (`NotInConfig`, `OtherSide`, `ConfigPlugin`, `TargetOutsideJar`,
    /// `AmbiguousTarget`, etc.) so `unwoven_mixin_members` does not emit a duplicate row.
    pub(crate) accounted_mixins: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StaticShadowField {
    /// `@Mutable`: Mixin strips `ACC_FINAL` from the target field when it merges this shadow.
    is_mutable: bool,
}

/// Resolution of a `getstatic`/`putstatic` on a `@Mixin` class.
///
/// Real Mixin rewrites a shadow's owner to the target class and merges the mixin's
/// code into that target, so a read or write is the target field's own, judged by
/// the JVM's rules for code inside the target class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StaticShadowResolution {
    /// Not a static `@Shadow` (or inherited game static): use the class's own static slot.
    NotShadow,
    /// An unambiguous static `@Shadow` (or inherited game static), resolved to the field
    /// `target` names (its declaring class), and how a `putstatic` through it is judged.
    Resolved {
        target: MethodRef,
        write: StaticShadowWrite,
    },
    /// A static field Mixin moved onto the target (`mergeNewFields`): a mixin's own
    /// non-shadow static, or another mixin's `@Shadow` of it. Mixin rewrites every
    /// `M.field` to `Target.field` (`transformFieldRef`), so the target, the mixin and any
    /// other mixin merged into the same target share this one slot. It lives in the
    /// runtime's static fields (the game never names a field a mod added), holds the JVM
    /// default (or the field's `ConstantValue`) until written, and is initialized by
    /// initializing `init`, the declaring mixin (whose `<clinit>` Mixin merged into the
    /// target's for a jar class).
    Merged {
        slot: MethodRef,
        init: String,
        write: StaticShadowWrite,
    },
    /// The access has no exact answer, so the caller counts or refuses it without
    /// reading or writing any slot: an ambiguous multi-target static `@Shadow`, a shadow
    /// of a mixin real Mixin rejects (`InvalidMixinException`), or a shadow its jar-local
    /// target does not declare as a static field (which Mixin also rejects, unless an
    /// alias or refmap entry this runtime does not follow names another field).
    Unresolved,
}

/// How a `putstatic` through a resolved static `@Shadow` is judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StaticShadowWrite {
    /// The target field is not `final` (or a `@Mutable` shadow cleared it): the write
    /// goes through, whatever `@Final` says.
    Writable,
    /// The jar-local target field is `ACC_FINAL`. The merged code lives in the target
    /// class, so HotSpot's `LinkResolver::resolve_field` allows the write only from its
    /// `<clinit>` (the mixin's static initialiser, which Mixin appends there). Anything
    /// else is `IllegalAccessError`.
    Final {
        /// The class the field reference names after Mixin's rewrite (the target).
        symbolic_owner: String,
        /// The class that declares the field.
        declaring: String,
    },
    /// A host-modelled game field: its flags are not known here. A write is never
    /// applied to the game, so it is counted or refused whatever they are.
    HostField,
}

/// Where a call to a `@Mixin` class's static `@Shadow` method goes
/// ([`Runtime::resolve_static_shadow_method`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StaticShadowCall {
    /// Not a static `@Shadow` method: the method runs as declared.
    NotShadow,
    /// The target method the merged class calls instead: Mixin rewrites `M.method` to
    /// `Target.method` (its `@Shadow` prefix removed) and drops the placeholder body.
    Target(MethodRef),
    /// No exact answer (a rejected mixin, an ambiguous multi-target shadow, or a
    /// jar-local target that does not declare the method as static): counted or refused,
    /// never answered from the placeholder body.
    Unresolved,
}

/// The JVM's verdict on a `putstatic` of a `final` jar field
/// ([`Runtime::judge_final_static_put`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FinalStaticPut {
    /// The update is the declaring class's own `<clinit>`'s: it goes through.
    Allowed,
    /// `IllegalAccessError` with this message.
    Illegal(String),
    /// No exact answer (a pre-Java-9 class file Mixin may upgrade): count or refuse.
    Unknown,
}

/// Detailed member breakdown of one `@Mixin` class.
#[derive(Debug, Clone)]
struct MixinClassDetails {
    mixin: String,
    targets: Vec<String>,
    is_interface: bool,
    interfaces: Vec<String>,
    added_fields: Vec<MethodRef>,
    added_instance_fields: BTreeSet<(String, String)>,
    shadow_fields: BTreeSet<(String, String)>,
    static_shadow_fields: BTreeMap<(String, String), StaticShadowField>,
    added_methods: Vec<MethodRef>,
    added_public_methods: BTreeSet<(String, String)>,
    /// `(name, descriptor)` of every shadow method → the target method name it stands for
    /// (its name with the `@Shadow` prefix removed, `MixinPreProcessorStandard.prepareShadow`).
    shadow_methods: BTreeMap<(String, String), String>,
    has_initializer: bool,
    unrunnable_initializer: bool,
    extends_game_class: bool,
    declared_chain_fields: BTreeSet<(String, String)>,
    /// Why real Mixin rejects this class with `InvalidMixinException` before applying it
    /// to any target, if it does ([`invalid_mixin_reason`]).
    invalid: Option<String>,
}

/// Runtime metadata for one active interface-injection mixin on a host target.
#[derive(Debug, Clone)]
struct ActiveInterfaceMixin {
    mixin: String,
    interfaces: Vec<String>,
    added_public_methods: BTreeSet<(String, String)>,
}

/// Runtime metadata for any `@Mixin` class (active or inactive, interface-injecting or not).
#[derive(Debug, Clone)]
struct MixinClassMeta {
    targets: Vec<String>,
    active_interface_targets: BTreeSet<String>,
    added_instance_fields: BTreeSet<(String, String)>,
    shadow_fields: BTreeSet<(String, String)>,
    static_shadow_fields: BTreeMap<(String, String), StaticShadowField>,
    shadow_methods: BTreeMap<(String, String), String>,
    has_initializer: bool,
    extends_game_class: bool,
    declared_chain_fields: BTreeSet<(String, String)>,
    /// Why real Mixin rejects the class (`InvalidMixinException`); such a class is
    /// applied to no target.
    invalid: Option<String>,
}

/// Cached per-`Runtime` index of active `@Mixin` classes across the primary jar and
/// linked jars, built lazily on first type-check or game-handle dispatch.
#[derive(Debug, Default)]
pub(crate) struct RuntimeMixinIndex {
    /// Target internal name → active `@Mixin` classes targeting it (accessors and classes).
    by_target: BTreeMap<String, Vec<String>>,
    /// Host-modelled target internal name → active interface-injection mixins targeting it.
    interface_by_target: BTreeMap<String, Vec<ActiveInterfaceMixin>>,
    /// Mixin internal name → member classification metadata.
    meta: BTreeMap<String, MixinClassMeta>,
}

/// Non-private, non-static methods declared by `java/lang/Object`.
const JAVA_OBJECT_METHODS: &[(&str, &str)] = &[
    ("clone", "()Ljava/lang/Object;"),
    ("equals", "(Ljava/lang/Object;)Z"),
    ("finalize", "()V"),
    ("getClass", "()Ljava/lang/Class;"),
    ("hashCode", "()I"),
    ("notify", "()V"),
    ("notifyAll", "()V"),
    ("toString", "()Ljava/lang/String;"),
    ("wait", "()V"),
    ("wait", "(J)V"),
    ("wait", "(JI)V"),
];

const VANILLA_HOST_METHODS_TSV: &str = include_str!("vanilla_host_methods.tsv");

#[derive(Default)]
struct VanillaTypeInfo {
    super_name: &'static str,
    interfaces: Vec<&'static str>,
    methods: BTreeSet<(&'static str, &'static str)>,
    /// Every field the type declares, `(name, descriptor)`.
    fields: BTreeSet<(&'static str, &'static str)>,
}

#[derive(Default)]
struct VanillaHostMethods {
    classes: BTreeMap<&'static str, VanillaTypeInfo>,
    interfaces: BTreeMap<&'static str, VanillaTypeInfo>,
}

#[derive(Debug, PartialEq, Eq)]
enum VanillaMethodCheck {
    /// A class in the target's superclass chain (or `java/lang/Object`) declares the
    /// method; under JVMS §5.4.3.3, class methods always win over interface defaults.
    ClassMethod(&'static str),
    /// No class in the superclass chain declares the method, and the full class +
    /// interface closure of the target is present in `vanilla_host_methods.tsv`.
    /// Lists all maximally-specific vanilla/JDK interfaces declaring a `default` method.
    VanillaDefaults(Vec<&'static str>),
    /// A class or interface edge in the target's hierarchy closure is missing from
    /// `vanilla_host_methods.tsv`, so callers must fail closed (count/refuse) rather
    /// than letting an injected interface default win.
    MissingHierarchyEdge(String),
}

impl VanillaHostMethods {
    fn parse(tsv: &'static str) -> Self {
        let mut table = Self::default();
        for line in tsv.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split('\t');
            match parts.next() {
                Some("C") => {
                    let (Some(name), Some(super_name), Some(ifaces)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        continue;
                    };
                    let interfaces = if ifaces.is_empty() {
                        Vec::new()
                    } else {
                        ifaces.split(',').collect()
                    };
                    table.classes.insert(
                        name,
                        VanillaTypeInfo {
                            super_name,
                            interfaces,
                            ..VanillaTypeInfo::default()
                        },
                    );
                }
                Some("I") => {
                    let (Some(name), Some(_empty), Some(ifaces)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        continue;
                    };
                    let interfaces = if ifaces.is_empty() {
                        Vec::new()
                    } else {
                        ifaces.split(',').collect()
                    };
                    table.interfaces.insert(
                        name,
                        VanillaTypeInfo {
                            super_name: "",
                            interfaces,
                            ..VanillaTypeInfo::default()
                        },
                    );
                }
                Some("M") => {
                    let (Some(owner), Some(name), Some(desc)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        continue;
                    };
                    if let Some(info) = table.classes.get_mut(owner) {
                        info.methods.insert((name, desc));
                    } else if let Some(info) = table.interfaces.get_mut(owner) {
                        info.methods.insert((name, desc));
                    }
                }
                Some("F") => {
                    let (Some(owner), Some(name), Some(desc)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        continue;
                    };
                    if let Some(info) = table.classes.get_mut(owner) {
                        info.fields.insert((name, desc));
                    } else if let Some(info) = table.interfaces.get_mut(owner) {
                        info.fields.insert((name, desc));
                    }
                }
                _ => {}
            }
        }
        table
    }

    /// Whether game class `class` itself declares field `name descriptor`, or `None` when
    /// `class` is not in the table.
    fn declares_field(&self, class: &str, name: &str, descriptor: &str) -> Option<bool> {
        let info = self
            .classes
            .get(class)
            .or_else(|| self.interfaces.get(class))?;
        Some(info.fields.contains(&(name, descriptor)))
    }

    /// Whether `iface` equals or transitively extends `wanted` within the vanilla/JDK table.
    fn interface_extends(&self, iface: &str, wanted: &str) -> bool {
        if iface == wanted {
            return true;
        }
        let mut queue: VecDeque<&str> = VecDeque::from([iface]);
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        while let Some(cur) = queue.pop_front() {
            if !seen.insert(cur) {
                continue;
            }
            if let Some(info) = self.interfaces.get(cur) {
                for &parent in &info.interfaces {
                    if parent == wanted {
                        return true;
                    }
                    queue.push_back(parent);
                }
            }
        }
        false
    }

    /// Inspect `declared`'s full vanilla/JDK class and interface hierarchy for
    /// `(name, descriptor)` under JVMS §5.4.3.3 / §5.4.3.4:
    ///
    /// 1. Any non-private, non-static method declared on a class in `declared`'s
    ///    superclass chain (or `java/lang/Object`) wins over every interface default.
    /// 2. Otherwise, walk the full vanilla/JDK interface closure for `default` methods.
    /// 3. If ANY class or interface edge in `declared`'s closure is missing from the
    ///    table, return [`VanillaMethodCheck::MissingHierarchyEdge`] so callers fail
    ///    closed rather than silently skipping the missing edge.
    fn check_target_methods(
        &self,
        declared: &[String],
        name: &str,
        descriptor: &str,
    ) -> VanillaMethodCheck {
        if JAVA_OBJECT_METHODS
            .iter()
            .any(|(m, d)| *m == name && *d == descriptor)
        {
            return VanillaMethodCheck::ClassMethod("java/lang/Object");
        }
        if declared.is_empty() {
            return VanillaMethodCheck::MissingHierarchyEdge("<empty>".to_owned());
        }

        // 1. Walk class hierarchy (most specific first) and collect direct vanilla interfaces.
        let mut seen_classes: BTreeSet<&str> = BTreeSet::new();
        let mut iface_queue: VecDeque<&'static str> = VecDeque::new();
        let mut missing_edge: Option<String> = None;
        for start in declared {
            let mut cur = start.as_str();
            for _ in 0..32 {
                if cur.is_empty() || cur == "java/lang/Object" || !seen_classes.insert(cur) {
                    break;
                }
                if let Some((&key, info)) = self.classes.get_key_value(cur) {
                    if info.methods.contains(&(name, descriptor)) {
                        return VanillaMethodCheck::ClassMethod(key);
                    }
                    iface_queue.extend(info.interfaces.iter().copied());
                    cur = info.super_name;
                } else if let Some((&key, _)) = self.interfaces.get_key_value(cur) {
                    iface_queue.push_back(key);
                    break;
                } else {
                    missing_edge.get_or_insert_with(|| cur.to_owned());
                    break;
                }
            }
        }

        // 2. Walk the target's full vanilla/JDK interface closure for concrete default methods.
        let mut seen_ifaces: BTreeSet<&'static str> = BTreeSet::new();
        let mut default_owners: Vec<&'static str> = Vec::new();
        while let Some(iface) = iface_queue.pop_front() {
            if !seen_ifaces.insert(iface) {
                continue;
            }
            let Some((&key, info)) = self.interfaces.get_key_value(iface) else {
                missing_edge.get_or_insert_with(|| iface.to_owned());
                continue;
            };
            if info.methods.contains(&(name, descriptor)) {
                default_owners.push(key);
            }
            iface_queue.extend(info.interfaces.iter().copied());
        }

        // Fail closed when any class or interface edge in the target's closure is missing
        // from the table rather than silently letting an injected default win.
        if let Some(missing) = missing_edge {
            return VanillaMethodCheck::MissingHierarchyEdge(missing);
        }

        let maximal = default_owners
            .iter()
            .copied()
            .filter(|&owner| {
                !default_owners
                    .iter()
                    .any(|&other| other != owner && self.interface_extends(other, owner))
            })
            .collect();
        VanillaMethodCheck::VanillaDefaults(maximal)
    }
}

/// Whether `iface` equals or transitively extends `wanted`, walking both jar classfiles
/// and `vanilla_host_methods.tsv` (including JDK superinterfaces).
fn iface_extends_with_vanilla<H: Host>(
    runtime: &Runtime<'_, H>,
    table: &VanillaHostMethods,
    iface: &str,
    wanted: &str,
) -> bool {
    if iface == wanted {
        return true;
    }
    if runtime.interface_extends(iface, wanted) || table.interface_extends(iface, wanted) {
        return true;
    }
    let mut queue = VecDeque::from([iface.to_owned()]);
    let mut seen = BTreeSet::new();
    while let Some(cur) = queue.pop_front() {
        if cur == wanted {
            return true;
        }
        if seen.len() >= crate::MAX_SUPERCLASS_WALK || !seen.insert(cur.clone()) {
            continue;
        }
        if let Some(file) = runtime.class(&cur) {
            queue.extend(file.interface_names());
        } else if let Some(info) = table.interfaces.get(cur.as_str()) {
            queue.extend(info.interfaces.iter().map(|s| (*s).to_owned()));
        }
    }
    false
}

fn vanilla_host_methods() -> &'static VanillaHostMethods {
    static TABLE: OnceLock<VanillaHostMethods> = OnceLock::new();
    TABLE.get_or_init(|| VanillaHostMethods::parse(VANILLA_HOST_METHODS_TSV))
}

/// Game class or interface `class` and every supertype it loads with (its superclass
/// chain and every superinterface, `java/lang/Object` left out), by
/// `vanilla_host_methods.tsv`; `None` when the table lacks any edge of that closure, so
/// the answer is not known.
pub(crate) fn vanilla_supertypes(class: &str) -> Option<Vec<&'static str>> {
    let table = vanilla_host_methods();
    let mut out: Vec<&'static str> = Vec::new();
    let mut pending: Vec<&str> = vec![class];
    while let Some(current) = pending.pop() {
        if current.is_empty() || current == "java/lang/Object" {
            continue;
        }
        let (key, info) = table
            .classes
            .get_key_value(current)
            .or_else(|| table.interfaces.get_key_value(current))?;
        if out.contains(key) {
            continue;
        }
        if out.len() >= 256 {
            return None;
        }
        out.push(key);
        pending.push(info.super_name);
        pending.extend(info.interfaces.iter().copied());
    }
    Some(out)
}

/// Resolve the owning target class for a `@Shadow` member on a `@Mixin` class given its
/// declared targets and the actual receiver's vanilla class hierarchy.
///
/// - Single-target mixins resolve unambiguously to their sole target.
/// - Multi-target mixins resolve to the unique target present in `receiver_hierarchy`.
/// - Zero or multiple matching targets are ambiguous (`None`), so callers count/refuse
///   instead of silently routing through `targets.first()`.
pub(crate) fn resolve_shadow_target<'a>(
    targets: &'a [String],
    receiver_hierarchy: &[String],
) -> Option<&'a str> {
    if targets.len() == 1 {
        return targets.first().map(String::as_str);
    }
    let mut matched: Option<&'a str> = None;
    for target in targets {
        if receiver_hierarchy.iter().any(|ty| ty == target) {
            if matched.is_some() {
                return None;
            }
            matched = Some(target.as_str());
        }
    }
    matched
}

/// Active host-modelled targets of `@Mixin` `class` on `side`, filtered through the
/// shared config, physical-side, `IMixinConfigPlugin`, and initialiser-soundness gate.
///
/// Used by `game_hooks::MixinModel::build` so game-hook `instanceof` and call dispatch
/// obey the exact same gating as `RuntimeMixinIndex`.
pub(crate) fn active_host_mixin_targets(
    jar: &NativeJar,
    configs: &[MixinConfig],
    config_sides: &BTreeMap<String, MixinSide>,
    plugins: &ConfigPlugins,
    side: MixinSide,
    class: &ClassFile,
) -> Vec<String> {
    let Some(details) = inspect_mixin_class(class, |name| jar.classes.get(name)) else {
        return Vec::new();
    };
    // Real Mixin rejects the class (`InvalidMixinException`) for every target.
    if details.invalid.is_some() {
        return Vec::new();
    }
    let is_interface_injection = !details.is_interface && !details.interfaces.is_empty();
    let mut active = Vec::new();
    for target in details.targets {
        if jar.classes.contains_key(&target) {
            continue;
        }
        if configs.is_empty() {
            continue;
        }
        if mixin_config_gate_with_plugins(
            configs,
            config_sides,
            plugins,
            &details.mixin,
            &target,
            side,
        )
        .is_err()
        {
            continue;
        }
        if is_interface_injection
            && (details.unrunnable_initializer || !is_host_modelled_game_target(&target))
        {
            continue;
        }
        active.push(target);
    }
    active
}

/// A class a `required` Mixin config's rejected mixin fails to load
/// ([`NativeJar::failed_required_targets`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FailedTarget {
    /// Which config and mixin, and Mixin's own message.
    pub(crate) reason: String,
    /// `false` when a config plugin governs the pair and its `shouldApplyMixin` answer
    /// is not known (a refused session, a failed question): the class fails only if the
    /// plugin kept the mixin, so the failure is counted where it would happen, never
    /// refused.
    pub(crate) certain: bool,
}

/// Key for sharing immutable per-`(jar, side, linked_classes, host_precedence)` runtime
/// caches across invocations on the same [`NativeJar`].
pub(crate) type RuntimeCacheKey = (MixinSide, u64, crate::class_precedence::HostClassPrecedence);

/// Maximum distinct `(side, linked_table, host_precedence)` caches kept per [`NativeJar`].
const MAX_RUNTIME_CACHES_PER_JAR: usize = 32;
/// Maximum memoised class-keyed entries inside one [`SharedRuntimeCache`].
const MAX_CACHED_CLASSES: usize = 65_536;
/// Maximum memoised target classes per source class in `instance_of` / `interface_extends`.
const MAX_CACHED_TARGETS_PER_CLASS: usize = 4_096;
/// Maximum memoised member/field lookup entries per class.
const MAX_CACHED_MEMBERS_PER_CLASS: usize = 1_024;

/// Thread-safe cache shared across every [`Runtime`] that executes the same [`NativeJar`]
/// on the same physical side with the same linked class table and host class-precedence rule.
///
/// Every entry here depends only on the immutable jar classes, mixin configs/plugins, linked
/// classes, side, and host `models_class` rule — never on mutable heap, static fields,
/// initialized/erroneous class state, or per-firing host state.
#[derive(Debug, Default)]
pub(crate) struct SharedRuntimeCache {
    pub(crate) index: std::sync::OnceLock<RuntimeMixinIndex>,
    pub(crate) package_prefixes: std::sync::OnceLock<Vec<String>>,
    pub(crate) unloadable_pools:
        std::sync::RwLock<crate::fx_hash::FxHashMap<String, Option<std::sync::Arc<BTreeSet<u16>>>>>,
    pub(crate) load_failures: std::sync::RwLock<
        crate::fx_hash::FxHashMap<String, crate::mixin_class_loading::LoadFailure>,
    >,
    pub(crate) refused_mixin_package: std::sync::RwLock<crate::fx_hash::FxHashMap<String, bool>>,
    pub(crate) instance_of: std::sync::RwLock<
        crate::fx_hash::FxHashMap<String, crate::fx_hash::FxHashMap<String, bool>>,
    >,
    pub(crate) interface_extends: std::sync::RwLock<
        crate::fx_hash::FxHashMap<String, crate::fx_hash::FxHashMap<String, bool>>,
    >,
    pub(crate) member_owners: std::sync::RwLock<
        crate::fx_hash::FxHashMap<
            String,
            crate::fx_hash::FxHashMap<String, Vec<(String, Option<(String, bool)>)>>,
        >,
    >,
    pub(crate) field_owners: std::sync::RwLock<
        crate::fx_hash::FxHashMap<String, crate::fx_hash::FxHashMap<String, Vec<(String, String)>>>,
    >,
}

impl SharedRuntimeCache {
    pub(crate) fn get_instance_of(&self, class: &str, wanted: &str) -> Option<bool> {
        let guard = self.instance_of.read().ok()?;
        guard.get(class)?.get(wanted).copied()
    }

    pub(crate) fn insert_instance_of(&self, class: &str, wanted: &str, answer: bool) {
        if let Ok(mut guard) = self.instance_of.write() {
            if let Some(inner) = guard.get_mut(class) {
                if inner.len() < MAX_CACHED_TARGETS_PER_CLASS {
                    inner.insert(wanted.to_owned(), answer);
                }
            } else if guard.len() < MAX_CACHED_CLASSES {
                let mut inner = crate::fx_hash::FxHashMap::default();
                inner.insert(wanted.to_owned(), answer);
                guard.insert(class.to_owned(), inner);
            }
        }
    }

    pub(crate) fn get_interface_extends(&self, interface: &str, wanted: &str) -> Option<bool> {
        let guard = self.interface_extends.read().ok()?;
        guard.get(interface)?.get(wanted).copied()
    }

    pub(crate) fn insert_interface_extends(&self, interface: &str, wanted: &str, answer: bool) {
        if let Ok(mut guard) = self.interface_extends.write() {
            if let Some(inner) = guard.get_mut(interface) {
                if inner.len() < MAX_CACHED_TARGETS_PER_CLASS {
                    inner.insert(wanted.to_owned(), answer);
                }
            } else if guard.len() < MAX_CACHED_CLASSES {
                let mut inner = crate::fx_hash::FxHashMap::default();
                inner.insert(wanted.to_owned(), answer);
                guard.insert(interface.to_owned(), inner);
            }
        }
    }

    pub(crate) fn get_member_owner(
        &self,
        class: &str,
        name: &str,
        descriptor: &str,
    ) -> Option<Option<(String, bool)>> {
        let guard = self.member_owners.read().ok()?;
        let by_name = guard.get(class)?;
        let entries = by_name.get(name)?;
        entries
            .iter()
            .find(|(d, _)| d == descriptor)
            .map(|(_, ans)| ans.clone())
    }

    pub(crate) fn insert_member_owner(
        &self,
        class: &str,
        name: &str,
        descriptor: &str,
        answer: Option<(String, bool)>,
    ) {
        if let Ok(mut guard) = self.member_owners.write() {
            if let Some(by_name) = guard.get_mut(class) {
                if let Some(entries) = by_name.get_mut(name) {
                    if !entries.iter().any(|(d, _)| d == descriptor) && entries.len() < 64 {
                        entries.push((descriptor.to_owned(), answer));
                    }
                } else if by_name.len() < MAX_CACHED_MEMBERS_PER_CLASS {
                    by_name.insert(name.to_owned(), vec![(descriptor.to_owned(), answer)]);
                }
            } else if guard.len() < MAX_CACHED_CLASSES {
                let mut by_name = crate::fx_hash::FxHashMap::default();
                by_name.insert(name.to_owned(), vec![(descriptor.to_owned(), answer)]);
                guard.insert(class.to_owned(), by_name);
            }
        }
    }

    pub(crate) fn get_field_owner(
        &self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> Option<String> {
        let guard = self.field_owners.read().ok()?;
        let by_name = guard.get(owner)?;
        let entries = by_name.get(name)?;
        entries
            .iter()
            .find(|(d, _)| d == descriptor)
            .map(|(_, ans)| ans.clone())
    }

    pub(crate) fn insert_field_owner(
        &self,
        owner: &str,
        name: &str,
        descriptor: &str,
        resolved: &str,
    ) {
        if let Ok(mut guard) = self.field_owners.write() {
            if let Some(by_name) = guard.get_mut(owner) {
                if let Some(entries) = by_name.get_mut(name) {
                    if !entries.iter().any(|(d, _)| d == descriptor) && entries.len() < 64 {
                        entries.push((descriptor.to_owned(), resolved.to_owned()));
                    }
                } else if by_name.len() < MAX_CACHED_MEMBERS_PER_CLASS {
                    by_name.insert(
                        name.to_owned(),
                        vec![(descriptor.to_owned(), resolved.to_owned())],
                    );
                }
            } else if guard.len() < MAX_CACHED_CLASSES {
                let mut by_name = crate::fx_hash::FxHashMap::default();
                by_name.insert(
                    name.to_owned(),
                    vec![(descriptor.to_owned(), resolved.to_owned())],
                );
                guard.insert(owner.to_owned(), by_name);
            }
        }
    }
}

/// Per-[`NativeJar`] cache of immutable metadata parsed from jar resources and class files.
///
/// Reset when [`NativeJar::with_woven_classes`] replaces classes on a jar.
#[derive(Debug, Default)]
pub(crate) struct JarMetaCache {
    pub(crate) mixin_config_scan:
        std::sync::OnceLock<(std::sync::Arc<Vec<MixinConfig>>, Vec<(String, String)>)>,
    pub(crate) mixin_config_sides: std::sync::OnceLock<std::sync::Arc<BTreeMap<String, MixinSide>>>,
    pub(crate) tooltip_create_callbacks:
        std::sync::RwLock<BTreeMap<String, Result<Vec<crate::MixinInjectionPlan>, String>>>,
    pub(crate) runtime_caches: std::sync::RwLock<
        crate::fx_hash::FxHashMap<RuntimeCacheKey, std::sync::Arc<SharedRuntimeCache>>,
    >,
}

impl JarMetaCache {
    pub(crate) fn runtime_cache(&self, key: RuntimeCacheKey) -> std::sync::Arc<SharedRuntimeCache> {
        if let Ok(guard) = self.runtime_caches.read()
            && let Some(existing) = guard.get(&key)
        {
            return std::sync::Arc::clone(existing);
        }
        if let Ok(mut guard) = self.runtime_caches.write() {
            if let Some(existing) = guard.get(&key) {
                return std::sync::Arc::clone(existing);
            }
            let fresh = std::sync::Arc::new(SharedRuntimeCache::default());
            if guard.len() < MAX_RUNTIME_CACHES_PER_JAR {
                guard.insert(key, std::sync::Arc::clone(&fresh));
            }
            return fresh;
        }
        std::sync::Arc::new(SharedRuntimeCache::default())
    }
}

/// Per-`Runtime` mixin configuration gate and shared immutable cache handle.
#[derive(Debug, Default)]
pub(crate) struct RuntimeMixinState {
    pub(crate) configs: std::sync::Arc<Vec<MixinConfig>>,
    pub(crate) config_sides: std::sync::Arc<BTreeMap<String, MixinSide>>,
    plugins: std::sync::Arc<ConfigPlugins>,
    pub(crate) side: Option<MixinSide>,
    require_config: bool,
    /// Classes (jar and game) a `required` config's rejected mixin fails to load, with
    /// why ([`NativeJar::failed_required_targets`]).
    pub(crate) failed_required_targets: std::sync::Arc<BTreeMap<String, FailedTarget>>,
    pub(crate) cache: std::sync::Arc<SharedRuntimeCache>,
}

impl RuntimeMixinState {
    /// Build state for a real `NativeJar` invocation, honouring the jar's mixin configs,
    /// config plugins, and the host's physical side.
    pub(crate) fn from_jar<H: Host>(jar: &NativeJar, host: &H) -> Self {
        Self::from_jar_side_host(jar, host.mixin_side(), host)
    }

    /// Build state for a real `NativeJar` invocation pinned to an explicit physical side,
    /// sharing immutable per-jar caches when the host's class-precedence rule is cacheable.
    pub(crate) fn from_jar_side_host<H: Host>(jar: &NativeJar, side: MixinSide, host: &H) -> Self {
        let cache = if crate::mixin_plugins::running_config_plugins() {
            std::sync::Arc::new(SharedRuntimeCache::default())
        } else if let Some(precedence) = host.class_precedence_key() {
            let linked_id = host.linked_class_table().map_or(0, |t| t.cache_id());
            jar.meta_cache.runtime_cache((side, linked_id, precedence))
        } else {
            std::sync::Arc::new(SharedRuntimeCache::default())
        };
        Self {
            configs: jar.cached_mixin_configs_arc(),
            config_sides: jar.cached_mixin_config_sides_arc(),
            plugins: jar.cached_config_plugins_arc(side),
            side: Some(side),
            require_config: true,
            failed_required_targets: jar.failed_required_targets_arc(side),
            cache,
        }
    }

    /// Build an unconfigured state for synthetic in-memory unit tests that construct a
    /// `Runtime` directly without a `NativeJar`.
    #[cfg(test)]
    pub(crate) fn unconfigured() -> Self {
        Self {
            configs: std::sync::Arc::new(Vec::new()),
            config_sides: std::sync::Arc::new(BTreeMap::new()),
            plugins: std::sync::Arc::new(ConfigPlugins::default()),
            side: None,
            require_config: false,
            failed_required_targets: std::sync::Arc::new(BTreeMap::new()),
            cache: std::sync::Arc::new(SharedRuntimeCache::default()),
        }
    }

    /// Whether `(mixin, target)` passes the primary jar's Mixin config, physical-side,
    /// and `IMixinConfigPlugin` gate.
    ///
    /// Returns `true` when config gating is disabled (`unconfigured()` in synthetic unit
    /// tests) or when the jar has readable Mixin configs that list `mixin` for the active
    /// side and any config plugin allows `(mixin, target)`. A jar with no readable Mixin
    /// configs never passes this gate.
    fn allows_config_gated(&self, mixin: &str, target: &str) -> bool {
        if !self.require_config {
            return true;
        }
        if self.configs.is_empty() {
            return false;
        }
        let Some(side) = self.side else {
            return false;
        };
        mixin_config_gate_with_plugins(
            &self.configs,
            &self.config_sides,
            &self.plugins,
            mixin,
            target,
            side,
        )
        .is_ok()
    }
}

#[derive(Clone, Copy)]
struct MixinRegistration {
    /// True for primary-jar classes (preserving the existing jar-local and non-host-game
    /// `by_target` scan), false for linked-jar classes.
    is_primary: bool,
    /// True when the primary jar has no readable Mixin config at all.
    primary_has_no_configs: bool,
}

/// Outcome of [`Runtime::game_handle_put_field`].
pub(crate) enum GameFieldWrite {
    /// Stored on the game handle as an active interface-injection mixin's added field.
    Stored,
    /// Counted as an unknown write (or refused on a strict host); do not store a fake field.
    Handled,
    /// Not a game handle or deferred to the caller's existing path.
    Fallthrough,
}

fn is_game_class(owner: &str) -> bool {
    owner.starts_with("net/minecraft/") || owner.starts_with("com/mojang/")
}

fn mixin_annotations(class: &ClassFile) -> Vec<&Annotation> {
    class
        .annotations
        .iter()
        .filter(|a| a.descriptor == crate::MIXIN_ANNOTATION_DESCRIPTOR)
        .collect()
}

fn class_targets(class: &ClassFile) -> Vec<String> {
    let mut targets = Vec::new();
    for annotation in mixin_annotations(class) {
        for owner in mixin_target_owners(annotation) {
            if !targets.contains(&owner) {
                targets.push(owner);
            }
        }
    }
    targets.sort();
    targets
}

fn has_annotation(descriptors: Option<&Vec<String>>, target: &str) -> bool {
    descriptors.is_some_and(|list| list.iter().any(|d| d == target))
}

/// [`has_annotation`] over annotations read with their values.
fn has_annotation_value(annotations: Option<&Vec<Annotation>>, target: &str) -> bool {
    annotations.is_some_and(|list| list.iter().any(|a| a.descriptor == target))
}

/// The `@Shadow` prefix that applies to a member: its `@Shadow(prefix = ...)` value, or
/// the annotation's default `shadow$` when it has no `@Shadow` or omits `prefix`
/// (`Annotations.getValue(shadow, "prefix", Shadow.class)`, which falls back to the
/// default for a null annotation too).
fn shadow_prefix(annotations: Option<&Vec<Annotation>>) -> &str {
    annotations
        .and_then(|list| {
            list.iter()
                .find(|a| a.descriptor == MIXIN_SHADOW_DESCRIPTOR)
        })
        .and_then(|shadow| match shadow.values.get("prefix") {
            Some(AnnotationValue::String(prefix)) => Some(prefix.as_str()),
            _ => None,
        })
        .unwrap_or(DEFAULT_SHADOW_PREFIX)
}

/// The target method a `@Shadow` method named `name` stands for:
/// `MixinPreProcessorStandard.prepareShadow` removes the `@Shadow` prefix when the
/// name starts with it (`shadow$getId` → `getId`), and keeps the name otherwise.
/// A member without `@Shadow` (an abstract stand-in, an accessor) keeps its name.
pub(crate) fn shadow_method_target_name(
    name: &str,
    annotations: Option<&Vec<Annotation>>,
) -> String {
    if !has_annotation_value(annotations, MIXIN_SHADOW_DESCRIPTOR) {
        return name.to_owned();
    }
    name.strip_prefix(shadow_prefix(annotations))
        .unwrap_or(name)
        .to_owned()
}

/// Why sponge-mixin 0.17.5 rejects `class` with `InvalidMixinException` before
/// applying it to any target, for the rules that need nothing but the class itself:
/// the `validateField` check `attachFields` runs on every field (shadow or not).
///
/// Which preprocessor runs depends on `MixinInfo.getVariant`:
/// - a class (`MixinPreProcessorStandard.validateField`): a static field that is
///   neither `private`, synthetic nor `@Shadow` is "non-private static" (`@Unique` does
///   not exempt it); then any field whose name starts with its `@Shadow` prefix (the
///   annotation's `prefix`, else `shadow$`) has "a shadow prefix". Prefixes rename
///   shadow *methods* only;
/// - an interface (`MixinPreProcessorInterface`, also used for accessor mixins): every
///   field must be `public static final`, a `@Shadow`, not `@Mutable`, and unprefixed;
/// - an enum (`MixinPreProcessorEnumExtension`): an enum constant (`ACC_ENUM | ACC_STATIC`
///   of the class's own type, `Bytecode.isEnumConstant`) skips the class rules but must
///   carry neither `@Shadow` nor `@Unique` ("Enum constant %s in %s has @%s annotation");
///   every other field takes the class rules.
///
/// The exception drops the whole mixin from the target (`MixinApplicatorStandard.apply`
/// removes it, or rethrows for a `required` config, failing the target class). Mixin
/// reads only visible annotations; jars built against `CLASS`-retained stubs carry
/// `@Shadow` invisibly, and this runtime reads both, as everywhere else.
fn invalid_mixin_reason(class: &ClassFile, field_anns: &MemberAnnotationValues) -> Option<String> {
    let mixin = class.name.replace('/', ".");
    let access = class_access(class).unwrap_or(0);
    let is_interface = access & ACC_INTERFACE != 0;
    let is_enum = access & ACC_ENUM != 0;
    let own_type = format!("L{};", class.name);
    class.fields.iter().find_map(|field| {
        let anns = field_anns.get(&format!("{}|{}", field.name, field.descriptor));
        let is_shadow = has_annotation_value(anns, MIXIN_SHADOW_DESCRIPTOR);
        if is_enum
            && field.access & (ACC_ENUM | ACC_STATIC) == ACC_ENUM | ACC_STATIC
            && field.descriptor == own_type
        {
            // `DISALLOWED_ANNOTATIONS_ON_CONSTANTS`, in that order.
            return [
                (MIXIN_SHADOW_DESCRIPTOR, "Shadow"),
                (MIXIN_UNIQUE_DESCRIPTOR, "Unique"),
            ]
            .into_iter()
            .find(|(descriptor, _)| has_annotation_value(anns, descriptor))
            .map(|(_, simple)| {
                format!(
                    "Enum constant {} in {mixin} has @{simple} annotation. This is not allowed.",
                    field.name
                )
            });
        }
        if is_interface {
            if field.access & (ACC_PUBLIC | ACC_STATIC | ACC_FINAL)
                != ACC_PUBLIC | ACC_STATIC | ACC_FINAL
            {
                return Some(format!(
                    "Interface mixin contains an illegal field! Found {} {} in {mixin}",
                    field_modifiers(field.access),
                    field.name
                ));
            }
            if !is_shadow {
                return Some(format!(
                    "Interface mixin {mixin} contains a non-shadow field: {}",
                    field.name
                ));
            }
            if has_annotation_value(anns, MIXIN_MUTABLE_DESCRIPTOR) {
                return Some(format!(
                    "@Shadow field {mixin}.{} is marked as mutable. This is not allowed.",
                    field.name
                ));
            }
        } else if field.is_static()
            && field.access & (ACC_PRIVATE | ACC_SYNTHETIC) == 0
            && !is_shadow
        {
            return Some(format!(
                "Mixin {mixin} contains non-private static field {}:{}",
                field.name, field.descriptor
            ));
        }
        field.name.starts_with(shadow_prefix(anns)).then(|| {
            format!(
                "@Shadow field {mixin}.{} has a shadow prefix. This is not allowed.",
                field.name
            )
        })
    })
}

/// `java.lang.reflect.Modifier.toString` for a field's access flags.
fn field_modifiers(access: u16) -> String {
    [
        (ACC_PUBLIC, "public"),
        (0x0004, "protected"),
        (ACC_PRIVATE, "private"),
        (ACC_STATIC, "static"),
        (ACC_FINAL, "final"),
        (0x0080, "transient"),
        (0x0040, "volatile"),
    ]
    .iter()
    .filter(|(flag, _)| access & flag != 0)
    .map(|(_, word)| *word)
    .collect::<Vec<_>>()
    .join(" ")
}

fn is_planned_method_annotation(descriptor: &str) -> bool {
    PLANNED_INJECTOR_ANNOTATIONS.contains(&descriptor)
        || MixinExtrasKind::from_descriptor(descriptor).is_some()
        || descriptor == crate::MIXIN_ACCESSOR_DESCRIPTOR
        || descriptor == crate::MIXIN_INVOKER_DESCRIPTOR
}

/// Inspect one `@Mixin` class for its targets, interfaces, added members, `@Shadow`
/// members, and `<init>` initialiser properties.
fn inspect_mixin_class<'a>(
    class: &ClassFile,
    lookup: impl Fn(&str) -> Option<&'a ClassFile>,
) -> Option<MixinClassDetails> {
    let targets = class_targets(class);
    if targets.is_empty() {
        return None;
    }
    let is_interface = class_access(class).is_some_and(|access| access & ACC_INTERFACE != 0);
    let interfaces = class.interface_names();
    let (field_anns, method_anns) = member_annotation_values(class).unwrap_or_default();
    let invalid = invalid_mixin_reason(class, &field_anns);

    let mut added_fields = Vec::new();
    let mut added_instance_fields = BTreeSet::new();
    let mut shadow_fields = BTreeSet::new();
    let mut static_shadow_fields = BTreeMap::new();
    for field in &class.fields {
        let key = format!("{}|{}", field.name, field.descriptor);
        let anns = field_anns.get(&key);
        let is_shadow = has_annotation_value(anns, MIXIN_SHADOW_DESCRIPTOR);
        if is_shadow {
            let member = (field.name.clone(), field.descriptor.clone());
            shadow_fields.insert(member.clone());
            if field.is_static() {
                // `ClassInfo.Field` records `@Mutable` (and `@Final`) only for a `@Shadow`
                // field; `@Final` changes nothing at runtime, so only `@Mutable` is kept.
                let is_mutable = has_annotation_value(anns, MIXIN_MUTABLE_DESCRIPTOR);
                static_shadow_fields.insert(member, StaticShadowField { is_mutable });
            }
            continue;
        }
        if field.access & ACC_SYNTHETIC != 0 {
            continue;
        }
        added_fields.push(MethodRef {
            owner: class.name.clone(),
            name: field.name.clone(),
            descriptor: field.descriptor.clone(),
        });
        if !field.is_static() {
            added_instance_fields.insert((field.name.clone(), field.descriptor.clone()));
        }
    }
    added_fields.sort_by(|a, b| (&a.name, &a.descriptor).cmp(&(&b.name, &b.descriptor)));

    let mut added_methods = Vec::new();
    let mut added_public_methods = BTreeSet::new();
    let mut shadow_methods = BTreeMap::new();
    for method in &class.methods {
        if method.name == "<init>" || method.name == "<clinit>" {
            continue;
        }
        let key = format!("{}|{}", method.name, method.descriptor);
        let annotated = method_anns.get(&key);
        let descriptors: Option<Vec<String>> =
            annotated.map(|list| list.iter().map(|a| a.descriptor.clone()).collect());
        let invis = descriptors.as_ref();
        let is_shadow = has_annotation(invis, MIXIN_SHADOW_DESCRIPTOR)
            || method
                .annotations
                .iter()
                .any(|a| a.descriptor == MIXIN_SHADOW_DESCRIPTOR);
        let is_accessor = resolve_mixin_accessor(class, method).is_some()
            || has_annotation(invis, crate::MIXIN_ACCESSOR_DESCRIPTOR)
            || has_annotation(invis, crate::MIXIN_INVOKER_DESCRIPTOR)
            || method.annotations.iter().any(|a| {
                a.descriptor == crate::MIXIN_ACCESSOR_DESCRIPTOR
                    || a.descriptor == crate::MIXIN_INVOKER_DESCRIPTOR
            });
        if is_shadow || (!is_accessor && method.access & ACC_ABSTRACT != 0) {
            shadow_methods.insert(
                (method.name.clone(), method.descriptor.clone()),
                shadow_method_target_name(&method.name, annotated),
            );
            continue;
        }
        if method.is_static() || method.access & ACC_SYNTHETIC != 0 || is_accessor {
            continue;
        }
        let is_injector = method
            .annotations
            .iter()
            .any(|a| is_planned_method_annotation(&a.descriptor))
            || invis.is_some_and(|list| list.iter().any(|d| is_planned_method_annotation(d)));
        if is_injector {
            continue;
        }
        if method.code.is_some() {
            added_methods.push(MethodRef {
                owner: class.name.clone(),
                name: method.name.clone(),
                descriptor: method.descriptor.clone(),
            });
            if method.access & ACC_PRIVATE == 0 {
                added_public_methods.insert((method.name.clone(), method.descriptor.clone()));
            }
        }
    }
    added_methods.sort_by(|a, b| (&a.name, &a.descriptor).cmp(&(&b.name, &b.descriptor)));

    let has_initializer = class.super_name == "java/lang/Object"
        && class
            .method("<init>", "()V")
            .is_some_and(|m| m.code.is_some());
    let unrunnable_initializer = !has_initializer
        && !added_instance_fields.is_empty()
        && init_writes_added_field(class, &added_instance_fields);

    let mut declared_chain_fields = BTreeSet::new();
    let mut exit = class.super_name.clone();
    for field in &class.fields {
        declared_chain_fields.insert((field.name.clone(), field.descriptor.clone()));
    }
    for _ in 0..crate::MAX_SUPERCLASS_WALK {
        let Some(super_file) = lookup(&exit) else {
            break;
        };
        for field in &super_file.fields {
            declared_chain_fields.insert((field.name.clone(), field.descriptor.clone()));
        }
        if super_file.super_name.is_empty() || super_file.super_name == exit {
            exit.clear();
            break;
        }
        exit = super_file.super_name.clone();
    }
    let extends_game_class = lookup(&exit).is_none() && is_game_class(&exit);

    Some(MixinClassDetails {
        mixin: class.name.clone(),
        targets,
        is_interface,
        interfaces,
        added_fields,
        added_instance_fields,
        shadow_fields,
        static_shadow_fields,
        added_methods,
        added_public_methods,
        shadow_methods,
        has_initializer,
        unrunnable_initializer,
        extends_game_class,
        declared_chain_fields,
        invalid,
    })
}

/// Whether any constructor (`<init>`) of `class` executes `putfield` (`0xb5`) targeting
/// one of `added_instance_fields`. Used to refuse non-`Object` mixins whose field
/// initialisers live in a superclass-chaining constructor the runtime cannot run.
fn init_writes_added_field(
    class: &ClassFile,
    added_instance_fields: &BTreeSet<(String, String)>,
) -> bool {
    for method in &class.methods {
        if method.name != "<init>" {
            continue;
        }
        let Some(code) = method.code.as_deref() else {
            continue;
        };
        if bytecode_puts_field(class, code, added_instance_fields) {
            return true;
        }
    }
    false
}

/// Walk JVM bytecode instruction boundaries accurately and report whether any `putfield`
/// (`0xb5`) writes to `(class.name, name, desc)` in `added_instance_fields`.
fn bytecode_puts_field(
    class: &ClassFile,
    code: &[u8],
    added_instance_fields: &BTreeSet<(String, String)>,
) -> bool {
    let mut pc = 0usize;
    while pc < code.len() {
        let op = code[pc];
        if op == 0xb5 {
            if pc + 2 >= code.len() {
                return true;
            }
            let idx = u16::from_be_bytes([code[pc + 1], code[pc + 2]]);
            match class.field_ref(idx) {
                Ok(field) => {
                    if field.owner == class.name
                        && added_instance_fields.contains(&(field.name, field.descriptor))
                    {
                        return true;
                    }
                }
                Err(_) => return true,
            }
            pc += 3;
            continue;
        }
        let Some(next) = next_instruction_pc(code, pc) else {
            // Unrecognised or truncated opcode: assume conservative `true` so we never
            // silently drop an initialiser we could not inspect.
            return true;
        };
        pc = next;
    }
    false
}

/// Compute the next PC after the JVM instruction at `pc`, or `None` on malformed bytecode.
fn next_instruction_pc(code: &[u8], pc: usize) -> Option<usize> {
    let op = *code.get(pc)?;
    let step = match op {
        // 1-byte opcodes (no operands)
        0x00..=0x0f
        | 0x1a..=0x35
        | 0x3b..=0x83
        | 0x85..=0x98
        | 0xac..=0xb1
        | 0xbe
        | 0xbf
        | 0xc2
        | 0xc3 => 1,
        // 2-byte opcodes (1-byte operand)
        0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xa9 | 0xbc => 2,
        // 3-byte opcodes (2-byte operand)
        0x11
        | 0x13
        | 0x14
        | 0x84
        | 0x99..=0xa8
        | 0xb2..=0xb8
        | 0xbb
        | 0xbd
        | 0xc0
        | 0xc1
        | 0xc6
        | 0xc7 => 3,
        // multianewarray: 2-byte class index + 1-byte dimensions
        0xc5 => 4,
        // invokeinterface, invokedynamic, goto_w, jsr_w: 4-byte operand
        0xb9 | 0xba | 0xc8 | 0xc9 => 5,
        // wide
        0xc4 => {
            let sub = *code.get(pc + 1)?;
            if sub == 0x84 { 6 } else { 4 }
        }
        // tableswitch
        0xaa => {
            let mut cursor = (pc + 4) & !3;
            let low = i32::from_be_bytes(code.get(cursor + 4..cursor + 8)?.try_into().ok()?);
            let high = i32::from_be_bytes(code.get(cursor + 8..cursor + 12)?.try_into().ok()?);
            if high < low {
                return None;
            }
            let count = usize::try_from(i64::from(high) - i64::from(low) + 1).ok()?;
            cursor = cursor.checked_add(12)?.checked_add(count.checked_mul(4)?)?;
            return (cursor <= code.len()).then_some(cursor);
        }
        // lookupswitch
        0xab => {
            let mut cursor = (pc + 4) & !3;
            let npairs = i32::from_be_bytes(code.get(cursor + 4..cursor + 8)?.try_into().ok()?);
            if npairs < 0 {
                return None;
            }
            let count = usize::try_from(npairs).ok()?;
            cursor = cursor.checked_add(8)?.checked_add(count.checked_mul(8)?)?;
            return (cursor <= code.len()).then_some(cursor);
        }
        _ => return None,
    };
    let next = pc.checked_add(step)?;
    (next <= code.len()).then_some(next)
}

fn class_ref(owner: &str) -> MethodRef {
    MethodRef {
        owner: owner.to_owned(),
        name: String::new(),
        descriptor: String::new(),
    }
}

impl NativeJar {
    /// Every interface-injection `@Mixin` plan targeting a host-modelled game class that
    /// this jar's classes declare, in deterministic `(target, mixin)` order.
    pub fn mixin_interface_plans(&self) -> Vec<MixinInterfacePlan> {
        let mut plans = Vec::new();
        for class in self.classes.values() {
            let Some(details) = inspect_mixin_class(class, |name| self.classes.get(name)) else {
                continue;
            };
            if details.is_interface
                || details.interfaces.is_empty()
                || details.unrunnable_initializer
                || details.invalid.is_some()
            {
                continue;
            }
            for target in &details.targets {
                if !is_host_modelled_game_target(target) {
                    continue;
                }
                plans.push(MixinInterfacePlan {
                    mixin: details.mixin.clone(),
                    target: target.clone(),
                    interfaces: details.interfaces.clone(),
                    added_fields: details.added_fields.clone(),
                    added_methods: details.added_methods.clone(),
                    has_initializer: details.has_initializer,
                });
            }
        }
        plans.sort_by(|a, b| (&a.target, &a.mixin).cmp(&(&b.target, &b.mixin)));
        plans
    }

    /// One row per target of every `@Mixin` class real Mixin rejects with
    /// `InvalidMixinException` ([`invalid_mixin_reason`]): where the config (and, for the
    /// hook tier, its plugin) would apply the class, `invalid-mixin` when the config is
    /// not `required` (Mixin drops the class) and `required-mixin-failed` when it is
    /// (Mixin rethrows and the target fails to load); else the gate's own reason. Returns
    /// the rejected classes, whose plans of every family the weave then leaves out:
    /// Mixin drops the whole class, not single members.
    pub(crate) fn invalid_mixin_rows(
        &self,
        side: MixinSide,
        configs: &[MixinConfig],
        config_sides: &BTreeMap<String, MixinSide>,
        plugins: Option<&ConfigPlugins>,
        skipped: &mut Vec<SkippedMixinPlan>,
    ) -> BTreeSet<String> {
        let mut rejected = BTreeSet::new();
        for class in self.classes.values() {
            if mixin_annotations(class).is_empty() {
                continue;
            }
            let Some(details) = inspect_mixin_class(class, |name| self.classes.get(name)) else {
                continue;
            };
            let Some(detail) = details.invalid else {
                continue;
            };
            let required = crate::listing_mixin_config(configs, config_sides, &details.mixin, side)
                .is_some_and(|config| config.required);
            let detail = crate::bounded_weave_reason(&detail);
            for target in &details.targets {
                let gate = match plugins {
                    Some(plugins) => self.game_hook_config_gate(
                        configs,
                        config_sides,
                        plugins,
                        &details.mixin,
                        target,
                        side,
                    ),
                    None => mixin_config_gate(configs, config_sides, &details.mixin, side),
                };
                skipped.push(SkippedMixinPlan {
                    target: class_ref(target),
                    callback: class_ref(&details.mixin),
                    at: "@Mixin".to_owned(),
                    reason: gate.err().unwrap_or_else(|| {
                        if required {
                            MixinSkipReason::RequiredMixinFailed(detail.clone())
                        } else {
                            MixinSkipReason::InvalidMixin(detail.clone())
                        }
                    }),
                });
            }
            rejected.insert(details.mixin);
        }
        rejected
    }

    /// The classes a `required` config's rejected mixin fails on `side`, each with why:
    /// Mixin rethrows the `InvalidMixinException` (`MixinApplicatorStandard.apply`), and
    /// Fabric's Knot turns it into `RuntimeException: Mixin transformation of <target>
    /// failed` from the class load, for a jar class and a game class alike.
    ///
    /// Only where Mixin applies the class: a config plugin that declines `(mixin, target)`
    /// keeps it off that target (`MixinInfo.readDeclaredTargets`), so it neither applies
    /// nor validates there; a plugin answer that is not known leaves the failure
    /// [`FailedTarget::certain`] `false`. What the runtime does with the map:
    /// [`Runtime::class_load_failure`] (the use of such a class is counted where the jar's
    /// code first resolves it) and [`Runtime::refuse_failed_game_mixin_target`] (a game
    /// class the server loads before any mod runs: the game does not start). Cached per
    /// side.
    pub(crate) fn failed_required_targets(
        &self,
        side: MixinSide,
    ) -> &BTreeMap<String, FailedTarget> {
        static NONE: std::sync::OnceLock<BTreeMap<String, FailedTarget>> =
            std::sync::OnceLock::new();
        // A plugin session's own runtime (`mixin_plugins.rs`) runs inside Mixin's
        // selection, where nothing is applied; asking for the plugins here would also
        // re-enter the session that is computing them.
        if crate::mixin_plugins::running_config_plugins() {
            return NONE.get_or_init(BTreeMap::new);
        }
        let slot = match side {
            MixinSide::Server => &self.server_failed_required_targets,
            MixinSide::Client => &self.client_failed_required_targets,
        };
        if let Some(failed) = slot.get() {
            return failed.as_ref();
        }
        // Before the cell: the sessions build runtimes of their own.
        let plugins = self.cached_config_plugins(side);
        slot.get_or_init(|| {
            let configs = self.cached_mixin_configs_arc();
            let config_sides = self.cached_mixin_config_sides_arc();
            let mut failed = BTreeMap::new();
            for class in self.classes.values() {
                if mixin_annotations(class).is_empty() {
                    continue;
                }
                let Some(details) = inspect_mixin_class(class, |name| self.classes.get(name))
                else {
                    continue;
                };
                let Some(detail) = details.invalid else {
                    continue;
                };
                let Some(config) =
                    crate::listing_mixin_config(&configs, &config_sides, &details.mixin, side)
                        .filter(|config| config.required)
                else {
                    continue;
                };
                for target in details.targets {
                    // The plain gate passes it, or defers to the config's plugin.
                    let certain =
                        match mixin_config_gate(&configs, &config_sides, &details.mixin, side) {
                            Ok(()) => true,
                            Err(MixinSkipReason::ConfigPlugin) => {
                                match plugins.verdict(&config.path, &details.mixin, &target) {
                                    Some(true) => true,
                                    Some(false) => continue,
                                    None => false,
                                }
                            }
                            Err(_) => continue,
                        };
                    let reason = format!(
                        "required Mixin config {} rejects {}: {detail}",
                        config.path,
                        details.mixin.replace('/', ".")
                    );
                    // A certain failure names the class that surely fails the target.
                    let entry = failed.entry(target).or_insert_with(|| FailedTarget {
                        reason: reason.clone(),
                        certain,
                    });
                    if certain && !entry.certain {
                        *entry = FailedTarget { reason, certain };
                    }
                }
            }
            std::sync::Arc::new(failed)
        })
        .as_ref()
    }

    pub(crate) fn failed_required_targets_arc(
        &self,
        side: MixinSide,
    ) -> std::sync::Arc<BTreeMap<String, FailedTarget>> {
        if crate::mixin_plugins::running_config_plugins() {
            return std::sync::Arc::new(BTreeMap::new());
        }
        // Populate the OnceLock via `failed_required_targets`, then clone the Arc.
        let _ = self.failed_required_targets(side);
        let slot = match side {
            MixinSide::Server => &self.server_failed_required_targets,
            MixinSide::Client => &self.client_failed_required_targets,
        };
        slot.get()
            .map(std::sync::Arc::clone)
            .unwrap_or_else(|| std::sync::Arc::new(BTreeMap::new()))
    }

    /// Plan interface-injection mixins for [`NativeJar::plan_load_time_weave`] /
    /// [`NativeJar::plan_load_time_weave_with_game_hooks`], recording applied plans in
    /// `weave.interface_mixins` and every refusal in `weave.skipped`.
    pub(crate) fn plan_interface_mixins(
        &self,
        side: MixinSide,
        configs: &[MixinConfig],
        config_sides: &BTreeMap<String, MixinSide>,
        plugins: Option<&ConfigPlugins>,
        weave: &mut LoadTimeMixinWeave,
    ) -> InterfaceWeaveSummary {
        let mut summary = InterfaceWeaveSummary::default();
        let mut candidates: Vec<(MixinInterfacePlan, BTreeSet<(String, String)>)> = Vec::new();

        for class in self.classes.values() {
            let Some(details) = inspect_mixin_class(class, |name| self.classes.get(name)) else {
                continue;
            };
            // A class real Mixin rejects has one row per target already
            // (`NativeJar::invalid_mixin_rows`), which covers every member.
            if details.invalid.is_some() {
                summary.accounted_mixins.insert(details.mixin);
                continue;
            }
            if details.is_interface || details.interfaces.is_empty() {
                continue;
            }
            let pure_interface_shape = details.shadow_fields.is_empty()
                && details.shadow_methods.is_empty()
                && !details.unrunnable_initializer;
            let mut recorded_any = false;
            for target in &details.targets {
                let gate = match plugins {
                    Some(plugins) => self.game_hook_config_gate(
                        configs,
                        config_sides,
                        plugins,
                        &details.mixin,
                        target,
                        side,
                    ),
                    None => mixin_config_gate(configs, config_sides, &details.mixin, side),
                };
                if let Err(reason) = gate {
                    weave.skipped.push(SkippedMixinPlan {
                        target: class_ref(target),
                        callback: class_ref(&details.mixin),
                        at: "@Mixin(implements)".to_owned(),
                        reason,
                    });
                    recorded_any = true;
                    continue;
                }
                if !is_host_modelled_game_target(target) {
                    let reason = if self.classes.contains_key(target) {
                        MixinSkipReason::UnsupportedFamily
                    } else {
                        MixinSkipReason::TargetOutsideJar
                    };
                    weave.skipped.push(SkippedMixinPlan {
                        target: class_ref(target),
                        callback: class_ref(&details.mixin),
                        at: "@Mixin(implements)".to_owned(),
                        reason,
                    });
                    recorded_any = true;
                    continue;
                }
                if details.unrunnable_initializer {
                    weave.skipped.push(SkippedMixinPlan {
                        target: class_ref(target),
                        callback: class_ref(&details.mixin),
                        at: "@Mixin(implements)".to_owned(),
                        reason: MixinSkipReason::UnsupportedFamily,
                    });
                    recorded_any = true;
                    continue;
                }
                candidates.push((
                    MixinInterfacePlan {
                        mixin: details.mixin.clone(),
                        target: target.clone(),
                        interfaces: details.interfaces.clone(),
                        added_fields: details.added_fields.clone(),
                        added_methods: details.added_methods.clone(),
                        has_initializer: details.has_initializer,
                    },
                    details.added_public_methods.clone(),
                ));
                recorded_any = true;
            }
            if recorded_any && pure_interface_shape {
                summary.accounted_mixins.insert(details.mixin);
            }
        }

        // Refuse ambiguous interface-injection mixins that declare the same non-private
        // method on the same target class.
        let mut method_owners: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
        for (plan, public_methods) in &candidates {
            for (name, desc) in public_methods {
                let owners = method_owners
                    .entry((plan.target.clone(), name.clone(), desc.clone()))
                    .or_default();
                if !owners.contains(&plan.mixin) {
                    owners.push(plan.mixin.clone());
                }
            }
        }
        let mut ambiguous: BTreeSet<(String, String)> = BTreeSet::new();
        for ((target, name, desc), owners) in &method_owners {
            if owners.len() > 1 {
                for mixin in owners {
                    ambiguous.insert((mixin.clone(), target.clone()));
                    weave.skipped.push(SkippedMixinPlan {
                        target: MethodRef {
                            owner: target.clone(),
                            name: name.clone(),
                            descriptor: desc.clone(),
                        },
                        callback: MethodRef {
                            owner: mixin.clone(),
                            name: name.clone(),
                            descriptor: desc.clone(),
                        },
                        at: "@Mixin(implements)".to_owned(),
                        reason: MixinSkipReason::AmbiguousTarget,
                    });
                }
            }
        }

        for (plan, _) in candidates {
            if ambiguous.contains(&(plan.mixin.clone(), plan.target.clone())) {
                continue;
            }
            summary.applied_mixins.insert(plan.mixin.clone());
            weave.interface_mixins.push(plan);
        }
        weave
            .interface_mixins
            .sort_by(|a, b| (&a.target, &a.mixin).cmp(&(&b.target, &b.mixin)));
        summary
    }

    /// The first unwoven member of `@Mixin` `class`, ignoring members already handled by
    /// applied interface-injection plans (or by an accounted interface-mixin skip row).
    pub(crate) fn unwoven_member_for_class(
        &self,
        class: &ClassFile,
        summary: &InterfaceWeaveSummary,
    ) -> Option<(String, String)> {
        if summary.accounted_mixins.contains(&class.name) {
            return None;
        }
        let is_applied = summary.applied_mixins.contains(&class.name);
        let (field_anns, method_anns) = if is_applied {
            member_annotations(class).unwrap_or_default()
        } else {
            Default::default()
        };

        let field = class
            .fields
            .iter()
            .find(|field| {
                if field.access & ACC_SYNTHETIC != 0 {
                    return false;
                }
                if !is_applied {
                    return true;
                }
                let key = format!("{}|{}", field.name, field.descriptor);
                has_annotation(field_anns.get(&key), MIXIN_SHADOW_DESCRIPTOR)
            })
            .map(|field| (field.name.clone(), field.descriptor.clone()));

        let method = class
            .methods
            .iter()
            .find(|method| {
                if method.is_static()
                    || method.name == "<init>"
                    || method.access & ACC_SYNTHETIC != 0
                {
                    return false;
                }
                if method
                    .annotations
                    .iter()
                    .any(|annotation| is_planned_method_annotation(&annotation.descriptor))
                {
                    return false;
                }
                if resolve_mixin_accessor(class, method).is_some() {
                    return false;
                }
                if !is_applied {
                    return true;
                }
                let key = format!("{}|{}", method.name, method.descriptor);
                let invis = method_anns.get(&key);
                if invis.is_some_and(|list| list.iter().any(|d| is_planned_method_annotation(d))) {
                    return false;
                }
                let is_shadow = has_annotation(invis, MIXIN_SHADOW_DESCRIPTOR)
                    || method
                        .annotations
                        .iter()
                        .any(|a| a.descriptor == MIXIN_SHADOW_DESCRIPTOR);
                is_shadow || method.code.is_none() || method.access & ACC_ABSTRACT != 0
            })
            .map(|method| (method.name.clone(), method.descriptor.clone()));

        field.or(method)
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Build or fetch the cached index of active `@Mixin` classes across the primary
    /// jar and all linked jars.
    fn runtime_mixin_index(&self) -> &RuntimeMixinIndex {
        self.mixin_index.cache.index.get_or_init(|| {
            let mut index = RuntimeMixinIndex::default();
            let mut seen_mixins = BTreeSet::new();

            // 1. Primary jar classes.
            let primary_has_no_configs = self.mixin_index.configs.is_empty();
            for class in self.classes.values() {
                if !seen_mixins.insert(class.name.clone()) {
                    continue;
                }
                let reg = MixinRegistration {
                    is_primary: true,
                    primary_has_no_configs,
                };
                index.register_class(
                    class,
                    reg,
                    |target| self.mixin_index.allows_config_gated(&class.name, target),
                    |target| self.classes.contains_key(target),
                    |name| self.class(name),
                );
            }

            // 2. Linked jar classes (gated by each linked jar's own mixin configs and plugins).
            if let Some(side) = self.mixin_index.side {
                for set in self.linked.mixin_config_sets() {
                    let plugins = match side {
                        MixinSide::Server => &set.server_plugins,
                        MixinSide::Client => &set.client_plugins,
                    };
                    for class_name in &set.classes {
                        if !seen_mixins.insert(class_name.clone()) {
                            continue;
                        }
                        let Some(class) = self.class(class_name) else {
                            continue;
                        };
                        let reg = MixinRegistration {
                            is_primary: false,
                            primary_has_no_configs: false,
                        };
                        index.register_class(
                            class,
                            reg,
                            |target| {
                                mixin_config_gate_with_plugins(
                                    &set.configs,
                                    &set.config_sides,
                                    plugins,
                                    class_name,
                                    target,
                                    side,
                                )
                                .is_ok()
                            },
                            |target| self.classes.contains_key(target),
                            |name| self.class(name),
                        );
                    }
                }
            }

            // Also record non-config-listed linked mixin classes' shadow metadata so
            // calling/reading them cannot silently bypass `@Shadow` checks.
            for (class_name, class) in self.linked.all_classes() {
                if !seen_mixins.insert(class_name.clone()) {
                    continue;
                }
                if mixin_annotations(class).is_empty() {
                    continue;
                }
                let reg = MixinRegistration {
                    is_primary: false,
                    primary_has_no_configs: false,
                };
                let unconfigured = !self.mixin_index.require_config;
                index.register_class(
                    class,
                    reg,
                    |_target| unconfigured,
                    |target| self.classes.contains_key(target),
                    |name| self.class(name),
                );
            }

            index
        })
    }

    /// Active `@Mixin` classes targeting `class` (internal name), across the primary jar
    /// and linked jars, respecting config gating.
    pub(crate) fn mixins_targeting(&self, class: &str) -> &[String] {
        self.runtime_mixin_index()
            .by_target
            .get(class)
            .map_or(&[], Vec::as_slice)
    }

    /// Declared vanilla classes of a game handle (using host override when running under
    /// `GameHookHost`, falling back to `game_object::declared_types`).
    pub(crate) fn game_handle_types(&self, key: &str) -> Vec<String> {
        if let Some(types) = self.host.game_object_types(key) {
            return types;
        }
        crate::game_object::declared_types(key)
            .iter()
            .map(|ty| (*ty).to_owned())
            .collect()
    }

    /// Whether `r`, run by `invokespecial` in a frame of `frame_class` on a game object, is a
    /// `@Mixin` class's constructor delegating to its superclass (`super()`): Mixin merges
    /// only the instructions after that call into the target's constructors
    /// (`Constructor.getRange` starts the initialiser after the delegating `<init>`), so on
    /// the target it is no call at all.
    pub(crate) fn is_unmerged_mixin_super_init(
        &self,
        op: u8,
        r: &MethodRef,
        receiver: Option<&Value>,
        frame_class: &str,
    ) -> bool {
        op == 0xb7
            && r.name == "<init>"
            && receiver.and_then(crate::game_object::handle_key).is_some()
            && self.runtime_mixin_index().meta.contains_key(frame_class)
    }

    /// Run a mixin's no-argument `<init>()V` field initialiser once per `(mixin, handle_key)`
    /// pair before its first method invocation or field access on that game handle.
    pub(crate) fn ensure_game_mixin_initialized(
        &mut self,
        mixin: &str,
        receiver: &Value,
    ) -> Result<(), NativeError> {
        let Some(key) = crate::game_object::handle_key(receiver).map(str::to_owned) else {
            return Ok(());
        };
        let has_initializer = self
            .runtime_mixin_index()
            .meta
            .get(mixin)
            .is_some_and(|meta| meta.has_initializer);
        if !has_initializer {
            return Ok(());
        }
        if !self.host.mark_game_mixin_initialized(mixin, &key) {
            return Ok(());
        }
        self.invoke(mixin, "<init>", "()V", Some(receiver.clone()), &[])?;
        Ok(())
    }

    /// Dispatch a method call on a game handle through an active interface-injection Mixin
    /// (or redirect a `@Shadow` method on a Mixin class to its target on the host).
    ///
    /// Returns `Ok(None)` when the call does not target an injected interface or a Mixin
    /// class for this receiver.
    pub(crate) fn invoke_interface_mixin(
        &mut self,
        r: &MethodRef,
        op: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        let Some(handle_key) = crate::game_object::handle_key(receiver).map(str::to_owned) else {
            return Ok(None);
        };
        if r.owner == "java/lang/Object" || r.name == "<init>" {
            return Ok(None);
        }

        let declared = self.game_handle_types(&handle_key);

        // 1. Direct call on a `@Mixin` class (e.g. `this.helper()` or `@Shadow` call).
        let direct_action = {
            let index = self.runtime_mixin_index();
            index.meta.get(&r.owner).map(|meta| {
                // The target method's name drops the `@Shadow` prefix (`shadow$getId`
                // stands for `getId`).
                let shadow_name = meta
                    .shadow_methods
                    .get(&(r.name.clone(), r.descriptor.clone()))
                    .cloned();
                let shadow_target = if shadow_name.is_some() {
                    resolve_shadow_target(&meta.targets, &declared).map(str::to_owned)
                } else {
                    None
                };
                let active_on_handle = declared
                    .iter()
                    .any(|ty| meta.active_interface_targets.contains(ty.as_str()));
                (shadow_name, shadow_target, active_on_handle)
            })
        };
        if let Some((shadow_name, shadow_target, active_on_handle)) = direct_action {
            if let Some(shadow_name) = shadow_name {
                return match shadow_target {
                    Some(owner) => {
                        let target_ref = MethodRef {
                            owner,
                            name: shadow_name,
                            descriptor: r.descriptor.clone(),
                        };
                        self.host
                            .invoke_instance(&target_ref, receiver, args)
                            .map(Some)
                    }
                    // Ambiguous multi-target `@Shadow`: count/refuse loudly rather than
                    // guessing `targets.first()`.
                    None => {
                        self.host.unknown_operand_call(r)?;
                        Ok(Some(Value::default_for(return_desc(&r.descriptor))))
                    }
                };
            }
            if active_on_handle
                && self
                    .class(&r.owner)
                    .and_then(|file| file.method(&r.name, &r.descriptor))
                    .is_some_and(|m| !m.is_static() && m.code.is_some())
            {
                self.ensure_game_mixin_initialized(&r.owner, receiver)?;
                return self
                    .invoke(
                        &r.owner,
                        &r.name,
                        &r.descriptor,
                        Some(receiver.clone()),
                        args,
                    )
                    .map(Some);
            }
            return Ok(None);
        }

        // Non-virtual `invokespecial` on an interface (`I.super.m()`) runs that interface's
        // concrete default directly without overriding back into the mixin.
        if op == 0xb7 {
            if self
                .class(&r.owner)
                .and_then(|file| file.method(&r.name, &r.descriptor))
                .is_some_and(|m| !m.is_static() && m.code.is_some())
            {
                return Ok(None);
            }
            return Ok(None);
        }

        // 2. Virtual/interface call targeting an interface added by an active interface-injection
        //    mixin for one of this handle's declared vanilla types.
        enum Resolution {
            None,
            Mixin(String),
            Ambiguous,
            UnknownTargetHierarchy,
            TargetClassMethod(String),
            InterfaceDefault(String),
            DefaultConflict(Vec<String>),
            UnimplementedInterface,
        }

        let resolution = {
            let index = self.runtime_mixin_index();
            let mut matching_mixins_by_level: Vec<Vec<String>> = Vec::new();
            let mut any_matching_mixin = false;
            let mut all_active_mixins: Vec<String> = Vec::new();

            for target_type in &declared {
                let Some(mixins) = index.interface_by_target.get(target_type.as_str()) else {
                    continue;
                };
                let mut level_impls = Vec::new();
                for active in mixins {
                    if !all_active_mixins.contains(&active.mixin) {
                        all_active_mixins.push(active.mixin.clone());
                    }
                    let implements_owner = active
                        .interfaces
                        .iter()
                        .any(|iface| iface == &r.owner || self.interface_extends(iface, &r.owner));
                    if implements_owner {
                        any_matching_mixin = true;
                    }
                    if active
                        .added_public_methods
                        .contains(&(r.name.clone(), r.descriptor.clone()))
                        && !level_impls.contains(&active.mixin)
                    {
                        level_impls.push(active.mixin.clone());
                    }
                }
                if !level_impls.is_empty() {
                    matching_mixins_by_level.push(level_impls);
                }
            }

            if !any_matching_mixin {
                Resolution::None
            } else if let Some(top_level) = matching_mixins_by_level.into_iter().next() {
                if top_level.len() > 1 {
                    Resolution::Ambiguous
                } else {
                    Resolution::Mixin(top_level.into_iter().next().unwrap_or_default())
                }
            } else {
                // JVMS §5.4.3.3 / §5.4.3.4 ("classes win over interfaces" + maximally-specific
                // superinterface defaults):
                // 1. Any method declared by the target's vanilla/JDK superclass chain wins over
                //    every interface default and dispatches through the host.
                // 2. Any missing edge in the target's hierarchy fails closed (counted/refused).
                // 3. Otherwise combine the target's vanilla/JDK interface defaults with all
                //    injected interfaces' declarations: two unrelated defaults conflict with
                //    `IncompatibleClassChangeError`.
                let table = vanilla_host_methods();
                match table.check_target_methods(&declared, &r.name, &r.descriptor) {
                    VanillaMethodCheck::ClassMethod(target_owner) => {
                        Resolution::TargetClassMethod(target_owner.to_owned())
                    }
                    VanillaMethodCheck::MissingHierarchyEdge(_) => {
                        Resolution::UnknownTargetHierarchy
                    }
                    VanillaMethodCheck::VanillaDefaults(vanilla_defaults) => {
                        // Collect all non-static, non-private declarations across the full
                        // superinterface closure of all active mixins on this target.
                        let mut queue = VecDeque::new();
                        for mixin in &all_active_mixins {
                            if let Some(file) = self.class(mixin) {
                                queue.extend(file.interface_names());
                            }
                        }
                        let mut seen_ifaces = Vec::new();
                        // (owner, has_body, is_vanilla_host_default)
                        let mut all_declaring: Vec<(String, bool, bool)> = vanilla_defaults
                            .into_iter()
                            .map(|owner| (owner.to_owned(), true, true))
                            .collect();
                        while let Some(iface) = queue.pop_front() {
                            if seen_ifaces.len() >= crate::MAX_SUPERCLASS_WALK {
                                break;
                            }
                            if seen_ifaces.contains(&iface) {
                                continue;
                            }
                            if let Some(file) = self.class(&iface) {
                                queue.extend(file.interface_names());
                                if let Some(m) = file.method(&r.name, &r.descriptor)
                                    && !m.is_static()
                                    && m.access & ACC_PRIVATE == 0
                                    && !all_declaring.iter().any(|(o, _, _)| o == &iface)
                                {
                                    all_declaring.push((iface.clone(), m.code.is_some(), false));
                                }
                                seen_ifaces.push(iface);
                            } else if let Some(info) = table.interfaces.get(iface.as_str()) {
                                queue.extend(info.interfaces.iter().map(|s| (*s).to_owned()));
                                if info
                                    .methods
                                    .contains(&(r.name.as_str(), r.descriptor.as_str()))
                                    && !all_declaring.iter().any(|(o, _, _)| o == &iface)
                                {
                                    all_declaring.push((iface.clone(), true, true));
                                }
                                seen_ifaces.push(iface);
                            }
                        }
                        let maximal: Vec<&(String, bool, bool)> = all_declaring
                            .iter()
                            .filter(|(iface, _, _)| {
                                !all_declaring.iter().any(|(other, _, _)| {
                                    other != iface
                                        && iface_extends_with_vanilla(self, table, other, iface)
                                })
                            })
                            .collect();
                        let concrete: Vec<&(String, bool, bool)> = maximal
                            .into_iter()
                            .filter(|(_, has_body, _)| *has_body)
                            .collect();
                        match concrete.as_slice() {
                            [] => Resolution::UnimplementedInterface,
                            [(owner, _, true)] => Resolution::TargetClassMethod(owner.clone()),
                            [(owner, _, false)] => Resolution::InterfaceDefault(owner.clone()),
                            many => Resolution::DefaultConflict(
                                many.iter().map(|(owner, _, _)| owner.clone()).collect(),
                            ),
                        }
                    }
                }
            }
        };

        match resolution {
            Resolution::None => Ok(None),
            Resolution::Mixin(mixin) => {
                self.ensure_game_mixin_initialized(&mixin, receiver)?;
                self.invoke(&mixin, &r.name, &r.descriptor, Some(receiver.clone()), args)
                    .map(Some)
            }
            Resolution::Ambiguous | Resolution::UnknownTargetHierarchy => {
                self.host.unknown_operand_call(r)?;
                Ok(Some(Value::default_for(return_desc(&r.descriptor))))
            }
            Resolution::TargetClassMethod(owner) => {
                let target_ref = MethodRef {
                    owner,
                    name: r.name.clone(),
                    descriptor: r.descriptor.clone(),
                };
                self.host
                    .invoke_instance(&target_ref, receiver, args)
                    .map(Some)
            }
            Resolution::InterfaceDefault(iface) => self
                .invoke(&iface, &r.name, &r.descriptor, Some(receiver.clone()), args)
                .map(Some),
            Resolution::DefaultConflict(owners) => Err(NativeError::UncaughtException {
                class: "java/lang/IncompatibleClassChangeError".into(),
                message: Some(format!(
                    "Conflicting default methods: {}",
                    owners
                        .iter()
                        .map(|owner| format!("{owner}.{}", r.name))
                        .collect::<Vec<_>>()
                        .join(" ")
                )),
                frame: None,
            }),
            Resolution::UnimplementedInterface => {
                self.host.invoke_instance(r, receiver, args).map(Some)
            }
        }
    }

    /// Resolve a `getstatic`/`putstatic` naming a `@Mixin` class's static `@Shadow` field
    /// (or a static it inherits from a game superclass) the way the merged class does.
    ///
    /// Mixin rewrites every `M.field` reference to `Target.field` and merges the code into
    /// the target, so:
    /// - a mixin real Mixin rejects ([`invalid_mixin_reason`]) is never applied, and an
    ///   access through its shadow has no answer;
    /// - a multi-target mixin's shadow names no single field here;
    /// - on a jar-local target the shadow must be a static field the target class itself
    ///   declares with that descriptor (`TargetClassContext.findAliasedField`, then the
    ///   `STATIC modifier` check), else Mixin rejects the class;
    /// - the field's own access flags (after any applied `@Mutable` cleared `ACC_FINAL`)
    ///   decide a write, never `@Final` ([`StaticShadowWrite`]).
    ///
    /// Anything without an exact answer is [`StaticShadowResolution::Unresolved`], which
    /// the caller counts or refuses.
    #[inline(never)]
    pub(crate) fn resolve_static_shadow(&self, field: &MethodRef) -> StaticShadowResolution {
        enum Lookup {
            Shadow(String),
            Inherited(String),
            Own,
            Unresolved,
        }
        let lookup = {
            let index = self.runtime_mixin_index();
            let Some(meta) = index.meta.get(&field.owner) else {
                return StaticShadowResolution::NotShadow;
            };
            let member = (field.name.clone(), field.descriptor.clone());
            let single_target = || resolve_shadow_target(&meta.targets, &[]).map(str::to_owned);
            if meta.static_shadow_fields.contains_key(&member) {
                match single_target() {
                    Some(target) if meta.invalid.is_none() => Lookup::Shadow(target),
                    _ => Lookup::Unresolved,
                }
            } else if meta.extends_game_class && !meta.declared_chain_fields.contains(&member) {
                match single_target() {
                    Some(target) if meta.invalid.is_none() => Lookup::Inherited(target),
                    _ => Lookup::Unresolved,
                }
            } else if meta.invalid.is_none() {
                Lookup::Own
            } else {
                return StaticShadowResolution::NotShadow;
            }
        };
        match lookup {
            Lookup::Unresolved => StaticShadowResolution::Unresolved,
            Lookup::Own => self.resolve_own_mixin_static(field),
            Lookup::Shadow(target) => {
                // A field an earlier mixin merged into the target: the shared slot.
                if let Some(merged) = self.merged_static_declared_before(&target, field) {
                    return merged;
                }
                let Some(file) = self.class(&target) else {
                    // A host-modelled game class: the host answers the read, and no write
                    // reaches the game.
                    return StaticShadowResolution::Resolved {
                        target: MethodRef {
                            owner: target,
                            ..field.clone()
                        },
                        write: StaticShadowWrite::HostField,
                    };
                };
                let Some(declared) = file
                    .fields
                    .iter()
                    .find(|f| f.name == field.name && f.descriptor == field.descriptor)
                else {
                    // "@Shadow field ... was not located in the target class": aliases and
                    // refmap remapping are not followed here, so this has no exact answer.
                    return StaticShadowResolution::Unresolved;
                };
                if declared.access & ACC_STATIC == 0 {
                    // "STATIC modifier of @Shadow field ... does not match the target".
                    return StaticShadowResolution::Unresolved;
                }
                let target_ref = MethodRef {
                    owner: target,
                    ..field.clone()
                };
                let write = self.jar_static_write(&target_ref, &target_ref.owner, declared.access);
                StaticShadowResolution::Resolved {
                    target: target_ref,
                    write,
                }
            }
            Lookup::Inherited(target) => {
                // An ordinary field reference after the owner rewrite: JVM resolution
                // through the target's own hierarchy.
                let symbolic = MethodRef {
                    owner: target,
                    ..field.clone()
                };
                let declaring = self.field_owner(&symbolic);
                let access = self.class(&declaring).and_then(|file| {
                    file.fields
                        .iter()
                        .find(|f| f.name == field.name && f.descriptor == field.descriptor)
                        .map(|f| f.access)
                });
                let write = match access {
                    Some(access) => self.jar_static_write(&symbolic, &declaring, access),
                    None => StaticShadowWrite::HostField,
                };
                StaticShadowResolution::Resolved {
                    target: MethodRef {
                        owner: declaring,
                        ..symbolic
                    },
                    write,
                }
            }
        }
    }

    /// A `@Mixin` class's own (non-shadow) static field `field`, once Mixin merged it into
    /// the target ([`StaticShadowResolution::Merged`]).
    ///
    /// - a mixin applied to no target keeps its own slot (`NotShadow`): real Mixin never
    ///   runs its code, so only a direct embedder call reaches it;
    /// - a mixin applied to two or more targets has one copy of the field per target,
    ///   which its code reaches through whichever target it was merged into: no single
    ///   slot holds that, so the access is unresolved;
    /// - on a target that already declares a field of that name and descriptor (a jar
    ///   class's fields, a game class's from `vanilla_host_methods.tsv`), Mixin renames a
    ///   private `@Unique` field, discards a public one, or takes a plain field for the
    ///   target's own: none of that is modelled, so the access is unresolved, as it is on
    ///   a game class the table does not hold.
    fn resolve_own_mixin_static(&self, field: &MethodRef) -> StaticShadowResolution {
        let Some(declared) = self.class(&field.owner).and_then(|file| {
            file.fields
                .iter()
                .find(|f| f.name == field.name && f.descriptor == field.descriptor)
                .map(|f| f.access)
        }) else {
            return StaticShadowResolution::NotShadow;
        };
        if declared & ACC_STATIC == 0 {
            return StaticShadowResolution::NotShadow;
        }
        let targets = self
            .runtime_mixin_index()
            .meta
            .get(&field.owner)
            .map(|meta| meta.targets.clone())
            .unwrap_or_default();
        let applied: Vec<&String> = targets
            .iter()
            .filter(|target| {
                self.mixins_applied_to_target(target)
                    .iter()
                    .any(|mixin| mixin == &field.owner)
            })
            .collect();
        let target = match applied.as_slice() {
            [] => return StaticShadowResolution::NotShadow,
            [target] if targets.len() == 1 => (*target).clone(),
            _ => return StaticShadowResolution::Unresolved,
        };
        if self.target_declares_field(&target, field) != Some(false) {
            return StaticShadowResolution::Unresolved;
        }
        // A mixin applied earlier merged a static of that name and descriptor first: this
        // one's then collides with it (the same unmodelled rename / discard / adopt).
        let (order, exact) = self.static_declarers_in_apply_order(&target, field, &field.owner);
        let earlier = order
            .iter()
            .take_while(|(mixin, _)| *mixin != field.owner)
            .any(|(_, access)| access.is_some());
        let others = order
            .iter()
            .any(|(mixin, access)| *mixin != field.owner && access.is_some());
        if earlier || (others && !exact) {
            return StaticShadowResolution::Unresolved;
        }
        self.merged_static(&target, &field.owner, field, declared)
    }

    /// Whether `target`'s own class declares a field with `field`'s name and descriptor
    /// (what `TargetClassContext.findAliasedField` matches): the jar class's fields, or a
    /// game class's from `vanilla_host_methods.tsv` (`None` when the table does not hold
    /// that game class, which then has no exact answer).
    fn target_declares_field(&self, target: &str, field: &MethodRef) -> Option<bool> {
        match self.class(target) {
            Some(file) => Some(
                file.fields
                    .iter()
                    .any(|f| f.name == field.name && f.descriptor == field.descriptor),
            ),
            None => vanilla_host_methods().declares_field(target, &field.name, &field.descriptor),
        }
    }

    /// The shared slot of `field` when a mixin applied to `target` before the one whose
    /// `@Shadow` this is declares it as its own static: `mergeNewFields` added it to the
    /// target before this shadow's `findAliasedField` looked. The first such mixin in apply
    /// order is the one whose field is there; a later one's collides with it. A field
    /// declared only by a mixin applied later is not there yet, so Mixin rejects the shadow
    /// (unresolved), and so is an apply order this runtime cannot read exactly.
    fn merged_static_declared_before(
        &self,
        target: &str,
        field: &MethodRef,
    ) -> Option<StaticShadowResolution> {
        let (order, exact) = self.static_declarers_in_apply_order(target, field, &field.owner);
        let first = order
            .iter()
            .position(|(mixin, access)| *mixin != field.owner && access.is_some())?;
        if !exact {
            return Some(StaticShadowResolution::Unresolved);
        }
        let shadow = order.iter().position(|(mixin, _)| *mixin == field.owner)?;
        let (declaring, access) = &order[first];
        // The declaring mixin's own field must itself merge as one slot.
        let single_target = self
            .runtime_mixin_index()
            .meta
            .get(declaring)
            .is_some_and(|meta| meta.targets.len() == 1);
        if first < shadow
            && single_target
            && self.target_declares_field(target, field) == Some(false)
            && let Some(access) = access
        {
            return Some(self.merged_static(target, declaring, field, *access));
        }
        Some(StaticShadowResolution::Unresolved)
    }

    /// The mixins applied to `target` that declare `field` (name and descriptor) as a
    /// static of their own (not a `@Shadow`), each with its access flags, and `with`
    /// (`None` flags when it declares no such field), in the order real Mixin applies
    /// them to `target`: `MixinInfo.compareTo` sorts a target's mixins by `@Mixin(priority)`,
    /// then by the order their `MixinInfo`s were made, which is config order
    /// (`MixinConfig.compareTo`: the config's `priority`, then its registration) and the
    /// position in the config's `mixins` list, then its side's list
    /// ([`crate::mixin_apply_order_key`]). The flag is `false` when that order is not known:
    /// two or more of them and one from a linked jar, whose configs' registration order
    /// against this jar's is not modelled.
    fn static_declarers_in_apply_order(
        &self,
        target: &str,
        field: &MethodRef,
        with: &str,
    ) -> (Vec<(String, Option<u16>)>, bool) {
        let member = (field.name.clone(), field.descriptor.clone());
        let mut order: Vec<(String, Option<u16>)> = self
            .mixins_applied_to_target(target)
            .into_iter()
            .filter_map(|mixin| {
                let shadow = self
                    .runtime_mixin_index()
                    .meta
                    .get(&mixin)
                    .is_some_and(|meta| meta.shadow_fields.contains(&member));
                let access = self
                    .class(&mixin)
                    .and_then(|file| {
                        file.fields
                            .iter()
                            .find(|f| {
                                f.name == field.name
                                    && f.descriptor == field.descriptor
                                    && f.is_static()
                            })
                            .map(|f| f.access)
                    })
                    .filter(|_| !shadow);
                (access.is_some() || mixin == with).then_some((mixin, access))
            })
            .collect();
        if !order.iter().any(|(mixin, _)| mixin == with) {
            order.push((with.to_owned(), None));
        }
        let exact = order.len() < 2
            || order
                .iter()
                .all(|(mixin, _)| self.classes.contains_key(mixin.as_str()));
        order.sort_by_cached_key(|(mixin, _)| {
            (
                crate::mixin_apply_order_key(
                    self.classes.get(mixin.as_str()),
                    mixin,
                    &self.mixin_index.configs,
                    self.mixin_index.side,
                    &self.mixin_index.config_sides,
                ),
                mixin.clone(),
            )
        });
        (order, exact)
    }

    /// The [`StaticShadowResolution::Merged`] slot `target#name` of a static `declaring`
    /// merged into `target`, with the write rule of its access flags.
    fn merged_static(
        &self,
        target: &str,
        declaring: &str,
        field: &MethodRef,
        access: u16,
    ) -> StaticShadowResolution {
        let slot = MethodRef {
            owner: target.to_owned(),
            ..field.clone()
        };
        let write = if access & ACC_FINAL == 0 || self.mutable_shadow_applied(target, &slot) {
            StaticShadowWrite::Writable
        } else {
            StaticShadowWrite::Final {
                symbolic_owner: target.to_owned(),
                declaring: target.to_owned(),
            }
        };
        StaticShadowResolution::Merged {
            slot,
            init: declaring.to_owned(),
            write,
        }
    }

    /// Resolve a call to static method `name descriptor` of `owner` when it is a `@Mixin`
    /// class's static `@Shadow` method.
    ///
    /// A static `@Shadow` method needs a body to compile, but it is only a placeholder:
    /// Mixin merges no body for it and rewrites every `invokestatic M.method` to
    /// `Target.method`, with the `@Shadow` prefix removed (`prepareShadow`). The target
    /// method must be one the target class itself declares with the same descriptor and
    /// `static` modifier (`TargetClassContext.findAliasedMethod`, then "STATIC modifier of
    /// ... does not match the target" in `attachSpecialMethod`), else Mixin rejects the
    /// class. A host-modelled game target is the host's to answer, as any static call to
    /// it is.
    #[inline(never)]
    pub(crate) fn resolve_static_shadow_method(
        &self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> StaticShadowCall {
        // Cheap checks first: this runs at every `invokestatic`.
        let Some(file) = self.class(owner) else {
            return StaticShadowCall::NotShadow;
        };
        if !file
            .annotations
            .iter()
            .any(|annotation| annotation.descriptor == crate::MIXIN_ANNOTATION_DESCRIPTOR)
            || !file
                .method(name, descriptor)
                .is_some_and(|method| method.is_static())
        {
            return StaticShadowCall::NotShadow;
        }
        let (target, target_name) = {
            let index = self.runtime_mixin_index();
            let Some(meta) = index.meta.get(owner) else {
                return StaticShadowCall::NotShadow;
            };
            let Some(target_name) = meta
                .shadow_methods
                .get(&(name.to_owned(), descriptor.to_owned()))
            else {
                return StaticShadowCall::NotShadow;
            };
            if meta.invalid.is_some() {
                return StaticShadowCall::Unresolved;
            }
            let Some(target) = resolve_shadow_target(&meta.targets, &[]) else {
                return StaticShadowCall::Unresolved;
            };
            (target.to_owned(), target_name.clone())
        };
        if let Some(target_file) = self.class(&target)
            && !target_file
                .method(&target_name, descriptor)
                .is_some_and(|method| method.is_static())
        {
            return StaticShadowCall::Unresolved;
        }
        StaticShadowCall::Target(MethodRef {
            owner: target,
            name: target_name,
            descriptor: descriptor.to_owned(),
        })
    }

    /// Mixin's rewrite of a merged method's `invokestatic M.method` of its own static
    /// `@Shadow` method (`MixinTargetContext.transformMethodRef`, the renamed,
    /// prefix-free name included) to the target, so the call then takes the ordinary path
    /// of a call to that target ([`Self::resolve_static_shadow_method`]). Only the mixin's
    /// own code (or its nested classes) is rewritten: any other code naming the mixin class
    /// cannot load it (`mixin_class_loading.rs`). An unresolved shadow is left for
    /// [`Self::invoke_static_shadow_body`] to count.
    #[inline(never)]
    pub(crate) fn rewritten_static_shadow_call(
        &self,
        frame_class: &str,
        r: &MethodRef,
    ) -> Option<MethodRef> {
        let merged_self = frame_class == r.owner
            || frame_class
                .strip_prefix(r.owner.as_str())
                .is_some_and(|rest| rest.starts_with('$'));
        if merged_self
            && let StaticShadowCall::Target(target) =
                self.resolve_static_shadow_method(&r.owner, &r.name, &r.descriptor)
        {
            return Some(target);
        }
        None
    }

    /// The targets `@Mixin` class `mixin` names (applied or not), from the index.
    pub(crate) fn mixin_targets(&self, mixin: &str) -> Vec<String> {
        self.runtime_mixin_index()
            .meta
            .get(mixin)
            .map(|meta| meta.targets.clone())
            .unwrap_or_default()
    }

    /// Never run a static `@Shadow` method's placeholder body: a frame about to run one
    /// (reached through a method reference, a handle, or an unresolved call site) calls
    /// the target instead, or counts the call when there is no exact target.
    #[inline(never)]
    pub(crate) fn invoke_static_shadow_body(
        &mut self,
        class: &str,
        method: &crate::Method,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !method.is_static() {
            return Ok(None);
        }
        match self.resolve_static_shadow_method(class, &method.name, &method.descriptor) {
            StaticShadowCall::NotShadow => Ok(None),
            StaticShadowCall::Unresolved => {
                self.host.unknown_operand_call(&MethodRef {
                    owner: class.to_owned(),
                    name: method.name.clone(),
                    descriptor: method.descriptor.clone(),
                })?;
                Ok(Some(Value::default_for(return_desc(&method.descriptor))))
            }
            StaticShadowCall::Target(target) if self.has_class(&target.owner) => self
                .invoke(&target.owner, &target.name, &target.descriptor, None, args)
                .map(Some),
            StaticShadowCall::Target(target) => self.host.invoke(&target, args).map(Some),
        }
    }

    /// How a `putstatic` of jar field `field` (declared by `declaring` with `access`) is
    /// judged: [`StaticShadowWrite::Writable`] unless the field is `ACC_FINAL` and no
    /// applied `@Shadow @Mutable` cleared that flag.
    fn jar_static_write(
        &self,
        field: &MethodRef,
        declaring: &str,
        access: u16,
    ) -> StaticShadowWrite {
        if access & ACC_FINAL == 0 || self.mutable_shadow_applied(declaring, field) {
            return StaticShadowWrite::Writable;
        }
        StaticShadowWrite::Final {
            symbolic_owner: field.owner.clone(),
            declaring: declaring.to_owned(),
        }
    }

    /// Whether a mixin applied to jar class `target` declares `field` (by name and
    /// descriptor) as a static `@Shadow @Mutable`, so `mergeShadowFields` cleared the
    /// target field's `ACC_FINAL` for every writer, not only that mixin.
    fn mutable_shadow_applied(&self, target: &str, field: &MethodRef) -> bool {
        let member = (field.name.clone(), field.descriptor.clone());
        self.mixins_applied_to_jar_target(target)
            .into_iter()
            .any(|mixin| {
                self.runtime_mixin_index()
                    .meta
                    .get(&mixin)
                    .and_then(|meta| meta.static_shadow_fields.get(&member))
                    .is_some_and(|shadow| shadow.is_mutable)
            })
    }

    /// The `@Mixin` classes real Mixin applies to jar class `target`: the index's
    /// `by_target` entries (which leave out rejected classes), with primary-jar classes
    /// also passing the config gate (`by_target` keeps them config-free for type checks).
    pub(crate) fn mixins_applied_to_jar_target(&self, target: &str) -> Vec<String> {
        if !self.has_class(target) {
            return Vec::new();
        }
        self.mixins_applied_to_target(target)
    }

    /// The `@Mixin` classes real Mixin applies to `target`, a jar class or a game class:
    /// the index's `by_target` entries (which leave out rejected classes), with primary-jar
    /// classes also passing the config gate.
    pub(crate) fn mixins_applied_to_target(&self, target: &str) -> Vec<String> {
        self.mixins_targeting(target)
            .iter()
            .filter(|mixin| {
                !self.classes.contains_key(mixin.as_str())
                    || self.mixin_index.allows_config_gated(mixin, target)
            })
            .cloned()
            .collect()
    }

    /// Why class `class` (a jar or a game class) cannot load, when a `required` Mixin
    /// config's rejected mixin targets it: Mixin rethrows the `InvalidMixinException` and
    /// Fabric's Knot fails the class load ([`NativeJar::failed_required_targets`]).
    pub(crate) fn required_mixin_failure(&self, class: &str) -> Option<&FailedTarget> {
        self.mixin_index.failed_required_targets.get(class)
    }

    /// The game class whose failed load stops the game before any mod runs, with why: a
    /// certain failure of a class no jar declares that a real Fabric 26.3 dedicated server
    /// loads before its first `main` entrypoint
    /// ([`crate::mixin_class_loading::server_loads_before_mods`], measured, not guessed).
    /// Any other failed class (one the game loads later or never, or any on the client,
    /// whose bootstrap is not measured) is counted where the jar's code first uses it
    /// ([`Self::class_load_failure`]).
    pub(crate) fn game_fails_before_mods(&self) -> Option<(&str, &FailedTarget)> {
        if self.mixin_index.side != Some(MixinSide::Server) {
            return None;
        }
        self.mixin_index
            .failed_required_targets
            .iter()
            .find(|(target, failure)| {
                failure.certain
                    && !self.has_class(target)
                    && crate::mixin_class_loading::server_loads_before_mods(target)
            })
            .map(|(target, failure)| (target.as_str(), failure))
    }

    /// Refuse an entry into the jar's code when the game does not start
    /// ([`Self::game_fails_before_mods`]).
    #[inline(never)]
    pub(crate) fn refuse_failed_game_mixin_target(&self) -> Result<(), NativeError> {
        match self.game_fails_before_mods() {
            Some((target, failure)) => {
                let binary = target.replace('/', ".");
                Err(NativeError::Metadata(format!(
                    "the game does not start: {}; Mixin rethrows for a required config and Fabric's Knot fails the game's own load of {binary} (\"Mixin transformation of {binary} failed\"), which a 26.3 dedicated server makes before any mod initializer runs",
                    failure.reason
                )))
            }
            None => Ok(()),
        }
    }

    /// The one jar class whose static initialiser absorbs `class`'s, when `class` is a
    /// `@Mixin` applied to exactly one target and that target is a jar class.
    ///
    /// `MixinApplicatorStandard.appendInsns` inserts the mixin's `<clinit>` before the
    /// target `<clinit>`'s first `RETURN` (or makes it the target's `<clinit>`), and
    /// `mergeNewFields` moves its static fields onto the target. So the mixin's statics
    /// are initialised by, and only by, the target's initialisation. A multi-target
    /// mixin's initialiser runs once per target with per-target copies of its statics,
    /// which one static slot per mixin cannot hold, so it keeps running on its own.
    pub(crate) fn merged_initializer_target(&self, class: &str) -> Option<String> {
        let target = {
            let meta = self.runtime_mixin_index().meta.get(class)?;
            if meta.invalid.is_some() {
                return None;
            }
            match meta.targets.as_slice() {
                [target] => target.clone(),
                _ => return None,
            }
        };
        self.mixins_applied_to_jar_target(&target)
            .iter()
            .any(|mixin| mixin == class)
            .then_some(target)
    }

    /// The mixins whose static initialisers Mixin appends to jar class `target`'s, in the
    /// order Mixin applies them (see [`Self::merged_initializer_target`]), and whether
    /// that order is known exactly.
    ///
    /// The apply order is `@Mixin(priority)`, then the config registration order
    /// ([`crate::mixin_apply_order_key`]), which is read here only for the primary jar's
    /// own configs. With two or more initialisers and one of them from a linked jar, the
    /// order is not known: the list is then sorted by name and the caller counts it.
    pub(crate) fn merged_initializers(&self, target: &str) -> (Vec<String>, bool) {
        let mut merged: Vec<String> = self
            .mixins_applied_to_jar_target(target)
            .into_iter()
            .filter(|mixin| self.merged_initializer_target(mixin).as_deref() == Some(target))
            .filter(|mixin| {
                self.class(mixin)
                    .and_then(|file| file.method("<clinit>", "()V"))
                    .is_some_and(|clinit| clinit.code.is_some())
            })
            .collect();
        if merged.len() < 2 {
            return (merged, true);
        }
        if merged
            .iter()
            .any(|mixin| !self.classes.contains_key(mixin.as_str()))
        {
            merged.sort();
            return (merged, false);
        }
        merged.sort_by_cached_key(|mixin| {
            (
                crate::mixin_apply_order_key(
                    self.classes.get(mixin.as_str()),
                    mixin,
                    &self.mixin_index.configs,
                    self.mixin_index.side,
                    &self.mixin_index.config_sides,
                ),
                mixin.clone(),
            )
        });
        (merged, true)
    }

    /// The class a `putstatic` executing in a frame of `frame_class` really runs in:
    /// Mixin merges a mixin's code into its target, so a frame of a `@Mixin` class
    /// targeting `target` (or of `target` itself) is `target`'s.
    pub(crate) fn merged_frame_class(&self, frame_class: &str, target: &str) -> String {
        let merged = frame_class == target
            || self
                .runtime_mixin_index()
                .meta
                .get(frame_class)
                .is_some_and(|meta| meta.targets.iter().any(|t| t == target));
        if merged {
            target.to_owned()
        } else {
            frame_class.to_owned()
        }
    }

    /// What the JVM does with a `putstatic` of `ACC_FINAL` field `name` (named through
    /// `symbolic_owner`, declared by `declaring`) executed by `method` of `frame_class`.
    ///
    /// HotSpot `LinkResolver::resolve_field`: a final field can be updated only by its
    /// declaring class, and (class files of version 53 and later) only from `<clinit>`.
    /// The messages are HotSpot's, checked on JDK 25 against a Mixin-merged class (the
    /// trailing space included). A mixin's `<clinit>` runs inside the target's, so it
    /// is the target's initialiser. Below version 53 the JVM allows the update from any
    /// method, but Mixin may raise the target's version to the mixin's requirement
    /// (`applyAttributes`), so that case is [`FinalStaticPut::Unknown`].
    pub(crate) fn judge_final_static_put(
        &self,
        symbolic_owner: &str,
        declaring: &str,
        name: &str,
        frame_class: &str,
        method: &str,
    ) -> FinalStaticPut {
        let external = |class: &str| class.replace('/', ".");
        let current = self.merged_frame_class(frame_class, symbolic_owner);
        if declaring != current {
            return FinalStaticPut::Illegal(format!(
                "Update to static final field {}.{name} attempted from a different class ({}) than the field's declaring class",
                external(symbolic_owner),
                external(&current),
            ));
        }
        if method == "<clinit>" {
            return FinalStaticPut::Allowed;
        }
        match self.class_major_version(declaring) {
            Some(version) if version >= FINAL_PUT_CLINIT_ONLY_VERSION => {
                FinalStaticPut::Illegal(format!(
                    "Update to static final field {}.{name} attempted from a different method ({method}) than the initializer method <clinit> ",
                    external(symbolic_owner),
                ))
            }
            _ => FinalStaticPut::Unknown,
        }
    }

    /// The class-file major version of jar class `class`, if its bytes are kept.
    pub(crate) fn class_major_version(&self, class: &str) -> Option<u16> {
        let raw = &self.class(class)?.raw;
        raw.get(6..8)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Handle `getfield` on a game handle for `@Shadow` fields (routing to the host so
    /// unmodelled shadows are counted/refused) and for added fields of active
    /// interface-injection mixins (ensuring `<init>()V` ran and returning Java defaults
    /// without counting an unknown host call).
    pub(crate) fn game_handle_get_field(
        &mut self,
        receiver: &Value,
        r: &MethodRef,
    ) -> Result<Option<Value>, NativeError> {
        let Some(handle_key) = crate::game_object::handle_key(receiver).map(str::to_owned) else {
            return Ok(None);
        };
        let declared = self.game_handle_types(&handle_key);
        enum Action {
            None,
            Shadow(Option<MethodRef>),
            AddedMixinField,
        }
        let action = {
            let index = self.runtime_mixin_index();
            match index.meta.get(&r.owner) {
                Some(meta) => {
                    let member = (r.name.clone(), r.descriptor.clone());
                    if meta.shadow_fields.contains(&member)
                        || (meta.extends_game_class
                            && !meta.declared_chain_fields.contains(&member))
                    {
                        // A mixin real Mixin rejects is never applied: its shadow names
                        // nothing, so the read counts like an ambiguous one.
                        let target_ref = resolve_shadow_target(&meta.targets, &declared)
                            .filter(|_| meta.invalid.is_none())
                            .map(|owner| MethodRef {
                                owner: owner.to_owned(),
                                name: r.name.clone(),
                                descriptor: r.descriptor.clone(),
                            });
                        Action::Shadow(target_ref)
                    } else if meta.added_instance_fields.contains(&member)
                        && declared
                            .iter()
                            .any(|ty| meta.active_interface_targets.contains(ty.as_str()))
                    {
                        Action::AddedMixinField
                    } else {
                        Action::None
                    }
                }
                None => Action::None,
            }
        };
        match action {
            Action::None => Ok(None),
            Action::Shadow(Some(target_ref)) => {
                // Under `GameHookHost`, pass the original mixin ref so its `@Shadow` error
                // diagnostic names both the mixin field and `@Shadow`.
                let query = if self.host.defers_game_field_write_check() {
                    r
                } else {
                    &target_ref
                };
                self.host.get_field(receiver, query).map(Some)
            }
            Action::Shadow(None) => {
                // Ambiguous multi-target `@Shadow`: count/refuse loudly.
                if self.host.defers_game_field_write_check() {
                    self.host.get_field(receiver, r).map(Some)
                } else {
                    self.host.unknown_operand_call(r)?;
                    Ok(Some(Value::default_for(&r.descriptor)))
                }
            }
            Action::AddedMixinField => {
                self.ensure_game_mixin_initialized(&r.owner, receiver)?;
                let value = instance_field_key(receiver, r)
                    .and_then(|key| self.fields.get(&key).cloned())
                    .unwrap_or_else(|| Value::default_for(&r.descriptor));
                Ok(Some(value))
            }
        }
    }

    /// Handle `putfield` on a game handle for added fields of active interface-injection
    /// mixins (ensuring `<init>()V` ran first) and for unmodelled `@Shadow` / game-class
    /// field writes (counted/refused loudly instead of silently faking a host mutation).
    pub(crate) fn game_handle_put_field(
        &mut self,
        receiver: &Value,
        r: &MethodRef,
        value: &Value,
    ) -> Result<GameFieldWrite, NativeError> {
        let Some(handle_key) = crate::game_object::handle_key(receiver).map(str::to_owned) else {
            return Ok(GameFieldWrite::Fallthrough);
        };
        let declared = self.game_handle_types(&handle_key);
        enum Action {
            Fallthrough,
            AddedMixinField,
            UnmodelledGameField(MethodRef),
        }
        let action = {
            let index = self.runtime_mixin_index();
            match index.meta.get(&r.owner) {
                Some(meta) => {
                    let member = (r.name.clone(), r.descriptor.clone());
                    let is_shadow_or_inherited_game = meta.shadow_fields.contains(&member)
                        || (meta.extends_game_class
                            && !meta.declared_chain_fields.contains(&member));
                    if is_shadow_or_inherited_game {
                        if self.host.defers_game_field_write_check() {
                            Action::Fallthrough
                        } else {
                            let target_owner = resolve_shadow_target(&meta.targets, &declared)
                                .unwrap_or(r.owner.as_str())
                                .to_owned();
                            Action::UnmodelledGameField(MethodRef {
                                owner: target_owner,
                                name: r.name.clone(),
                                descriptor: r.descriptor.clone(),
                            })
                        }
                    } else if meta.added_instance_fields.contains(&member)
                        && declared
                            .iter()
                            .any(|ty| meta.active_interface_targets.contains(ty.as_str()))
                    {
                        Action::AddedMixinField
                    } else {
                        Action::Fallthrough
                    }
                }
                None if !self.has_class(&r.owner)
                    && is_game_class(&r.owner)
                    && !self.host.defers_game_field_write_check() =>
                {
                    Action::UnmodelledGameField(r.clone())
                }
                None => Action::Fallthrough,
            }
        };
        match action {
            Action::Fallthrough => Ok(GameFieldWrite::Fallthrough),
            Action::AddedMixinField => {
                self.ensure_game_mixin_initialized(&r.owner, receiver)?;
                if let Some(key) = instance_field_key(receiver, r) {
                    self.fields.insert(key, value.clone());
                }
                Ok(GameFieldWrite::Stored)
            }
            Action::UnmodelledGameField(target_ref) => {
                self.host.unknown_operand_call(&target_ref)?;
                Ok(GameFieldWrite::Handled)
            }
        }
    }
}

impl RuntimeMixinIndex {
    fn register_class<'a>(
        &mut self,
        class: &ClassFile,
        reg: MixinRegistration,
        config_active_for_target: impl Fn(&str) -> bool,
        is_jar_local: impl Fn(&str) -> bool,
        lookup: impl Fn(&str) -> Option<&'a ClassFile>,
    ) {
        let Some(details) = inspect_mixin_class(class, lookup) else {
            return;
        };
        let is_interface_injection = !details.is_interface && !details.interfaces.is_empty();
        let mut active_interface_targets = BTreeSet::new();
        // A class real Mixin rejects (`InvalidMixinException`) is applied to no target:
        // it adds no interface, takes no part in type checks or dispatch, and appends no
        // static initialiser. Its metadata stays so its shadows count instead of resolving.
        let targets = if details.invalid.is_some() {
            &[][..]
        } else {
            &details.targets[..]
        };
        for target in targets {
            let is_external_host_game_target =
                !is_jar_local(target) && is_host_modelled_game_target(target);
            let config_active = config_active_for_target(target);
            let (include_in_by_target, include_in_interface_dispatch) = if reg.is_primary {
                if is_external_host_game_target && is_interface_injection {
                    // Host-game-object interface-injection Mixins are always strictly
                    // config-, side-, and plugin-gated and require a runnable no-arg initialiser.
                    let active = config_active && !details.unrunnable_initializer;
                    (active, active)
                } else if is_external_host_game_target {
                    // Accessor/invoker interfaces on host-modelled game targets keep the
                    // config-less probe scan when the jar has no Mixin configs, and obey
                    // the config/plugin gate when it does.
                    (reg.primary_has_no_configs || config_active, false)
                } else {
                    // Jar-local woven classes (e.g. `demo/Woven$Target`) and non-host-game
                    // targets (e.g. `ArrayBackedEvent`) preserve the existing unconditional
                    // primary-jar `by_target` scan and never enter host-object dispatch.
                    (true, false)
                }
            } else {
                // Linked-jar Mixins are always config- and plugin-gated.
                let active = config_active && !details.unrunnable_initializer;
                (
                    active,
                    active && is_external_host_game_target && is_interface_injection,
                )
            };

            if include_in_by_target {
                let mixins = self.by_target.entry(target.clone()).or_default();
                if !mixins.contains(&details.mixin) {
                    mixins.push(details.mixin.clone());
                }
            }
            if include_in_interface_dispatch {
                active_interface_targets.insert(target.clone());
                self.interface_by_target
                    .entry(target.clone())
                    .or_default()
                    .push(ActiveInterfaceMixin {
                        mixin: details.mixin.clone(),
                        interfaces: details.interfaces.clone(),
                        added_public_methods: details.added_public_methods.clone(),
                    });
            }
        }
        self.meta.insert(
            details.mixin,
            MixinClassMeta {
                targets: details.targets,
                active_interface_targets,
                added_instance_fields: details.added_instance_fields,
                shadow_fields: details.shadow_fields,
                static_shadow_fields: details.static_shadow_fields,
                shadow_methods: details.shadow_methods,
                has_initializer: details.has_initializer,
                extends_game_class: details.extends_game_class,
                declared_chain_fields: details.declared_chain_fields,
                invalid: details.invalid,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_modelled_game_targets_are_strictly_sorted() {
        assert!(
            HOST_MODELLED_GAME_TARGETS.windows(2).all(|w| w[0] < w[1]),
            "HOST_MODELLED_GAME_TARGETS must stay sorted for binary_search"
        );
        assert!(is_host_modelled_game_target(
            "net/minecraft/world/entity/player/Player"
        ));
        assert!(is_host_modelled_game_target(
            "net/minecraft/server/level/ServerPlayer"
        ));
        assert!(!is_host_modelled_game_target(
            "net/minecraft/world/level/chunk/LevelChunk"
        ));
    }

    #[test]
    fn vanilla_host_methods_table_records_declared_fields() {
        let table = vanilla_host_methods();
        // Vanilla 26.3's own `ItemStack.EMPTY` and `Player`'s fields are in the table; a
        // name the class does not declare is not; a class outside it has no answer.
        let stack = "net/minecraft/world/item/ItemStack";
        assert_eq!(
            table.declares_field(stack, "EMPTY", "Lnet/minecraft/world/item/ItemStack;"),
            Some(true)
        );
        assert_eq!(table.declares_field(stack, "EMPTY", "I"), Some(false));
        assert_eq!(table.declares_field(stack, "SHARED", "I"), Some(false));
        assert_eq!(
            table.declares_field(
                "net/minecraft/resources/Identifier",
                "path",
                "Ljava/lang/String;"
            ),
            None
        );
        let fields: usize = table
            .classes
            .values()
            .chain(table.interfaces.values())
            .map(|info| info.fields.len())
            .sum();
        assert!(fields > 1000, "{fields}");
    }

    #[test]
    fn vanilla_host_methods_table_is_hierarchy_closed_and_includes_jdk_defaults() {
        let table = vanilla_host_methods();
        for target in HOST_MODELLED_GAME_TARGETS {
            assert!(
                table.classes.contains_key(target) || table.interfaces.contains_key(target),
                "HOST_MODELLED_GAME_TARGETS entry {target} missing from vanilla_host_methods.tsv"
            );
        }
        for (cname, info) in &table.classes {
            if !info.super_name.is_empty() && info.super_name != "java/lang/Object" {
                assert!(
                    table.classes.contains_key(info.super_name),
                    "class {cname} superclass {} missing from vanilla_host_methods.tsv",
                    info.super_name
                );
            }
            for &iface in &info.interfaces {
                assert!(
                    table.interfaces.contains_key(iface),
                    "class {cname} interface {iface} missing from vanilla_host_methods.tsv"
                );
            }
        }
        for (iname, info) in &table.interfaces {
            for &parent in &info.interfaces {
                assert!(
                    table.interfaces.contains_key(parent),
                    "interface {iname} superinterface {parent} missing from vanilla_host_methods.tsv"
                );
            }
        }
        let inv = vec!["net/minecraft/world/entity/player/Inventory".to_owned()];
        assert_eq!(
            table.check_target_methods(&inv, "forEach", "(Ljava/util/function/Consumer;)V"),
            VanillaMethodCheck::VanillaDefaults(vec!["java/lang/Iterable"]),
        );
        assert_eq!(
            table.check_target_methods(&inv, "spliterator", "()Ljava/util/Spliterator;"),
            VanillaMethodCheck::VanillaDefaults(vec!["java/lang/Iterable"]),
        );
        let player = vec![
            "net/minecraft/server/level/ServerPlayer".to_owned(),
            "net/minecraft/world/entity/player/Player".to_owned(),
        ];
        assert_eq!(
            table.check_target_methods(&player, "getId", "()I"),
            VanillaMethodCheck::ClassMethod("net/minecraft/world/entity/Entity"),
        );
        assert_eq!(
            table.check_target_methods(&player, "getScore", "()I"),
            VanillaMethodCheck::ClassMethod("net/minecraft/world/entity/player/Player"),
        );
    }

    #[test]
    fn check_target_methods_fails_closed_on_missing_hierarchy_edge() {
        let synthetic = VanillaHostMethods::parse(
            "C\tdemo/MissingSuper\tdemo/NotInTable\t\n\
             C\tdemo/HasIface\tjava/lang/Object\tdemo/IfaceWithMissingParent\n\
             I\tdemo/IfaceWithMissingParent\t\tdemo/MissingParent\n",
        );
        assert_eq!(
            synthetic.check_target_methods(&["demo/MissingSuper".to_owned()], "foo", "()V"),
            VanillaMethodCheck::MissingHierarchyEdge("demo/NotInTable".to_owned()),
        );
        assert_eq!(
            synthetic.check_target_methods(&["demo/HasIface".to_owned()], "foo", "()V"),
            VanillaMethodCheck::MissingHierarchyEdge("demo/MissingParent".to_owned()),
        );
        assert_eq!(
            synthetic.check_target_methods(&["demo/UnknownStart".to_owned()], "foo", "()V"),
            VanillaMethodCheck::MissingHierarchyEdge("demo/UnknownStart".to_owned()),
        );
    }
}
