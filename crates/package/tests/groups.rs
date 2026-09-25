use dnr_package::{NativeUse, PackOptions, Package, PackageConfig, pack_with_config};
use std::{fs, path::Path, sync::Arc};

fn fixture() -> (tempfile::TempDir, PackOptions, PackageConfig) {
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("app");
    fs::create_dir_all(app.join("plugin/data")).unwrap();
    fs::write(app.join("main.ts"), "import './plugin/wrapper.js';").unwrap();
    fs::write(
        app.join("plugin/wrapper.js"),
        "export const data = new URL('./data/a', import.meta.url);",
    )
    .unwrap();
    fs::write(app.join("plugin/data/a"), "resource").unwrap();
    fs::write(app.join("plugin/addon.node"), "fixture native bytes").unwrap();
    fs::write(app.join("plugin/worker"), "#!/bin/sh\nexit 0\n").unwrap();
    std::os::unix::fs::symlink("data/a", app.join("plugin/link")).unwrap();
    let host = dnr_package::config::Target::host();
    let mut config: PackageConfig = serde_json::from_value(serde_json::json!({
        "schemaVersion": 1, "targets": { host.id(): host },
        "groups": [{ "id": "plugin", "files": ["plugin/**"], "native": {
            "addons": [{"path": "plugin/addon.node", "napi": 8}], "executables": ["plugin/worker"]
        }}]
    }))
    .unwrap();
    config.base_dir = temp.path().to_owned();
    let options = PackOptions {
        directory: app,
        entry: "main.ts".into(),
        output: temp.path().join("app.dnp"),
        includes: vec![],
        excludes: vec![],
        app_id: Some("groups.test".into()),
        force: false,
    };
    (temp, options, config)
}
fn open(options: &PackOptions, cache: &Path) -> Package {
    Package::open(&options.output, 0)
        .unwrap()
        .with_cache_directory(cache)
        .unwrap()
}

#[test]
fn lazy_group_is_complete_readonly_and_reused_between_instances() {
    let (temp, options, config) = fixture();
    pack_with_config(&options, &config).unwrap();
    let cache = temp.path().join("cache");
    let package = open(&options, &cache);
    assert_eq!(package.manifest.format_version, 3);
    assert_eq!(
        &*package.read("plugin/data/a").unwrap().unwrap(),
        b"resource"
    );
    let module = package.module_path("plugin/wrapper.js").unwrap().unwrap();
    assert!(
        !module.exists(),
        "reserving a group URL must not extract files"
    );
    assert_eq!(package.logical_path(&module).unwrap(), "plugin/wrapper.js");
    let native = package
        .native_library_path("plugin/addon.node")
        .unwrap()
        .unwrap();
    assert!(module.exists());
    let root = native.parent().unwrap();
    assert_eq!(fs::read(root.join("data/a")).unwrap(), b"resource");
    assert_eq!(fs::read(root.join("link")).unwrap(), b"resource");
    assert!(
        package
            .native_path("plugin/worker", NativeUse::Executable)
            .unwrap()
            .unwrap()
            .exists()
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(root.join("worker"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o555
    );
    let before = fs::metadata(&native).unwrap().modified().unwrap();
    package.cleanup_native_libraries();
    assert!(native.exists());
    let reopened = open(&options, &cache);
    assert_eq!(
        reopened
            .native_library_path("plugin/addon.node")
            .unwrap()
            .unwrap(),
        native
    );
    assert_eq!(fs::metadata(native).unwrap().modified().unwrap(), before);
    assert_eq!(reopened.stats().decompressions, 0);
}

#[test]
fn declared_use_and_group_membership_are_enforced() {
    let (temp, options, mut config) = fixture();
    let missing = PackageConfig {
        groups: vec![],
        targets: Default::default(),
        ..Default::default()
    };
    assert!(pack_with_config(&options, &missing).is_err());
    config.groups.push(config.groups[0].clone());
    assert!(pack_with_config(&options, &config).is_err());
    config.groups.pop();
    pack_with_config(&options, &config).unwrap();
    let package = open(&options, &temp.path().join("cache"));
    assert!(package.native_library_path("plugin/data/a").is_err());
    assert!(
        package
            .native_path("plugin/addon.node", NativeUse::Executable)
            .is_err()
    );
    assert!(package.native_library_path("missing.so").unwrap().is_none());
}

#[test]
fn parallel_instances_reuse_one_group() {
    let (temp, options, config) = fixture();
    pack_with_config(&options, &config).unwrap();
    let cache = temp.path().join("cache");
    // Start with no files on disk: all instances race to prepare the same group.
    let first = open(&options, &cache);
    let expected = first.module_path("plugin/addon.node").unwrap().unwrap();
    let packages: Vec<_> = (0..8).map(|_| Arc::new(open(&options, &cache))).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = packages
            .iter()
            .map(|p| s.spawn(move || p.native_library_path("plugin/addon.node").unwrap().unwrap()))
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), expected);
        }
    });
}

#[test]
fn cache_corruption_is_repaired_and_content_changes_identity() {
    let (temp, mut options, config) = fixture();
    pack_with_config(&options, &config).unwrap();
    let cache = temp.path().join("cache");
    let first = open(&options, &cache);
    let id = first.package_id().to_owned();
    let native = first
        .native_library_path("plugin/addon.node")
        .unwrap()
        .unwrap();
    drop(first);
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&native, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&native, "corrupt").unwrap();
    let repaired = open(&options, &cache);
    assert_eq!(
        fs::read(
            repaired
                .native_library_path("plugin/addon.node")
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        b"fixture native bytes"
    );
    drop(repaired);
    fs::write(options.directory.join("plugin/data/a"), "changed").unwrap();
    options.force = true;
    pack_with_config(&options, &config).unwrap();
    let new = open(&options, &cache);
    assert_ne!(new.package_id(), id);
    assert_ne!(
        new.native_library_path("plugin/addon.node")
            .unwrap()
            .unwrap(),
        native
    );
}

#[test]
fn platform_variants_share_logical_paths_but_only_selected_files_materialize() {
    let (temp, options, mut config) = fixture();
    fs::remove_file(options.directory.join("plugin/addon.node")).unwrap();
    fs::create_dir(temp.path().join("mac")).unwrap();
    fs::create_dir(temp.path().join("linux")).unwrap();
    fs::write(temp.path().join("mac/addon.node"), "mac native").unwrap();
    fs::write(temp.path().join("linux/addon.node"), "linux native").unwrap();
    config.targets = serde_json::from_value(serde_json::json!({"darwin_arm64":{"os":"darwin","arch":"arm64"},"linux_x64_glibc":{"os":"linux","arch":"x64","libc":"glibc"}})).unwrap();
    config.groups[0].variants = serde_json::from_value(serde_json::json!({"darwin_arm64":[{"from":"mac","to":"plugin"}],"linux_x64_glibc":[{"from":"linux","to":"plugin"}]})).unwrap();
    pack_with_config(&options, &config).unwrap();
    for (target, expected) in [
        ("darwin_arm64", "mac native"),
        ("linux_x64_glibc", "linux native"),
    ] {
        let package = Package::open_for_target(&options.output, 0, Some(target))
            .unwrap()
            .with_cache_directory(&temp.path().join("cache"))
            .unwrap();
        assert_eq!(
            &*package.read("plugin/addon.node").unwrap().unwrap(),
            expected.as_bytes()
        );
        let file = package
            .native_library_path("plugin/addon.node")
            .unwrap()
            .unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(), expected);
    }
    let package = Package::open(&options.output, 0).unwrap();
    let mut tree = Vec::new();
    package.write_tree(&mut tree).unwrap();
    let text = String::from_utf8(tree).unwrap();
    assert!(!text.contains("payloads"));
    assert_eq!(text.matches("addon.node").count(), 2);
    package.extract_to(&temp.path().join("raw")).unwrap();
}

#[test]
fn sidecar_install_wins_and_clean_skips_active_groups() {
    let (temp, options, config) = fixture();
    pack_with_config(&options, &config).unwrap();
    let dest = temp.path().join("installed");
    dnr_package::commands::run(&[
        "install".into(),
        options.output.display().to_string(),
        dest.display().to_string(),
    ])
    .unwrap();
    let installed = Package::open(&dest.join("app.dnp"), 0)
        .unwrap()
        .with_cache_directory(&temp.path().join("unused-cache"))
        .unwrap();
    let native = installed
        .native_library_path("plugin/addon.node")
        .unwrap()
        .unwrap();
    assert!(native.starts_with(dest.canonicalize().unwrap().join("app.dnp.unpacked")));
    let clean = vec![
        "cache".into(),
        "clean".into(),
        "--all".into(),
        "--directory".into(),
        dest.display().to_string(),
    ];
    dnr_package::commands::run(&clean).unwrap();
    assert!(native.exists());
    drop(installed);
    dnr_package::commands::run(&clean).unwrap();
    assert!(!native.exists());
    assert!(dest.join("app.dnp").exists());
}

#[test]
fn symlink_outside_group_is_rejected() {
    let (_temp, options, config) = fixture();
    std::os::unix::fs::symlink("../main.ts", options.directory.join("plugin/outside")).unwrap();
    assert!(pack_with_config(&options, &config).is_err());
}

#[test]
fn valid_zip_crc_does_not_replace_the_content_checksum() {
    use std::io::{Read, Write};
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
    let (temp, options, config) = fixture();
    pack_with_config(&options, &config).unwrap();
    let mut input = fs::File::open(&options.output).unwrap();
    let offset = dnr_package::zip_offset(&mut input).unwrap();
    let mut archive = ZipArchive::new(dnr_package::Region::new(input, offset).unwrap()).unwrap();
    let corrupt = temp.path().join("corrupt.dnp");
    let mut file = fs::File::create(&corrupt).unwrap();
    file.write_all(&fs::read(&options.output).unwrap()[..offset as usize])
        .unwrap();
    let mut writer = ZipWriter::new(dnr_package::Region::new(file, offset).unwrap());
    for i in 0..archive.len() {
        let mut source = archive.by_index(i).unwrap();
        let name = source.name().to_owned();
        let mode = source.unix_mode().unwrap_or(0o644);
        let opt = SimpleFileOptions::default()
            .compression_method(if name == dnr_package::v3::META {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Zstd
            })
            .unix_permissions(mode & 0o777);
        let mut bytes = Vec::new();
        source.read_to_end(&mut bytes).unwrap();
        if source.is_dir() {
            writer.add_directory(name, opt).unwrap();
        } else if mode & 0o170000 == 0o120000 {
            writer
                .add_symlink(name, String::from_utf8(bytes).unwrap(), opt)
                .unwrap();
        } else {
            if name == "plugin/addon.node" {
                bytes[0] ^= 1;
            }
            writer.start_file(name, opt).unwrap();
            writer.write_all(&bytes).unwrap();
        }
    }
    writer.finish().unwrap();
    let package = Package::open(&corrupt, 0)
        .unwrap()
        .with_cache_directory(&temp.path().join("cache"))
        .unwrap();
    assert!(package.read("plugin/addon.node").is_err());
    let planned = package.module_path("plugin/addon.node").unwrap().unwrap();
    assert!(package.native_library_path("plugin/addon.node").is_err());
    assert!(!planned.exists());
    assert!(package.extract_to(&temp.path().join("export")).is_err());
}

#[test]
fn scanner_uses_npm_platform_and_bin_metadata_without_executing_files() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("node_modules/helper");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("package.json"),
        r#"{"bin":"cli","os":["linux"],"cpu":["arm64"],"libc":"musl"}"#,
    )
    .unwrap();
    fs::write(
        package.join("cli"),
        "console.log('must not run during scan')",
    )
    .unwrap();
    let (config, notes) = dnr_package::config::scan(temp.path()).unwrap();
    assert!(config.targets.contains_key("linux_arm64_musl"));
    assert_eq!(config.groups.len(), 1);
    assert_eq!(
        config.groups[0].native.executables,
        vec!["node_modules/helper/cli"]
    );
    assert!(notes.iter().any(|n| n.contains("candidate")));
}

#[test]
fn install_skips_groups_only_available_on_other_platforms() {
    let (temp, options, mut config) = fixture();
    let other: dnr_package::config::Target = serde_json::from_value(if cfg!(target_os = "macos") {
        serde_json::json!({"os":"linux","arch":"x64","libc":"glibc"})
    } else {
        serde_json::json!({"os":"darwin","arch":"arm64"})
    })
    .unwrap();
    let target = other.id();
    config.targets.insert(target.clone(), other);
    fs::write(temp.path().join("foreign-tool"), "#!/bin/sh\nexit 0\n").unwrap();
    config.groups.push(serde_json::from_value(serde_json::json!({"id":"foreign","variants":{target:[{"from":"foreign-tool","to":"foreign/tool"}]},"native":{"executables":["foreign/tool"]}})).unwrap());
    pack_with_config(&options, &config).unwrap();
    dnr_package::commands::run(&[
        "install".into(),
        options.output.display().to_string(),
        temp.path().join("installed").display().to_string(),
    ])
    .unwrap();
    let package = Package::open(&options.output, 0).unwrap();
    assert!(
        package
            .native_path("foreign/tool", NativeUse::Executable)
            .is_err()
    );
}

#[test]
fn lookups_do_not_probe_per_file_and_warm_groups_validate_once() {
    let (temp, options, mut config) = fixture();
    fs::create_dir(options.directory.join("other")).unwrap();
    fs::write(
        options.directory.join("other/wrapper.js"),
        "export default 1;",
    )
    .unwrap();
    fs::write(
        options.directory.join("other/addon.node"),
        "another library",
    )
    .unwrap();
    config.groups.push(serde_json::from_value(serde_json::json!({"id":"other","files":["other/**"],"native":{"addons":[{"path":"other/addon.node","napi":8}]}})).unwrap());
    pack_with_config(&options, &config).unwrap();
    let cache = temp.path().join("cache");
    let package = open(&options, &cache);
    for _ in 0..100 {
        assert!(package.module_path("main.ts").unwrap().is_none());
    }
    assert_eq!(package.native_cache_stats(), Default::default());
    let a = package.module_path("plugin/wrapper.js").unwrap().unwrap();
    let b = package.module_path("other/wrapper.js").unwrap().unwrap();
    assert_eq!(package.native_cache_stats().sidecar_probes, 1);
    for _ in 0..100 {
        assert_eq!(
            package.logical_path(&a).as_deref(),
            Some("plugin/wrapper.js")
        );
        assert_eq!(
            package.logical_path(&b).as_deref(),
            Some("other/wrapper.js")
        );
        assert_eq!(
            package.module_path("plugin/wrapper.js").unwrap().as_ref(),
            Some(&a)
        );
    }
    assert!(
        !a.exists() && !b.exists(),
        "URL reservation must not extract payloads"
    );
    let plain = package.root.join("main.ts");
    assert!(matches!(
        package.virtual_path_cow(&plain),
        std::borrow::Cow::Borrowed(_)
    ));
    let relative = Path::new("../outside");
    assert_eq!(package.virtual_path_cow(relative).as_ref(), relative);
    let disk = Path::new("/outside/link/../file");
    assert_eq!(package.virtual_path_cow(disk).as_ref(), disk);
    package.native_library_path("plugin/addon.node").unwrap();
    assert_eq!(package.native_cache_stats().extractions, 1);
    let reopened = open(&options, &cache);
    reopened.native_library_path("plugin/addon.node").unwrap();
    let stats = reopened.native_cache_stats();
    assert_eq!(stats.verifications, 1);
    assert_eq!(stats.files_hashed, 4);
    assert_eq!(stats.extractions, 0);
    for _ in 0..100 {
        reopened.native_library_path("plugin/addon.node").unwrap();
    }
    assert_eq!(reopened.native_cache_stats(), stats);
}

#[test]
fn sidecar_binding_retains_its_lease_and_does_not_hash_twice() {
    let (temp, options, config) = fixture();
    pack_with_config(&options, &config).unwrap();
    let dest = temp.path().join("installed");
    dnr_package::commands::run(&[
        "install".into(),
        options.output.display().to_string(),
        dest.display().to_string(),
    ])
    .unwrap();
    let package = Package::open(&dest.join("app.dnp"), 0).unwrap();
    let module = package.module_path("plugin/wrapper.js").unwrap().unwrap();
    let before = package.native_cache_stats();
    assert_eq!(before.verifications, 1);
    assert_eq!(before.files_hashed, 4);
    dnr_package::commands::run(&[
        "cache".into(),
        "clean".into(),
        "--all".into(),
        "--directory".into(),
        dest.display().to_string(),
    ])
    .unwrap();
    assert!(
        module.exists(),
        "an issued sidecar URL must stay pinned even before native load"
    );
    package.native_library_path("plugin/addon.node").unwrap();
    assert_eq!(package.native_cache_stats(), before);
}

#[test]
fn single_pass_validation_detects_missing_extra_and_redirected_members() {
    let (temp, options, config) = fixture();
    pack_with_config(&options, &config).unwrap();
    let cache = temp.path().join("cache");
    let first = open(&options, &cache);
    let native = first
        .native_library_path("plugin/addon.node")
        .unwrap()
        .unwrap();
    let folder = native.parent().unwrap().to_owned();
    drop(first);
    fs::remove_file(folder.join("data/a")).unwrap();
    drop(
        open(&options, &cache)
            .native_library_path("plugin/addon.node")
            .unwrap(),
    );
    assert_eq!(fs::read(folder.join("data/a")).unwrap(), b"resource");
    fs::write(folder.join("injected-library.so"), "unexpected").unwrap();
    drop(
        open(&options, &cache)
            .native_library_path("plugin/addon.node")
            .unwrap(),
    );
    assert!(!folder.join("injected-library.so").exists());
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("a"), "outside").unwrap();
    fs::remove_dir_all(folder.join("data")).unwrap();
    std::os::unix::fs::symlink(&outside, folder.join("data")).unwrap();
    drop(
        open(&options, &cache)
            .native_library_path("plugin/addon.node")
            .unwrap(),
    );
    assert!(
        !fs::symlink_metadata(folder.join("data"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(outside.join("a")).unwrap(), b"outside");
}

#[test]
fn current_implicit_directories_are_not_hidden_by_foreign_explicit_directories() {
    let (temp, options, mut config) = fixture();
    fs::create_dir(temp.path().join("linux-native")).unwrap();
    fs::write(temp.path().join("mac.node"), "mac native").unwrap();
    fs::write(temp.path().join("linux-native/addon.node"), "linux native").unwrap();
    config.targets=serde_json::from_value(serde_json::json!({"darwin_arm64":{"os":"darwin","arch":"arm64"},"linux_x64_glibc":{"os":"linux","arch":"x64","libc":"glibc"}})).unwrap();
    config.groups.push(serde_json::from_value(serde_json::json!({"id":"variant","variants":{"darwin_arm64":[{"from":"mac.node","to":"variant/addon.node"}],"linux_x64_glibc":[{"from":"linux-native","to":"variant"}]},"native":{"addons":[{"path":"variant/addon.node","napi":8}]}})).unwrap());
    pack_with_config(&options, &config).unwrap();
    let package = Package::open_for_target(&options.output, 0, Some("darwin_arm64")).unwrap();
    assert!(!package.is_unavailable("variant"));
    assert_eq!(
        &*package.read("variant/addon.node").unwrap().unwrap(),
        b"mac native"
    );
}
