//! Human-readable management. SQLite is an index; deletion always checks disk and leases.
use super::*;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use regex::Regex;
use rusqlite::params;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Options {
    pub path: Option<PathBuf>,
    pub prefix: Option<String>,
    pub stale: bool,
    pub dry: bool,
    pub json: bool,
    pub before: Option<i64>,
    pub max_size: Option<u64>,
    pub path_regex: Option<Regex>,
}

pub fn human_size(bytes: u64) -> String {
    let (divisor, unit) = if bytes >= 1_000_000_000 {
        (1_000_000_000.0, "GB")
    } else if bytes >= 1_000_000 {
        (1_000_000.0, "MB")
    } else if bytes >= 1_000 {
        (1_000.0, "KB")
    } else {
        return format!("{bytes} B");
    };
    format!("{:.2} {unit}", bytes as f64 / divisor)
}

pub fn parse_size(input: &str) -> Result<u64> {
    let value = input.trim().to_ascii_uppercase();
    let end = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let factor = match value[end..].trim() {
        "" | "B" => 1u64,
        "KB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        _ => anyhow::bail!("size must use B, KB, MB or GB (decimal)"),
    };
    let number = value[..end].parse::<f64>().context("invalid cache size")?;
    let bytes = number * factor as f64;
    ensure!(
        bytes.is_finite() && bytes >= 1.0 && bytes < u64::MAX as f64,
        "size must be positive and fit in u64"
    );
    Ok(bytes.floor() as u64)
}

pub fn parse_before(input: &str) -> Result<i64> {
    if let Ok(date) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        return Ok(Local
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .context(
                "date midnight is ambiguous or absent in local time; use RFC 3339 with an offset",
            )?
            .timestamp());
    }
    Ok(DateTime::parse_from_rfc3339(input)
        .context("expected YYYY-MM-DD or RFC 3339 with a timezone")?
        .timestamp())
}

fn timestamp(value: Option<i64>) -> String {
    value
        .and_then(|v| Local.timestamp_opt(v, 0).single())
        .map(|v| v.to_rfc3339())
        .unwrap_or_else(|| "unknown".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: PathBuf,
    pub directory: PathBuf,
    pub content_hash: String,
    pub app_id: String,
    pub bytes: u64,
    pub last_used: Option<i64>,
    pub indexed_at: Option<i64>,
    pub state: String,
    pub action: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub cache_path: PathBuf,
    pub bytes: u64,
    pub path_count: usize,
    pub generation_count: usize,
    pub indexed_at: Option<i64>,
    pub indexed: bool,
    pub logical_bytes_removed: u64,
    pub remaining_bytes: u64,
    pub target_reached: Option<bool>,
    pub dry_run: bool,
    pub entries: Vec<Entry>,
}

fn normalized(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        crate::materialize::lexical(&if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        })
    })
}

fn disk_entries(base: &Path) -> Result<Vec<Entry>> {
    let accesses = catalog::read_accesses(base).unwrap_or_default();
    let mut entries = Vec::new();
    for directory in catalog::directories(base)? {
        // Never follow a redirected generation, path root, or receipt.
        if directory.canonicalize().ok().as_ref() != Some(&directory) {
            continue;
        }
        let receipt_file = directory.join("receipt.json");
        if !fs::symlink_metadata(&receipt_file)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink() && m.len() <= 65536)
        {
            continue;
        }
        let Ok(receipt) = serde_json::from_slice::<Receipt>(&fs::read(receipt_file)?) else {
            continue;
        };
        let root = directory.parent().unwrap().parent().unwrap();
        if receipt.path_hash != root.file_name().unwrap().to_string_lossy()
            || receipt.content_hash != directory.file_name().unwrap().to_string_lossy()
            || !receipt.path.is_absolute()
            || path_hash(&receipt.path) != receipt.path_hash
        {
            continue;
        }
        entries.push(Entry {
            last_used: accesses
                .get(receipt.path.to_string_lossy().as_ref())
                .copied(),
            path: receipt.path,
            directory: directory.clone(),
            content_hash: receipt.content_hash,
            app_id: receipt.app_id,
            bytes: catalog::size(&directory)?,
            indexed_at: None,
            state: "ready".into(),
            action: None,
        });
    }
    Ok(entries)
}

fn indexed_entries(base: &Path) -> Result<Vec<Entry>> {
    ensure!(
        base.join("index.sqlite3").is_file(),
        "cache index is missing; scan receipts"
    );
    let conn = catalog::connect(base)?;
    let mut q = conn.prepare("SELECT g.path,g.directory,g.content_hash,g.app_id,g.bytes,a.last_used,g.updated FROM generations g LEFT JOIN accesses a ON a.path=g.path ORDER BY g.path,g.content_hash")?;
    Ok(q.query_map([], |r| {
        Ok(Entry {
            path: PathBuf::from(r.get::<_, String>(0)?),
            directory: PathBuf::from(r.get::<_, String>(1)?),
            content_hash: r.get(2)?,
            app_id: r.get(3)?,
            bytes: r.get(4)?,
            last_used: r.get(5)?,
            indexed_at: r.get(6)?,
            state: "indexed".into(),
            action: None,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?
    .into_iter()
    .filter(|e| e.directory.is_dir())
    .collect())
}

/// Report also provides a structured test surface independent of stdout.
pub fn inspect(base: &Path, command: &str, options: &Options) -> Result<Report> {
    let base = normalized(base);
    if !base.exists() {
        return Ok(Report {
            cache_path: base,
            bytes: 0,
            path_count: 0,
            generation_count: 0,
            indexed_at: None,
            indexed: true,
            logical_bytes_removed: 0,
            remaining_bytes: 0,
            target_reached: options.max_size.map(|_| true),
            dry_run: options.dry,
            entries: Vec::new(),
        });
    }
    let lease = lock_file(&base.join(".catalog.lock"))?;
    // This serializes reconciliation/deletion with index publication, never the module loader.
    lease.lock()?;
    let detailed =
        command == "clean" || options.path.is_some() || options.prefix.is_some() || options.stale;
    let mut indexed = !detailed;
    let mut entries = if detailed {
        disk_entries(&base)?
    } else {
        match indexed_entries(&base) {
            Ok(entries) => entries,
            Err(_) => {
                indexed = false;
                disk_entries(&base)?
            }
        }
    };
    let bytes: u64 = entries.iter().map(|e| e.bytes).sum();
    if command == "clean" && !options.dry {
        // Reconcile every validated generation, including those excluded by the
        // selector. Otherwise a missing/stale index could report zero after a
        // partial cleanup even though unrelated cache content remains.
        let conn = catalog::connect(&base)?;
        for entry in &entries {
            conn.execute(
                "INSERT INTO generations(directory,path,path_hash,content_hash,app_id,targets,compatibility,bytes,updated) VALUES (?1,?2,?3,?4,?5,'','',?6,unixepoch()) ON CONFLICT(directory) DO UPDATE SET path=excluded.path,path_hash=excluded.path_hash,content_hash=excluded.content_hash,app_id=excluded.app_id,bytes=excluded.bytes,updated=excluded.updated",
                params![entry.directory.to_string_lossy(), entry.path.to_string_lossy(), path_hash(&entry.path), entry.content_hash, entry.app_id, entry.bytes],
            )?;
        }
    }
    let mut remaining = bytes;
    let path_count = entries
        .iter()
        .map(|e| &e.path)
        .collect::<BTreeSet<_>>()
        .len();
    let generation_count = entries.len();
    let indexed_at = entries.iter().filter_map(|e| e.indexed_at).max();
    let selected = options.path.as_deref().map(normalized);
    let mut groups = BTreeMap::<PathBuf, Vec<Entry>>::new();
    for entry in entries.drain(..) {
        groups.entry(entry.path.clone()).or_default().push(entry);
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    if command == "clean" {
        groups.sort_by(|a, b| a.1[0].last_used.cmp(&b.1[0].last_used).then(a.0.cmp(&b.0)));
    }
    for (path, mut group) in groups {
        if selected.as_ref().is_some_and(|p| p != &path)
            || options
                .path_regex
                .as_ref()
                .is_some_and(|r| !r.is_match(&path.to_string_lossy()))
            || options
                .before
                .is_some_and(|before| group[0].last_used.is_none_or(|last| last >= before))
        {
            continue;
        }
        if command == "clean" && options.max_size.is_some_and(|limit| remaining < limit) {
            break;
        }
        let root = group[0]
            .directory
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        let gate = if detailed {
            let gate = lock_file(&root.join(".gate"))?;
            gate.lock()?;
            Some(gate)
        } else {
            None
        };
        let mut locks = Vec::new();
        let mut active = false;
        if detailed {
            // Include *all* generations, even those excluded by a content selector.
            for entry in fs::read_dir(root.join("generations"))? {
                let entry = entry?;
                let id = entry.file_name().to_string_lossy().into_owned();
                if !is_hash(&id) {
                    continue;
                }
                let lock = lock_file(&root.join(".leases").join(&id))?;
                active |= lock.try_lock().is_err();
                locks.push(lock);
            }
        }
        let current = fs::read_to_string(root.join("current")).unwrap_or_default();
        group.retain(|e| {
            !options
                .prefix
                .as_ref()
                .is_some_and(|p| !e.content_hash.starts_with(p))
                && (!options.stale || !path.exists() || e.content_hash != current)
        });
        for mut entry in group {
            if detailed {
                if !entry.directory.is_dir() {
                    continue;
                }
                let actual = catalog::size(&entry.directory)?;
                remaining = remaining.saturating_sub(entry.bytes).saturating_add(actual);
                entry.bytes = actual;
                entry.state = if active { "in-use" } else { "ready" }.into();
            }
            if command == "clean" {
                if active {
                    entry.action = Some("skipped-in-use".into());
                } else {
                    if !options.dry {
                        fs::remove_dir_all(&entry.directory)?;
                        if let Ok(conn) = catalog::connect(&base) {
                            conn.execute(
                                "DELETE FROM generations WHERE directory=?1",
                                [entry.directory.to_string_lossy().as_ref()],
                            )?;
                        }
                    }
                    remaining = remaining.saturating_sub(entry.bytes);
                    entry.action = Some(
                        if options.dry {
                            "would-remove"
                        } else {
                            "removed"
                        }
                        .into(),
                    );
                }
            }
            entries.push(entry);
        }
        drop(locks);
        drop(gate);
    }
    // Reconcile vanished generations after deletion without inventing usage timestamps.
    if command == "clean" && !options.dry {
        let conn = catalog::connect(&base)?;
        let paths = conn
            .prepare("SELECT directory FROM generations")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for path in paths {
            if !Path::new(&path).is_dir() {
                conn.execute("DELETE FROM generations WHERE directory=?1", params![path])?;
            }
        }
    }
    Ok(Report {
        cache_path: base,
        bytes,
        path_count,
        generation_count,
        indexed_at,
        indexed,
        logical_bytes_removed: bytes.saturating_sub(remaining),
        remaining_bytes: remaining,
        target_reached: options.max_size.map(|limit| remaining < limit),
        dry_run: options.dry,
        entries,
    })
}

pub fn command(base: &Path, command: &str, options: &Options) -> Result<()> {
    if command == "rebuild" {
        if !options.dry {
            catalog::rebuild(base)?;
        }
        if options.json {
            println!(
                "{}",
                serde_json::json!({"cachePath":base,"dryRun":options.dry,"action":"rebuild"})
            );
        } else {
            println!(
                "{} cache index: {}",
                if options.dry {
                    "Would rebuild"
                } else {
                    "Rebuilt"
                },
                base.display()
            );
        }
        return Ok(());
    }
    let report = inspect(base, command, options)?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    let summary =
        command == "info" && options.path.is_none() && options.prefix.is_none() && !options.stale;
    if !summary {
        for entry in &report.entries {
            println!(
                "{}\n  App: {}\n  Content: {}\n  Cache: {}\n  Size: {} (logical)\n  Last used: {}\n  State: {}{}",
                entry.path.display(),
                entry.app_id,
                entry.content_hash,
                entry.directory.display(),
                human_size(entry.bytes),
                timestamp(entry.last_used),
                entry.state,
                entry
                    .action
                    .as_ref()
                    .map(|a| format!(" ({a})"))
                    .unwrap_or_default()
            );
        }
    }
    println!(
        "Cache: {}\nTotal: {} (logical{})\nPaths: {}  Generations: {}\nIndex updated: {}",
        report.cache_path.display(),
        human_size(report.bytes),
        if report.indexed { ", indexed" } else { "" },
        report.path_count,
        report.generation_count,
        timestamp(report.indexed_at)
    );
    if command == "clean" {
        println!(
            "{}: {} (logical)\nRemaining: {} (logical)",
            if options.dry {
                "Would remove"
            } else {
                "Removed"
            },
            human_size(report.logical_bytes_removed),
            human_size(report.remaining_bytes)
        );
        if report.target_reached == Some(false) {
            println!(
                "Target not reached: matching inactive caches are insufficient; in-use or excluded caches remain."
            );
        }
    }
    Ok(())
}
