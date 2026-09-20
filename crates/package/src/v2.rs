use crate::{Entry, EntryKind, Manifest, PackageConfig, config};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const INDEX: &str = ".dnr/index.json";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Integrity {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Record {
    pub path: String,
    pub source: String,
    pub kind: String,
    pub size: u64,
    pub mode: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub napi: Option<u32>,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn file_digest(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
pub(crate) fn copy_checked(
    path: &Path,
    output: &mut impl Write,
    expected: Option<&str>,
) -> Result<u64> {
    let mut input = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0; 65536];
    let mut size = 0;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        if expected.is_some() {
            hasher.update(&buf[..n]);
        }
        size += n as u64;
    }
    if let Some(expected) = expected {
        ensure!(
            format!("{:x}", hasher.finalize()) == expected,
            "source changed while packaging: {}",
            path.display()
        );
    }
    Ok(size)
}

pub(crate) struct Prepared {
    pub bytes: Vec<u8>,
    records: BTreeMap<String, Record>,
}
impl Prepared {
    pub fn integrity(&self) -> Integrity {
        Integrity {
            path: INDEX.into(),
            sha256: digest(&self.bytes),
        }
    }
    pub fn hash(&self, source: &str) -> Option<&str> {
        self.records.get(source).and_then(|r| r.sha256.as_deref())
    }
    pub fn mode(&self, source: &str) -> Option<u32> {
        self.records.get(source).map(|r| r.mode)
    }
}

pub(crate) fn prepare(
    config: &PackageConfig,
    entries: &mut BTreeMap<String, PathBuf>,
    excludes: &[String],
    output: &Path,
) -> Result<Prepared> {
    config.validate()?;
    let filters = config
        .groups
        .iter()
        .map(|g| config::patterns(&g.files))
        .collect::<Result<Vec<_>>>()?;
    let mut origins = BTreeMap::new();
    for name in entries.keys() {
        let groups: Vec<_> = filters
            .iter()
            .enumerate()
            .filter(|(_, f)| f.is_match(name))
            .map(|(i, _)| i)
            .collect();
        ensure!(groups.len() <= 1, "file belongs to multiple groups: {name}");
        origins.insert(name.clone(), (name.clone(), groups.first().copied(), None));
    }
    for (gi, group) in config.groups.iter().enumerate() {
        for (target, mappings) in &group.variants {
            let mut variants = BTreeMap::new();
            for mapping in mappings {
                crate::collect(
                    &config.base_dir.join(&mapping.from),
                    &mapping.to,
                    &mut variants,
                    excludes,
                    output,
                )?;
            }
            for (logical, source) in variants {
                let physical = format!(".dnr/payloads/{}_{target}/{logical}", group.id);
                ensure!(
                    entries.insert(physical.clone(), source).is_none(),
                    "duplicate payload: {physical}"
                );
                origins.insert(physical, (logical, Some(gi), Some(target.clone())));
            }
        }
    }
    let mut records = Vec::new();
    let exec = config
        .groups
        .iter()
        .map(|g| config::patterns(&g.native.executables))
        .collect::<Result<Vec<_>>>()?;
    let libs = config
        .groups
        .iter()
        .map(|g| config::patterns(&g.native.libraries))
        .collect::<Result<Vec<_>>>()?;
    for (source, disk) in entries.iter() {
        let (path, gi, target) = &origins[source];
        let meta = fs::symlink_metadata(disk)?;
        let mut record = Record {
            path: path.clone(),
            source: source.clone(),
            kind: "file".into(),
            size: meta.len(),
            mode: 0o644,
            sha256: None,
            link: None,
            group: gi.map(|i| config.groups[i].id.clone()),
            target: target.clone(),
            native: None,
            napi: None,
        };
        if meta.file_type().is_symlink() {
            record.kind = "symlink".into();
            record.mode = 0o777;
            let link = fs::read_link(disk)?
                .to_str()
                .context("non-UTF-8 symlink")?
                .to_owned();
            crate::resolve_link(path, &link)?;
            record.size = link.len() as u64;
            record.sha256 = Some(digest(link.as_bytes()));
            record.link = Some(link);
        } else if meta.is_dir() {
            record.kind = "directory".into();
            record.mode = 0o755;
            record.size = 0;
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 != 0 {
                    record.mode = 0o755;
                }
            }
            record.sha256 = Some(file_digest(disk)?);
            if let Some(i) = gi {
                let native = &config.groups[*i].native;
                let addon = native.addons.iter().find(|a| a.path == *path);
                let matches = usize::from(exec[*i].is_match(path))
                    + usize::from(libs[*i].is_match(path))
                    + usize::from(addon.is_some());
                ensure!(matches <= 1, "conflicting native declarations: {path}");
                if exec[*i].is_match(path) {
                    record.native = Some("executable".into());
                    record.mode = 0o755;
                }
                if libs[*i].is_match(path) {
                    record.native = Some("library".into());
                }
                if let Some(addon) = addon {
                    record.native = Some("addon".into());
                    record.napi = Some(addon.napi);
                }
            }
            if let Some(found) = config::detect(disk)? {
                // JS entrypoints with shebangs remain scripts unless explicitly declared executable.
                let script = matches!(
                    disk.extension().and_then(|s| s.to_str()),
                    Some("js" | "ts" | "mjs" | "cjs" | "jsx" | "tsx")
                ) && found.os.is_none();
                ensure!(
                    script || record.native.is_some(),
                    "undeclared native candidate {path}; run dnc scan and review the package configuration"
                );
                if record.native.is_some() {
                    for t in config
                        .targets
                        .iter()
                        .filter(|(id, _)| target.as_ref().is_none_or(|wanted| wanted == *id))
                        .map(|(_, t)| t)
                    {
                        ensure!(
                            found.os.is_none_or(|os| os == t.os)
                                && found.arch.is_none_or(|arch| arch == t.arch),
                            "binary platform disagrees with declaration: {path} for {}",
                            t.id()
                        );
                    }
                }
            }
        }
        records.push(record);
    }
    for group in &config.groups {
        ensure!(
            records
                .iter()
                .any(|r| r.group.as_ref() == Some(&group.id) && r.native.is_some()),
            "group has no native entrypoint: {}",
            group.id
        );
        for pattern in group
            .native
            .executables
            .iter()
            .chain(&group.native.libraries)
        {
            let filter = config::patterns(std::slice::from_ref(pattern))?;
            ensure!(
                records.iter().any(|r| r.group.as_ref() == Some(&group.id)
                    && r.native.is_some()
                    && filter.is_match(&r.path)),
                "native pattern matches no group files: {pattern}"
            );
        }
        for addon in &group.native.addons {
            ensure!(
                records.iter().any(|r| r.group.as_ref() == Some(&group.id)
                    && r.path == addon.path
                    && r.napi.is_some()),
                "missing addon: {}",
                addon.path
            );
        }
    }
    let bytes = serde_json::to_vec(&records)?;
    Ok(Prepared {
        bytes,
        records: records.into_iter().map(|r| (r.source.clone(), r)).collect(),
    })
}

pub(crate) struct State {
    pub id: String,
    pub target: String,
    pub records: Vec<Record>,
    pub unavailable: HashSet<String>,
    hashes: BTreeMap<usize, String>,
    by_path: HashMap<String, usize>,
    by_group: HashMap<String, Vec<usize>>,
}
impl State {
    pub fn open(
        manifest: &Manifest,
        bytes: &[u8],
        archive: &mut zip::ZipArchive<crate::reader::PackageReader>,
        entries: &BTreeMap<String, Entry>,
        modes: &[u32],
        requested: Option<&str>,
    ) -> Result<(Self, BTreeMap<String, Entry>)> {
        let integrity = manifest
            .integrity
            .as_ref()
            .context("v2 requires integrity index")?;
        ensure!(integrity.path == INDEX, "unknown integrity index");
        let mut source = archive.by_name(INDEX)?;
        // Bound metadata independently of uncompressed application payloads.
        ensure!(
            source.size() <= 64 * 1024 * 1024,
            "oversized integrity index"
        );
        let mut index_bytes = Vec::new();
        source.read_to_end(&mut index_bytes)?;
        drop(source);
        ensure!(
            digest(&index_bytes) == integrity.sha256,
            "integrity index checksum mismatch"
        );
        let records: Vec<Record> = serde_json::from_slice(&index_bytes)?;
        let mut groups = BTreeSet::new();
        for g in &manifest.groups {
            config::identifier(g)?;
            ensure!(groups.insert(g), "duplicate group: {g}");
        }
        let mut definitions = BTreeSet::new();
        for (id, t) in &manifest.targets {
            config::identifier(id)?;
            t.validate()?;
            ensure!(definitions.insert(t.id()), "duplicate platform");
        }
        ensure!(
            groups.is_empty() || !manifest.targets.is_empty(),
            "groups require targets"
        );
        let host = config::Target::host();
        let target = match requested {
            Some(t) => {
                ensure!(manifest.targets.contains_key(t), "unknown target: {t}");
                t.to_owned()
            }
            None => manifest
                .targets
                .iter()
                .find(|(_, t)| **t == host)
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| host.id()),
        };
        let mut hashes = BTreeMap::new();
        let mut sources = HashSet::with_capacity(records.len());
        for record in &records {
            ensure!(
                crate::is_normalized_name(&record.path)
                    && record.path != ".dnr"
                    && !record.path.starts_with(".dnr/"),
                "invalid logical path"
            );
            ensure!(
                sources.insert(&record.source),
                "duplicate index source: {}",
                record.source
            );
            ensure!(
                record.group.as_ref().is_none_or(|g| groups.contains(g)),
                "unknown group"
            );
            ensure!(
                record
                    .target
                    .as_ref()
                    .is_none_or(|t| manifest.targets.contains_key(t))
                    && (record.target.is_none() || record.group.is_some()),
                "invalid file target"
            );
            let original = entries
                .get(&record.source)
                .context("index source missing from ZIP")?;
            ensure!(
                record.size == original.size && record.mode & !0o777 == 0,
                "index size/mode mismatch: {}",
                record.path
            );
            let expected_kind = match &original.kind {
                EntryKind::File => "file",
                EntryKind::Directory => "directory",
                EntryKind::Symlink(link) => {
                    ensure!(record.link.as_ref() == Some(link), "index symlink mismatch");
                    "symlink"
                }
            };
            ensure!(record.kind == expected_kind, "index kind mismatch");
            let archive_mode = modes[original.index];
            ensure!(
                archive_mode == record.mode,
                "index permissions mismatch: {}",
                record.path
            );
            if record.kind != "directory" {
                let hash = record.sha256.as_ref().context("missing file checksum")?;
                ensure!(
                    hash.len() == 64
                        && hash
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                    "invalid checksum"
                );
                if let Some(link) = &record.link {
                    ensure!(
                        digest(link.as_bytes()) == *hash,
                        "symlink checksum mismatch"
                    );
                }
                hashes.insert(original.index, hash.clone());
            }
            if let Some(native) = &record.native {
                ensure!(
                    record.kind == "file"
                        && record.group.is_some()
                        && matches!(native.as_str(), "addon" | "library" | "executable"),
                    "invalid native declaration"
                );
                ensure!(
                    (native == "addon") == record.napi.is_some() && record.napi != Some(0),
                    "invalid Node-API requirement"
                );
                ensure!(
                    native != "executable" || record.mode & 0o111 != 0,
                    "executable has no execute permission"
                );
            } else {
                ensure!(record.napi.is_none(), "unexpected Node-API requirement");
            }
        }
        ensure!(
            entries
                .keys()
                .filter(|k| k.as_str() != INDEX)
                .all(|k| sources.contains(k)),
            "ZIP contains unindexed files"
        );
        // Validate every platform, not just the builder's host.
        let mut current_view = None;
        if manifest.targets.is_empty() {
            current_view = Some(validate_view(&records, entries, &target)?);
        }
        for t in manifest.targets.keys() {
            let view = validate_view(&records, entries, t)?;
            if t == &target {
                current_view = Some(view);
            }
        }
        let view = match current_view {
            Some(view) => view,
            None => make_view(
                &records
                    .iter()
                    .filter(|r| r.target.is_none())
                    .collect::<Vec<_>>(),
                entries,
            )?,
        };
        let unavailable = records
            .iter()
            .filter(|r| {
                if r.target.as_ref().is_none_or(|t| t == &target) || view.contains_key(&r.path) {
                    return false;
                }
                // A current-platform file can synthesize a parent directory
                // that only the other variant explicitly records in its ZIP.
                let prefix = format!("{}/", r.path);
                !view
                    .range(prefix.clone()..)
                    .next()
                    .is_some_and(|(name, _)| name.starts_with(&prefix))
            })
            .map(|r| r.path.clone())
            .collect();
        let selected: Vec<_> = records
            .into_iter()
            .filter(|r| r.target.as_ref().is_none_or(|t| t == &target))
            .collect();
        let mut by_path = HashMap::with_capacity(selected.len());
        let mut by_group: HashMap<String, Vec<usize>> =
            HashMap::with_capacity(manifest.groups.len());
        for (index, record) in selected.iter().enumerate() {
            by_path.entry(record.path.clone()).or_insert(index);
            if let Some(group) = &record.group {
                by_group.entry(group.clone()).or_default().push(index);
            }
        }
        Ok((
            Self {
                id: digest(bytes),
                target,
                hashes,
                records: selected,
                unavailable,
                by_path,
                by_group,
            },
            view,
        ))
    }
    pub fn record(&self, name: &str) -> Option<&Record> {
        self.by_path.get(name).map(|i| &self.records[*i])
    }
    pub fn group(&self, group: &str) -> Vec<&Record> {
        self.by_group
            .get(group)
            .into_iter()
            .flatten()
            .map(|i| &self.records[*i])
            .collect()
    }
    pub fn group_indices(&self, group: &str) -> Option<&[usize]> {
        self.by_group.get(group).map(Vec::as_slice)
    }
    pub fn has_groups(&self) -> bool {
        !self.by_group.is_empty()
    }
    pub fn verify_file(&self, index: usize, path: &Path) -> Result<()> {
        if let Some(expected) = self.hashes.get(&index) {
            ensure!(
                file_digest(path)? == *expected,
                "file SHA-256 mismatch: {}",
                path.display()
            );
        }
        Ok(())
    }
    pub fn verify(&self, index: usize, bytes: &[u8]) -> Result<()> {
        if let Some(expected) = self.hashes.get(&index) {
            ensure!(digest(bytes) == *expected, "file SHA-256 mismatch");
        }
        Ok(())
    }
}

fn make_view(
    records: &[&Record],
    raw: &BTreeMap<String, Entry>,
) -> Result<BTreeMap<String, Entry>> {
    if records.len() + 1 == raw.len() && records.iter().all(|r| r.path == r.source) {
        let mut view = raw.clone();
        view.remove(INDEX);
        return Ok(view);
    }
    let mut view: BTreeMap<String, Entry> = BTreeMap::new();
    for r in records {
        let mut entry = raw[&r.source].clone();
        entry.name = r.path.clone();
        if let Some(old) = view.get(&r.path) {
            ensure!(
                old.kind == EntryKind::Directory && entry.kind == EntryKind::Directory,
                "platform path conflict: {}",
                r.path
            );
        } else {
            view.insert(r.path.clone(), entry);
        }
    }
    Ok(view)
}
fn validate_view(
    records: &[Record],
    raw: &BTreeMap<String, Entry>,
    target: &str,
) -> Result<BTreeMap<String, Entry>> {
    let selected: Vec<_> = records
        .iter()
        .filter(|r| r.target.as_deref().is_none_or(|t| t == target))
        .collect();
    let view = make_view(&selected, raw)?;
    let by_path: HashMap<_, _> = selected.iter().map(|r| (r.path.as_str(), *r)).collect();
    let mut native_groups: HashMap<&str, bool> = HashMap::new();
    for r in &selected {
        if let Some(group) = &r.group {
            *native_groups.entry(group).or_default() |= r.native.is_some();
        }
    }
    let parents: HashSet<_> = view
        .keys()
        .flat_map(|name| name.match_indices('/').map(|(n, _)| &name[..n]))
        .collect();
    for parent in parents {
        if let Some(p) = view.get(parent) {
            ensure!(
                p.kind == EntryKind::Directory,
                "file/directory conflict: {parent}"
            );
        }
    }
    for (name, entry) in &view {
        if let EntryKind::Symlink(_) = &entry.kind {
            let mut next = name.clone();
            let mut done = false;
            for _ in 0..40 {
                let mut replacement = None;
                let parts: Vec<_> = next.split('/').collect();
                for n in 1..=parts.len() {
                    let prefix = parts[..n].join("/");
                    if let Some(Entry {
                        kind: EntryKind::Symlink(link),
                        ..
                    }) = view.get(&prefix)
                    {
                        let dest = crate::resolve_link(&prefix, link)?;
                        replacement = Some(if n == parts.len() {
                            dest
                        } else {
                            format!("{dest}/{}", parts[n..].join("/"))
                        });
                        break;
                    }
                }
                match replacement {
                    Some(value) => next = value,
                    None => {
                        done = true;
                        break;
                    }
                }
            }
            ensure!(done, "symlink loop: {name}");
            let prefix = format!("{next}/");
            ensure!(
                view.contains_key(&next)
                    || view
                        .range(prefix.clone()..)
                        .next()
                        .is_some_and(|(p, _)| p.starts_with(&prefix)),
                "dangling symlink: {name}"
            );
            let owner = by_path.get(name.as_str()).and_then(|r| r.group.as_deref());
            if let Some(owner) = owner {
                let dest_owner = by_path
                    .get(next.as_str())
                    .filter(|r| r.kind != "directory")
                    .and_then(|r| r.group.as_deref());
                let mut children = view
                    .range(prefix.clone()..)
                    .take_while(|(p, _)| p.starts_with(&prefix))
                    .filter(|(_, e)| e.kind != EntryKind::Directory)
                    .peekable();
                let closed = children.peek().is_some()
                    && children.all(|(name, _)| {
                        by_path.get(name.as_str()).and_then(|r| r.group.as_deref()) == Some(owner)
                    });
                ensure!(
                    dest_owner == Some(owner) || closed,
                    "group symlink escapes its group: {name}; merge related files into one group"
                );
            }
        }
    }
    for (group, has_native) in native_groups {
        ensure!(
            has_native,
            "group {group} has no native entrypoint for {target}"
        );
    }
    Ok(view)
}
