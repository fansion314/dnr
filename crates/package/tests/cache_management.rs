use dnr_package::{
    PackOptions, Package, pack,
    persistent::{
        Generation, SourceStamp, catalog, hash,
        management::{self, Options},
    },
};
use std::{fs, path::Path, sync::Arc};

fn generation(base: &Path, source: &Path, payload: &[u8]) -> Arc<Generation> {
    fs::write(source, payload).unwrap();
    let generation = Generation::open(
        base,
        source,
        &hash(payload),
        "cache.test",
        &SourceStamp::read(source).unwrap(),
    )
    .unwrap();
    fs::write(generation.directory.join("payload"), payload).unwrap();
    catalog::index(base, &generation.directory).unwrap();
    generation
}
fn used(base: &Path, path: &Path, time: i64) {
    let conn = rusqlite::Connection::open(base.join("index.sqlite3")).unwrap();
    conn.execute("INSERT INTO accesses(path,last_used) VALUES (?1,?2) ON CONFLICT(path) DO UPDATE SET last_used=excluded.last_used", rusqlite::params![path.canonicalize().unwrap().to_string_lossy(), time]).unwrap();
}

#[test]
fn units_dates_and_regex_options_are_strict() {
    assert_eq!(management::human_size(999), "999 B");
    assert_eq!(management::human_size(187896), "187.90 KB");
    assert_eq!(management::human_size(1_000_000), "1.00 MB");
    assert_eq!(management::parse_size("1.5GB").unwrap(), 1_500_000_000);
    for bad in [
        "0",
        "-1MB",
        "NaN",
        "1GiB",
        "1.2.3KB",
        "999999999999999999999999GB",
    ] {
        assert!(management::parse_size(bad).is_err(), "{bad}");
    }
    assert_eq!(
        management::parse_before("2026-09-01T08:00:00+08:00").unwrap(),
        management::parse_before("2026-09-01T00:00:00Z").unwrap()
    );
    assert!(management::parse_before("2026-02-30").is_err());
    assert!(management::parse_before("2026-09-01T12:00:00").is_err());
    for args in [
        vec!["clean", "--path-regex", "["],
        vec!["cache", "info", "--before", "2026-09-01"],
        vec!["clean", "--directory", "/tmp", "--max-size", "1GB"],
        vec!["cache", "info", "--trace"],
        vec!["clean", "--directory", "/tmp", "--trace"],
    ] {
        assert!(
            dnr_package::commands::run(&args.into_iter().map(str::to_owned).collect::<Vec<_>>())
                .is_err()
        );
    }
}

#[test]
fn lru_cleanup_skips_live_paths_and_updates_summary() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().join("cache");
    let old = generation(&base, &t.path().join("old"), b"old payload");
    let active = generation(&base, &t.path().join("active"), b"active payload");
    let recent = generation(&base, &t.path().join("recent"), b"recent payload");
    let old_dir = old.directory.clone();
    let recent_dir = recent.directory.clone();
    used(&base, &old.receipt.path, 10);
    used(&base, &active.receipt.path, 20);
    used(&base, &recent.receipt.path, 30);
    drop(old);
    drop(recent);
    let summary = management::inspect(&base, "info", &Options::default()).unwrap();
    let dry = management::inspect(
        &base,
        "clean",
        &Options {
            max_size: Some(1),
            dry: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        dry.entries
            .iter()
            .map(|e| e.path.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>(),
        ["old", "active", "recent"]
    );
    assert_eq!(dry.entries[1].state, "in-use");
    assert_eq!(dry.target_reached, Some(false));
    assert!(old_dir.exists() && recent_dir.exists());
    let real = management::inspect(
        &base,
        "clean",
        &Options {
            max_size: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(real.logical_bytes_removed, dry.logical_bytes_removed);
    assert!(!old_dir.exists() && !recent_dir.exists());
    assert!(active.directory.exists());
    let after = management::inspect(&base, "info", &Options::default()).unwrap();
    assert_eq!(after.generation_count, 1);
    assert_eq!(after.bytes, summary.bytes - real.logical_bytes_removed);
}

#[test]
fn unknown_time_date_intersection_and_rebuild_preserve_history() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().join("cache");
    let known = generation(&base, &t.path().join("matching-old"), b"old");
    let unknown = generation(&base, &t.path().join("matching-unknown"), b"unknown");
    used(&base, &known.receipt.path, 100);
    let known_dir = known.directory.clone();
    let unknown_dir = unknown.directory.clone();
    drop(known);
    drop(unknown);
    catalog::rebuild(&base).unwrap();
    let report = management::inspect(
        &base,
        "clean",
        &Options {
            before: Some(101),
            path_regex: Some(regex::Regex::new("matching-").unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.entries[0].last_used, Some(100));
    assert!(!known_dir.exists());
    assert!(unknown_dir.exists());
}

#[test]
fn runtime_access_is_distinct_from_index_updates_and_management() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().join("cache");
    let generation = generation(&base, &t.path().join("app"), b"app");
    let before = management::inspect(&base, "info", &Options::default()).unwrap();
    assert_eq!(before.entries[0].last_used, None);
    generation.mark_used();
    catalog::flush().unwrap();
    let after = management::inspect(&base, "info", &Options::default()).unwrap();
    assert!(after.entries[0].last_used.is_some());
    catalog::index(&base, &generation.directory).unwrap();
    catalog::rebuild(&base).unwrap();
    let rebuilt = management::inspect(&base, "info", &Options::default()).unwrap();
    assert_eq!(after.entries[0].last_used, rebuilt.entries[0].last_used);
    let json = serde_json::to_value(rebuilt).unwrap();
    assert!(json["bytes"].is_u64());
    assert!(json["cachePath"].is_string());
}

#[test]
fn path_globs_match_source_paths_with_shell_separators() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().join("cache");
    let top = generation(&base, &t.path().join("app-one.dnp"), b"one");
    let nested_dir = t.path().join("nested");
    fs::create_dir(&nested_dir).unwrap();
    let nested_source = nested_dir.join("app-two.dnp");
    let nested = generation(&base, &nested_source, b"two");
    let top_dir = top.directory.clone();
    let nested_dir = nested.directory.clone();
    drop(top);
    drop(nested);
    let report = management::inspect(
        &base,
        "clean",
        &Options {
            path: Some(t.path().join("app-*.dnp")),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(report.entries.len(), 1);
    assert!(!top_dir.exists());
    assert!(nested_dir.exists());
    let report = management::inspect(
        &base,
        "clean",
        &Options {
            path: Some(t.path().join("**/app-*.dnp")),
            dry: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(report.entries.len(), 1);
    assert_eq!(
        report.entries[0].path,
        nested_source.canonicalize().unwrap()
    );
    let from_root = management::inspect(
        &base,
        "clean",
        &Options {
            path: Some("/**/app-two.dnp".into()),
            dry: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(from_root.entries.len(), 1);
    let gone_source = t.path().join("gone.dnp");
    let gone = generation(&base, &gone_source, b"gone");
    let gone_dir = gone.directory.clone();
    drop(gone);
    fs::remove_file(&gone_source).unwrap();
    let exact = management::inspect(
        &base,
        "clean",
        &Options {
            path: Some(gone_source),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(exact.entries.len(), 1);
    assert!(!gone_dir.exists());
}

#[test]
fn trace_compares_dnp_version_and_prunes_missing_sources() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().join("cache");
    let source = t.path().join("source");
    fs::create_dir(&source).unwrap();
    let dnp = t.path().join("app.dnp");
    let build = |text: &str| {
        fs::write(source.join("main.ts"), text).unwrap();
        pack(&PackOptions {
            directory: source.clone(),
            entry: "main.ts".into(),
            output: dnp.clone(),
            includes: vec![],
            excludes: vec![],
            app_id: Some("test.trace".into()),
            force: true,
        })
        .unwrap();
        Package::open(&dnp, 0).unwrap().package_id().to_owned()
    };
    let first = build("export const value = 1;");
    let old = Generation::open(
        &base,
        &dnp,
        &first,
        "test.trace",
        &SourceStamp::read(&dnp).unwrap(),
    )
    .unwrap();
    let old_dir = old.directory.clone();
    catalog::index(&base, &old_dir).unwrap();
    drop(old);
    assert!(
        management::inspect(
            &base,
            "clean",
            &Options {
                trace: true,
                dry: true,
                ..Default::default()
            }
        )
        .unwrap()
        .entries
        .is_empty()
    );

    let second = build("export const value = 2;");
    assert_ne!(first, second);
    let dry = management::inspect(
        &base,
        "clean",
        &Options {
            trace: true,
            dry: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(dry.entries[0].action.as_deref(), Some("would-remove"));
    assert!(old_dir.exists());
    management::inspect(
        &base,
        "clean",
        &Options {
            trace: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!old_dir.exists());

    let current = Generation::open(
        &base,
        &dnp,
        &second,
        "test.trace",
        &SourceStamp::read(&dnp).unwrap(),
    )
    .unwrap();
    let current_dir = current.directory.clone();
    drop(current);
    assert!(
        management::inspect(
            &base,
            "clean",
            &Options {
                trace: true,
                ..Default::default()
            }
        )
        .unwrap()
        .entries
        .is_empty()
    );
    assert!(current_dir.exists());
    fs::write(&dnp, b"temporarily invalid source").unwrap();
    let invalid = management::inspect(
        &base,
        "clean",
        &Options {
            trace: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        invalid.entries[0].action.as_deref(),
        Some("skipped-unreadable")
    );
    assert!(current_dir.exists());
    fs::remove_file(&dnp).unwrap();
    let removed = management::inspect(
        &base,
        "clean",
        &Options {
            trace: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(removed.entries.len(), 1);
    assert!(!current_dir.exists());
}

#[test]
fn trace_checks_full_install_descriptor() {
    let t = tempfile::tempdir().unwrap();
    let source = t.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("main.ts"), "export const ok = true;").unwrap();
    let dnp = t.path().join("app.dnp");
    pack(&PackOptions {
        directory: source,
        entry: "main.ts".into(),
        output: dnp.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("test.full.trace".into()),
        force: false,
    })
    .unwrap();
    let full = t.path().join("full");
    dnr_package::commands::install(
        &[
            dnp.display().to_string(),
            full.display().to_string(),
            "--mode".into(),
            "full".into(),
        ],
        true,
    )
    .unwrap();
    let descriptor = full.join(dnr_package::metadata::INSTALL);
    let content = Package::open(&dnp, 0).unwrap().package_id().to_owned();
    let base = t.path().join("cache");
    let generation = Generation::open(
        &base,
        &full,
        &content,
        "test.full.trace",
        &SourceStamp::read(&descriptor).unwrap(),
    )
    .unwrap();
    let directory = generation.directory.clone();
    drop(generation);
    assert!(
        management::inspect(
            &base,
            "clean",
            &Options {
                trace: true,
                ..Default::default()
            }
        )
        .unwrap()
        .entries
        .is_empty()
    );
    assert!(directory.exists());
    fs::remove_file(descriptor).unwrap();
    let report = management::inspect(
        &base,
        "clean",
        &Options {
            trace: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(report.entries.len(), 1);
    assert!(!directory.exists());
}
