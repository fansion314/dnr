use dnr_package::{DEFAULT_CACHE_BYTES, PackOptions, Package, pack};
use std::{fs, path::PathBuf, sync::Arc};

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("node_modules/addon/build")).unwrap();
    fs::write(source.join("main.ts"), "").unwrap();
    fs::write(source.join(NAME), b"packaged library").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        "build/addon.node",
        source.join("node_modules/addon/link.node"),
    )
    .unwrap();
    let package = temp.path().join("app.dnp");
    pack(&PackOptions {
        directory: source,
        entry: "main.ts".into(),
        output: package.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: None,
        force: false,
    })
    .unwrap();
    (temp, package)
}

const NAME: &str = "node_modules/addon/build/addon.node";

#[test]
fn extracts_once_to_tmp_and_ignores_existing_sibling() {
    let (temp, path) = fixture();
    fs::create_dir_all(temp.path().join("node_modules/addon/build")).unwrap();
    fs::write(temp.path().join(NAME), b"disk override").unwrap();
    let package = Package::open(&path, DEFAULT_CACHE_BYTES).unwrap();
    assert!(
        package
            .native_library_path("missing.node")
            .unwrap()
            .is_none()
    );
    assert!(package.native_library_path("node_modules").is_err());
    assert!(package.native_library_path("../escape.node").is_err());
    let extracted = package.native_library_path(NAME).unwrap().unwrap();
    assert!(!extracted.starts_with(&package.root));
    assert_eq!(fs::read(&extracted).unwrap(), b"packaged library");
    assert_eq!(
        package.native_library_path(NAME).unwrap(),
        Some(extracted.clone())
    );
    #[cfg(unix)]
    assert_eq!(
        package
            .native_library_path("node_modules/addon/link.node")
            .unwrap(),
        Some(extracted.clone())
    );
    assert_eq!(package.stats().decompressions, 1);
    let reopened = Package::open(&path, 0).unwrap();
    let second = reopened.native_library_path(NAME).unwrap().unwrap();
    assert_ne!(extracted, second);
    assert_eq!(fs::read(second).unwrap(), b"packaged library");
    assert_eq!(fs::read(temp.path().join(NAME)).unwrap(), b"disk override");
}

#[test]
fn workers_share_private_stable_tmp_and_cleanup() {
    let (temp, path) = fixture();
    fs::write(temp.path().join("node_modules"), "do not replace").unwrap();
    let package = Arc::new(Package::open(&path, 0).unwrap());
    let paths = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let package = package.clone();
                scope.spawn(move || package.native_library_path(NAME).unwrap().unwrap())
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(paths.iter().all(|p| p == &paths[0]));
    assert!(paths[0].ends_with(NAME));
    assert!(!paths[0].starts_with(&package.root));
    assert_eq!(fs::read(&paths[0]).unwrap(), b"packaged library");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let root = paths[0].ancestors().nth(4).unwrap();
        assert_eq!(
            fs::metadata(root).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    let other = Package::open(&path, 0).unwrap();
    let other_path = other.native_library_path(NAME).unwrap().unwrap();
    assert_ne!(paths[0], other_path);
    package.cleanup_native_libraries();
    assert!(!paths[0].exists());
    assert!(other_path.exists());
    drop(other);
    assert!(!other_path.exists());
    assert_eq!(
        fs::read(temp.path().join("node_modules")).unwrap(),
        b"do not replace"
    );
}

#[test]
#[cfg(unix)]
fn disk_symlinks_do_not_redirect_extraction_or_reuse() {
    let (temp, path) = fixture();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), temp.path().join("node_modules")).unwrap();
    let package = Package::open(&path, 0).unwrap();
    let extracted = package.native_library_path(NAME).unwrap().unwrap();
    assert!(!extracted.starts_with(&package.root));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    fs::remove_file(temp.path().join("node_modules")).unwrap();
    fs::create_dir_all(temp.path().join("node_modules/addon/build")).unwrap();
    let target = outside.path().join("existing.node");
    fs::write(&target, b"outside").unwrap();
    std::os::unix::fs::symlink(&target, temp.path().join(NAME)).unwrap();
    let reopened = Package::open(&path, 0).unwrap();
    let extracted = reopened.native_library_path(NAME).unwrap().unwrap();
    assert!(!extracted.starts_with(&package.root));
    assert_eq!(fs::read(target).unwrap(), b"outside");
}
