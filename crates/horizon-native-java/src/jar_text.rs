//! `String.valueOf(Object)` for objects this run constructed: what Java prints
//! for a jar enum constant, record or class instance.
//!
//! A jar object is a `class#n` marker. Every path that turns one into text
//! (`toString()`, string concatenation, `String.valueOf`, `Objects.toString`,
//! `StringBuilder.append(Object)`, `PrintStream.print(Object)`, `%s`, a
//! record's generated `toString`) used to print that marker, and nothing was
//! counted, so `"" + Mode.B` answered `demo/Probe$Mode#1` where Java answers
//! `B`, and a run that depended on it still read `ran-clean` (#69).
//!
//! Java's answer is the receiver's own `toString()`, selected virtually:
//!
//! * a jar class (or superclass) that declares `toString()` runs that body;
//! * an enum that does not inherits `Enum.toString()`, which is `name()`, the
//!   name its constructor received;
//! * a class whose chain reaches `Object` without a declaration prints
//!   `getClass().getName() + "@" + Integer.toHexString(hashCode())`. The
//!   identity hash is not reproducible, so the runtime does not invent it: the
//!   host's `Object.toString` counts the call as unknown;
//! * a jar class that extends a platform class gets that class's `toString`
//!   from the host, which answers what it models and counts the rest.
//!
//! Any other object (`new Object()`, a platform class's instance) goes to the
//! host's `Object.toString`, which prints the objects it models and counts the
//! rest. A jar object inside a host collection or optional is printed by the
//! host, which cannot run jar code: it is counted there too, never passed off
//! as Java's text.

use super::*;

/// `toString()`'s descriptor.
const TO_STRING: &str = "()Ljava/lang/String;";

/// The `Object.toString()` reference the unknown answer is counted under.
pub(crate) fn object_to_string() -> MethodRef {
    MethodRef {
        owner: "java/lang/Object".into(),
        name: "toString".into(),
        descriptor: TO_STRING.into(),
    }
}

/// The `Object`-typed overloads that print their argument through
/// `String.valueOf`, mapped to the `String` overload with the same effect.
/// The rewritten call takes the text as a `String`, so nothing downstream
/// reads the text back as a marker.
fn string_overload(r: &MethodRef) -> Option<&'static str> {
    Some(
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                "java/lang/StringBuilder",
                "append",
                "(Ljava/lang/Object;)Ljava/lang/StringBuilder;",
            ) => "(Ljava/lang/String;)Ljava/lang/StringBuilder;",
            (
                "java/lang/StringBuffer",
                "append",
                "(Ljava/lang/Object;)Ljava/lang/StringBuffer;",
            ) => "(Ljava/lang/String;)Ljava/lang/StringBuffer;",
            (
                "java/io/PrintStream" | "java/io/PrintWriter",
                "print" | "println",
                "(Ljava/lang/Object;)V",
            ) => "(Ljava/lang/String;)V",
            _ => return None,
        },
    )
}

impl<H: Host> Runtime<'_, H> {
    /// The single boundary from runtime values to Java text. A counted String
    /// has no known contents; callers must leave the enclosing operation to
    /// the host instead of ever exposing its allocation marker.
    pub(crate) fn java_text(
        &mut self,
        value: &Value,
        desc: &str,
        class: &str,
        method: &str,
    ) -> Result<Option<String>, NativeError> {
        if self.is_counted_string(value)
            || self
                .uncertain_identity_root(value, &object_to_string())
                .is_some()
        {
            return Ok(None);
        }
        if matches!(value, Value::Ref(Some(_))) || crate::jar_object::is_object(value) {
            if let Some(result) = self.runtime_value_of(value, class, method)? {
                if self.is_counted_string(&result) {
                    return Ok(None);
                }
                return Ok(match result {
                    Value::Ref(Some(text)) => Some(text),
                    Value::Ref(None) => Some("null".into()),
                    _ => Some("null".into()),
                });
            }
            if let Some(text) = self.host.modelled_to_string_checked(value)? {
                return Ok(Some(text));
            }
            if desc == "Ljava/lang/String;"
                || matches!(value, Value::Ref(Some(text)) if crate::value_class::is_plain_string(text))
            {
                return Ok(Some(crate::value_text(value)));
            }
            return Ok(None);
        }
        Ok(Some(crate::java_value_text(&mut *self.host, value, desc)?))
    }

    /// The jar class of an object this run built from a jar's own class.
    fn jar_object_class(&self, value: &Value) -> Option<String> {
        let class = crate::jar_object::packaged_object_class(value)?;
        (!self.host.models_class(class) && self.class(class).is_some()).then(|| class.to_owned())
    }

    /// `value.toString()` for a jar object: a library model's answer first
    /// (the runtime's Guava `MapMaker` wins over a copy of the class a jar
    /// ships, as it does in ordinary dispatch), then the virtual selection
    /// the module docs describe. `None` when `value` is not a jar object.
    pub(crate) fn jar_to_string(
        &mut self,
        value: &Value,
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        // A private JDK collection the skeletons built (a sublist).
        if let Some(text) = self.invoke_jdk_skeleton(&object_to_string(), 0xb6, Some(value), &[])? {
            return Ok(Some(text));
        }
        let Some(receiver_class) = self.jar_object_class(value) else {
            return Ok(None);
        };
        // A model keyed on the class (`invokevirtual MapMaker.toString`) is
        // asked as a call typed by the receiver's own class would ask it.
        let typed = MethodRef {
            owner: receiver_class,
            name: "toString".into(),
            descriptor: TO_STRING.into(),
        };
        if let Some(text) = self.invoke_library_model(&typed, Some(value), &[], class, method)? {
            return Ok(Some(text));
        }
        if let Some(text) = self.library_to_string(value, class, method)? {
            return Ok(Some(Value::Ref(Some(text))));
        }
        self.jar_object_to_string(value)
    }

    /// [`Self::jar_to_string`] after the library models declined.
    fn jar_object_to_string(&mut self, value: &Value) -> Result<Option<Value>, NativeError> {
        let Some(class) = self.jar_object_class(value) else {
            return Ok(None);
        };
        let Some((owner, jar_owned)) = self.member_owner(&class, "toString", TO_STRING) else {
            return Ok(None);
        };
        if jar_owned {
            return self
                .invoke(&owner, "toString", TO_STRING, Some(value.clone()), &[])
                .map(Some);
        }
        // `Throwable.toString()` on a jar exception (`jar_throwables.rs`).
        if let Some(text) = self.invoke_jar_throwable(&object_to_string(), 0xb6, Some(value))? {
            return Ok(Some(text));
        }
        // `Enum.toString()` is `name()`: the name its constructor received.
        if owner == "java/lang/Enum"
            && let Some((name, _)) = expect_ref(value).and_then(|m| self.enum_constants.get(m))
        {
            return Ok(Some(Value::Ref(Some(name.clone()))));
        }
        // A JDK skeleton's `toString` (`AbstractCollection`, `AbstractMap`,
        // `SimpleEntry`) prints the jar object's own elements, which the host
        // does not hold: counted (`jar_operands.rs`).
        if let Some(declaring) = jar_operands::skeleton_declaring(&owner, "toString", TO_STRING) {
            let to_string = MethodRef {
                owner: declaring.to_owned(),
                name: "toString".into(),
                descriptor: TO_STRING.into(),
            };
            // Run where `jdk_skeletons/` implements it, counted otherwise.
            if let Some(text) = self.invoke_jdk_skeleton(&to_string, 0xb6, Some(value), &[])? {
                return Ok(Some(text));
            }
            return self.host.unknown_operand_call(&to_string).map(Some);
        }
        // A concrete JDK collection's `toString` over the jar's own
        // `iterator()`/`entrySet()` override: counted, the host prints its row.
        let typed = MethodRef {
            owner: owner.clone(),
            name: "toString".into(),
            descriptor: TO_STRING.into(),
        };
        if let Some(counted) = self.concrete_subclass_callback(&typed, value) {
            self.host.unknown_operand_call(&counted)?;
        }
        // `Object.toString()` (an identity hash), an enum constant this run
        // has no name for, or a platform superclass's method: the host answers
        // what it models and counts the rest.
        let to_string = MethodRef {
            owner,
            name: "toString".into(),
            descriptor: TO_STRING.into(),
        };
        self.host.invoke_instance(&to_string, value, &[]).map(Some)
    }

    /// `String.valueOf(value)` for an object (`class#n`): a builder's text, a
    /// library model's `toString` (a proxy's handler), a jar object's, and
    /// otherwise the host's `Object.toString`, which answers the objects it
    /// models and counts the rest. `None` leaves every other value (null,
    /// strings, boxes, the host's other marker families) to its caller.
    ///
    /// `class` and `method` name the calling frame, for a library model that
    /// runs a callback.
    pub(crate) fn runtime_value_of(
        &mut self,
        value: &Value,
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        if self
            .uncertain_identity_root(value, &object_to_string())
            .is_some()
        {
            return self
                .host
                .unknown_operand_call(&object_to_string())
                .map(Some);
        }
        // A host handle prints its kind's `toString` (`host_ref.rs`); one
        // whose held value has no text here is counted.
        if let Value::Host(handle) = value
            && !crate::jar_object::is_object_handle(handle)
        {
            return match self.host_ref_text(handle, class, method)? {
                Some(text) => Ok(Some(Value::Ref(Some(text)))),
                None => self
                    .host
                    .unknown_operand_call(&object_to_string())
                    .map(Some),
            };
        }
        // A `String` spelling an object's key is that text.
        if crate::jar_object::spells_key(value) {
            return Ok(None);
        }
        let Some(marker) = expect_ref(value) else {
            return Ok(None);
        };
        if let Some(text) = self.string_builders.get(marker) {
            return Ok(Some(Value::Ref(Some(text.clone()))));
        }
        // A stream's `toString` is its stage class and identity hash: counted,
        // and its marker never printed (`stream_pipeline::STREAM_TEXT`).
        if self.is_live_stream(marker) {
            self.host.unknown_operand_call(&object_to_string())?;
            return Ok(Some(Value::Ref(Some(
                crate::stream_pipeline::STREAM_TEXT.into(),
            ))));
        }
        if let Some(text) = self.invoke_enum_set_text(
            &crate::MethodRef {
                owner: "java/util/EnumSet".into(),
                name: "toString".into(),
                descriptor: "()Ljava/lang/String;".into(),
            },
            Some(value),
        )? {
            return Ok(Some(text));
        }
        // A host map entry prints `key=value` (`host_entry.rs`).
        if let Some(text) = self.host_entry_text(value, class, method)? {
            return Ok(Some(text));
        }
        // `File.toString()` is its path.
        if let Some(path) = self.file_path(value) {
            return Ok(Some(Value::Ref(Some(path.to_owned()))));
        }
        if crate::jar_object::packaged_object_class(value).is_none() {
            return Ok(None);
        }
        // A jar's own `BlockPos`/`LevelResource` (a subclass included):
        // `Vec3i.toString` or the record's, by its fields
        // (`jar_server_values.rs`). A subclass with its own `toString` is
        // not answered here and runs it below.
        if let Some(text) = self.jar_server_value_text(value) {
            return Ok(Some(Value::Ref(Some(text))));
        }
        if let Some(text) = self.item_stack_to_string_opt(value, class, method)? {
            return Ok(Some(Value::Ref(Some(text))));
        }
        if let Some(text) = self.jar_to_string(value, class, method)? {
            // `String.valueOf` returns what `toString` returned, even `null`.
            return Ok(Some(text));
        }
        if let Some(text) = self.library_to_string(value, class, method)? {
            return Ok(Some(Value::Ref(Some(text))));
        }
        // A collection the host made prints through its model (`AbstractCollection.toString`),
        // and a cycle in it ends in a `StackOverflowError`, as Java's does (#250); before, this
        // fell through to `Object.toString`, which the host counts.
        if self.host_collection_kind(value).is_some()
            && let Some(text) = self.host.modelled_to_string_checked(value)?
        {
            return Ok(Some(Value::Ref(Some(text))));
        }
        self.host
            .invoke_instance(&object_to_string(), value, &[])
            .map(Some)
    }

    /// `String.valueOf(value)` as text, for a runtime path that prints a
    /// value it holds (a record component): the runtime's own answer, then a
    /// host model's `toString`, then the value's own text (a string, a box, a
    /// primitive by its descriptor).
    pub(crate) fn value_of_text(
        &mut self,
        value: &Value,
        desc: &str,
        class: &str,
        method: &str,
    ) -> Result<String, NativeError> {
        if self
            .uncertain_identity_root(value, &object_to_string())
            .is_some()
        {
            self.host.unknown_operand_call(&object_to_string())?;
            return Ok("null".into());
        }
        if let Value::Host(handle) = value
            && !crate::jar_object::is_object_handle(handle)
        {
            if let Some(text) = self.host_ref_text(handle, class, method)? {
                return Ok(text);
            }
            self.host.unknown_operand_call(&object_to_string())?;
        }
        if matches!(value, Value::Ref(Some(_))) || crate::jar_object::is_object(value) {
            match self.runtime_value_of(value, class, method)? {
                Some(Value::Ref(Some(text))) => return Ok(text),
                Some(_) => return Ok("null".into()),
                None => {}
            }
            if let Some(text) = self.host.modelled_to_string_checked(value)? {
                return Ok(text);
            }
            // No model prints it: a `String` is its own text, anything else
            // (a host marker, a stream, an object) is counted, never shown
            // as its key without a count.
            if !matches!(value, Value::Ref(Some(text)) if crate::value_class::is_plain_string(text))
            {
                self.host.unknown_operand_call(&object_to_string())?;
            }
        }
        java_value_text(&mut *self.host, value, desc)
    }

    /// The calls that stringify a jar object, answered with its real
    /// `toString` (#69): `toString()` itself when the receiver inherits it,
    /// `String.valueOf(Object)`, `Objects.toString`, and the `Object`
    /// overloads of `StringBuilder.append` and `PrintStream.print(ln)`.
    ///
    /// `None` leaves the call to the dispatch below, unchanged.
    pub(crate) fn invoke_jar_text(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        // A virtual `toString()` on a jar object or a host map entry.
        // `invokespecial` (a `super.toString()`) keeps its symbolic owner.
        if r.name == "toString"
            && r.descriptor == TO_STRING
            && matches!(opcode, 0xb6 | 0xb9)
            && let Some(receiver) = receiver
        {
            if let Some(key) = crate::jar_object::object_key(receiver)
                && self.counted_text_objects.contains(key)
            {
                return self.counted_unknown(r).map(Some);
            }
            if self.is_counted_string(receiver) {
                return self.counted_unknown(r).map(Some);
            }
            if let Some(text) = self.host_entry_text(receiver, class, method)? {
                return Ok(Some(text));
            }
            if expect_ref(receiver).is_some_and(|marker| self.is_live_stream(marker)) {
                return self.runtime_value_of(receiver, class, method);
            }
            let result = self.jar_to_string(receiver, class, method)?;
            if result
                .as_ref()
                .is_some_and(|value| self.is_counted_string(value))
            {
                return self.counted_unknown(r).map(Some);
            }
            return Ok(result);
        }
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("java/lang/String", "valueOf", "(Ljava/lang/Object;)Ljava/lang/String;")
            | ("java/util/Objects", "toString", "(Ljava/lang/Object;)Ljava/lang/String;") => {
                let Some(value) = args.first() else {
                    return Ok(None);
                };
                if self.is_counted_string(value) {
                    return self.counted_unknown(r).map(Some);
                }
                let result = self.runtime_value_of(value, class, method)?;
                if result
                    .as_ref()
                    .is_some_and(|result| self.is_counted_string(result))
                {
                    return self.counted_unknown(r).map(Some);
                }
                return Ok(result);
            }
            // `Objects.toString(o, nullDefault)`: `o == null ? nullDefault :
            // o.toString()`, which for a non-null `o` is `String.valueOf(o)`.
            (
                "java/util/Objects",
                "toString",
                "(Ljava/lang/Object;Ljava/lang/String;)Ljava/lang/String;",
            ) => {
                let [value, default] = args else {
                    return Ok(None);
                };
                if *value == Value::Ref(None) {
                    return Ok(Some(default.clone()));
                }
                if self.is_counted_string(value) {
                    return self.counted_unknown(r).map(Some);
                }
                if let Some(text) = self.runtime_value_of(value, class, method)? {
                    if self.is_counted_string(&text) {
                        return self.counted_unknown(r).map(Some);
                    }
                    return Ok(Some(text));
                }
                let value_of = MethodRef {
                    owner: "java/lang/String".into(),
                    name: "valueOf".into(),
                    descriptor: "(Ljava/lang/Object;)Ljava/lang/String;".into(),
                };
                return self
                    .host
                    .invoke(&value_of, std::slice::from_ref(value))
                    .map(Some);
            }
            _ => {}
        }
        // `append(Object)`/`print(Object)` of a value whose text only the
        // runtime knows: the same call with that text as a `String`.
        let Some(descriptor) = string_overload(r) else {
            return Ok(None);
        };
        let Some(value) = args.first() else {
            return Ok(None);
        };
        // A builder argument keeps the builder model's own path.
        if self.builder_text(value).is_some() {
            return Ok(None);
        }
        if self.is_counted_string(value)
            || self
                .uncertain_identity_root(value, &object_to_string())
                .is_some()
        {
            return self.counted_unknown(r).map(Some);
        }
        let Some(text) = self.runtime_value_of(value, class, method)? else {
            return Ok(None);
        };
        if self.is_counted_string(&text) {
            return self.counted_unknown(r).map(Some);
        }
        let rewritten = MethodRef {
            owner: r.owner.clone(),
            name: r.name.clone(),
            descriptor: descriptor.into(),
        };
        let args = [text];
        if let Some(value) = self.invoke_string_builder(&rewritten, receiver, &args)? {
            return Ok(Some(value));
        }
        if let Some(value) = self.invoke_io(&rewritten, receiver, &args, class, method)? {
            return Ok(Some(value));
        }
        match receiver {
            Some(receiver) => self.host.invoke_instance(&rewritten, receiver, &args),
            None => self.host.invoke(&rewritten, &args),
        }
        .map(Some)
    }

    /// A string-concat site's object operands as their `String.valueOf`
    /// text. Each resolved operand's descriptor becomes `String`, so the
    /// concat's later passes read the text as text, not as a marker.
    pub(crate) fn concat_object_texts(
        &mut self,
        captured: &mut [Value],
        arguments: &mut [String],
        class: &str,
        method: &str,
    ) -> Result<bool, NativeError> {
        for (position, value) in captured.iter_mut().enumerate() {
            if arguments.get(position).is_some_and(|argument| {
                argument == "Ljava/lang/String;" || !argument.starts_with(['L', '['])
            }) {
                continue;
            }
            let Some(text) = self.java_text(
                value,
                arguments
                    .get(position)
                    .map_or("Ljava/lang/Object;", String::as_str),
                class,
                method,
            )?
            else {
                return Ok(true);
            };
            *value = Value::Ref(Some(text));
            if let Some(argument) = arguments.get_mut(position) {
                *argument = "Ljava/lang/String;".into();
            }
        }
        Ok(false)
    }
}
