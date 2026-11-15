use super::*;

impl<H: Host> Runtime<'_, H> {
    pub(super) fn invoke_enum_collection(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        match r.owner.as_str() {
            "java/util/EnumSet" => self.invoke_enum_set(r, receiver, args),
            "java/util/EnumMap" => self.invoke_enum_map(r, receiver, args),
            _ => Ok(None),
        }
    }

    fn invoke_enum_set(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if let Some(receiver) = receiver {
            if r.name == "toString" && r.descriptor == "()Ljava/lang/String;" {
                return self.enum_collection_text(receiver, false).map(Some);
            }
            let result = self.host.invoke_instance(r, receiver, args)?;
            if r.name == "add" {
                self.sort_enum_set(receiver, r)?;
            }
            return Ok(Some(result));
        }
        if r.name == "of" && r.descriptor.ends_with("Ljava/util/EnumSet;") {
            let values = if args.len() == 2 && r.descriptor.contains("[Ljava/lang/Enum;") {
                self.array(args.get(1).ok_or_else(|| {
                    NativeError::Metadata("EnumSet.of needs its varargs array".into())
                })?)?
                .clone()
            } else {
                args.to_vec()
            };
            return self.make_enum_set(r, values);
        }
        if r.name == "noneOf" && r.descriptor == "(Ljava/lang/Class;)Ljava/util/EnumSet;" {
            let Some(class) = args.first().and_then(host_class::class_name) else {
                return Ok(None);
            };
            if !self.enum_class_supported(class) {
                return self.host.unknown_operand_call(r).map(Some);
            }
            return self.make_collection("set", Vec::new()).map(Some);
        }
        if r.name == "allOf" && r.descriptor == "(Ljava/lang/Class;)Ljava/util/EnumSet;" {
            let Some(class) = args.first().and_then(host_class::class_name) else {
                return Ok(None);
            };
            let Some(values) = self.enum_values(class)? else {
                return self.host.unknown_operand_call(r).map(Some);
            };
            return self.make_enum_set(r, values);
        }
        if r.name == "copyOf" && r.descriptor == "(Ljava/util/Collection;)Ljava/util/EnumSet;" {
            let Some(values) =
                self.host
                    .mutable_collection_values(args.first().ok_or_else(|| {
                        NativeError::Metadata("EnumSet.copyOf needs a collection".into())
                    })?)
            else {
                return Ok(None);
            };
            return self.make_enum_set(r, values);
        }
        if r.name == "range"
            && r.descriptor == "(Ljava/lang/Enum;Ljava/lang/Enum;)Ljava/util/EnumSet;"
        {
            let (Some(first), Some(last)) = (args.first(), args.get(1)) else {
                return Ok(None);
            };
            let Some(class) = self.enum_value_class(first) else {
                return Ok(None);
            };
            let Some(values) = self.enum_values(&class)? else {
                return self.host.unknown_operand_call(r).map(Some);
            };
            let first = enum_ordinal(self, first)?;
            let last = enum_ordinal(self, last)?;
            let (Some(first), Some(last)) = (first, last) else {
                return self.host.unknown_operand_call(r).map(Some);
            };
            let values = values
                .into_iter()
                .enumerate()
                .filter(|(index, _)| *index >= first && *index <= last)
                .map(|(_, value)| value)
                .collect();
            return self.make_collection("set", values).map(Some);
        }
        if r.name == "complementOf" && r.descriptor == "(Ljava/util/EnumSet;)Ljava/util/EnumSet;" {
            let Some(source) = args.first() else {
                return Ok(None);
            };
            let Some(class) = self
                .host
                .mutable_collection_values(source)
                .and_then(|values| {
                    values
                        .first()
                        .and_then(|value| self.enum_value_class(value))
                })
            else {
                return Ok(None);
            };
            let Some(values) = self.enum_values(&class)? else {
                return self.host.unknown_operand_call(r).map(Some);
            };
            let source = self
                .host
                .mutable_collection_values(source)
                .unwrap_or_default();
            let values = values
                .into_iter()
                .filter(|value| !source.iter().any(|item| item == value))
                .collect();
            return self.make_collection("set", values).map(Some);
        }
        Ok(None)
    }

    fn invoke_enum_map(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.name == "<init>" && r.descriptor == "(Ljava/lang/Class;)V" {
            let Some(class) = args.first().and_then(host_class::class_name) else {
                return Ok(None);
            };
            match self.enum_class_kind(class) {
                Some(true) => return Ok(None),
                Some(false) => {
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some(format!("{class} is not an enum type")),
                        frame: None,
                    });
                }
                None => return Ok(None),
            }
        }
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        if r.name == "toString" && r.descriptor == "()Ljava/lang/String;" {
            return self.enum_collection_text(receiver, true).map(Some);
        }
        let result = self.host.invoke_instance(r, receiver, args)?;
        if matches!(r.name.as_str(), "put" | "putAll") {
            self.sort_enum_map(receiver, r)?;
        }
        Ok(Some(result))
    }

    fn make_enum_set(
        &mut self,
        r: &MethodRef,
        values: Vec<Value>,
    ) -> Result<Option<Value>, NativeError> {
        let Some(class) = values
            .first()
            .and_then(|value| self.enum_value_class(value))
        else {
            return self.host.unknown_operand_call(r).map(Some);
        };
        if values
            .iter()
            .any(|value| self.enum_value_class(value).as_deref() != Some(class.as_str()))
        {
            return self.host.unknown_operand_call(r).map(Some);
        }
        if values.iter().any(|value| matches!(value, Value::Ref(None))) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            });
        }
        let mut values = values;
        self.sort_enum_values(&mut values, r)?;
        self.make_collection("set", values).map(Some)
    }

    pub(crate) fn enum_collection_text_for(
        &mut self,
        value: &Value,
    ) -> Result<Option<Value>, NativeError> {
        let Some(values) = self.host.mutable_collection_values(value) else {
            return Ok(None);
        };
        let class = match value {
            Value::Host(handle) if handle.kind() == HostKind::Collection => {
                jar_object::collection_class(handle.key())
            }
            Value::Host(_) => jar_object::object_class(value).unwrap_or(""),
            _ => "",
        };
        let map = class == "java/util/EnumMap";
        let set = !values.is_empty()
            && values
                .iter()
                .all(|item| self.enum_marker_for_value(item).is_some());
        if !map && !set {
            return Ok(None);
        }
        let mut text = Vec::with_capacity(values.len());
        for item in &values {
            let rendered = self
                .jar_to_string(item, "java/util/EnumSet", "toString")?
                .and_then(|item| match item {
                    Value::Ref(Some(text)) => Some(text),
                    _ => None,
                })
                .or_else(|| {
                    self.enum_marker_for_value(item)
                        .and_then(|marker| marker.rsplit_once('/').map(|(_, name)| name.to_owned()))
                })
                .unwrap_or_else(|| value_text(item));
            text.push(rendered);
        }
        let rendered = if map {
            let pairs = text
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| format!("{}={}", pair[0], pair[1]))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{pairs}}}")
        } else {
            format!("[{}]", text.join(", "))
        };
        Ok(Some(Value::Ref(Some(rendered))))
    }

    fn enum_collection_text(&mut self, receiver: &Value, map: bool) -> Result<Value, NativeError> {
        let values = self
            .host
            .mutable_collection_values(receiver)
            .ok_or_else(|| NativeError::Metadata("Enum collection has no values".into()))?;
        let mut text = Vec::with_capacity(values.len());
        for value in &values {
            let rendered = self
                .jar_to_string(value, "java/util/EnumSet", "toString")?
                .and_then(|value| match value {
                    Value::Ref(Some(text)) => Some(text),
                    _ => None,
                })
                .or_else(|| {
                    self.enum_marker_for_value(value)
                        .and_then(|marker| marker.rsplit_once('/').map(|(_, name)| name.to_owned()))
                })
                .unwrap_or_else(|| value_text(value));
            text.push(rendered);
        }
        let rendered = if map {
            let pairs = text
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| format!("{}={}", pair[0], pair[1]))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{pairs}}}")
        } else {
            format!("[{}]", text.join(", "))
        };
        Ok(Value::Ref(Some(rendered)))
    }

    fn make_collection(&mut self, kind: &str, values: Vec<Value>) -> Result<Value, NativeError> {
        let value = self
            .host
            .empty_mutable_collection(kind)
            .ok_or_else(|| NativeError::Metadata("host has no mutable collection".into()))?;
        self.host.replace_mutable_collection(&value, values);
        Ok(value)
    }

    fn sort_enum_map(&mut self, receiver: &Value, _r: &MethodRef) -> Result<(), NativeError> {
        let Some(mut values) = self.host.mutable_collection_values(receiver) else {
            return Ok(());
        };
        if values.len() % 2 != 0 {
            return Ok(());
        }
        let mut keyed = Vec::with_capacity(values.len() / 2);
        for pair in values.as_chunks::<2>().0 {
            let Some(ordinal) = enum_ordinal(self, &pair[0])? else {
                return Ok(());
            };
            keyed.push((ordinal, pair[0].clone(), pair[1].clone()));
        }
        keyed.sort_by_key(|(ordinal, _, _)| *ordinal);
        values = keyed
            .into_iter()
            .flat_map(|(_, key, value)| [key, value])
            .collect();
        self.host.replace_mutable_collection(receiver, values);
        Ok(())
    }

    fn sort_enum_set(&mut self, receiver: &Value, r: &MethodRef) -> Result<(), NativeError> {
        let Some(mut values) = self.host.mutable_collection_values(receiver) else {
            return Ok(());
        };
        self.sort_enum_values(&mut values, r)?;
        self.host.replace_mutable_collection(receiver, values);
        Ok(())
    }

    fn sort_enum_values(
        &mut self,
        values: &mut [Value],
        _r: &MethodRef,
    ) -> Result<(), NativeError> {
        let mut keyed = Vec::with_capacity(values.len());
        for value in values.iter() {
            let Some(ordinal) = enum_ordinal(self, value)? else {
                return Ok(());
            };
            keyed.push((ordinal, value.clone()));
        }
        keyed.sort_by_key(|(ordinal, _)| *ordinal);
        for (slot, (_, value)) in values.iter_mut().zip(keyed) {
            *slot = value;
        }
        Ok(())
    }

    fn enum_value_class(&self, value: &Value) -> Option<String> {
        self.enum_marker_for_value(value)?
            .strip_prefix("enum/")
            .and_then(|marker| marker.rsplit_once('/'))
            .map(|(class, _)| class.to_owned())
    }

    fn enum_marker_for_value(&self, value: &Value) -> Option<String> {
        let marker = expect_ref(value)?;
        if let Some((name, _)) = self.enum_constants.get(marker) {
            let class = self.enum_declaring_class(Some(value))?;
            return Some(format!("enum/{class}/{name}"));
        }
        marker
            .strip_prefix("enum/")
            .map(str::to_owned)
            .filter(|marker| marker.rsplit_once('/').is_some())
            .map(|marker| format!("enum/{marker}"))
    }

    fn enum_values(&mut self, class: &str) -> Result<Option<Vec<Value>>, NativeError> {
        if let Some(constants) = game_enum_constants(class) {
            return Ok(Some(
                constants
                    .iter()
                    .map(|name| Value::Ref(Some(format!("enum/{class}/{name}"))))
                    .collect(),
            ));
        }
        let Some(file) = self.class(class) else {
            return Ok(None);
        };
        if file.super_name != "java/lang/Enum" {
            return Ok(None);
        }
        let value = self.invoke(class, "values", &format!("()[L{class};"), None, &[])?;
        Ok(Some(self.array(&value)?.clone()))
    }

    fn enum_class_kind(&self, class: &str) -> Option<bool> {
        if let Some(file) = self.class(class) {
            return Some(file.super_name == "java/lang/Enum");
        }
        game_enum_constants(class).map(|_| true)
    }

    fn enum_class_supported(&self, class: &str) -> bool {
        self.enum_class_kind(class) == Some(true)
    }
}

fn enum_ordinal<H: Host>(
    runtime: &mut Runtime<'_, H>,
    value: &Value,
) -> Result<Option<usize>, NativeError> {
    let Some(marker) = runtime.enum_marker_for_value(value) else {
        return Ok(None);
    };
    let Some(value) = runtime.declared_enum_ordinal(&marker)? else {
        return Ok(None);
    };
    match value {
        Value::Int(value) if value >= 0 => Ok(Some(value as usize)),
        _ => Ok(None),
    }
}
