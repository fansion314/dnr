use dnr_package::{PackOptions, pack};
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires native dnr; set DNR_BIN and pass --ignored"]
fn numeric_open_flags_from_disk_and_package() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let entry = source.join("main.mjs");
    fs::write(
        &entry,
        include_str!("../../../examples/node-open-flags.mjs"),
    )
    .unwrap();
    let package = temp.path().join("flags.dnp");
    pack(&PackOptions {
        directory: source,
        entry: "main.mjs".into(),
        output: package.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("test.node-open-flags".into()),
        force: false,
    })
    .unwrap();
    for input in [entry, package] {
        let output = Command::new(&binary).arg(input).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout)
                .matches("NODE_OPEN_FLAGS_OK")
                .count(),
            3
        );
    }
}
