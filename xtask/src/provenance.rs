//! Pinned Git snapshots and prepared-source verification. Mirrors are read-only.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const RECORD: &str = ".dnr-provenance.json";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Upstream {
    name: String,
    repository: String,
    revision: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Provenance {
    version: u32,
    upstream: Upstream,
    patch_sha256: String,
    files: BTreeMap<String, Entry>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Entry {
    kind: String,
    sha256: String,
    mode: u32,
}

fn pins() -> Result<Vec<Upstream>> {
    let pins: Vec<Upstream> =
        serde_json::from_str(include_str!("../../integration/upstreams.json"))?;
    ensure!(pins.len() == 2, "expected Deno and Laufey upstream pins");
    for (pin, expected) in pins.iter().zip(["deno", "laufey"]) {
        ensure!(
            pin.name == expected
                && pin.revision.len() == 40
                && pin.revision.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid upstream pin: {}",
            pin.name
        );
    }
    Ok(pins)
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn managed(name: &str, path: &str) -> bool {
    path == RECORD
        || path == ".git"
        || (name == "deno"
            && (path == "Cargo.lock"
                || path.strip_prefix("cli/rt/").is_some_and(|file| {
                    matches!(
                        file,
                        "dnr.rs"
                            | "dnr_cache.rs"
                            | "dnr_vfs.rs"
                            | "dnr_main.rs"
                            | "dnr_backend.rs"
                            | "dnr_zoom_config.rs"
                            | "dnr_state.rs"
                            | "dnr_windows.js"
                            | "dnr_desktop.rs"
                            | "build.rs"
                    )
                })))
}

fn inventory(root: &Path, name: &str) -> Result<BTreeMap<String, Entry>> {
    let mut result = BTreeMap::new();
    let mut directories = vec![root.to_owned()];
    let mut buffer = [0u8; 64 * 1024];
    while let Some(directory) = directories.pop() {
        for child in fs::read_dir(directory)? {
            let child = child?;
            let path = child.path();
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .context("non-UTF-8 upstream path")?;
            if managed(name, relative) {
                continue;
            }
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                directories.push(path);
                continue;
            }
            let (kind, sha256) = if meta.file_type().is_symlink() {
                (
                    "symlink",
                    hash(fs::read_link(&path)?.as_os_str().as_bytes()),
                )
            } else {
                ensure!(
                    meta.is_file(),
                    "unsupported upstream node: {}",
                    path.display()
                );
                let mut digest = Sha256::new();
                let mut file = File::open(&path)?;
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    digest.update(&buffer[..count]);
                }
                ("file", format!("{:x}", digest.finalize()))
            };
            result.insert(
                relative.to_owned(),
                Entry {
                    kind: kind.into(),
                    sha256,
                    mode: meta.permissions().mode() & 0o777,
                },
            );
        }
    }
    Ok(result)
}

fn patch(root: &Path, upstream: &Upstream) -> PathBuf {
    root.join("integration")
        .join(format!("{}.patch", upstream.name))
}

fn verify(root: &Path, upstream: &Upstream) -> Result<()> {
    let directory = root.join(".upstream").join(&upstream.name);
    ensure!(
        fs::symlink_metadata(&directory)?.is_dir(),
        "prepared source is not a directory"
    );
    let record: Provenance = serde_json::from_slice(&fs::read(directory.join(RECORD))?)?;
    ensure!(
        record.version == 1 && record.upstream == *upstream,
        "upstream pin changed"
    );
    ensure!(
        record.patch_sha256 == hash(&fs::read(patch(root, upstream))?),
        "upstream patch changed"
    );
    let actual = inventory(&directory, &upstream.name)?;
    if record.files != actual {
        let changed = record
            .files
            .keys()
            .chain(actual.keys())
            .find(|path| record.files.get(*path) != actual.get(*path));
        bail!(
            "prepared source changed: {}",
            changed.map(String::as_str).unwrap_or("unknown")
        );
    }
    Ok(())
}

fn lease(root: &Path, exclusive: bool) -> Result<File> {
    let directory = root.join(".upstream");
    fs::create_dir_all(&directory)?;
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(".prepare.lock"))?;
    if exclusive {
        file.lock()?;
    } else {
        file.lock_shared()?;
    }
    Ok(file)
}

pub fn verify_for_build(root: &Path) -> Result<File> {
    let lease = lease(root, false)?;
    for upstream in pins()? {
        verify(root, &upstream).with_context(|| format!(
            "{} preparation is missing or stale; run cargo run -p xtask -- prepare (existing source trees will be preserved)", upstream.name
        ))?;
    }
    Ok(lease)
}

fn archive(source: &Path, revision: &str, destination: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["rev-parse", "--verify", &format!("{revision}^{{commit}}")])
        .output()?;
    ensure!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == revision,
        "source {} does not contain pinned commit {revision}; provide an independent checkout containing that commit",
        source.display()
    );
    let mut archive = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["archive", "--format=tar", revision])
        .stdout(Stdio::piped())
        .spawn()?;
    let extract = Command::new("tar")
        .args(["-xf", "-", "-C"])
        .arg(destination)
        .stdin(archive.stdout.take().context("Git archive pipe")?)
        .status();
    if !extract.as_ref().is_ok_and(|status| status.success()) {
        let _ = archive.kill();
    }
    let status = archive.wait()?;
    ensure!(
        extract?.success() && status.success(),
        "extracting pinned Git snapshot failed"
    );
    Ok(())
}

fn stage(root: &Path, upstream: &Upstream, source: &Path) -> Result<tempfile::TempDir> {
    let stage = tempfile::Builder::new()
        .prefix(&format!(".prepare-{}-", upstream.name))
        .tempdir_in(root.join(".upstream"))?;
    archive(source, &upstream.revision, stage.path())?;
    super::apply_upstream_patch(stage.path(), &patch(root, upstream))?;
    let record = Provenance {
        version: 1,
        upstream: upstream.clone(),
        patch_sha256: hash(&fs::read(patch(root, upstream))?),
        files: inventory(stage.path(), &upstream.name)?,
    };
    fs::write(stage.path().join(RECORD), serde_json::to_vec(&record)?)?;
    Ok(stage)
}

fn publish(root: &Path, upstream: &Upstream, stage: tempfile::TempDir) -> Result<()> {
    let parent = root.join(".upstream");
    let destination = parent.join(&upstream.name);
    let backup = if fs::symlink_metadata(&destination).is_ok() {
        let reserve = tempfile::Builder::new()
            .prefix(&format!("{}.backup-", upstream.name))
            .tempdir_in(&parent)?;
        let path = reserve.path().to_owned();
        reserve.close()?;
        fs::rename(&destination, &path)?;
        Some(path)
    } else {
        None
    };
    if let Err(error) = fs::rename(stage.path(), &destination) {
        if let Some(backup) = &backup {
            fs::rename(backup, &destination).context("restoring prior prepared tree")?;
        }
        return Err(error.into());
    }
    if let Some(backup) = backup {
        eprintln!(
            "Preserved previous {} sources at {}",
            upstream.name,
            backup.display()
        );
    }
    Ok(())
}

pub fn prepare(root: &Path, sources: &[(&str, PathBuf)]) -> Result<File> {
    let lease = lease(root, true)?;
    let mut stages = Vec::new();
    for upstream in pins()? {
        if verify(root, &upstream).is_ok() {
            continue;
        }
        let source = sources
            .iter()
            .find(|(name, _)| *name == upstream.name)
            .map(|(_, source)| source)
            .context("missing upstream source")?;
        stages.push((upstream.clone(), stage(root, &upstream, source)?));
    }
    // Finish every snapshot and patch check before replacing any current tree.
    for (upstream, stage) in stages {
        publish(root, &upstream, stage)?;
    }
    Ok(lease)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(tempfile::TempDir, Upstream, PathBuf)> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("source");
        fs::create_dir(&source)?;
        super::super::run(Command::new("git").args(["init", "--quiet"]).arg(&source))?;
        fs::write(source.join("value.txt"), "before\n")?;
        super::super::run(
            Command::new("git")
                .arg("-C")
                .arg(&source)
                .args(["add", "."]),
        )?;
        super::super::run(Command::new("git").arg("-C").arg(&source).args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ]))?;
        let output = Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["rev-parse", "HEAD"])
            .output()?;
        let pin = Upstream {
            name: "fixture".into(),
            repository: "unused".into(),
            revision: String::from_utf8(output.stdout)?.trim().into(),
        };
        fs::create_dir(temp.path().join("integration"))?;
        fs::create_dir(temp.path().join(".upstream"))?;
        fs::write(
            patch(temp.path(), &pin),
            "diff --git a/value.txt b/value.txt\n--- a/value.txt\n+++ b/value.txt\n@@ -1 +1 @@\n-before\n+after\n",
        )?;
        Ok((temp, pin, source))
    }

    #[test]
    fn snapshots_commit_and_detects_edits_and_patch_changes() -> Result<()> {
        let (temp, pin, source) = fixture()?;
        fs::write(source.join("value.txt"), "dirty working tree\n")?;
        fs::write(source.join("untracked.txt"), "not in snapshot")?;
        let prepared = stage(temp.path(), &pin, &source)?;
        assert_eq!(
            fs::read_to_string(prepared.path().join("value.txt"))?,
            "after\n"
        );
        assert!(!prepared.path().join("untracked.txt").exists());
        publish(temp.path(), &pin, prepared)?;
        verify(temp.path(), &pin)?;
        let directory = temp.path().join(".upstream/fixture");
        // Verification does not rewrite an already valid tree, and every
        // upstream input (including unpatched files) participates.
        let record = fs::read(directory.join(RECORD))?;
        verify(temp.path(), &pin)?;
        assert_eq!(fs::read(directory.join(RECORD))?, record);
        let mut other_pin = pin.clone();
        other_pin.revision = "0".repeat(40);
        assert!(verify(temp.path(), &other_pin).is_err());
        fs::write(directory.join("unexpected.txt"), "unexpected source")?;
        assert!(verify(temp.path(), &pin).is_err());
        fs::remove_file(directory.join("unexpected.txt"))?;
        fs::write(directory.join("value.txt"), "debug edit\n")?;
        assert!(verify(temp.path(), &pin).is_err());
        fs::write(directory.join("value.txt"), "after\n")?;
        fs::write(patch(temp.path(), &pin), "changed patch")?;
        assert!(verify(temp.path(), &pin).is_err());
        assert_eq!(
            fs::read_to_string(source.join("value.txt"))?,
            "dirty working tree\n"
        );
        Ok(())
    }

    #[test]
    fn replacement_preserves_old_tree_and_failed_stage_leaves_it_intact() -> Result<()> {
        let (temp, pin, source) = fixture()?;
        let destination = temp.path().join(".upstream/fixture");
        fs::create_dir(&destination)?;
        fs::write(destination.join("debug.txt"), "preserve me")?;
        let prepared = stage(temp.path(), &pin, &source)?;
        publish(temp.path(), &pin, prepared)?;
        verify(temp.path(), &pin)?;
        let backup = fs::read_dir(temp.path().join(".upstream"))?
            .filter_map(|entry| entry.ok())
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("fixture.backup-")
            })
            .unwrap();
        assert_eq!(
            fs::read_to_string(backup.path().join("debug.txt"))?,
            "preserve me"
        );
        fs::write(patch(temp.path(), &pin), "not a patch")?;
        assert!(stage(temp.path(), &pin, &source).is_err());
        assert_eq!(
            fs::read_to_string(destination.join("value.txt"))?,
            "after\n"
        );
        Ok(())
    }
}
