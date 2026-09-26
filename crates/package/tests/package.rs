use dnr_package::*;
mod support;
use std::{fs, sync::Arc};

// Concurrent fork/exec can inherit another test's writable script descriptor
// until exec closes it, causing ETXTBSY even after the writer was dropped.
// Serialize creation and execution of scripts, preserving direct-exec coverage.
// https://github.com/rust-lang/rust/issues/114554
#[cfg(unix)]
static EXECUTABLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fixture() -> (tempfile::TempDir, PackOptions) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("main.ts"), "console.log('hello');\n").unwrap();
    fs::write(root.join("assets/a.txt"), "a".repeat(8192)).unwrap();
    fs::write(root.join("assets/b.txt"), "b".repeat(8192)).unwrap();
    let opts = PackOptions {
        directory: root,
        entry: "main.ts".into(),
        output: temp.path().join("demo.dnp"),
        includes: vec![],
        excludes: vec![],
        app_id: Some("test.demo".into()),
        force: false,
    };
    (temp, opts)
}

#[test]
fn lazy_reads_and_clock() {
    let (_temp, opts) = fixture();
    pack(&opts).unwrap();
    let package = Package::open(&opts.output, 8192).unwrap();
    assert_eq!(package.stats().decompressions, 0);
    assert!(package.entries.contains_key("assets/a.txt"));
    assert_eq!(package.read("assets/a.txt").unwrap().unwrap().len(), 8192);
    package.read("assets/a.txt").unwrap();
    assert_eq!(package.stats().hits, 1);
    package.read("assets/b.txt").unwrap();
    assert_eq!(package.stats().resident_bytes, 8192);
    package.read("assets/a.txt").unwrap();
    assert_eq!(package.stats().decompressions, 3);
    assert!(package.read("missing.txt").unwrap().is_none());
}

#[test]
fn concurrent_reads_share_decompression() {
    let (_temp, opts) = fixture();
    pack(&opts).unwrap();
    let p = Arc::new(Package::open(&opts.output, DEFAULT_CACHE_BYTES).unwrap());
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let p = p.clone();
            std::thread::spawn(move || p.read("assets/a.txt").unwrap())
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(p.stats().decompressions, 1);
}

#[test]
fn parallel_zip_reads_survive_eviction_and_retained_handles() {
    let (_temp, opts) = fixture();
    for n in 0..16 {
        let bytes: Vec<_> = (0..65537).map(|i| ((i * 37 + n) % 251) as u8).collect();
        fs::write(opts.directory.join(format!("file{n}")), bytes).unwrap();
    }
    pack(&opts).unwrap();
    let package = Package::open(&opts.output, 3 * 65537).unwrap();
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let package = &package;
            scope.spawn(move || {
                let retained = package.read("file0").unwrap().unwrap();
                for iteration in 0..100 {
                    let n = (iteration * 7 + worker) % 16;
                    let bytes = package.read(&format!("file{n}")).unwrap().unwrap();
                    assert_eq!(bytes.len(), 65537);
                    for (i, &byte) in bytes.iter().enumerate() {
                        assert_eq!(byte, ((i * 37 + n) % 251) as u8);
                    }
                    assert!(package.stats().resident_bytes <= 3 * 65537);
                }
                for (i, &byte) in retained.iter().enumerate() {
                    assert_eq!(byte, ((i * 37) % 251) as u8);
                }
            });
        }
    });
}

#[test]
fn replacing_package_path_does_not_replace_open_reader() {
    let (_temp, mut opts) = fixture();
    pack(&opts).unwrap();
    let original = Package::open(&opts.output, 0).unwrap();
    fs::rename(&opts.output, opts.output.with_extension("old")).unwrap();
    fs::write(opts.directory.join("assets/a.txt"), "new").unwrap();
    opts.force = true;
    pack(&opts).unwrap();
    assert_eq!(original.read("assets/a.txt").unwrap().unwrap().len(), 8192);
    assert_eq!(
        &*Package::open(&opts.output, 0)
            .unwrap()
            .read("assets/a.txt")
            .unwrap()
            .unwrap(),
        b"new"
    );
    assert!(original.read_index(usize::MAX).is_err());
}

#[test]
fn cache_budget_and_data_survive_hits_evictions_and_oversized_files() {
    let (_temp, opts) = fixture();
    for n in 0..16 {
        fs::write(
            opts.directory.join(format!("cache{n}")),
            vec![n as u8; n * 7],
        )
        .unwrap();
    }
    pack(&opts).unwrap();
    for budget in [0, 7, 49, 200, 1000] {
        let p = Package::open(&opts.output, budget).unwrap();
        let mut seed = 42u64;
        let mut retained = Vec::new();
        for step in 0..2000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let key = ((seed >> 32) % 16) as usize;
            let size = key * 7;
            let bytes = p.read(&format!("cache{key}")).unwrap().unwrap();
            assert_eq!(&*bytes, vec![key as u8; size]);
            if step % 101 == 0 {
                retained.push((key, bytes));
            }
            let stats = p.stats();
            assert!(stats.resident_bytes <= budget);
            assert_eq!(stats.hits + stats.decompressions, step + 1);
            if budget >= 840 {
                assert!(stats.decompressions <= 16);
            }
        }
        // Eviction removes only the cache's reference, never a reader's data.
        for (key, bytes) in retained {
            assert_eq!(&*bytes, vec![key as u8; key * 7]);
        }
    }
}

#[test]
fn corrupted_payload_is_lazy_and_never_cached() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("corrupt.dnp");
    support::archive(
        &path,
        &[("main.js", "unique payload for CRC testing", false)],
    );
    let mut bytes = fs::read(&path).unwrap();
    let start = bytes
        .windows(14)
        .position(|v| v == b"unique payload")
        .unwrap();
    bytes[start] ^= 1;
    fs::write(&path, bytes).unwrap();
    let p = Package::open(&path, DEFAULT_CACHE_BYTES).unwrap();
    assert_eq!(p.stats().decompressions, 0);
    for _ in 0..2 {
        assert!(p.read("main.js").is_err());
    }
    fs::write(temp.path().join("main.js"), b"disk override").unwrap();
    assert!(p.native_library_path("main.js").is_err());
    assert_eq!(
        fs::read(temp.path().join("main.js")).unwrap(),
        b"disk override"
    );
    assert_eq!(p.stats().resident_bytes, 0);
    assert_eq!(p.stats().hits, 0);
    // A failed flight must allow a later retry; repair the same open inode.
    use std::os::unix::fs::FileExt;
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all_at(b"u", start as u64)
        .unwrap();
    assert_eq!(
        &*p.read("main.js").unwrap().unwrap(),
        b"unique payload for CRC testing"
    );
}

#[test]
fn reproducible_and_compressed() {
    let (_temp, mut opts) = fixture();
    let report = pack(&opts).unwrap();
    let first = fs::read(&opts.output).unwrap();
    assert!(report.package_bytes < report.source_bytes / 2);
    assert!(String::from_utf8_lossy(&first[..120.min(first.len())]).contains("\"$@\""));
    opts.force = true;
    pack(&opts).unwrap();
    assert_eq!(first, fs::read(&opts.output).unwrap());
}

#[test]
fn excludes_and_missing_entry() {
    let (_temp, mut opts) = fixture();
    opts.excludes.push("assets".into());
    pack(&opts).unwrap();
    assert!(
        !Package::open(&opts.output, 0)
            .unwrap()
            .entries
            .contains_key("assets/a.txt")
    );
    opts.force = true;
    opts.excludes.push("main.ts".into());
    assert!(pack(&opts).is_err());
}

#[test]
fn output_inside_source_does_not_include_itself() {
    let (_temp, mut opts) = fixture();
    opts.output = opts.directory.join("demo.dnp");
    pack(&opts).unwrap();
    opts.force = true;
    pack(&opts).unwrap();
    assert!(
        !Package::open(&opts.output, 0)
            .unwrap()
            .entries
            .contains_key("demo.dnp")
    );
}

#[test]
fn refuse_overwrite_and_conflicts() {
    let (_temp, mut opts) = fixture();
    pack(&opts).unwrap();
    assert!(pack(&opts).is_err());
    opts.force = true;
    opts.includes.push(Include {
        source: opts.directory.join("main.ts"),
        destination: "main.ts".into(),
    });
    assert!(pack(&opts).is_err());
}

#[test]
fn invalid_paths_are_rejected() {
    for name in ["../a", "/a", "a/../../b", "a\0b", "a\nb", "a\\b", ""] {
        assert!(normalized_name(name).is_err(), "{name:?}");
    }
    assert!(resolve_link("a", "../../b").is_err());
}

#[test]
#[cfg(unix)]
fn symlinks_resolve_without_uncompressing_files() {
    let (_temp, opts) = fixture();
    std::os::unix::fs::symlink("assets", opts.directory.join("linked")).unwrap();
    pack(&opts).unwrap();
    let p = Package::open(&opts.output, 8192).unwrap();
    assert_eq!(p.resolve("linked/a.txt").unwrap(), "assets/a.txt");
    assert_eq!(p.stats().decompressions, 0);
    assert_eq!(p.read("linked/a.txt").unwrap().unwrap().len(), 8192);
}

#[test]
#[cfg(unix)]
fn symlink_cycles_are_rejected_before_publication() {
    let (_temp, opts) = fixture();
    std::os::unix::fs::symlink("b", opts.directory.join("a")).unwrap();
    std::os::unix::fs::symlink("a", opts.directory.join("b")).unwrap();
    assert!(pack(&opts).is_err());
    assert!(!opts.output.exists());
}

#[test]
fn malformed_package_fails() {
    let (_temp, opts) = fixture();
    pack(&opts).unwrap();
    let mut bytes = fs::read(&opts.output).unwrap();
    bytes.truncate(bytes.len() - 30);
    fs::write(&opts.output, bytes).unwrap();
    assert!(Package::open(&opts.output, 0).is_err());
}

#[test]
fn legacy_packages_are_rejected_with_repack_guidance() {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};
    let temp = tempfile::tempdir().unwrap();
    for version in [1, 2] {
        let path = temp.path().join(format!("v{version}.dnp"));
        let mut file = fs::File::create(&path).unwrap();
        let header = format!("#!/bin/sh\nexit 127\n{MARKER}");
        file.write_all(header.as_bytes()).unwrap();
        let mut zip = ZipWriter::new(Region::new(file, header.len() as u64).unwrap());
        let opts = SimpleFileOptions::default().unix_permissions(0o644);
        let records = serde_json::to_vec(&serde_json::json!([{
            "path":"main.js", "source":"main.js", "kind":"file", "size":0,
            "mode":420, "sha256": format!("{:x}", Sha256::digest([]))
        }]))
        .unwrap();
        let mut manifest =
            serde_json::json!({"formatVersion":version,"entry":"main.js","appId":"legacy"});
        if version == 2 {
            manifest["integrity"] = serde_json::json!({"path":".dnr/index.json","sha256":format!("{:x}", Sha256::digest(&records))});
        }
        zip.start_file(".dnr/manifest.json", opts).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        if version == 2 {
            zip.start_file(".dnr/index.json", opts).unwrap();
            zip.write_all(&records).unwrap();
        }
        zip.start_file("main.js", opts).unwrap();
        zip.finish().unwrap();
        let error = Package::open(&path, 0).unwrap_err().to_string();
        assert!(
            error.contains("only v4") && error.contains("repack"),
            "{error}"
        );
    }
}

#[test]
#[cfg(unix)]
fn shell_header_preserves_arguments_and_entry_quotes() {
    let _guard = EXECUTABLE_TEST_LOCK.lock().unwrap();
    use std::os::unix::fs::PermissionsExt;
    let (temp, mut opts) = fixture();
    fs::rename(
        opts.directory.join("main.ts"),
        opts.directory.join("a'b main.ts"),
    )
    .unwrap();
    opts.entry = "a'b main.ts".into();
    opts.output = temp.path().join("a package.dnp");
    pack(&opts).unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let stub = bin.join("dnr");
    fs::write(&stub, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    let output = std::process::Command::new(&opts.output)
        .env("PATH", &bin)
        .args(["hello world", "", "--", "a'b"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let expected = format!(
        "--package\n{}\n--entry\na'b main.ts\n--\nhello world\n\n--\na'b\n",
        opts.output.display()
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
#[cfg(unix)]
fn missing_runtime_exits_before_zip_bytes() {
    let _guard = EXECUTABLE_TEST_LOCK.lock().unwrap();
    let (_temp, opts) = fixture();
    pack(&opts).unwrap();
    let output = std::process::Command::new(&opts.output)
        .env("PATH", "/nonexistent-dnr-bin")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(127));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("dnr"));
    assert!(!error.contains("syntax error"));
}
