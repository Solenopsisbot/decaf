//! Client registration-only APIs.
//!
//! These calls populate registries that the real client consumes later. The
//! native probe records their order and identities, but never invokes a
//! renderer, model, screen, particle provider, or other game-state callback.

use std::collections::{BTreeMap, BTreeSet};

use crate::runtime::host_ref::{HostKind, HostRef};
use crate::runtime::{MethodRef, NativeError, Value};

const PARTICLE_REGISTRY: &str =
    "net/fabricmc/fabric/api/client/particle/v1/ParticleProviderRegistry";
const PARTICLE_TYPES: &str = "net/fabricmc/fabric/api/particle/v1/FabricParticleTypes";
const DEBUG_ENTRIES: &str = "net/minecraft/client/gui/components/debug/DebugScreenEntries";
const MENU_SCREENS: &str = "net/minecraft/client/gui/screens/MenuScreens";
const MODEL_LAYERS: &str = "net/fabricmc/fabric/api/client/rendering/v1/ModelLayerRegistry";
const ENTITY_RENDERERS: &str = "net/fabricmc/fabric/api/client/rendering/v1/EntityRendererRegistry";
const MODEL_LOADING: &str = "net/fabricmc/fabric/api/client/model/loading/v1/ModelLoadingPlugin";
const PREPARABLE_MODEL_LOADING: &str =
    "net/fabricmc/fabric/api/client/model/loading/v1/PreparableModelLoadingPlugin";
const MODEL_LOCATION: &str = "net/minecraft/client/model/geom/ModelLayerLocation";
const VANILLA_HUD: &str = "net/fabricmc/fabric/api/client/rendering/v1/hud/VanillaHudElements";
const DEBUG_PROFILE: &str = "net/minecraft/client/gui/components/debug/DebugScreenProfile";
const DEBUG_STATUS: &str = "net/minecraft/client/gui/components/debug/DebugScreenEntryStatus";
const MAP_CODEC: &str = "com/mojang/serialization/MapCodec";
const STREAM_CODEC: &str = "net/minecraft/network/codec/StreamCodec";

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Registration {
    DebugEntry {
        id: String,
        entry: Value,
    },
    ParticleProvider {
        particle_type: Value,
        provider: Value,
    },
    MenuScreen {
        menu: Value,
        constructor: Value,
    },
    EntityRenderer {
        entity_type: Value,
        provider: Value,
    },
    ModelLayer {
        location: Value,
        provider: Value,
    },
    ModelLoadingPlugin {
        plugin: Value,
    },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    pub(crate) registrations: Vec<Registration>,
    next_particle_type: u64,
    particle_codecs: BTreeMap<String, (Value, Value)>,
    model_layers: BTreeSet<String>,
    armor_layers: BTreeSet<String>,
    model_locations: BTreeMap<String, (String, String)>,
    menu_screens: BTreeSet<String>,
    static_values: BTreeMap<String, Value>,
}

fn client_handle(text: impl Into<String>) -> Value {
    crate::runtime::game_object::client_value(format!("client/{}", text.into()))
}

fn value_key(value: &Value) -> String {
    crate::runtime::expect_ref(value)
        .cloned()
        .unwrap_or_else(|| format!("{value:?}"))
}

fn identifier(value: &Value) -> Option<String> {
    crate::runtime::host_ids::identifier_key(value).map(str::to_owned)
}

fn debug_profiles_map(name: &str) -> Value {
    Value::Host(HostRef::new(HostKind::DebugProfilesMap, name))
}

fn debug_profile(value: &Value) -> Option<&str> {
    crate::runtime::expect_ref(value)?
        .strip_prefix("enum/net/minecraft/client/gui/components/debug/DebugScreenProfile/")
}

fn debug_status(status: &str) -> Value {
    Value::Ref(Some(format!("enum/{DEBUG_STATUS}/{status}")))
}

const DEFAULT_PROFILE_IDS: &[&str] = &[
    "three_dimensional_crosshair",
    "game_version",
    "tps",
    "fps",
    "memory",
    "system_specs",
    "player_position",
    "player_section_position",
    "simple_performance_impactors",
];
const PERFORMANCE_PROFILE_IDS: &[&str] = &[
    "tps",
    "fps",
    "gpu_utilization",
    "memory",
    "simple_performance_impactors",
];

fn profile_ids(profile: &str) -> Option<&'static [&'static str]> {
    match profile {
        "DEFAULT" => Some(DEFAULT_PROFILE_IDS),
        "PERFORMANCE" => Some(PERFORMANCE_PROFILE_IDS),
        _ => None,
    }
}

fn profile_status(profile: &str, id: &str) -> Option<Value> {
    profile_ids(profile)?.contains(&id).then(|| {
        let status = if profile == "PERFORMANCE" && id == "fps" {
            "ALWAYS_ON"
        } else {
            "IN_OVERLAY"
        };
        debug_status(status)
    })
}

fn profile_map_call(
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let Value::Host(handle) = receiver else {
        return None;
    };
    if handle.kind() != HostKind::DebugProfilesMap || r.owner != "java/util/Map" {
        return None;
    }
    let profile = handle.key().strip_prefix("profile/");
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("get", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
            let key = args.first()?;
            if let Some(profile) = profile {
                let id = identifier(key)?;
                let path = id.strip_prefix("minecraft:").unwrap_or(&id);
                Some(Ok(profile_status(profile, path).unwrap_or(Value::Ref(None))))
            } else {
                Some(Ok(debug_profile(args.first()?)
                    .map_or(Value::Ref(None), |name| {
                        debug_profiles_map(&format!("profile/{name}"))
                    })))
            }
        }
        ("containsKey", "(Ljava/lang/Object;)Z") => {
            let key = args.first()?;
            let found = if let Some(profile) = profile {
                identifier(key)
                    .and_then(|id| {
                        profile_status(profile, id.strip_prefix("minecraft:").unwrap_or(&id))
                    })
                    .is_some()
            } else {
                debug_profile(key).is_some_and(|name| profile_ids(name).is_some())
            };
            Some(Ok(Value::Int(i32::from(found))))
        }
        ("size", "()I") => {
            Some(Ok(Value::Int(profile.map_or(2, |name| {
                profile_ids(name).map_or(0, <[&str]>::len) as i32
            }))))
        }
        ("isEmpty", "()Z") => Some(Ok(Value::Int(0))),
        ("getOrDefault", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
            let key = args.first()?;
            let value = if let Some(profile) = profile {
                identifier(key)
                    .and_then(|id| {
                        profile_status(profile, id.strip_prefix("minecraft:").unwrap_or(&id))
                    })
                    .unwrap_or_else(|| args.get(1).cloned().unwrap_or(Value::Ref(None)))
            } else {
                debug_profile(key)
                    .filter(|name| profile_ids(name).is_some())
                    .map_or_else(
                        || args.get(1).cloned().unwrap_or(Value::Ref(None)),
                        |name| debug_profiles_map(&format!("profile/{name}")),
                    )
            };
            Some(Ok(value))
        }
        _ => None,
    }
}

fn illegal_state(message: impl Into<String>) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalStateException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn illegal_argument(message: impl Into<String>) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

const DEBUG_IDS: &[&str] = &[
    "game_version",
    "fps",
    "tps",
    "memory",
    "detailed_memory",
    "system_specs",
    "looking_at_block_state",
    "looking_at_block_tags",
    "looking_at_fluid_state",
    "looking_at_fluid_tags",
    "looking_at_entity",
    "looking_at_entity_tags",
    "chunk_render_stats",
    "chunk_generation_stats",
    "entity_render_stats",
    "particle_render_stats",
    "chunk_source_stats",
    "player_position",
    "player_section_position",
    "player_speed",
    "light_levels",
    "heightmap",
    "biome",
    "local_difficulty",
    "day_count",
    "entity_spawn_counts",
    "sound_mood",
    "sound_cache",
    "post_effects",
    "entity_hitboxes",
    "chunk_borders",
    "three_dimensional_crosshair",
    "chunk_section_paths",
    "gpu_utilization",
    "simple_performance_impactors",
    "chunk_section_octree",
    "visualize_water_levels",
    "visualize_heightmap",
    "visualize_collision_boxes",
    "visualize_entity_supporting_blocks",
    "visualize_block_light_levels",
    "visualize_sky_light_levels",
    "visualize_solid_faces",
    "visualize_chunks_on_server",
    "visualize_sky_light_sections",
    "chunk_section_visibility",
];

const HUD_IDS: &[&str] = &[
    "misc_overlays",
    "crosshair",
    "spectator_menu",
    "hotbar",
    "armor_bar",
    "health_bar",
    "food_bar",
    "air_bar",
    "mount_health",
    "info_bar",
    "experience_level",
    "info_bar_foreground",
    "held_item_tooltip",
    "spectator_tooltip",
    "mob_effects",
    "boss_bar",
    "sleep",
    "demo_timer",
    "scoreboard",
    "overlay_message",
    "title_and_subtitle",
    "chat",
    "player_list",
    "subtitles",
];

/// Static fields whose values are canonical identifiers or enum constants.
pub(crate) fn static_field(state: &mut State, r: &MethodRef) -> Option<Value> {
    let cache_key = format!("{}#{}:{}", r.owner, r.name, r.descriptor);
    if let Some(value) = state.static_values.get(&cache_key) {
        return Some(value.clone());
    }
    let lower_name = r.name.to_ascii_lowercase();
    let value = if (r.owner == VANILLA_HUD
        && r.descriptor == "Lnet/minecraft/resources/Identifier;"
        && HUD_IDS.iter().any(|id| *id == lower_name))
        || (r.owner == DEBUG_ENTRIES
            && r.descriptor == "Lnet/minecraft/resources/Identifier;"
            && DEBUG_IDS.iter().any(|id| *id == lower_name))
    {
        Some(crate::runtime::host_ids::identifier_of_text(&format!(
            "minecraft:{lower_name}"
        )))
    } else if r.owner == DEBUG_ENTRIES && r.name == "PROFILES" && r.descriptor == "Ljava/util/Map;"
    {
        Some(debug_profiles_map("profiles"))
    } else if r.owner == DEBUG_STATUS
        && r.descriptor == "Lnet/minecraft/client/gui/components/debug/DebugScreenEntryStatus;"
        && matches!(r.name.as_str(), "IN_OVERLAY" | "ALWAYS_ON")
    {
        Some(debug_status(&r.name))
    } else if r.owner == DEBUG_PROFILE
        && r.descriptor == "Lnet/minecraft/client/gui/components/debug/DebugScreenProfile;"
        && matches!(r.name.as_str(), "DEFAULT" | "PERFORMANCE")
    {
        Some(Value::Ref(Some(format!("enum/{DEBUG_PROFILE}/{}", r.name))))
    } else {
        None
    }?;
    state.static_values.insert(cache_key, value.clone());
    Some(value)
}

fn model_layer_identity(state: &State, value: &Value) -> Option<(String, String)> {
    let object = crate::runtime::jar_object::instance_key(value)?;
    let (id, layer) = state.model_locations.get(object)?;
    Some((
        format!("{id}\u{1f}{layer}"),
        format!("ModelLayerLocation[model={id}, layer={layer}]"),
    ))
}

fn particle_unit_codec(particle_key: &str, declared: &str, name: &str) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::OpaqueCodec,
        &[
            Value::Ref(Some(declared.to_owned())),
            Value::Ref(Some(format!("particle/{particle_key}"))),
            Value::Ref(Some(name.to_owned())),
        ],
    ))
}

/// Static factories and registration-only static APIs.
pub(crate) fn invoke(
    state: &mut State,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    match (r.owner.as_str(), r.name.as_str()) {
        (PARTICLE_TYPES, "simple") => {
            let limiter = match r.descriptor.as_str() {
                "()Lnet/minecraft/core/particles/SimpleParticleType;" => false,
                "(Z)Lnet/minecraft/core/particles/SimpleParticleType;" => {
                    matches!(args.first(), Some(Value::Int(value)) if *value != 0)
                }
                _ => return None,
            };
            let key = state.next_particle_type;
            state.next_particle_type = state.next_particle_type.saturating_add(1);
            let particle = Value::Host(HostRef::new(
                HostKind::ParticleType,
                format!("simple/{key}/{}/", i32::from(limiter)),
            ));
            let particle_key = match &particle {
                Value::Host(handle) => handle.key().to_owned(),
                _ => unreachable!(),
            };
            state.particle_codecs.insert(
                particle_key.clone(),
                (
                    particle_unit_codec(&particle_key, MAP_CODEC, "codec"),
                    particle_unit_codec(&particle_key, STREAM_CODEC, "streamCodec"),
                ),
            );
            Some(Ok(particle))
        }
        (PARTICLE_TYPES, "complex") => {
            let limiter = match (r.descriptor.as_str(), args.len()) {
                (
                    "(Lcom/mojang/serialization/MapCodec;Lnet/minecraft/network/codec/StreamCodec;)Lnet/minecraft/core/particles/ParticleType;",
                    2,
                )
                | (
                    "(Ljava/util/function/Function;Ljava/util/function/Function;)Lnet/minecraft/core/particles/ParticleType;",
                    2,
                ) => false,
                (
                    "(ZLcom/mojang/serialization/MapCodec;Lnet/minecraft/network/codec/StreamCodec;)Lnet/minecraft/core/particles/ParticleType;",
                    3,
                )
                | (
                    "(ZLjava/util/function/Function;Ljava/util/function/Function;)Lnet/minecraft/core/particles/ParticleType;",
                    3,
                ) => {
                    matches!(args.first(), Some(Value::Int(value)) if *value != 0)
                }
                _ => return None,
            };
            let key = state.next_particle_type;
            state.next_particle_type = state.next_particle_type.saturating_add(1);
            let family = if r.descriptor.contains("java/util/function/Function") {
                "complex-function"
            } else {
                "complex"
            };
            let particle = Value::Host(HostRef::new(
                HostKind::ParticleType,
                format!("{family}/{key}/{}/", i32::from(limiter)),
            ));
            if family == "complex" {
                let particle_key = match &particle {
                    Value::Host(handle) => handle.key().to_owned(),
                    _ => unreachable!(),
                };
                state.particle_codecs.insert(
                    particle_key,
                    (args[args.len() - 2].clone(), args[args.len() - 1].clone()),
                );
            }
            Some(Ok(particle))
        }
        (PARTICLE_REGISTRY, "getInstance") => Some(Ok(client_handle("particle-provider-registry"))),
        (PARTICLE_REGISTRY, "register") => {
            let particle_type = args.first().cloned().unwrap_or(Value::Ref(None));
            let provider = args.get(1).cloned().unwrap_or(Value::Ref(None));
            state.registrations.push(Registration::ParticleProvider {
                particle_type,
                provider,
            });
            Some(Ok(Value::Void))
        }
        (DEBUG_ENTRIES, "register") if args.len() == 2 => {
            let id = identifier(&args[0]).unwrap_or_else(|| value_key(&args[0]));
            state.registrations.push(Registration::DebugEntry {
                id: id.clone(),
                entry: args[1].clone(),
            });
            Some(Ok(args[0].clone()))
        }
        (MENU_SCREENS, "register") if args.len() == 2 => {
            let key = value_key(&args[0]);
            if !state.menu_screens.insert(key.clone()) {
                return Some(Err(illegal_state(format!(
                    "Duplicate registration for {key}"
                ))));
            }
            state.registrations.push(Registration::MenuScreen {
                menu: args[0].clone(),
                constructor: args[1].clone(),
            });
            Some(Ok(Value::Void))
        }
        (ENTITY_RENDERERS, "register") if args.len() == 2 => {
            state.registrations.push(Registration::EntityRenderer {
                entity_type: args[0].clone(),
                provider: args[1].clone(),
            });
            Some(Ok(Value::Void))
        }
        (MODEL_LAYERS, "registerModelLayer") if args.len() == 2 => {
            if args[0] == Value::Ref(None) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("ModelLayerLocation cannot be null".into()),
                    frame: None,
                }));
            }
            if args[1] == Value::Ref(None) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("TexturedLayerDefinitionProvider cannot be null".into()),
                    frame: None,
                }));
            }
            let (key, text) = model_layer_identity(state, &args[0])?;
            if !state.model_layers.insert(key) {
                return Some(Err(illegal_argument(format!(
                    "Cannot replace registration for model layer \"{text}\""
                ))));
            }
            state.registrations.push(Registration::ModelLayer {
                location: args[0].clone(),
                provider: args[1].clone(),
            });
            Some(Ok(Value::Void))
        }
        (MODEL_LAYERS, "registerArmorModelLayers") if args.len() == 2 => {
            if args[0] == Value::Ref(None) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("ArmorModelSet cannot be null".into()),
                    frame: None,
                }));
            }
            if args[1] == Value::Ref(None) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("TexturedArmorModelSetProvider cannot be null".into()),
                    frame: None,
                }));
            }
            let key = value_key(&args[0]);
            if !state.armor_layers.insert(key.clone()) {
                return Some(Err(illegal_argument(format!(
                    "Cannot replace registration for armor model layer \"{key}\""
                ))));
            }
            state.registrations.push(Registration::ModelLayer {
                location: args[0].clone(),
                provider: args[1].clone(),
            });
            Some(Ok(Value::Void))
        }
        (PREPARABLE_MODEL_LOADING, "register") if args.len() == 2 => {
            if args[0] == Value::Ref(None) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("data loader must not be null".into()),
                    frame: None,
                }));
            }
            if args[1] == Value::Ref(None) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("plugin must not be null".into()),
                    frame: None,
                }));
            }
            state.registrations.push(Registration::ModelLoadingPlugin {
                plugin: args[1].clone(),
            });
            Some(Ok(Value::Void))
        }
        (MODEL_LOADING, "register") if args.len() == 1 => {
            if args[0] == Value::Ref(None) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("plugin must not be null".into()),
                    frame: None,
                }));
            }
            state.registrations.push(Registration::ModelLoadingPlugin {
                plugin: args[0].clone(),
            });
            Some(Ok(Value::Void))
        }
        _ => None,
    }
}

/// Instance calls on the particle registry singleton.
pub(crate) fn instance(
    state: &mut State,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if let Some(answer) = profile_map_call(r, receiver, args) {
        return Some(answer);
    }
    if r.owner == MODEL_LOCATION
        && r.name == "<init>"
        && matches!(
            r.descriptor.as_str(),
            "(Lnet/minecraft/resources/Identifier;Ljava/lang/String;)V"
                | "(Ljava/lang/Object;Ljava/lang/String;)V"
        )
    {
        let (Some(object), Some(id), Some(layer)) = (
            crate::runtime::jar_object::instance_key(receiver).map(str::to_owned),
            args.first().and_then(identifier),
            args.get(1).and_then(crate::runtime::expect_ref).cloned(),
        ) else {
            return None;
        };
        state.model_locations.insert(object, (id, layer));
        return Some(Ok(Value::Void));
    }
    if let Value::Host(handle) = receiver
        && handle.kind() == HostKind::ParticleType
    {
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("codec", "()Lcom/mojang/serialization/MapCodec;") => {
                return state
                    .particle_codecs
                    .get(handle.key())
                    .map(|(codec, _)| Ok(codec.clone()));
            }
            ("streamCodec", "()Lnet/minecraft/network/codec/StreamCodec;") => {
                return state
                    .particle_codecs
                    .get(handle.key())
                    .map(|(_, stream_codec)| Ok(stream_codec.clone()));
            }
            ("getOverrideLimiter", "()Z") => {
                let enabled = handle
                    .key()
                    .split('/')
                    .nth(2)
                    .is_some_and(|value| value == "1");
                return Some(Ok(Value::Int(i32::from(enabled))));
            }
            ("getType", "()Lnet/minecraft/core/particles/SimpleParticleType;")
                if handle.key().starts_with("simple/") =>
            {
                return Some(Ok(receiver.clone()));
            }
            _ => {}
        }
    }
    if r.owner != PARTICLE_REGISTRY || r.name != "register" || args.len() != 2 {
        return None;
    }
    if crate::runtime::game_object::handle_key(receiver) != Some("client/particle-provider-registry") {
        return None;
    }
    state.registrations.push(Registration::ParticleProvider {
        particle_type: args[0].clone(),
        provider: args[1].clone(),
    });
    Some(Ok(Value::Void))
}

pub(crate) fn summary(state: &State) -> serde_json::Value {
    let order = state
        .registrations
        .iter()
        .map(|registration| match registration {
            Registration::DebugEntry { id, .. } => format!("debug:{id}"),
            Registration::ParticleProvider { .. } => "particle_provider".to_owned(),
            Registration::MenuScreen { .. } => "menu_screen".to_owned(),
            Registration::EntityRenderer { .. } => "entity_renderer".to_owned(),
            Registration::ModelLayer { .. } => "model_layer".to_owned(),
            Registration::ModelLoadingPlugin { .. } => "model_loading_plugin".to_owned(),
        })
        .collect::<Vec<_>>();
    serde_json::json!({ "count": order.len(), "order": order })
}
