//! `Enum.valueOf(Class, String)`, and through it every enum's synthetic
//! `valueOf(String)` (javac compiles it to `Enum.valueOf(Color.class, name)`).
//!
//! The host used to build an `enum/<class>/<name>` marker for any non-empty
//! name, so `Color.valueOf("BLUE")` on an enum with no `BLUE` returned an
//! object and a config reader's `catch (IllegalArgumentException)` fallback
//! never ran. The JDK answers the constant with that name, or throws
//! (OpenJDK 27):
//!
//! - unknown name: `IllegalArgumentException: No enum constant demo.E.Color.BLUE`
//!   (the class's canonical name, so a nested class's `$` reads `.`);
//! - `null` name: `NullPointerException: Name is null`.
//!
//! A jar (or linked) enum answers the very constant object its `<clinit>`
//! built, read from the class's `ACC_ENUM` static field, so `valueOf(..) ==
//! Color.GREEN` holds. A game enum the runtime has a constant table for is
//! checked against that table. Any other class is answered by the host, which
//! counts the call because it cannot check the name.

use super::*;

const ACC_ENUM: u16 = 0x4000;

/// Known non-jar enum constants whose class data is supplied by the host.
pub(crate) fn known_enum_values(class: &str) -> Option<Vec<Value>> {
    if class == "java/util/concurrent/TimeUnit" {
        return Some(
            crate::host_jdk_constant::TIME_UNITS
                .iter()
                .map(|name| crate::host_jdk_constant::time_unit(name))
                .collect(),
        );
    }
    super::game_enum_constants(class).map(|constants| {
        constants
            .iter()
            .map(|name| Value::Ref(Some(format!("enum/{class}/{name}"))))
            .collect()
    })
}

impl<H: Host> Runtime<'_, H> {
    pub(super) fn invoke_enum_value_of(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != "java/lang/Enum"
            || r.name != "valueOf"
            || r.descriptor != "(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;"
        {
            return Ok(None);
        }
        let class_value = args.first().unwrap_or(&Value::Ref(None));
        let name = match args.get(1) {
            Some(Value::Ref(Some(name))) => name.clone(),
            Some(Value::Ref(None)) | None => {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("Name is null".into()),
                    frame: None,
                });
            }
            _ => return Ok(None),
        };
        let Some(class_name) = crate::host_class::class_name(class_value) else {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: Some("Cannot invoke \"java.lang.Class.enumConstantDirectory()\" because \"enumClass\" is null".into()),
                frame: None,
            });
        };
        let class = class_name.replace('.', "/");
        if let Some(values) = known_enum_values(&class) {
            return values
                .into_iter()
                .find(|value| match value {
                    Value::Host(handle) => handle.key() == name,
                    Value::Ref(Some(marker)) => marker.rsplit('/').next() == Some(name.as_str()),
                    _ => false,
                })
                .map(|value| Ok(Some(value)))
                .unwrap_or_else(|| Err(no_enum_constant(&class, &name)));
        }
        let field = match self.class(&class) {
            Some(file) if file.super_name == "java/lang/Enum" => file
                .fields
                .iter()
                .find(|field| {
                    field.is_static() && field.access & ACC_ENUM != 0 && field.name == name
                })
                .map(|field| format!("{class}#{}{}", field.name, field.descriptor)),
            Some(_) => return Err(non_enum_constant(&class)),
            None if crate::jdk_closure::class(&class).is_some() => {
                return Err(non_enum_constant(&class));
            }
            None => return self.host.unknown_operand_call(r).map(Some),
        };
        let Some(key) = field else {
            return Err(no_enum_constant(&class, &name));
        };
        self.ensure_initialized(&class)?;
        Ok(self.static_fields.get(&key).cloned())
    }
}

fn non_enum_constant(class: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(format!("{} is not an enum class", class.replace('/', "."))),
        frame: None,
    }
}

fn no_enum_constant(class: &str, name: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(format!(
            "No enum constant {}.{name}",
            class.replace(['/', '$'], ".")
        )),
        frame: None,
    }
}
