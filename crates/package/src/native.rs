//! Materialize only libraries requested by the native loader, outside the VFS.
use crate::{EntryKind, Package};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(crate) struct NativeLibraries {
    paths: BTreeMap<String, PathBuf>,
    temporary: Option<tempfile::TempDir>,
}

impl Package {
    /// Resolve a packaged native library into a private temporary directory.
    /// None means the archive has no such entry; ordinary disk loading applies.
    pub fn native_library_path(&self, name: &str) -> Result<Option<PathBuf>> {
        let name = self.resolve(name)?;
        let Some(entry) = self.entries.get(&name) else {
            return Ok(None);
        };
        ensure!(
            entry.kind == EntryKind::File,
            "not a native library file: {name}"
        );
        // Workers share this state, so a library keeps the same OS identity for
        // the lifetime of the package. Separate package instances are isolated.
        let mut native = self.native.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(path) = native.paths.get(&name) {
            return Ok(Some(path.clone()));
        }

        // ZIP corruption is an application error, never a reason to load disk
        // bytes. Validate completely before creating or publishing anything.
        let bytes = self.read_index(entry.index)?;
        if native.temporary.is_none() {
            let mut builder = tempfile::Builder::new();
            builder.prefix("dnr-native-");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                builder.permissions(fs::Permissions::from_mode(0o700));
            }
            native.temporary = Some(
                builder
                    .tempdir()
                    .context("creating temporary native library directory")?,
            );
        }
        let root = native.temporary.as_ref().unwrap().path().canonicalize()?;
        prepare_parent(&root, &name)?;
        let path = root.join(&name);
        let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        file.write_all(&bytes)?;
        file.flush()?;
        // Publish a complete file, never overwrite an unexpected existing file.
        file.persist_noclobber(&path)
            .with_context(|| format!("extracting native library to {}", path.display()))?;
        native.paths.insert(name, path.clone());
        Ok(Some(path))
    }

    /// Only call when the host is exiting and will no longer load libraries.
    /// The runtime holds Package in a static, so it cannot rely on Drop at exit.
    pub fn cleanup_native_libraries(&self) {
        let mut native = self.native.lock().unwrap_or_else(|e| e.into_inner());
        native.temporary.take();
        native.paths.clear();
    }
}

// Do not follow symlinks if application code modifies its temporary tree.
// Archive symlinks have already been resolved by Package::resolve.
fn prepare_parent(root: &Path, name: &str) -> io::Result<()> {
    let mut parent = root.to_owned();
    for component in Path::new(name).parent().unwrap().components() {
        parent.push(component);
        if let Err(error) = fs::symlink_metadata(&parent) {
            if error.kind() != io::ErrorKind::NotFound {
                return Err(error);
            }
            fs::create_dir(&parent)?;
        }
        let meta = fs::symlink_metadata(&parent)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(io::Error::other(
                "native library parent is not a real directory",
            ));
        }
    }
    Ok(())
}
