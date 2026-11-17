use std::fs;

#[test]
fn mixin_service_capture_has_exact_identity_name_and_side_rows() {
    let capture = fs::read_to_string("tests/fixtures/mixin_service_probe/java25.txt").unwrap();
    for row in ["service_same=true", "fabric_name=Knot/Fabric", "fabric_server_side=SERVER", "neoforge_name=ModLauncher", "neoforge_server_side=SERVER"] {
        assert!(capture.lines().any(|line| line == row), "missing {row}");
    }
}

#[test]
fn mixin_service_bytecode_provider_stays_counted() {
    let capture = fs::read_to_string("tests/fixtures/mixin_service_probe/java25.txt").unwrap();
    assert_eq!(capture.lines().filter(|line| *line == "getBytecodeProvider=COUNTED").count(), 1);
}
