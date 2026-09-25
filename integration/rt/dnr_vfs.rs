//! dnr ZIP overlay helpers, compiled as a child of Deno's file_system module.
//! Keep host policy here; upstream trait methods only dispatch at their boundaries.
use super::*;

impl DenoRtSys {
    pub fn is_dnr_overlay(&self) -> bool {
        self.vfs.dnr_overlay
    }
    pub(super) fn dnr_checked_path(&self, path: CheckedPathBuf) -> CheckedPathBuf {
        match self.vfs.dnr_path(&path) {
            Cow::Borrowed(_) => path,
            Cow::Owned(path) => CheckedPathBuf::unsafe_new(path),
        }
    }
}

impl FileBackedVfs {
    pub(super) fn dnr_path<'a>(&self, path: &'a Path) -> Cow<'a, Path> {
        match &self.dnr_package {
            Some(package) => package.virtual_path_cow(path),
            None => Cow::Borrowed(path),
        }
    }

    pub(super) fn dnr_contains(&self, path: &Path) -> bool {
        if self
            .dnr_package
            .as_ref()
            .is_some_and(|p| p.is_unavailable_path(path))
        {
            return true;
        }
        if path == self.root() {
            return self.dnr_package.is_some();
        }
        let mut current = Some(path);
        while let Some(p) = current {
            if !p.starts_with(self.root()) || p == self.root() {
                break;
            }
            match self.fs_root.find_entry_no_follow(p, self.case_sensitivity) {
                Ok((_, VfsEntryRef::Dir(_))) if p != path => {}
                Ok(_) => return true,
                Err(e) if e.kind() != ErrorKind::NotFound => return true,
                _ => {}
            }
            current = p.parent();
        }
        false
    }

    pub(super) fn dnr_read_dir(
        &self,
        path: &Path,
        dir: &VirtualDirectory,
    ) -> std::io::Result<Vec<FsDirEntry>> {
        let mut entries: Vec<FsDirEntry> = dir
            .entries
            .iter()
            .map(|entry| FsDirEntry {
                name: entry.name().to_string(),
                is_file: matches!(entry, VfsEntry::File(_)),
                is_directory: matches!(entry, VfsEntry::Dir(_)),
                is_symlink: matches!(entry, VfsEntry::Symlink(_)),
            })
            .collect();
        let mut names = entries
            .iter()
            .map(|e| e.name.clone())
            .collect::<std::collections::HashSet<_>>();
        match std::fs::read_dir(path) {
            Ok(disk) => {
                for child in disk {
                    let child = child?;
                    let name = child.file_name().to_string_lossy().into_owned();
                    if !names.insert(name.clone()) {
                        continue;
                    }
                    let kind = child.file_type()?;
                    entries.push(FsDirEntry {
                        name,
                        is_file: kind.is_file(),
                        is_directory: kind.is_dir(),
                        is_symlink: kind.is_symlink(),
                    });
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {}
            Err(e) => return Err(e),
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }
}

impl FileBackedVfsFile {
    pub(super) fn dnr_bytes(&self) -> std::io::Result<Option<Arc<[u8]>>> {
        let Some(package) = &self.vfs.dnr_package else {
            return Ok(None);
        };
        if let Some(bytes) = self.dnr_bytes.borrow().as_ref() {
            return Ok(Some(bytes.clone()));
        }
        let bytes = package
            .read_index(self.file.offset.offset as usize)
            .map_err(std::io::Error::other)?;
        *self.dnr_bytes.borrow_mut() = Some(bytes.clone());
        Ok(Some(bytes))
    }
    pub(super) fn read_position(&self, position: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(bytes) = self.dnr_bytes()? {
            let start = usize::try_from(position)
                .map_err(std::io::Error::other)?
                .min(bytes.len());
            let count = buf.len().min(bytes.len() - start);
            buf[..count].copy_from_slice(&bytes[start..start + count]);
            return Ok(count);
        }
        self.vfs.read_file(&self.file, position, buf)
    }
}

pub(super) struct DnrNativeLoader(pub(super) Arc<FileBackedVfs>);
impl DenoRtNativeAddonLoader for DnrNativeLoader {
    fn load_if_in_vfs(&self, _: &Path) -> Option<Cow<'static, [u8]>> {
        None
    }
    fn load_and_resolve_path<'a>(
        &self,
        path: &'a Path,
    ) -> Result<Cow<'a, Path>, denort_helper::LoadError> {
        if let Some(package) = &self.0.dnr_package {
            if let Some(relative) = package.logical_path(path) {
                if package
                    .required_napi(&relative)
                    .is_some_and(|version| version > 10)
                {
                    return Err(denort_helper::LoadError::Io(std::io::Error::other(
                        "package requires a newer Node-API version than this runtime (10)",
                    )));
                }
                let resolved = package.native_library_path(&relative).map_err(|e| {
                    denort_helper::LoadError::Io(std::io::Error::other(format!("{e:#}")))
                })?;
                if let Some(resolved) = resolved {
                    return Ok(Cow::Owned(resolved));
                }
            }
        }
        Ok(Cow::Borrowed(path))
    }
}
