use crate::{MethodRef, NativeError, Value};
use std::collections::BTreeMap;
const OWNER:&str="net/minecraft/world/phys/Vec2";
#[derive(Debug, Clone, Copy)] struct V{ x:f64,y:f64 }
fn row(n:&str)->Option<V>{Some(match n{"ZERO"=>V{x:0.0,y:0.0},"ONE"=>V{x:1.0,y:1.0},"UNIT_X"=>V{x:1.0,y:0.0},"NEG_UNIT_X"=>V{x:-1.0,y:0.0},"UNIT_Y"=>V{x:0.0,y:1.0},"NEG_UNIT_Y"=>V{x:0.0,y:-1.0},"MAX"=>V{x:f32::MAX as f64,y:f32::MAX as f64},"MIN"=>V{x:f32::MIN_POSITIVE as f64,y:f32::MIN_POSITIVE as f64},_=>return None})}
#[derive(Debug, Clone, Default)] pub(crate) struct State{pub handles:BTreeMap<String,(Value,V)>}
pub(crate) fn static_field(s:&mut State,r:&MethodRef)->Option<Result<Value,NativeError>>{if r.owner!=OWNER||r.descriptor.as_str()!=format!("L{OWNER};"){return None}let v=row(&r.name)?;let h=Value::Ref(Some(format!("{OWNER}#{}",r.name)));s.handles.insert(r.name.clone(),(h.clone(),v));Some(Ok(h))}
pub(crate) fn instance_call(s:&mut State,r:&MethodRef,recv:&Value,_args:&[Value])->Option<Result<Value,NativeError>>{let (_,v)=s.handles.values().find(|(h,_)|h==recv)?;match r.name.as_str(){"x"=>Some(Ok(Value::Float(v.x))),"y"=>Some(Ok(Value::Float(v.y))),_=>None}}
