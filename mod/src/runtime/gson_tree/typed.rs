//! `Gson.fromJson` into a declared type that is not one of Gson's tree classes:
//! a JDK collection, map or array, `Object`, `String`, a box or primitive, or a
//! jar class whose fields are read the same way (#253 re-check).
//!
//! The document is read with this module's parent's parser, which keeps an
//! object's names in document order and every number's text, so each value can
//! go through the adapter gson-2.14.0 picks for the declared type:
//!
//! * the type is the class literal, the `Type` argument, the `TypeToken`'s
//!   type, or a field's generic `Signature` (its erased descriptor when it has
//!   none), so `List<Integer>` reads its elements as `Integer` and a raw `List`
//!   reads them as `Object`;
//! * `Object` is `ObjectTypeAdapter` with `ToNumberPolicy.DOUBLE`: a `String`,
//!   a `Double`, a `Boolean`, `null`, an `ArrayList`, a `LinkedTreeMap` in
//!   document order;
//! * `String`, the boxes and the primitives are `TypeAdapters`' readers,
//!   `JsonReader.nextInt`/`nextLong`/`nextDouble`/`nextString`'s rules
//!   included (`1.0` is an `int` 1, `1.5` throws);
//! * a collection or map is the class `ConstructorConstructor` makes for it
//!   (the class itself when concrete; `TreeSet`, `LinkedHashSet`, `ArrayList`,
//!   `TreeMap`, and for `Map` a `LinkedTreeMap`, or a `LinkedHashMap` when a
//!   parameterized key type is no `String`), filled through its own
//!   `add`/`put`;
//! * an array is its component's reader per element.
//!
//! The rule is Gson's exact answer or a counted unknown (`None` from every
//! reader here, which the `fromJson` call counts), never an approximation.
//! Counted: a type variable, a class this does not list (`Queue`'s
//! `ArrayDeque`, `EnumSet`, `ConcurrentHashMap`, `Iterable`, which Gson
//! refuses with a `JsonIOException`, ...), a value of the wrong kind
//! for its reader (Gson's `JsonSyntaxException`), a `null` in a primitive
//! array, a `TreeSet`/`TreeMap` whose elements are not one comparable kind
//! (Java's `ClassCastException`), a map key read as anything but a `String` or
//! an integral box, a map written as an array of pairs, and any document that
//! repeats a name in one object (Gson's map adapter throws "duplicate key").

use super::parse::{Parsed, Parser, Tree};
use super::*;

/// The call a read is counted as when no adapter here reproduces it.
const FROM_JSON: (&str, &str, &str) = (
    "com/google/gson/Gson",
    "fromJson",
    "(Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/Object;",
);

/// `Integer.parseInt`, then `JsonReader.nextInt`'s double fallback: an
/// integral `double` in range.
fn json_int(text: &str) -> Option<i32> {
    match text.parse::<i64>() {
        Ok(value) => i32::try_from(value).ok(),
        Err(_) => {
            let double = json_double(text)?;
            let int = double as i32;
            (f64::from(int) == double).then_some(int)
        }
    }
}

/// `Long.parseLong`, then `JsonReader.nextLong`'s double fallback.
fn json_long(text: &str) -> Option<i64> {
    match text.parse::<i64>() {
        Ok(value) => Some(value),
        Err(_) => {
            let double = json_double(text)?;
            let long = double as i64;
            (long as f64 == double).then_some(long)
        }
    }
}

/// `Double.parseDouble` of a JSON number's text. Text of any other shape is
/// refused: Java's parser and Rust's accept different spellings beyond it.
fn json_double(text: &str) -> Option<f64> {
    match text {
        "NaN" => return Some(f64::NAN),
        "Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    json_number(text)?;
    text.parse::<f64>().ok()
}

/// A reader's view of a scalar token: a number's or a string's text.
fn scalar_text(tree: &Tree) -> Option<&str> {
    match tree {
        Tree::Num(text) | Tree::Str(text) => Some(text),
        _ => None,
    }
}

/// The type a reader reads for `ty`: a wildcard is its upper bound (`?` and
/// `? super T` are `Object`, as `$Gson$Types` reads them); a type variable is
/// not resolved here.
fn resolved(ty: &GenericType) -> Option<GenericType> {
    match ty {
        GenericType::Wildcard { upper, lower } => match (upper.as_slice(), lower.is_empty()) {
            ([bound], true) => resolved(bound),
            ([], _) | (_, false) => Some(GenericType::Class("java/lang/Object".into())),
            _ => None,
        },
        GenericType::Variable { .. } => None,
        other => Some(other.clone()),
    }
}

/// The erased class of a resolved type (`List<String>` is `java/util/List`).
fn raw_class(ty: &GenericType) -> Option<&str> {
    match ty {
        GenericType::Class(name) => Some(name),
        GenericType::Parameterized { raw, .. } => Some(raw),
        _ => None,
    }
}

/// The JVM descriptor of an array component.
fn descriptor_of(ty: &GenericType) -> Option<String> {
    Some(match ty {
        GenericType::Class(name) if name.starts_with('[') => name.clone(),
        GenericType::Class(name) => match name.as_str() {
            "boolean" => "Z".into(),
            "byte" => "B".into(),
            "char" => "C".into(),
            "short" => "S".into(),
            "int" => "I".into(),
            "long" => "J".into(),
            "float" => "F".into(),
            "double" => "D".into(),
            other => format!("L{other};"),
        },
        GenericType::Parameterized { raw, .. } => format!("L{raw};"),
        GenericType::Array(component) => format!("[{}", descriptor_of(component)?),
        _ => return None,
    })
}

fn is_primitive(class: &str) -> bool {
    matches!(
        class,
        "boolean" | "byte" | "char" | "short" | "int" | "long" | "float" | "double"
    )
}

/// The class `ConstructorConstructor` makes for a collection type, or `None`
/// where this does not reproduce Gson (counted).
fn collection_class(class: &str) -> Option<&'static str> {
    Some(match class {
        "java/util/ArrayList" => "java/util/ArrayList",
        "java/util/LinkedList" => "java/util/LinkedList",
        "java/util/HashSet" => "java/util/HashSet",
        "java/util/LinkedHashSet" => "java/util/LinkedHashSet",
        "java/util/TreeSet" => "java/util/TreeSet",
        "java/util/SortedSet" | "java/util/NavigableSet" => "java/util/TreeSet",
        "java/util/Set" | "java/util/AbstractSet" | "java/util/SequencedSet" => {
            "java/util/LinkedHashSet"
        }
        "java/util/Collection"
        | "java/util/List"
        | "java/util/AbstractCollection"
        | "java/util/AbstractList"
        | "java/util/SequencedCollection" => "java/util/ArrayList",
        _ => return None,
    })
}

/// The class `ConstructorConstructor` makes for a map type. `string_keys`:
/// the type is raw, or its key type's raw class is `String`.
fn map_class(class: &str, string_keys: bool) -> Option<&'static str> {
    Some(match class {
        "java/util/HashMap" => "java/util/HashMap",
        "java/util/LinkedHashMap" => "java/util/LinkedHashMap",
        "java/util/TreeMap" => "java/util/TreeMap",
        "java/util/SortedMap" | "java/util/NavigableMap" => "java/util/TreeMap",
        "java/util/Map" | "java/util/AbstractMap" if string_keys => TREE_MAP,
        "java/util/Map" | "java/util/AbstractMap" => "java/util/LinkedHashMap",
        _ => return None,
    })
}

/// The one comparable kind a sorted collection's elements share, or `None`
/// (a `null`, or two kinds `compareTo` cannot order: Java's exception).
fn one_comparable_kind(values: &[Value]) -> bool {
    let kind = |value: &Value| -> Option<u8> {
        Some(match value {
            Value::Ref(Some(text)) if crate::runtime::value_class::is_plain_string(text) => 0,
            Value::Int(_) | Value::Boxed(Boxed::Integer(_)) => 1,
            Value::Long(_) => 2,
            Value::Float(_) => 3,
            Value::Boxed(Boxed::Boolean(_)) => 4,
            _ => return None,
        })
    };
    let mut kinds = values.iter().map(kind);
    match kinds.next() {
        None => true,
        Some(None) => false,
        Some(first) => kinds.all(|kind| kind == first),
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `Gson.fromJson(text, target)` for a target that is not a tree class:
    /// `None` where Gson's answer is not reproduced, for the caller to count.
    pub(crate) fn gson_read_text(
        &mut self,
        text: &str,
        target: &GenericType,
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        match Parser::parse_noting_duplicates_with_special_floats(
            text,
            style.allows_special_floats(),
        ) {
            // An empty document is `null`.
            (Parsed::Empty, _) => Ok(Some(Value::Ref(None))),
            (Parsed::Value(tree), false) => self.gson_read(&tree, target, style),
            // Read as `Object`, the reader reaches the 256th bracket before
            // any adapter can refuse a shape. Another target's adapter may
            // throw first, and a repeated name may matter: counted.
            (Parsed::TooDeep(message), false) if matches!(target, GenericType::Class(class) if class == "java/lang/Object") => {
                Err(self.json_syntax_exception(message))
            }
            (Parsed::Value(_), true) | (Parsed::TooDeep(_), _) | (Parsed::Unsupported, _) => {
                Ok(None)
            }
        }
    }

    /// One value read as `ty`.
    fn gson_read(
        &mut self,
        tree: &Tree,
        ty: &GenericType,
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        let Some(ty) = resolved(ty) else {
            return Ok(None);
        };
        if let GenericType::Array(component) = &ty {
            return self.gson_read_array(tree, component, style);
        }
        let Some(class) = raw_class(&ty) else {
            return Ok(None);
        };
        if let Some(component) = class.strip_prefix('[') {
            let component = GenericType::Class(crate::runtime::erased_field_type(component));
            return self.gson_read_array(tree, &component, style);
        }
        if let Tree::Null = tree {
            // A primitive's reader reads `null` as `null`, which cannot be
            // stored (counted).
            return Ok((!is_primitive(class)).then_some(Value::Ref(None)));
        }
        let args: &[GenericType] = match &ty {
            GenericType::Parameterized { args, .. } => args,
            _ => &[],
        };
        let class = class.to_owned();
        // Gson's STRICT reader rejects non-finite values for every numeric
        // adapter and ObjectTypeAdapter. StringAdapter still reads the token
        // text, so retain that exact result.
        if !style.allows_special_floats()
            && class != "java/lang/String"
            && matches!(
                tree,
                Tree::Num(text)
                    if json_double(text).is_some_and(|value| !value.is_finite())
            )
        {
            return Ok(None);
        }
        if class == "java/lang/Object" {
            return self.gson_object_value(tree);
        }
        if let Some(value) = gson_scalar(tree, &class) {
            return Ok(value);
        }
        if tree_class(&class) {
            let fits = matches!(
                (tree, class.as_str()),
                (_, JSON_ELEMENT)
                    | (Tree::Object(_), JSON_OBJECT)
                    | (Tree::Array(_), JSON_ARRAY)
                    | (Tree::Str(_) | Tree::Num(_) | Tree::Bool(_), JSON_PRIMITIVE)
            );
            return if fits {
                self.materialize(tree.clone()).map(Some)
            } else {
                Ok(None)
            };
        }
        if map_class(&class, true).is_some() {
            return self.gson_read_map(tree, &class, args, style);
        }
        if let Some(made) = collection_class(&class) {
            let element = match args {
                [] => GenericType::Class("java/lang/Object".into()),
                [element] => element.clone(),
                _ => return Ok(None),
            };
            return self.gson_read_collection(tree, made, &element, style);
        }
        if crate::runtime::is_collection_owner(&class) || class == "java/lang/Iterable" {
            return Ok(None);
        }
        if !class.starts_with("java/") && self.has_class(&class) {
            return self.gson_read_jar_object(tree, &class, style);
        }
        Ok(None)
    }

    /// `ObjectTypeAdapter`.
    fn gson_object_value(&mut self, tree: &Tree) -> Result<Option<Value>, NativeError> {
        Ok(match tree {
            Tree::Null => Some(Value::Ref(None)),
            Tree::Str(text) => Some(Value::Ref(Some(text.clone()))),
            Tree::Bool(flag) => Some(Value::Boxed(Boxed::Boolean(*flag))),
            // Gson's ObjectTypeAdapter reads lenient unquoted NaN/Infinity
            // through JsonReader.nextString(), so these special tokens remain
            // Strings for Object. Ordinary JSON numbers, including overflow,
            // use the default Double policy.
            Tree::Num(text) if matches!(text.as_str(), "NaN" | "Infinity" | "-Infinity") => {
                Some(Value::Ref(Some(text.clone())))
            }
            Tree::Num(text) => json_double(text).map(Value::Float),
            Tree::Array(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    let Some(value) = self.gson_object_value(element)? else {
                        return Ok(None);
                    };
                    values.push(value);
                }
                Some(self.new_host_collection("java/util/ArrayList", values, FROM_JSON)?)
            }
            Tree::Object(members) => {
                let map = self.new_host_collection(TREE_MAP, Vec::new(), FROM_JSON)?;
                for (name, member) in members {
                    let Some(value) = self.gson_object_value(member)? else {
                        return Ok(None);
                    };
                    self.gson_put(&map, Value::Ref(Some(name.clone())), value)?;
                }
                Some(map)
            }
        })
    }

    fn gson_put(&mut self, map: &Value, key: Value, value: Value) -> Result<Value, NativeError> {
        self.call_virtual(
            crate::runtime::jdk_call::Site::Interface,
            "java/util/Map",
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            map,
            &[key, value],
        )
    }

    /// `ArrayTypeAdapter`: each element through the component's reader.
    fn gson_read_array(
        &mut self,
        tree: &Tree,
        component: &GenericType,
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        let elements = match tree {
            Tree::Null => return Ok(Some(Value::Ref(None))),
            Tree::Array(elements) => elements,
            _ => return Ok(None),
        };
        let Some(component) = resolved(component) else {
            return Ok(None);
        };
        let Some(descriptor) = descriptor_of(&component) else {
            return Ok(None);
        };
        let primitive = descriptor.len() == 1;
        let mut values = Vec::with_capacity(elements.len());
        for element in elements {
            let value = match self.gson_read(element, &component, style)? {
                // `Array.set` of a primitive array refuses `null`.
                Some(Value::Ref(None)) if primitive => return Ok(None),
                Some(value) => value,
                None => return Ok(None),
            };
            values.push(value);
        }
        Ok(Some(
            self.new_array_value(&format!("[{descriptor}"), values),
        ))
    }

    /// `CollectionTypeAdapterFactory`: the collection Gson makes, each element
    /// through `add`.
    fn gson_read_collection(
        &mut self,
        tree: &Tree,
        made: &str,
        element: &GenericType,
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        let Tree::Array(elements) = tree else {
            return Ok(None);
        };
        let mut values = Vec::with_capacity(elements.len());
        for element_tree in elements {
            let Some(value) = self.gson_read(element_tree, element, style)? else {
                return Ok(None);
            };
            values.push(value);
        }
        if made == "java/util/TreeSet" && !one_comparable_kind(&values) {
            return Ok(None);
        }
        if matches!(made, "java/util/ArrayList" | "java/util/LinkedList") {
            return self.new_host_collection(made, values, FROM_JSON).map(Some);
        }
        // A set takes each element through its own `add`: a `TreeSet` sorts,
        // every set drops a repeat.
        let set = self.new_host_collection(made, Vec::new(), FROM_JSON)?;
        for value in values {
            self.call_virtual(
                crate::runtime::jdk_call::Site::Interface,
                "java/util/Set",
                "add",
                "(Ljava/lang/Object;)Z",
                &set,
                &[value],
            )?;
        }
        Ok(Some(set))
    }

    /// `MapTypeAdapterFactory` reading an object: each name through the key
    /// type's reader, each value through the value type's, then `put`.
    fn gson_read_map(
        &mut self,
        tree: &Tree,
        class: &str,
        args: &[GenericType],
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        // The array-of-pairs form (`[[k, v], ...]`) is not reproduced.
        let Tree::Object(members) = tree else {
            return Ok(None);
        };
        let object = GenericType::Class("java/lang/Object".into());
        let (key, value) = match args {
            [] => (object.clone(), object),
            [key, value] => {
                let (Some(key), Some(value)) = (resolved(key), resolved(value)) else {
                    return Ok(None);
                };
                (key, value)
            }
            _ => return Ok(None),
        };
        let key_class = raw_class(&key).unwrap_or_default().to_owned();
        let Some(made) = map_class(class, args.is_empty() || key_class == "java/lang/String")
        else {
            return Ok(None);
        };
        let mut keys = Vec::with_capacity(members.len());
        let mut values = Vec::with_capacity(members.len());
        for (name, member) in members {
            // The key reader reads the name as a string token.
            let key = if let Some(read) =
                self.gson_read_jar_enum(&Tree::Str(name.clone()), &key_class)?
            {
                // EnumTypeAdapter returns null for an unknown name. Gson's
                // ordinary map adapter accepts that null key; a custom
                // toString path remains counted by the enum reader.
                read.unwrap_or(Value::Ref(None))
            } else {
                match key_class.as_str() {
                    "java/lang/String" | "java/lang/Object" => Value::Ref(Some(name.clone())),
                    "java/lang/Integer" => match json_int(name) {
                        Some(int) => Value::Int(int),
                        None => return Ok(None),
                    },
                    "java/lang/Long" => match json_long(name) {
                        Some(long) => Value::Long(long),
                        None => return Ok(None),
                    },
                    _ => return Ok(None),
                }
            };
            // Two names reading as one key: Gson's "duplicate key".
            if keys.contains(&key) {
                return Ok(None);
            }
            let Some(read) = self.gson_read(member, &value, style)? else {
                return Ok(None);
            };
            keys.push(key);
            values.push(read);
        }
        if made == "java/util/TreeMap" && !one_comparable_kind(&keys) {
            return Ok(None);
        }
        let map = self.new_host_collection(made, Vec::new(), FROM_JSON)?;
        for (key, value) in keys.into_iter().zip(values) {
            self.gson_put(&map, key, value)?;
        }
        Ok(Some(map))
    }

    /// `ReflectiveTypeAdapterFactory` for a jar class: the object with each
    /// field the document names read as the field's declared type. A jar enum
    /// is `TypeAdapters.EnumTypeAdapter`'s instead ([`Self::gson_read_jar_enum`]);
    /// a JSON string for any other jar class is Gson's `JsonSyntaxException`
    /// (counted, #253 re-check B6).
    fn gson_read_jar_object(
        &mut self,
        tree: &Tree,
        class: &str,
        style: &gson_model::GsonStyle,
    ) -> Result<Option<Value>, NativeError> {
        if let Some(read) = self.gson_read_jar_enum(tree, class)? {
            return Ok(read);
        }
        let Tree::Object(members) = tree else {
            return Ok(None);
        };
        let fields = self.gson_fields(class);
        // Gson's reflective adapter has no useful instance target when the
        // class contains only static, synthetic, or otherwise excluded
        // fields. Keep this counted rather than materializing an empty jar
        // object (the mc-armor-hud config shape).
        if !fields
            .iter()
            .any(|(_, _, _, access)| access & 0x0008 == 0 && style.includes_field(*access))
        {
            return Ok(None);
        }
        self.ensure_initialized(class)?;
        let object = self.new_object(class);
        if self.class(class).is_some_and(|file| {
            file.methods
                .iter()
                .any(|method| method.name == "<init>" && method.descriptor == "()V")
        }) {
            self.invoke(class, "<init>", "()V", Some(object.clone()), &[])?;
        }
        for (declaring, name, descriptor, access) in fields {
            if !style.includes_field(access) {
                continue;
            }
            let names = self.gson_serialized_names(&declaring, &name, &descriptor, style);
            let Some((_, member)) = names
                .iter()
                .find_map(|name| members.iter().find(|(key, _)| key == name))
            else {
                continue;
            };
            let field_type = self.gson_field_type(&declaring, &name, &descriptor);
            let Some(value) = self.gson_read(member, &field_type, style)? else {
                return Ok(None);
            };
            let field = MethodRef {
                owner: declaring,
                name,
                descriptor,
            };
            if access & 0x0008 != 0 {
                // Gson sets a static field through reflection; the
                // invocation's static table is the same field here.
                std::sync::Arc::make_mut(&mut self.static_fields)
                    .insert(Self::static_field_key(&field), value);
            } else if let Some(key) = instance_field_key(&object, &field) {
                self.fields.insert(key, value);
            }
        }
        Ok(Some(object))
    }

    /// `TypeAdapters.EnumTypeAdapter` (gson-2.14.0) when `class` is a jar enum
    /// (`Some(read)`, `read` `None` where counted); `None` for any other class.
    ///
    /// The adapter maps each constant field (`isEnumConstant`, declaration
    /// order, a later `put` replacing an earlier one) by its `@SerializedName`
    /// `alternate`s, then its `value` or else `name()`; a second map holds
    /// `toString()`. A read is `nextString` (a string or a number's text; any
    /// other token throws, counted), then the first map, then the second, else
    /// `null`. The constant answered is the jar's own object the class
    /// initializer built, never a marker of a name. A constant whose
    /// `toString` the jar overrides is counted when the first map misses: its
    /// text is not computed here.
    pub(crate) fn gson_read_jar_enum(
        &mut self,
        tree: &Tree,
        class: &str,
    ) -> Result<Option<Option<Value>>, NativeError> {
        const ACC_ENUM: u16 = 0x4000;
        const SERIALIZED_NAME: &str = "Lcom/google/gson/annotations/SerializedName;";
        let Some(file) = self.class(class) else {
            return Ok(None);
        };
        if file.super_name != "java/lang/Enum"
            || crate::runtime::jdk_model::class_access_flags(file).is_none_or(|flags| flags & ACC_ENUM == 0)
        {
            return Ok(None);
        }
        let constant_fields = file
            .fields
            .iter()
            .filter(|field| field.is_static() && field.access & ACC_ENUM != 0)
            .map(|field| {
                let serialized = file
                    .field_annotations(&field.name, &field.descriptor)
                    .into_iter()
                    .find(|annotation| annotation.descriptor == SERIALIZED_NAME);
                (field.name.clone(), field.descriptor.clone(), serialized)
            })
            .collect::<Vec<_>>();
        let key = match tree {
            Tree::Str(text) | Tree::Num(text) => text.clone(),
            _ => return Ok(Some(None)),
        };
        self.ensure_initialized(class)?;
        // (map key, constant), in `put` order: a later entry wins.
        let mut by_name: Vec<(String, Value)> = Vec::new();
        let mut by_text: Vec<(String, Value)> = Vec::new();
        let mut text_overridden = false;
        for (name, descriptor, serialized) in constant_fields {
            let field = MethodRef {
                owner: class.to_owned(),
                name,
                descriptor,
            };
            let Some(constant) = self
                .static_fields
                .get(&Self::static_field_key(&field))
                .cloned()
            else {
                return Ok(Some(None));
            };
            let Some((constant_name, _)) = expect_ref(&constant)
                .and_then(|marker| self.enum_constants.get(marker))
                .cloned()
            else {
                return Ok(Some(None));
            };
            // `toString()`: `Enum.name()` unless the enum or the constant's
            // body class declares its own.
            let body = crate::runtime::jar_object::object_class(&constant)
                .unwrap_or(class)
                .to_owned();
            text_overridden |= [class, body.as_str()].iter().any(|owner| {
                self.class(owner).is_some_and(|file| {
                    file.methods.iter().any(|method| {
                        method.name == "toString" && method.descriptor == "()Ljava/lang/String;"
                    })
                })
            });
            let mut json_name = constant_name.clone();
            if let Some(annotation) = serialized {
                match annotation.values.get("value") {
                    Some(crate::runtime::AnnotationValue::String(value)) => json_name = value.clone(),
                    _ => return Ok(Some(None)),
                }
                if let Some(alternates) = annotation.values.get("alternate") {
                    let crate::runtime::AnnotationValue::Array(alternates) = alternates else {
                        return Ok(Some(None));
                    };
                    for alternate in alternates {
                        let crate::runtime::AnnotationValue::String(alternate) = alternate else {
                            return Ok(Some(None));
                        };
                        by_name.push((alternate.clone(), constant.clone()));
                    }
                }
            }
            by_name.push((json_name, constant.clone()));
            by_text.push((constant_name, constant));
        }
        let last = |entries: &[(String, Value)]| {
            entries
                .iter()
                .rev()
                .find(|(name, _)| *name == key)
                .map(|(_, constant)| constant.clone())
        };
        if let Some(constant) = last(&by_name) {
            return Ok(Some(Some(constant)));
        }
        if text_overridden {
            return Ok(Some(None));
        }
        Ok(Some(Some(last(&by_text).unwrap_or(Value::Ref(None)))))
    }

    /// A field's declared type: its generic `Signature` when it has one, else
    /// its erased descriptor.
    fn gson_field_type(&self, class: &str, name: &str, descriptor: &str) -> GenericType {
        let erased = || GenericType::Class(crate::runtime::erased_field_type(descriptor));
        let Some(file) = self.class(class) else {
            return erased();
        };
        let scope = crate::runtime::reflect_generic::class_scope(class, Some(file));
        file.field_signature(name, descriptor)
            .and_then(|signature| crate::runtime::parse_generic_signature(&signature, scope))
            .unwrap_or_else(erased)
    }
}

/// `TypeAdapters`' readers for `String`, the boxes and the primitives:
/// `Some(answer)` when `class` is one of them (`Some(None)` where the reader
/// throws or this cannot tell), `None` for any other class.
fn gson_scalar(tree: &Tree, class: &str) -> Option<Option<Value>> {
    let integral = |tree: &Tree| scalar_text(tree).and_then(json_int);
    Some(match class {
        // `nextString`: a string, a number's text; a boolean's text.
        "java/lang/String" => match tree {
            Tree::Str(text) | Tree::Num(text) => Some(Value::Ref(Some(text.clone()))),
            Tree::Bool(flag) => Some(Value::Ref(Some(flag.to_string()))),
            _ => None,
        },
        "int" => integral(tree).map(Value::Int),
        "java/lang/Integer" => integral(tree).map(|int| Value::Boxed(Boxed::Integer(int))),
        "long" | "java/lang/Long" => scalar_text(tree).and_then(json_long).map(Value::Long),
        // `nextInt`, a `JsonSyntaxException` past 65535 or 255 (or below the
        // type's minimum), then the narrowing cast.
        "short" | "java/lang/Short" => integral(tree)
            .filter(|int| (i32::from(i16::MIN)..=65535).contains(int))
            .map(|int| {
                if class == "short" {
                    Value::Int(i32::from(int as i16))
                } else {
                    Value::Boxed(Boxed::Short(int as i16))
                }
            }),
        "byte" | "java/lang/Byte" => integral(tree)
            .filter(|int| (i32::from(i8::MIN)..=255).contains(int))
            .map(|int| {
                if class == "byte" {
                    Value::Int(i32::from(int as i8))
                } else {
                    Value::Boxed(Boxed::Byte(int as i8))
                }
            }),
        "double" | "java/lang/Double" => scalar_text(tree).and_then(json_double).map(Value::Float),
        "float" => scalar_text(tree)
            .and_then(json_double)
            .map(|double| Value::Float(f64::from(double as f32))),
        "java/lang/Float" => scalar_text(tree)
            .and_then(json_double)
            .map(|double| Value::Boxed(Boxed::float(double))),
        // `nextBoolean`, or `Boolean.parseBoolean` of a string.
        "boolean" | "java/lang/Boolean" => {
            let flag = match tree {
                Tree::Bool(flag) => Some(*flag),
                Tree::Str(text) => Some(text.eq_ignore_ascii_case("true")),
                _ => None,
            };
            flag.map(|flag| {
                if class == "boolean" {
                    Value::Int(i32::from(flag))
                } else {
                    Value::Boxed(Boxed::Boolean(flag))
                }
            })
        }
        // `TypeAdapters.CHARACTER`: a string of exactly one UTF-16 unit.
        "char" | "java/lang/Character" => match tree {
            Tree::Str(text) | Tree::Num(text) => {
                let mut units = text.encode_utf16();
                match (units.next(), units.next()) {
                    (Some(unit), None) if class == "char" => Some(Value::Int(i32::from(unit))),
                    (Some(unit), None) => Some(Value::Boxed(Boxed::Character(unit))),
                    _ => None,
                }
            }
            _ => None,
        },
        // `Number`, `CharSequence`, `BigDecimal`...: not reproduced.
        "java/lang/Number" | "java/lang/CharSequence" => None,
        _ => return None,
    })
}
