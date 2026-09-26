mod support;
use dnr_package::{PackOptions, Package, metadata::META, pack};
use std::{fs, path::Path};

fn archive(path: &Path, entries: &[(&str, &str, bool)]) {
    let mut all = vec![("main.js", "throw Error('must not execute');", false)];
    all.extend_from_slice(entries);
    support::archive(path, &all);
}

#[test]
fn tree_includes_metadata_implicit_directories_and_links_without_disk_overlay() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("app.dnp");
    archive(
        &path,
        &[
            ("a/z", "zip", false),
            ("a-/中文", "", false),
            ("link", "a", true),
            ("tab\tname", "", false),
        ],
    );
    fs::write(temp.path().join("disk-only"), "hidden").unwrap();
    let package = Package::open(&path, 0).unwrap();
    let mut tree = Vec::new();
    package.write_tree(&mut tree).unwrap();
    assert_eq!(
        String::from_utf8(tree).unwrap(),
        concat!(
            ".\n",
            "├── .dnr/\n",
            "│   └── meta.bin\n",
            "├── a/\n",
            "│   └── z\n",
            "├── a-/\n",
            "│   └── 中文\n",
            "├── link -> a\n",
            "├── main.js\n",
            "└── tab\\tname\n",
        )
    );
    assert_eq!(package.stats().decompressions, 0);
    let output = temp.path().join("output");
    package.extract_to(&output).unwrap();
    assert_eq!(
        fs::read(output.join(META)).unwrap(),
        &*package.read_archive_file(META).unwrap()
    );
    assert_eq!(fs::read(output.join("a/z")).unwrap(), b"zip");
    assert!(!output.join("disk-only").exists());
}

#[test]
#[cfg(unix)]
fn extract_round_trips_bytes_empty_directories_links_and_executable_modes() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("empty")).unwrap();
    let bytes: Vec<u8> = (0..131073).map(|i| (i % 251) as u8).collect();
    fs::write(source.join("main.js"), &bytes).unwrap();
    fs::set_permissions(source.join("main.js"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(source.join("zero"), []).unwrap();
    symlink("main.js", source.join("link")).unwrap();
    symlink("empty", source.join("dir-link")).unwrap();
    let path = temp.path().join("app.dnp");
    pack(&PackOptions {
        directory: source,
        entry: "main.js".into(),
        output: path.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: None,
        force: false,
    })
    .unwrap();
    let package = Package::open(&path, 0).unwrap();
    for existing in [false, true] {
        let output = temp.path().join(format!("nested/{existing}/output"));
        if existing {
            fs::create_dir_all(&output).unwrap();
        }
        package.extract_to(&output).unwrap();
        assert_eq!(fs::read(output.join("main.js")).unwrap(), bytes);
        assert_eq!(fs::read(output.join("link")).unwrap(), bytes);
        assert_eq!(
            fs::read_link(output.join("dir-link")).unwrap(),
            Path::new("empty")
        );
        assert_eq!(
            fs::metadata(output.join("main.js"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(fs::metadata(output.join("zero")).unwrap().len(), 0);
        assert_eq!(fs::read_dir(output.join("empty")).unwrap().count(), 0);
        assert!(output.join(META).is_file());
    }
    assert_eq!(package.stats().resident_bytes, 0);
}

#[test]
fn extraction_refuses_existing_content_and_symlink_destinations() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("app.dnp");
    archive(&path, &[]);
    let package = Package::open(&path, 0).unwrap();
    let output = temp.path().join("output");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("keep"), "keep").unwrap();
    assert!(package.extract_to(&output).is_err());
    assert!(package.extract_to(&output.join("keep")).is_err());
    assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
    assert_eq!(fs::read(output.join("keep")).unwrap(), b"keep");
    #[cfg(unix)]
    {
        let empty = temp.path().join("empty");
        fs::create_dir(&empty).unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&empty, &link).unwrap();
        assert!(package.extract_to(&link).is_err());
        assert_eq!(fs::read_dir(empty).unwrap().count(), 0);
    }
}

#[test]
fn corrupt_file_can_be_listed_but_extraction_is_not_published() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("app.dnp");
    archive(&path, &[("z-last", "unique payload", false)]);
    let mut bytes = fs::read(&path).unwrap();
    let offset = bytes
        .windows(14)
        .position(|w| w == b"unique payload")
        .unwrap();
    bytes[offset] ^= 1;
    fs::write(&path, bytes).unwrap();
    let package = Package::open(&path, 0).unwrap();
    package.write_tree(&mut Vec::new()).unwrap();
    fs::write(temp.path().join("z-last"), "disk fallback must not be used").unwrap();
    let output = temp.path().join("output");
    assert!(package.extract_to(&output).is_err());
    assert!(!output.exists());
    fs::create_dir(&output).unwrap();
    assert!(package.extract_to(&output).is_err());
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
    assert!(!fs::read_dir(temp.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".dnr-extract-")
    }));
}

#[test]
fn extraction_rejects_traversal_link_cycles_and_file_directory_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("app.dnp");
    let cases = [
        vec![("../escape", "bad", false)],
        vec![("/absolute", "bad", false)],
        vec![("link", "../escape", true)],
        vec![("a", "b", true), ("b", "a", true)],
        vec![("a", "target", true), ("a/file", "bad", false)],
        vec![(".dnr", "main.js", true)],
        vec![("a", "file", false), ("a/b", "bad", false)],
        vec![("a/./b", "one", false), ("a/b", "two", false)],
    ];
    for entries in cases {
        archive(&path, &entries);
        let output = temp.path().join("output");
        let result = Package::open(&path, 0).and_then(|p| p.extract_to(&output));
        assert!(result.is_err(), "accepted {entries:?}");
        assert!(!output.exists());
    }
}
