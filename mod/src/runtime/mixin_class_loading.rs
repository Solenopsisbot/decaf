//! Mixin's class-loading rule: which classes of a mod real Fabric refuses to load.
//!
//! sponge-mixin 0.17.5 (`MixinProcessor.applyMixins`) runs on every class Knot loads.
//! A class whose binary name starts with a selected Mixin config's package
//! (`MixinConfig.packageMatch`, the config's `package` plus `.`) is refused with
//! `IllegalClassLoadError` ("... cannot be referenced directly" / "... is in a defined
//! mixin package ..."), unless:
//!
//! - a coprocessor passes it through: an accessor mixin a selected config lists
//!   (`MixinCoprocessorPassthrough` registers `MixinInfo.isLoadable()`, true only for
//!   `SubType.Accessor`, an interface mixin whose methods are all `@Accessor`/`@Invoker`);
//! - it is an `InjectionPoint` subclass, or an `ITargetSelectorDynamic` implementation
//!   (`ClassInfo.hasSuperClass`, interfaces included only for the interface).
//!
//! fabric-loader 0.20.1's Knot rethrows the error as `RuntimeException: Mixin
//! transformation of <class> failed` from the class load. Measured on a real Fabric
//! 26.3 server, from a mod's `onInitialize`: the context class loader and a mod class's
//! own loader (both `KnotClassLoader`) throw exactly that from `loadClass`, a second
//! attempt the same, and `Class.forName` of the class or of an array of it (any
//! dimension: the JVM loads the element class) as well; Knot's `loadClass` of an array
//! name is a `ClassNotFoundException`, with nothing transformed. The system class loader
//! is the JDK's `AppClassLoader`, which never sees a mod class
//! (`ClassNotFoundException`, in a mixin package or not); the runtime does not model it,
//! so `getSystemClassLoader()` is counted. The runtime counts a refused load rather than
//! throwing it (the exception's cause, and how a bytecode resolution surfaces it, are not
//! modelled), and runs nothing for it.
//!
//! Two more loads fail the same way ([`Runtime::class_load_failure`]):
//!
//! - a class a `required` Mixin config's rejected mixin targets: Mixin rethrows from that
//!   class's transformation ([`NativeJar::failed_required_targets`], which a config
//!   plugin's `shouldApplyMixin` gates as Mixin does). When the class is a game class a
//!   real Fabric 26.3 dedicated server loads before any mod's initializer runs
//!   ([`server_loads_before_mods`], measured with `-Xlog:class+load`), the game does not
//!   start, and the jar's code is refused as a whole
//!   ([`Runtime::refuse_failed_game_mixin_target`]);
//! - a class whose superclass or a superinterface fails to load (the JVM loads them with
//!   it), a jar class through the class files and a game class through
//!   `vanilla_host_methods.tsv`.
//!
//! Only a load that surely fails refuses: the game not starting, or an entrypoint whose
//! own load surely fails. A load that may fail and cannot be told
//! ([`LoadFailure::Unknown`]: a config plugin answer that is not known, a game supertype
//! outside the table while a game class failed) is counted wherever it happens, the
//! entrypoint's own load included, and never refused.
//!
//! A mixin's own code is merged into its target, and Mixin rewrites a merged method's
//! references to its own class (`MixinTargetContext.transformMethodRef` /
//! `transformFieldRef`, and inner classes through `innerClasses`): those never load the
//! mixin class. So a reference to a mixin package is refused only from code that is not
//! merged with the class it names ([`Runtime::is_unloadable_reference`]).
//!
//! The one point that sees every bytecode class reference is the constant pool: `new`,
//! `anewarray`, `multianewarray`, `checkcast`, `instanceof`, `ldc` of a class, method type
//! or handle, field and method references and `invokedynamic` all resolve a pool entry.
//! Each frame looks up once which of its class's pool entries would load a refused class
//! ([`Runtime::unloadable_pool_entries`]); an instruction resolving one is counted and
//! skipped with its stack effect ([`Runtime::skip_unloadable_instruction`]). A load by
//! name goes through [`Runtime::is_unloadable_by_name`], with no merged exemption (Mixin
//! rewrites bytecode references, not strings): `Class.forName` and Knot's `loadClass`
//! from a call, a method reference or a method handle
//! ([`Runtime::count_refused_reflective_load`]), a `ServiceLoader` provider, and the
//! loader's own entrypoint load ([`Runtime::entrypoint_load_failure`]). A call on a game
//! object whose class failed to load is counted too ([`Runtime::count_failed_game_receiver`]):
//! real Fabric never made such an object.
//!
//! Mixin config plugins run inside Mixin's own selection and application, where the
//! processor is re-entrant and loads pass through: nothing is refused while one runs.

use super::*;
use std::sync::Arc;

/// Mixin's custom injection-point base class; a subclass may live in a mixin package.
const INJECTION_POINT: &str = "org/spongepowered/asm/mixin/injection/InjectionPoint";
/// Mixin's dynamic target-selector interface; an implementation may live there too.
const TARGET_SELECTOR_DYNAMIC: &str =
    "org/spongepowered/asm/mixin/injection/selectors/ITargetSelectorDynamic";

/// The name of a counted class load ([`Runtime::skip_unloadable_instruction`]).
const LOAD: &str = "<load>";

/// The game classes a real Fabric 26.3 dedicated server loads before its first `main`
/// entrypoint, measured with `-Xlog:class+load` (`tools/trace_fabric_bootstrap_classes.py`).
const SERVER_BOOTSTRAP_CLASSES: &str = crate::runtime::vendored_fabric_server_bootstrap_classes_26_3_txt::VENDORED_DATA;

/// Whether a real Fabric 26.3 dedicated server loads game class `class` (internal name)
/// before any mod's initializer runs ([`SERVER_BOOTSTRAP_CLASSES`]). Every such load goes
/// through Mixin: its processor is up before the first game class loads.
pub(crate) fn server_loads_before_mods(class: &str) -> bool {
    static CLASSES: std::sync::OnceLock<BTreeSet<&'static str>> = std::sync::OnceLock::new();
    CLASSES
        .get_or_init(|| {
            SERVER_BOOTSTRAP_CLASSES
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .collect()
        })
        .contains(class)
}

/// The class an array name's elements are (`[[Lfoo/Bar;` is `foo/Bar`), the name itself
/// for a plain class, `None` for an array of primitives or a malformed array name.
fn loaded_class(name: &str) -> Option<String> {
    match name.strip_prefix('[') {
        Some(_) => descriptor_classes(name).into_iter().next(),
        None => Some(name.to_owned()),
    }
}

/// What a class load does on real Fabric ([`Runtime::class_load_failure`]), ordered so
/// the worst of a class's supertypes is its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LoadFailure {
    /// It loads.
    Loads,
    /// It may fail, and this runtime cannot tell (a config plugin answer that is not
    /// known, a game supertype outside `vanilla_host_methods.tsv`): counted at first use,
    /// never refused.
    Unknown,
    /// It surely fails.
    Fails,
}

/// `ClassLoader.loadClass(String)` / `loadClass(String, boolean)` on the runtime's one
/// loader marker (`getSystemClassLoader()`, `Thread.getContextClassLoader()`,
/// `Class.getClassLoader()` all answer it), whatever class the call names as its owner.
fn is_loader_load_class(r: &MethodRef, receiver: Option<&Value>) -> bool {
    r.name == "loadClass"
        && matches!(
            r.descriptor.as_str(),
            "(Ljava/lang/String;)Ljava/lang/Class;" | "(Ljava/lang/String;Z)Ljava/lang/Class;"
        )
        && matches!(receiver, Some(Value::Ref(Some(marker))) if marker.starts_with("classloader/"))
}

/// The classes resolving pool entry `index` of `file` loads (JVMS §5.4.3): a class
/// entry's class (an array's element class), a field or method reference's class, a
/// method handle's member's class, every class a method type's descriptor names, and for
/// an `invokedynamic` its call-site descriptor's classes, bootstrap method and static
/// arguments.
pub(crate) fn pool_entry_classes(file: &ClassFile, index: u16) -> Vec<String> {
    let mut out = Vec::new();
    collect_pool_entry_classes(file, index, &mut out, 0);
    out
}

fn collect_pool_entry_classes(file: &ClassFile, index: u16, out: &mut Vec<String>, depth: u8) {
    if depth > 4 {
        return;
    }
    match file.pool.get(index as usize) {
        Some(Cp::Class(name)) => {
            if let Ok(name) = utf8(&file.pool, *name) {
                match name.strip_prefix('[') {
                    Some(_) => out.extend(descriptor_classes(&name)),
                    None => out.push(name),
                }
            }
        }
        Some(
            Cp::FieldRef { class, .. }
            | Cp::MethodRef { class, .. }
            | Cp::InterfaceMethodRef { class, .. },
        ) => collect_pool_entry_classes(file, *class, out, depth + 1),
        Some(Cp::MethodHandle {
            reference_index, ..
        }) => collect_pool_entry_classes(file, *reference_index, out, depth + 1),
        Some(Cp::MethodType { descriptor }) => {
            if let Ok(descriptor) = utf8(&file.pool, *descriptor) {
                out.extend(descriptor_classes(&descriptor));
            }
        }
        Some(Cp::InvokeDynamic { bootstrap, nt }) => {
            if let Some(Cp::NameType { descriptor, .. }) = file.pool.get(*nt as usize)
                && let Ok(descriptor) = utf8(&file.pool, *descriptor)
            {
                out.extend(descriptor_classes(&descriptor));
            }
            if let Some(method) = file.bootstrap_methods.get(*bootstrap as usize) {
                collect_pool_entry_classes(file, method.method_ref, out, depth + 1);
                for argument in &method.arguments {
                    collect_pool_entry_classes(file, *argument, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// Every class a field or method descriptor names (`Lfoo/Bar;`, arrays' elements).
fn descriptor_classes(descriptor: &str) -> Vec<String> {
    let mut classes = Vec::new();
    let mut rest = descriptor;
    while let Some(start) = rest.find('L') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            break;
        };
        classes.push(after[..end].to_owned());
        rest = &after[end + 1..];
    }
    classes
}

/// Opcodes whose operand is a constant-pool index they resolve, with the index's width
/// and the instruction's length after the opcode byte.
fn pool_operand(op: u8) -> Option<(bool, usize)> {
    match op {
        // ldc: a one-byte index.
        0x12 => Some((false, 1)),
        // ldc_w; get/putstatic, get/putfield, invokevirtual/special/static; new;
        // anewarray; checkcast; instanceof.
        0x13 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 => Some((true, 2)),
        // invokeinterface and invokedynamic carry two more bytes.
        0xb9 | 0xba => Some((true, 4)),
        // multianewarray: index and dimensions.
        0xc5 => Some((true, 3)),
        _ => None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The internal-name prefixes (`demo/`) of every Mixin config selected on this side,
    /// the primary jar's and each linked jar's.
    fn mixin_package_prefixes(&self) -> &[String] {
        self.mixin_index.cache.package_prefixes.get_or_init(|| {
            let Some(side) = self.mixin_index.side else {
                return Vec::new();
            };
            let selected = |configs: &[MixinConfig], sides: &BTreeMap<String, MixinSide>| {
                configs
                    .iter()
                    .filter(|config| sides.get(&config.path).is_none_or(|only| *only == side))
                    .filter_map(|config| config.package.as_deref())
                    .filter(|package| !package.is_empty())
                    .map(|package| format!("{}/", package.replace('.', "/").trim_end_matches('/')))
                    .collect::<Vec<_>>()
            };
            let mut prefixes = selected(&self.mixin_index.configs, &self.mixin_index.config_sides);
            for set in self.linked.mixin_config_sets() {
                prefixes.extend(selected(&set.configs, &set.config_sides));
            }
            prefixes.sort();
            prefixes.dedup();
            prefixes
        })
    }

    /// Whether loading class `name` (internal name; an array loads its element class) by
    /// name, from anywhere, fails on real Fabric or may (module docs): it is in a selected
    /// mixin package and no coprocessor or library-type exemption applies, or its load
    /// fails otherwise or cannot be told ([`Self::class_load_failure`]). Either way the
    /// load is counted, never answered.
    #[inline(never)]
    pub(crate) fn is_unloadable_by_name(&self, name: &str) -> bool {
        if crate::runtime::mixin_plugins::running_config_plugins() {
            return false;
        }
        let Some(name) = loaded_class(name) else {
            return false;
        };
        self.in_refused_mixin_package(&name)
            || self.class_load_failure(&name, 0) != LoadFailure::Loads
    }

    /// What the loader's own load of entrypoint class `class` does on real Fabric:
    /// `Some(why)` when it surely fails, which stops the loader there (refused): the
    /// class is in a mixin package, or its load or a supertype's surely fails
    /// ([`LoadFailure::Fails`]). A failure that cannot be told (a config plugin answer
    /// that is not known, a game supertype outside `vanilla_host_methods.tsv`) is counted
    /// here and the entrypoint runs: `None`, as for a class that loads.
    pub(crate) fn entrypoint_load_failure(
        &mut self,
        class: &str,
    ) -> Result<Option<String>, NativeError> {
        if crate::runtime::mixin_plugins::running_config_plugins() {
            return Ok(None);
        }
        let binary = class.replace('/', ".");
        if self.in_refused_mixin_package(class) {
            return Ok(Some(format!(
                "entrypoint {binary} is in a Mixin config's package: Mixin refuses to load it (IllegalClassLoadError) and Fabric's Knot fails the load (\"Mixin transformation of {binary} failed\")"
            )));
        }
        match self.class_load_failure(class, 0) {
            LoadFailure::Loads => Ok(None),
            LoadFailure::Unknown => self
                .host
                .unknown_operand_call(&MethodRef {
                    owner: class.to_owned(),
                    name: LOAD.to_owned(),
                    descriptor: "()V".to_owned(),
                })
                .map(|_| None),
            LoadFailure::Fails => {
                if let Some(failure) = self.required_mixin_failure(class) {
                    return Ok(Some(format!(
                        "entrypoint {binary} fails to load: {}; Mixin rethrows for a required config and Fabric's Knot throws \"Mixin transformation of {binary} failed\"",
                        failure.reason
                    )));
                }
                let parent = self
                    .class(class)
                    .map(|file| {
                        std::iter::once(file.super_name.clone())
                            .chain(file.interface_names())
                            .find(|parent| {
                                self.in_refused_mixin_package(parent)
                                    || self.class_load_failure(parent, 1) == LoadFailure::Fails
                            })
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                Ok(Some(format!(
                    "entrypoint {binary} fails to load: its supertype {} does not load on real Fabric (a Mixin package or a failed required Mixin target)",
                    parent.replace('/', ".")
                )))
            }
        }
    }

    /// Mixin's own refusal ([`Self::is_unloadable_by_name`]'s first half).
    fn in_refused_mixin_package(&self, name: &str) -> bool {
        if !self
            .mixin_package_prefixes()
            .iter()
            .any(|prefix| name.starts_with(prefix.as_str()))
        {
            return false;
        }
        if let Ok(guard) = self.mixin_index.cache.refused_mixin_package.read()
            && let Some(&cached) = guard.get(name)
        {
            return cached;
        }
        let refused = !self.is_passthrough_accessor(name) && !self.is_mixin_library_subtype(name);
        if let Ok(mut guard) = self.mixin_index.cache.refused_mixin_package.write()
            && guard.len() < 65_536
        {
            guard.insert(name.to_owned(), refused);
        }
        refused
    }

    /// Whether class `name`'s load fails for any reason but its own mixin package:
    ///
    /// - a `required` config's rejected mixin targets it
    ///   ([`NativeJar::failed_required_targets`]): Mixin rethrows from the class's
    ///   transformation and Knot fails the load ([`LoadFailure::Unknown`] when that hangs
    ///   on a config plugin answer that is not known);
    /// - a jar class's superclass or superinterface fails to load: the JVM loads them with
    ///   the class (JVMS §5.3.5), so the class fails with them;
    /// - a game class whose supertypes include a failed game class (`ServerPlayer` when
    ///   `Player` failed), by `vanilla_host_methods.tsv`. While any game class has failed,
    ///   a game class that table does not hold the hierarchy of cannot be told apart:
    ///   [`LoadFailure::Unknown`].
    ///
    /// A surely failing supertype makes the class fail; else an unknown one makes it
    /// unknown.
    pub(crate) fn class_load_failure(&self, name: &str, depth: usize) -> LoadFailure {
        let failed = &self.mixin_index.failed_required_targets;
        if let Some(failure) = failed.get(name) {
            return if failure.certain {
                LoadFailure::Fails
            } else {
                LoadFailure::Unknown
            };
        }
        if self.mixin_package_prefixes().is_empty() && failed.is_empty() {
            return LoadFailure::Loads;
        }
        if depth >= MAX_SUPERCLASS_WALK {
            return LoadFailure::Loads;
        }
        if let Ok(guard) = self.mixin_index.cache.load_failures.read()
            && let Some(&known) = guard.get(name)
        {
            return known;
        }
        let outcome = match self.class(name) {
            Some(file) => std::iter::once(file.super_name.as_str())
                .chain(file.interfaces().iter().map(String::as_str))
                .filter(|parent| !parent.is_empty())
                .map(|parent| {
                    if self.in_refused_mixin_package(parent) {
                        LoadFailure::Fails
                    } else {
                        self.class_load_failure(parent, depth + 1)
                    }
                })
                .max()
                .unwrap_or(LoadFailure::Loads),
            None => self.game_class_load_failure(name),
        };
        if let Ok(mut guard) = self.mixin_index.cache.load_failures.write()
            && guard.len() < 65_536
        {
            guard.insert(name.to_owned(), outcome);
        }
        outcome
    }

    /// A class no jar declares whose load fails because a failed game class is among its
    /// supertypes ([`Self::class_load_failure`]).
    fn game_class_load_failure(&self, name: &str) -> LoadFailure {
        let failed_game: Vec<(&str, bool)> = self
            .mixin_index
            .failed_required_targets
            .iter()
            .filter(|(target, _)| !self.has_class(target))
            .map(|(target, failure)| (target.as_str(), failure.certain))
            .collect();
        if failed_game.is_empty()
            || library_dispatch::jdk_class_exists(name)
            || crate::runtime::jdk_closure::in_jdk_package(name)
        {
            return LoadFailure::Loads;
        }
        let Some(supertypes) = crate::runtime::interface_mixins::vanilla_supertypes(name) else {
            return LoadFailure::Unknown;
        };
        failed_game
            .iter()
            .filter(|(target, _)| supertypes.contains(target))
            .map(|(_, certain)| {
                if *certain {
                    LoadFailure::Fails
                } else {
                    LoadFailure::Unknown
                }
            })
            .max()
            .unwrap_or(LoadFailure::Loads)
    }

    /// Whether code in `frame_class` resolving a reference to class `named` is refused:
    /// [`Self::is_unloadable_by_name`], unless the frame's code is merged with that class
    /// (module docs): the class itself, a nested class of the same mixin (its inner and
    /// Decaf's woven `M$$HorizonBridge$...` classes), one of the mixin's targets (Decaf's
    /// woven callback calls stand where Mixin merged the callback), or a subclass.
    pub(crate) fn is_unloadable_reference(&self, frame_class: &str, named: &str) -> bool {
        if frame_class == named || !self.is_unloadable_by_name(named) {
            return false;
        }
        let Some(mixin) = self.mixin_root(named) else {
            return true;
        };
        let nested_in_mixin = frame_class == mixin
            || frame_class
                .strip_prefix(mixin.as_str())
                .is_some_and(|rest| rest.starts_with('$'));
        let targets_frame = self
            .mixin_targets(&mixin)
            .iter()
            .any(|target| target == frame_class);
        !(nested_in_mixin || targets_frame || self.extends_jar_class(frame_class, &mixin))
    }

    /// The `@Mixin` class `class` is or is nested in (`M`, `M$1`, `M$$HorizonBridge$...`).
    fn mixin_root(&self, class: &str) -> Option<String> {
        let mut candidate = class;
        loop {
            let is_mixin = self.class(candidate).is_some_and(|file| {
                file.annotations
                    .iter()
                    .any(|annotation| annotation.descriptor == MIXIN_ANNOTATION_DESCRIPTOR)
            });
            if is_mixin {
                return Some(candidate.to_owned());
            }
            candidate = &candidate[..candidate.rfind('$')?];
        }
    }

    /// Whether jar class `class`'s superclass chain includes `ancestor`.
    fn extends_jar_class(&self, class: &str, ancestor: &str) -> bool {
        let mut current = self.class(class).map(|file| file.super_name.clone());
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(name) = current else {
                return false;
            };
            if name == ancestor {
                return true;
            }
            current = self.class(&name).map(|file| file.super_name.clone());
        }
        false
    }

    /// An accessor mixin a selected config lists: an interface `@Mixin` whose every
    /// non-synthetic method is an `@Accessor` or `@Invoker` (`MixinInfo.getVariant`'s
    /// `ACCESSOR`), which `MixinCoprocessorPassthrough` lets load.
    fn is_passthrough_accessor(&self, class: &str) -> bool {
        let Some(file) = self.class(class) else {
            return false;
        };
        let interface =
            crate::runtime::checkcast::class_access(file).is_some_and(|access| access & 0x0200 != 0);
        let is_mixin = file
            .annotations
            .iter()
            .any(|annotation| annotation.descriptor == MIXIN_ANNOTATION_DESCRIPTOR);
        if !interface || !is_mixin {
            return false;
        }
        let (_, methods) = crate::runtime::game_hooks::member_annotations(file).unwrap_or_default();
        let all_accessors = file
            .methods
            .iter()
            .filter(|method| method.access & 0x1000 == 0)
            .all(|method| {
                methods
                    .get(&format!("{}|{}", method.name, method.descriptor))
                    .is_some_and(|descriptors| {
                        descriptors.iter().any(|descriptor| {
                            descriptor == MIXIN_ACCESSOR_DESCRIPTOR
                                || descriptor == MIXIN_INVOKER_DESCRIPTOR
                        })
                    })
            });
        let listed = self.mixin_index.side.is_some_and(|side| {
            crate::runtime::listing_mixin_config(
                &self.mixin_index.configs,
                &self.mixin_index.config_sides,
                class,
                side,
            )
            .is_some()
        }) || self
            .linked
            .mixin_config_sets()
            .iter()
            .any(|set| set.classes.iter().any(|listed| listed == class));
        all_accessors && listed
    }

    /// An `InjectionPoint` subclass (superclass chain) or an `ITargetSelectorDynamic`
    /// implementation (any supertype), which Mixin lets a mixin package hold.
    fn is_mixin_library_subtype(&self, class: &str) -> bool {
        let mut superclass = self.class(class).map(|file| file.super_name.clone());
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(name) = superclass else {
                break;
            };
            if name == INJECTION_POINT {
                return true;
            }
            superclass = self.class(&name).map(|file| file.super_name.clone());
        }
        let mut pending = vec![class.to_owned()];
        let mut seen = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if current == TARGET_SELECTOR_DYNAMIC {
                return true;
            }
            if seen.len() >= MAX_SUPERCLASS_WALK || !seen.insert(current.clone()) {
                continue;
            }
            if let Some(file) = self.class(&current) {
                pending.push(file.super_name.clone());
                pending.extend(file.interface_names());
            }
        }
        false
    }

    /// The pool entries of `class` (whose frames run `file`) that load a class this code
    /// may not load ([`Self::is_unloadable_reference`]), or `None` when there are none.
    /// Cached per class across invocations on the same `(jar, side, linked, precedence)`.
    #[inline(never)]
    pub(crate) fn unloadable_pool_entries(
        &self,
        class: &str,
        file: &ClassFile,
    ) -> Option<Arc<BTreeSet<u16>>> {
        if crate::runtime::mixin_plugins::running_config_plugins()
            || (self.mixin_package_prefixes().is_empty()
                && self.mixin_index.failed_required_targets.is_empty())
        {
            return None;
        }
        if let Ok(guard) = self.mixin_index.cache.unloadable_pools.read()
            && let Some(cached) = guard.get(class)
        {
            return cached.clone();
        }
        let mut refused = BTreeSet::new();
        for index in 1..file.pool.len() {
            let Ok(index) = u16::try_from(index) else {
                break;
            };
            if pool_entry_classes(file, index)
                .iter()
                .any(|named| self.is_unloadable_reference(class, named))
            {
                refused.insert(index);
            }
        }
        let entries = (!refused.is_empty()).then(|| Arc::new(refused));
        if let Ok(mut guard) = self.mixin_index.cache.unloadable_pools.write()
            && guard.len() < 65_536
        {
            guard.insert(class.to_owned(), entries.clone());
        }
        entries
    }

    /// Count an instruction of `class` (whose code is `code`, the opcode `op` just read,
    /// `pc` past it) that resolves a refused pool entry, and give it its stack effect with
    /// the counted placeholder as its result, so nothing it names runs. Returns the next
    /// instruction's `pc`, or `None` when the instruction resolves no refused entry.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn skip_unloadable_instruction(
        &mut self,
        refused: &BTreeSet<u16>,
        class: &str,
        file: &ClassFile,
        op: u8,
        code: &[u8],
        pc: usize,
        stack: &mut Vec<Value>,
    ) -> Result<Option<usize>, NativeError> {
        let Some((wide, length)) = pool_operand(op) else {
            return Ok(None);
        };
        let index = if wide {
            u16::from_be_bytes([
                *code.get(pc).ok_or(NativeError::Truncated)?,
                *code.get(pc + 1).ok_or(NativeError::Truncated)?,
            ])
        } else {
            u16::from(*code.get(pc).ok_or(NativeError::Truncated)?)
        };
        if !refused.contains(&index) {
            return Ok(None);
        }
        // `checkcast` and `instanceof` of `null` resolve nothing (JVMS §6.5): the
        // instruction runs as usual.
        if matches!(op, 0xc0 | 0xc1) && matches!(stack.last(), Some(Value::Ref(None))) {
            return Ok(None);
        }
        let named = pool_entry_classes(file, index)
            .into_iter()
            .find(|named| self.is_unloadable_reference(class, named))
            .unwrap_or_default();
        let element = |name: &str| {
            if name.starts_with('[') {
                name.to_owned()
            } else {
                format!("L{name};")
            }
        };
        // (values popped, result descriptor)
        let (pops, result): (usize, Option<String>) = match op {
            0x12 | 0x13 => (
                0,
                Some(
                    match file.pool.get(index as usize) {
                        Some(Cp::MethodType { .. }) => "Ljava/lang/invoke/MethodType;",
                        Some(Cp::MethodHandle { .. }) => "Ljava/lang/invoke/MethodHandle;",
                        _ => "Ljava/lang/Class;",
                    }
                    .to_owned(),
                ),
            ),
            0xb2 => (0, Some(file.field_ref(index)?.descriptor)),
            0xb3 => (1, None),
            0xb4 => (1, Some(file.field_ref(index)?.descriptor)),
            0xb5 => (2, None),
            0xb6..=0xb9 => {
                let descriptor = file.method_ref(index)?.descriptor;
                let receiver = usize::from(op != 0xb8);
                let returns = return_desc(&descriptor);
                (
                    arg_count(&descriptor) + receiver,
                    (returns != "V").then(|| returns.to_owned()),
                )
            }
            0xba => {
                let descriptor = file.invoke_dynamic_descriptor(index)?;
                let returns = return_desc(&descriptor);
                (
                    arg_count(&descriptor),
                    (returns != "V").then(|| returns.to_owned()),
                )
            }
            0xbb => (0, Some(element(&named))),
            0xbd => (1, Some(format!("[{}", element(&named)))),
            0xc0 => (1, Some(element(&named))),
            0xc1 => (1, Some("I".to_owned())),
            0xc5 => {
                let dimensions = *code.get(pc + 2).ok_or(NativeError::Truncated)?;
                let array = match file.pool.get(index as usize) {
                    Some(Cp::Class(name)) => utf8(&file.pool, *name).ok(),
                    _ => None,
                }
                .unwrap_or_else(|| format!("[{}", element(&named)));
                (usize::from(dimensions), Some(array))
            }
            _ => return Ok(None),
        };
        for _ in 0..pops {
            stack.pop().ok_or_else(|| NativeError::StackUnderflow {
                class: class.to_owned(),
                method: String::new(),
                pc,
            })?;
        }
        let load = MethodRef {
            owner: named,
            name: LOAD.to_owned(),
            descriptor: format!("(){}", result.as_deref().unwrap_or("V")),
        };
        let counted = self.host.unknown_operand_call(&load)?;
        if result.is_some() {
            stack.push(counted);
        }
        Ok(Some(pc + length))
    }

    /// Count a reflective load (`Class.forName`, `ClassLoader.loadClass`, a
    /// `ServiceLoader` provider) of a class real Fabric refuses
    /// ([`Self::is_unloadable_by_name`]; an array name loads its element class, as
    /// `Class.forName("[Lfoo.Bar;", ..)` does through `loader.loadClass("foo.Bar")`): the
    /// counted placeholder stands for the class.
    #[inline(never)]
    pub(crate) fn count_unloadable_class_by_name(
        &mut self,
        name: &str,
    ) -> Result<Option<Value>, NativeError> {
        if !self.is_unloadable_by_name(name) {
            return Ok(None);
        }
        let owner = loaded_class(name).unwrap_or_else(|| name.to_owned());
        self.host
            .unknown_operand_call(&MethodRef {
                owner,
                name: LOAD.to_owned(),
                descriptor: "()Ljava/lang/Class;".to_owned(),
            })
            .map(Some)
    }

    /// A reflective load by name that real Fabric refuses, from every path a call takes
    /// into the library models (a direct call, a method reference, a method handle):
    /// `Class.forName` in each overload, and `ClassLoader.loadClass` on the runtime's
    /// loader, whose Knot implementation runs the class through Mixin. Counted, with the
    /// placeholder as the call's answer; `None` for any other call, or a load that
    /// passes. `loadClass` takes no array name (it answers `ClassNotFoundException`
    /// without transforming anything), so only a plain name is checked there.
    #[inline(never)]
    pub(crate) fn count_refused_reflective_load(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let name = if r.owner == "java/lang/Class"
            && r.name == "forName"
            && r.descriptor.ends_with(")Ljava/lang/Class;")
        {
            let index = usize::from(r.descriptor.starts_with("(Ljava/lang/Module;"));
            args.get(index)
        } else if is_loader_load_class(r, receiver) {
            args.first()
                .filter(|name| !matches!(name, Value::Ref(Some(name)) if name.starts_with('[')))
        } else {
            return Ok(None);
        };
        let Some(Value::Ref(Some(binary))) = name else {
            return Ok(None);
        };
        // A binary name: `java/lang/String` names no class at all.
        if binary.contains('/') {
            return Ok(None);
        }
        let internal = binary.replace('.', "/");
        self.count_unloadable_class_by_name(&internal)
    }

    /// A call on a game object whose class cannot exist: one of its declared types failed
    /// to load or may ([`Self::class_load_failure`]), so real Fabric never made such an object and
    /// the jar's code never reaches it. Counted where the jar's code first uses one, with
    /// the placeholder as the call's answer (or `void`); `None` otherwise. A call naming
    /// the failed class itself is already counted at the constant pool
    /// ([`Self::skip_unloadable_instruction`]); this covers calls that name another type,
    /// such as an interface a mixin injected.
    #[inline(never)]
    pub(crate) fn count_failed_game_receiver(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
    ) -> Result<Option<Value>, NativeError> {
        if self.mixin_index.failed_required_targets.is_empty() {
            return Ok(None);
        }
        let Some(key) = receiver.and_then(crate::runtime::game_object::handle_key) else {
            return Ok(None);
        };
        let Some(failed) = self
            .game_handle_types(key)
            .into_iter()
            .find(|ty| self.class_load_failure(ty, 0) != LoadFailure::Loads)
        else {
            return Ok(None);
        };
        let returns = return_desc(&r.descriptor).to_owned();
        let counted = self.host.unknown_operand_call(&MethodRef {
            owner: failed,
            name: LOAD.to_owned(),
            descriptor: format!("(){returns}"),
        })?;
        Ok(Some(if returns == "V" { Value::Void } else { counted }))
    }
}
