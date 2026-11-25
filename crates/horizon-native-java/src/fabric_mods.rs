//! `FabricLoader.getAllMods` and each container's own `ModMetadata`, as Fabric
//! Loader 0.19.5 reports them.
//!
//! **The list.** `FabricLoaderImpl.getAllMods` is
//! `Collections.unmodifiableList(mods)`, and `setup` adds a container for each
//! mod `ModResolver.resolve` selected, in the resolver's order.
//! `findCompatibleSet` ends with `sort(Comparator.comparing(ModCandidateImpl::getId))`
//! (bytecode offset 841), and the dev-only shuffle never runs in production, so
//! the list is in mod-id order. The run holds:
//!
//! * every jar in it that has Fabric metadata, with its own `fabric.mod.json`:
//!   the running jar, its linked siblings and the nested (JiJ) jars of both;
//! * the loader's built-in mods: `java`, `minecraft`, `fabricloader`, and
//!   `mixinextras` (a nested jar of Fabric Loader since 0.15);
//! * the Fabric API ids the host provides (`dependencies::HOST_PROVIDED_MODS`).
//!
//! A dependency a jar declares but nothing ships is not a container, as on a
//! real loader (the host used to list those as `unknown`).
//!
//! **A jar's metadata** comes from its own document, read the way
//! `V1ModMetadataParser` reads it (checked against the 0.19.5 parser on JDK 25:
//! `tests/fixture_fabric_mods.rs`). `getType` is `fabric`; an absent or empty
//! `name` is the id; an absent `description` is `""`; `environment` `*`, absent,
//! `client` or `server` are `UNIVERSAL`/`CLIENT`/`SERVER`; `authors` and
//! `contributors` are people (a string, or `{name, contact}`); `license` is a
//! string or a list; `custom` values keep their JSON types.
//!
//! **`fabricloader` and `mixinextras`** are the containers Fabric Loader
//! 0.19.5 builds for itself and for the MixinExtras 0.5.5 jar it bundles: V1
//! metadata from those jars' own `fabric.mod.json` ([`LOADER_METADATA`],
//! [`MIXINEXTRAS_METADATA`]). Named departure: Horizon has no real Fabric
//! Loader; it reports the version whose behaviour the runtime reproduces
//! (0.19.5, the loader this runtime is ported from and checked against). A jar
//! in the run that ships its own MixinExtras keeps its own metadata, where
//! Fabric would pick the newest version.
//!
//! **The Fabric API modules the host provides** (`fabric-api`,
//! `fabric-api-base`, `fabric-command-api-v2`, `fabric-lifecycle-events-v1`,
//! `fabric-resource-loader-v1`) answer the metadata those modules carry in
//! `fabric-api-0.165.0+26.3`, the release the host's Fabric API model
//! reproduces (`events.rs`): the same named departure, Horizon reports the
//! release whose behaviour it reproduces. A jar in the run that ships its own
//! copy of one of these keeps its own metadata.
//!
//! **`java` and `minecraft`** are the loader's `BuiltinModMetadata` (see
//! `builtin_metadata`).
//!
//! **Version:** `getVersion()` is a `SemanticVersion` object for the texts
//! `fabric_version.rs` has an oracle row for, and counted for every other text (the loader
//! answers those with a `StringVersion`, which is not modelled).
//!
//! **Counted, not answered:** iterating a custom object or array
//! (`serde_json` here keeps object keys sorted, not in file order),
//! `ContactInformation.asMap` (a `HashMap` in Fabric), `getCustomValues`,
//! `getDependencies`, a custom number's `getAsNumber` (Fabric's is a
//! a parsed `Double` from `JsonReader.nextDouble()`, an icon path, and a `getAs*` of the wrong type.

use super::*;

const MOD_METADATA: &str = "net/fabricmc/loader/api/metadata/ModMetadata";
const MOD_DEPENDENCY: &str = "net/fabricmc/loader/api/metadata/ModDependency";
const MOD_DEPENDENCY_KIND: &str = "net/fabricmc/loader/api/metadata/ModDependency$Kind";
const PERSON: &str = "net/fabricmc/loader/api/metadata/Person";
const CONTACT: &str = "net/fabricmc/loader/api/metadata/ContactInformation";
const CUSTOM_VALUE: &str = "net/fabricmc/loader/api/metadata/CustomValue";
const CV_OBJECT: &str = "net/fabricmc/loader/api/metadata/CustomValue$CvObject";
const CV_ARRAY: &str = "net/fabricmc/loader/api/metadata/CustomValue$CvArray";
const MOD_ENVIRONMENT: &str = "net/fabricmc/loader/api/metadata/ModEnvironment";
const CV_TYPE: &str = "net/fabricmc/loader/api/metadata/CustomValue$CvType";

/// `ModEnvironment`'s constants, in declaration order.
const ENVIRONMENTS: &[&str] = &["CLIENT", "SERVER", "UNIVERSAL"];
/// `CustomValue.CvType`'s constants, in declaration order.
const CV_TYPES: &[&str] = &["OBJECT", "ARRAY", "STRING", "NUMBER", "BOOLEAN", "NULL"];

/// Fabric Loader's built-in mods beside the host-provided ids.
const BUILTIN_MODS: &[&str] = &["java", "minecraft", "fabricloader", "mixinextras"];

/// Fabric Loader 0.19.5's own `fabric.mod.json` (Apache-2.0), less its
/// `jars` list: the metadata its `fabricloader` container reports.
const LOADER_METADATA: &str = r#"{"schemaVersion":1,"id":"fabricloader","name":"Fabric Loader","version":"0.19.5","environment":"*","description":"The base mod loader.","contact":{"homepage":"https://fabricmc.net","irc":"ircs://irc.esper.net:6697/fabric","issues":"https://github.com/FabricMC/fabric-loader/issues","sources":"https://github.com/FabricMC/fabric-loader"},"license":"Apache-2.0","icon":"assets/fabricloader/icon.png","authors":["FabricMC"]}"#;

/// MixinExtras 0.5.5's `fabric.mod.json` (MIT), the jar Fabric Loader 0.19.5
/// bundles, less its `mixins` list.
const MIXINEXTRAS_METADATA: &str = r#"{"schemaVersion":1,"id":"mixinextras","provides":["com_github_llamalad7_mixinextras"],"version":"0.5.5","name":"MixinExtras","description":"Companion library to Mixin with lots of features to improve the compatibility and concision of your mixins!","authors":["LlamaLad7"],"contact":{"homepage":"https://github.com/LlamaLad7/MixinExtras","sources":"https://github.com/LlamaLad7/MixinExtras"},"license":"MIT","environment":"*","depends":{"fabricloader":">=0.14.25"},"custom":{"modmenu":{"badges":["library"]}}}"#;

/// The Fabric API modules the host provides, as `fabric-api-0.165.0+26.3`
/// (Apache-2.0) ships them: the umbrella jar's and its nested module jars'
/// own `fabric.mod.json`, less their `depends`, `entrypoints`, `mixins`,
/// `accessWidener` and `jars`. That is the release the host's Fabric API
/// event model reproduces (`events.rs`).
const FABRIC_API_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-api","name":"Fabric API","version":"0.165.0+26.3","environment":"*","description":"Core API module providing key hooks and intercompatibility features.","license":"Apache-2.0","icon":"assets/fabric/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"]}"#;
const FABRIC_API_BASE_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-api-base","name":"Fabric API Base","version":"2.0.6+fcdff87f5d","environment":"*","description":"Contains the essentials for Fabric API modules.","license":"Apache-2.0","icon":"assets/fabric-api-base/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"],"custom":{"fabric-api:module-lifecycle":"stable"}}"#;
const FABRIC_COMMAND_API_V2_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-command-api-v2","name":"Fabric Command API (v2)","version":"3.1.2+fcdff87f5d","environment":"*","description":"Adds command-related hooks.","license":"Apache-2.0","icon":"assets/fabric-command-api-v2/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"],"custom":{"fabric-api:module-lifecycle":"stable"}}"#;
const FABRIC_LIFECYCLE_EVENTS_V1_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-lifecycle-events-v1","name":"Fabric Lifecycle Events (v1)","version":"4.1.9+ffef5f675d","environment":"*","description":"Events for the game's lifecycle.","license":"Apache-2.0","icon":"assets/fabric-lifecycle-events-v1/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"],"custom":{"fabric-api:module-lifecycle":"stable"}}"#;
const FABRIC_RESOURCE_LOADER_V1_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-resource-loader-v1","name":"Fabric Resource Loader (v1)","version":"3.1.0+5af7744b5d","environment":"*","description":"Asset and data resource loading.","license":"Apache-2.0","icon":"assets/fabric-resource-loader-v1/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"],"custom":{"fabric-api:module-lifecycle":"stable"}}"#;

/// `java.specification.version` on the Java 25 that Minecraft 26.3 requires
/// (`ModDiscoverer.createJavaMod` strips a leading `1.`; there is none).
const JAVA_SPECIFICATION_VERSION: &str = "25";

use crate::host_fabric_loader::{self as loader, Pointed};

fn enum_marker(owner: &str, name: &str) -> Value {
    Value::Ref(Some(format!("enum/{owner}/{name}")))
}

pub(crate) fn enum_constants(owner: &str) -> Option<&'static [&'static str]> {
    match owner {
        MOD_ENVIRONMENT => Some(ENVIRONMENTS),
        CV_TYPE => Some(CV_TYPES),
        _ => None,
    }
}
pub(crate) fn enum_values(owner: &str) -> Option<Vec<Value>> {
    enum_constants(owner).map(|names| names.iter().map(|name| enum_marker(owner, name)).collect())
}

fn text(value: impl Into<String>) -> Value {
    Value::Ref(Some(value.into()))
}

/// RFC 6901 escaping of one JSON-pointer segment.
fn pointer_segment(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// `(id, pointer)` of a `<prefix><id><pointer>` marker; the pointer keeps
/// its leading `/` (a mod id never contains one).
/// The pointer to `key` under `pointer`.
fn child(pointer: &str, key: &str) -> String {
    format!("{pointer}/{}", pointer_segment(key))
}

fn cv_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Object(_) => "OBJECT",
        serde_json::Value::Array(_) => "ARRAY",
        serde_json::Value::String(_) => "STRING",
        serde_json::Value::Number(_) => "NUMBER",
        serde_json::Value::Bool(_) => "BOOLEAN",
        serde_json::Value::Null => "NULL",
    }
}

impl StdHost {
    fn nested_candidate_is_newer(&self, owner: &str, document: &serde_json::Value) -> bool {
        let incoming = document.get("version").and_then(serde_json::Value::as_str);
        let existing = self
            .fabric_metadata
            .get(owner)
            .and_then(|metadata| metadata.get("version"))
            .and_then(serde_json::Value::as_str);
        crate::nested_jars::version_cmp(incoming, existing).is_gt()
    }

    fn remove_fabric_container(&mut self, id: &str) {
        self.fabric_metadata.remove(id);
        self.fabric_aliases.retain(|_, owner| owner != id);
        self.fabric_entrypoint_containers
            .retain(|entry| entry.provider != id);
        self.fabric_origins.retain(|key, _| key != id);
        Arc::make_mut(&mut self.linked_classes).remove_fabric_data_layer(id);
    }

    /// Record a Fabric jar's metadata, refusing a duplicate id or alias.
    pub(crate) fn try_record_fabric_metadata(
        &mut self,
        jar: &NativeJar,
        nested: bool,
    ) -> Result<(), NativeError> {
        let Some(document) = jar
            .resource_bytes
            .get("fabric.mod.json")
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        else {
            return Ok(());
        };
        if let Some(id) = document
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
        {
            let mut replace_nested = Vec::new();
            if let Some(owner) = self
                .fabric_aliases
                .get(&id)
                .cloned()
                .or_else(|| self.fabric_metadata.contains_key(&id).then(|| id.clone()))
            {
                let is_nested = self
                    .fabric_origins
                    .get(&owner)
                    .is_some_and(|(kind, _)| kind == "nested");
                if is_nested {
                    if nested && !self.nested_candidate_is_newer(&owner, &document) {
                        return Ok(());
                    }
                    replace_nested.push(owner);
                } else {
                    return Err(NativeError::Metadata(format!("duplicate mod {id}")));
                }
            }
            let aliases = document
                .get("provides")
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut seen_keys = std::collections::BTreeSet::from([id.clone()]);
            for alias in &aliases {
                if alias == &id || !seen_keys.insert(alias.clone()) {
                    return Err(NativeError::Metadata(format!("duplicate mod {alias}")));
                }
                if let Some(owner) = self.fabric_aliases.get(alias).cloned().or_else(|| {
                    self.fabric_metadata
                        .contains_key(alias)
                        .then(|| alias.clone())
                }) {
                    let is_nested = self
                        .fabric_origins
                        .get(&owner)
                        .is_some_and(|(kind, _)| kind == "nested");
                    if is_nested {
                        if nested && !self.nested_candidate_is_newer(&owner, &document) {
                            return Ok(());
                        }
                        replace_nested.push(owner);
                    } else {
                        return Err(NativeError::Metadata(format!("duplicate mod {alias}")));
                    }
                }
            }
            replace_nested.sort();
            replace_nested.dedup();
            for owner in replace_nested {
                self.remove_fabric_container(&owner);
            }
            self.fabric_metadata.insert(id.clone(), document.clone());
            // Fabric's mod map also indexes every provides alias to the same
            // selected container, so isModLoaded/getModContainer answer aliases.
            for alias in aliases {
                self.fabric_aliases.insert(alias, id.clone());
            }
            // Linked and nested Fabric jars contribute their declared
            // entrypoints to the same loader storage. Keep declaration order
            // within a provider; the host sorts providers by mod id below.
            for (kind, entries) in &jar.fabric_entrypoints {
                for entry in entries {
                    let row = FabricEntrypointContainer {
                        kind: kind.clone(),
                        class: entry.class.clone(),
                        provider: id.clone(),
                    };
                    // EntrypointStorage.add appends a fresh Entry for every
                    // declaration. Fabric does not deduplicate identical
                    // kind/class/provider rows.
                    self.fabric_entrypoint_containers.push(row);
                }
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn record_fabric_metadata(&mut self, jar: &NativeJar) {
        if let Err(error) = self.try_record_fabric_metadata(jar, false) {
            let message = match error {
                NativeError::Metadata(message) => message,
                other => other.to_string(),
            };
            self.fabric_resolution_errors.push(message);
        }
    }

    /// A jar container's metadata document: one recorded for the run, or the
    /// host's own `fabric.mod.json` resource when it names `id`.
    fn fabric_document(&self, id: &str) -> Option<std::borrow::Cow<'_, serde_json::Value>> {
        if let Some(document) = self.fabric_metadata.get(id) {
            return Some(std::borrow::Cow::Borrowed(document));
        }
        if let Some(owner) = self.fabric_aliases.get(id)
            && let Some(document) = self.fabric_metadata.get(owner)
        {
            return Some(std::borrow::Cow::Borrowed(document));
        }
        let own = self
            .resource_bytes
            .get("fabric.mod.json")
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
            .filter(|document| document.get("id").and_then(serde_json::Value::as_str) == Some(id));
        if let Some(document) = own {
            return Some(std::borrow::Cow::Owned(document));
        }
        let loader = match id {
            "fabricloader" => LOADER_METADATA,
            "mixinextras" => MIXINEXTRAS_METADATA,
            "fabric-api" => FABRIC_API_METADATA,
            "fabric-api-base" => FABRIC_API_BASE_METADATA,
            "fabric-command-api-v2" => FABRIC_COMMAND_API_V2_METADATA,
            "fabric-lifecycle-events-v1" => FABRIC_LIFECYCLE_EVENTS_V1_METADATA,
            "fabric-resource-loader-v1" => FABRIC_RESOURCE_LOADER_V1_METADATA,
            _ => return None,
        };
        serde_json::from_str(loader)
            .ok()
            .map(std::borrow::Cow::Owned)
    }

    /// Whether `id` is a container the loader builds without a jar.
    fn builtin_fabric_mod(id: &str) -> bool {
        BUILTIN_MODS.contains(&id)
            || (dependencies::HOST_PROVIDED_MODS.contains(&id)
                && !matches!(id, "neoforge" | "forge"))
    }

    /// Every container `getAllMods` lists, in mod-id order.
    fn fabric_containers(&self) -> Vec<Value> {
        let mut ids = self
            .fabric_metadata
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        ids.extend(
            BUILTIN_MODS
                .iter()
                .chain(dependencies::HOST_PROVIDED_MODS)
                .filter(|id| Self::builtin_fabric_mod(id))
                .map(|id| (*id).to_owned()),
        );
        ids.into_iter()
            .map(|id| self.fabric_container(&id))
            .collect()
    }

    /// The version mod `id`'s metadata answers: its `fabric.mod.json`'s, a
    /// built-in's (`java`: Java 25's; `minecraft`: the seeded game version),
    /// or the run's mod list's; `None` when nothing says.
    fn fabric_version(&self, id: &str) -> Option<String> {
        if let Some(version) = self.fabric_document(id).and_then(|document| {
            document
                .get("version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        }) {
            return Some(version);
        }
        match id {
            "java" => Some(JAVA_SPECIFICATION_VERSION.to_owned()),
            "minecraft" => self.game_version.clone(),
            _ => self.mods.get(id).cloned(),
        }
    }

    /// Whether `getModContainer(id)` and `isModLoaded(id)` find mod `id`: Fabric
    /// Loader answers both from `modMap`, the containers `getAllMods` lists,
    /// so every id [`Self::fabric_containers`] lists is present. The host
    /// also counts the run's mods (and the dependencies a running jar
    /// requires, which a real loader would have had to find) and the loaders
    /// it always reports as loaded.
    pub(crate) fn fabric_mod_present(&self, id: &str) -> bool {
        if self.fabric_resolution_errors.iter().any(|error| {
            error == &format!("duplicate mod {id}")
                || error.contains(&format!("duplicate mod {id}"))
        }) {
            return false;
        }
        self.mods.contains_key(id)
            || is_always_loaded_mod(id)
            || self.fabric_metadata.contains_key(id)
            || self.fabric_aliases.contains_key(id)
            || Self::builtin_fabric_mod(id)
    }

    /// The one `ModContainer` for mod `id` (`host_fabric_loader.rs`): every
    /// call that hands one out builds it here, so its key is the same.
    pub(crate) fn fabric_container(&self, id: &str) -> Value {
        let owner = self
            .fabric_aliases
            .get(id)
            .map(String::as_str)
            .unwrap_or(id);
        loader::container(owner, self.fabric_version(owner).as_deref())
    }

    /// The origin that Fabric Loader would attach to this run's container.
    pub(crate) fn fabric_origin(&self, id: &str) -> Value {
        let (kind, argument) = self
            .fabric_origins
            .get(id)
            .cloned()
            .unwrap_or_else(|| ("path".into(), id.to_owned()));
        loader::origin(&kind, &argument)
    }

    /// Answer the loader's container list and metadata surface, or `None`
    /// for a call this module does not own.
    pub(crate) fn fabric_mods_instance(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("net/fabricmc/loader/api/FabricLoader", "getAllMods", "()Ljava/util/Collection;") => {
                let containers = self.fabric_containers();
                // Fabric calls Collections.unmodifiableList(mods) inside
                // getAllMods, so each query makes a fresh wrapper over the
                // same loader-owned list. The private backing list is not
                // exposed to the jar; the view exists only to preserve the
                // JDK wrapper class and identity contract.
                let source = self.fresh_list(containers);
                Some(Ok(self
                    .unmodifiable_view("list", &source)
                    .expect("fresh list is a host collection")))
            }
            (MOD_DEPENDENCY, ..) => {
                let (mod_id, kind) = loader::dependency_parts(receiver)?;
                Some(match (r.name.as_str(), r.descriptor.as_str()) {
                    ("getModId", "()Ljava/lang/String;") => Ok(text(mod_id)),
                    ("getKind", "()Lnet/fabricmc/loader/api/metadata/ModDependency$Kind;") => Ok(
                        Value::Ref(Some(format!("enum/{MOD_DEPENDENCY_KIND}/{kind}"))),
                    ),
                    _ => self.unknown_host_call(r),
                })
            }
            (MOD_DEPENDENCY_KIND, ..) => {
                let name = expect_ref(receiver)?.rsplit('/').next()?.to_owned();
                Some(match (r.name.as_str(), r.descriptor.as_str()) {
                    ("name", "()Ljava/lang/String;") | ("toString", "()Ljava/lang/String;") => {
                        Ok(text(name))
                    }
                    ("ordinal", "()I") => Ok(Value::Int(match name.as_str() {
                        "DEPENDS" => 0,
                        "RECOMMENDS" => 1,
                        "SUGGESTS" => 2,
                        "CONFLICTS" => 3,
                        "BREAKS" => 4,
                        _ => 5,
                    })),
                    _ => self.unknown_host_call(r),
                })
            }
            (MOD_METADATA, ..) => {
                let id = loader::metadata_id(receiver)?;
                if let Some(document) = self.fabric_document(&id).map(std::borrow::Cow::into_owned)
                {
                    Some(self.jar_metadata(r, receiver, &id, &document, args))
                } else if Self::builtin_fabric_mod(&id) {
                    Some(self.builtin_metadata(r, receiver, &id))
                } else {
                    None
                }
            }
            (PERSON, ..) => {
                let (id, pointer) = loader::pointed_parts(receiver, Pointed::Person)?;
                let person = self.fabric_document(&id)?.pointer(&pointer)?.clone();
                Some(match r.name.as_str() {
                    "getName" => Ok(text(match &person {
                        serde_json::Value::Object(fields) => fields
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        other => other.as_str().unwrap_or_default().to_owned(),
                    })),
                    // `SimplePerson` (a string) and an object without
                    // `contact` answer `ContactInformation.EMPTY`.
                    "getContact" => Ok(if person.get("contact").is_some() {
                        loader::pointed(Pointed::Contact, &id, &child(&pointer, "contact"))
                    } else {
                        loader::empty_contact()
                    }),
                    _ => self.unknown_host_call(r),
                })
            }
            (CONTACT, ..) => {
                // `ContactInformation.EMPTY` has no entries.
                let contact = if loader::is_empty_contact(receiver) {
                    None
                } else {
                    let (id, pointer) = loader::pointed_parts(receiver, Pointed::Contact)?;
                    self.fabric_document(&id)
                        .and_then(|document| document.pointer(&pointer).cloned())
                };
                Some(match (r.name.as_str(), r.descriptor.as_str()) {
                    ("get", "(Ljava/lang/String;)Ljava/util/Optional;") => {
                        let key = expect_string(Self::argument(r, args, 0).ok()?);
                        Ok(contact
                            .as_ref()
                            .and_then(|contact| contact.get(key))
                            .and_then(serde_json::Value::as_str)
                            .map_or_else(empty_optional, |value| {
                                crate::optional_wrap(text(value.to_owned()))
                            }))
                    }
                    _ => self.unknown_host_call(r),
                })
            }
            (CUSTOM_VALUE | CV_OBJECT | CV_ARRAY, ..) => {
                // A shared `true`/`false`/`null` constant: no object or array
                // below it, so no pointer is needed.
                if let Some(value) = loader::constant_custom_value(receiver) {
                    return Some(self.custom_value_call(r, "", "", &value, args));
                }
                let (id, pointer) = loader::pointed_parts(receiver, Pointed::CustomValue)?;
                let value = self.fabric_document(&id)?.pointer(&pointer)?.clone();
                Some(self.custom_value_call(r, &id, &pointer, &value, args))
            }
            (MOD_ENVIRONMENT | CV_TYPE, "ordinal" | "name" | "toString", _) => {
                let constants = if r.owner == MOD_ENVIRONMENT {
                    ENVIRONMENTS
                } else {
                    CV_TYPES
                };
                let name = expect_ref(receiver)?
                    .strip_prefix("enum/")?
                    .strip_prefix(r.owner.as_str())?
                    .strip_prefix('/')?;
                let ordinal = constants.iter().position(|constant| *constant == name)?;
                Some(Ok(match r.name.as_str() {
                    "ordinal" => Value::Int(ordinal as i32),
                    _ => text(name),
                }))
            }
            (MOD_ENVIRONMENT, "matches", "(Lnet/fabricmc/api/EnvType;)Z") => {
                let name = expect_ref(receiver)?
                    .strip_prefix("enum/")?
                    .strip_prefix(MOD_ENVIRONMENT)?
                    .strip_prefix('/')?;
                let side = Self::argument(r, args, 0)
                    .ok()
                    .and_then(crate::host_fabric_runtime::env_type_name)
                    .unwrap_or_default();
                Some(Ok(Value::Int(i32::from(
                    name == "UNIVERSAL" || name == side,
                ))))
            }
            _ => None,
        }
    }

    /// `ModEnvironment` and `CvType` constants, the markers the answers above
    /// hand out, so `==` against them holds.
    pub(crate) fn fabric_mods_static(r: &MethodRef) -> Option<Value> {
        if r.owner == CONTACT && r.name == "EMPTY" {
            return Some(loader::empty_contact());
        }
        let constants = match r.owner.as_str() {
            MOD_ENVIRONMENT => ENVIRONMENTS,
            CV_TYPE => CV_TYPES,
            _ => return None,
        };
        constants
            .contains(&r.name.as_str())
            .then(|| enum_marker(&r.owner, &r.name))
    }

    pub(crate) fn counted_metadata_collection(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
    ) -> Result<Value, NativeError> {
        let class = if r.descriptor.contains("Ljava/util/Map;") {
            "map"
        } else {
            "list"
        };
        let cache = cache_identity(receiver).map(|identity| (identity, r.name.clone()));
        if let Some(cache) = cache.as_ref()
            && let Some(marker) = self.cached_collections.get(cache).cloned()
            && self.mutable_collections.contains_key(&marker)
            && self
                .persistent_static_fields
                .values()
                .any(|value| expect_ref(value) == Some(&marker))
        {
            // The neutral value remains fabricated, so every observation stays
            // counted in both normal and strict modes. The cache only preserves
            // the identity a jar already retained; it never suppresses the
            // unknown-host accounting.
            self.unknown_host_call(r)?;
            return Ok(crate::jar_object::collection(marker));
        }
        self.unknown_host_call(r)?;
        let made = self.empty_collection(class);
        if let Some(cache) = cache
            && let Some(marker) = expect_ref(&made)
        {
            self.cached_collections.insert(cache, marker.clone());
        }
        Ok(made)
    }

    fn jar_metadata(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        id: &str,
        document: &serde_json::Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let field = |name: &str| document.get(name);
        let strings = |name: &str| -> Vec<Value> {
            match field(name) {
                Some(serde_json::Value::String(one)) => vec![text(one.clone())],
                Some(serde_json::Value::Array(many)) => many
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(text)
                    .collect(),
                _ => Vec::new(),
            }
        };
        let schema = field("schemaVersion")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        if schema != 1
            && matches!(
                (r.name.as_str(), r.descriptor.as_str()),
                (
                    "getAuthors"
                        | "getContributors"
                        | "getLicense"
                        | "getProvides"
                        | "getDependencies",
                    "()Ljava/util/Collection;"
                )
            )
        {
            return self.counted_metadata_collection(r, receiver);
        }
        let dependencies = || -> Vec<Value> {
            [
                ("depends", "DEPENDS"),
                ("recommends", "RECOMMENDS"),
                ("suggests", "SUGGESTS"),
                ("conflicts", "CONFLICTS"),
                ("breaks", "BREAKS"),
            ]
            .into_iter()
            .flat_map(|(field_name, kind)| {
                field(field_name)
                    .and_then(serde_json::Value::as_object)
                    .into_iter()
                    .flat_map(move |entries| {
                        entries.keys().map(move |id| loader::dependency(id, kind))
                    })
            })
            .collect()
        };
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("getId", "()Ljava/lang/String;") => Ok(text(id)),
            ("getType", "()Ljava/lang/String;") => Ok(text("fabric")),
            ("getVersion", "()Lnet/fabricmc/loader/api/Version;") => {
                match field("version").and_then(serde_json::Value::as_str) {
                    None => Ok(Value::Ref(None)),
                    Some(version) => match crate::fabric_version::version_value(version) {
                        Some(value) => Ok(value),
                        None => self.unknown_host_call(r),
                    },
                }
            }
            ("getName", "()Ljava/lang/String;") => Ok(text(
                field("name")
                    .and_then(serde_json::Value::as_str)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(id),
            )),
            ("getDescription", "()Ljava/lang/String;") => Ok(text(
                field("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
            )),
            ("getEnvironment", "()Lnet/fabricmc/loader/api/metadata/ModEnvironment;") => {
                let name = match field("environment").and_then(serde_json::Value::as_str) {
                    Some("client") => "CLIENT",
                    Some("server") => "SERVER",
                    _ => "UNIVERSAL",
                };
                Ok(enum_marker(MOD_ENVIRONMENT, name))
            }
            ("getAuthors" | "getContributors", "()Ljava/util/Collection;") => {
                let key = if r.name == "getAuthors" {
                    "authors"
                } else {
                    "contributors"
                };
                let count = field(key)
                    .and_then(serde_json::Value::as_array)
                    .map_or(0, Vec::len);
                let people = (0..count)
                    .map(|index| loader::pointed(Pointed::Person, id, &format!("/{key}/{index}")))
                    .collect::<Vec<_>>();
                Ok(self.cached_unmodifiable_collection(
                    receiver,
                    &r.name,
                    "java/util/Collections$UnmodifiableCollection",
                    people,
                ))
            }
            // `V1ModMetadata` holds `ContactInformation.EMPTY` without one.
            ("getContact", "()Lnet/fabricmc/loader/api/metadata/ContactInformation;") => {
                Ok(if field("contact").is_some() {
                    loader::pointed(Pointed::Contact, id, "/contact")
                } else {
                    loader::empty_contact()
                })
            }
            ("getLicense", "()Ljava/util/Collection;") => {
                let values = strings("license");
                {
                    Ok(self.cached_unmodifiable_collection(
                        receiver,
                        "getLicense",
                        "java/util/Collections$UnmodifiableCollection",
                        values,
                    ))
                }
            }
            ("getProvides", "()Ljava/util/Collection;") => Ok(self.cached_unmodifiable_collection(
                receiver,
                "getProvides",
                "java/util/Collections$UnmodifiableCollection",
                strings("provides"),
            )),
            ("getDependencies", "()Ljava/util/Collection;") => Ok(self
                .cached_unmodifiable_collection(
                    receiver,
                    "getDependencies",
                    "java/util/Collections$UnmodifiableCollection",
                    dependencies(),
                )),
            ("containsCustomValue", "(Ljava/lang/String;)Z") => {
                let key = expect_string(Self::argument(r, args, 0)?);
                let present = field("custom").is_some_and(|custom| custom.get(key).is_some());
                Ok(Value::Int(i32::from(present)))
            }
            (
                "getCustomValue",
                "(Ljava/lang/String;)Lnet/fabricmc/loader/api/metadata/CustomValue;",
            ) => {
                let key = expect_string(Self::argument(r, args, 0)?);
                Ok(field("custom")
                    .and_then(|custom| custom.get(key))
                    .map_or(Value::Ref(None), |value| {
                        loader::custom_value(id, &child("/custom", key), value)
                    }))
            }
            ("getIconPath", "(I)Ljava/util/Optional;") if field("icon").is_none() => {
                Ok(empty_optional())
            }
            _ => self.unknown_host_call(r),
        }
    }

    /// `java` and `minecraft` are `BuiltinModMetadata` built by the loader
    /// itself (`ModDiscoverer.createJavaMod`, `MinecraftGameProvider
    /// .getBuiltinMods`): the builder's defaults (`description` `""`,
    /// `UNIVERSAL`, no people, no license, `ContactInformation.EMPTY`, no
    /// custom values, type `builtin`). `java`'s version is Java 25's
    /// `java.specification.version`; its name is `java.vm.name`, which
    /// depends on the JDK's vendor, so it is counted. `minecraft`'s name is
    /// `Minecraft` and its version the game version a caller seeded (counted
    /// without one).
    fn builtin_metadata(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        id: &str,
    ) -> Result<Value, NativeError> {
        if r.name == "getId" && r.descriptor == "()Ljava/lang/String;" {
            return Ok(text(id));
        }
        if !matches!(id, "java" | "minecraft") {
            return self.unknown_host_call(r);
        }
        let answer = match (r.name.as_str(), r.descriptor.as_str()) {
            ("getType", "()Ljava/lang/String;") => Some(text("builtin")),
            ("getVersion", "()Lnet/fabricmc/loader/api/Version;") => {
                if id == "java" {
                    crate::fabric_version::version_value(JAVA_SPECIFICATION_VERSION)
                } else {
                    self.game_version
                        .as_deref()
                        .and_then(crate::fabric_version::version_value)
                }
            }
            ("getName", "()Ljava/lang/String;") if id == "minecraft" => Some(text("Minecraft")),
            ("getDescription", "()Ljava/lang/String;") => Some(text("")),
            ("getEnvironment", "()Lnet/fabricmc/loader/api/metadata/ModEnvironment;") => {
                Some(enum_marker(MOD_ENVIRONMENT, "UNIVERSAL"))
            }
            (
                "getAuthors" | "getContributors" | "getLicense" | "getProvides" | "getDependencies",
                "()Ljava/util/Collection;",
            ) => return self.counted_metadata_collection(r, receiver),
            ("getContact", "()Lnet/fabricmc/loader/api/metadata/ContactInformation;") => {
                Some(loader::empty_contact())
            }
            ("containsCustomValue", "(Ljava/lang/String;)Z") => Some(Value::Int(0)),
            (
                "getCustomValue",
                "(Ljava/lang/String;)Lnet/fabricmc/loader/api/metadata/CustomValue;",
            ) => Some(Value::Ref(None)),
            _ => None,
        };
        match answer {
            Some(value) => Ok(value),
            None => self.unknown_host_call(r),
        }
    }

    fn custom_value_call(
        &mut self,
        r: &MethodRef,
        id: &str,
        pointer: &str,
        value: &serde_json::Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let wanted = match r.name.as_str() {
            "getAsObject" => Some("OBJECT"),
            "getAsArray" => Some("ARRAY"),
            "getAsString" => Some("STRING"),
            "getAsBoolean" => Some("BOOLEAN"),
            _ => None,
        };
        if wanted.is_some_and(|wanted| wanted != cv_type(value)) {
            return self.unknown_host_call(r);
        }
        let marker = |pointer: String| loader::pointed(Pointed::CustomValue, id, &pointer);
        if r.name == "iterator" && r.descriptor == "()Ljava/util/Iterator;" {
            if let serde_json::Value::Array(items) = value {
                let values = items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| loader::custom_value(id, &child(pointer, &i.to_string()), v))
                    .collect();
                return match self
                    .new_host_iterator(values, Some("fabric/custom-value/array".into()))
                {
                    Some(iterator) => Ok(iterator),
                    None => self.unknown_host_call(r),
                };
            }
            if let serde_json::Value::Object(entries) = value {
                let mut pairs = Vec::with_capacity(entries.len() * 2);
                for (key, child_value) in entries {
                    pairs.push(text(key.clone()));
                    pairs.push(loader::custom_value(id, &child(pointer, key), child_value));
                }
                let entries = self.mint_map_entries(
                    &Value::Ref(None),
                    &pairs,
                    crate::host_map_entry::EntryMade::Holder,
                );
                return match self
                    .new_host_iterator(entries, Some("fabric/custom-value/object".into()))
                {
                    Some(iterator) => Ok(iterator),
                    None => self.unknown_host_call(r),
                };
            }
        }
        match (r.name.as_str(), r.descriptor.as_str(), value) {
            ("getType", _, _) => Ok(enum_marker(CV_TYPE, cv_type(value))),
            ("getAsObject" | "getAsArray", _, _) => Ok(marker(pointer.to_owned())),
            ("getAsString", _, serde_json::Value::String(string)) => Ok(text(string.clone())),
            ("getAsBoolean", _, serde_json::Value::Bool(flag)) => Ok(Value::Int(i32::from(*flag))),
            ("getAsNumber", _, serde_json::Value::Number(number)) => {
                match number.to_string().parse::<f64>() {
                    Ok(value) => Ok(Value::Float(value)),
                    Err(_) => self.unknown_host_call(r),
                }
            }
            ("size", "()I", serde_json::Value::Object(entries)) => {
                Ok(Value::Int(entries.len() as i32))
            }
            ("size", "()I", serde_json::Value::Array(items)) => Ok(Value::Int(items.len() as i32)),
            ("containsKey", "(Ljava/lang/String;)Z", serde_json::Value::Object(entries)) => {
                let key = expect_string(Self::argument(r, args, 0)?);
                Ok(Value::Int(i32::from(entries.contains_key(key))))
            }
            (
                "get",
                "(Ljava/lang/String;)Lnet/fabricmc/loader/api/metadata/CustomValue;",
                serde_json::Value::Object(entries),
            ) => {
                let key = expect_string(Self::argument(r, args, 0)?);
                Ok(entries.get(key).map_or(Value::Ref(None), |value| {
                    loader::custom_value(id, &child(pointer, key), value)
                }))
            }
            (
                "get",
                "(I)Lnet/fabricmc/loader/api/metadata/CustomValue;",
                serde_json::Value::Array(items),
            ) => {
                let index = expect_int(r, Self::argument(r, args, 0)?)?;
                match usize::try_from(index)
                    .ok()
                    .filter(|index| *index < items.len())
                {
                    Some(index) => Ok(loader::custom_value(
                        id,
                        &child(pointer, &index.to_string()),
                        &items[index],
                    )),
                    // `ArrayList.get` out of range.
                    None => Err(NativeError::UncaughtException {
                        class: "java/lang/IndexOutOfBoundsException".into(),
                        message: Some(format!(
                            "Index {index} out of bounds for length {}",
                            items.len()
                        )),
                        frame: None,
                    }),
                }
            }
            _ => self.unknown_host_call(r),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fabric(id: &str, provides: &[&str]) -> NativeJar {
        fabric_version(id, "1", provides)
    }

    fn fabric_version(id: &str, version: &str, provides: &[&str]) -> NativeJar {
        let aliases = provides
            .iter()
            .map(|alias| format!(r#""{alias}""#))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            r#"{{"schemaVersion":1,"id":"{id}","version":"{version}","provides":[{aliases}]}}"#
        );
        NativeJar {
            resource_bytes: BTreeMap::from([(String::from("fabric.mod.json"), json.into_bytes())]),
            ..NativeJar::default()
        }
    }

    #[test]
    fn alias_container_is_the_provider_container() {
        let mut host = StdHost::new();
        host.record_fabric_metadata(&fabric("provider", &["alias"]));
        assert_eq!(
            host.fabric_container("alias"),
            host.fabric_container("provider")
        );
    }

    #[test]
    fn alias_collision_is_a_resolution_error_without_first_wins() {
        let mut host = StdHost::new();
        host.record_fabric_metadata(&fabric("one", &["alias"]));
        host.record_fabric_metadata(&fabric("two", &["alias"]));
        assert!(!host.fabric_mod_present("alias"));
        assert!(
            host.fabric_resolution_errors
                .iter()
                .any(|error| error == "duplicate mod alias")
        );
    }

    #[test]
    fn linked_duplicate_id_is_a_loader_refusal() {
        let one = fabric("one", &[]);
        let two = fabric("one", &[]);
        let mut host = StdHost::new();
        host.link_jar("one", &one).unwrap();
        let error = host.link_jar("two", &two).unwrap_err();
        assert!(error.to_string().contains("duplicate mod one"));
    }

    #[test]
    fn a_root_replaces_a_nested_candidate_with_the_same_id() {
        let mut host = StdHost::new();
        host.fabric_origins.insert(
            "module".into(),
            ("nested".into(), "carrier|module.jar".into()),
        );
        host.record_fabric_metadata(&fabric("module", &["nested-alias"]));
        host.record_fabric_metadata(&fabric("module", &[]));
        assert!(host.fabric_resolution_errors.is_empty());
        assert!(host.fabric_aliases.is_empty());
        assert!(host.fabric_metadata.contains_key("module"));
    }

    #[test]
    fn an_older_nested_candidate_is_merged_into_the_newer_id() {
        let mut host = StdHost::new();
        host.fabric_origins.insert(
            "module".into(),
            ("nested".into(), "carrier|module.jar".into()),
        );
        host.try_record_fabric_metadata(&fabric_version("module", "2", &[]), true)
            .unwrap();
        host.try_record_fabric_metadata(&fabric_version("module", "1", &[]), true)
            .unwrap();
        assert_eq!(host.fabric_version("module").as_deref(), Some("2"));
    }
}
