//! Explicit archive inspection/export, independent of the runtime VFS overlay.
use crate::{Entry, EntryKind, MANIFEST, Package};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fs, io, path::Path};

impl Package {
    // The runtime index intentionally hides metadata; archive tools include it.
    fn contents(&self) -> Result<BTreeMap<String, Entry>> {
        let mut entries = self.entries.clone();
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
        let index = archive
            .index_for_name(MANIFEST)
            .context("missing manifest")?;
        let file = archive.by_index(index)?;
        entries.insert(
            MANIFEST.into(),
            Entry {
                name: MANIFEST.into(),
                kind: EntryKind::File,
                index,
                size: file.size(),
            },
        );
        for entry in entries.values() {
            if matches!(entry.kind, EntryKind::Symlink(_)) {
                self.resolve(&entry.name)?;
            }
        }
        Ok(entries)
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
