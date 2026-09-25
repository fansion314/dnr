//! Explicit archive inspection/export, independent of the runtime VFS overlay.
use crate::{Entry, EntryKind, Package};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fs, io, path::Path};

impl Package {
    pub fn archive_names(&self) -> Result<Vec<String>> {
        Ok(self
            .contents()?
            .into_values()
            .map(|entry| {
                if entry.kind == EntryKind::Directory {
                    format!("{}/", entry.name)
                } else {
                    entry.name
                }
            })
            .collect())
    }

    pub fn read_archive_file(&self, name: &str) -> Result<std::sync::Arc<[u8]>> {
        let mut archive = self.archive.clone();
        let source = archive.by_name(name)?;
        ensure!(!source.is_dir(), "not an archive file");
        drop(source);
        let index = archive
            .index_for_name(name)
            .context("missing archive file")?;
        self.read_index(index)
    }
    // The runtime index intentionally hides metadata; archive tools include it.
    fn contents(&self) -> Result<BTreeMap<String, Entry>> {
        let mut entries = self.raw_entries.clone();
        for name in entries.keys().cloned().collect::<Vec<_>>() {
            for parent in Path::new(&name)
                .ancestors()
                .skip(1)
                .filter(|p| !p.as_os_str().is_empty())
            {
                let parent = parent.to_str().unwrap();
                entries.entry(parent.into()).or_insert(Entry {
                    name: parent.into(),
                    kind: EntryKind::Directory,
                    index: usize::MAX,
                    size: 0,
                });
            }
        }
        ensure!(
            entries
                .get(".dnr")
                .is_none_or(|e| e.kind == EntryKind::Directory),
            "file/directory conflict: .dnr"
        );
        entries.entry(".dnr".into()).or_insert(Entry {
            name: ".dnr".into(),
            kind: EntryKind::Directory,
            index: usize::MAX,
            size: 0,
        });
        let mut archive = self.archive.clone();
        let metadata_name = crate::v3::META;
        let index = archive
            .index_for_name(metadata_name)
            .context("missing manifest")?;
        let file = archive.by_index(index)?;
        entries.insert(
            metadata_name.into(),
            Entry {
                name: metadata_name.into(),
                kind: EntryKind::File,
                index,
                size: file.size(),
            },
        );
        Ok(entries)
    }

    pub(crate) fn install_full(&self, destination: &Path, force: bool) -> Result<()> {
        self.check_platform()?;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let parent = parent.canonicalize()?;
        let destination = parent.join(destination.file_name().context("directory needs a name")?);
        if destination.exists() {
            let meta = fs::symlink_metadata(&destination)?;
            ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "invalid install directory"
            );
            ensure!(
                fs::read_dir(&destination)?.next().is_none()
                    || (force && crate::v3::installed_manifest(&destination).is_ok()),
                "full install needs an empty directory or --force on an existing full installation"
            );
        }
        let install_lock = destination.join(crate::v3::INSTALL_LOCK);
        let lease = crate::persistent::lock_file(&install_lock)?;
        lease
            .try_lock()
            .context("full installation is in use; retry after it exits")?;
        let stage = tempfile::Builder::new()
            .prefix(".dnr-install-")
            .tempdir_in(&parent)?;
        for entry in self.entries.values() {
            let path = stage.path().join(&entry.name);
            fs::create_dir_all(path.parent().unwrap())?;
            match &entry.kind {
                EntryKind::Directory => fs::create_dir_all(path)?,
                EntryKind::Symlink(_) => {}
                EntryKind::File => {
                    let bytes = self.read_index(entry.index)?;
                    use std::io::Write;
                    let mut output = fs::File::create_new(&path)?;
                    output.write_all(&bytes)?;
                    use std::os::unix::fs::PermissionsExt;
                    let mut archive = self.archive.clone();
                    let mode = archive.by_index(entry.index)?.unix_mode().unwrap_or(0o644) & 0o777;
                    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
                }
            }
        }
        for entry in self.entries.values() {
            if let EntryKind::Symlink(link) = &entry.kind {
                std::os::unix::fs::symlink(link, stage.path().join(&entry.name))?;
            }
        }
        fs::create_dir_all(stage.path().join(".dnr"))?;
        fs::write(
            stage.path().join(crate::v3::INSTALL),
            crate::v3::installation(&self.metadata_bytes, self.selected_target())?,
        )?;
        fs::hard_link(&install_lock, stage.path().join(crate::v3::INSTALL_LOCK))?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(stage.path(), fs::Permissions::from_mode(0o755))?;
        let backup = tempfile::Builder::new()
            .prefix(".dnr-old-install-")
            .tempdir_in(&parent)?;
        let old = backup.path().join("old");
        if destination.exists() {
            fs::rename(&destination, &old)?;
        }
        if let Err(e) = fs::rename(stage.path(), &destination) {
            if old.exists() {
                fs::rename(&old, &destination)?;
            }
            return Err(e.into());
        }
        Ok(())
    }

    /// Print the ZIP tree, including metadata, without reading ordinary file bodies.
    /// Symlinks are displayed as links and never traversed; disk fallback is not used.
    pub fn write_tree(&self, output: &mut impl io::Write) -> Result<()> {
        let entries = self.contents()?;
        let mut children: BTreeMap<&str, Vec<&Entry>> = BTreeMap::new();
        for entry in entries.values() {
            let parent = entry.name.rsplit_once('/').map_or("", |(p, _)| p);
            children.entry(parent).or_default().push(entry);
        }
        writeln!(output, ".")?;
        let roots = children.remove("").unwrap_or_default();
        let len = roots.len();
        // An explicit traversal stack also handles deeply nested archives.
        let mut nodes = roots
            .into_iter()
            .enumerate()
            .rev()
            .map(|(i, e)| (e, String::new(), i + 1 == len))
            .collect::<Vec<_>>();
        while let Some((entry, prefix, last)) = nodes.pop() {
            let basename = entry.name.rsplit('/').next().unwrap();
            write!(
                output,
                "{prefix}{}{}",
                if last { "└── " } else { "├── " },
                visible(basename)
            )?;
            match &entry.kind {
                EntryKind::Directory => write!(output, "/")?,
                EntryKind::Symlink(target) => write!(output, " -> {}", visible(target))?,
                EntryKind::File => {}
            }
            writeln!(output)?;
            if let Some(items) = children.remove(entry.name.as_str()) {
                let prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
                let len = items.len();
                nodes.extend(
                    items
                        .into_iter()
                        .enumerate()
                        .rev()
                        .map(|(i, e)| (e, prefix.clone(), i + 1 == len)),
                );
            }
        }
        Ok(())
    }

    /// Extract the complete ZIP into a new or empty directory. Files are streamed
    /// and CRC checked in private staging before publication. Existing content
    /// is never merged/overwritten, and symlinks are created after regular files.
    pub fn extract_to(&self, destination: &Path) -> Result<()> {
        use io::Read;
        let entries = self.contents()?;
        let name = destination
            .file_name()
            .context("destination must name a directory")?;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let parent = parent.canonicalize()?;
        let destination = parent.join(name);
        match fs::symlink_metadata(&destination) {
            Ok(meta) => {
                ensure!(
                    meta.is_dir() && !meta.file_type().is_symlink(),
                    "destination must be a real directory"
                );
                ensure!(
                    fs::read_dir(&destination)?.next().is_none(),
                    "destination directory is not empty"
                );
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let staging = tempfile::Builder::new()
            .prefix(".dnr-extract-")
            .tempdir_in(&parent)?;
        let mut archive = self.archive.clone();
        for entry in entries.values() {
            let path = staging.path().join(&entry.name);
            match &entry.kind {
                EntryKind::Directory => fs::create_dir(&path)?,
                EntryKind::Symlink(_) => continue,
                EntryKind::File => {
                    let mut source = archive.by_index(entry.index)?;
                    let mut output = fs::File::create_new(&path)?;
                    let size = io::copy(
                        &mut source.by_ref().take(entry.size.saturating_add(1)),
                        &mut output,
                    )
                    .with_context(|| format!("extracting {}", entry.name))?;
                    ensure!(
                        size == entry.size && source.read(&mut [0u8; 1])? == 0,
                        "ZIP size mismatch: {}",
                        entry.name
                    );
                    self.index.verify_file(entry.index, &path)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        output.set_permissions(fs::Permissions::from_mode(
                            source.unix_mode().unwrap_or(0o644) & 0o777,
                        ))?;
                    }
                }
            }
        }
        for entry in entries.values() {
            if let EntryKind::Symlink(target) = &entry.kind {
                #[cfg(unix)]
                std::os::unix::fs::symlink(target, staging.path().join(&entry.name))?;
                #[cfg(not(unix))]
                anyhow::bail!(
                    "symlink extraction requires Unix: {} -> {target}",
                    entry.name
                );
            }
        }
        #[cfg(unix)]
        for entry in entries.values().rev() {
            use std::os::unix::fs::PermissionsExt;
            if entry.kind == EntryKind::Directory {
                let mode = if entry.index == usize::MAX {
                    0o755
                } else {
                    archive.by_index(entry.index)?.unix_mode().unwrap_or(0o755) & 0o777
                };
                fs::set_permissions(
                    staging.path().join(&entry.name),
                    fs::Permissions::from_mode(mode),
                )?;
            }
        }
        fs::rename(staging.path(), &destination).context("publishing extracted directory")?;
        Ok(())
    }
}

fn visible(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
