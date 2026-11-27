//! Objects the runtime allocated (`new`, `Runtime::new_object`): the one place
//! that reads one (#156 P12).
//!
//! An object is a handle, `Value::Host` of [`HostKind::JarObject`], whose key
//! is `<internal class name>#<n>`: its class and the runtime's allocation
//! number. Every allocation is one (a jar class's, a JDK class's a jar `new`s,
//! a model's own), so no object is text a `String` can spell (#156 P12b): a
//! `Value::Ref` spelling a key is a `String` to every reader (`is_plain_string`,
//! the accessors below). The key is the text the object used to be, so every
//! table keyed by an object (instance fields, the host's throwable messages and
//! collections, the resident heap) keeps its rows, and `expect_ref` answers a
//! handle's key to the readers that look one up.
//!
//! * **Class**: [`class_of_key`], from the key (`ClassRule::FromKey`); the
//!   class file the run holds (or Java 25's table for a JDK class) answers the
//!   rest of the `Class` surface.
//! * **Identity**: one key per allocation, never re-keyed, so the kind is
//!   canonical: `==` is key equality.
//! * **`toString`/`hashCode`/`equals`**: the class's own, which only the
//!   runtime can run; the row hands the handle back to the caller's
//!   [`Nested`], which runs them (the runtime), or answers what the host
//!   models and counts the rest. `Object`'s identity versions are counted.
//!
//! **Parsing a key.** A key ends in the allocation number, which the runtime
//! wrote, so the class is everything before the **last** `#` and the number
//! is the digits after it. A class name may itself hold `#`, `$` or `/` (the
//! JVM allows `#` in a binary name), which splitting at the first `#` misread.
//! A key is never built from text a jar chose apart from the class name the
//! class file declares, and no other separator is read back out of it.
//!
//! Not objects here: a handle-only class's text (an array, `[I#0`) and a
//! member reference `owner#name`, whose suffix is not a number.
//!
//! **Host-made collections** (#156 P14c) are objects too, of
//! [`HostKind::Collection`]: an unmodelled call's neutral answer, `subList`,
//! an unmodifiable view, `List.of`, a reversed view. Their keys are the text
//! they used to be, `<JDK class>#neutral<n>`, `#immutable<n>` and
//! `#reversed<n>` ([`collection_key`]), so the host's tables keep their rows,
//! and they share the object row: the readers below answer for both kinds
//! alike, as for a jar's own `new ArrayList()`. A `String` spelling such a
//! key is a `String`.

use crate::Value;
use crate::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, instance_of_class};

/// A jar object's handle for an allocation key `class#n`.
pub(crate) fn handle(key: String) -> Value {
    debug_assert!(split_key(&key).is_some(), "{key:?} is no allocation key");
    Value::Host(HostRef::new(HostKind::JarObject, key))
}

/// The running heap's allocator, lent to a host model whose answer is a new object keyed as a
/// jar object (`class#n`): an `EntityType` builder, an `EntityDataAccessor`, `Vec2.ZERO`
/// (`Host::allocating_call`). One counter numbers every object the jar can see, so a model's
/// object never takes a number the heap hands out (and frees) for another (#669: the models
/// counted from `700_000`, `720_000`, `900_000` or an accessor's id, numbers the heap reaches,
/// so a model's object could be born "collected", or `==` to another).
pub(crate) struct Numbers<'a> {
    next: &'a mut usize,
    exhausted: Option<String>,
}

impl<'a> Numbers<'a> {
    pub(crate) fn new(next: &'a mut usize) -> Self {
        Self {
            next,
            exhausted: None,
        }
    }

    /// A new object of `class`: `class#n`, `n` the heap's next number. `usize::MAX` marks the
    /// numbers exhausted (`Runtime::new_object_key`): then no object is made, the call fails,
    /// and [`Self::take_exhausted`] names the class for the host to count.
    pub(crate) fn handle(&mut self, class: &str) -> Result<Value, crate::NativeError> {
        let number = *self.next;
        if number == usize::MAX {
            self.exhausted = Some(class.to_owned());
            return Err(crate::allocation_numbers_exhausted(class));
        }
        *self.next = number + 1;
        Ok(handle(format!("{class}#{number}")))
    }

    /// The class an allocation ran out of numbers for, once: the host counts it
    /// (`Host::note_allocation_numbers_exhausted`).
    pub(crate) fn take_exhausted(&mut self) -> Option<String> {
        self.exhausted.take()
    }
}

/// A canonical handle for one of the runtime's modelled `java.io.Writer`
/// classes. Keeping this family separate from ordinary jar objects is part of
/// the sink boundary: Gson may write only to this kind (or an Appendable
/// family explicitly handled by its overload).
pub(crate) fn writer_handle(key: String) -> Value {
    debug_assert!(split_key(&key).is_some(), "{key:?} is no allocation key");
    Value::Host(HostRef::new(HostKind::Writer, key))
}

/// The object a table row of the runtime's own names by its key (a model's
/// field holding a child's key). Never for a jar's text: a key is a handle's
/// only, and a `String` spelling one is no object. The kind is canonical, so
/// this is the same object the key was read from.
pub(crate) fn stored(key: &str) -> Value {
    if collection_key(key).is_some() {
        return collection(key.to_owned());
    }
    if menu_object_key(key).is_some() {
        return menu_object(key.to_owned());
    }
    if matches!(
        key_class(key),
        Some(
            "java/io/BufferedWriter"
                | "java/io/FileWriter"
                | "java/io/OutputStreamWriter"
                | "java/io/PrintWriter"
                | "java/io/StringWriter"
        )
    ) {
        return writer_handle(key.to_owned());
    }
    handle(key.to_owned())
}

/// A value a model kept as its text (a guard's source collection, a view's
/// map, a proxy's own marker): an object's key is that object's handle, any
/// other text the `Ref` it was. Only for text the runtime stored from a value
/// (`expect_ref` of one), never a jar's: no `Ref` is an object, so a key the
/// runtime holds is an object's.
pub(crate) fn stored_value(text: &str) -> Value {
    if parse_key(text).is_some()
        || collection_key(text).is_some()
        || menu_object_key(text).is_some()
    {
        stored(text)
    } else {
        Value::Ref(Some(text.to_owned()))
    }
}

/// A host-synthesized menu-model object's handle for its key (`<class>#menu<n>`).
pub(crate) fn menu_object(key: String) -> Value {
    debug_assert!(
        menu_object_key(&key).is_some(),
        "{key:?} is no menu object key"
    );
    Value::Host(HostRef::new(HostKind::MenuObject, key))
}

/// A host-synthesized menu-model object's key: `(class, n)` for `<class>#menu<n>`,
/// a class with no `#`, `:`, space, `.`, `,` or `\`. `parse_key` still returns `None`
/// for this shape so `#menu<n>` ids stay out of `freed_objects` numeric ranges, while
/// `spells_key_text` keeps any Java `String` literal with this text as a plain `String`.
pub(crate) fn menu_object_key(text: &str) -> Option<(&str, &str)> {
    let (class, rest) = text.split_once('#')?;
    let number = rest.strip_prefix("menu")?;
    (!class.is_empty()
        && !class.contains([':', ' ', '.', ',', '\\'])
        && !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit()))
    .then_some((class, number))
}

/// `ClassRule::FromKey` of a host-synthesized menu object: the class before `#menu<n>`.
pub(crate) fn menu_object_class(key: &str) -> &str {
    key.split_once('#').map_or(key, |(class, _)| class)
}

/// Known 26.3 supertypes of host-synthesized `#menu<n>` classes (`javap` on 26.3).
fn menu_object_supertypes(class: &str) -> &'static [&'static str] {
    const RESULT_CONTAINER: &[&str] = &[
        "net/minecraft/world/inventory/ResultContainer",
        "net/minecraft/world/Container",
        "net/minecraft/world/Clearable",
        "java/lang/Iterable",
        "net/minecraft/world/entity/SlotProvider",
        "net/minecraft/world/inventory/RecipeCraftingHolder",
    ];
    const SIMPLE_CONTAINER: &[&str] = &[
        "net/minecraft/world/SimpleContainer",
        "net/minecraft/world/Container",
        "net/minecraft/world/Clearable",
        "java/lang/Iterable",
        "net/minecraft/world/entity/SlotProvider",
        "net/minecraft/world/inventory/StackedContentsCompatible",
    ];
    const SLOT: &[&str] = &["net/minecraft/world/inventory/Slot"];
    const RECORD: &[&str] = &["java/lang/Record"];
    const SIMPLE_MENU_PROVIDER: &[&str] = &[
        "net/minecraft/world/MenuProvider",
        "net/minecraft/world/inventory/MenuConstructor",
    ];
    const CONTAINER_LEVEL_ACCESS: &[&str] = &["net/minecraft/world/inventory/ContainerLevelAccess"];
    const INVENTORY: &[&str] = &[
        "net/minecraft/world/Container",
        "net/minecraft/world/Clearable",
        "java/lang/Iterable",
        "net/minecraft/world/entity/SlotProvider",
        "net/minecraft/world/Nameable",
    ];
    match class {
        "net/minecraft/core/NonNullList" => crate::java_immutable::cast_supertypes(class),
        "net/minecraft/world/inventory/ItemCombinerMenu$1" => RESULT_CONTAINER,
        "net/minecraft/world/inventory/ItemCombinerMenu$4" => SIMPLE_CONTAINER,
        "net/minecraft/world/inventory/ItemCombinerMenu$2"
        | "net/minecraft/world/inventory/ItemCombinerMenu$3" => SLOT,
        "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition$SlotDefinition" => RECORD,
        "net/minecraft/world/SimpleMenuProvider" => SIMPLE_MENU_PROVIDER,
        "net/minecraft/world/inventory/ContainerLevelAccess$2" => CONTAINER_LEVEL_ACCESS,
        "net/minecraft/world/entity/player/Inventory" => INVENTORY,
        _ => &[],
    }
}

fn menu_object_instance_of(
    handle: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    let class = menu_object_class(handle.key());
    if wanted == "java/lang/Object"
        || class == wanted
        || menu_object_supertypes(class).contains(&wanted)
    {
        return Some(true);
    }
    if wanted.starts_with('[') {
        return Some(false);
    }
    if class == "net/minecraft/core/NonNullList" && wanted == "java/util/RandomAccess" {
        return Some(false);
    }
    hierarchy.is_instance(class, wanted)
}

/// A host-made collection's handle for its key (`<class>#neutral<n>`,
/// `<class>#immutable<n>`).
pub(crate) fn collection(key: String) -> Value {
    debug_assert!(
        collection_key(&key).is_some(),
        "{key:?} is no collection key"
    );
    Value::Host(HostRef::new(HostKind::Collection, key))
}

/// Which of the host's collections a key names: a mutable one (an
/// unmodelled call's answer, `subList`, a map's view, a loader's list), an
/// unmodifiable one (`List.of`, `Collections.unmodifiableList`) or a
/// reversed view. Its word is the key's (`collection_key_for`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CollectionFamily {
    Neutral,
    Immutable,
    Reversed,
}

impl CollectionFamily {
    const ALL: [Self; 3] = [Self::Neutral, Self::Immutable, Self::Reversed];

    fn word(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Immutable => "immutable",
            Self::Reversed => "reversed",
        }
    }
}

/// The key of the `n`th collection of `family` the host makes as `class`:
/// the one place a collection key is written (#156 P14d1), read back by
/// [`collection_key`]. Each family's counter only grows, so no key is
/// handed out twice.
pub(crate) fn collection_key_for(class: &str, family: CollectionFamily, n: usize) -> String {
    format!("{class}#{}{n}", family.word())
}

/// A host-made collection's key: `(class, n)` for `<class>#neutral<n>`,
/// `<class>#immutable<n>` or a reversed view's `<class>#reversed<n>`, a JDK
/// (or Guava) class with no `#`, `:`, space, `.`, `,` or `\\`. The host
/// wrote it; a `String` spelled so is no collection.
pub(crate) fn collection_key(text: &str) -> Option<(&str, &str)> {
    let (class, rest) = text.split_once('#')?;
    let number = CollectionFamily::ALL
        .iter()
        .find_map(|family| rest.strip_prefix(family.word()))?;
    (!class.is_empty()
        && !class.contains([':', ' ', '.', ',', '\\'])
        && !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit()))
    .then_some((class, number))
}

/// `ClassRule::FromKey` of a host-made collection: the JDK class before the
/// `#` (`java/util/ArrayList#neutral3` is an `ArrayList`).
pub(crate) fn collection_class(key: &str) -> &str {
    key.split_once('#').map_or(key, |(class, _)| class)
}

/// Whether `value` is an allocated object (a [`HostKind::JarObject`] or a
/// host-made [`HostKind::Collection`]).
pub(crate) fn is_object(value: &Value) -> bool {
    object_key(value).is_some()
}

/// Whether `value` is a `String` whose text spells an object's key, a jar
/// object's or a host-made collection's (no object is text, so it is just
/// that string).
pub(crate) fn spells_key(value: &Value) -> bool {
    matches!(value, Value::Ref(Some(text)) if spells_key_text(text))
}

/// [`spells_key`] of a reference's text.
pub(crate) fn spells_key_text(text: &str) -> bool {
    parse_key(text).is_some() || collection_key(text).is_some() || menu_object_key(text).is_some()
}

/// Whether `handle` is an allocated object: a jar's (or a model's) `new`, a
/// host-made collection, or a host-synthesized menu object.
pub(crate) fn is_object_handle(handle: &HostRef) -> bool {
    matches!(
        handle.kind(),
        HostKind::JarObject
            | HostKind::Writer
            | HostKind::Collection
            | HostKind::MenuObject
            | HostKind::GuavaMultimap
            | HostKind::RenderObject
    )
}

/// `ClassRule::FromKey` of a jar object: the class before the allocation
/// number (a key always has one; [`handle`] is its only producer).
pub(crate) fn class_of_key(key: &str) -> &str {
    key_class(key).unwrap_or(key)
}

/// The kind's row: each method is the class's own, so the row asks the
/// caller's [`Nested`] about the object itself, as it asks about a value a
/// handle holds. The runtime's runs the jar's body (or counts `Object`'s
/// identity hash); the host's answers what it models and leaves the rest
/// unknown, so the caller counts it.
pub(crate) const JAR_OBJECT: Methods = Methods {
    to_string: |handle, nested| nested.text(&Value::Host(handle.clone())),
    hash_code: |handle, nested| nested.hash(&Value::Host(handle.clone())),
    equals: |handle, other, nested| nested.equals(&Value::Host(handle.clone()), other),
    instance_of: instance_of_class,
    // An object is its own jar's: another jar's runtime holds none of it.
    crosses_jars: |_, _| false,
    // `compareTo` dispatches to the class's own body like any method; a
    // caller that only has the row (a host sort) counts it.
    compare_to: None,
};

/// Row for [`HostKind::MenuObject`]: shares `JAR_OBJECT`'s `Object` dispatch
/// while using the 26.3 menu-object supertype table for `instanceof` / `checkcast`.
pub(crate) const MENU_OBJECT: Methods = Methods {
    instance_of: menu_object_instance_of,
    ..JAR_OBJECT
};

/// A handle's key `class#n`: `(class, n)`. The runtime wrote it
/// (`Runtime::new_object`), so the split is all there is to it.
fn split_key(key: &str) -> Option<(&str, &str)> {
    let (class, number) = key.rsplit_once('#')?;
    (!class.is_empty() && !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
        .then_some((class, number))
}

/// Whether a reference's *text* spells an object's key, `(class, n)`: what
/// decides that a `Ref` is a `String` and no host marker. Stricter than a
/// handle's key: not an array's (`[I#0`), and no `:`, space, `.`, `,` or `\`
/// (no class name has one; a host marker may). Whatever the class's package:
/// `map/Thing#0` is a jar class's key, so a `String` spelling it is a
/// `String`.
pub(crate) fn parse_key(text: &str) -> Option<(&str, &str)> {
    let (class, number) = split_key(text)?;
    (!crate::host_array::handle_only_class(class) && !class.contains([':', ' ', '.', ',', '\\']))
        .then_some((class, number))
}

/// The class of a handle's key (`class#n`), `None` for a text with no
/// allocation number.
pub(crate) fn key_class(key: &str) -> Option<&str> {
    split_key(key).map(|(class, _)| class)
}

/// The class any `#` marker names: an object's (split at its allocation
/// number), or, for the host models' own collection and view markers
/// (`java/util/ArrayList#neutral0`, `…ListN#immutable3`, collections P14),
/// the text before the first `#`, as their readers have always taken it.
pub(crate) fn marker_class(text: &str) -> Option<&str> {
    parse_key(text)
        .map(|(class, _)| class)
        .or_else(|| text.split_once('#').map(|(class, _)| class))
}

/// [`marker_class`], or the text itself when it has no `#`.
pub(crate) fn marker_class_or_text(text: &str) -> &str {
    marker_class(text).unwrap_or(text)
}

/// [`marker_class`] of a value: an object's class, or a host marker's.
pub(crate) fn value_marker_class(value: &Value) -> Option<&str> {
    object_class(value).or_else(|| ref_marker_class(value))
}

/// The key of the object `value` is, `None` for any other value (a
/// `String` spelling a key included).
pub(crate) fn object_key(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle) if is_object_handle(handle) => Some(handle.key()),
        _ => None,
    }
}

/// The text an instance field of `value` is filed under
/// (`<receiver>|<owner>#<name><descriptor>`): an object's key, or the text of
/// any other reference with fields (a jar enum constant's `enum/…`). A
/// `String` spelling a key has none.
pub(crate) fn instance_key(value: &Value) -> Option<&str> {
    // A game object a Mixin callback runs on keeps the Mixin's fields under
    // its handle's key (`game_hooks.rs`, #156 P16e).
    object_key(value)
        .or_else(|| crate::game_object::handle_key(value))
        .or_else(|| match value {
            Value::Ref(Some(text)) if !spells_key_text(text) => Some(text.as_str()),
            _ => None,
        })
}

/// [`marker_class`] of a `Ref`'s text, `None` for a `String` that spells an
/// object's key (it is no marker).
fn ref_marker_class(value: &Value) -> Option<&str> {
    match value {
        Value::Ref(Some(text)) if !spells_key_text(text) => marker_class(text),
        _ => None,
    }
}

/// The class of the object `value` is, `None` for any other value.
pub(crate) fn object_class(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::Collection => {
            Some(collection_class(handle.key()))
        }
        Value::Host(handle) if handle.kind() == HostKind::MenuObject => {
            Some(menu_object_class(handle.key()))
        }
        _ => key_class(object_key(value)?),
    }
}

/// [`object_class`] of a class in a package (`a/B`, not `B`): the
/// `class#n` objects `new` and the models build in this run, as
/// `jar_text`/`jdk_model` have always read them.
pub(crate) fn packaged_object_class(value: &Value) -> Option<&str> {
    object_class(value).filter(|class| class.contains('/'))
}

/// [`key_class`] of a class in a package.
pub(crate) fn packaged_key_class(text: &str) -> Option<&str> {
    key_class(text).filter(|class| class.contains('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A jar class in a package named like a host collection (`map`, `list`,
    /// `set`) has keys like any other class.
    #[test]
    fn a_class_in_a_collection_named_package_has_keys() {
        assert_eq!(parse_key("map/Thing#0"), Some(("map/Thing", "0")));
        assert_eq!(
            parse_key("list/render/Thing#2"),
            Some(("list/render/Thing", "2"))
        );
    }

    /// The class is everything before the last `#`, whatever the class name
    /// holds; a suffix that is no allocation number is no object.
    #[test]
    fn keys_split_at_the_allocation_number() {
        assert_eq!(parse_key("demo/Foo#3"), Some(("demo/Foo", "3")));
        assert_eq!(
            parse_key("demo/Outer$Inner#12"),
            Some(("demo/Outer$Inner", "12"))
        );
        assert_eq!(parse_key("demo/We#ird#7"), Some(("demo/We#ird", "7")));
        assert_eq!(parse_key("Foo#0"), Some(("Foo", "0")));
        assert_eq!(packaged_key_class("Foo#0"), None);
        for text in [
            "demo/Foo#",
            "#3",
            "demo/Foo#3a",
            "java/util/ArrayList#immutable3",
            "demo/Foo#bar",
            "net/minecraft/core/NonNullList#menu0",
            "[I#0",
            "a b#3",
            "minecraft:stone#3",
            "demo.Foo#3",
            "plain text",
            // A host collection marker whose element ends like a key.
            "list/r#1,r#2,r#3",
            "set/r\\#3",
        ] {
            assert_eq!(parse_key(text), None, "{text:?}");
        }
        assert_eq!(marker_class_or_text("demo/Foo"), "demo/Foo");
        assert_eq!(marker_class_or_text("demo/Foo#4"), "demo/Foo");
        assert_eq!(marker_class_or_text("demo/We#ird#4"), "demo/We#ird");
        // A host model's own collection marker keeps its old reading.
        assert_eq!(
            marker_class("java/util/ArrayList#neutral0"),
            Some("java/util/ArrayList")
        );
        assert_eq!(key_class("java/util/ArrayList#neutral0"), None);
        assert_eq!(object_class(&handle("demo/Foo#4".into())), Some("demo/Foo"));
        // A handle's key is the runtime's: its class is the split, even in a
        // package named like a host collection.
        assert_eq!(object_class(&handle("map/Foo#3".into())), Some("map/Foo"));
        // Text no key is shaped like reads back as the `Ref` it was.
        assert_eq!(
            stored_value("list/r\\#3"),
            Value::Ref(Some("list/r\\#3".into()))
        );
        // The key of an object of a jar class in a package named like a host
        // collection, kept by a model, is that object again.
        assert_eq!(stored_value("map/Thing#0"), handle("map/Thing#0".into()));
        // A `String` spelling a key is no object.
        assert_eq!(object_class(&Value::Ref(Some("demo/Foo#4".into()))), None);
        assert_eq!(instance_key(&Value::Ref(Some("demo/Foo#4".into()))), None);
        assert!(spells_key(&Value::Ref(Some("demo/Foo#4".into()))));
        assert_eq!(object_class(&Value::Int(4)), None);
    }
}
