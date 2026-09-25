use dnr_package::{
    PackOptions, Package, PackageConfig, pack_with_config,
    persistent::{Generation, SourceStamp, hash},
};
use std::{fs, path::Path};

fn build(root: &Path, output: &Path) {
    pack_with_config(
        &PackOptions {
            directory: root.into(),
            entry: "main.ts".into(),
            output: output.into(),
            includes: vec![],
            excludes: vec![],
            app_id: Some("test.v3".into()),
            force: true,
        },
        &PackageConfig::default(),
    )
    .unwrap();
}
#[test]
fn stored_metadata_content_identity_and_logical_full_install() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("main.ts"), "export const value: number = 3;").unwrap();
    fs::create_dir(src.join("empty")).unwrap();
    std::os::unix::fs::symlink("main.ts", src.join("alias.ts")).unwrap();
    let a = t.path().join("a.dnp");
    let b = t.path().join("b.dnp");
    build(&src, &a);
    build(&src, &b);
    let package = Package::open(&a, 0).unwrap();
    assert_eq!(package.manifest.format_version, 3);
    assert_eq!(
        package.package_id(),
        Package::open(&b, 0).unwrap().package_id()
    );
    let mut file = fs::File::open(&a).unwrap();
    let offset = dnr_package::zip_offset(&mut file).unwrap();
    let mut zip = zip::ZipArchive::new(dnr_package::Region::new(file, offset).unwrap()).unwrap();
    assert_eq!(zip.by_index(0).unwrap().name(), ".dnr/meta.bin");
    assert_eq!(
        zip.by_index(0).unwrap().compression(),
        zip::CompressionMethod::Stored
    );
    assert!(zip.by_name(".dnr/manifest.json").is_err());
    assert_eq!(
        package.read("alias.ts").unwrap().unwrap().as_ref(),
        b"export const value: number = 3;"
    );
    let full = t.path().join("full");
    dnr_package::commands::install(
        &[
            a.display().to_string(),
            full.display().to_string(),
            "--mode".into(),
            "full".into(),
        ],
        true,
    )
    .unwrap();
    assert_eq!(
        fs::read(full.join("main.ts")).unwrap(),
        fs::read(src.join("main.ts")).unwrap()
    );
    assert!(full.join("empty").is_dir());
    assert!(full.join("alias.ts").is_symlink());
    let (manifest, content) = dnr_package::v3::installed_manifest(&full).unwrap();
    assert_eq!(manifest.app_id, "test.v3");
    assert_eq!(content.as_str(), package.package_id());
    assert!(!full.join("a.dnp").exists());
    fs::write(src.join("main.ts"), "export const value: number = 4;").unwrap();
    build(&src, &b);
    assert_ne!(
        package.package_id(),
        Package::open(&b, 0).unwrap().package_id()
    );
    let lease = dnr_package::v3::installed_lease(&full).unwrap();
    let update = vec![
        b.display().to_string(),
        full.display().to_string(),
        "--mode".into(),
        "full".into(),
        "--force".into(),
    ];
    assert!(dnr_package::commands::install(&update, true).is_err());
    assert!(
        String::from_utf8(fs::read(full.join("main.ts")).unwrap())
            .unwrap()
            .contains("= 3")
    );
    drop(lease);
    dnr_package::commands::install(&update, true).unwrap();
    assert!(
        String::from_utf8(fs::read(full.join("main.ts")).unwrap())
            .unwrap()
            .contains("= 4")
    );
}

#[test]
fn path_generations_keep_live_readers_and_reject_late_promotion() {
    let t = tempfile::tempdir().unwrap();
    let source = t.path().join("app");
    fs::write(&source, "old").unwrap();
    let cache = t.path().join("cache");
    let stamp = SourceStamp::read(&source).unwrap();
    let old = Generation::open(&cache, &source, &hash(b"old"), "app", &stamp).unwrap();
    let old_path = old.directory.clone();
    let replacement = t.path().join("replacement");
    fs::write(&replacement, "new").unwrap();
    fs::rename(replacement, &source).unwrap();
    let new = Generation::open(
        &cache,
        &source,
        &hash(b"new"),
        "app",
        &SourceStamp::read(&source).unwrap(),
    )
    .unwrap();
    assert!(old_path.exists());
    let late = Generation::open(&cache, &source, &hash(b"old"), "app", &stamp).unwrap();
    let current = new
        .directory
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("current");
    assert_eq!(fs::read_to_string(current).unwrap(), hash(b"new"));
    drop(old);
    assert!(old_path.exists());
    drop(late);
    assert!(!old_path.exists());
    assert!(new.directory.exists());
}

#[test]
fn compile_records_replace_source_and_recover_from_damage() {
    let t = tempfile::tempdir().unwrap();
    let source = t.path().join("app");
    fs::write(&source, "app").unwrap();
    let generation = Generation::open(
        &t.path().join("cache"),
        &source,
        &hash(b"app"),
        "app",
        &SourceStamp::read(&source).unwrap(),
    )
    .unwrap();
    let compile = generation.compile("runtime1").unwrap();
    compile
        .set("code", "file:///main.js", "old", b"bytecode1")
        .unwrap();
    assert_eq!(
        compile.get("code", "file:///main.js", "old").unwrap(),
        b"bytecode1"
    );
    assert!(compile.get("code", "file:///main.js", "new").is_none());
    compile
        .set("code", "file:///main.js", "new", b"bytecode2")
        .unwrap();
    assert!(compile.get("code", "file:///main.js", "old").is_none());
    let path = generation
        .directory
        .join("compile/generations")
        .join(hash(b"runtime1"))
        .join("code")
        .join(hash(b"file:///main.js"));
    fs::write(&path, b"damaged").unwrap();
    assert!(compile.get("code", "file:///main.js", "new").is_none());
    compile
        .set("code", "file:///main.js", "new", b"bytecode2")
        .unwrap();
    assert_eq!(
        compile.get("code", "file:///main.js", "new").unwrap(),
        b"bytecode2"
    );
    let new = generation.compile("runtime2").unwrap();
    assert!(path.exists());
    drop(compile);
    assert!(!path.exists());
    assert!(new.get("code", "file:///main.js", "new").is_none());
    let orphan = generation
        .directory
        .join("compile/generations")
        .join(hash(b"runtime2"))
        .join("code/.staging/orphan");
    fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    fs::write(&orphan, "interrupted write").unwrap();
    drop(new);
    drop(generation);
    let reopened = Generation::open(
        &t.path().join("cache"),
        &source,
        &hash(b"app"),
        "app",
        &SourceStamp::read(&source).unwrap(),
    )
    .unwrap();
    assert!(!orphan.exists());
    drop(reopened);
}

#[test]
fn corrupt_metadata_is_rejected_before_payload_loading() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("main.ts"), "console.log(1)").unwrap();
    let output = t.path().join("app");
    build(&src, &output);
    let mut bytes = fs::read(&output).unwrap();
    let magic = bytes.windows(8).position(|b| b == b"DNRMETA3").unwrap();
    bytes[magic + 20] ^= 1;
    fs::write(&output, bytes).unwrap();
    assert!(Package::open(&output, 0).is_err());
}

#[test]
fn short_variants_hardlink_reuse_and_sidecar_precedence() {
    use std::os::unix::fs::MetadataExt;
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("main.ts"), "export {};").unwrap();
    let tool = t.path().join("tool");
    fs::write(&tool, "#!/bin/sh\necho v3\n").unwrap();
    let host = dnr_package::config::Target::host();
    let config = t.path().join("config.json");
    fs::write(&config, serde_json::to_vec(&serde_json::json!({
        "schemaVersion": 1, "targets": {"host": host},
        "groups": [{"id": "tool", "files": [], "variants": {"host": [{"from": tool, "to": "deep/path/tool"}]}, "native": {"executables": ["deep/path/tool"]}}]
    })).unwrap()).unwrap();
    let a = t.path().join("a.dnp");
    pack_with_config(
        &PackOptions {
            directory: src,
            entry: "main.ts".into(),
            output: a.clone(),
            includes: vec![],
            excludes: vec![],
            app_id: Some("app".into()),
            force: false,
        },
        &PackageConfig::load(&config).unwrap(),
    )
    .unwrap();
    let cache = t.path().join("cache");
    let first = Package::open(&a, 0)
        .unwrap()
        .with_cache_directory(&cache)
        .unwrap();
    let inspection = first.inspect().unwrap();
    let source = inspection["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["native"] == "executable")
        .unwrap()["source"]
        .as_str()
        .unwrap();
    assert!(source.starts_with(".dnr/p/"));
    assert_eq!(source.matches('/').count(), 2);
    let native = first
        .native_path("deep/path/tool", dnr_package::NativeUse::Executable)
        .unwrap()
        .unwrap();
    let gen_a = first.cache_generation().unwrap();
    dnr_package::persistent::catalog::flush().unwrap();
    dnr_package::persistent::catalog::index(&cache, &gen_a.directory).unwrap();
    let b = t.path().join("b.dnp");
    fs::copy(&a, &b).unwrap();
    let second = Package::open(&b, 0)
        .unwrap()
        .with_cache_directory(&cache)
        .unwrap();
    let native_b = second
        .native_path("deep/path/tool", dnr_package::NativeUse::Executable)
        .unwrap()
        .unwrap();
    assert_ne!(native, native_b);
    assert_eq!(
        fs::metadata(&native).unwrap().ino(),
        fs::metadata(&native_b).unwrap().ino()
    );
    assert_eq!(fs::read(native_b).unwrap(), b"#!/bin/sh\necho v3\n");
    let dest = t.path().join("installed");
    dnr_package::commands::install(&[a.display().to_string(), dest.display().to_string()], true)
        .unwrap();
    let unused = t.path().join("unused");
    let installed = Package::open(&dest.join("a.dnp"), 0)
        .unwrap()
        .with_cache_directory(&unused)
        .unwrap();
    let real = installed
        .native_path("deep/path/tool", dnr_package::NativeUse::Executable)
        .unwrap()
        .unwrap();
    assert!(real.starts_with(dest.canonicalize().unwrap()));
    assert!(!unused.exists());
}

#[test]
fn linked_variants_preserve_safe_physical_and_logical_paths() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("main.ts"), "export {};").unwrap();
    let native = temp.path().join("native");
    fs::create_dir_all(native.join("dir/links")).unwrap();
    fs::write(native.join("tool"), "#!/bin/sh\necho linked\n").unwrap();
    std::os::unix::fs::symlink("../../tool", native.join("dir/links/link")).unwrap();
    let config_path = temp.path().join("config.json");
    fs::write(&config_path, serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"targets":{"host":dnr_package::config::Target::host()},"groups":[{"id":"native","files":[],"variants":{"host":[{"from":native,"to":"deep"}]},"native":{"executables":["deep/tool"]}}]})).unwrap()).unwrap();
    let output = temp.path().join("app");
    pack_with_config(
        &PackOptions {
            directory: src,
            output: output.clone(),
            entry: "main.ts".into(),
            includes: vec![],
            excludes: vec![],
            app_id: None,
            force: false,
        },
        &PackageConfig::load(&config_path).unwrap(),
    )
    .unwrap();
    let package = Package::open(&output, 0).unwrap();
    assert_eq!(
        package
            .read("deep/dir/links/link")
            .unwrap()
            .unwrap()
            .as_ref(),
        b"#!/bin/sh\necho linked\n"
    );
    let info = package.inspect().unwrap();
    let link = info["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "symlink")
        .unwrap()["source"]
        .as_str()
        .unwrap();
    let extracted = temp.path().join("raw");
    package.extract_to(&extracted).unwrap();
    assert_eq!(
        fs::read(extracted.join(link)).unwrap(),
        b"#!/bin/sh\necho linked\n"
    );
    assert!(
        extracted
            .join(link)
            .canonicalize()
            .unwrap()
            .starts_with(extracted.canonicalize().unwrap())
    );
}

#[test]
fn cache_management_rejects_ambiguous_options_and_dry_rebuild_is_readonly() {
    for args in [
        vec!["cache", "clean", "--directory", ".", "--stale"],
        vec!["cache", "rebuild", "--directory", "."],
        vec!["cache", "clean", "--all", "--path", "app"],
    ] {
        assert!(
            dnr_package::commands::run(&args.into_iter().map(str::to_owned).collect::<Vec<_>>())
                .is_err()
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("absent-cache");
    dnr_package::persistent::catalog::command(&cache, "rebuild", None, None, false, true).unwrap();
    assert!(!cache.exists());
}
