//! Data component types a jar builds itself, and the default values its items give them.
//!
//! A jar may build a `DataComponentType` with `DataComponentType.builder().persistent(codec)
//! ...build()` (`declarations_content.rs` answers the builder and keeps the codec object) and
//! register it into `minecraft:data_component_type`, from its own code or from a Mixin into a game
//! class's static initializer (`bootstrap_mixins.rs`). An item's `Item.Properties.component(type,
//! value)` then gives the item that component by default.
//!
//! Sunrise stores such a component as NBT (`Hooks::component_types`, "NBT component types"), and a
//! content package declares an item's default as JSON its loader turns into NBT. So:
//!
//! * [`StdHost::jar_component_types`] lists every type the jar built and registered, with whether
//!   it has a persistent codec (only those are NBT component types);
//! * [`NativeJar::declare_jar_component_defaults`] encodes each item's default value with its
//!   type's own codec, as `codec.encodeStart(NbtOps.INSTANCE, value)` would
//!   (`dfu_codec::encode_value_nbt`), and adds it to the item's declaration as
//!   `component:<type id>`, in the JSON whose Sunrise reading is exactly that NBT
//!   ([`content_json`]).
//!
//! A default the declaration can't carry exactly is counted under [`UNENCODED`] and left out: a
//! type that was never registered or has no persistent codec, a value the host did not build as an
//! object, a codec error, or NBT Sunrise's content JSON has no spelling for. A codec the DFU models
//! can't run is counted by the encode itself (`Codec#encodeStart`).

use super::*;

/// The `Item.Properties.component` setting of a jar-built type: `component:object:<type marker>`
/// holding the value's object marker, empty for `null` (vanilla's `DataComponentMap.Builder.set`
/// removes the type) or [`VALUE_UNKNOWN`] for a value that is not an object the host built. The
/// `component:` prefix keeps the value rooted while its builder lives (`heap.rs`).
pub(crate) const SETTING_PREFIX: &str = "component:object:";
/// [`SETTING_PREFIX`]'s value for a value no object key can spell.
const VALUE_UNKNOWN: &str = "?";
/// The unknown-call key of an item default a declaration cannot carry exactly.
pub(crate) const UNENCODED: &str = "net/minecraft/core/component/DataComponentType#codec";
/// The internal name of the type objects the interpreter builds.
const COMPONENT_TYPE: &str = "net/minecraft/core/component/DataComponentType";
/// `Item.Properties.component`'s descriptor.
const COMPONENT_DESCRIPTOR: &str = "(Lnet/minecraft/core/component/DataComponentType;Ljava/lang/Object;)Lnet/minecraft/world/item/Item$Properties;";
/// The largest default (as JSON text) a declaration carries, and how deep it may nest. Sunrise
/// refuses a value past the network NBT limits (`sunrise-core` `nbt::validate_component_value`)
/// and, with it, the whole package; these bounds stay far inside them, and a larger value is
/// counted rather than measured against Sunrise's accounting.
const MAX_DEFAULT_JSON_BYTES: usize = 16 * 1024;
const MAX_DEFAULT_DEPTH: usize = 64;

/// One data component type a jar built and registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JarComponentType {
    /// The id it is registered under.
    pub id: String,
    /// Whether its builder's `persistent` was given a codec: only then is it saved, and only then
    /// is it an NBT component type to Sunrise.
    pub persistent: bool,
}

/// The builder setting `Item.Properties.component(type, value)` records when `type` is a
/// `DataComponentType` the interpreter built (a jar's own type), as `(key, value)`; `None` for any
/// other call (a vanilla type is a game handle, handled with the item's other settings).
pub(crate) fn component_setting(
    method: &str,
    descriptor: &str,
    args: &[Value],
) -> Option<(String, String)> {
    if method != "component" || descriptor != COMPONENT_DESCRIPTOR {
        return None;
    }
    let kind = args.first()?;
    let kind_key = jar_object::instance_key(kind)?;
    if jar_object::key_class(kind_key) != Some(COMPONENT_TYPE) {
        return None;
    }
    let kind_marker = expect_ref(kind)?;
    let value = match args.get(1) {
        Some(Value::Ref(None)) => String::new(),
        Some(value) => jar_object::instance_key(value)
            .filter(|key| jar_object::key_class(key).is_some())
            .and_then(|_| expect_ref(value))
            .cloned()
            .unwrap_or_else(|| VALUE_UNKNOWN.to_owned()),
        None => VALUE_UNKNOWN.to_owned(),
    };
    Some((format!("{SETTING_PREFIX}{kind_marker}"), value))
}

impl StdHost {
    /// Every data component type the jar built and registered into
    /// `minecraft:data_component_type`, in registration order, each id once.
    pub fn jar_component_types(&self) -> Vec<JarComponentType> {
        let mut seen = BTreeSet::new();
        self.registrations
            .iter()
            .filter(|entry| entry.registry == "minecraft:data_component_type")
            .filter_map(|entry| {
                let marker = expect_ref(&entry.value)?;
                let built = matches!(
                    entry.declaration,
                    Some(RegistryDeclaration::Custom { ref kind, .. }) if kind == "data_component"
                );
                (built && seen.insert(entry.id.clone())).then(|| JarComponentType {
                    id: entry.id.clone(),
                    persistent: self.content_builders.component_codec(marker).is_some(),
                })
            })
            .collect()
    }

    /// The id and persistent codec of the jar-built type `marker`, when it was registered.
    fn registered_component(&self, marker: &str) -> Option<(String, Option<Value>)> {
        let entry = self.registrations.iter().find(|entry| {
            entry.registry == "minecraft:data_component_type"
                && expect_ref(&entry.value).is_some_and(|value| value == marker)
        })?;
        Some((
            entry.id.clone(),
            self.content_builders.component_codec(marker).cloned(),
        ))
    }

    fn count_unencoded_component(&mut self) {
        *self
            .unknown_host_calls
            .entry(UNENCODED.to_owned())
            .or_default() += 1;
    }
}

/// The prototype value of the jar-built type `component` on the jar item `item_id`, as its
/// `Item.Properties.component(type, value)` call set it: `Some(Some(Some(object)))` for the very
/// object the jar built (vanilla's `Item.components()` holds that object, so `ItemStack.get`
/// answers it by identity), `Some(Some(None))` for a `null` (the type removed), `Some(None)`
/// (unknown: a read counts) for a value the host did not build as an object, and `None` when
/// the item's builder recorded no such call (the caller's other prototype sources answer).
pub(crate) fn recorded_item_default(
    host: &StdHost,
    item_id: &str,
    component: &str,
) -> Option<Option<Option<Value>>> {
    let item = host
        .registrations
        .iter()
        .rev()
        .find(|entry| entry.registry == "minecraft:item" && entry.id == item_id)?;
    let builder = host.item_property_builders.get(expect_ref(&item.value)?)?;
    let kind =
        host.registrations.iter().rev().find(|entry| {
            entry.registry == "minecraft:data_component_type" && entry.id == component
        })?;
    let value = builder.get(&format!("{SETTING_PREFIX}{}", expect_ref(&kind.value)?))?;
    Some(match value.as_str() {
        "" => Some(None),
        VALUE_UNKNOWN => None,
        marker => Some(Some(jar_object::stored_value(marker))),
    })
}

impl NativeJar {
    /// Adds each registered item's jar-typed default components to its declaration
    /// (`component:<type id>`, the JSON [`content_json`] gives the value's NBT), encoding each with
    /// its type's persistent codec. Run once a jar's entrypoints have run, before its declarations
    /// are read. Each pass takes only the registrations made since the last one, so running it
    /// again (after a later stage registered more) declares and counts nothing twice.
    pub fn declare_jar_component_defaults(&self, host: &mut StdHost) {
        let first = host.component_defaults_declared;
        host.component_defaults_declared = host.registrations.len();
        // (registration index, setting value, type marker) for each jar-typed setting of each
        // item declared from its builder.
        let mut work = Vec::new();
        for (index, entry) in host.registrations.iter().enumerate().skip(first) {
            if entry.registry != "minecraft:item" {
                continue;
            }
            let Some(RegistryDeclaration::Item { .. }) = entry.declaration else {
                continue;
            };
            let Some(builder) =
                expect_ref(&entry.value).and_then(|marker| host.item_property_builders.get(marker))
            else {
                continue;
            };
            for (key, value) in builder {
                if let Some(kind) = key.strip_prefix(SETTING_PREFIX) {
                    work.push((index, value.clone(), kind.to_owned()));
                }
            }
        }
        for (index, value, kind) in work {
            // `null` removes the type from the item's map; the template (a vanilla item) never
            // has a jar's type, so there is nothing to declare.
            if value.is_empty() {
                continue;
            }
            let Some((id, Some(codec))) = host.registered_component(&kind) else {
                // Never registered (the item holds a type the game can't save or sync), or
                // transient (no codec: not an NBT component type).
                host.count_unencoded_component();
                continue;
            };
            if value == VALUE_UNKNOWN {
                host.count_unencoded_component();
                continue;
            }
            let property = format!("component:{id}");
            let object = jar_object::stored_value(&value);
            let encoded = match self.encode_dfu_codec_nbt(&codec, &object, host) {
                Ok(DfuCodecResult::Ok(nbt)) => content_json(&nbt),
                // Counted by the encode itself (`Codec#encodeStart`).
                Ok(DfuCodecResult::Unmodelled(_)) => continue,
                Ok(DfuCodecResult::Error { .. }) | Err(_) => None,
            };
            let Some(json) = encoded else {
                host.count_unencoded_component();
                continue;
            };
            if let Some(RegistryDeclaration::Item { properties, .. }) =
                &mut host.registrations[index].declaration
            {
                properties.insert(property, json.to_string());
            }
        }
    }
}

/// The content-file JSON whose Sunrise reading is exactly `nbt`, or `None` when it has none.
///
/// Sunrise reads an item file's value for a native component type with
/// `sunrise_client_core::scoreboard::json_to_tag` (`mod_content.rs` `component_patch`): `true`/
/// `false` are `ByteTag` 1/0, an integer number an `IntTag` (or a `LongTag` outside `int`), any
/// other number a `DoubleTag`, a string a `StringTag`, an array a `ListTag` and an object a
/// `CompoundTag`. So a `ByteTag` other than 0 or 1, a `ShortTag`, a `FloatTag`, a `LongTag` an
/// `int` holds, the array tags, and a double that is not finite or is `-0.0` (which
/// `validate_component_value` refuses: disk NBT normalises it) have no spelling. A value nested
/// deeper than [`MAX_DEFAULT_DEPTH`] or longer than [`MAX_DEFAULT_JSON_BYTES`] is refused too.
pub fn content_json(nbt: &Nbt) -> Option<serde_json::Value> {
    let json = content_json_depth(nbt, 0)?;
    (json.to_string().len() <= MAX_DEFAULT_JSON_BYTES).then_some(json)
}

fn content_json_depth(nbt: &Nbt, depth: usize) -> Option<serde_json::Value> {
    use serde_json::Value as J;
    if depth > MAX_DEFAULT_DEPTH {
        return None;
    }
    Some(match nbt {
        Nbt::Byte(0) => J::Bool(false),
        Nbt::Byte(1) => J::Bool(true),
        Nbt::Int(value) => J::from(*value),
        Nbt::Long(value) if i32::try_from(*value).is_err() => J::from(*value),
        Nbt::Double(bits) => {
            let value = f64::from_bits(*bits);
            if !value.is_finite() || (value == 0.0 && value.is_sign_negative()) {
                return None;
            }
            J::Number(serde_json::Number::from_f64(value)?)
        }
        Nbt::String(value) => J::String(value.clone()),
        Nbt::List(values) => J::Array(
            values
                .iter()
                .map(|value| content_json_depth(value, depth + 1))
                .collect::<Option<_>>()?,
        ),
        Nbt::Compound(entries) => J::Object(
            entries
                .iter()
                .map(|(key, value)| Some((key.clone(), content_json_depth(value, depth + 1)?)))
                .collect::<Option<_>>()?,
        ),
        Nbt::Byte(_)
        | Nbt::Short(_)
        | Nbt::Long(_)
        | Nbt::Float(_)
        | Nbt::ByteArray(_)
        | Nbt::IntArray(_)
        | Nbt::LongArray(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each tag `json_to_tag` can produce comes back as its JSON, and each it can't is refused.
    #[test]
    fn content_json_spells_exactly_the_tags_sunrise_reads_back() {
        let compound = |entries: Vec<(&str, Nbt)>| {
            Nbt::Compound(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value))
                    .collect(),
            )
        };
        assert_eq!(
            content_json(&compound(vec![
                ("content", Nbt::List(Vec::new())),
                ("size_multiplier", Nbt::Int(4)),
            ])),
            Some(serde_json::json!({"content": [], "size_multiplier": 4}))
        );
        assert_eq!(content_json(&Nbt::Byte(1)), Some(serde_json::json!(true)));
        assert_eq!(content_json(&Nbt::Byte(0)), Some(serde_json::json!(false)));
        assert_eq!(content_json(&Nbt::Byte(2)), None);
        assert_eq!(content_json(&Nbt::Short(1)), None);
        assert_eq!(content_json(&Nbt::Float(1.0f32.to_bits())), None);
        assert_eq!(content_json(&Nbt::Long(1)), None);
        assert_eq!(
            content_json(&Nbt::Long(1 << 40)),
            Some(serde_json::json!(1_i64 << 40))
        );
        assert_eq!(
            content_json(&Nbt::Double(2.0f64.to_bits())).map(|json| json.to_string()),
            Some("2.0".to_owned())
        );
        assert_eq!(content_json(&Nbt::Double((-0.0f64).to_bits())), None);
        assert_eq!(content_json(&Nbt::Double(f64::NAN.to_bits())), None);
        assert_eq!(content_json(&Nbt::IntArray(vec![1])), None);
        assert_eq!(
            content_json(&Nbt::List(vec![Nbt::String("a".into()), Nbt::Short(1)])),
            None
        );
        let mut deep = Nbt::Int(0);
        for _ in 0..=MAX_DEFAULT_DEPTH {
            deep = Nbt::List(vec![deep]);
        }
        assert_eq!(content_json(&deep), None);
    }
}
