//! Immutable, content-addressed native groups. No application-data storage here.
use crate::{
    Package,
    v2::{self, Record},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Clone, Copy, Debug)]
pub enum NativeUse {
    Library,
    Executable,
}
pub(crate) struct Groups {
    slots: HashMap<String, GroupSlot>,
    sidecar_target: Option<PathBuf>,
    sidecar_present: OnceLock<bool>,
    cache: OnceLock<PathBuf>,
    cache_target: OnceLock<PathBuf>,
    stats: Counters,
}
#[derive(Default)]
struct GroupSlot {
    selection: Mutex<()>,
    bound: OnceLock<Bound>,
    layout: OnceLock<Layout>,
}
struct Bound {
    directory: PathBuf,
    root: PathBuf,
    ready: AtomicBool,
    lease: Mutex<Option<File>>,
    sidecar: bool,
}
struct Layout {
    nodes: HashMap<PathBuf, Option<usize>>,
    bytes: u64,
}
#[derive(Default)]
struct Counters {
    sidecar_probes: AtomicU64,
    verifications: AtomicU64,
    files_hashed: AtomicU64,
    bytes_hashed: AtomicU64,
    extractions: AtomicU64,
}
/// Native-cache work only; ordinary path hits do not update counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NativeCacheStats {
    pub sidecar_probes: u64,
    pub verifications: u64,
    pub files_hashed: u64,
    pub bytes_hashed: u64,
    pub extractions: u64,
}
impl Groups {
    pub(crate) fn new(manifest: &crate::Manifest, state: Option<&v2::State>, path: &Path) -> Self {
        Self {
            slots: manifest
                .groups
                .iter()
                .map(|g| (g.clone(), GroupSlot::default()))
                .collect(),
            sidecar_target: state.filter(|s| s.has_groups()).map(|s| {
                if manifest.format_version == 3 {
                    sidecar(path).join("v3").join(&s.id).join(&s.target)
                } else {
                    generation(&sidecar(path), s)
                }
            }),
            sidecar_present: OnceLock::new(),
            cache: OnceLock::new(),
            cache_target: OnceLock::new(),
            stats: Counters::default(),
        }
    }
    pub(crate) fn cache_base(&self) -> Result<PathBuf> {
        self.cache
            .get()
            .cloned()
            .map(Ok)
            .unwrap_or_else(cache_directory)
    }
    fn alias_suffix<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        // Two generation prefixes, regardless of the number of groups. A group
        // ID is one path component, followed by the fixed "root" component.
        for base in [
            self.sidecar_target.as_deref(),
            self.cache_target.get().map(PathBuf::as_path),
        ]
        .into_iter()
        .flatten()
        {
            let Ok(relative) = path.strip_prefix(base) else {
                continue;
            };
            let Some(id) = relative
                .components()
                .next()
                .and_then(|c| c.as_os_str().to_str())
            else {
                continue;
            };
            let Some(bound) = self.slots.get(id).and_then(|s| s.bound.get()) else {
                continue;
            };
            if let Ok(suffix) = path.strip_prefix(&bound.root) {
                return Some(suffix);
            }
        }
        None
    }
}
fn generation(base: &Path, state: &v2::State) -> PathBuf {
    base.join("v2")
        .join(&state.id[..2])
        .join(&state.id)
        .join(&state.target)
}
fn normalized_path(path: &Path) -> Cow<'_, Path> {
    if path.is_absolute() && path.components().any(|c| c == Component::ParentDir) {
        Cow::Owned(lexical(path))
    } else {
        Cow::Borrowed(path)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Receipt {
    pub format: u32,
    pub package_id: String,
    pub app_id: String,
    pub target: String,
    pub group: String,
    pub bytes: u64,
}

pub fn cache_directory() -> Result<PathBuf> {
    let path = if let Some(path) = std::env::var_os("DNR_CACHE_DIR") {
        PathBuf::from(path)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME").context("HOME is unset; set DNR_CACHE_DIR")?)
            .join("Library/Caches/dnr")
    } else if let Some(path) = std::env::var_os("XDG_CACHE_HOME").filter(|s| !s.is_empty()) {
        PathBuf::from(path).join("dnr")
    } else {
        PathBuf::from(std::env::var_os("HOME").context("HOME is unset; set DNR_CACHE_DIR")?)
            .join(".cache/dnr")
    };
    ensure!(
        path.is_absolute(),
        "cache directory must be absolute: {}",
        path.display()
    );
    Ok(lexical(&path))
}

pub(crate) fn lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
pub(crate) fn sidecar(package: &Path) -> PathBuf {
    let mut name = package.as_os_str().to_os_string();
    name.push(".unpacked");
    PathBuf::from(name)
}

impl Package {
    /// Override the user cache for an embedded host or an isolated test.
    pub fn with_cache_directory(mut self, path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "cache directory must be absolute");
        ensure!(
            self.materialized
                .slots
                .values()
                .all(|s| s.bound.get().is_none()),
            "cannot change cache after issuing native paths"
        );
        self.materialized.cache = OnceLock::from(lexical(path));
        self.materialized.cache_target = OnceLock::new();
        Ok(self)
    }
    pub fn native_cache_stats(&self) -> NativeCacheStats {
        let stats = &self.materialized.stats;
        NativeCacheStats {
            sidecar_probes: stats.sidecar_probes.load(Ordering::Relaxed),
            verifications: stats.verifications.load(Ordering::Relaxed),
            files_hashed: stats.files_hashed.load(Ordering::Relaxed),
            bytes_hashed: stats.bytes_hashed.load(Ordering::Relaxed),
            extractions: stats.extractions.load(Ordering::Relaxed),
        }
    }
    pub fn package_id(&self) -> Option<&str> {
        self.v2.as_ref().map(|v| v.id.as_str())
    }
    pub fn selected_target(&self) -> Option<&str> {
        self.v2.as_ref().map(|v| v.target.as_str())
    }
    pub fn check_platform(&self) -> Result<()> {
        if let Some(v) = &self.v2 {
            ensure!(
                self.manifest.targets.is_empty() || self.manifest.targets.contains_key(&v.target),
                "package does not support {}; available: {}",
                v.target,
                self.manifest
                    .targets
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok(())
    }
    pub(crate) fn group_directory(&self, base: &Path, group: &str) -> Result<PathBuf> {
        let v = self
            .v2
            .as_ref()
            .context("repack as format v2 to use native groups")?;
        if self.manifest.format_version == 3 {
            return Ok(base.join("v3").join(&v.id).join(&v.target).join(group));
        }
        Ok(base
            .join("v2")
            .join(&v.id[..2])
            .join(&v.id)
            .join(&v.target)
            .join(group))
    }
    pub(crate) fn group_records(&self, group: &str) -> Result<Vec<&Record>> {
        let v = self.v2.as_ref().context("v2 group required")?;
        ensure!(
            self.materialized.slots.contains_key(group),
            "unknown group: {group}"
        );
        let records = v.group(group);
        ensure!(
            !records.is_empty() && records.iter().any(|r| r.native.is_some()),
            "group {group} has no implementation for {}",
            v.target
        );
        Ok(records)
    }
    fn group_layout(&self, group: &str) -> Result<&Layout> {
        let state = self.v2.as_ref().context("v2 group required")?;
        let slot = self
            .materialized
            .slots
            .get(group)
            .context("unknown group")?;
        let members = state
            .group_indices(group)
            .context("group unavailable on this platform")?;
        Ok(slot.layout.get_or_init(|| {
            let mut nodes = HashMap::new();
            let mut bytes = 0;
            for &index in members {
                let r = &state.records[index];
                bytes += r.size;
                nodes.insert(
                    PathBuf::from(&r.path),
                    if r.kind == "directory" {
                        None
                    } else {
                        Some(index)
                    },
                );
                for parent in Path::new(&r.path)
                    .ancestors()
                    .skip(1)
                    .filter(|p| !p.as_os_str().is_empty())
                {
                    nodes.entry(parent.to_owned()).or_insert(None);
                }
            }
            Layout { nodes, bytes }
        }))
    }
    fn receipt(&self, group: &str) -> Result<Receipt> {
        let v = self.v2.as_ref().context("v2 group required")?;
        Ok(Receipt {
            format: self.manifest.format_version,
            package_id: v.id.clone(),
            app_id: self.manifest.app_id.clone(),
            target: v.target.clone(),
            group: group.into(),
            bytes: self.group_layout(group)?.bytes,
        })
    }
    fn valid_group(&self, directory: &Path, group: &str) -> Result<bool> {
        let verify = || -> Result<()> {
            real_directory(directory)?;
            self.materialized
                .stats
                .verifications
                .fetch_add(1, Ordering::Relaxed);
            let receipt_path = directory.join("receipt.json");
            let meta = fs::symlink_metadata(&receipt_path)?;
            ensure!(meta.is_file() && meta.len() <= 65536, "invalid receipt");
            let receipt: Receipt = serde_json::from_slice(&fs::read(receipt_path)?)?;
            let state = self.v2.as_ref().unwrap();
            let layout = self.group_layout(group)?;
            ensure!(
                receipt.format == self.manifest.format_version
                    && receipt.package_id == state.id
                    && receipt.target == state.target
                    && receipt.group == group
                    && receipt.bytes == layout.bytes,
                "receipt mismatch"
            );
            let root = directory.join("root");
            real_directory(&root)?;
            let mut queue = vec![root.clone()];
            let mut count = 0usize;
            // Visit each directory and file once. Parent traversal establishes
            // that descendants cannot be reached through an archive symlink.
            while let Some(dir) = queue.pop() {
                for child in fs::read_dir(dir)? {
                    let child = child?;
                    let path = child.path();
                    let expected = layout
                        .nodes
                        .get(path.strip_prefix(&root)?)
                        .context("unexpected file in native group")?;
                    count += 1;
                    let kind = child.file_type()?;
                    let Some(index) = expected else {
                        ensure!(kind.is_dir() && !kind.is_symlink(), "invalid directory");
                        queue.push(path);
                        continue;
                    };
                    let r = &state.records[*index];
                    if r.kind == "symlink" {
                        ensure!(
                            kind.is_symlink()
                                && fs::read_link(&path)?.to_str() == r.link.as_deref(),
                            "symlink mismatch"
                        );
                    } else {
                        ensure!(
                            kind.is_file() && !kind.is_symlink(),
                            "not a regular group file"
                        );
                        let meta = child.metadata()?;
                        ensure!(meta.len() == r.size, "file size mismatch");
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            ensure!(
                                meta.permissions().mode() & 0o777 == r.mode & !0o222,
                                "file mode mismatch"
                            );
                        }
                        self.materialized
                            .stats
                            .files_hashed
                            .fetch_add(1, Ordering::Relaxed);
                        self.materialized
                            .stats
                            .bytes_hashed
                            .fetch_add(r.size, Ordering::Relaxed);
                        ensure!(
                            Some(v2::file_digest(&path)?.as_str()) == r.sha256.as_deref(),
                            "cached file checksum mismatch"
                        );
                    }
                }
            }
            ensure!(count == layout.nodes.len(), "incomplete native group");
            Ok(())
        };
        Ok(verify().is_ok())
    }
    fn bind_group(&self, group: &str) -> Result<&Bound> {
        let groups = &self.materialized;
        let slot = groups.slots.get(group).context("unknown native group")?;
        if let Some(bound) = slot.bound.get() {
            return Ok(bound);
        }
        let _selection = slot.selection.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(bound) = slot.bound.get() {
            return Ok(bound);
        }
        self.check_platform()?;
        let mut selected = None;
        if let Some(base) = &groups.sidecar_target {
            let present = *groups.sidecar_present.get_or_init(|| {
                groups.stats.sidecar_probes.fetch_add(1, Ordering::Relaxed);
                real_directory(base).is_ok()
            });
            if present {
                let directory = base.join(group);
                // Validate once while holding the same lease used by loading.
                // It also prevents our cleaner from invalidating issued URLs.
                if let Ok(lease) = group_lock(&directory, false) {
                    lease.lock_shared()?;
                    if self.valid_group(&directory, group)? {
                        selected = Some(Bound {
                            root: directory.join("root"),
                            directory,
                            ready: AtomicBool::new(true),
                            lease: Mutex::new(Some(lease)),
                            sidecar: true,
                        });
                    }
                }
            }
        }
        let bound = match selected {
            Some(bound) => bound,
            None => {
                if groups.cache_target.get().is_none() {
                    let base = groups
                        .cache
                        .get()
                        .cloned()
                        .map(Ok)
                        .unwrap_or_else(cache_directory)?;
                    let v = self.v2.as_ref().unwrap();
                    let target = if self.manifest.format_version == 3 {
                        self.cache_generation()?
                            .directory
                            .join("native")
                            .join(&v.target)
                    } else {
                        generation(&base, v)
                    };
                    let _ = groups.cache_target.set(target);
                }
                let directory = groups.cache_target.get().unwrap().join(group);
                Bound {
                    root: directory.join("root"),
                    directory,
                    ready: AtomicBool::new(false),
                    lease: Mutex::new(None),
                    sidecar: false,
                }
            }
        };
        let _ = slot.bound.set(bound);
        Ok(slot.bound.get().unwrap())
    }
    fn indexed_record(&self, name: &str) -> Result<Option<&Record>> {
        let Some(state) = &self.v2 else {
            return Ok(None);
        };
        if let Some(record) = state.record(name).filter(|r| r.kind != "symlink") {
            return Ok(Some(record));
        }
        let resolved = self.resolve(name)?;
        Ok(state.record(&resolved))
    }
    /// Reserve a stable group URL without extracting any file. Non-group files
    /// return before locks, filesystem calls, or construction of physical paths.
    pub fn module_path(&self, name: &str) -> Result<Option<PathBuf>> {
        if self.v2.as_ref().is_none_or(|s| !s.has_groups()) {
            return Ok(None);
        }
        let Some(record) = self.indexed_record(name)? else {
            return Ok(None);
        };
        let Some(group) = record
            .group
            .as_deref()
            .filter(|_| record.kind != "directory")
        else {
            return Ok(None);
        };
        Ok(Some(self.bind_group(group)?.root.join(&record.path)))
    }
    pub fn logical_path(&self, path: &Path) -> Option<String> {
        let path = normalized_path(path);
        self.materialized
            .alias_suffix(&path)
            .or_else(|| path.strip_prefix(&self.root).ok())
            .filter(|r| !r.as_os_str().is_empty())
            .map(|r| r.to_string_lossy().into_owned())
    }
    /// Filesystem hot path: retain the caller's allocation when no alias or
    /// parent traversal needs translating (including all plain v1/v2 paths).
    pub fn virtual_path_cow<'a>(&self, path: &'a Path) -> Cow<'a, Path> {
        let original = path;
        let path = normalized_path(path);
        if let Some(relative) = self.materialized.alias_suffix(&path) {
            return Cow::Owned(self.root.join(relative));
        }
        match path {
            Cow::Owned(normalized) if normalized.starts_with(&self.root) => Cow::Owned(normalized),
            _ => Cow::Borrowed(original),
        }
    }
    pub fn virtual_path(&self, path: &Path) -> PathBuf {
        self.virtual_path_cow(path).into_owned()
    }
    pub fn is_unavailable(&self, name: &str) -> bool {
        self.v2.as_ref().is_some_and(|v| {
            !v.unavailable.is_empty()
                && (v.unavailable.contains(name)
                    || Path::new(name)
                        .ancestors()
                        .skip(1)
                        .any(|p| v.unavailable.contains(p.to_string_lossy().as_ref())))
        })
    }
    pub fn is_unavailable_path(&self, path: &Path) -> bool {
        self.v2.as_ref().is_some_and(|v| !v.unavailable.is_empty())
            && self
                .logical_path(path)
                .is_some_and(|name| self.is_unavailable(&name))
    }

    pub fn native_path(&self, name: &str, usage: NativeUse) -> Result<Option<PathBuf>> {
        if self.v2.is_none() {
            return match usage {
                NativeUse::Library => self.native_library_path(name),
                NativeUse::Executable => {
                    let name = self.resolve(name)?;
                    ensure!(
                        !self.entries.contains_key(&name),
                        "packaged executables require format v2"
                    );
                    Ok(None)
                }
            };
        }
        let Some(record) = self.indexed_record(name)? else {
            let name = self.resolve(name)?;
            ensure!(
                !self.is_unavailable(&name),
                "native file is unavailable for {}: {name}",
                self.selected_target().unwrap()
            );
            ensure!(
                !self.entries.contains_key(&name),
                "native path is not a regular file"
            );
            return Ok(None);
        };
        ensure!(record.kind == "file", "native path is not a regular file");
        let allowed = match usage {
            NativeUse::Library => matches!(record.native.as_deref(), Some("library" | "addon")),
            NativeUse::Executable => record.native.as_deref() == Some("executable"),
        };
        ensure!(
            allowed,
            "undeclared packaged native {:?}: {name}; add it to a group in dnr.package.json",
            usage
        );
        let group = record
            .group
            .as_deref()
            .context("native file has no group")?;
        Ok(Some(self.ensure_group(group)?.join(&record.path)))
    }
    pub fn required_napi(&self, name: &str) -> Option<u32> {
        self.indexed_record(name).ok().flatten()?.napi
    }
    pub fn is_executable(&self, name: &str) -> bool {
        self.v2
            .as_ref()
            .and_then(|v| v.record(name))
            .is_some_and(|r| r.mode & 0o111 != 0)
    }
    pub(crate) fn prepare_group(&self, group: &str) -> Result<()> {
        self.ensure_group(group).map(|_| ())
    }
    fn ensure_group(&self, group: &str) -> Result<&Path> {
        let bound = self.bind_group(group)?;
        if bound.ready.load(Ordering::Acquire) {
            return Ok(&bound.root);
        }
        let mut lease = bound.lease.lock().unwrap_or_else(|e| e.into_inner());
        if lease.is_none() {
            let guard = preparation_lock(&bound.directory, !bound.sidecar)?;
            guard.lock()?;
            let lock = group_lock(&bound.directory, !bound.sidecar)?;
            lock.lock_shared()?;
            if !self.valid_group(&bound.directory, group)? {
                ensure!(
                    !bound.sidecar,
                    "pre-extracted group changed after its module paths were resolved: {group}"
                );
                lock.unlock()?;
                lock.try_lock()
                    .context("native group is in use; retry after that process exits")?;
                // The preparation gate excludes other writers throughout this
                // check and publication; no second full hash pass is needed.
                self.publish_group(&bound.directory, group)?;
                lock.lock_shared()?;
            }
            *lease = Some(lock);
            bound.ready.store(true, Ordering::Release);
        }
        Ok(&bound.root)
    }
    pub(crate) fn prepare_at(&self, base: &Path) -> Result<()> {
        self.check_platform()?;
        ensure!(
            self.v2.is_some(),
            "install requires a v2 package; repack with dnc"
        );
        for group in &self.manifest.groups {
            if self.v2.as_ref().unwrap().group_indices(group).is_none() {
                continue;
            }
            let directory = self.group_directory(base, group)?;
            let guard = preparation_lock(&directory, true)?;
            guard.lock()?;
            let lock = group_lock(&directory, true)?;
            lock.lock_shared()?;
            if !self.valid_group(&directory, group)? {
                lock.unlock()?;
                lock.try_lock()
                    .context("native group is in use or being prepared")?;
                self.publish_group(&directory, group)?;
            }
        }
        Ok(())
    }
    fn publish_group(&self, destination: &Path, group: &str) -> Result<()> {
        let parent = destination.parent().unwrap();
        secure_directory(parent)?;
        let stage = tempfile::Builder::new()
            .prefix(".dnr-stage-")
            .tempdir_in(parent)?;
        let root = stage.path().join("root");
        fs::create_dir(&root)?;
        let receipt = serde_json::to_vec(&self.receipt(group)?)?;
        fs::write(stage.path().join("pending.json"), &receipt)?;
        let records = self.group_records(group)?;
        let reuse = self.reusable_group(group);
        for record in &records {
            let path = root.join(&record.path);
            fs::create_dir_all(path.parent().unwrap())?;
            match record.kind.as_str() {
                "directory" => fs::create_dir_all(&path)?,
                "symlink" => {}
                "file" => {
                    if let Some((source, _lease, _group_lease)) = &reuse
                        && fs::hard_link(source.join("root").join(&record.path), &path).is_ok()
                    {
                        continue;
                    }
                    let mut archive = self.archive.clone();
                    let mut source = archive.by_name(&record.source)?;
                    let mut file = File::create_new(&path)?;
                    let mut hash = Sha256::new();
                    let mut size = 0u64;
                    let mut buf = [0u8; 65536];
                    loop {
                        let n = source.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        size = size.checked_add(n as u64).context("file size overflow")?;
                        ensure!(size <= record.size, "ZIP size mismatch: {}", record.path);
                        hash.update(&buf[..n]);
                        file.write_all(&buf[..n])?;
                    }
                    ensure!(
                        size == record.size
                            && Some(format!("{:x}", hash.finalize()).as_str())
                                == record.sha256.as_deref(),
                        "ZIP checksum mismatch: {}",
                        record.path
                    );
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        file.set_permissions(fs::Permissions::from_mode(record.mode & !0o222))?;
                    }
                    file.sync_all()?;
                }
                _ => bail!("invalid record type"),
            }
        }
        for record in &records {
            if let Some(link) = &record.link {
                std::os::unix::fs::symlink(link, root.join(&record.path))?;
            }
        }
        fs::rename(
            stage.path().join("pending.json"),
            stage.path().join("receipt.json"),
        )?;
        File::open(stage.path().join("receipt.json"))?.sync_all()?;
        if fs::symlink_metadata(destination).is_ok() {
            real_directory(destination)?;
            fs::remove_dir_all(destination)?;
        }
        fs::rename(stage.path(), destination)?;
        File::open(parent)?.sync_all()?;
        self.materialized
            .stats
            .extractions
            .fetch_add(1, Ordering::Relaxed);
        if let Some(generation) = self.generation.get() {
            generation.notify_index();
        }
        Ok(())
    }
    fn reusable_group(&self, group: &str) -> Option<(PathBuf, File, File)> {
        let generation = self.generation.get()?;
        for directory in crate::persistent::catalog::candidates(
            &generation.base,
            &generation.receipt.content_hash,
        ) {
            if directory == generation.directory {
                continue;
            }
            let attempt = || -> Result<(PathBuf, File, File)> {
                ensure!(
                    directory.starts_with(generation.base.join("v3"))
                        && directory.canonicalize()? == directory,
                    "invalid catalog path"
                );
                let root = directory
                    .parent()
                    .and_then(Path::parent)
                    .context("bad generation")?;
                let gate = crate::persistent::lock_file(&root.join(".gate"))?;
                gate.lock()?;
                let lease = crate::persistent::lock_file(
                    &root.join(".leases").join(&generation.receipt.content_hash),
                )?;
                lease.lock_shared()?;
                let group_path = directory
                    .join("native")
                    .join(self.selected_target().unwrap())
                    .join(group);
                let group_lease = group_lock(&group_path, false)?;
                group_lease.lock_shared()?;
                ensure!(
                    self.valid_group(&group_path, group)?,
                    "invalid reusable group"
                );
                Ok((group_path, lease, group_lease))
            };
            if let Ok(value) = attempt() {
                return Some(value);
            }
        }
        None
    }
    pub fn release_native_groups(&self) {
        for slot in self.materialized.slots.values() {
            if let Some(bound) = slot.bound.get() {
                let mut lease = bound.lease.lock().unwrap_or_else(|e| e.into_inner());
                bound.ready.store(false, Ordering::Release);
                lease.take();
            }
        }
    }
}

fn real_directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "not a real directory: {}",
        path.display()
    );
    Ok(())
}
pub(crate) fn secure_directory(path: &Path) -> Result<()> {
    // Reject redirection in the managed tree; the configured cache base itself may be a canonicalized symlink.
    if let Ok(meta) = fs::symlink_metadata(path) {
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "cache path is not a real directory: {}",
            path.display()
        );
        return Ok(());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        secure_directory(parent)?;
    }
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => real_directory(path)?,
        Err(e) => return Err(e.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub(crate) fn group_lock(directory: &Path, create: bool) -> Result<File> {
    let parent = directory
        .parent()
        .context("group needs parent")?
        .join(".locks");
    if create {
        secure_directory(&parent)?;
    } else {
        real_directory(&parent)?;
    }
    let path = parent.join(directory.file_name().context("group needs name")?);
    if create {
        match File::create_new(&path) {
            Ok(f) => return Ok(f),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    let meta = fs::symlink_metadata(&path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "invalid group lock"
    );
    Ok(File::open(path)?)
}

fn preparation_lock(directory: &Path, create: bool) -> Result<File> {
    let parent = directory
        .parent()
        .context("group needs parent")?
        .join(".preparing");
    if create {
        secure_directory(&parent)?;
    } else {
        real_directory(&parent)?;
    }
    let path = parent.join(directory.file_name().context("group needs name")?);
    if create {
        match File::create_new(&path) {
            Ok(f) => return Ok(f),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    let meta = fs::symlink_metadata(&path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "invalid preparation lock"
    );
    Ok(File::open(path)?)
}
