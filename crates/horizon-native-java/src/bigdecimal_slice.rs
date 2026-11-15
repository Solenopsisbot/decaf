//! A bounded exact slice of `java.math.BigDecimal` for config formatting.
//!
//! The state is carried by the jar object's handle.  The slice keeps the
//! unscaled decimal digits and scale, so equality (which includes scale),
//! numeric comparison, formatting, precision and the common scale-changing
//! operations agree with Java for the values it accepts.

use crate::{Host, MethodRef, NativeError, Runtime, Value};

const OWNER: &str = "java/math/BigDecimal";
const STATE: &str = "|#bigDecimal";
const CACHE_ZERO: &str = "java/math/BigDecimal#18446744073709551610";
const CACHE_ONE: &str = "java/math/BigDecimal#18446744073709551611";
const CACHE_TEN: &str = "java/math/BigDecimal#18446744073709551612";
// Java caches `valueOf(long)` (and the scale-zero overload) for every value in
// this range.  These are explicit, unreachable allocation keys: never derive
// them from a table length, which would make the identity depend on execution
// order.
const CACHE_TWO: &str = "java/math/BigDecimal#18446744073709551613";
const CACHE_THREE: &str = "java/math/BigDecimal#18446744073709551614";
const CACHE_FOUR: &str = "java/math/BigDecimal#18446744073709551615";
const CACHE_FIVE: &str = "java/math/BigDecimal#18446744073709551616";
const CACHE_SIX: &str = "java/math/BigDecimal#18446744073709551617";
const CACHE_SEVEN: &str = "java/math/BigDecimal#18446744073709551618";
const CACHE_EIGHT: &str = "java/math/BigDecimal#18446744073709551619";
const CACHE_NINE: &str = "java/math/BigDecimal#18446744073709551620";

#[derive(Clone, Debug)]
struct Decimal {
    sign: i8,
    digits: String,
    scale: i64,
}

impl Decimal {
    fn zero() -> Self {
        Self {
            sign: 0,
            digits: "0".into(),
            scale: 0,
        }
    }

    fn parse(text: &str) -> Option<Self> {
        let (negative, text) = match text.strip_prefix(['+', '-']) {
            Some(rest) => (text.starts_with('-'), rest),
            None => (false, text),
        };
        let (mantissa, exponent) = match text.find(['e', 'E']) {
            Some(at) => (&text[..at], text[at + 1..].parse::<i64>().ok()?),
            None => (text, 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if whole.is_empty() && fraction.is_empty() {
            return None;
        }
        if !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let raw = format!("{whole}{fraction}");
        let scale = fraction.len() as i64 - exponent;
        let Some(first) = raw.find(|b: char| b != '0') else {
            return Some(Self {
                sign: 0,
                digits: "0".into(),
                scale,
            });
        };
        let digits = raw[first..].to_owned();
        let sign = if negative { -1 } else { 1 };
        Some(Self {
            sign,
            digits,
            scale,
        })
    }

    fn plain(&self) -> String {
        if self.sign == 0 {
            return if self.scale > 0 {
                format!("0.{}", "0".repeat(self.scale as usize))
            } else {
                "0".into()
            };
        }
        let mut out = if self.scale <= 0 {
            format!(
                "{}{}{}",
                if self.sign < 0 { "-" } else { "" },
                self.digits,
                "0".repeat((-self.scale) as usize)
            )
        } else if self.scale as usize >= self.digits.len() {
            format!(
                "{}0.{}{}",
                if self.sign < 0 { "-" } else { "" },
                "0".repeat(self.scale as usize - self.digits.len()),
                self.digits
            )
        } else {
            let at = self.digits.len() - self.scale as usize;
            format!(
                "{}{}.{}",
                if self.sign < 0 { "-" } else { "" },
                &self.digits[..at],
                &self.digits[at..]
            )
        };
        if out == "-0" {
            out = "0".into();
        }
        out
    }

    fn string(&self) -> String {
        let adjusted = self.digits.len() as i64 - self.scale - 1;
        if self.scale >= 0 && adjusted >= -6 {
            return self.plain();
        }
        let mut out = String::new();
        if self.sign < 0 {
            out.push('-');
        }
        out.push(self.digits.as_bytes()[0] as char);
        if self.digits.len() > 1 {
            out.push('.');
            out.push_str(&self.digits[1..]);
        }
        out.push('E');
        if adjusted >= 0 {
            out.push('+');
        }
        out.push_str(&adjusted.to_string());
        out
    }

    fn precision(&self) -> i32 {
        self.digits.len() as i32
    }

    fn strip_zeros(mut self) -> Self {
        if self.sign == 0 {
            return Self::zero();
        }
        while self.digits.ends_with('0') {
            self.digits.pop();
            self.scale -= 1;
        }
        self
    }

    fn aligned(&self, scale: i64) -> String {
        if self.scale >= scale {
            self.digits.clone()
        } else {
            format!(
                "{}{}",
                self.digits,
                "0".repeat((scale - self.scale) as usize)
            )
        }
    }

    fn compare(&self, other: &Self) -> i32 {
        if self.sign != other.sign {
            return self.sign.cmp(&other.sign) as i32;
        }
        if self.sign == 0 {
            return 0;
        }
        let scale = self.scale.max(other.scale);
        let a = self.aligned(scale);
        let b = other.aligned(scale);
        let order = a.len().cmp(&b.len()).then_with(|| a.cmp(&b));
        (order as i32) * self.sign as i32
    }

    fn set_scale(mut self, new_scale: i64, mode: &str) -> Result<Self, NativeError> {
        let delta = new_scale - self.scale;
        if self.sign == 0 {
            self.scale = new_scale;
            return Ok(self);
        }
        if delta >= 0 {
            self.digits.push_str(&"0".repeat(delta as usize));
            self.scale = new_scale;
            return Ok(self);
        }
        let drop = (-delta) as usize;
        if drop >= self.digits.len() {
            let discarded = &self.digits;
            // The unscaled value may have fewer digits than the number being
            // discarded.  Those implicit leading zeroes are significant to
            // half-rounding: a value such as 5.7E-69 is below half of 1E-7,
            // even though its first stored digit is 5.
            let round = if drop > self.digits.len() {
                match mode {
                    "DOWN" => false,
                    "UP" => true,
                    "CEILING" => self.sign > 0,
                    "FLOOR" => self.sign < 0,
                    "UNNECESSARY" => {
                        return Err(NativeError::UncaughtException {
                            class: "java/lang/ArithmeticException".into(),
                            message: Some("Rounding necessary".into()),
                            frame: None,
                        });
                    }
                    _ => false,
                }
            } else {
                should_round(self.sign, "0", discarded, mode)?
            };
            self.digits = if round { "1".into() } else { "0".into() };
            self.sign = if self.digits == "0" { 0 } else { self.sign };
            self.scale = new_scale;
            return Ok(self);
        }
        let cut = self.digits.len() - drop;
        let kept = self.digits[..cut].to_owned();
        let discarded = &self.digits[cut..];
        let round = should_round(self.sign, &kept, discarded, mode)?;
        self.digits = kept;
        self.scale = new_scale;
        if round {
            increment(&mut self.digits);
        }
        Ok(self)
    }
}

fn increment(digits: &mut String) {
    let mut bytes = digits.as_bytes().to_vec();
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] == b'9' {
            bytes[i] = b'0';
        } else {
            bytes[i] += 1;
            *digits = String::from_utf8(bytes).unwrap();
            return;
        }
    }
    *digits = format!("1{}", String::from_utf8(bytes).unwrap());
}

fn should_round(sign: i8, kept: &str, discarded: &str, mode: &str) -> Result<bool, NativeError> {
    if discarded.chars().all(|c| c == '0') {
        return Ok(false);
    }
    match mode {
        "DOWN" => Ok(false),
        "UP" => Ok(true),
        "CEILING" => Ok(sign > 0),
        "FLOOR" => Ok(sign < 0),
        "UNNECESSARY" => Err(NativeError::UncaughtException {
            class: "java/lang/ArithmeticException".into(),
            message: Some("Rounding necessary".into()),
            frame: None,
        }),
        "HALF_DOWN" => Ok(discarded.as_bytes()[0] > b'5'
            || (discarded.as_bytes()[0] == b'5' && discarded[1..].bytes().any(|b| b != b'0'))),
        "HALF_EVEN" => Ok(discarded.as_bytes()[0] > b'5'
            || (discarded.as_bytes()[0] == b'5'
                && (discarded[1..].bytes().any(|b| b != b'0')
                    || kept.as_bytes().last().is_some_and(|b| (b - b'0') % 2 == 1)))),
        _ => Ok(discarded.as_bytes()[0] >= b'5'), // HALF_UP and the default
    }
}

fn state_key(marker: &str) -> String {
    format!("{marker}{STATE}")
}
fn read<H: Host>(runtime: &Runtime<'_, H>, receiver: &Value) -> Option<Decimal> {
    let marker = crate::jar_object::object_key(receiver)?;
    match runtime.fields.get(&state_key(marker)) {
        Some(Value::Ref(Some(text))) => {
            let mut parts = text.splitn(3, ';');
            Some(Decimal {
                sign: parts.next()?.parse().ok()?,
                scale: parts.next()?.parse().ok()?,
                digits: parts.next()?.into(),
            })
        }
        _ => cached_decimal(marker),
    }
}

fn cached_decimal(key: &str) -> Option<Decimal> {
    let digits = match key {
        CACHE_ZERO => "0",
        CACHE_ONE => "1",
        CACHE_TWO => "2",
        CACHE_THREE => "3",
        CACHE_FOUR => "4",
        CACHE_FIVE => "5",
        CACHE_SIX => "6",
        CACHE_SEVEN => "7",
        CACHE_EIGHT => "8",
        CACHE_NINE => "9",
        CACHE_TEN => "10",
        _ => return None,
    };
    Some(Decimal {
        sign: if digits == "0" { 0 } else { 1 },
        digits: digits.into(),
        scale: 0,
    })
}
fn write<H: Host>(runtime: &mut Runtime<'_, H>, receiver: &Value, value: &Decimal) -> Option<()> {
    let marker = crate::jar_object::object_key(receiver)?;
    runtime.fields.insert(
        state_key(marker),
        Value::Ref(Some(format!(
            "{};{};{}",
            value.sign, value.scale, value.digits
        ))),
    );
    Some(())
}
fn new_value<H: Host>(runtime: &mut Runtime<'_, H>, value: Decimal) -> Value {
    if let Some(cached) = cached_value(&value) {
        return cached;
    }
    let object = runtime.new_object(OWNER);
    let _ = write(runtime, &object, &value);
    object
}

fn cached_value(value: &Decimal) -> Option<Value> {
    if value.scale != 0 || value.sign < 0 {
        return None;
    }
    match (value.sign, value.digits.as_str()) {
        (0, "0") => Some(crate::jar_object::stored(CACHE_ZERO)),
        (1, "1") => Some(crate::jar_object::stored(CACHE_ONE)),
        (1, "2") => Some(crate::jar_object::stored(CACHE_TWO)),
        (1, "3") => Some(crate::jar_object::stored(CACHE_THREE)),
        (1, "4") => Some(crate::jar_object::stored(CACHE_FOUR)),
        (1, "5") => Some(crate::jar_object::stored(CACHE_FIVE)),
        (1, "6") => Some(crate::jar_object::stored(CACHE_SIX)),
        (1, "7") => Some(crate::jar_object::stored(CACHE_SEVEN)),
        (1, "8") => Some(crate::jar_object::stored(CACHE_EIGHT)),
        (1, "9") => Some(crate::jar_object::stored(CACHE_NINE)),
        (1, "10") => Some(crate::jar_object::stored(CACHE_TEN)),
        _ => None,
    }
}
fn rounding_mode(value: Option<&Value>) -> Option<&str> {
    value.and_then(crate::host_jdk_constant::rounding_mode_name)
}
pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    if r.owner == OWNER && r.descriptor == "Ljava/math/BigDecimal;" {
        return match r.name.as_str() {
            "ZERO" => Some(crate::jar_object::stored(CACHE_ZERO)),
            "ONE" => Some(crate::jar_object::stored(CACHE_ONE)),
            "TEN" => Some(crate::jar_object::stored(CACHE_TEN)),
            _ => None,
        };
    }
    if r.owner == "java/math/RoundingMode"
        && r.descriptor == "Ljava/math/RoundingMode;"
        && matches!(
            r.name.as_str(),
            "UP" | "DOWN"
                | "CEILING"
                | "FLOOR"
                | "HALF_UP"
                | "HALF_DOWN"
                | "HALF_EVEN"
                | "UNNECESSARY"
        )
    {
        return Some(crate::host_jdk_constant::rounding_mode(&r.name));
    }
    None
}

fn bad_number() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NumberFormatException".into(),
        message: Some("Infinite or NaN".into()),
        frame: None,
    }
}

pub(crate) fn static_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if r.owner != OWNER || r.name != "valueOf" {
        return None;
    }
    let value = match r.descriptor.as_str() {
        "(D)Ljava/math/BigDecimal;" => match args.first() {
            Some(Value::Float(v)) => Decimal::parse(&crate::jdk_float_text::double_to_string(*v)),
            _ => None,
        },
        "(J)Ljava/math/BigDecimal;" => match args.first() {
            Some(Value::Long(v)) => Decimal::parse(&v.to_string()),
            _ => None,
        },
        "(JI)Ljava/math/BigDecimal;" => match (args.first(), args.get(1)) {
            (Some(Value::Long(v)), Some(Value::Int(scale))) => Some(Decimal {
                sign: if *v == 0 {
                    0
                } else if *v < 0 {
                    -1
                } else {
                    1
                },
                digits: v.unsigned_abs().to_string(),
                scale: i64::from(*scale),
            }),
            _ => None,
        },
        _ => return None,
    };
    Some(value.map(|v| new_value(runtime, v)).ok_or_else(bad_number))
}

pub(crate) fn instance_call<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    if crate::jar_object::object_class(receiver) != Some(OWNER) {
        return None;
    }
    if r.name == "<init>" {
        let value = match r.descriptor.as_str() {
            "(Ljava/lang/String;)V" => args
                .first()
                .and_then(crate::expect_ref)
                .and_then(|text| Decimal::parse(text)),
            "(I)V" => args.first().and_then(|v| {
                if let Value::Int(v) = v {
                    Decimal::parse(&v.to_string())
                } else {
                    None
                }
            }),
            "(J)V" => args.first().and_then(|v| {
                if let Value::Long(v) = v {
                    Decimal::parse(&v.to_string())
                } else {
                    None
                }
            }),
            "(D)V" => args.first().and_then(|v| {
                if let Value::Float(v) = v {
                    Decimal::parse(&crate::jdk_float_text::double_to_string(*v))
                } else {
                    None
                }
            }),
            _ => return None,
        };
        return Some(
            value
                .map(|v| {
                    let _ = write(runtime, receiver, &v);
                    Value::Void
                })
                .ok_or_else(bad_number),
        );
    }
    let value = read(runtime, receiver)?;
    let result = match (r.name.as_str(), r.descriptor.as_str()) {
        ("stripTrailingZeros", "()Ljava/math/BigDecimal;") => {
            Ok(new_value(runtime, value.clone().strip_zeros()))
        }
        ("toPlainString", "()Ljava/lang/String;") => Ok(Value::Ref(Some(value.plain()))),
        ("toString", "()Ljava/lang/String;") => Ok(Value::Ref(Some(value.string()))),
        ("scale", "()I") => Ok(Value::Int(value.scale as i32)),
        ("precision", "()I") => Ok(Value::Int(value.precision())),
        ("setScale", "(I)Ljava/math/BigDecimal;") => {
            let scale = match args.first() {
                Some(Value::Int(v)) => i64::from(*v),
                _ => return Some(Err(bad_number())),
            };
            value
                .set_scale(scale, "UNNECESSARY")
                .map(|v| new_value(runtime, v))
        }
        ("setScale", "(ILjava/math/RoundingMode;)Ljava/math/BigDecimal;") => {
            let scale = match args.first() {
                Some(Value::Int(v)) => i64::from(*v),
                _ => return Some(Err(bad_number())),
            };
            if matches!(args.get(1), Some(Value::Ref(None))) {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some(
                        "Cannot read field \"oldMode\" because \"roundingMode\" is null".into(),
                    ),
                    frame: None,
                }));
            }
            let mode = rounding_mode(args.get(1)).unwrap_or("UNNECESSARY");
            value.set_scale(scale, mode).map(|v| new_value(runtime, v))
        }
        ("compareTo", "(Ljava/math/BigDecimal;)I") => {
            let other = read(runtime, args.first()?)?;
            Ok(Value::Int(value.compare(&other)))
        }
        ("equals", "(Ljava/lang/Object;)Z") => {
            let answer = read(runtime, args.first()?).is_some_and(|other| {
                value.sign == other.sign
                    && value.scale == other.scale
                    && value.digits == other.digits
            });
            Ok(Value::Int(answer as i32))
        }
        ("hashCode", "()I") => Ok(Value::Int(big_decimal_hash(&value))),
        _ => return None,
    };
    Some(result)
}

fn big_integer_hash(digits: &str, sign: i8) -> i32 {
    let mut decimal = digits.trim_start_matches('0').as_bytes().to_vec();
    if decimal.is_empty() {
        return 0;
    }
    let mut words = Vec::new();
    while !decimal.is_empty() {
        let mut carry: u64 = 0;
        let mut next = Vec::new();
        for d in decimal {
            let n = carry * 10 + u64::from(d - b'0');
            let q = n / 4_294_967_296;
            carry = n % 4_294_967_296;
            if !next.is_empty() || q > 0 {
                next.push(b'0' + q as u8);
            }
        }
        words.push(carry as u32);
        decimal = next;
    }
    let mut hash = 0i32;
    for word in words.iter().rev() {
        hash = hash.wrapping_mul(31).wrapping_add(*word as i32);
    }
    if sign < 0 { hash.wrapping_neg() } else { hash }
}
fn big_decimal_hash(value: &Decimal) -> i32 {
    big_integer_hash(&value.digits, value.sign)
        .wrapping_mul(31)
        .wrapping_add(value.scale as i32)
}
