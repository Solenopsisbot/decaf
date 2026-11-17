//! Fabric's `BiomeModifications.create` and `BiomeModification.add` (#476,
//! `fabric_biome_modifications.rs`).
//!
//! `fixtures/biome_modifications/demo/Probe.java` runs on Java 25 against the real Fabric Biome
//! API and the 26.3 jar (command in the README), and here against compile-only stubs: only `demo/`
//! ships, so every call reaches the host. `rows()` must equal `java25.txt` with nothing counted.
//! The other probe methods are cases Decaf counts rather than answers; Java 25's answers are in
//! `java25_counted.txt`, and each one must count exactly the call named here.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/biome_modifications/demo/Probe.java");
const JAVA25_CAPTURE: &str = include_str!("fixtures/biome_modifications/java25.txt");
const JAVA25_COUNTED: &str = include_str!("fixtures/biome_modifications/java25_counted.txt");

const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public final class Identifier { public static Identifier parse(String s) { throw new AssertionError(); } }",
    ),
    (
        "net/fabricmc/fabric/api/biome/v1/BiomeSelectionContext.java",
        "package net.fabricmc.fabric.api.biome.v1; public interface BiomeSelectionContext {}",
    ),
    (
        "net/fabricmc/fabric/api/biome/v1/BiomeModificationContext.java",
        "package net.fabricmc.fabric.api.biome.v1; public interface BiomeModificationContext {}",
    ),
    (
        "net/fabricmc/fabric/api/biome/v1/ModificationPhase.java",
        "package net.fabricmc.fabric.api.biome.v1; public enum ModificationPhase { ADDITIONS, REMOVALS, REPLACEMENTS, POST_PROCESSING }",
    ),
    (
        "net/fabricmc/fabric/api/biome/v1/BiomeModification.java",
        "package net.fabricmc.fabric.api.biome.v1; import java.util.function.*; public class BiomeModification { BiomeModification() {} public BiomeModification add(ModificationPhase p, Predicate<BiomeSelectionContext> s, Consumer<BiomeModificationContext> m) { throw new AssertionError(); } public BiomeModification add(ModificationPhase p, Predicate<BiomeSelectionContext> s, BiConsumer<BiomeSelectionContext, BiomeModificationContext> m) { throw new AssertionError(); } }",
    ),
    (
        "net/fabricmc/fabric/api/biome/v1/BiomeModifications.java",
        "package net.fabricmc.fabric.api.biome.v1; import net.minecraft.resources.Identifier; public final class BiomeModifications { public static BiomeModification create(Identifier id) { throw new AssertionError(); } }",
    ),
];

const ADD: &str = "net/fabricmc/fabric/api/biome/v1/BiomeModification#add(Lnet/fabricmc/fabric/api/biome/v1/ModificationPhase;Ljava/util/function/Predicate;Ljava/util/function/Consumer;)Lnet/fabricmc/fabric/api/biome/v1/BiomeModification;";
const ADD_CONTEXT: &str = "net/fabricmc/fabric/api/biome/v1/BiomeModification#add(Lnet/fabricmc/fabric/api/biome/v1/ModificationPhase;Ljava/util/function/Predicate;Ljava/util/function/BiConsumer;)Lnet/fabricmc/fabric/api/biome/v1/BiomeModification;";

fn fixture() -> Fixture {
    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    Fixture::build("fabric-biome-modifications", &sources)
}

#[test]
fn biome_modification_rows_match_java25() {
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    let rows = fixture.text_on(&mut host, "rows");
    let unknown = host.unknown_host_calls();
    assert_eq!(rows, JAVA25_CAPTURE, "unknown calls: {unknown:?}");
    assert_eq!(unknown, Vec::<(String, usize)>::new());
}

#[test]
fn null_modifiers_are_counted() {
    let fixture = fixture();
    // (probe method, the one call it must count). Java 25 throws `NullPointerException` in each,
    // but a null here may be a counted call's answer, so Decaf doesn't throw it.
    let cases = [
        ("nullSelector", ADD),
        ("nullModifier", ADD),
        ("nullContextModifier", ADD_CONTEXT),
    ];
    // Every case the capture holds is asserted here, and no other.
    let captured: Vec<&str> = JAVA25_COUNTED
        .lines()
        .map(|line| line.split_once('=').expect("method=answer").0)
        .collect();
    assert_eq!(captured, cases.map(|(method, _)| method));
    for (method, counted) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        fixture.text_on(&mut host, method);
        assert_eq!(
            host.unknown_host_calls(),
            vec![(counted.to_owned(), 1)],
            "{method}"
        );
    }
}
