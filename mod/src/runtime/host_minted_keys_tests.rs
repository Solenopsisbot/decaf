//! Every object a jar can see takes its number from the heap's one allocator (#669): no host
//! model mints a key or serial from a counter of its own, or from a truncated copy of the
//! heap's.
//!
//! The probes are javac-compiled at test time from `tests/fixtures/<dir>` (only `demo/` classes
//! ship in the jar, so a stubbed library's calls reach the runtime's models). Each expected
//! string is what the same `demo` class printed on Java 25 (OpenJDK 25.0.5), kept in that
//! fixture's `java25.txt` or in the fixture test that already checks it.
//!
//! These run in-crate because the failure needs a heap whose allocator is already past an
//! `i32`, as every game hook leaves it ([`crate::runtime::game_hooks::GAME_HOOK_OBJECT_BASE`]), and only
//! the crate can set that directly.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::runtime::game_hooks::GAME_HOOK_OBJECT_BASE;
use crate::runtime::{NativeJar, StdHost, Value};

/// `tests/fixtures/<path>`, from this crate or from the mod that vendors it.
fn fixture(path: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures").join(path),
        manifest
            .join("../crates/horizon-native-java/tests/fixtures")
            .join(path),
    ]
    .into_iter()
    .find(|path| path.exists())
    .unwrap_or_else(|| panic!("tests/fixtures/{path} is in crates/horizon-native-java"))
}

fn java_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            java_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "java") {
            out.push(path);
        }
    }
}

fn class_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            class_files(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "class") {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((name, path));
        }
    }
}

/// Compile `sources` (each relative to `tests/fixtures`: a `.java` file, or a directory whose
/// every `.java` file is taken) with `javac --release 21` and pack the `demo/` classes as a
/// Fabric jar.
fn compile(name: &str, sources: &[&str]) -> NativeJar {
    use std::io::Write;
    let mut files = Vec::new();
    for source in sources {
        let path = fixture(source);
        if path.is_dir() {
            java_files(&path, &mut files);
        } else {
            files.push(path);
        }
    }
    let scratch = std::env::temp_dir().join(format!(
        "horizon-native-java-host-minted-keys-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let classes = scratch.join("classes");
    std::fs::create_dir_all(&classes).unwrap();
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "21", "-nowarn", "-d"])
        .arg(&classes)
        .args(&files)
        .status()
        .expect("javac runs");
    assert!(status.success(), "javac failed on the {name} probe");
    let mut compiled = Vec::new();
    class_files(&classes, &classes, &mut compiled);
    let mut bytes = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("fabric.mod.json", options).unwrap();
        zip.write_all(
            br#"{"schemaVersion":1,"id":"host_minted_keys","version":"1","entrypoints":{"main":[]}}"#,
        )
        .unwrap();
        for (entry, path) in compiled {
            if entry.starts_with("demo/") {
                zip.start_file(entry, options).unwrap();
                zip.write_all(&std::fs::read(path).unwrap()).unwrap();
            }
        }
        zip.finish().unwrap();
    }
    let _ = std::fs::remove_dir_all(&scratch);
    NativeJar::from_bytes(&bytes).expect("the probe jar parses")
}

/// A host whose heap has numbered past every `i32`, as one game hook leaves it.
fn after_a_game_hook(jar: &NativeJar) -> StdHost {
    let mut host = jar.seed_host(StdHost::new());
    host.heap.next_object = GAME_HOOK_OBJECT_BASE;
    host
}

/// `String <class>.<method>()`: its answer and the calls it counted.
fn run(
    jar: &NativeJar,
    host: &mut StdHost,
    class: &str,
    method: &str,
) -> (String, Vec<(String, usize)>) {
    let value = jar
        .invoke_method(class, method, "()Ljava/lang/String;", None, host)
        .unwrap_or_else(|error| panic!("{class}.{method} failed: {error}"));
    let Value::Ref(Some(text)) = value else {
        panic!("{class}.{method} answered {value:?}");
    };
    (text, host.unknown_host_calls())
}

// ---------------------------------------------------------------------------------------------
// config_serial (`client_config.rs`)

fn config_jar() -> &'static NativeJar {
    static JAR: OnceLock<NativeJar> = OnceLock::new();
    JAR.get_or_init(|| {
        compile(
            "configs",
            &["host_values/p7/stubs", "host_values/p7/demo/Configs.java"],
        )
    })
}

/// Two YACL builders and two configlib builders made after a game hook are two objects, as on
/// Java 25: `fixture_host_values_p7`'s rows for the same methods, captured against the real
/// libraries (yet_another_config_lib_v3 3.9.7, supermartijn642configlib 1.1.8a). Before #669 the
/// serial was `i32::try_from(next_object).unwrap_or(i32::MAX)`: past `1 << 40` every builder's
/// serial was `i32::MAX`, one key for all, so `b == c` and `b.equals(c)` answered `true` and a
/// `HashSet` of two held one.
#[test]
fn config_objects_made_after_a_game_hook_are_distinct() {
    let jar = config_jar();
    for (method, java25) in [
        ("yaclTwoBuilders", "falsefalse"),
        ("yaclBuildersInCollections", "22false2"),
        ("configlibTwoBuilders", "falsefalse"),
    ] {
        let mut host = after_a_game_hook(jar);
        let (answer, counted) = run(jar, &mut host, "demo/Configs", method);
        assert_eq!(answer, java25, "{method}");
        assert!(counted.is_empty(), "{method}: {counted:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// EnumSet.allOf (`library_dispatch.rs`)

/// `EnumSet.allOf` answers `values()`, in its order, and an empty enum's empty set; a failing
/// static initializer's `ExceptionInInitializerError` (then `NoClassDefFoundError`) comes out of
/// it as on Java 25 (`tests/fixtures/host_minted_keys/enum_set/java25.txt`, a JDK 25.0.5 run of
/// the same class). Before #669 a failing or empty `values()` was swallowed and the set made up
/// from the enum's static fields: `[null]` for an empty enum with a static field of its own
/// type, and `<enum>#<ordinal>` handles, numbers the heap hands out to other objects, for a
/// broken one.
#[test]
fn enum_set_all_of_answers_values_or_fails_as_java() {
    let jar = compile("enum_set", &["host_minted_keys/enum_set"]);
    let oracle = std::fs::read_to_string(fixture("host_minted_keys/enum_set/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    let (answer, counted) = run(&jar, &mut host, "demo/EnumSetProbe", "run");
    assert_eq!(answer, oracle.trim_end());
    assert!(counted.is_empty(), "{counted:?}");
}

// ---------------------------------------------------------------------------------------------
// ServiceLoader serials (`service_loader.rs`)

/// The heap number a `ServiceLoader` handle's serial holds, however it is kept.
fn loader_serial(loader: &Value) -> usize {
    let Value::Host(handle) = loader else {
        panic!("{loader:?} is no loader");
    };
    match handle.values().unwrap().as_slice() {
        [_, Value::Int(serial)] => *serial as usize,
        [_, Value::Long(serial)] => *serial as usize,
        other => panic!("{other:?}"),
    }
}

/// A loader made after a game hook is not one made before it (Java 25:
/// `tests/fixtures/host_minted_keys/service_loader/java25.txt`). The second load is numbered
/// `1 << 40` past the first; before #669 the serial was kept as `serial as i32`, which wrapped it
/// onto the first loader's: one key, so `first == second` answered `true`.
#[test]
fn a_service_loader_after_a_game_hook_is_a_new_loader() {
    let jar = compile("service_loader", &["host_minted_keys/service_loader"]);
    let oracle =
        std::fs::read_to_string(fixture("host_minted_keys/service_loader/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    host.heap.next_object = 7;
    let first = jar
        .invoke_method(
            "demo/ServiceKeys",
            "load",
            "()Ljava/lang/Object;",
            None,
            &mut host,
        )
        .unwrap();
    host.heap.next_object = GAME_HOOK_OBJECT_BASE + loader_serial(&first);
    let answer = jar
        .invoke_method_with_args(
            "demo/ServiceKeys",
            "same",
            "(Ljava/lang/Object;)Ljava/lang/String;",
            None,
            &[first],
            &mut host,
        )
        .unwrap();
    assert_eq!(answer, Value::Ref(Some(oracle.trim_end().to_owned())));
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}

// ---------------------------------------------------------------------------------------------
// `Field` copy numbers (`reflect_override.rs`)

/// Each `getDeclaredField` is a new `Field` copy numbered by the heap (Java 25:
/// `tests/fixtures/host_minted_keys/field_copies/java25.txt`). With one number left, the second
/// copy finds the numbers exhausted: counted by name and the call fails, as for every allocation
/// (#667). Before #669 the copy saturated at the mark, unnamed, and the second and third copies
/// shared it.
#[test]
fn field_copies_take_heap_numbers_and_count_running_out() {
    let jar = compile("field_copies", &["host_minted_keys/field_copies"]);
    let oracle =
        std::fs::read_to_string(fixture("host_minted_keys/field_copies/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    let (answer, counted) = run(&jar, &mut host, "demo/FieldCopies", "run");
    assert_eq!(answer, oracle.trim_end());
    assert!(counted.is_empty(), "{counted:?}");

    let mut host = jar.seed_host(StdHost::new());
    host.heap.next_object = usize::MAX - 1;
    let failed = jar
        .invoke_method(
            "demo/FieldCopies",
            "run",
            "()Ljava/lang/String;",
            None,
            &mut host,
        )
        .unwrap_err()
        .to_string();
    assert!(
        failed.contains("allocation numbers are exhausted"),
        "{failed}"
    );
    let counted = host.unknown_host_calls();
    assert!(
        counted
            .iter()
            .any(|(name, _)| name == "java/lang/reflect/Field#<new>: allocation numbers exhausted"),
        "{counted:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Host models' jar objects (`entity_registration.rs`, `vec2.rs`)

/// Each `SynchedEntityData.defineId` is a new `EntityDataAccessor`, even when two holders' ids
/// are equal (real 26.3 server jar on JDK 25.0.5:
/// `tests/fixtures/host_minted_keys/entity_accessors/java25.txt`, command in `command.txt`).
/// Before #669 an accessor's key was `EntityDataAccessor#<id>`, so two unrelated holders' first
/// accessors (both id 0) were one key, `==`, and a number the heap hands out to other objects.
#[test]
fn entity_data_accessors_are_new_objects_whatever_their_id() {
    let jar = compile(
        "entity_accessors",
        &[
            "entity_registration/stubs",
            "host_minted_keys/entity_accessors/demo",
        ],
    );
    let oracle =
        std::fs::read_to_string(fixture("host_minted_keys/entity_accessors/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    let (answer, counted) = run(&jar, &mut host, "demo/AccessorProbe", "rows");
    assert_eq!(answer, oracle.trim_end());
    assert!(counted.is_empty(), "{counted:?}");
}

/// `Vec2`'s constants are the host's objects, numbered by the heap like any other: a heap that
/// already handed out and freed the numbers the model used to mint from its own counter
/// (`Vec2#900000`..) still reads `Vec2.ZERO.x`, as Java 25 does (`fixture_vec2`'s oracle,
/// `tests/fixtures/vec2/java25.txt`). Before #669 `Vec2.ZERO` was born with a freed number, and
/// its first field read failed as "collected while still reachable".
#[test]
fn vec2_constants_are_never_born_collected() {
    let jar = compile(
        "vec2_numbers",
        &["host_minted_keys/vec2_numbers", "vec2/demo/Probe.java"],
    );
    let oracle = std::fs::read_to_string(fixture("vec2/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    host.heap.next_object = 900_000;
    let made = jar
        .invoke_method_with_args(
            "demo/Churn",
            "churn",
            "(I)I",
            None,
            &[Value::Int(2048)],
            &mut host,
        )
        .unwrap();
    assert_eq!(made, Value::Int(2048));
    let (answer, counted) = run(&jar, &mut host, "demo/Probe", "rows");
    assert_eq!(answer, oracle);
    assert!(counted.is_empty(), "{counted:?}");
}

/// A model-layer builder's and layer's keys (handles of their own kinds, numbered by the model)
/// never spell a jar object's allocation key, whose number the collector would read as the
/// heap's. Before #669 they were `cube-builder#<n>` and `layer-definition#<n>`.
#[test]
fn model_layer_keys_are_no_allocation_keys() {
    use crate::runtime::host_ref::{HostKind, HostRef};
    use crate::runtime::model_layers::{CUBE_LIST_BUILDER, LAYER_DEFINITION, MESH_DEFINITION};
    let mut state = crate::runtime::model_layers::State::default();
    let builder = crate::runtime::model_layers::invoke(
        &mut state,
        &crate::runtime::MethodRef {
            owner: CUBE_LIST_BUILDER.into(),
            name: "create".into(),
            descriptor: format!("()L{CUBE_LIST_BUILDER};"),
        },
        &[],
    )
    .unwrap()
    .unwrap();
    let mesh_key = format!("{MESH_DEFINITION}#5");
    crate::runtime::model_layers::ensure_mesh(&mut state, &mesh_key);
    let layer = crate::runtime::model_layers::invoke(
        &mut state,
        &crate::runtime::MethodRef {
            owner: LAYER_DEFINITION.into(),
            name: "create".into(),
            descriptor: format!("(L{MESH_DEFINITION};II)L{LAYER_DEFINITION};"),
        },
        &[
            Value::Host(HostRef::new(HostKind::MeshDefinition, mesh_key)),
            Value::Int(64),
            Value::Int(32),
        ],
    )
    .unwrap()
    .unwrap();
    for value in [builder, layer] {
        let Value::Host(handle) = &value else {
            panic!("{value:?}");
        };
        assert!(
            !crate::runtime::jar_object::spells_key_text(handle.key()),
            "{} spells an allocation key",
            handle.key()
        );
    }
}

/// A jar's own `new ToolMaterial(..)` made after `ToolMaterial.WOOD` is read is another object
/// (real 26.3 server jar on JDK 25.0.5: `tests/fixtures/host_minted_keys/tool_material/java25.txt`,
/// `false|false|59|7`). The record's constructor is the game's, so the new object's fields are
/// unknown: its `durability()` is counted, never answered. Before #669's follow-up the constant
/// was `ToolMaterial#800000` from a counter of its own: with the heap at 800000 the `new` took
/// the same key, `made == WOOD` answered `true` and `made.durability()` WOOD's 59, uncounted.
#[test]
fn a_tool_material_constant_never_shares_a_new_records_key() {
    let jar = compile("tool_material", &["host_minted_keys/tool_material"]);
    let oracle =
        std::fs::read_to_string(fixture("host_minted_keys/tool_material/java25.txt")).unwrap();
    let oracle: Vec<&str> = oracle.trim_end().split('|').collect();
    let mut host = jar.seed_host(StdHost::new());
    host.heap.next_object = 800_000;
    let (answer, counted) = run(&jar, &mut host, "demo/ToolKeys", "run");
    let answer: Vec<&str> = answer.split('|').collect();
    // `made == wood`, `made == ToolMaterial.WOOD`, `wood.durability()`: exact.
    assert_eq!(answer[..3], oracle[..3], "{answer:?}");
    assert!(
        counted
            .iter()
            .any(|(call, _)| call.starts_with("net/minecraft/world/item/ToolMaterial#durability")),
        "the new record's durability() must be counted: {answer:?} {counted:?}"
    );
}

/// The other host models that number handles of their own kinds (a biome modification, a
/// creative tab builder, a resource condition type) key them in a family the collector cannot
/// read as a heap number (#669 review: they were `bm#<n>`, `b#<n>`, `rct#<n>`).
#[test]
fn own_kind_model_keys_are_no_allocation_keys() {
    let method = |owner: &str, name: &str, descriptor: &str| crate::runtime::MethodRef {
        owner: owner.into(),
        name: name.into(),
        descriptor: descriptor.into(),
    };
    let modification = crate::runtime::fabric_biome_modifications::static_call(
        &mut Default::default(),
        &method(
            "net/fabricmc/fabric/api/biome/v1/BiomeModifications",
            "create",
            "(Lnet/minecraft/resources/Identifier;)Lnet/fabricmc/fabric/api/biome/v1/BiomeModification;",
        ),
        &[crate::runtime::host_ids::identifier("demo", "biomes")],
    );
    let builder = crate::runtime::host_creative_tab::static_call(
        &mut Default::default(),
        &method(
            "net/fabricmc/fabric/api/creativetab/v1/FabricCreativeModeTab",
            "builder",
            "()Lnet/minecraft/world/item/CreativeModeTab$Builder;",
        ),
        &[],
    );
    let condition = crate::runtime::fabric_resource_conditions::static_call(
        &mut Default::default(),
        &method(
            "net/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType",
            "create",
            "(Lnet/minecraft/resources/Identifier;Lcom/mojang/serialization/MapCodec;)Lnet/fabricmc/fabric/api/resource/conditions/v1/ResourceConditionType;",
        ),
        &[
            crate::runtime::host_ids::identifier("demo", "condition"),
            Value::Ref(Some("codec".into())),
        ],
    );
    for made in [modification, builder, condition] {
        let Some(Ok(Value::Host(handle))) = &made else {
            panic!("{made:?}");
        };
        assert!(
            !crate::runtime::jar_object::spells_key_text(handle.key()),
            "{} spells an allocation key",
            handle.key()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Enum constants' text (#670)

/// An enum constant in a collection prints its class's own `toString()`: a constant-specific
/// body's override, an enum-wide override, else the name. Every path a jar turns one into text
/// agrees (`EnumSet`, `ArrayList`, `HashMap` and its `values()`, `List.of`,
/// `String.valueOf`, concatenation, `StringBuilder.append(Object)`). Java 25:
/// `tests/fixtures/enum_text/java25.txt` (JDK 25.0.5 run of the same class). Before #670 a
/// collection printed a constant-specific body's constant by its name (`GREEN`), uncounted.
#[test]
fn enum_constants_print_their_own_to_string_in_collections() {
    let jar = compile("enum_text", &["enum_text"]);
    let oracle = std::fs::read_to_string(fixture("enum_text/java25.txt")).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    let (answer, counted) = run(&jar, &mut host, "demo/EnumText", "run");
    assert_eq!(answer, oracle.trim_end());
    assert!(counted.is_empty(), "{counted:?}");
}
