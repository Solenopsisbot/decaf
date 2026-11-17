//! Bounded Gson 2.14.0 reflective deserialization.
//
// The tree reader remains in gson_tree; this module supplies the ordinary
// reflective adapter used by config classes. Every shape it cannot resolve is
// handed back to the host and counted, rather than being replaced with an
// erased map or an empty object.

use super::*;
use crate::runtime::gson_tree::parse::{Parsed, Parser, Tree};

const GSON: &str = "com/google/gson/Gson";
const SERIALIZED_NAME: &str = "Lcom/google/gson/annotations/SerializedName;";
const GSON_R8_TROUBLESHOOTING: &str =
    "\nSee https://github.com/google/gson/blob/main/Troubleshooting.md#r8-abstract-class";
const MAX_GSON_DEPTH: usize = 255;
const FROM_JSON_COLLECTION: (&str, &str, &str) = (
    "com/google/gson/Gson",
    "fromJson",
    "(Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/Object;",
);

fn tree_target(class: &str) -> bool {
    matches!(
        class,
        crate::runtime::gson_tree::JSON_ELEMENT
            | crate::runtime::gson_tree::JSON_OBJECT
            | crate::runtime::gson_tree::JSON_ARRAY
            | crate::runtime::gson_tree::JSON_PRIMITIVE
            | crate::runtime::gson_tree::JSON_NULL
    )
}

fn class_from_descriptor(descriptor: &str) -> Option<String> {
    match descriptor {
        "Z" => Some("boolean".into()),
        "B" => Some("byte".into()),
        "C" => Some("char".into()),
        "S" => Some("short".into()),
        "I" => Some("int".into()),
        "J" => Some("long".into()),
        "F" => Some("float".into()),
        "D" => Some("double".into()),
        "V" => Some("void".into()),
        other if other.starts_with('L') && other.ends_with(';') => {
            Some(other[1..other.len() - 1].into())
        }
        other if other.starts_with('[') => Some(other.into()),
        _ => None,
    }
}

fn descriptor_type(descriptor: &str) -> Option<GenericType> {
    class_from_descriptor(descriptor).map(GenericType::Class)
}

fn raw_type(type_: &GenericType) -> Option<String> {
    match type_ {
        GenericType::Class(class) => Some(class.clone()),
        GenericType::Parameterized { raw, .. } => Some(raw.clone()),
        GenericType::Array(component) => Some(format!("[{}", generic_descriptor(component)?)),
        GenericType::Wildcard { upper, .. } => upper.first().and_then(raw_type),
        GenericType::Variable { .. } => None,
    }
}

fn generic_descriptor(type_: &GenericType) -> Option<String> {
    match type_ {
        GenericType::Class(class) => match class.as_str() {
            "boolean" => Some("Z".into()),
            "byte" => Some("B".into()),
            "char" => Some("C".into()),
            "short" => Some("S".into()),
            "int" => Some("I".into()),
            "long" => Some("J".into()),
            "float" => Some("F".into()),
            "double" => Some("D".into()),
            "void" => Some("V".into()),
            class if class.starts_with('[') => Some(class.into()),
            class => Some(format!("L{class};")),
        },
        GenericType::Parameterized { raw, .. } => Some(format!("L{raw};")),
        GenericType::Array(component) => Some(format!("[{}", generic_descriptor(component)?)),
        GenericType::Wildcard { .. } | GenericType::Variable { .. } => None,
    }
}

fn type_args(type_: &GenericType) -> &[GenericType] {
    match type_ {
        GenericType::Parameterized { args, .. } => args,
        _ => &[],
    }
}

fn contains_variable(type_: &GenericType) -> bool {
    match type_ {
        GenericType::Variable { .. } => true,
        GenericType::Parameterized { owner, args, .. } => {
            owner.as_deref().is_some_and(contains_variable) || args.iter().any(contains_variable)
        }
        GenericType::Wildcard { upper, lower } => {
            upper.iter().any(contains_variable) || lower.iter().any(contains_variable)
        }
        GenericType::Array(component) => contains_variable(component),
        GenericType::Class(_) => false,
    }
}

fn numeric_text(tree: &Tree) -> Option<&str> {
    match tree {
        Tree::Num(text) | Tree::Str(text) => Some(text),
        _ => None,
    }
}

fn bool_text(tree: &Tree) -> Option<bool> {
    match tree {
        Tree::Bool(value) => Some(*value),
        Tree::Str(text) => match text.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn json_object(tree: &Tree) -> Option<&[(String, Tree)]> {
    match tree {
        Tree::Object(fields) => Some(fields),
        _ => None,
    }
}

fn json_array(tree: &Tree) -> Option<&[Tree]> {
    match tree {
        Tree::Array(values) => Some(values),
        _ => None,
    }
}

fn tree_depth(tree: &Tree, depth: usize) -> usize {
    match tree {
        Tree::Array(values) => values
            .iter()
            .map(|value| tree_depth(value, depth + 1))
            .max()
            .unwrap_or(depth),
        Tree::Object(values) => values
            .iter()
            .map(|(_, value)| tree_depth(value, depth + 1))
            .max()
            .unwrap_or(depth),
        _ => depth,
    }
}

impl<H: Host> Runtime<'_, H> {
    fn gson_tree_to_parse(&mut self, value: &Value) -> Option<Tree> {
        self.gson_tree_to_parsed(value)
    }

    fn gson_collection_value(
        &mut self,
        kind: &str,
        values: Vec<Value>,
    ) -> Result<Value, NativeError> {
        let class = match kind {
            "map" => crate::runtime::gson_tree::TREE_MAP,
            "set" => "java/util/LinkedHashSet",
            _ => "java/util/ArrayList",
        };
        self.new_host_collection(class, values, FROM_JSON_COLLECTION)
    }

    fn count_gson_fromjson(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        self.host
            .invoke_instance(r, receiver.unwrap_or(&Value::Ref(None)), args)
            .map(Some)
    }

    fn gson_source(&self, value: Option<&Value>, reader: bool) -> Option<String> {
        let value = value?;
        match value {
            Value::Ref(None) => None,
            Value::Ref(Some(text)) if !reader => Some(text.clone()),
            value if reader => crate::runtime::jar_object::object_key(value)
                .or_else(|| crate::runtime::expect_ref(value).map(String::as_str))
                .and_then(|marker| self.string_readers.get(marker))
                .cloned(),
            _ => None,
        }
    }

    fn gson_type_for_field(
        &self,
        declaring: &str,
        name: &str,
        descriptor: &str,
    ) -> Option<GenericType> {
        let file = self.class(declaring)?;
        let parsed = file
            .field_signature(name, descriptor)
            .and_then(|signature| {
                parse_generic_signature(
                    &signature,
                    crate::runtime::reflect_generic::class_scope(declaring, Some(file)),
                )
            });
        parsed.or_else(|| descriptor_type(descriptor))
    }

    pub(crate) fn gson_serialized_names(
        &self,
        declaring: &str,
        name: &str,
        descriptor: &str,
        style: &gson_model::GsonStyle,
    ) -> Vec<String> {
        let mut names = vec![style.field_name(name)];
        let Some(file) = self.class(declaring) else {
            return names;
        };
        for annotation in file.field_annotations(name, descriptor) {
            if annotation.descriptor != SERIALIZED_NAME {
                continue;
            }
            if let Some(AnnotationValue::String(primary)) = annotation.values.get("value") {
                names[0] = primary.clone();
            }
            if let Some(AnnotationValue::Array(alternates)) = annotation.values.get("alternate") {
                names.extend(alternates.iter().filter_map(|value| match value {
                    AnnotationValue::String(value) => Some(value.clone()),
                    _ => None,
                }));
            }
        }
        names
    }

    fn gson_json_io(message: String) -> NativeError {
        NativeError::UncaughtException {
            class: "com/google/gson/JsonIOException".into(),
            message: Some(message),
            frame: None,
        }
    }

    fn gson_record(
        &mut self,
        fields: &[(String, Tree)],
        class: &str,
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        let Some(file) = self.class(class) else {
            return Ok(None);
        };
        let components: Vec<_> = file
            .fields
            .iter()
            .filter(|field| field.access & 0x0008 == 0)
            .cloned()
            .collect();
        let has_constructor = file.methods.iter().any(|method| {
            method.name == "<init>" && {
                let descriptor = format!(
                    "({})V",
                    components
                        .iter()
                        .map(|field| field.descriptor.as_str())
                        .collect::<String>()
                );
                method.descriptor == descriptor
            }
        });
        if components.is_empty() {
            return Ok(None);
        }
        let mut args = Vec::with_capacity(components.len());
        for field in &components {
            let names = self.gson_serialized_names(class, &field.name, &field.descriptor, style);
            let raw = names.iter().find_map(|name| {
                fields
                    .iter()
                    .find(|(candidate, _)| candidate == name)
                    .map(|(_, value)| value)
            });
            let value = match raw {
                Some(raw) => {
                    let Some(type_) =
                        self.gson_type_for_field(class, &field.name, &field.descriptor)
                    else {
                        return Ok(None);
                    };
                    let Some(value) = self.gson_materialize_type(raw, &type_, style)? else {
                        return Ok(None);
                    };
                    value
                }
                None => Value::default_for(&field.descriptor),
            };
            args.push(value);
        }
        let descriptor = format!(
            "({})V",
            components
                .iter()
                .map(|field| field.descriptor.as_str())
                .collect::<String>()
        );
        if !has_constructor {
            return Ok(None);
        }
        self.ensure_initialized(class)?;
        let value = self.new_object(class);
        self.invoke(class, "<init>", &descriptor, Some(value.clone()), &args)?;
        Ok(Some(value))
    }

    fn gson_object(
        &mut self,
        fields: &[(String, Tree)],
        class: &str,
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        if class.starts_with("java/") || !self.has_class(class) {
            return Ok(None);
        }
        let Some(file) = self.class(class) else {
            return Ok(None);
        };
        let flags = crate::runtime::jdk_model::class_access_flags(file).unwrap_or_default();
        if flags & 0x0200 != 0 {
            return Err(Self::gson_json_io(format!(
                "Interfaces can't be instantiated! Register an InstanceCreator or a TypeAdapter for this type. Interface name: {}",
                class.replace('/', ".")
            )));
        }
        if flags & 0x0400 != 0 {
            return Err(Self::gson_json_io(format!(
                "Abstract classes can't be instantiated! Adjust the R8 configuration or register an InstanceCreator or a TypeAdapter for this type. Class name: {}{GSON_R8_TROUBLESHOOTING}",
                class.replace('/', ".")
            )));
        }
        let is_record = file.super_name == "java/lang/Record" || flags & 0x1000 != 0;
        if is_record {
            if let Some(value) = self.gson_record(fields, class, style)? {
                return Ok(Some(value));
            }
            return Ok(None);
        }
        let declared = self.gson_fields(class);
        if !declared
            .iter()
            .any(|(_, _, _, access)| access & 0x0008 == 0 && style.includes_field(*access))
        {
            return Ok(None);
        }
        if declared.iter().any(|(owner, name, descriptor, _)| {
            self.gson_type_for_field(owner, name, descriptor)
                .is_some_and(|type_| contains_variable(&type_))
        }) {
            return Ok(None);
        }
        self.ensure_initialized(class)?;
        let value = self.new_object(class);
        let has_noarg = self.class(class).is_some_and(|file| {
            file.methods
                .iter()
                .any(|method| method.name == "<init>" && method.descriptor == "()V")
        });
        if has_noarg {
            self.invoke(class, "<init>", "()V", Some(value.clone()), &[])?;
        }
        for (declaring, name, descriptor, access) in declared {
            if !style.includes_field(access) {
                continue;
            }
            let matching: Vec<String> = self
                .gson_serialized_names(&declaring, &name, &descriptor, style)
                .into_iter()
                .filter(|key| fields.iter().any(|(candidate, _)| candidate == key))
                .collect();
            if matching.len() > 1 {
                return Ok(None);
            }
            let key = matching.into_iter().next();
            let Some(key) = key else {
                continue;
            };
            let Some(raw) = fields
                .iter()
                .find(|(candidate, _)| candidate == &key)
                .map(|(_, value)| value)
            else {
                continue;
            };
            let Some(type_) = self.gson_type_for_field(&declaring, &name, &descriptor) else {
                return Ok(None);
            };
            let Some(field_value) = self.gson_materialize_type(raw, &type_, style)? else {
                return Ok(None);
            };
            let field = MethodRef {
                owner: declaring,
                name,
                descriptor,
            };
            if access & 0x0008 != 0 {
                self.static_fields
                    .insert(Self::static_field_key(&field), field_value);
            } else if let Some(key) = instance_field_key(&value, &field) {
                self.fields.insert(key, field_value);
            }
        }
        Ok(Some(value))
    }

    fn gson_materialize_type(
        &mut self,
        tree: &Tree,
        type_: &GenericType,
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        if contains_variable(type_) {
            return Ok(None);
        }
        let Some(class) = raw_type(type_) else {
            return Ok(None);
        };
        let args = type_args(type_);
        if tree_matches_null(tree) {
            return Ok(Some(Value::Ref(None)));
        }
        match class.as_str() {
            "java/lang/Object" => return self.gson_object_value_reflective(tree),
            "java/lang/String" => {
                return Ok(numeric_text(tree)
                    .map(|text| Value::Ref(Some(text.to_owned())))
                    .or_else(|| bool_text(tree).map(|value| Value::Ref(Some(value.to_string())))));
            }
            "boolean" | "java/lang/Boolean" => {
                // JVM boolean fields and method arguments use the int 0/1
                // representation. Only the reference wrapper is a boxed
                // Boolean; handing that box to Boolean.valueOf(Z) makes the
                // interpreter reject the operand just as #313 did in Noisium.
                return Ok(bool_text(tree).map(|value| {
                    if class == "boolean" {
                        Value::Int(i32::from(value))
                    } else {
                        boolean_value(value)
                    }
                }));
            }
            "byte" | "java/lang/Byte" => {
                return Ok(numeric_text(tree)
                    .and_then(|text| text.parse::<i8>().ok())
                    .map(|value| Value::Int(i32::from(value))));
            }
            "short" | "java/lang/Short" => {
                return Ok(numeric_text(tree)
                    .and_then(|text| text.parse::<i16>().ok())
                    .map(|value| Value::Int(i32::from(value))));
            }
            "int" | "java/lang/Integer" => {
                return Ok(numeric_text(tree)
                    .and_then(|text| text.parse::<i32>().ok())
                    .map(Value::Int));
            }
            "long" | "java/lang/Long" => {
                return Ok(numeric_text(tree)
                    .and_then(|text| text.parse::<i64>().ok())
                    .map(Value::Long));
            }
            "float" | "java/lang/Float" => {
                return Ok(numeric_text(tree)
                    .and_then(|text| text.parse::<f64>().ok())
                    .map(Value::Float));
            }
            "double" | "java/lang/Double" => {
                return Ok(numeric_text(tree)
                    .and_then(|text| text.parse::<f64>().ok())
                    .map(Value::Float));
            }
            "char" | "java/lang/Character" => {
                return Ok(numeric_text(tree).and_then(|text| {
                    let mut chars = text.chars();
                    let value = chars.next()?;
                    chars.next().is_none().then_some(Value::Int(value as i32))
                }));
            }
            _ => {}
        }
        if let Some(component_desc) = class.strip_prefix('[') {
            let Some(values) = json_array(tree) else {
                return Ok(None);
            };
            let Some(component) = descriptor_type(component_desc) else {
                return Ok(None);
            };
            let mut materialized = Vec::with_capacity(values.len());
            for value in values {
                let Some(value) = self.gson_materialize_type(value, &component, style)? else {
                    return Ok(None);
                };
                materialized.push(value);
            }
            return Ok(Some(self.new_array_value(&class, materialized)));
        }
        if is_collection_owner(&class) && !is_map_owner(&class) {
            let Some(values) = json_array(tree) else {
                return Ok(None);
            };
            let element = args
                .first()
                .cloned()
                .unwrap_or_else(|| GenericType::Class("java/lang/Object".into()));
            let mut materialized = Vec::with_capacity(values.len());
            for value in values {
                let Some(value) = self.gson_materialize_type(value, &element, style)? else {
                    return Ok(None);
                };
                materialized.push(value);
            }
            return Ok(Some(self.gson_collection_value(
                if is_set_owner(&class) { "set" } else { "list" },
                materialized,
            )?));
        }
        if is_map_owner(&class) {
            let Some(values) = json_object(tree) else {
                return Ok(None);
            };
            let element = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| GenericType::Class("java/lang/Object".into()));
            let key_type = args
                .first()
                .cloned()
                .unwrap_or_else(|| GenericType::Class("java/lang/String".into()));
            let key_class = raw_type(&key_type).unwrap_or_default();
            let mut materialized = Vec::with_capacity(values.len() * 2);
            for (key, value) in values {
                let key_value = if let Some(read) =
                    self.gson_read_jar_enum(&Tree::Str(key.clone()), &key_class)?
                {
                    read.unwrap_or(Value::Ref(None))
                } else {
                    Value::Ref(Some(key.clone()))
                };
                materialized.push(key_value);
                let Some(value) = self.gson_materialize_type(value, &element, style)? else {
                    return Ok(None);
                };
                materialized.push(value);
            }
            return Ok(Some(self.gson_collection_value("map", materialized)?));
        }
        if self.has_class(&class) {
            if let Some(value) = self.gson_read_jar_enum(tree, &class)? {
                return Ok(value);
            }
            let Some(values) = json_object(tree) else {
                return Ok(None);
            };
            return self.gson_object(values, &class, style);
        }
        Ok(gson_scalar_value(tree))
    }

    fn gson_object_value_reflective(&mut self, tree: &Tree) -> Result<Option<Value>, NativeError> {
        Ok(match tree {
            Tree::Null => Some(Value::Ref(None)),
            Tree::Bool(value) => Some(boolean_value(*value)),
            Tree::Num(text) => {
                if let Ok(value) = text.parse::<i32>() {
                    Some(Value::Int(value))
                } else if let Ok(value) = text.parse::<i64>() {
                    Some(Value::Long(value))
                } else {
                    text.parse::<f64>().ok().map(Value::Float)
                }
            }
            Tree::Str(text) => Some(Value::Ref(Some(text.clone()))),
            Tree::Array(values) => {
                let mut materialized = Vec::with_capacity(values.len());
                for value in values {
                    let Some(value) = self.gson_object_value_reflective(value)? else {
                        return Ok(None);
                    };
                    materialized.push(value);
                }
                Some(self.gson_collection_value("list", materialized)?)
            }
            Tree::Object(values) => {
                let mut materialized = Vec::with_capacity(values.len() * 2);
                for (key, value) in values {
                    materialized.push(Value::Ref(Some(key.clone())));
                    let Some(value) = self.gson_object_value_reflective(value)? else {
                        return Ok(None);
                    };
                    materialized.push(value);
                }
                Some(self.gson_collection_value("map", materialized)?)
            }
        })
    }

    fn gson_fromjson_type(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != GSON || r.name != "fromJson" {
            return Ok(None);
        }
        let (reader, type_arg) = match r.descriptor.as_str() {
            "(Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/Object;"
            | "(Ljava/io/Reader;Ljava/lang/Class;)Ljava/lang/Object;"
            | "(Lcom/google/gson/JsonElement;Ljava/lang/Class;)Ljava/lang/Object;" => {
                (r.descriptor.starts_with("(Ljava/io/Reader;"), args.get(1))
            }
            "(Ljava/lang/String;Ljava/lang/reflect/Type;)Ljava/lang/Object;"
            | "(Ljava/io/Reader;Ljava/lang/reflect/Type;)Ljava/lang/Object;" => {
                (r.descriptor.starts_with("(Ljava/io/Reader;"), args.get(1))
            }
            "(Ljava/lang/String;Lcom/google/gson/reflect/TypeToken;)Ljava/lang/Object;"
            | "(Ljava/io/Reader;Lcom/google/gson/reflect/TypeToken;)Ljava/lang/Object;"
            | "(Lcom/google/gson/JsonElement;Lcom/google/gson/reflect/TypeToken;)Ljava/lang/Object;" => {
                (r.descriptor.starts_with("(Ljava/io/Reader;"), args.get(1))
            }
            "(Lcom/google/gson/JsonElement;Ljava/lang/reflect/Type;)Ljava/lang/Object;" => {
                (false, args.get(1))
            }
            _ => return Ok(None),
        };
        let Some(type_arg) = type_arg else {
            return self.count_gson_fromjson(r, receiver, args);
        };
        let target = if r.descriptor.contains("Ljava/lang/Class;") {
            let Some(class) = crate::runtime::host_class::class_name(type_arg) else {
                return self.count_gson_fromjson(r, receiver, args);
            };
            GenericType::Class(class.to_owned())
        } else if r.descriptor.contains("TypeToken") {
            let Some(type_value) = self.gson_token_type(type_arg) else {
                return self.count_gson_fromjson(r, receiver, args);
            };
            let Some(type_) = crate::runtime::host_gson_type::any_node(&type_value) else {
                return self.count_gson_fromjson(r, receiver, args);
            };
            type_
        } else {
            let Some(type_) = crate::runtime::host_gson_type::any_node(type_arg) else {
                return self.count_gson_fromjson(r, receiver, args);
            };
            type_
        };
        let Some(class) = raw_type(&target) else {
            return self.count_gson_fromjson(r, receiver, args);
        };
        // The #253 typed reader owns JDK scalars, Object, collections, arrays,
        // and tree classes. Leave those targets to it so Gson's Double policy,
        // nesting bound, and duplicate-name boundary remain single-source.
        if class.starts_with("java/") || class.starts_with('[') {
            return Ok(None);
        }
        let element_input = r.descriptor.starts_with("(Lcom/google/gson/JsonElement;");
        if tree_target(&class) && !element_input {
            return Ok(None);
        }
        let style = self.gson_style_of(receiver);
        if style.is_counted() {
            return self.count_gson_fromjson(r, receiver, args);
        }
        if !element_input {
            let reflective_special = self.class(&class).is_some_and(|file| {
                let flags = crate::runtime::jdk_model::class_access_flags(file).unwrap_or_default();
                file.super_name == "java/lang/Record"
                    || flags & (0x0200 | 0x0400) != 0
                    || flags & 0x1000 != 0
            });
            if !reflective_special {
                if !reader && matches!(args.first(), Some(Value::Ref(None))) {
                    return Ok(Some(Value::Ref(None)));
                }
                let Some(text) = self.gson_source(args.first(), reader) else {
                    return self.count_gson_fromjson(r, receiver, args);
                };
                if reader && let Some(marker) = args.first().and_then(crate::runtime::jar_object::object_key)
                {
                    self.string_readers.remove(marker);
                }
                return match self.gson_read_text(&text, &target, &style)? {
                    Some(value) => Ok(Some(value)),
                    None => self.count_gson_fromjson(r, receiver, args),
                };
            }
        }
        let parsed = if element_input {
            let Some(element) = args.first() else {
                return self.count_gson_fromjson(r, receiver, args);
            };
            if *element == Value::Ref(None) {
                return Ok(Some(Value::Ref(None)));
            }
            let Some(tree) = self.gson_tree_to_parse(element) else {
                return self.count_gson_fromjson(r, receiver, args);
            };
            tree
        } else {
            if reader && matches!(args.first(), Some(Value::Ref(None))) {
                return self.count_gson_fromjson(r, receiver, args);
            }
            let Some(text) = self.gson_source(args.first(), reader) else {
                return self.count_gson_fromjson(r, receiver, args);
            };
            let (parsed, duplicate) = Parser::parse_noting_duplicates(&text);
            if duplicate {
                // Gson's map adapter throws its duplicate-key JsonSyntaxException;
                // this bounded adapter counts the same unsupported shape rather
                // than silently collapsing the repeated member.
                return Ok(None);
            }
            match parsed {
                Parsed::Empty => return Ok(Some(Value::Ref(None))),
                Parsed::Value(tree) => tree,
                // Keep the established bounded parser's diagnostic for malformed,
                // BOM-prefixed and otherwise unsupported input.
                Parsed::Unsupported => return Ok(None),
            }
        };
        if tree_depth(&parsed, 0) > MAX_GSON_DEPTH {
            return Ok(None);
        }
        match self.gson_materialize_type(&parsed, &target, &style)? {
            Some(value) => Ok(Some(value)),
            None => self.count_gson_fromjson(r, receiver, args),
        }
    }

    pub(super) fn invoke_gson_fromjson(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        self.gson_fromjson_type(r, receiver, args)
    }
}

fn tree_matches_null(tree: &Tree) -> bool {
    matches!(tree, Tree::Null)
}

fn gson_scalar_value(tree: &Tree) -> Option<Value> {
    match tree {
        Tree::Bool(value) => Some(boolean_value(*value)),
        Tree::Num(text) => text
            .parse::<i32>()
            .map(Value::Int)
            .or_else(|_| text.parse::<i64>().map(Value::Long))
            .or_else(|_| text.parse::<f64>().map(Value::Float))
            .ok(),
        Tree::Str(text) => Some(Value::Ref(Some(text.clone()))),
        _ => None,
    }
}
