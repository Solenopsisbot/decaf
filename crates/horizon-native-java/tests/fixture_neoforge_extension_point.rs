//! NeoForge's `ModLoadingContext.registerExtensionPoint(Class, Supplier)` (#468).
//!
//! FML can't run outside the game (`getActiveContainer` reads `ModList`), so the oracle is FML
//! 12.0.12's own bytecode: `fixtures/neoforge_extension_point/javap_facts.txt` (command in the
//! README). The test reads those lines and derives each expectation from them:
//! * the call is `getActiveContainer().registerExtensionPoint(point, supplier)`, whose body is
//!   `extensionPoints.put(point, supplier)`, so the supplier is never called;
//! * `extensionPoints` is an `IdentityHashMap`, which takes null keys and values, so a null point
//!   or supplier doesn't throw.
//!
//! Only `demo/` ships, so every call reaches the host, and nothing may be counted.
#[path = "support/javac_fixture.rs"]
mod javac_fixture;
use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const JAVAP: &str = include_str!("fixtures/neoforge_extension_point/javap_facts.txt");
const PROBE: &str = include_str!("fixtures/neoforge_extension_point/demo/Probe.java");

const STUBS: &[(&str, &str)] = &[
    (
        "net/neoforged/fml/IExtensionPoint.java",
        "package net.neoforged.fml; public interface IExtensionPoint {}",
    ),
    (
        "net/neoforged/fml/ModLoadingContext.java",
        "package net.neoforged.fml; public class ModLoadingContext { public static ModLoadingContext get() { return null; } public <T extends IExtensionPoint> void registerExtensionPoint(Class<T> point, java.util.function.Supplier<T> extension) {} }",
    ),
];

#[test]
fn extension_points_register_as_fml_does() {
    // What FML's bytecode says (see the module doc).
    assert!(JAVAP.contains("Method getActiveContainer:()Lnet/neoforged/fml/ModContainer;"));
    assert!(JAVAP.contains("InterfaceMethod java/util/Map.put:"));
    let supplier_called = JAVAP.contains("java/util/function/Supplier.get");
    let nulls_allowed = JAVAP.contains("java/util/IdentityHashMap.\"<init>\"");
    assert!(
        nulls_allowed,
        "javap_facts.txt: extensionPoints is not an IdentityHashMap"
    );
    let null_row = if nulls_allowed {
        "ok"
    } else {
        "java.lang.NullPointerException"
    };
    let expected = format!(
        "register=ok\nsupplierCalled={supplier_called}\nnullPoint={null_row}\nnullSupplier={null_row}\n"
    );

    let mut sources = STUBS.to_vec();
    sources.push(("demo/Probe.java", PROBE));
    let fixture = Fixture::build("neoforge-extension-point", &sources);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(fixture.text_on(&mut host, "rows"), expected);
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}
