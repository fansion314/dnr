use super::*;
use rusqlite::{Connection, params};
use std::{
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};
enum Event {
    Update(PathBuf, PathBuf),
    Access(PathBuf, PathBuf, i64),
    Flush(mpsc::Sender<()>),
}
static WRITER: OnceLock<mpsc::SyncSender<Event>> = OnceLock::new();
fn send(event: Event) {
    let sender = WRITER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<Event>(64);
        std::thread::spawn(move || {
            while let Ok(first) = receiver.recv() {
                let mut pending = std::collections::BTreeSet::new();
                let mut accesses = std::collections::BTreeMap::new();
                let mut replies = Vec::new();
                for event in std::iter::once(first).chain(receiver.try_iter()) {
                    match event {
                        Event::Update(base, path) => {
                            pending.insert((base, path));
                        }
                        Event::Access(base, path, timestamp) => {
                            accesses
                                .entry((base, path))
                                .and_modify(|t: &mut i64| *t = (*t).max(timestamp))
                                .or_insert(timestamp);
                        }
                        Event::Flush(reply) => replies.push(reply),
                    }
                }
                for (base, path) in pending {
                    let _ = index(&base, &path);
                }
                for ((base, path), timestamp) in accesses {
                    let _ = record_access(&base, &path, timestamp);
                }
                for reply in replies {
                    let _ = reply.send(());
                }
            }
        });
        sender
    });
    // Metadata is best effort. A full queue never delays a module loader.
    let _ = sender.try_send(event);
}
pub fn notify(base: PathBuf, path: PathBuf) {
    send(Event::Update(base, path));
}
pub fn access(base: PathBuf, path: PathBuf) {
    send(Event::Access(base, path, chrono::Utc::now().timestamp()));
}
fn record_access(base: &Path, path: &Path, timestamp: i64) -> Result<()> {
    let gate = lock_file(&base.join(".catalog.lock"))?;
    gate.lock_shared()?;
    connect(base)?.execute("INSERT INTO accesses(path,last_used) VALUES (?1,?2) ON CONFLICT(path) DO UPDATE SET last_used=max(last_used,excluded.last_used)", params![path.to_string_lossy(), timestamp])?;
    Ok(())
}
fn flush_until(sender: &mpsc::SyncSender<Event>, deadline: Instant) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut event = Event::Flush(tx);
    loop {
        match sender.try_send(event) {
            Ok(()) => break,
            Err(mpsc::TrySendError::Full(pending)) => event = pending,
            Err(mpsc::TrySendError::Disconnected(_)) => {
                anyhow::bail!("cache catalog writer disconnected");
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(!remaining.is_zero(), "cache catalog flush timed out");
        // A blocked catalog writer can leave the bounded queue full. Count
        // this wait against the same deadline as the acknowledgement below.
        std::thread::sleep(remaining.min(Duration::from_millis(1)));
    }
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))?;
    Ok(())
}

fn flush_for(timeout: Duration) -> Result<()> {
    WRITER.get().map_or(Ok(()), |sender| {
        flush_until(sender, Instant::now() + timeout)
    })
}

/// Explicit management/test barrier, bounded across both enqueue and completion.
pub fn flush() -> Result<()> {
    flush_for(Duration::from_secs(3))
}

/// Best-effort exit barrier. Disposable index metadata must not delay application exit.
pub fn flush_on_exit() -> Result<()> {
    flush_for(Duration::from_millis(100))
}
pub(super) fn connect(base: &Path) -> Result<Connection> {
    secure_directory(base)?;
    let db = base.join("index.sqlite3");
    if let Ok(meta) = fs::symlink_metadata(&db) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "invalid cache catalog"
        );
    }
    let conn = Connection::open(db)?;
    conn.busy_timeout(std::time::Duration::from_millis(250))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS generations (directory TEXT PRIMARY KEY, path TEXT NOT NULL, path_hash TEXT NOT NULL, content_hash TEXT NOT NULL, app_id TEXT NOT NULL, targets TEXT NOT NULL, compatibility TEXT NOT NULL, bytes INTEGER NOT NULL, updated INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS by_content ON generations(content_hash);")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS accesses(path TEXT PRIMARY KEY, last_used INTEGER NOT NULL);",
    )?;
    Ok(conn)
}
pub(super) fn size(path: &Path) -> Result<u64> {
    let mut bytes = 0;
    let mut queue = vec![path.to_owned()];
    while let Some(p) = queue.pop() {
        for e in fs::read_dir(p)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                queue.push(e.path());
            } else {
                bytes += fs::symlink_metadata(e.path())?.len();
            }
        }
    }
    Ok(bytes)
}
pub fn index(base: &Path, directory: &Path) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    let gate = lock_file(&base.join(".catalog.lock"))?;
    gate.lock()?;
    index_inner(base, directory)
}
fn index_inner(base: &Path, directory: &Path) -> Result<()> {
    let receipt: Receipt = serde_json::from_slice(&fs::read(directory.join("receipt.json"))?)?;
    let bytes = size(directory)?;
    let conn = connect(base)?;
    let names = |path: PathBuf| -> String {
        let mut values = fs::read_dir(path)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        values.sort();
        values.join(",")
    };
    let targets = names(directory.join("native"));
    let compatibility = names(directory.join("compile/generations"));
    conn.execute(
        "INSERT OR REPLACE INTO generations VALUES (?1,?2,?3,?4,?5,?6,?7,?8,unixepoch())",
        params![
            directory.to_string_lossy(),
            receipt.path.to_string_lossy(),
            receipt.path_hash,
            receipt.content_hash,
            receipt.app_id,
            targets,
            compatibility,
            bytes
        ],
    )?;
    let mut query = conn.prepare("SELECT directory FROM generations WHERE path_hash=?1")?;
    let old = query
        .query_map([&receipt.path_hash], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for path in old {
        if !Path::new(&path).is_dir() {
            conn.execute("DELETE FROM generations WHERE directory=?1", [path])?;
        }
    }
    Ok(())
}
pub fn candidates(base: &Path, content: &str) -> Vec<PathBuf> {
    let query = || -> Result<Vec<PathBuf>> {
        let gate = lock_file(&base.join(".catalog.lock"))?;
        gate.try_lock_shared()?;
        let conn = Connection::open_with_flags(
            base.join("index.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        conn.busy_timeout(std::time::Duration::ZERO)?;
        let mut q = conn.prepare("SELECT directory FROM generations WHERE content_hash=?1")?;
        Ok(q.query_map([content], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(PathBuf::from)
            .collect())
    };
    query().unwrap_or_default()
}
pub fn rebuild(base: &Path) -> Result<()> {
    // Serialize rebuild against catalog readers and asynchronous writers.
    let gate = lock_file(&base.join(".catalog.lock"))?;
    gate.lock()?;
    let accesses = read_accesses(base).unwrap_or_default();
    for suffix in ["", "-wal", "-shm"] {
        let path = base.join(format!("index.sqlite3{suffix}"));
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    let conn = connect(base)?;
    for (path, timestamp) in accesses {
        conn.execute(
            "INSERT INTO accesses VALUES (?1,?2)",
            params![path, timestamp],
        )?;
    }
    drop(conn);
    for path in directories(base)? {
        if let Err(error) = index_inner(base, &path) {
            eprintln!("Skipped cache receipt {}: {error}", path.display());
        }
    }
    Ok(())
}
pub(super) fn read_accesses(base: &Path) -> Result<std::collections::BTreeMap<String, i64>> {
    let conn = Connection::open_with_flags(
        base.join("index.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let mut q = conn.prepare("SELECT path,last_used FROM accesses")?;
    Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?)
}
pub fn directories(base: &Path) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    let Ok(paths) = fs::read_dir(base.join("v4")) else {
        return Ok(result);
    };
    for path in paths {
        let path = path?;
        if !path.file_type()?.is_dir() || !is_hash(&path.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(generations) = fs::read_dir(path.path().join("generations")) else {
            continue;
        };
        for generation in generations {
            let generation = generation?;
            if generation.file_type()?.is_dir()
                && is_hash(&generation.file_name().to_string_lossy())
            {
                result.push(generation.path());
            }
        }
    }
    Ok(result)
}
pub fn command(
    base: &Path,
    command: &str,
    path: Option<&Path>,
    prefix: Option<&str>,
    stale: bool,
    dry: bool,
) -> Result<()> {
    super::management::command(
        base,
        command,
        &super::management::Options {
            path: path.map(Path::to_owned),
            prefix: prefix.map(str::to_owned),
            stale,
            dry,
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_does_not_block_on_a_full_or_disconnected_queue() {
        let (sender, receiver) = mpsc::sync_channel(1);
        assert!(
            sender
                .try_send(Event::Access(PathBuf::new(), PathBuf::new(), 0))
                .is_ok()
        );
        let start = Instant::now();
        assert!(flush_until(&sender, start + Duration::from_millis(30)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        drop(receiver);
        assert!(flush_until(&sender, Instant::now() + Duration::from_secs(3)).is_err());
    }

    #[test]
    fn expired_deadline_applies_to_queue_admission_and_acknowledgement() {
        let (sender, receiver) = mpsc::sync_channel(1);
        assert!(
            sender
                .try_send(Event::Access(PathBuf::new(), PathBuf::new(), 0))
                .is_ok()
        );
        let deadline = Instant::now();
        // An expired admission wait leaves the queued event untouched.
        assert!(flush_until(&sender, deadline).is_err());
        assert!(matches!(receiver.try_recv().unwrap(), Event::Access(..)));
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));

        // Freeing the queue must not start a fresh acknowledgement budget.
        // Keep the reply sender alive so this fails specifically on the
        // original deadline, rather than a disconnected writer.
        let result = flush_until(&sender, deadline).unwrap_err();
        assert_eq!(
            result.downcast_ref::<mpsc::RecvTimeoutError>(),
            Some(&mpsc::RecvTimeoutError::Timeout)
        );
        let Event::Flush(reply) = receiver.try_recv().unwrap() else {
            panic!("expected flush barrier");
        };
        assert!(reply.send(()).is_err());
    }

    #[test]
    fn flush_succeeds_when_the_writer_acknowledges() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let Event::Flush(reply) = receiver.recv().unwrap() else {
                panic!("expected flush barrier");
            };
            reply.send(()).unwrap();
        });
        flush_until(&sender, Instant::now() + Duration::from_secs(1)).unwrap();
        worker.join().unwrap();
    }
}
