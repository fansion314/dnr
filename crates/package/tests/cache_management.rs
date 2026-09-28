use dnr_package::persistent::{
    Generation, SourceStamp, catalog, hash,
    management::{self, Options},
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
