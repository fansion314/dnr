use dnr_package::{DEFAULT_CACHE_BYTES, PackOptions, Package, PackageConfig, pack_with_config};
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
    let host = dnr_package::config::Target::host();
    let config: PackageConfig = serde_json::from_value(serde_json::json!({
        "schemaVersion": 1, "targets": { host.id(): host },
        "groups": [{ "id": "addon", "files": ["node_modules/addon/**"],
            "native": {"addons": [{"path": NAME, "napi": 8}]} }]
    }))
    .unwrap();
    pack_with_config(
        &PackOptions {
            directory: source,
            entry: "main.ts".into(),
            output: package.clone(),
            includes: vec![],
            excludes: vec![],
            app_id: None,
            force: false,
        },
        &config,
    )
    .unwrap();
    (temp, package)
}

const NAME: &str = "node_modules/addon/build/addon.node";

fn open(path: &std::path::Path, budget: usize) -> Package {
    Package::open(path, budget)
        .unwrap()
        .with_cache_directory(&path.parent().unwrap().join("cache"))
        .unwrap()
}

#[test]
fn extracts_once_to_cache_and_ignores_existing_sibling() {
    let (temp, path) = fixture();
    fs::create_dir_all(temp.path().join("node_modules/addon/build")).unwrap();
    fs::write(temp.path().join(NAME), b"disk override").unwrap();
    let package = open(&path, DEFAULT_CACHE_BYTES);
    assert!(
        package
            .native_library_path("missing.node")
            .unwrap()
            .is_none()
    );
    assert!(package.native_library_path("node_modules").is_err());
    assert!(package.native_library_path("../escape.node").is_err());
    let extracted = package.native_library_path(NAME).unwrap().unwrap();
    assert!(extracted.starts_with(temp.path().join("cache").canonicalize().unwrap()));
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
    assert_eq!(package.native_cache_stats().extractions, 1);
    let reopened = open(&path, 0);
    let second = reopened.native_library_path(NAME).unwrap().unwrap();
    assert_eq!(extracted, second);
    assert_eq!(fs::read(second).unwrap(), b"packaged library");
    assert_eq!(fs::read(temp.path().join(NAME)).unwrap(), b"disk override");
}

#[test]
fn workers_share_stable_cache_and_release_leases() {
    let (temp, path) = fixture();
    fs::write(temp.path().join("node_modules"), "do not replace").unwrap();
    let package = Arc::new(open(&path, 0));
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
    assert!(paths[0].starts_with(temp.path().join("cache").canonicalize().unwrap()));
    assert_eq!(fs::read(&paths[0]).unwrap(), b"packaged library");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&paths[0]).unwrap().permissions().mode() & 0o222,
            0
        );
    }
    let other = open(&path, 0);
    let other_path = other.native_library_path(NAME).unwrap().unwrap();
    assert_eq!(paths[0], other_path);
    package.cleanup_native_libraries();
    assert!(paths[0].exists());
    assert!(other_path.exists());
    drop(other);
    assert!(other_path.exists());
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
    let package = open(&path, 0);
    let extracted = package.native_library_path(NAME).unwrap().unwrap();
    assert!(extracted.starts_with(temp.path().join("cache").canonicalize().unwrap()));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    fs::remove_file(temp.path().join("node_modules")).unwrap();
    fs::create_dir_all(temp.path().join("node_modules/addon/build")).unwrap();
    let target = outside.path().join("existing.node");
    fs::write(&target, b"outside").unwrap();
    std::os::unix::fs::symlink(&target, temp.path().join(NAME)).unwrap();
    let reopened = open(&path, 0);
    let extracted = reopened.native_library_path(NAME).unwrap().unwrap();
    assert!(extracted.starts_with(temp.path().join("cache").canonicalize().unwrap()));
    assert_eq!(fs::read(target).unwrap(), b"outside");
}
