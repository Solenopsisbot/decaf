//! A linked jar's resources, found by `Class.getResourceAsStream` and
//! `getResource` the way a JVM class path finds them.
//!
//! A JVM's class loader looks a resource up across every jar on the class
//! path, the running jar first and then the others in order, and the first jar
//! that holds the name answers. The host used to answer only the running jar's
//! own resources, so a linked library reading its bundled defaults got `null`:
//! Forge Config API Port's `ForgeConfigApiPortConfig.loadFrom` does
//! `Objects.requireNonNull(getClass().getResourceAsStream("/forgeconfigapiport.toml"))`
//! and failed with an NPE in every Fabric jar that links it.
//!
//! The linked table keeps each linked jar's resources (the bytes `NativeJar`
//! already bounds: 4 MiB a resource, 32 MiB a jar) under the first jar that
//! names them, as it keeps classes. The running jar's own resources still
//! answer first.

use super::*;

impl LinkedClassTable {
    /// Keep `jar`'s resources whose names no earlier jar in the table took (for classpath lookup),
    /// and record `jar`'s `data/` resources with its Fabric metadata for pack-ordered recipe/tag loading.
    pub(crate) fn link_resources(&mut self, id: &str, jar: &NativeJar) {
        self.bump_cache_id();
        let mut data_layer = BTreeMap::new();
        for (name, bytes) in &jar.resource_bytes {
            if name.ends_with(".class") {
                continue;
            }
            if !jar.data_namespaces_unknown && name.starts_with("data/") {
                data_layer.insert(name.clone(), bytes.clone());
            }
            self.resources
                .entry(name.clone())
                .or_insert_with(|| (format!("resource-bytes/{name}"), bytes.clone()));
        }
        if !data_layer.is_empty() || !jar.data_namespaces.is_empty() || jar.data_namespaces_unknown
        {
            let fabric_doc = jar
                .resource_bytes
                .get("fabric.mod.json")
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok());
            let fabric_mod_id = jar.fabric_id.clone().or_else(|| {
                fabric_doc
                    .as_ref()
                    .and_then(|doc| doc.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            });
            let resource_load_order = fabric_doc
                .as_ref()
                .and_then(|doc| doc.get("custom"))
                .and_then(|custom| custom.get("fabric:resource_load_order"))
                .cloned();
            self.data_layers.push(crate::runtime::pack_order::JarDataLayer {
                link_id: id.to_owned(),
                fabric_mod_id,
                resource_load_order,
                data_namespaces: if jar.data_namespaces_unknown {
                    BTreeSet::new()
                } else {
                    jar.data_namespaces.clone()
                },
                data_namespaces_unknown: jar.data_namespaces_unknown,
                resources: data_layer,
            });
        }
    }

    /// The marker and bytes of a resource a linked jar holds.
    pub(crate) fn resource(&self, name: &str) -> Option<(&str, &[u8])> {
        self.resources
            .get(name)
            .map(|(marker, bytes)| (marker.as_str(), bytes.as_slice()))
    }

    /// Per-jar `data/` resource layers across all linked/nested jars.
    pub(crate) fn data_layers(&self) -> &[crate::runtime::pack_order::JarDataLayer] {
        &self.data_layers
    }

    /// Remove any linked/nested data layer owned by `mod_id` when Fabric replaces an older nested
    /// container with a newer or top-level container.
    pub(crate) fn remove_fabric_data_layer(&mut self, mod_id: &str) {
        self.data_layers
            .retain(|layer| layer.fabric_mod_id.as_deref() != Some(mod_id));
    }

    /// Total `data/` entries across all linked jar layers (used for cache invalidation).
    pub(crate) fn data_layer_entry_count(&self) -> usize {
        self.data_layers
            .iter()
            .map(|layer| {
                layer.resources.len()
                    + layer.data_namespaces.len()
                    + usize::from(layer.data_namespaces_unknown)
            })
            .sum()
    }
}
