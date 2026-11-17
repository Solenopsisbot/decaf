//! Fabric's `ResourceLoader.registerBuiltinPack` (#478, `fabric_builtin_pack.rs`).
//!
//! `fixtures/fabric_builtin_pack/demo/Probe.java` runs on Java 25 against the real Fabric resource
//! loader with a container rooted at a jar of `resources/` (command in the README), and here as
//! the running jar, which holds the same `resources/`, asking for its own container. `rows()` must
//! equal `java25.txt` with nothing counted. Asking about another mod's container is counted: the
//! host holds only the running jar's files exactly.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/fabric_builtin_pack/demo/Probe.java");
const JAVA25_CAPTURE: &str = include_str!("fixtures/fabric_builtin_pack/java25.txt");

/// The fixture's `resources/`, packed into the probe jar at the same paths.
const RESOURCES: &[(&str, &[u8])] = &[
    (
        "resourcepacks/present/pack.mcmeta",
        include_bytes!("fixtures/fabric_builtin_pack/resources/resourcepacks/present/pack.mcmeta"),
    ),
    (
        "resourcepacks/present/assets/demo/marker.txt",
        include_bytes!(
            "fixtures/fabric_builtin_pack/resources/resourcepacks/present/assets/demo/marker.txt"
        ),
    ),
    (
        "resourcepacks/demo/nested/assets/demo/marker.txt",
        include_bytes!(
            "fixtures/fabric_builtin_pack/resources/resourcepacks/demo/nested/assets/demo/marker.txt"
        ),
    ),
    (
        "resourcepacks/meta_only/pack.mcmeta",
        include_bytes!(
            "fixtures/fabric_builtin_pack/resources/resourcepacks/meta_only/pack.mcmeta"
        ),
    ),
    (
        "resourcepacks/dataonly/data/demo/marker.txt",
        include_bytes!(
            "fixtures/fabric_builtin_pack/resources/resourcepacks/dataonly/data/demo/marker.txt"
        ),
    ),
    (
        "resourcepacks/badns/assets/BadNS/marker.txt",
        include_bytes!(
            "fixtures/fabric_builtin_pack/resources/resourcepacks/badns/assets/BadNS/marker.txt"
        ),
    ),
    (
        "resourcepacks/badns/assets/marker.txt",
        include_bytes!(
            "fixtures/fabric_builtin_pack/resources/resourcepacks/badns/assets/marker.txt"
        ),
    ),
    (
        "resourcepacks/fileonly",
        include_bytes!("fixtures/fabric_builtin_pack/resources/resourcepacks/fileonly"),
    ),
    (
        "packs/custom/assets/demo/marker.txt",
        include_bytes!(
            "fixtures/fabric_builtin_pack/resources/packs/custom/assets/demo/marker.txt"
        ),
    ),
];

/// The probe jar is mod `demo`, the container `rows()` asks for.
const MOD_JSON: &str = r#"{"schemaVersion":1,"id":"demo","version":"1"}"#;

const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/fabric/api/resource/v1/ResourceLoader.java",
        "package net.fabricmc.fabric.api.resource.v1; import net.fabricmc.fabric.api.resource.v1.pack.PackActivationType; import net.fabricmc.loader.api.ModContainer; import net.minecraft.network.chat.Component; import net.minecraft.resources.Identifier; public interface ResourceLoader { static boolean registerBuiltinPack(Identifier id, ModContainer c, Component n, PackActivationType a) { throw new AssertionError(); } static boolean registerBuiltinPack(Identifier id, String sub, ModContainer c, Component n, PackActivationType a) { throw new AssertionError(); } }",
    ),
    (
        "net/fabricmc/fabric/api/resource/v1/pack/PackActivationType.java",
        "package net.fabricmc.fabric.api.resource.v1.pack; public enum PackActivationType { NORMAL, DEFAULT_ENABLED, ALWAYS_ENABLED }",
    ),
    (
        "net/fabricmc/loader/api/FabricLoader.java",
        "package net.fabricmc.loader.api; public interface FabricLoader { static FabricLoader getInstance() { throw new AssertionError(); } java.util.Optional<ModContainer> getModContainer(String id); }",
    ),
    (
        "net/fabricmc/loader/api/ModContainer.java",
        "package net.fabricmc.loader.api; public interface ModContainer {}",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier fromNamespaceAndPath(String n, String p) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/network/chat/Component.java",
        "package net.minecraft.network.chat; public interface Component { static MutableComponent literal(String s) { throw new AssertionError(); } }",
    ),
    (
        "net/minecraft/network/chat/MutableComponent.java",
        "package net.minecraft.network.chat; public interface MutableComponent extends Component {}",
    ),
];

const REGISTER: &str = "net/fabricmc/fabric/api/resource/v1/ResourceLoader#registerBuiltinPack(Lnet/minecraft/resources/Identifier;Lnet/fabricmc/loader/api/ModContainer;Lnet/minecraft/network/chat/Component;Lnet/fabricmc/fabric/api/resource/v1/pack/PackActivationType;)Z";
const REGISTER_SUB_PATH: &str = "net/fabricmc/fabric/api/resource/v1/ResourceLoader#registerBuiltinPack(Lnet/minecraft/resources/Identifier;Ljava/lang/String;Lnet/fabricmc/loader/api/ModContainer;Lnet/minecraft/network/chat/Component;Lnet/fabricmc/fabric/api/resource/v1/pack/PackActivationType;)Z";

fn fixture() -> Fixture {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    Fixture::build_with_mod_json_and_extra("fabric-builtin-pack", &sources, MOD_JSON, RESOURCES)
}

#[test]
fn own_container_rows_match_java25() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    let unknown = host.unknown_host_calls();
    assert_eq!(rows, JAVA25_CAPTURE, "unknown calls: {unknown:?}");
    assert_eq!(unknown, Vec::<(String, usize)>::new());
}

#[test]
fn another_mods_container_is_counted() {
    // The host has the container of `fabricloader`, a mod it always reports, but not its files:
    // every registration against it is counted (eight of one overload, two of the other).
    let probe = PROBE.replace(
        "getModContainer(\"demo\")",
        "getModContainer(\"fabricloader\")",
    );
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", &probe));
    let other = Fixture::build_with_mod_json_and_extra(
        "fabric-builtin-pack-other",
        &sources,
        MOD_JSON,
        RESOURCES,
    );
    let mut host_other = other.jar.seed_host(StdHost::new());
    other.text_on(&mut host_other, "rows");
    let mut unknown = host_other.unknown_host_calls();
    unknown.sort();
    let mut expected = vec![(REGISTER.to_owned(), 8), (REGISTER_SUB_PATH.to_owned(), 2)];
    expected.sort();
    assert_eq!(unknown, expected);
}
