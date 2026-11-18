//! `Class.isEnum` and `Class.getEnumConstants` over the class-file model.
//!
//! Settings frameworks pick a converter per field type and treat enums as a
//! case of their own (Carpet's `ParsedRule` asks `type.isEnum()` and lists the
//! options from `getEnumConstants()`), so both must answer the JVM's way:
//!
//! - `isEnum` is true exactly when the class's direct superclass is
//!   `java.lang.Enum` (a constant with its own body is a subclass of the enum,
//!   not an enum itself, on the JVM as well);
//! - `getEnumConstants` runs the enum's own `values()`, which is what the JVM
//!   does, so the constants are the objects the enum's `<clinit>` built; a game
//!   enum answers its modelled constants, and a non-enum answers null.

use super::*;

const ACC_ENUM: u16 = 0x4000;

impl<H: Host> Runtime<'_, H> {
    pub(super) fn invoke_reflect_enum(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != "java/lang/Class" {
            return Ok(None);
        }
        let is_enum_query = r.name == "isEnum" && r.descriptor == "()Z";
        let constants_query =
            r.name == "getEnumConstants" && r.descriptor == "()[Ljava/lang/Object;";
        if !is_enum_query && !constants_query {
            return Ok(None);
        }
        let class = reflection_class_name(receiver, r)?.replace('.', "/");
        // A held class file is authoritative, including anonymous subclasses of
        // enum constants: Java sets ACC_ENUM only on the enum declaration, and
        // its direct superclass must be java.lang.Enum. The generated JDK 25
        // table carries the same direct-superclass fact for boot classes.
        let held_enum = self.class(&class).map(|file| {
            crate::runtime::jdk_model::class_access_flags(file).is_some_and(|flags| flags & ACC_ENUM != 0)
                && file.super_name == "java/lang/Enum"
        });
        let jdk_enum = crate::runtime::jdk_closure::class_is_enum(&class);
        let game_constants = game_enum_constants(&class);
        // Primitive and array Class objects are never enum classes; Java
        // answers this without consulting class metadata.
        let known_enum = if crate::runtime::host_class::is_primitive(&class) || class.starts_with('[') {
            Some(false)
        } else {
            held_enum
                .or(jdk_enum)
                .or_else(|| game_constants.map(|_| true))
                // A game enum whose constants are not listed: an enum all the same.
                .or_else(|| crate::runtime::game_enum_class(&class).then_some(true))
        };
        if is_enum_query {
            return Ok(known_enum.map(|is_enum| Value::Int(i32::from(is_enum))));
        }
        match (held_enum, game_constants) {
            (Some(true), _) => {
                let descriptor = format!("()[L{class};");
                self.invoke(&class, "values", &descriptor, None, &[])
                    .map(Some)
            }
            (Some(false), _) => Ok(Some(Value::Ref(None))),
            (None, Some(constants)) => {
                let values = constants
                    .iter()
                    .map(|name| Value::Ref(Some(format!("enum/{class}/{name}"))))
                    .collect();
                // A clone of `values()`: an array of the enum class.
                Ok(Some(self.new_array_value(&format!("[L{class};"), values)))
            }
            (None, None) => Ok(None),
        }
    }
}
