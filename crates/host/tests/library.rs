use std::fs;

#[test]
fn generates_all_library_modules_and_declarations() {
    let root = std::env::temp_dir().join(format!(
        "croon-library-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    host::library::generate(&root);
    let kernel = fs::read_to_string(root.join("kernel.py")).unwrap();
    let model = fs::read_to_string(root.join("model.py")).unwrap();
    let directory = fs::read_to_string(root.join("directory.py")).unwrap();
    let infer = fs::read_to_string(root.join("infer.py")).unwrap();
    assert!(kernel.contains("Outcome = Schema("));
    for name in [
        "Pack",
        "Construct",
        "Instantiate",
        "Look",
        "Notify",
        "Correct",
        "Revoke",
    ] {
        assert!(
            kernel.contains(&format!("{name} = Port(")),
            "missing {name}"
        );
    }
    assert!(model.contains("Generate = Port("));
    assert!(directory.contains("Exec = Port("));
    assert!(infer.contains("Infer = Port("));
    fs::remove_dir_all(root).unwrap();
}
