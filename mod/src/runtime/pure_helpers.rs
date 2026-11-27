//! Small side-effect-free library helpers whose contracts are independent of host state.

use crate::runtime::{MethodRef, NativeError, StdHost, Value};

const LOG4J_STRINGS: &str = "org/apache/logging/log4j/util/Strings";
const LWJGL_MATH: &str = "org/lwjgl/system/MathUtil";
const ARGB: &str = "net/minecraft/util/ARGB";
const MTH: &str = "net/minecraft/util/Mth";

fn mth_floor_float(value: f32) -> i32 {
    f64::from(value).floor() as i32
}

fn argb_as_8bit_channel(value: f32) -> i32 {
    mth_floor_float(value * 255.0_f32)
}

fn argb_color(a: i32, r: i32, g: i32, b: i32) -> i32 {
    ((a & 0xFF) << 24) | ((r & 0xFF) << 16) | ((g & 0xFF) << 8) | (b & 0xFF)
}

fn argb_alpha(color: i32) -> i32 {
    ((color as u32) >> 24) as i32
}

fn argb_red(color: i32) -> i32 {
    (color >> 16) & 0xFF
}

fn argb_green(color: i32) -> i32 {
    (color >> 8) & 0xFF
}

fn argb_blue(color: i32) -> i32 {
    color & 0xFF
}

fn argb_from_8bit_channel(channel: i32) -> f32 {
    (channel as f32) / 255.0_f32
}

fn argb_static_call(r: &MethodRef, args: &[Value]) -> Option<Result<Value, NativeError>> {
    match (r.name.as_str(), r.descriptor.as_str(), args) {
        ("alpha", "(I)I", [Value::Int(c)]) => Some(Ok(Value::Int(argb_alpha(*c)))),
        ("red", "(I)I", [Value::Int(c)]) => Some(Ok(Value::Int(argb_red(*c)))),
        ("green", "(I)I", [Value::Int(c)]) => Some(Ok(Value::Int(argb_green(*c)))),
        ("blue", "(I)I", [Value::Int(c)]) => Some(Ok(Value::Int(argb_blue(*c)))),
        ("color", "(IIII)I", [Value::Int(a), Value::Int(r), Value::Int(g), Value::Int(b)]) => {
            Some(Ok(Value::Int(argb_color(*a, *r, *g, *b))))
        }
        ("color", "(III)I", [Value::Int(r), Value::Int(g), Value::Int(b)]) => {
            Some(Ok(Value::Int(argb_color(255, *r, *g, *b))))
        }
        ("as8BitChannel", "(F)I", [Value::Float(v)]) => {
            Some(Ok(Value::Int(argb_as_8bit_channel(*v as f32))))
        }
        (
            "colorFromFloat",
            "(FFFF)I",
            [
                Value::Float(a),
                Value::Float(r),
                Value::Float(g),
                Value::Float(b),
            ],
        ) => Some(Ok(Value::Int(argb_color(
            argb_as_8bit_channel(*a as f32),
            argb_as_8bit_channel(*r as f32),
            argb_as_8bit_channel(*g as f32),
            argb_as_8bit_channel(*b as f32),
        )))),
        ("gray", "(F)I", [Value::Float(v)]) => {
            let ch = argb_as_8bit_channel(*v as f32);
            Some(Ok(Value::Int(argb_color(255, ch, ch, ch))))
        }
        ("multiply", "(II)I", [Value::Int(c1), Value::Int(c2)]) => {
            if *c1 == -1 {
                return Some(Ok(Value::Int(*c2)));
            }
            if *c2 == -1 {
                return Some(Ok(Value::Int(*c1)));
            }
            Some(Ok(Value::Int(argb_color(
                argb_alpha(*c1) * argb_alpha(*c2) / 255,
                argb_red(*c1) * argb_red(*c2) / 255,
                argb_green(*c1) * argb_green(*c2) / 255,
                argb_blue(*c1) * argb_blue(*c2) / 255,
            ))))
        }
        ("average", "(II)I", [Value::Int(c1), Value::Int(c2)]) => Some(Ok(Value::Int(argb_color(
            (argb_alpha(*c1) + argb_alpha(*c2)) / 2,
            (argb_red(*c1) + argb_red(*c2)) / 2,
            (argb_green(*c1) + argb_green(*c2)) / 2,
            (argb_blue(*c1) + argb_blue(*c2)) / 2,
        )))),
        ("alphaFloat", "(I)F", [Value::Int(c)]) => Some(Ok(Value::Float(f64::from(
            argb_from_8bit_channel(argb_alpha(*c)),
        )))),
        ("redFloat", "(I)F", [Value::Int(c)]) => Some(Ok(Value::Float(f64::from(
            argb_from_8bit_channel(argb_red(*c)),
        )))),
        ("greenFloat", "(I)F", [Value::Int(c)]) => Some(Ok(Value::Float(f64::from(
            argb_from_8bit_channel(argb_green(*c)),
        )))),
        ("blueFloat", "(I)F", [Value::Int(c)]) => Some(Ok(Value::Float(f64::from(
            argb_from_8bit_channel(argb_blue(*c)),
        )))),
        _ => None,
    }
}

fn mth_static_call(r: &MethodRef, args: &[Value]) -> Option<Result<Value, NativeError>> {
    match (r.name.as_str(), r.descriptor.as_str(), args) {
        ("floor", "(F)I", [Value::Float(v)]) => Some(Ok(Value::Int(mth_floor_float(*v as f32)))),
        ("floor", "(D)I", [Value::Float(v)]) => Some(Ok(Value::Int(v.floor() as i32))),
        ("lfloor", "(D)J", [Value::Float(v)]) => Some(Ok(Value::Long(v.floor() as i64))),
        ("abs", "(F)F", [Value::Float(v)]) => Some(Ok(Value::Float(f64::from((*v as f32).abs())))),
        ("abs", "(I)I", [Value::Int(v)]) => Some(Ok(Value::Int(v.wrapping_abs()))),
        // 26.3 `Mth.positiveCeilDiv(int, int)`: `-Math.floorDiv(-a, b)` (`ineg` wraps).
        ("positiveCeilDiv", "(II)I", [Value::Int(a), Value::Int(b)]) => {
            if *b == 0 {
                return Some(Err(crate::runtime::divide_by_zero()));
            }
            Some(Ok(Value::Int(
                a.wrapping_neg().div_floor_java(*b).wrapping_neg(),
            )))
        }
        // `Mth.positiveCeilDiv(long, long)`: `-Math.floorDiv(-a, b)` (`lneg` wraps).
        ("positiveCeilDiv", "(JJ)J", [Value::Long(a), Value::Long(b)]) => {
            if *b == 0 {
                return Some(Err(crate::runtime::divide_by_zero()));
            }
            Some(Ok(Value::Long(
                a.wrapping_neg().div_floor_java(*b).wrapping_neg(),
            )))
        }
        (
            "mulAndTruncate",
            "(Lorg/apache/commons/lang3/math/Fraction;I)I",
            [fraction, Value::Int(factor)],
        ) => {
            if matches!(fraction, Value::Ref(None)) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: None,
                    frame: None,
                }));
            }
            let (num, den) = crate::runtime::commons_fraction::fraction_parts(fraction)?;
            Some(Ok(Value::Int(num.wrapping_mul(*factor).wrapping_div(den))))
        }
        _ => None,
    }
}

pub(crate) fn static_call(
    host: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner == ARGB {
        return argb_static_call(r, args);
    }
    if r.owner == MTH {
        return mth_static_call(r, args);
    }
    if r.owner == LWJGL_MATH {
        let [Value::Int(value)] = args else {
            return Some(host.unknown_host_call(r));
        };
        return Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("mathIsPoT", "(I)Z") => Ok(Value::Int(((*value as u32).count_ones() == 1) as i32)),
            ("mathRoundPoT", "(I)I") => {
                let predecessor = value.wrapping_sub(1) as u32;
                let shift = (32 - predecessor.leading_zeros()) & 31;
                Ok(Value::Int(1i32.wrapping_shl(shift)))
            }
            _ => host.unknown_host_call(r),
        });
    }
    if r.owner != LOG4J_STRINGS {
        return None;
    }
    let Some(value) = args.first() else {
        return Some(host.unknown_host_call(r));
    };
    Some(match (r.name.as_str(), r.descriptor.as_str()) {
        ("isEmpty", "(Ljava/lang/CharSequence;)Z") => match string_text(value) {
            Some(text) => Ok(Value::Int(text.is_none_or(String::is_empty) as i32)),
            None => host.unknown_host_call(r),
        },
        ("isNotEmpty", "(Ljava/lang/CharSequence;)Z") => match string_text(value) {
            Some(text) => Ok(Value::Int((!text.is_none_or(String::is_empty)) as i32)),
            None => host.unknown_host_call(r),
        },
        ("isBlank", "(Ljava/lang/String;)Z") => match string_text(value) {
            Some(text) => Ok(Value::Int(is_blank(text) as i32)),
            None => host.unknown_host_call(r),
        },
        ("isNotBlank", "(Ljava/lang/String;)Z") => match string_text(value) {
            Some(text) => Ok(Value::Int((!is_blank(text)) as i32)),
            None => host.unknown_host_call(r),
        },
        _ => host.unknown_host_call(r),
    })
}

pub(crate) fn empty_factory_class(r: &MethodRef) -> Option<&'static str> {
    matches!(
        (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()),
        (
            "org/apache/commons/compress/utils/Lists",
            "newArrayList",
            "()Ljava/util/ArrayList;"
        )
    )
    .then_some("java/util/ArrayList")
}

fn string_text(value: &Value) -> Option<Option<&String>> {
    match value {
        Value::Ref(None) => Some(None),
        Value::Ref(Some(text)) if crate::runtime::value_class::is_plain_string(text) => Some(Some(text)),
        _ => None,
    }
}

fn is_blank(text: Option<&String>) -> bool {
    text.is_none_or(|text| {
        text.is_empty()
            || text
                .encode_utf16()
                .all(|unit| crate::runtime::jdk_character::is_whitespace(i32::from(unit)))
    })
}

/// `Math.floorDiv` (JDK 25: `q = x / y; if ((x ^ y) < 0 && q * y != x) q--`), `MIN / -1`
/// wrapping to `MIN` as Java's does. The divisor is not zero.
trait FloorDivJava: Sized {
    fn div_floor_java(self, divisor: Self) -> Self;
}

impl FloorDivJava for i32 {
    fn div_floor_java(self, divisor: i32) -> i32 {
        let quotient = self.wrapping_div(divisor);
        if ((self < 0) != (divisor < 0)) && quotient.wrapping_mul(divisor) != self {
            quotient - 1
        } else {
            quotient
        }
    }
}

impl FloorDivJava for i64 {
    fn div_floor_java(self, divisor: i64) -> i64 {
        let quotient = self.wrapping_div(divisor);
        if ((self < 0) != (divisor < 0)) && quotient.wrapping_mul(divisor) != self {
            quotient - 1
        } else {
            quotient
        }
    }
}
