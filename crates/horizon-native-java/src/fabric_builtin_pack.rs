//! Fabric's `ResourceLoader.registerBuiltinPack` (#478), read from `fabric-resource-loader-v1`
//! 3.1.0's bytecode and checked on Java 25 (`tests/fixture_fabric_builtin_pack.rs`):
//!
//! * `registerBuiltinPack(id, container, name, type)` is the sub-path overload with
//!   `"resourcepacks/" + id.getPath()`: the id's namespace is not part of the folder.
//! * `ResourceLoaderImpl.registerBuiltinPack(id, subPath, container, name, type)` calls
//!   `ModNioPackResources.create` for `CLIENT_RESOURCES` and for `SERVER_DATA` and answers whether
//!   either made a pack (adding each one made to `BUILTIN_PACK_RESOURCES`, which a mod can't read).
//! * `create` resolves the sub-path against each of the container's roots (a jar mod has one,
//!   its jar's root), keeps the ones that exist, and returns null when the pack it builds has no
//!   namespace: no folder under `assets/` (client) or `data/` (server) whose name matches
//!   `[a-z0-9-_.]+`. Others are logged and skipped. A `pack.mcmeta` alone makes no pack.
//!
//! Answered only for the running jar's own container: the host holds that jar's entries exactly,
//! and a zip folder exists when any entry is under it (a jar with or without directory entries
//! answers the same on Java 25). Counted: another mod's container (its root is a jar the host
//! doesn't hold), a null argument (it may be a counted call's answer), an `Identifier` the host
//! didn't make, and a sub-path with `.`, `..`, empty or absolute parts, which `normalize()` and
//! the `startsWith(root)` check could treat differently.

use crate::{MethodRef, Value, host_fabric_loader, host_ids};

const RESOURCE_LOADER: &str = "net/fabricmc/fabric/api/resource/v1/ResourceLoader";
const REGISTER: &str = "(Lnet/minecraft/resources/Identifier;Lnet/fabricmc/loader/api/ModContainer;Lnet/minecraft/network/chat/Component;Lnet/fabricmc/fabric/api/resource/v1/pack/PackActivationType;)Z";
const REGISTER_SUB_PATH: &str = "(Lnet/minecraft/resources/Identifier;Ljava/lang/String;Lnet/fabricmc/loader/api/ModContainer;Lnet/minecraft/network/chat/Component;Lnet/fabricmc/fabric/api/resource/v1/pack/PackActivationType;)Z";

/// What a `registerBuiltinPack` call is.
pub(crate) enum Call {
    /// Not `registerBuiltinPack`.
    Other,
    /// `registerBuiltinPack`, counted.
    Counted,
    /// `registerBuiltinPack`, answered.
    Answer(bool),
}

/// `registerBuiltinPack`, given the running jar's mod id and its entry names.
pub(crate) fn register<'a>(
    r: &MethodRef,
    args: &[Value],
    own_mod: Option<&str>,
    entries: impl Iterator<Item = &'a str>,
) -> Call {
    if r.owner != RESOURCE_LOADER || r.name != "registerBuiltinPack" {
        return Call::Other;
    }
    let (id, sub_path, container) = match (r.descriptor.as_str(), args) {
        (REGISTER, [id, container, _, _]) => {
            let Some(key) = host_ids::identifier_key(id) else {
                return Call::Counted;
            };
            let path = key.split_once(':').map_or(key, |(_, path)| path);
            (id, format!("resourcepacks/{path}"), container)
        }
        (REGISTER_SUB_PATH, [id, Value::Ref(Some(sub_path)), container, _, _]) => {
            (id, sub_path.clone(), container)
        }
        (REGISTER | REGISTER_SUB_PATH, _) => return Call::Counted,
        _ => return Call::Other,
    };
    if host_ids::identifier_key(id).is_none() || args.iter().any(|a| matches!(a, Value::Ref(None)))
    {
        return Call::Counted;
    }
    let Some(mod_id) = host_fabric_loader::container_id(container) else {
        return Call::Counted;
    };
    if own_mod != Some(mod_id.as_str()) || !plain_relative(&sub_path) {
        return Call::Counted;
    }
    Call::Answer(has_namespace(&sub_path, entries))
}

/// A relative path of plain names: resolving it against a root needs no normalising.
fn plain_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Whether `<sub>/assets/` or `<sub>/data/` holds a folder named like a namespace.
fn has_namespace<'a>(sub_path: &str, entries: impl Iterator<Item = &'a str>) -> bool {
    let assets = format!("{sub_path}/assets/");
    let data = format!("{sub_path}/data/");
    entries.into_iter().any(|entry| {
        let entry = entry.strip_prefix('/').unwrap_or(entry);
        let Some(rest) = entry
            .strip_prefix(&assets)
            .or_else(|| entry.strip_prefix(&data))
        else {
            return false;
        };
        // A folder: a name with something (or an explicit directory entry's `/`) after it.
        rest.split_once('/')
            .is_some_and(|(name, _)| is_namespace(name))
    })
}

/// `ModNioPackResources.RESOURCE_PACK_PATH`: `[a-z0-9-_.]+`.
fn is_namespace(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_folder_rule() {
        let entries = [
            "p/assets/demo/x.txt",
            "q/assets/x.txt",
            "r/data/ok_1.-x/y",
            "s/assets/Bad/x",
            "t/assets/empty/",
        ];
        let has = |sub: &str| has_namespace(sub, entries.iter().copied());
        assert!(has("p"));
        assert!(
            !has("q"),
            "a file directly under assets/ is not a namespace"
        );
        assert!(has("r"));
        assert!(!has("s"), "an invalid namespace is skipped");
        assert!(has("t"), "an explicit empty directory entry is a folder");
        assert!(!has("missing"));
    }

    #[test]
    fn only_plain_relative_sub_paths() {
        for ok in ["resourcepacks/x", "packs/a.b/c"] {
            assert!(plain_relative(ok), "{ok}");
        }
        for counted in ["", "/abs", "a//b", "a/./b", "a/../b", "a/", "a\\b"] {
            assert!(!plain_relative(counted), "{counted}");
        }
    }
}
