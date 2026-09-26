//! DNR's executable ZIP format. No JavaScript engine or platform GUI dependency.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

mod cache;
pub mod commands;
pub mod config;
mod contents;
mod index;
mod materialize;
pub mod metadata;
pub mod persistent;
mod reader;
pub use config::PackageConfig;
pub use materialize::{NativeCacheStats, NativeUse, cache_directory};

pub const FORMAT_VERSION: u32 = 4;
pub const MARKER: &str = "# DNRZIP1\n";
pub const DEFAULT_CACHE_BYTES: usize = 256 * 1024 * 1024;
const MAX_HEADER: usize = 16 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub entry: String,
    pub app_id: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub targets: BTreeMap<String, config::Target>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop: Option<DesktopMetadata>,
}

/// Optional in-memory desktop identity; no files need to be extracted for an icon.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopMetadata {
    pub name: Option<String>,
    pub window_icon: Option<WindowIcon>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WindowIcon {
    pub width: u32,
    pub height: u32,
    /// Straight (unpremultiplied) RGBA8, row-major, with no padding.
    pub rgba: Vec<u8>,
}

impl DesktopMetadata {
    pub fn validate(&self) -> Result<()> {
        if let Some(name) = &self.name {
            ensure!(
                !name.trim().is_empty()
                    && name.len() <= 4096
                    && !name.chars().any(char::is_control),
                "invalid desktop name"
            );
        }
        if let Some(icon) = &self.window_icon {
            ensure!(
                (1..=128).contains(&icon.width) && (1..=128).contains(&icon.height),
                "window icon dimensions must be 1..128"
            );
            ensure!(
                icon.rgba.len() == (icon.width * icon.height * 4) as usize,
                "invalid window icon RGBA length"
            );
        }
        Ok(())
    }
}

pub fn normalized_name(name: &str) -> Result<String> {
    if is_normalized_name(name) {
        return Ok(name.to_owned());
    }
    ensure!(
        !name.is_empty() && !name.contains(['\0', '\n', '\r', '\\']),
        "invalid archive path: {name:?}"
    );
    let mut parts = Vec::new();
    for c in Path::new(name).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str().context("archive paths must be UTF-8")?),
            Component::CurDir => {}
            _ => bail!("archive path must be relative and contained: {name:?}"),
        }
    }
    ensure!(!parts.is_empty(), "empty archive path");
    Ok(parts.join("/"))
}

pub(crate) fn is_normalized_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\0', '\n', '\r', '\\'])
        && name
            .split('/')
            .all(|p| !p.is_empty() && !matches!(p, "." | ".."))
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

pub fn resolve_link(name: &str, target: &str) -> Result<String> {
    ensure!(
        !target.contains(['\0', '\n', '\r', '\\']) && !Path::new(target).is_absolute(),
        "invalid symlink: {name} -> {target}"
    );
    let mut parts: Vec<&str> = name.split('/').collect();
    parts.pop();
    for c in Path::new(target).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str().context("non-UTF-8 symlink")?),
            Component::CurDir => {}
            Component::ParentDir => {
                ensure!(parts.pop().is_some(), "symlink escapes archive: {name}");
            }
            _ => bail!("invalid symlink: {name}"),
        }
    }
    Ok(parts.join("/"))
}

#[derive(Debug, Clone)]
pub struct Include {
    pub source: PathBuf,
    pub destination: String,
}
impl Include {
    pub fn parse(value: &str) -> Result<Self> {
        let (source, destination) = match value.split_once('=') {
            Some((s, d)) => (PathBuf::from(s), d.to_owned()),
            None => {
                let p = PathBuf::from(value);
                let d = p
                    .file_name()
                    .context("include needs a name")?
                    .to_str()
                    .context("non-UTF-8 include")?
                    .to_owned();
                (p, d)
            }
        };
        Ok(Self {
            source,
            destination: normalized_name(&destination)?,
        })
    }
}
pub struct PackOptions {
    pub directory: PathBuf,
    pub entry: String,
    pub output: PathBuf,
    pub includes: Vec<Include>,
    pub excludes: Vec<String>,
    pub app_id: Option<String>,
    pub force: bool,
}
pub struct PackReport {
    pub files: usize,
    pub source_bytes: u64,
    pub package_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink(String),
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub index: usize,
    pub kind: EntryKind,
}

fn excluded(name: &str, excludes: &[String]) -> bool {
    name.split('/').any(|p| p == ".git")
        || excludes
            .iter()
            .any(|p| name == p || name.strip_prefix(p).is_some_and(|s| s.starts_with('/')))
}

fn collect(
    source: &Path,
    name: &str,
    entries: &mut BTreeMap<String, PathBuf>,
    excludes: &[String],
    output: &Path,
) -> Result<()> {
    if excluded(name, excludes) {
        return Ok(());
    }
    ensure!(
        name != ".dnr" && !name.starts_with(".dnr/"),
        ".dnr is reserved for package metadata"
    );
    let meta =
        fs::symlink_metadata(source).with_context(|| format!("reading {}", source.display()))?;
    if !meta.file_type().is_symlink() && fs::canonicalize(source).ok().as_deref() == Some(output) {
        return Ok(());
    }
    if !name.is_empty() {
        ensure!(
            entries
                .insert(normalized_name(name)?, source.to_owned())
                .is_none(),
            "duplicate destination: {name}"
        );
    }
    if meta.is_dir() {
        let mut children = fs::read_dir(source)?.collect::<io::Result<Vec<_>>>()?;
        children.sort_by_key(|e| e.file_name());
        for e in children {
            let child = e
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("non-UTF-8 filename"))?;
            collect(
                &e.path(),
                &if name.is_empty() {
                    child
                } else {
                    format!("{name}/{child}")
                },
                entries,
                excludes,
                output,
            )?;
        }
    } else {
        ensure!(
            meta.is_file() || meta.file_type().is_symlink(),
            "unsupported special file: {}",
            source.display()
        );
    }
    Ok(())
}

fn identity(options: &PackOptions) -> String {
    if let Some(id) = &options.app_id {
        return id.clone();
    }
    for name in ["deno.json", "package.json"] {
        if let Ok(bytes) = fs::read(options.directory.join(name))
            && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Some(name) = json.get("name").and_then(|n| n.as_str())
        {
            return name.to_owned();
        }
    }
    options
        .directory
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "application".into())
}

/// Build a v4 package without native groups.
pub fn pack(options: &PackOptions) -> Result<PackReport> {
    pack_with_config(options, &PackageConfig::default())
}

/// Build a v4 package using a reviewed native-group configuration.
pub fn pack_with_config(options: &PackOptions, config: &PackageConfig) -> Result<PackReport> {
    pack_with_desktop(options, config, None)
}

pub fn pack_with_desktop(
    options: &PackOptions,
    config: &PackageConfig,
    desktop: Option<DesktopMetadata>,
) -> Result<PackReport> {
    if let Some(desktop) = &desktop {
        desktop.validate()?;
    }
    ensure!(options.directory.is_dir(), "input must be a directory");
    let entry = normalized_name(&options.entry)?;
    ensure!(!entry.starts_with(".dnr/"), "reserved entrypoint");
    let excludes = options
        .excludes
        .iter()
        .map(|s| normalized_name(s))
        .collect::<Result<Vec<_>>>()?;
    let parent = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let output = parent.canonicalize()?.join(
        options
            .output
            .file_name()
            .context("output needs a filename")?,
    );
    ensure!(
        options.force || !output.exists(),
        "output exists; use --force: {}",
        output.display()
    );
    let mut entries = BTreeMap::new();
    collect(&options.directory, "", &mut entries, &excludes, &output)?;
    for include in &options.includes {
        collect(
            &include.source,
            &include.destination,
            &mut entries,
            &excludes,
            &output,
        )?;
    }
    let mut prepared = index::prepare(config, &mut entries, &excludes, &output)?;
    {
        let mut records = BTreeMap::new();
        let linked_groups: std::collections::HashSet<_> = prepared
            .records
            .values()
            .filter(|r| r.kind == "symlink")
            .filter_map(|r| r.group.clone())
            .collect();
        let mut group_roots: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for record in prepared.records.values() {
            if let Some(group) = record.group.as_ref().filter(|g| linked_groups.contains(*g)) {
                let mut paths = vec![record.path.clone()];
                if let Some(link) = &record.link {
                    paths.push(resolve_link(&record.path, link)?);
                }
                for path in paths {
                    let components: Vec<String> = path
                        .split('/')
                        .filter(|p| !p.is_empty())
                        .map(str::to_owned)
                        .collect();
                    group_roots
                        .entry(group.clone())
                        .and_modify(|root| {
                            let shared = root
                                .iter()
                                .zip(&components)
                                .take_while(|(a, b)| a == b)
                                .count();
                            root.truncate(shared);
                        })
                        .or_insert(components);
                }
            }
        }
        let mut containers = BTreeMap::new();
        for (n, (old, mut record)) in std::mem::take(&mut prepared.records)
            .into_iter()
            .enumerate()
        {
            if old.starts_with(".dnr/payloads/") {
                // Relative links impose a real layout constraint. Keep these groups together
                // so physical extraction cannot turn a contained logical link into an escape.
                let name = if record
                    .group
                    .as_ref()
                    .is_some_and(|g| linked_groups.contains(g))
                {
                    let container = containers
                        .entry((record.group.clone(), record.target.clone()))
                        .or_insert(n);
                    let root = group_roots[record.group.as_ref().unwrap()].join("/");
                    let suffix = if record.path == root {
                        ""
                    } else if root.is_empty() {
                        &record.path
                    } else {
                        record.path.strip_prefix(&format!("{root}/")).unwrap()
                    };
                    if suffix.is_empty() {
                        format!(".dnr/p/{container:06}")
                    } else {
                        format!(".dnr/p/{container:06}/{suffix}")
                    }
                } else {
                    format!(".dnr/p/{n:06}-{}", record.path.rsplit('/').next().unwrap())
                };
                let source = entries.remove(&old).unwrap();
                entries.insert(name.clone(), source);
                record.source = name;
            }
            records.insert(record.source.clone(), record);
        }
        prepared.records = records;
    }
    let entry_path = entries
        .get(&entry)
        .context("entrypoint is missing or excluded")?;
    ensure!(entry_path.is_file(), "entrypoint must resolve to a file");
    let app_id = identity(options);
    ensure!(
        !app_id.trim().is_empty() && !app_id.chars().any(char::is_control),
        "invalid app id"
    );
    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        entry: entry.clone(),
        app_id,
        targets: config.targets.clone(),
        groups: config.groups.iter().map(|g| g.id.clone()).collect(),
        desktop,
    };
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let quoted_entry = entry.replace('\'', "'\\''");
    write!(
        temporary,
        "#!/bin/sh\nexec dnr --package \"$0\" --entry '{quoted_entry}' -- \"$@\"\nexit 127\n{MARKER}"
    )?;
    let offset = temporary.stream_position()?;
    let writer = Region::new(temporary.as_file_mut(), offset)?;
    let mut zip = ZipWriter::new(writer);
    let file_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Zstd)
        .compression_level(Some(6))
        .unix_permissions(0o644);
    zip.start_file(
        metadata::META,
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644),
    )?;
    let records = prepared.records.values().cloned().collect::<Vec<_>>();
    zip.write_all(&metadata::encode(&manifest, &records)?)?;
    let mut source_bytes = 0;
    for (name, source) in &entries {
        let meta = fs::symlink_metadata(source)?;
        if meta.file_type().is_symlink() {
            let target = fs::read_link(source)?
                .to_str()
                .context("non-UTF-8 symlink")?
                .to_owned();
            resolve_link(name, &target)?;
            zip.add_symlink(name, target, SimpleFileOptions::default())?;
        } else if meta.is_dir() {
            zip.add_directory(
                format!("{name}/"),
                SimpleFileOptions::default().unix_permissions(0o755),
            )?;
        } else {
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 != 0 {
                    0o755
                } else {
                    0o644
                }
            };
            #[cfg(not(unix))]
            let mode = 0o644;
            let mode = prepared.mode(name).unwrap_or(mode);
            zip.start_file(name, file_options.unix_permissions(mode))?;
            source_bytes += index::copy_checked(source, &mut zip, prepared.hash(name))?;
        }
    }
    zip.finish()?;
    temporary.as_file().sync_all()?;
    // Validate the exact bytes before publishing, including symlink resolution.
    let check = Package::open(temporary.path(), 0)?;
    let resolved_entry = check.resolve(&entry)?;
    ensure!(
        check
            .entries
            .get(&resolved_entry)
            .is_some_and(|e| e.kind == EntryKind::File),
        "entrypoint must resolve to a file inside the ZIP"
    );
    for item in check.entries.values() {
        if matches!(item.kind, EntryKind::Symlink(_)) {
            check.resolve(&item.name)?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o755))?;
    }
    let package_bytes = temporary.as_file().metadata()?.len();
    if options.force {
        temporary.persist(&output)?;
    } else {
        temporary.persist_noclobber(&output)?;
    }
    Ok(PackReport {
        files: entries.len(),
        source_bytes,
        package_bytes,
    })
}

/// Seekable view whose offsets start immediately after the shell header.
pub struct Region<T> {
    inner: T,
    base: u64,
}
impl<T: Seek> Region<T> {
    pub fn new(mut inner: T, base: u64) -> io::Result<Self> {
        inner.seek(SeekFrom::Start(base))?;
        Ok(Self { inner, base })
    }
}
impl<T: Read> Read for Region<T> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.inner.read(b)
    }
}
impl<T: Write> Write for Region<T> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.inner.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<T: Seek> Seek for Region<T> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let pos = match pos {
            SeekFrom::Start(n) => SeekFrom::Start(
                self.base
                    .checked_add(n)
                    .ok_or_else(|| io::Error::other("ZIP offset overflow"))?,
            ),
            other => other,
        };
        let absolute = self.inner.seek(pos)?;
        absolute
            .checked_sub(self.base)
            .ok_or_else(|| io::Error::other("seek before ZIP start"))
    }
}

pub fn zip_offset(file: &mut File) -> Result<u64> {
    file.seek(SeekFrom::Start(0))?;
    let mut reader = BufReader::new(file.take(MAX_HEADER as u64));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    ensure!(line == "#!/bin/sh\n", "not a DNR application package");
    let mut offset = line.len() as u64;
    loop {
        line.clear();
        ensure!(
            reader.read_line(&mut line)? != 0,
            "missing DNRZIP1 header marker"
        );
        offset += line.len() as u64;
        if line == MARKER {
            return Ok(offset);
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct CacheStats {
    pub decompressions: u64,
    pub hits: u64,
    pub resident_bytes: usize,
    /// Calls that joined an already running load (not resident cache hits).
    pub coalesced: u64,
}
pub struct Package {
    pub manifest: Manifest,
    pub entries: BTreeMap<String, Entry>,
    pub root: PathBuf,
    archive: ZipArchive<reader::PackageReader>,
    cache: cache::Cache,
    index: index::State,
    materialized: materialize::Groups,
    raw_entries: BTreeMap<String, Entry>,
    pub source_path: PathBuf,
    metadata_bytes: Vec<u8>,
    source_stamp: persistent::SourceStamp,
    generation: std::sync::OnceLock<Arc<persistent::Generation>>,
}
impl std::fmt::Debug for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Package")
            .field("root", &self.root)
            .field("manifest", &self.manifest)
            .finish()
    }
}
impl Package {
    pub fn open(path: &Path, budget: usize) -> Result<Self> {
        Self::open_for_target(path, budget, None)
    }

    pub fn open_for_target(path: &Path, budget: usize, target: Option<&str>) -> Result<Self> {
        let path = path.canonicalize()?;
        let mut file = File::open(&path)?;
        let source_stamp = persistent::SourceStamp::from_metadata(&file.metadata()?);
        let offset = zip_offset(&mut file)?;
        let manifest_bytes = metadata::read_prefix(&mut file, offset)?.context(
            "unsupported DNP format: only v4 is supported; repack with the matching dnc",
        )?;
        let metadata = metadata::decode(&manifest_bytes)?;
        let manifest = metadata.manifest.clone();
        let mut archive = ZipArchive::new(reader::PackageReader::new(file, offset)?)?;
        {
            let entry = archive.by_index(0)?;
            ensure!(
                entry.name() == metadata::META
                    && entry.compression() == CompressionMethod::Stored
                    && entry.size() == manifest_bytes.len() as u64
                    && entry.crc32() == crc32fast::hash(&manifest_bytes),
                "metadata local/central header mismatch"
            );
        }
        ensure!(
            normalized_name(&manifest.entry)? == manifest.entry
                && !manifest.entry.starts_with(".dnr/"),
            "invalid entrypoint"
        );
        ensure!(
            !manifest.app_id.trim().is_empty() && !manifest.app_id.chars().any(char::is_control),
            "invalid app id"
        );
        let mut entries = BTreeMap::new();
        let mut seen = std::collections::HashSet::new();
        let mut sizes = Vec::with_capacity(archive.len());
        let mut modes = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let mut file = archive.by_index(index)?;
            sizes.push(file.size());
            modes.push(file.unix_mode().unwrap_or(0o644) & 0o777);
            let name = normalized_name(file.name().trim_end_matches('/'))?;
            ensure!(seen.insert(name.clone()), "duplicate archive path: {name}");
            if name == metadata::META {
                continue;
            }
            ensure!(
                !name.starts_with(".dnr/") || name.starts_with(".dnr/p/"),
                "unknown reserved entry: {name}"
            );
            let kind = if file.is_dir() {
                EntryKind::Directory
            } else if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                ensure!(file.size() <= 4096, "oversized symlink");
                let mut target = String::new();
                file.read_to_string(&mut target)?;
                resolve_link(&name, &target)?;
                EntryKind::Symlink(target)
            } else {
                EntryKind::File
            };
            entries.insert(
                name.clone(),
                Entry {
                    name,
                    size: file.size(),
                    index,
                    kind,
                },
            );
        }
        let (state, view) = index::State::from_records(
            &manifest,
            metadata.content_hash,
            metadata.records,
            &entries,
            &modes,
            target,
        )?;
        let raw_entries = entries;
        let mut entries = view;
        // Check each distinct parent once, not once per child. Borrow validated
        // slash-delimited names; allocate only directories missing from the ZIP.
        let missing = {
            let parents: std::collections::HashSet<_> = entries
                .keys()
                .flat_map(|name| name.match_indices('/').map(|(n, _)| &name[..n]))
                .collect();
            let mut missing = Vec::new();
            for parent in parents {
                if let Some(entry) = entries.get(parent) {
                    ensure!(
                        entry.kind == EntryKind::Directory,
                        "file/directory conflict: {parent}"
                    );
                } else {
                    missing.push(parent.to_owned());
                }
            }
            missing
        };
        for name in missing {
            entries.insert(
                name.clone(),
                Entry {
                    name,
                    size: 0,
                    index: usize::MAX,
                    kind: EntryKind::Directory,
                },
            );
        }
        ensure!(
            entries.contains_key(&manifest.entry),
            "entrypoint missing from ZIP"
        );
        let materialized = materialize::Groups::new(&manifest, &state, &path);
        Ok(Self {
            manifest,
            entries,
            root: path.parent().unwrap().to_owned(),
            archive,
            cache: cache::Cache::new(sizes, budget),
            index: state,
            materialized,
            raw_entries,
            source_path: path,
            metadata_bytes: manifest_bytes,
            source_stamp,
            generation: Default::default(),
        })
    }

    pub fn resolve(&self, name: &str) -> Result<String> {
        let mut name = normalized_name(name)?;
        for _ in 0..40 {
            let parts = name.split('/').collect::<Vec<_>>();
            let mut replacement = None;
            for n in 1..=parts.len() {
                let prefix = parts[..n].join("/");
                if let Some(entry) = self.entries.get(&prefix) {
                    if let EntryKind::Symlink(target) = &entry.kind {
                        let resolved = resolve_link(&prefix, target)?;
                        replacement = Some(if n == parts.len() {
                            resolved
                        } else {
                            format!("{resolved}/{}", parts[n..].join("/"))
                        });
                        break;
                    }
                    ensure!(
                        n == parts.len() || entry.kind == EntryKind::Directory,
                        "not a directory: {prefix}"
                    );
                }
            }
            match replacement {
                Some(next) => name = next,
                None => return Ok(name),
            }
        }
        bail!("symlink loop: {name}")
    }

    pub fn read(&self, name: &str) -> Result<Option<Arc<[u8]>>> {
        let resolved = self.resolve(name)?;
        ensure!(
            !self.is_unavailable(&resolved),
            "file is not available for the selected platform: {name}"
        );
        let Some(entry) = self.entries.get(&resolved) else {
            return Ok(None);
        };
        ensure!(entry.kind == EntryKind::File, "not a regular file: {name}");
        self.read_index(entry.index).map(Some)
    }

    pub fn read_index(&self, index: usize) -> Result<Arc<[u8]>> {
        self.cache.read(index, || {
            // Clones share immutable ZIP metadata and the open file, but each
            // reader has its own logical cursor and each entry its own decoder.
            let mut archive = self.archive.clone();
            let mut source = archive.by_index(index)?;
            let size = usize::try_from(source.size()).context("file too large")?;
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(size)?;
            source
                .by_ref()
                .take((size as u64).saturating_add(1))
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() == size, "ZIP size mismatch");
            // Read once beyond declared size so the decoder validates EOF/CRC.
            ensure!(source.read(&mut [0u8; 1])? == 0, "ZIP size mismatch");
            self.index.verify(index, &bytes)?;
            Ok(bytes.into())
        })
    }
    pub fn finish_cache(&self) {
        if let Some(generation) = self.generation.get() {
            generation.finish();
        }
    }
    pub fn cache_generation(&self) -> Result<Arc<persistent::Generation>> {
        if let Some(generation) = self.generation.get() {
            return Ok(generation.clone());
        }
        let base = self.materialized.cache_base()?;
        let value = persistent::Generation::open(
            &base,
            &self.source_path,
            self.package_id(),
            &self.manifest.app_id,
            &self.source_stamp,
        )?;
        let _ = self.generation.set(value);
        Ok(self.generation.get().unwrap().clone())
    }
    pub fn inspect(&self) -> Result<serde_json::Value> {
        let records = serde_json::to_value(metadata::decode(&self.metadata_bytes)?.records)?;
        let mut manifest = serde_json::to_value(&self.manifest)?;
        if let Some(icon) = self
            .manifest
            .desktop
            .as_ref()
            .and_then(|d| d.window_icon.as_ref())
        {
            use sha2::{Digest, Sha256};
            manifest["desktop"]["windowIcon"] = serde_json::json!({
                "width": icon.width, "height": icon.height, "format": "rgba8",
                "bytes": icon.rgba.len(), "sha256": format!("{:x}", Sha256::digest(&icon.rgba)),
            });
        }
        Ok(
            serde_json::json!({"manifest": manifest, "records": records, "archiveEntries": self.archive_names()?, "contentHash": self.package_id(), "packageId": self.package_id()}),
        )
    }
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }
}
