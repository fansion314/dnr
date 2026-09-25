//! Path-owned cache generations. SQLite is a disposable catalog, never an authority.
use crate::{index::digest, materialize::secure_directory};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceStamp(u64, u64, u64, i64, i64, i64, i64);
impl SourceStamp {
    pub fn read(path: &Path) -> Result<Self> {
        Ok(Self::from_metadata(&fs::metadata(path)?))
    }
    pub(crate) fn from_metadata(m: &fs::Metadata) -> Self {
        Self(
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    }
}
pub fn path_hash(path: &Path) -> String {
    digest(path.as_os_str().as_bytes())
}
pub fn hash(bytes: &[u8]) -> String {
    digest(bytes)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub path: PathBuf,
    pub path_hash: String,
    pub content_hash: String,
    pub app_id: String,
}
pub struct Generation {
    pub directory: PathBuf,
    pub base: PathBuf,
    pub receipt: Receipt,
    lease: Mutex<Option<File>>,
}
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("cache entry needs parent")?;
    secure_directory(parent)?;
    let staging = parent.join(".staging");
    secure_directory(&staging)?;
    let mut stage = tempfile::NamedTempFile::new_in(&staging)?;
    stage.write_all(bytes)?;
    stage.flush()?;
    stage.persist(path)?;
    Ok(())
}
pub(crate) fn lock_file(path: &Path) -> Result<File> {
    secure_directory(path.parent().context("lock needs parent")?)?;
    match File::create_new(path) {
        Ok(f) => Ok(f),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let m = fs::symlink_metadata(path)?;
            ensure!(
                m.is_file() && !m.file_type().is_symlink(),
                "invalid cache lock"
            );
            Ok(File::open(path)?)
        }
        Err(e) => Err(e.into()),
    }
}
fn is_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
impl Generation {
    pub fn open(
        base: &Path,
        path: &Path,
        content: &str,
        app_id: &str,
        stamp: &SourceStamp,
    ) -> Result<Arc<Self>> {
        ensure!(is_hash(content), "invalid content hash");
        secure_directory(base)?;
        let canonical_base = base.canonicalize()?;
        let base = canonical_base.as_path();
        let path = path.canonicalize()?;
        let receipt = Receipt {
            path_hash: path_hash(&path),
            path,
            content_hash: content.into(),
            app_id: app_id.into(),
        };
        let root = base.join("v3").join(&receipt.path_hash);
        let gate = lock_file(&root.join(".gate"))?;
        gate.lock()?;
        let directory = root.join("generations").join(content);
        let lease = lock_file(&root.join(".leases").join(content))?;
        let inactive = lease.try_lock().is_ok();
        if !inactive {
            lease.lock_shared()?;
        }
        secure_directory(&directory)?;
        if inactive {
            clear_staging(&root);
            cleanup_partials(&directory);
            lease.lock_shared()?;
        }
        let receipt_path = directory.join("receipt.json");
        let expected = serde_json::to_vec(&receipt)?;
        if fs::read(&receipt_path).ok().as_deref() != Some(&expected) {
            atomic_write(&receipt_path, &expected)?;
        }
        let source = if receipt.path.is_dir() {
            receipt.path.join(crate::v3::INSTALL)
        } else {
            receipt.path.clone()
        };
        // A process holding an older open ZIP may use its generation but must not roll back current.
        if SourceStamp::read(&source).ok().as_ref() == Some(stamp)
            && fs::read_to_string(root.join("current")).ok().as_deref() != Some(content)
        {
            atomic_write(&root.join("current"), content.as_bytes())?;
        }
        collect_old(&root)?;
        let result = Arc::new(Self {
            directory,
            base: base.into(),
            receipt,
            lease: Mutex::new(Some(lease)),
        });
        Ok(result)
    }
    pub fn finish(&self) {
        self.lease.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(root) = self.directory.parent().and_then(Path::parent)
            && let Ok(gate) = lock_file(&root.join(".gate"))
            && gate.try_lock().is_ok()
        {
            let _ = collect_old(root);
        }
    }
    pub fn compile(self: &Arc<Self>, fingerprint: &str) -> Result<CompileCache> {
        let id = digest(fingerprint.as_bytes());
        let parent = self.directory.join("compile");
        let gate = lock_file(&parent.join(".gate"))?;
        gate.lock()?;
        let lease = lock_file(&parent.join(".leases").join(&id))?;
        lease.lock_shared()?;
        secure_directory(&parent.join("generations").join(&id))?;
        if fs::read_to_string(parent.join("current")).ok().as_deref() != Some(&id) {
            atomic_write(&parent.join("current"), id.as_bytes())?;
        }
        collect_old(&parent)?;
        Ok(CompileCache {
            generation: self.clone(),
            directory: parent.join("generations").join(&id),
            fingerprint: id,
            candidates: OnceLock::new(),
            lease: Some(lease),
        })
    }
    pub fn notify_index(&self) {
        catalog::notify(self.base.clone(), self.directory.clone());
    }
}
impl Drop for Generation {
    fn drop(&mut self) {
        self.finish();
    }
}
fn collect_old(root: &Path) -> Result<()> {
    let current = fs::read_to_string(root.join("current")).unwrap_or_default();
    let Ok(entries) = fs::read_dir(root.join("generations")) else {
        return Ok(());
    };
    for e in entries {
        let e = e?;
        let id = e.file_name().to_string_lossy().into_owned();
        if id != current && is_hash(&id) && e.file_type()?.is_dir() {
            let lock = lock_file(&root.join(".leases").join(&id))?;
            if lock.try_lock().is_ok() {
                fs::remove_dir_all(e.path())?;
                fs::remove_file(root.join(".leases").join(&id))?;
            }
        }
    }
    Ok(())
}

fn real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}
fn clear_staging(parent: &Path) {
    let staging = parent.join(".staging");
    if !real_dir(&staging) {
        return;
    }
    if let Ok(entries) = fs::read_dir(staging) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                let _ = fs::remove_dir_all(entry.path());
            } else {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}
fn cleanup_partials(directory: &Path) {
    // Only called with an exclusive generation lease. Never enumerate module cache files.
    clear_staging(directory);
    let compile = directory.join("compile");
    if real_dir(&compile) {
        clear_staging(&compile);
        let generations = compile.join("generations");
        if real_dir(&generations)
            && let Ok(entries) = fs::read_dir(generations)
        {
            for entry in entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            {
                for kind in ["emit", "code"] {
                    let parent = entry.path().join(kind);
                    if real_dir(&parent) {
                        clear_staging(&parent);
                    }
                }
            }
        }
    }
    let native = directory.join("native");
    if real_dir(&native)
        && let Ok(targets) = fs::read_dir(native)
    {
        for target in targets
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        {
            if let Ok(entries) = fs::read_dir(target.path()) {
                for entry in entries.flatten() {
                    if entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".dnr-stage-")
                        && entry.file_type().is_ok_and(|t| t.is_dir())
                    {
                        let _ = fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
    }
}

pub struct CompileCache {
    pub generation: Arc<Generation>,
    directory: PathBuf,
    lease: Option<File>,
    fingerprint: String,
    candidates: OnceLock<Vec<PathBuf>>,
}
impl CompileCache {
    fn path(&self, kind: &str, identity: &str) -> PathBuf {
        self.directory.join(kind).join(digest(identity.as_bytes()))
    }
    pub fn get(&self, kind: &str, identity: &str, source_key: &str) -> Option<Vec<u8>> {
        if !matches!(kind, "code" | "emit") {
            return None;
        }
        fn read(path: &Path, key: &str) -> Option<(Vec<u8>, Vec<u8>)> {
            let meta = fs::symlink_metadata(path).ok()?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 256 * 1024 * 1024 {
                return None;
            }
            let bytes = fs::read(path).ok()?;
            if bytes.len() < 72 || &bytes[..8] != b"DNRCACH3" {
                return None;
            }
            use sha2::{Digest, Sha256};
            if Sha256::digest(key.as_bytes()).as_slice() != &bytes[8..40]
                || Sha256::digest(&bytes[72..]).as_slice() != &bytes[40..72]
            {
                return None;
            }
            Some((bytes[72..].to_vec(), bytes))
        }
        let path = self.path(kind, identity);
        if let Some((payload, _)) = read(&path, source_key) {
            return Some(payload);
        }
        let candidates = self.candidates.get_or_init(|| {
            catalog::candidates(&self.generation.base, &self.generation.receipt.content_hash)
        });
        for directory in candidates {
            if directory == &self.generation.directory
                || !directory.starts_with(self.generation.base.join("v3"))
                || directory.canonicalize().ok().as_ref() != Some(directory)
            {
                continue;
            }
            let source = directory
                .join("compile/generations")
                .join(&self.fingerprint)
                .join(kind)
                .join(digest(identity.as_bytes()));
            if let Some((payload, bytes)) = read(&source, source_key) {
                if secure_directory(path.parent()?).is_ok() && !path.exists() {
                    if fs::hard_link(&source, &path).is_err() {
                        let _ = atomic_write(&path, &bytes);
                    }
                    self.generation.notify_index();
                }
                return Some(payload);
            }
        }
        None
    }
    pub fn set(&self, kind: &str, identity: &str, source_key: &str, payload: &[u8]) -> Result<()> {
        ensure!(matches!(kind, "emit" | "code"), "invalid cache kind");
        use sha2::{Digest, Sha256};
        let mut bytes = Vec::with_capacity(72 + payload.len());
        bytes.extend(b"DNRCACH3");
        bytes.extend(Sha256::digest(source_key.as_bytes()));
        bytes.extend(Sha256::digest(payload));
        bytes.extend(payload);
        atomic_write(&self.path(kind, identity), &bytes)?;
        self.generation.notify_index();
        Ok(())
    }
}
impl Drop for CompileCache {
    fn drop(&mut self) {
        self.lease.take();
        if let Some(root) = self.directory.parent().and_then(Path::parent)
            && let Ok(gate) = lock_file(&root.join(".gate"))
            && gate.try_lock().is_ok()
        {
            let _ = collect_old(root);
        }
    }
}

pub mod catalog {
    use super::*;
    use rusqlite::{Connection, params};
    use std::sync::{OnceLock, mpsc};
    enum Event {
        Update(PathBuf, PathBuf),
        Flush(mpsc::Sender<()>),
    }
    static WRITER: OnceLock<mpsc::SyncSender<Event>> = OnceLock::new();
    pub fn notify(base: PathBuf, path: PathBuf) {
        let sender = WRITER.get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<Event>(64);
            std::thread::spawn(move || {
                while let Ok(first) = receiver.recv() {
                    let mut pending = std::collections::BTreeSet::new();
                    let mut replies = Vec::new();
                    for event in std::iter::once(first).chain(receiver.try_iter()) {
                        match event {
                            Event::Update(base, path) => {
                                pending.insert((base, path));
                            }
                            Event::Flush(reply) => replies.push(reply),
                        }
                    }
                    for (base, path) in pending {
                        let _ = index(&base, &path);
                    }
                    for reply in replies {
                        let _ = reply.send(());
                    }
                }
            });
            sender
        });
        // Metadata is best effort. A full queue never delays a module loader.
        let _ = sender.try_send(Event::Update(base, path));
    }
    /// Explicit management/test barrier; application startup and exit never wait for SQLite.
    pub fn flush() -> Result<()> {
        if let Some(sender) = WRITER.get() {
            let (tx, rx) = mpsc::channel();
            sender.send(Event::Flush(tx))?;
            rx.recv_timeout(std::time::Duration::from_secs(3))?;
        }
        Ok(())
    }
    fn connect(base: &Path) -> Result<Connection> {
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
        Ok(conn)
    }
    fn size(path: &Path) -> Result<u64> {
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
        for suffix in ["", "-wal", "-shm"] {
            let path = base.join(format!("index.sqlite3{suffix}"));
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        let _ = connect(base)?;
        for path in directories(base)? {
            if let Err(error) = index_inner(base, &path) {
                eprintln!("Skipped cache receipt {}: {error}", path.display());
            }
        }
        Ok(())
    }
    pub fn directories(base: &Path) -> Result<Vec<PathBuf>> {
        let mut result = Vec::new();
        let Ok(paths) = fs::read_dir(base.join("v3")) else {
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
        if command == "rebuild" {
            if dry {
                println!("Would rebuild {}", base.join("index.sqlite3").display());
                return Ok(());
            }
            return rebuild(base);
        }
        let catalog_lease = lock_file(&base.join(".catalog.lock"))?;
        catalog_lease.lock_shared()?;
        if command != "clean"
            && path.is_none()
            && prefix.is_none()
            && !stale
            && let Ok(conn) = connect(base)
        {
            let mut q = conn.prepare(
                "SELECT path,content_hash,bytes,directory FROM generations ORDER BY path",
            )?;
            let rows = q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u64>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            for row in rows {
                let (path, hash, bytes, dir) = row?;
                if Path::new(&dir).is_dir() {
                    println!("{path} {hash} {bytes} logical bytes (indexed)");
                }
            }
            return Ok(());
        }
        let selected = path.map(|p| {
            p.canonicalize().unwrap_or_else(|_| {
                crate::materialize::lexical(&if p.is_absolute() {
                    p.to_owned()
                } else {
                    std::env::current_dir().unwrap_or_default().join(p)
                })
            })
        });
        for directory in directories(base)? {
            let Ok(bytes) = fs::read(directory.join("receipt.json")) else {
                continue;
            };
            let Ok(receipt) = serde_json::from_slice::<Receipt>(&bytes) else {
                continue;
            };
            if selected.as_ref().is_some_and(|p| p != &receipt.path)
                || prefix.is_some_and(|p| !receipt.content_hash.starts_with(p))
            {
                continue;
            }
            let root = directory.parent().unwrap().parent().unwrap();
            if receipt.path_hash != root.file_name().unwrap().to_string_lossy()
                || receipt.content_hash != directory.file_name().unwrap().to_string_lossy()
            {
                continue;
            }
            let obsolete = !receipt.path.exists()
                || fs::read_to_string(root.join("current")).ok().as_deref()
                    != Some(&receipt.content_hash);
            if stale && !obsolete {
                continue;
            }
            let gate = lock_file(&root.join(".gate"))?;
            gate.lock()?;
            let lease = lock_file(&root.join(".leases").join(&receipt.content_hash))?;
            let active = lease.try_lock().is_err();
            println!(
                "{} {} {} logical bytes {}",
                receipt.path.display(),
                receipt.content_hash,
                size(&directory)?,
                if active { "in-use" } else { "ready" }
            );
            if command == "clean" && !active && !dry {
                fs::remove_dir_all(&directory)?;
            }
        }
        Ok(())
    }
}
