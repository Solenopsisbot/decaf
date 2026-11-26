//! A typed registration as the fork's content declaration: the one mapping both the game bridge
//! (`mod/src/content.rs`) and the offline bake (`tools/bake.py`, through the probe's `content`
//! object) use.
//!
//! The runtime records what a jar's `Registry.register` calls built — an item's
//! `Item.Properties` chain, a block's `BlockBehaviour.Properties` chain — as typed
//! [`RegistryDeclaration`]s with the property strings the host understood. This module turns
//! those into the narrow vocabulary a content package declares
//! (`data/<namespace>/sunrise/{block,item}/<path>.json`): the template plus the property keys the
//! fork reads. Anything else stays in the jar's row and is mapped in `docs/m3-content-design.md`,
//! never guessed into content.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::{RegistryDeclaration, RegistryEntry};

/// `content::Rarity`, by its file spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
}

impl Rarity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "common" => Some(Self::Common),
            "uncommon" => Some(Self::Uncommon),
            "rare" => Some(Self::Rare),
            "epic" => Some(Self::Epic),
            _ => None,
        }
    }
}

/// A block a jar registered, in the content vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: String,
    /// The vanilla block it copies, `minecraft:<path>`.
    pub template: String,
    /// Whether it brings a block item of the same id.
    pub item: bool,
    pub hardness: Option<f32>,
    pub explosion_resistance: Option<f32>,
    pub friction: Option<f32>,
    pub requires_tool: Option<bool>,
    pub state_properties: BTreeMap<String, StateProperty>,
    pub default_state: BTreeMap<String, String>,
    pub occludes: Option<bool>,
    /// Block-local outline shape table (`sunrise-mod-package` `BlockShapes`), resolved by running
    /// the block's own `getShape` across every state.
    pub shapes: Option<BlockShapes>,
    /// Block-local collision shape table (`sunrise-mod-package` `BlockShapes`), emitted when
    /// `getCollisionShape` differs from `getShape` on any state (e.g. `Properties.noCollision()`
    /// or a separate `getCollisionShape` override).
    pub collision_shapes: Option<BlockShapes>,
}

/// Block-local outline boxes selected by an ordered tuple of state properties
/// (`sunrise-mod-package/src/content.rs` `BlockShapes`).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockShapes {
    /// State property names selecting a shape, in tuple order. Empty when `default` applies to
    /// every state.
    pub properties: Vec<String>,
    /// Comma-separated property-value tuples mapped to boxes `[min_x, min_y, min_z, max_x, max_y, max_z]`.
    pub values: BTreeMap<String, Vec<[f64; 6]>>,
    /// Boxes used when no tuple matches (or for every state when `properties`/`values` are empty).
    pub default: Option<Vec<[f64; 6]>>,
}

impl BlockShapes {
    /// Serialize into Sunrise's `BlockFile.shapes` JSON object.
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        if !self.properties.is_empty() {
            object.insert("properties".into(), json!(self.properties));
        }
        if !self.values.is_empty() {
            let values = self
                .values
                .iter()
                .map(|(k, boxes)| (k.clone(), json!(boxes)))
                .collect::<Map<_, _>>();
            object.insert("values".into(), Value::Object(values));
        }
        if let Some(default) = &self.default {
            object.insert("default".into(), json!(default));
        }
        Value::Object(object)
    }

    /// Parse from the JSON representation stored on a `RegistryDeclaration::Block`.
    pub fn from_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let parse_boxes = |value: &Value| -> Option<Vec<[f64; 6]>> {
            value
                .as_array()?
                .iter()
                .map(|entry| {
                    let coords = entry.as_array()?;
                    if coords.len() != 6 {
                        return None;
                    }
                    let mut out = [0.0; 6];
                    for (slot, raw) in out.iter_mut().zip(coords) {
                        *slot = raw.as_f64()?;
                    }
                    Some(out)
                })
                .collect()
        };
        let properties = match object.get("properties") {
            Some(raw) => raw
                .as_array()?
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()?,
            None => Vec::new(),
        };
        let values = match object.get("values") {
            Some(raw) => raw
                .as_object()?
                .iter()
                .map(|(key, entry)| Some((key.clone(), parse_boxes(entry)?)))
                .collect::<Option<BTreeMap<_, _>>>()?,
            None => BTreeMap::new(),
        };
        let default = match object.get("default") {
            Some(raw) => Some(parse_boxes(raw)?),
            None => None,
        };
        Some(Self {
            properties,
            values,
            default,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateProperty {
    pub kind: String,
    pub values: Vec<String>,
}

/// An item a jar registered, in the content vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    /// The vanilla item it copies, `minecraft:<path>`.
    pub template: String,
    pub max_stack_size: Option<u8>,
    pub rarity: Option<Rarity>,
    pub crafting_remainder: Option<String>,
    /// The 26.3 data component patch over the template the jar's `Item.Properties.component`
    /// calls set (only the types the host encodes exactly): `minecraft:lore` as its codec writes
    /// it to JSON, and each type the jar built and registered with a persistent codec as the JSON
    /// Sunrise reads back as the NBT that codec writes (`jar_components.rs`). The loader's
    /// `components` key (`sunrise-mod-package` `ItemFile`).
    pub components: BTreeMap<String, Value>,
}

/// One block or item a jar registered.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentDeclaration {
    Block(Block),
    Item(Item),
}

impl ContentDeclaration {
    pub fn id(&self) -> &str {
        match self {
            Self::Block(block) => &block.id,
            Self::Item(item) => &item.id,
        }
    }

    /// The namespace half of the id.
    pub fn namespace(&self) -> &str {
        self.id()
            .split_once(':')
            .map_or("", |(namespace, _)| namespace)
    }

    /// The path half of the id: the declaration file's name.
    pub fn path(&self) -> &str {
        self.id().split_once(':').map_or("", |(_, path)| path)
    }

    /// Whether the declaration is a block.
    pub fn is_block(&self) -> bool {
        matches!(self, Self::Block(_))
    }

    /// The `data/<namespace>/sunrise/{block,item}/<path>.json` value the content loader reads
    /// (`sunrise-mod-package/src/content.rs`, `BlockFile`/`ItemFile`).
    pub fn file_json(&self) -> Value {
        match self {
            Self::Block(block) => {
                let mut properties = Map::new();
                if let Some(value) = block.hardness {
                    properties.insert("hardness".into(), json!(value));
                }
                if let Some(value) = block.explosion_resistance {
                    properties.insert("explosion_resistance".into(), json!(value));
                }
                if let Some(value) = block.friction {
                    properties.insert("friction".into(), json!(value));
                }
                if let Some(value) = block.requires_tool {
                    properties.insert("requires_tool".into(), json!(value));
                }
                if let Some(value) = block.occludes {
                    properties.insert("occludes".into(), json!(value));
                }
                let state_properties = block
                    .state_properties
                    .iter()
                    .map(|(name, property)| {
                        let output_kind = if property.kind == "integer" {
                            "enum"
                        } else {
                            property.kind.as_str()
                        };
                        let mut value = json!({"type": output_kind});
                        if property.kind == "enum" || property.kind == "integer" {
                            value["values"] = json!(property.values);
                        }
                        (name.clone(), value)
                    })
                    .collect::<Map<_, _>>();
                let mut file = json!({
                    "template": block.template,
                    "properties": Value::Object(properties),
                    "state_properties": Value::Object(state_properties),
                    "default_state": block.default_state,
                    "item": block.item,
                });
                if let Some(shapes) = &block.shapes {
                    file["shapes"] = shapes.to_json();
                }
                if let Some(collision_shapes) = &block.collision_shapes {
                    file["collision_shapes"] = collision_shapes.to_json();
                }
                file
            }
            Self::Item(item) => {
                let mut value = json!({ "template": item.template });
                if let Some(size) = item.max_stack_size {
                    value["max_stack_size"] = json!(size);
                }
                if let Some(rarity) = item.rarity {
                    value["rarity"] = json!(rarity.name());
                }
                if let Some(remainder) = &item.crafting_remainder {
                    value["crafting_remainder"] = json!(remainder);
                }
                if !item.components.is_empty() {
                    value["components"] = json!(item.components);
                }
                value
            }
        }
    }
}

/// Every typed block and item registration of a jar, in registration order.
pub fn declarations(entries: &[RegistryEntry]) -> Vec<ContentDeclaration> {
    entries
        .iter()
        .filter_map(|entry| match &entry.declaration {
            Some(RegistryDeclaration::Block {
                template,
                properties,
                item,
            }) => Some(ContentDeclaration::Block(Block {
                id: entry.id.clone(),
                template: template.clone(),
                item: *item,
                hardness: number(properties, "hardness"),
                explosion_resistance: number(properties, "explosion_resistance"),
                friction: number(properties, "friction"),
                requires_tool: properties.get("requires_tool").map(|value| value == "true"),
                state_properties: state_properties(properties),
                default_state: default_state(properties),
                occludes: properties
                    .get("occludes")
                    .and_then(|value| value.parse().ok()),
                shapes: properties
                    .get("shapes")
                    .and_then(|value| serde_json::from_str::<Value>(value).ok())
                    .and_then(|value| BlockShapes::from_json(&value)),
                collision_shapes: properties
                    .get("collision_shapes")
                    .and_then(|value| serde_json::from_str::<Value>(value).ok())
                    .and_then(|value| BlockShapes::from_json(&value)),
            })),
            Some(RegistryDeclaration::Item {
                template,
                properties,
            }) => Some(ContentDeclaration::Item(Item {
                id: entry.id.clone(),
                template: template.clone(),
                max_stack_size: properties
                    .get("max_stack_size")
                    .and_then(|value| value.parse().ok())
                    .filter(|size| (1..=99).contains(size)),
                rarity: properties
                    .get("rarity")
                    .and_then(|rarity| Rarity::parse(rarity)),
                crafting_remainder: properties.get("crafting_remainder").cloned(),
                components: item_components(properties),
            })),
            _ => None,
        })
        .collect()
}

/// The `component:<type>` properties as a component patch: each value is the component's JSON
/// as the host encoded it; one that does not parse is left out.
fn item_components(properties: &BTreeMap<String, String>) -> BTreeMap<String, Value> {
    properties
        .iter()
        .filter_map(|(key, value)| {
            let kind = key.strip_prefix("component:")?;
            Some((kind.to_owned(), serde_json::from_str(value).ok()?))
        })
        .collect()
}

fn number(properties: &BTreeMap<String, String>, key: &str) -> Option<f32> {
    properties.get(key).and_then(|value| value.parse().ok())
}

fn state_properties(properties: &BTreeMap<String, String>) -> BTreeMap<String, StateProperty> {
    properties
        .iter()
        .filter_map(|(key, value)| {
            let name = key.strip_prefix("state_property:")?;
            let (kind, values) = value.split_once('|')?;
            Some((
                name.to_owned(),
                StateProperty {
                    kind: kind.to_owned(),
                    values: values
                        .split(',')
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .collect(),
                },
            ))
        })
        .collect()
}

fn default_state(properties: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    properties
        .iter()
        .filter_map(|(key, value)| {
            Some((
                key.strip_prefix("default_state:")?.to_owned(),
                value.clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, declaration: RegistryDeclaration) -> RegistryEntry {
        RegistryEntry {
            registry: "minecraft:item".into(),
            id: id.into(),
            value: crate::Value::Void,
            declaration: Some(declaration),
        }
    }

    /// The content file is the vocabulary the fork reads, with only the keys the runtime
    /// recorded: a block's properties, and an item's stack size, rarity and remainder.
    #[test]
    fn declarations_become_the_content_file_json() {
        let properties = BTreeMap::from([
            ("hardness".to_owned(), "3".to_owned()),
            ("explosion_resistance".to_owned(), "3".to_owned()),
            ("requires_tool".to_owned(), "true".to_owned()),
            ("light".to_owned(), "7".to_owned()),
        ]);
        let entries = vec![
            entry(
                "m3_fixture:ruby_block",
                RegistryDeclaration::Block {
                    template: "minecraft:stone".into(),
                    properties: properties.clone(),
                    item: true,
                },
            ),
            entry(
                "m3_fixture:ruby",
                RegistryDeclaration::Item {
                    template: "minecraft:stick".into(),
                    properties: BTreeMap::from([
                        ("max_stack_size".to_owned(), "16".to_owned()),
                        ("rarity".to_owned(), "rare".to_owned()),
                        (
                            "crafting_remainder".to_owned(),
                            "minecraft:paper".to_owned(),
                        ),
                    ]),
                },
            ),
        ];
        let declarations = declarations(&entries);
        let block = declarations[0].file_json();
        assert_eq!(block["template"], "minecraft:stone");
        assert_eq!(block["item"], true);
        assert_eq!(block["properties"]["hardness"], 3.0);
        assert_eq!(block["properties"]["requires_tool"], true);
        // `light` has no content key; the declaration does not invent one.
        assert!(block["properties"].get("light").is_none());
        let item = declarations[1].file_json();
        assert_eq!(item["template"], "minecraft:stick");
        assert_eq!(item["max_stack_size"], 16);
        assert_eq!(item["rarity"], "rare");
        assert_eq!(item["crafting_remainder"], "minecraft:paper");
        assert_eq!(declarations[1].namespace(), "m3_fixture");
        assert_eq!(declarations[1].path(), "ruby");
    }

    /// BUGS-3d34-round2 #4: a `component:<type>` property is the item file's component patch,
    /// the value as the host encoded it; one that is not JSON is left out.
    #[test]
    fn recorded_components_become_the_item_files_component_patch() {
        let entries = vec![entry(
            "m3_fixture:pouch",
            RegistryDeclaration::Item {
                template: "minecraft:stick".into(),
                properties: BTreeMap::from([
                    ("max_stack_size".to_owned(), "1".to_owned()),
                    (
                        "component:minecraft:lore".to_owned(),
                        r#"[{"translate":"item.m3_fixture.pouch.hint","color":"gray"}]"#.to_owned(),
                    ),
                    (
                        "component:minecraft:broken".to_owned(),
                        "not json".to_owned(),
                    ),
                ]),
            },
        )];
        let item = declarations(&entries)[0].file_json();
        assert_eq!(
            item["components"],
            json!({"minecraft:lore": [{"translate": "item.m3_fixture.pouch.hint", "color": "gray"}]})
        );
        assert_eq!(item["max_stack_size"], 1);
    }

    /// A recorded stack size outside the content vocabulary's range is not carried, as the
    /// bridge has always refused it.
    #[test]
    fn an_out_of_range_stack_size_is_left_out() {
        let entries = vec![entry(
            "m3_fixture:ruby",
            RegistryDeclaration::Item {
                template: "minecraft:stick".into(),
                properties: BTreeMap::from([("max_stack_size".to_owned(), "1000".to_owned())]),
            },
        )];
        assert!(declarations(&entries)[0].file_json()["max_stack_size"].is_null());
    }

    /// Block shapes serialize at the top level (`"shapes"`) — never inside `"properties"`, which
    /// Sunrise parses with `#[serde(deny_unknown_fields)]` — supporting both uniform (`default`)
    /// and per-state (`properties` + `values`) tables.
    #[test]
    fn block_shapes_round_trip_at_top_level_for_uniform_and_per_state() {
        let uniform = BlockShapes {
            properties: Vec::new(),
            values: BTreeMap::new(),
            default: Some(vec![[0.125, 0.0, 0.125, 0.875, 0.25, 0.875]]),
        };
        let per_state = BlockShapes {
            properties: vec!["facing".to_owned()],
            values: BTreeMap::from([
                (
                    "east".to_owned(),
                    vec![[0.625, 0.0, 0.125, 1.0, 0.75, 0.875]],
                ),
                (
                    "north".to_owned(),
                    vec![[0.125, 0.0, 0.0, 0.875, 0.75, 0.375]],
                ),
                (
                    "south".to_owned(),
                    vec![[0.125, 0.0, 0.625, 0.875, 0.75, 1.0]],
                ),
                (
                    "west".to_owned(),
                    vec![[0.0, 0.0, 0.125, 0.375, 0.75, 0.875]],
                ),
            ]),
            default: None,
        };
        let entries = vec![
            entry(
                "m3_fixture:pedestal",
                RegistryDeclaration::Block {
                    template: "minecraft:stone".into(),
                    properties: BTreeMap::from([
                        ("shapes".to_owned(), uniform.to_json().to_string()),
                        (
                            "collision_shapes".to_owned(),
                            BlockShapes {
                                properties: Vec::new(),
                                values: BTreeMap::new(),
                                default: Some(Vec::new()),
                            }
                            .to_json()
                            .to_string(),
                        ),
                    ]),
                    item: false,
                },
            ),
            entry(
                "m3_fixture:directional",
                RegistryDeclaration::Block {
                    template: "minecraft:stone".into(),
                    properties: BTreeMap::from([(
                        "shapes".to_owned(),
                        per_state.to_json().to_string(),
                    )]),
                    item: false,
                },
            ),
        ];
        let parsed = declarations(&entries);
        let pedestal_json = parsed[0].file_json();
        assert_eq!(
            pedestal_json["shapes"],
            json!({ "default": [[0.125, 0.0, 0.125, 0.875, 0.25, 0.875]] })
        );
        assert_eq!(pedestal_json["collision_shapes"], json!({ "default": [] }));
        assert!(pedestal_json["properties"].get("shapes").is_none());
        assert!(
            pedestal_json["properties"]
                .get("collision_shapes")
                .is_none()
        );

        let dir_json = parsed[1].file_json();
        assert_eq!(
            dir_json["shapes"],
            json!({
                "properties": ["facing"],
                "values": {
                    "east": [[0.625, 0.0, 0.125, 1.0, 0.75, 0.875]],
                    "north": [[0.125, 0.0, 0.0, 0.875, 0.75, 0.375]],
                    "south": [[0.125, 0.0, 0.625, 0.875, 0.75, 1.0]],
                    "west": [[0.0, 0.0, 0.125, 0.375, 0.75, 0.875]],
                }
            })
        );
        assert!(dir_json.get("collision_shapes").is_none());
        assert!(dir_json["properties"].get("shapes").is_none());
    }
}
