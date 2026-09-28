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
    last_access: std::sync::atomic::AtomicI64,
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
        let root = base.join("v4").join(&receipt.path_hash);
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
            receipt.path.join(crate::metadata::INSTALL)
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
            last_access: std::sync::atomic::AtomicI64::new(0),
        });
        Ok(result)
    }
    pub fn finish(&self) {
        if self
            .last_access
            .swap(0, std::sync::atomic::Ordering::Relaxed)
            != 0
        {
            catalog::access(self.base.clone(), self.receipt.path.clone());
        }
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
    /// Runtime use, distinct from installation, inspection and index maintenance.
    pub fn mark_used(&self) {
        use std::sync::atomic::Ordering;
        let now = chrono::Utc::now().timestamp();
        let old = self.last_access.load(Ordering::Relaxed);
        if (old == 0 || now.saturating_sub(old) >= 60)
            && self
                .last_access
                .compare_exchange(old, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            catalog::access(self.base.clone(), self.receipt.path.clone());
        }
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
                || !directory.starts_with(self.generation.base.join("v4"))
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

pub mod catalog;
pub mod management;
