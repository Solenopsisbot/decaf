//! Exact pure construction for Minecraft's client model-layer builders.
//!
//! The handles in this module carry only deterministic builder data. They do not
//! invoke a renderer, read game state, or call a resource manager.

use std::collections::BTreeMap;

use crate::runtime::host_ref::{HostKind, HostRef, Methods, Nested, instance_of_class};
use crate::runtime::{MethodRef, NativeError, Value};

pub(crate) const CUBE_DEFORMATION: &str =
    "net/minecraft/client/model/geom/builders/CubeDeformation";
pub(crate) const PART_POSE: &str = "net/minecraft/client/model/geom/PartPose";
pub(crate) const CUBE_LIST_BUILDER: &str =
    "net/minecraft/client/model/geom/builders/CubeListBuilder";
pub(crate) const MESH_DEFINITION: &str = "net/minecraft/client/model/geom/builders/MeshDefinition";
pub(crate) const PART_DEFINITION: &str = "net/minecraft/client/model/geom/builders/PartDefinition";
pub(crate) const LAYER_DEFINITION: &str =
    "net/minecraft/client/model/geom/builders/LayerDefinition";

#[derive(Clone, Debug, PartialEq)]
enum VisibleFaces {
    All,
    Set(Value),
}

#[derive(Clone, Debug, PartialEq)]
struct Cube {
    name: Option<String>,
    origin: [f32; 3],
    size: [f32; 3],
    grow: [f32; 3],
    tex: [i32; 2],
    mirror: bool,
    tex_scale: [f32; 2],
    visible: VisibleFaces,
}

#[derive(Clone, Debug, PartialEq)]
struct Builder {
    cubes: Vec<Cube>,
    tex: [i32; 2],
    mirror: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Node {
    cube_source: String,
    pose: [f32; 9],
    children: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
struct Mesh {
    root: String,
    nodes: BTreeMap<String, Node>,
}

#[derive(Clone, Debug, PartialEq)]
struct Layer {
    mesh: String,
    width: i32,
    height: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct State {
    next: u64,
    builders: BTreeMap<String, Builder>,
    meshes: BTreeMap<String, Mesh>,
    layers: BTreeMap<String, Layer>,
    deformations: BTreeMap<String, [f32; 3]>,
    poses: BTreeMap<String, [f32; 9]>,
    static_values: BTreeMap<String, Value>,
}

impl State {
    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        for builder in self.builders.values() {
            for cube in &builder.cubes {
                if let VisibleFaces::Set(value) = &cube.visible {
                    crate::runtime::heap::visit_value_texts(value, |t| out.push(t));
                }
            }
        }
        for (key, value) in &self.static_values {
            out.push(key.as_str());
            crate::runtime::heap::visit_value_texts(value, |t| out.push(t));
        }
    }
}

fn value_f32(value: &Value) -> Option<f32> {
    match value {
        Value::Float(v) => Some(*v as f32),
        Value::Int(v) => Some(*v as f32),
        Value::Long(v) => Some(*v as f32),
        _ => None,
    }
}

fn floats<const N: usize>(args: &[Value]) -> Option<[f32; N]> {
    args.iter()
        .take(N)
        .map(value_f32)
        .collect::<Option<Vec<_>>>()?
        .try_into()
        .ok()
}

fn int(value: &Value) -> Option<i32> {
    match value {
        Value::Int(v) => Some(*v),
        Value::Long(v) => i32::try_from(*v).ok(),
        _ => None,
    }
}

fn text(value: &Value) -> Option<String> {
    crate::runtime::expect_ref(value).cloned()
}

/// A new builder, layer or part's key, `<prefix>/<n>`, `n` from a counter that only grows.
///
/// `/`, not `#` (#669): these are handles of their own kinds, numbered by this model, and a
/// `<text>#<digits>` key spells a jar object's allocation key, whose number the collector reads
/// as the heap's (`heap::note_numbers`) and `jar_object::stored_value` reads back as a jar
/// object. `None` (the call unanswered) once the counter is spent, never one key for two.
fn next_key(state: &mut State, prefix: &str) -> Option<String> {
    let key = format!("{prefix}/{}", state.next);
    state.next = state.next.checked_add(1)?;
    Some(key)
}

fn handle(kind: HostKind, key: String) -> Value {
    Value::Host(HostRef::new(kind, key))
}

fn kind(value: &Value, expected: HostKind) -> Option<&HostRef> {
    match value {
        Value::Host(value) if value.kind() == expected => Some(value),
        _ => None,
    }
}

fn values(handle: &HostRef) -> Option<Vec<f32>> {
    handle
        .values()?
        .iter()
        .map(value_f32)
        .collect::<Option<Vec<_>>>()
}

fn deformation_values(state: &State, value: &Value) -> Option<[f32; 3]> {
    let handle = kind(value, HostKind::CubeDeformation)?;
    if let Some(values) = state.deformations.get(handle.key()) {
        return Some(*values);
    }
    let values = values(handle)?;
    values.as_slice().try_into().ok()
}

fn pose_values(state: &State, value: &Value) -> Option<[f32; 9]> {
    let handle = kind(value, HostKind::PartPose)?;
    if let Some(values) = state.poses.get(handle.key()) {
        return Some(*values);
    }
    let values = values(handle)?;
    values.as_slice().try_into().ok()
}

fn deformation(values: [f32; 3]) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::CubeDeformation,
        &values.map(|value| Value::Float(value as f64)),
    ))
}

fn pose(values: [f32; 9]) -> Value {
    Value::Host(HostRef::of_values(
        HostKind::PartPose,
        &values.map(|value| Value::Float(value as f64)),
    ))
}

fn zero_pose() -> [f32; 9] {
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0]
}

pub(crate) fn new_object(class: &str, key: &str) -> Option<Value> {
    let kind = match class {
        CUBE_DEFORMATION => HostKind::CubeDeformation,
        PART_POSE => HostKind::PartPose,
        CUBE_LIST_BUILDER => HostKind::CubeListBuilder,
        MESH_DEFINITION => HostKind::MeshDefinition,
        _ => return None,
    };
    Some(handle(kind, key.to_owned()))
}

pub(crate) fn static_field(state: &mut State, r: &MethodRef) -> Option<Value> {
    let key = format!("{}#{}:{}", r.owner, r.name, r.descriptor);
    if let Some(value) = state.static_values.get(&key) {
        return Some(value.clone());
    }
    let value = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        (
            CUBE_DEFORMATION,
            "NONE",
            "Lnet/minecraft/client/model/geom/builders/CubeDeformation;",
        ) => deformation([0.0, 0.0, 0.0]),
        (PART_POSE, "ZERO", "Lnet/minecraft/client/model/geom/PartPose;") => pose(zero_pose()),
        _ => return None,
    };
    state.static_values.insert(key, value.clone());
    Some(value)
}

pub(crate) fn invoke(
    state: &mut State,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let value = match (r.owner.as_str(), r.name.as_str()) {
        (CUBE_LIST_BUILDER, "create") if r.descriptor == format!("()L{CUBE_LIST_BUILDER};") => {
            let key = next_key(state, "cube-builder")?;
            state.builders.insert(
                key.clone(),
                Builder {
                    cubes: Vec::new(),
                    tex: [0, 0],
                    mirror: false,
                },
            );
            handle(HostKind::CubeListBuilder, key)
        }
        (PART_POSE, "offset")
            if r.descriptor == "(FFF)Lnet/minecraft/client/model/geom/PartPose;" =>
        {
            let [x, y, z] = floats(args)?;
            pose([x, y, z, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0])
        }
        (PART_POSE, "rotation")
            if r.descriptor == "(FFF)Lnet/minecraft/client/model/geom/PartPose;" =>
        {
            let [x, y, z] = floats(args)?;
            pose([0.0, 0.0, 0.0, x, y, z, 1.0, 1.0, 1.0])
        }
        (PART_POSE, "offsetAndRotation")
            if r.descriptor == "(FFFFFF)Lnet/minecraft/client/model/geom/PartPose;" =>
        {
            let [x, y, z, xr, yr, zr] = floats(args)?;
            pose([x, y, z, xr, yr, zr, 1.0, 1.0, 1.0])
        }
        (CUBE_DEFORMATION, "extend") => return None,
        (LAYER_DEFINITION, "create")
            if r.descriptor
                == "(Lnet/minecraft/client/model/geom/builders/MeshDefinition;II)Lnet/minecraft/client/model/geom/builders/LayerDefinition;" =>
        {
            let mesh = kind(args.first()?, HostKind::MeshDefinition)?;
            let width = int(args.get(1)?)?;
            let height = int(args.get(2)?)?;
            let key = next_key(state, "layer-definition")?;
            state.layers.insert(
                key.clone(),
                Layer {
                    mesh: mesh.key().to_owned(),
                    width,
                    height,
                },
            );
            handle(HostKind::LayerDefinition, key)
        }
        _ => return None,
    };
    Some(Ok(value))
}

fn add_box(builder: &mut Builder, descriptor: &str, args: &[Value], state: &State) -> Option<()> {
    let string_name = descriptor.starts_with("(Ljava/lang/String;");
    let base = usize::from(string_name);
    let name = string_name.then(|| text(args.first()?)).flatten();
    let origin = [
        value_f32(args.get(base)?)?,
        value_f32(args.get(base + 1)?)?,
        value_f32(args.get(base + 2)?)?,
    ];
    let size = [
        value_f32(args.get(base + 3)?)?,
        value_f32(args.get(base + 4)?)?,
        value_f32(args.get(base + 5)?)?,
    ];
    let has_set = descriptor.contains("Ljava/util/Set;");
    let has_boolean = descriptor
        .ends_with(")Lnet/minecraft/client/model/geom/builders/CubeListBuilder;")
        && descriptor.contains("Z)");
    let has_deformation = descriptor.contains("CubeDeformation;");
    let grow = if has_deformation {
        deformation_values(state, args.get(base + 6)?)?
    } else {
        [0.0; 3]
    };
    let mirror = if has_boolean {
        matches!(args.get(base + 6), Some(Value::Int(value)) if *value != 0)
    } else {
        builder.mirror
    };
    let visible = if has_set {
        VisibleFaces::Set(args.get(base + 6)?.clone())
    } else {
        VisibleFaces::All
    };
    let tex_scale = if has_deformation && !string_name && args.len() == 9 {
        [value_f32(args.get(7)?)?, value_f32(args.get(8)?)?]
    } else {
        [1.0, 1.0]
    };
    let tex = if string_name && !has_deformation && args.len() == 9 {
        [int(args.get(7)?)?, int(args.get(8)?)?]
    } else if string_name && has_deformation && args.len() == 10 {
        [int(args.get(8)?)?, int(args.get(9)?)?]
    } else {
        builder.tex
    };
    builder.cubes.push(Cube {
        name,
        origin,
        size,
        grow,
        tex,
        mirror,
        tex_scale,
        visible,
    });
    Some(())
}

pub(crate) fn instance(
    state: &mut State,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == CUBE_DEFORMATION {
        let handle = kind(receiver, HostKind::CubeDeformation)?;
        return match (r.name.as_str(), r.descriptor.as_str()) {
            ("<init>", "(F)V") => {
                state
                    .deformations
                    .insert(handle.key().to_owned(), [value_f32(args.first()?)?; 3]);
                Some(Ok(Value::Void))
            }
            ("<init>", "(FFF)V") => {
                state
                    .deformations
                    .insert(handle.key().to_owned(), floats(args)?);
                Some(Ok(Value::Void))
            }
            ("extend", "(F)Lnet/minecraft/client/model/geom/builders/CubeDeformation;") => {
                let old = deformation_values(state, receiver)?;
                let by = value_f32(args.first()?)?;
                Some(Ok(deformation(old.map(|value| value + by))))
            }
            ("extend", "(FFF)Lnet/minecraft/client/model/geom/builders/CubeDeformation;") => {
                let old = deformation_values(state, receiver)?;
                let by = floats::<3>(args)?;
                Some(Ok(deformation(std::array::from_fn(|i| old[i] + by[i]))))
            }
            _ => None,
        };
    }
    if r.owner == PART_POSE {
        let handle = kind(receiver, HostKind::PartPose)?;
        return match (r.name.as_str(), r.descriptor.as_str()) {
            ("<init>", "(FFFFFFFFF)V") => {
                state.poses.insert(handle.key().to_owned(), floats(args)?);
                Some(Ok(Value::Void))
            }
            ("translated", "(FFF)Lnet/minecraft/client/model/geom/PartPose;") => {
                let old = pose_values(state, receiver)?;
                let [x, y, z] = floats(args)?;
                Some(Ok(pose([
                    old[0] + x,
                    old[1] + y,
                    old[2] + z,
                    old[3],
                    old[4],
                    old[5],
                    old[6],
                    old[7],
                    old[8],
                ])))
            }
            ("withScale", "(F)Lnet/minecraft/client/model/geom/PartPose;") => {
                let old = pose_values(state, receiver)?;
                let scale = value_f32(args.first()?)?;
                Some(Ok(pose([
                    old[0], old[1], old[2], old[3], old[4], old[5], scale, scale, scale,
                ])))
            }
            ("x", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[0] as f64))),
            ("y", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[1] as f64))),
            ("z", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[2] as f64))),
            ("xRot", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[3] as f64))),
            ("yRot", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[4] as f64))),
            ("zRot", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[5] as f64))),
            ("xScale", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[6] as f64))),
            ("yScale", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[7] as f64))),
            ("zScale", "()F") => Some(Ok(Value::Float(pose_values(state, receiver)?[8] as f64))),
            _ => None,
        };
    }
    if let Some(handle) = kind(receiver, HostKind::CubeListBuilder) {
        let key = handle.key().to_owned();
        let result = match r.name.as_str() {
            "texOffs" if args.len() == 2 => {
                let tex = [int(args.first()?)?, int(args.get(1)?)?];
                state.builders.get_mut(&key)?.tex = tex;
                Some(Ok(receiver.clone()))
            }
            "mirror" if args.is_empty() => {
                state.builders.get_mut(&key)?.mirror = true;
                Some(Ok(receiver.clone()))
            }
            "mirror" if args.len() == 1 => {
                let mirror = matches!(args.first(), Some(Value::Int(value)) if *value != 0);
                state.builders.get_mut(&key)?.mirror = mirror;
                Some(Ok(receiver.clone()))
            }
            "addBox" => {
                let mut builder = state.builders.remove(&key)?;
                let result = add_box(&mut builder, r.descriptor.as_str(), args, state)
                    .map(|_| Ok(receiver.clone()));
                state.builders.insert(key, builder);
                result
            }
            "getCubes" => None,
            _ => None,
        };
        return result;
    }
    if r.owner == MESH_DEFINITION && r.name == "<init>" && r.descriptor == "()V" {
        let mesh_handle = kind(receiver, HostKind::MeshDefinition)?;
        ensure_mesh(state, mesh_handle.key());
        return Some(Ok(Value::Void));
    }
    if let Some(mesh_handle) = kind(receiver, HostKind::MeshDefinition) {
        let mesh = state.meshes.get(mesh_handle.key())?;
        if r.name == "getRoot" && args.is_empty() {
            return Some(Ok(handle(
                HostKind::PartDefinition,
                format!("{}|{}", mesh_handle.key(), mesh.root),
            )));
        }
    }
    if let Some(part_handle) = kind(receiver, HostKind::PartDefinition) {
        let (mesh_key, node_key) = part_handle.key().split_once('|')?;
        if r.name == "getChild" && args.len() == 1 {
            let name = text(args.first()?)?;
            let child = state
                .meshes
                .get(mesh_key)?
                .nodes
                .get(node_key)?
                .children
                .iter()
                .find(|(child_name, _)| child_name == &name)
                .map(|(_, child)| handle(HostKind::PartDefinition, child.clone()))
                .unwrap_or(Value::Ref(None));
            return Some(Ok(child));
        }
        if r.name == "addOrReplaceChild" && args.len() == 2 {
            let name = text(args.first()?)?;
            let child = kind(args.get(1)?, HostKind::PartDefinition)?;
            let (child_mesh, child_node) = child.key().split_once('|')?;
            if child_mesh != mesh_key {
                return None;
            }
            let old = state
                .meshes
                .get(mesh_key)?
                .nodes
                .get(node_key)?
                .children
                .iter()
                .find(|(child_name, _)| child_name == &name)
                .map(|(_, child)| child.clone());
            let old_children = old
                .as_ref()
                .and_then(|key| state.meshes.get(mesh_key)?.nodes.get(key))
                .map(|node| node.children.clone())
                .unwrap_or_default();
            let mesh = state.meshes.get_mut(mesh_key)?;
            let node = mesh.nodes.get_mut(child_node)?;
            for (old_name, old_key) in old_children {
                if let Some(entry) = node
                    .children
                    .iter_mut()
                    .find(|(child_name, _)| child_name == &old_name)
                {
                    entry.1 = old_key;
                } else {
                    node.children.push((old_name, old_key));
                }
            }
            if let Some(entry) = mesh
                .nodes
                .get_mut(node_key)?
                .children
                .iter_mut()
                .find(|(child_name, _)| child_name == &name)
            {
                entry.1 = child_node.to_owned();
            } else {
                mesh.nodes
                    .get_mut(node_key)?
                    .children
                    .push((name, child_node.to_owned()));
            }
            return Some(Ok(handle(HostKind::PartDefinition, child.key().to_owned())));
        }
        if r.name == "addOrReplaceChild" && args.len() == 3 {
            let name = text(args.first()?)?;
            let builder_key = kind(args.get(1)?, HostKind::CubeListBuilder)?
                .key()
                .to_owned();
            let pose = pose_values(state, args.get(2)?)?;
            if !state.builders.contains_key(&builder_key) {
                return None;
            }
            let old = state
                .meshes
                .get(mesh_key)?
                .nodes
                .get(node_key)?
                .children
                .iter()
                .find(|(child_name, _)| child_name == &name)
                .map(|(_, child)| child.clone());
            let inherited = old
                .as_ref()
                .and_then(|key| state.meshes.get(mesh_key)?.nodes.get(key))
                .map(|node| node.children.clone())
                .unwrap_or_default();
            let child = next_key(state, "part")?;
            let mesh = state.meshes.get_mut(mesh_key)?;
            if let Some(entry) = mesh
                .nodes
                .get_mut(node_key)?
                .children
                .iter_mut()
                .find(|(child_name, _)| child_name == &name)
            {
                entry.1 = child.clone();
            } else {
                mesh.nodes
                    .get_mut(node_key)?
                    .children
                    .push((name, child.clone()));
            }
            mesh.nodes.insert(
                child.clone(),
                Node {
                    cube_source: builder_key,
                    pose,
                    children: inherited,
                },
            );
            return Some(Ok(handle(
                HostKind::PartDefinition,
                format!("{mesh_key}|{child}"),
            )));
        }
    }
    None
}

pub(crate) fn ensure_mesh(state: &mut State, key: &str) {
    let root = "root".to_owned();
    state.meshes.entry(key.to_owned()).or_insert_with(|| {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            root.clone(),
            Node {
                cube_source: String::new(),
                pose: zero_pose(),
                children: Vec::new(),
            },
        );
        Mesh { root, nodes }
    });
}

fn record_float(value: f32) -> i32 {
    value.to_bits() as i32
}

fn pose_hash(values: [f32; 9]) -> i32 {
    values.into_iter().fold(1i32, |hash, value| {
        hash.wrapping_mul(31).wrapping_add(record_float(value))
    })
}

fn pose_text(handle: &HostRef, _nested: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    let values = handle
        .values()
        .and_then(|values| values.iter().map(value_f32).collect::<Option<Vec<_>>>())
        .and_then(|values| values.try_into().ok());
    let Some(values): Option<[f32; 9]> = values else {
        return Ok(None);
    };
    let names = [
        "x", "y", "z", "xRot", "yRot", "zRot", "xScale", "yScale", "zScale",
    ];
    Ok(Some(format!(
        "PartPose[{}]",
        names
            .into_iter()
            .zip(values)
            .map(|(name, value)| format!(
                "{name}={}",
                crate::runtime::jdk_float_text::float_to_string(value as f64)
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

pub(crate) const CUBE_DEFORMATION_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| {
        Ok(Some(match other {
            Value::Host(other) => handle.same_object(other) == Some(true),
            _ => false,
        }))
    },
    instance_of: instance_of_class,
    crosses_jars: |_, _| true,
    compare_to: None,
};
pub(crate) const PART_POSE_METHODS: Methods = Methods {
    to_string: pose_text,
    hash_code: |handle, _| {
        Ok(handle
            .values()
            .and_then(|values| values.iter().map(value_f32).collect::<Option<Vec<_>>>())
            .and_then(|v: Vec<f32>| v.try_into().ok())
            .map(|v: [f32; 9]| pose_hash(v)))
    },
    equals: |handle, other, _| {
        let Value::Host(other) = other else {
            return Ok(Some(false));
        };
        Ok(Some(handle.values() == other.values()))
    },
    instance_of: instance_of_class,
    crosses_jars: |_, _| true,
    compare_to: None,
};
pub(crate) const IDENTITY_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| {
        Ok(Some(match other {
            Value::Host(other) => handle.same_object(other) == Some(true),
            _ => false,
        }))
    },
    instance_of: instance_of_class,
    crosses_jars: |_, _| true,
    compare_to: None,
};
