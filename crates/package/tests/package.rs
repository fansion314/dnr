use dnr_package::*;
use std::{fs, sync::Arc};

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
fn lazy_reads_and_lru() {
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
fn lru_matches_reference_across_hits_evictions_and_oversized_files() {
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
        let mut lru = std::collections::VecDeque::new();
        let mut resident = 0;
        let (mut hits, mut misses) = (0, 0);
        let mut seed = 42u64;
        let mut retained = Vec::new();
        for step in 0..2000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let key = ((seed >> 32) % 16) as usize;
            let size = key * 7;
            if let Some(at) = lru.iter().position(|&n| n == key) {
                lru.remove(at);
                lru.push_back(key);
                hits += 1;
            } else {
                misses += 1;
                if size <= budget {
                    while resident + size > budget {
                        resident -= lru.pop_front().unwrap() * 7;
                    }
                    resident += size;
                    lru.push_back(key);
                }
            }
            let bytes = p.read(&format!("cache{key}")).unwrap().unwrap();
            assert_eq!(&*bytes, vec![key as u8; size]);
            if step % 101 == 0 {
                retained.push((key, bytes));
            }
            let stats = p.stats();
            assert_eq!(
                (stats.hits, stats.decompressions, stats.resident_bytes),
                (hits, misses, resident)
            );
        }
        // Eviction removes only the cache's reference, never a reader's data.
        for (key, bytes) in retained {
            assert_eq!(&*bytes, vec![key as u8; key * 7]);
        }
    }
}

#[test]
fn corrupted_payload_is_lazy_and_never_cached() {
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("corrupt.dnp");
    let header = format!("#!/bin/sh\nexit 127\n{MARKER}");
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(header.as_bytes()).unwrap();
    let mut zip = ZipWriter::new(Region::new(file, header.len() as u64).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(MANIFEST, options).unwrap();
    zip.write_all(br#"{"formatVersion":1,"entry":"main.js","appId":"crc-test"}"#)
        .unwrap();
    zip.start_file("main.js", options).unwrap();
    zip.write_all(b"unique payload for CRC testing").unwrap();
    zip.finish().unwrap();
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
    assert_eq!(p.stats().resident_bytes, 0);
    assert_eq!(p.stats().hits, 0);
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
#[cfg(unix)]
fn shell_header_preserves_arguments_and_entry_quotes() {
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
