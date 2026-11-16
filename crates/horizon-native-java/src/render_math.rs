//! Exact bounded model of pure PoseStack/JOML render math.
#![allow(dead_code, private_interfaces)]
use crate::{HostKind, HostRef, MethodRef, NativeError, StdHost, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug)]
struct M4([f32; 16]);
#[derive(Clone, Copy, Debug, PartialEq)]
struct M3([f32; 9]);
#[derive(Clone, Copy, Debug)]
struct Q([f32; 4]);
#[derive(Clone, Copy, Debug)]
struct V3([f32; 3]);
#[derive(Clone, Copy, Debug)]
struct Pose {
    p: M4,
    n: M3,
    trusted: bool,
}
#[derive(Clone, Debug)]
pub(crate) enum Obj {
    Stack(Vec<String>),
    Pose(Pose),
    M4(M4),
    M3(M3),
    Q(Q),
    V3(V3),
    V4([f32; 4]),
    Axis(V3),
}
const I4: M4 = M4([
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
]);
const I3: M3 = M3([1., 0., 0., 0., 1., 0., 0., 0., 1.]);
pub(crate) fn is_class(c: &str) -> bool {
    matches!(
        c,
        "com/mojang/blaze3d/vertex/PoseStack"
            | "com/mojang/blaze3d/vertex/PoseStack$Pose"
            | "org/joml/Matrix4f"
            | "org/joml/Matrix3f"
            | "org/joml/Quaternionf"
            | "org/joml/Vector3f"
            | "org/joml/Vector4f"
    )
}
pub(crate) fn new_value(_: &str, key: &str) -> Value {
    Value::Host(HostRef::new(HostKind::RenderObject, key.to_owned()))
}
pub(crate) fn class_of_key(key: &str) -> &str {
    key.rsplit_once('#').map_or(key, |(c, _)| c)
}
fn key(v: &Value) -> Option<&str> {
    match v {
        Value::Host(h) if h.kind() == HostKind::RenderObject => Some(h.key()),
        _ => None,
    }
}
pub(crate) fn reachable_keys<'a>(
    map: &BTreeMap<String, Obj>,
    roots: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut live: BTreeSet<String> = roots.into_iter().map(str::to_owned).collect();
    let mut pending: Vec<String> = live.iter().cloned().collect();
    while let Some(key) = pending.pop() {
        if let Some(Obj::Stack(slots)) = map.get(&key) {
            for slot in slots {
                if live.insert(slot.clone()) {
                    pending.push(slot.clone());
                }
            }
        }
    }
    live
}
fn f(v: &Value) -> Option<f32> {
    match v {
        Value::Float(x) => Some(*x as f32),
        Value::Int(x) => Some(*x as f32),
        Value::Long(x) => Some(*x as f32),
        _ => None,
    }
}
fn arg(r: &MethodRef, a: &[Value], i: usize) -> Option<f32> {
    a.get(i).and_then(f).or_else(|| {
        let _ = r;
        None
    })
}
fn h(k: &str) -> Value {
    new_value("", k)
}
fn mul4(a: M4, b: M4) -> M4 {
    let mut o = [0.; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a.0[k * 4 + r] * b.0[c * 4 + k]).sum();
        }
    }
    M4(o)
}
fn mul3(a: M3, b: M3) -> M3 {
    let mut o = [0.; 9];
    for c in 0..3 {
        for r in 0..3 {
            o[c * 3 + r] = (0..3).map(|k| a.0[k * 3 + r] * b.0[c * 3 + k]).sum();
        }
    }
    M3(o)
}
fn trans(x: f32, y: f32, z: f32) -> M4 {
    M4([1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., x, y, z, 1.])
}
fn scale(x: f32, y: f32, z: f32) -> M4 {
    M4([x, 0., 0., 0., 0., y, 0., 0., 0., 0., z, 0., 0., 0., 0., 1.])
}
const PROPERTY_BITS_REASON: &str =
    "depends on JOML property bits; exact path via real JOML bytecode planned";

fn counted(host: &mut StdHost, r: &MethodRef, reason: &str) -> Result<Value, NativeError> {
    let _ = reason;
    host.unknown_host_call(r)
}

fn axis_vec(k: &str) -> Option<V3> {
    Some(if k.ends_with("#XP") {
        V3([1., 0., 0.])
    } else if k.ends_with("#XN") {
        V3([-1., 0., 0.])
    } else if k.ends_with("#YP") {
        V3([0., 1., 0.])
    } else if k.ends_with("#YN") {
        V3([0., -1., 0.])
    } else if k.ends_with("#ZP") {
        V3([0., 0., 1.])
    } else if k.ends_with("#ZN") {
        V3([0., 0., -1.])
    } else {
        return None;
    })
}
fn copy_pose(map: &mut BTreeMap<String, Obj>, src: &str, dst: &str) {
    if let Some(p) = map.get(src).and_then(|o| match o {
        Obj::Pose(p) => Some(*p),
        _ => None,
    }) {
        map.insert(dst.into(), Obj::Pose(p));
    }
}
pub(crate) fn construct(
    map: &mut BTreeMap<String, Obj>,
    receiver: &Value,
    class: &str,
    args: &[Value],
) {
    let Some(k) = key(receiver) else { return };
    if matches!(class, "org/joml/Quaternionf")
        && (args.len() != 4 || args.iter().any(|v| f(v).is_none()))
    {
        return;
    }
    if matches!(class, "org/joml/Vector3f")
        && (args.len() != 3 || args.iter().any(|v| f(v).is_none()))
    {
        return;
    }
    if matches!(class, "org/joml/Vector4f")
        && (args.len() != 4 || args.iter().any(|v| f(v).is_none()))
    {
        return;
    }
    let o = match class {
        "com/mojang/blaze3d/vertex/PoseStack" => {
            let pk = format!("{k}/pose/0");
            map.insert(
                pk.clone(),
                Obj::Pose(Pose {
                    p: I4,
                    n: I3,
                    trusted: true,
                }),
            );
            Obj::Stack(vec![pk])
        }
        "com/mojang/blaze3d/vertex/PoseStack$Pose" => Obj::Pose(Pose {
            p: I4,
            n: I3,
            trusted: true,
        }),
        "org/joml/Matrix4f" => Obj::M4(I4),
        "org/joml/Matrix3f" => Obj::M3(I3),
        "org/joml/Quaternionf" => Obj::Q(if args.len() >= 4 {
            Q([
                f(&args[0]).unwrap_or(0.),
                f(&args[1]).unwrap_or(0.),
                f(&args[2]).unwrap_or(0.),
                f(&args[3]).unwrap_or(1.),
            ])
        } else {
            Q([0., 0., 0., 1.])
        }),
        "org/joml/Vector3f" => Obj::V3(V3([
            args.first().and_then(f).unwrap_or(0.),
            args.get(1).and_then(f).unwrap_or(0.),
            args.get(2).and_then(f).unwrap_or(0.),
        ])),
        "org/joml/Vector4f" => Obj::V4([
            args.first().and_then(f).unwrap_or(0.),
            args.get(1).and_then(f).unwrap_or(0.),
            args.get(2).and_then(f).unwrap_or(0.),
            args.get(3).and_then(f).unwrap_or(1.),
        ]),
        _ => return,
    };
    map.insert(k.into(), o);
}
pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    if r.owner == "com/mojang/math/Axis"
        && r.descriptor == "Lcom/mojang/math/Axis;"
        && matches!(r.name.as_str(), "XP" | "XN" | "YP" | "YN" | "ZP" | "ZN")
    {
        Some(h(&format!("com/mojang/math/Axis#{}", r.name)))
    } else {
        None
    }
}
pub(crate) fn call(
    host: &mut StdHost,
    map: &mut BTreeMap<String, Obj>,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let k = key(receiver)?.to_owned();
    if !host.owns_render_handle(&k) {
        return Some(host.unknown_host_call(r));
    }
    if r.name == "<init>" {
        if !supported_constructor(&r.owner, &r.descriptor) || class_of_key(&k) != r.owner {
            return Some(host.unknown_host_call(r));
        }
        construct(map, receiver, &r.owner, args);
        if r.owner == "com/mojang/blaze3d/vertex/PoseStack" {
            host.register_render_handle(&format!("{k}/pose/0"));
        }
        return Some(Ok(Value::Void));
    }
    if r.owner == "com/mojang/blaze3d/vertex/PoseStack" {
        stack(host, map, &k, r, args)
    } else if r.owner == "com/mojang/blaze3d/vertex/PoseStack$Pose" {
        pose(host, map, &k, r, args)
    } else if r.owner == "com/mojang/math/Axis" {
        axis_call(map, &k, r, args)
    } else if r.owner.starts_with("org/joml/") {
        joml(host, map, &k, r, args)
    } else {
        None
    }
}
fn sync_pose_aliases(map: &mut BTreeMap<String, Obj>, k: &str) {
    let pose = match map.get(k) {
        Some(Obj::Pose(p)) => *p,
        _ => return,
    };
    let m4 = format!("{k}/matrix4");
    if let Some(Obj::M4(m)) = map.get_mut(&m4) {
        *m = pose.p;
    }
    let m3 = format!("{k}/matrix3");
    if let Some(Obj::M3(m)) = map.get_mut(&m3) {
        *m = pose.n;
    }
}
fn sync_matrix_alias(map: &mut BTreeMap<String, Obj>, k: &str) {
    if let Some(pose_key) = k.strip_suffix("/matrix4")
        && let Some(m) = map.get(k).and_then(|o| match o {
            Obj::M4(m) => Some(*m),
            _ => None,
        })
        && let Some(Obj::Pose(p)) = map.get_mut(pose_key)
    {
        p.p = m;
    } else if let Some(pose_key) = k.strip_suffix("/matrix3")
        && let Some(m) = map.get(k).and_then(|o| match o {
            Obj::M3(m) => Some(*m),
            _ => None,
        })
        && let Some(Obj::Pose(p)) = map.get_mut(pose_key)
    {
        p.n = m;
    }
}

fn pose_translate(map: &mut BTreeMap<String, Obj>, k: &str, x: f32, y: f32, z: f32) {
    if let Some(Obj::Pose(p)) = map.get_mut(k) {
        p.p = mul4(p.p, trans(x, y, z));
    }
    sync_pose_aliases(map, k);
}
fn pose_scale(map: &mut BTreeMap<String, Obj>, k: &str, x: f32, y: f32, z: f32) {
    if let Some(Obj::Pose(p)) = map.get_mut(k) {
        p.p = mul4(p.p, scale(x, y, z));
        if x.abs() == y.abs() && y.abs() == z.abs() {
            if x < 0. || y < 0. || z < 0. {
                p.n = mul3(
                    p.n,
                    M3([x.signum(), 0., 0., 0., y.signum(), 0., 0., 0., z.signum()]),
                );
            }
        } else {
            p.n = mul3(p.n, M3([1. / x, 0., 0., 0., 1. / y, 0., 0., 0., 1. / z]));
            p.trusted = false;
        }
    }
    sync_pose_aliases(map, k);
}

fn stack(
    host: &mut StdHost,
    map: &mut BTreeMap<String, Obj>,
    k: &str,
    r: &MethodRef,
    a: &[Value],
) -> Option<Result<Value, NativeError>> {
    let s = match map.get(k) {
        Some(Obj::Stack(s)) => s.clone(),
        _ => return None,
    };
    match r.name.as_str() {
        "pushPose" if r.descriptor == "()V" => {
            let nk = format!("{k}/pose/{}", s.len());
            host.register_render_handle(&nk);
            copy_pose(map, s.last()?, &nk);
            if let Some(Obj::Stack(v)) = map.get_mut(k) {
                v.push(nk)
            }
            Some(Ok(Value::Void))
        }
        "popPose" if r.descriptor == "()V" => {
            if s.len() <= 1 {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/util/NoSuchElementException".into(),
                    message: None,
                    frame: None,
                }));
            }
            if let Some(Obj::Stack(v)) = map.get_mut(k) {
                v.pop();
            }
            Some(Ok(Value::Void))
        }
        "last" if r.descriptor == "()Lcom/mojang/blaze3d/vertex/PoseStack$Pose;" => {
            Some(Ok(h(s.last()?)))
        }
        "isEmpty" if r.descriptor == "()Z" => Some(Ok(Value::Int(i32::from(s.len() == 1)))),
        "setIdentity" if r.descriptor == "()V" => {
            if let Some(x) = s.last() {
                map.insert(
                    x.clone(),
                    Obj::Pose(Pose {
                        p: I4,
                        n: I3,
                        trusted: true,
                    }),
                );
                sync_pose_aliases(map, x);
            }
            Some(Ok(Value::Void))
        }
        "translate" if r.descriptor == "(FFF)V" => {
            pose_translate(map, s.last()?, arg(r, a, 0)?, arg(r, a, 1)?, arg(r, a, 2)?);
            Some(Ok(Value::Void))
        }
        "scale" if r.descriptor == "(FFF)V" => Some(counted(host, r, PROPERTY_BITS_REASON)),
        "rotateDegrees" | "rotate" | "rotateAround" => Some(host.unknown_host_call(r)),
        "mulPose" => Some(counted(host, r, PROPERTY_BITS_REASON)),
        _ => None,
    }
}
fn pose(
    host: &mut StdHost,
    map: &mut BTreeMap<String, Obj>,
    k: &str,
    r: &MethodRef,
    a: &[Value],
) -> Option<Result<Value, NativeError>> {
    let p = match map.get(k) {
        Some(Obj::Pose(p)) => *p,
        _ => return None,
    };
    match r.name.as_str() {
        "pose" if r.descriptor == "()Lorg/joml/Matrix4f;" => {
            let nk = format!("{k}/matrix4");
            host.register_render_handle(&nk);
            map.entry(nk.clone()).or_insert(Obj::M4(p.p));
            Some(Ok(h(&nk)))
        }
        "normal" if r.descriptor == "()Lorg/joml/Matrix3f;" => {
            let nk = format!("{k}/matrix3");
            host.register_render_handle(&nk);
            map.entry(nk.clone()).or_insert(Obj::M3(p.n));
            Some(Ok(h(&nk)))
        }
        "transformNormal"
            if r.descriptor == "(Lorg/joml/Vector3fc;Lorg/joml/Vector3f;)Lorg/joml/Vector3f;" =>
        {
            if p.n != I3 || !p.trusted {
                return Some(counted(host, r, PROPERTY_BITS_REASON));
            }
            let src = key(a.first()?)?.to_owned();
            let dst = key(a.get(1)?)?.to_owned();
            if !host.owns_render_handle(&src) || !host.owns_render_handle(&dst) {
                return None;
            }
            let v = match map.get(&src) {
                Some(Obj::V3(v)) => *v,
                _ => return None,
            };
            let Some(Obj::V3(out)) = map.get_mut(&dst) else {
                return None;
            };
            out.0 = v.0;
            Some(Ok(h(&dst)))
        }
        "transformNormal" if r.descriptor == "(FFFLorg/joml/Vector3f;)Lorg/joml/Vector3f;" => {
            if p.n != I3 || !p.trusted {
                return Some(counted(host, r, PROPERTY_BITS_REASON));
            }
            let dst = key(a.get(3)?)?.to_owned();
            if !host.owns_render_handle(&dst) {
                return None;
            }
            let Some(Obj::V3(out)) = map.get_mut(&dst) else {
                return None;
            };
            out.0 = [arg(r, a, 0)?, arg(r, a, 1)?, arg(r, a, 2)?];
            Some(Ok(h(&dst)))
        }
        "transformPosition"
            if r.descriptor == "(Lorg/joml/Vector3fc;Lorg/joml/Vector3f;)Lorg/joml/Vector3f;" =>
        {
            let src = key(a.first()?)?.to_owned();
            let dst = key(a.get(1)?)?.to_owned();
            if !host.owns_render_handle(&src) || !host.owns_render_handle(&dst) {
                return None;
            }
            let v = match map.get(&src) {
                Some(Obj::V3(v)) => *v,
                _ => return None,
            };
            let Some(Obj::V3(out)) = map.get_mut(&dst) else {
                return None;
            };
            let m = p.p.0;
            out.0 = [
                m[0] * v.0[0] + m[4] * v.0[1] + m[8] * v.0[2] + m[12],
                m[1] * v.0[0] + m[5] * v.0[1] + m[9] * v.0[2] + m[13],
                m[2] * v.0[0] + m[6] * v.0[1] + m[10] * v.0[2] + m[14],
            ];
            Some(Ok(h(&dst)))
        }
        "transformPosition" if r.descriptor == "(FFFLorg/joml/Vector3f;)Lorg/joml/Vector3f;" => {
            let dst = key(a.get(3)?)?.to_owned();
            if !host.owns_render_handle(&dst) {
                return None;
            }
            let Some(Obj::V3(out)) = map.get_mut(&dst) else {
                return None;
            };
            let m = p.p.0;
            let x = arg(r, a, 0)?;
            let y = arg(r, a, 1)?;
            let z = arg(r, a, 2)?;
            out.0 = [
                m[0] * x + m[4] * y + m[8] * z + m[12],
                m[1] * x + m[5] * y + m[9] * z + m[13],
                m[2] * x + m[6] * y + m[10] * z + m[14],
            ];
            Some(Ok(h(&dst)))
        }
        "copy" if r.descriptor == "()Lcom/mojang/blaze3d/vertex/PoseStack$Pose;" => {
            let nk = format!("{k}/copy");
            host.register_render_handle(&nk);
            map.insert(nk.clone(), Obj::Pose(p));
            Some(Ok(h(&nk)))
        }
        "setIdentity" if r.descriptor == "()V" => {
            map.insert(
                k.into(),
                Obj::Pose(Pose {
                    p: I4,
                    n: I3,
                    trusted: true,
                }),
            );
            sync_pose_aliases(map, k);
            Some(Ok(Value::Void))
        }
        "translate" if r.descriptor == "(FFF)V" => {
            pose_translate(map, k, arg(r, a, 0)?, arg(r, a, 1)?, arg(r, a, 2)?);
            Some(Ok(Value::Void))
        }
        "scale" if r.descriptor == "(FFF)V" => Some(counted(host, r, PROPERTY_BITS_REASON)),
        "rotateDegrees" | "rotate" | "rotateAround" => Some(host.unknown_host_call(r)),
        _ => None,
    }
}
fn axis_call(
    _map: &mut BTreeMap<String, Obj>,
    _k: &str,
    _r: &MethodRef,
    _a: &[Value],
) -> Option<Result<Value, NativeError>> {
    None
}

fn joml(
    host: &mut StdHost,
    map: &mut BTreeMap<String, Obj>,
    k: &str,
    r: &MethodRef,
    a: &[Value],
) -> Option<Result<Value, NativeError>> {
    match (r.owner.as_str(), r.name.as_str()) {
        ("org/joml/Matrix4f", "identity") if r.descriptor == "()Lorg/joml/Matrix4f;" => {
            map.insert(k.into(), Obj::M4(I4));
            Some(Ok(h(k)))
        }
        ("org/joml/Matrix4f", "translate") if r.descriptor == "(FFF)Lorg/joml/Matrix4f;" => {
            let (x, y, z) = (arg(r, a, 0)?, arg(r, a, 1)?, arg(r, a, 2)?);
            if let Some(Obj::M4(m)) = map.get_mut(k) {
                *m = mul4(*m, trans(x, y, z));
            }
            sync_matrix_alias(map, k);
            Some(Ok(h(k)))
        }
        ("org/joml/Matrix4f", "scale") => Some(counted(host, r, PROPERTY_BITS_REASON)),
        ("org/joml/Quaternionf", "mul") => None,
        ("org/joml/Vector3f", "set") if r.descriptor == "(FFF)Lorg/joml/Vector3f;" => {
            let (x, y, z) = (arg(r, a, 0)?, arg(r, a, 1)?, arg(r, a, 2)?);
            if let Some(Obj::V3(v)) = map.get_mut(k) {
                v.0 = [x, y, z];
            }
            Some(Ok(h(k)))
        }
        ("org/joml/Vector4f", "set") => None,
        (owner, name)
            if owner == "org/joml/Matrix4f"
                && r.descriptor == "()F"
                && name.len() == 3
                && name.starts_with("m") =>
        {
            let i = [
                "m00", "m01", "m02", "m03", "m10", "m11", "m12", "m13", "m20", "m21", "m22", "m23",
                "m30", "m31", "m32", "m33",
            ]
            .iter()
            .position(|x| *x == name)?;
            match map.get(k)? {
                Obj::M4(m) => Some(Ok(Value::Float(f64::from(m.0[i])))),
                _ => None,
            }
        }
        _ => None,
    }
}
fn supported_constructor(owner: &str, descriptor: &str) -> bool {
    matches!(
        (owner, descriptor),
        ("com/mojang/blaze3d/vertex/PoseStack", "()V")
            | ("com/mojang/blaze3d/vertex/PoseStack$Pose", "()V")
            | ("org/joml/Matrix4f", "()V")
            | ("org/joml/Matrix3f", "()V")
            | ("org/joml/Quaternionf", "()V")
            | ("org/joml/Quaternionf", "(FFFF)V")
            | ("org/joml/Vector3f", "()V")
            | ("org/joml/Vector3f", "(FFF)V")
            | ("org/joml/Vector4f", "()V")
            | ("org/joml/Vector4f", "(FFFF)V")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    fn r(owner: &str, name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }
    #[test]
    fn forged_render_handles_are_refused_and_counted() {
        let mut map = BTreeMap::new();
        let mut host = StdHost::new();
        let init = r("com/mojang/blaze3d/vertex/PoseStack", "<init>", "()V");
        let forged = new_value("", "com/mojang/blaze3d/vertex/PoseStack#forged");
        let answered = call(&mut host, &mut map, &init, &forged, &[]).unwrap();
        assert!(answered.is_ok());
        assert_eq!(host.unknown_host_call_count(), 1);
        let wrong_class = new_value("", "org/joml/Matrix4f#forged");
        host.register_render_handle("org/joml/Matrix4f#forged");
        let answered = call(&mut host, &mut map, &init, &wrong_class, &[]).unwrap();
        assert!(answered.is_ok());
        assert_eq!(host.unknown_host_call_count(), 2);
        let marker = Value::Ref(Some("com/mojang/blaze3d/vertex/PoseStack#forged".into()));
        assert!(call(&mut host, &mut map, &init, &marker, &[]).is_none());
    }

    #[test]
    fn property_sensitive_render_families_are_counted() {
        let mut map = BTreeMap::new();
        let mut host = StdHost::new();
        let stack = new_value("", "com/mojang/blaze3d/vertex/PoseStack#counted");
        host.register_render_handle("com/mojang/blaze3d/vertex/PoseStack#counted");
        let init = r("com/mojang/blaze3d/vertex/PoseStack", "<init>", "()V");
        call(&mut host, &mut map, &init, &stack, &[])
            .unwrap()
            .unwrap();
        let scale = r("com/mojang/blaze3d/vertex/PoseStack", "scale", "(FFF)V");
        call(
            &mut host,
            &mut map,
            &scale,
            &stack,
            &[Value::Float(2.), Value::Float(3.), Value::Float(4.)],
        )
        .unwrap()
        .unwrap();
        let mul = r(
            "com/mojang/blaze3d/vertex/PoseStack",
            "mulPose",
            "(Lorg/joml/Matrix4fc;)V",
        );
        let matrix = new_value("", "org/joml/Matrix4f#counted");
        host.register_render_handle("org/joml/Matrix4f#counted");
        construct(&mut map, &matrix, "org/joml/Matrix4f", &[]);
        call(&mut host, &mut map, &mul, &stack, &[matrix])
            .unwrap()
            .unwrap();
        assert_eq!(host.unknown_host_call_count(), 2);
    }

    #[test]
    fn ten_thousand_pose_sequences_measure() {
        let mut map = BTreeMap::new();
        let mut host = StdHost::new();
        let stack = new_value("", "com/mojang/blaze3d/vertex/PoseStack#bench");
        let stack_key = key(&stack).unwrap();
        host.register_render_handle(stack_key);
        construct(&mut map, &stack, "com/mojang/blaze3d/vertex/PoseStack", &[]);
        let axis = h("com/mojang/math/Axis#YP");
        let t = r("com/mojang/blaze3d/vertex/PoseStack", "translate", "(FFF)V");
        let sc = r("com/mojang/blaze3d/vertex/PoseStack", "scale", "(FFF)V");
        let rot = r(
            "com/mojang/blaze3d/vertex/PoseStack",
            "rotateDegrees",
            "(Lcom/mojang/math/Axis;F)V",
        );
        let push = r("com/mojang/blaze3d/vertex/PoseStack", "pushPose", "()V");
        let pop = r("com/mojang/blaze3d/vertex/PoseStack", "popPose", "()V");
        let mul = r(
            "com/mojang/blaze3d/vertex/PoseStack",
            "mulPose",
            "(Lorg/joml/Matrix4fc;)V",
        );
        let matrix = new_value("", "org/joml/Matrix4f#bench");
        host.register_render_handle(key(&matrix).unwrap());
        construct(&mut map, &matrix, "org/joml/Matrix4f", &[]);
        let start = Instant::now();
        for _ in 0..10_000 {
            call(&mut host, &mut map, &push, &stack, &[]);
            call(
                &mut host,
                &mut map,
                &t,
                &stack,
                &[Value::Float(1.25), Value::Float(-2.5), Value::Float(3.75)],
            );
            call(
                &mut host,
                &mut map,
                &sc,
                &stack,
                &[Value::Float(-2.0), Value::Float(3.0), Value::Float(4.0)],
            );
            call(
                &mut host,
                &mut map,
                &rot,
                &stack,
                &[axis.clone(), Value::Float(37.0)],
            );
            call(
                &mut host,
                &mut map,
                &mul,
                &stack,
                std::slice::from_ref(&matrix),
            );
            call(&mut host, &mut map, &pop, &stack, &[]);
        }
        eprintln!(
            "render-r0 10k push/translate/rotate/scale/mulPose/pop: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
