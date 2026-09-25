//! Runtime-independent install and cache commands (never initialize V8 or GUI).
use crate::{
    Package, cache_directory,
    materialize::{Receipt, group_lock, sidecar},
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn run(args: &[String]) -> Result<bool> {
    match args.first().map(String::as_str) {
        Some("install") => install(&args[1..], false)?,
        Some("cache") => cache(&args[1..])?,
        _ => return Ok(false),
    }
    Ok(true)
}
pub fn install(args: &[String], require_directory: bool) -> Result<()> {
    const HELP: &str = "usage: install <application.dnp> [directory] [--mode native|full] [--target <id>] [--force]";
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let mut positional = Vec::new();
    let mut force = false;
    let mut mode = "native";
    let mut target = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--force" => force = true,
            "--mode" => mode = args.next().context("--mode needs a value")?,
            "--target" => target = Some(args.next().context("--target needs a value")?.as_str()),
            value if value.starts_with("--") => bail!("unknown install option: {value}"),
            _ => positional.push(arg),
        }
    }
    ensure!((1..=2).contains(&positional.len()), "{HELP}");
    ensure!(matches!(mode, "native" | "full"), "invalid install mode");
    ensure!(
        !(require_directory || mode == "full") || positional.len() == 2,
        "this install requires a directory"
    );
    let source = Path::new(positional[0]);
    let package = Package::open_for_target(source, 0, target)?;
    ensure!(package.package_id().is_some(), "install requires v2 or v3");
    if mode == "full" {
        ensure!(
            package.manifest.format_version == 3,
            "full installation requires v3"
        );
        package.install_full(Path::new(positional[1]), force)?;
        println!("Installed full application in {}", positional[1]);
        return Ok(());
    }
    if positional.len() == 1 {
        let base = cache_directory()?;
        if package.manifest.format_version == 3 {
            let generation = package.cache_generation()?;
            for group in &package.manifest.groups {
                if package.v2.as_ref().unwrap().group_indices(group).is_some() {
                    package.prepare_group(group)?;
                }
            }
            generation.notify_index();
        } else {
            package.prepare_at(&base)?;
        }
        println!(
            "Prepared {} ({}) in {}",
            package.manifest.app_id,
            package.selected_target().unwrap(),
            base.display()
        );
        return Ok(());
    }
    let directory = Path::new(positional[1]);
    fs::create_dir_all(directory)?;
    ensure!(
        !fs::symlink_metadata(directory)?.file_type().is_symlink(),
        "install directory must be a real directory"
    );
    let directory = directory.canonicalize()?;
    let destination = directory.join(source.file_name().context("package needs a filename")?);
    if let Ok(meta) = fs::symlink_metadata(&destination) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "installed package must be a regular file"
        );
    }
    let install_lock = directory.join(".dnr-install.lock");
    let lock = match fs::File::create_new(&install_lock) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(
                fs::symlink_metadata(&install_lock)?.is_file(),
                "invalid install lock"
            );
            fs::File::open(&install_lock)?
        }
        Err(e) => return Err(e.into()),
    };
    lock.lock()?;
    let same = source.canonicalize()? == destination;
    if destination.exists() && !same {
        let existing = Package::open(&destination, 0).ok();
        ensure!(
            force || existing.as_ref().and_then(Package::package_id) == package.package_id(),
            "installed package differs; use --force"
        );
    }
    let mut stage = tempfile::NamedTempFile::new_in(&directory)?;
    std::io::copy(&mut fs::File::open(source)?, &mut stage)?;
    stage.flush()?;
    stage.as_file().sync_all()?;
    let copied = Package::open_for_target(stage.path(), 0, target)?;
    ensure!(
        copied.package_id() == package.package_id(),
        "source changed during installation"
    );
    copied.prepare_at(&sidecar(&destination))?;
    publish_permissions(&sidecar(&destination))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        stage
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o755))?;
    }
    if !same {
        if force || destination.exists() {
            stage.persist(&destination)?;
        } else {
            stage.persist_noclobber(&destination)?;
        }
        fs::File::open(&directory)?.sync_all()?;
    }
    println!(
        "Installed {} and {}",
        destination.display(),
        sidecar(&destination).display()
    );
    Ok(())
}

struct Cached {
    path: PathBuf,
    receipt: Receipt,
    partial: bool,
}
fn scan(base: &Path, format: u32) -> Result<Vec<Cached>> {
    let mut result = Vec::new();
    let mut queue = vec![(base.join(format!("v{format}")), 0)];
    let max_depth = if format == 3 { 3 } else { 4 };
    while let Some((path, depth)) = queue.pop() {
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        if !meta.is_dir() || meta.file_type().is_symlink() || depth > max_depth {
            continue;
        }
        if depth == max_depth {
            let (receipt_path, partial) = if path.join("receipt.json").is_file() {
                (path.join("receipt.json"), false)
            } else {
                (path.join("pending.json"), true)
            };
            let parsed = fs::symlink_metadata(&receipt_path)
                .ok()
                .filter(|m| m.is_file() && !m.file_type().is_symlink() && m.len() <= 65536)
                .and_then(|_| fs::read(&receipt_path).ok())
                .and_then(|b| serde_json::from_slice::<Receipt>(&b).ok());
            if let Some(receipt) = parsed {
                let target = path.parent().unwrap();
                let id = target.parent().unwrap();
                let shard = id.parent().unwrap();
                let valid_id = receipt.package_id.len() == 64
                    && receipt
                        .package_id
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
                if receipt.format == format
                    && valid_id
                    && crate::config::identifier(&receipt.group).is_ok()
                    && crate::config::identifier(&receipt.target).is_ok()
                    && id.file_name().unwrap() == receipt.package_id.as_str()
                    && target.file_name().unwrap() == receipt.target.as_str()
                    && (format == 3 || shard.file_name().unwrap() == &receipt.package_id[..2])
                    && (path.file_name().unwrap() == receipt.group.as_str()
                        || (partial
                            && path
                                .file_name()
                                .unwrap()
                                .to_string_lossy()
                                .starts_with(".dnr-stage-")))
                {
                    result.push(Cached {
                        path,
                        receipt,
                        partial,
                    });
                }
            }
            continue;
        }
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            if entry.file_name() != ".locks" {
                queue.push((entry.path(), depth + 1));
            }
        }
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}
fn cache(args: &[String]) -> Result<()> {
    const HELP: &str = "usage: dnr cache <list|info|clean|rebuild> [--directory <install-directory>] [--package <id-prefix>|--path <package-path>|--all|--stale] [--dry-run]";
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let command = &args[0];
    ensure!(
        matches!(command.as_str(), "list" | "info" | "clean" | "rebuild"),
        "{HELP}"
    );
    let mut directory = None;
    let mut package_path = None;
    let mut stale = false;
    let mut prefix = None;
    let mut all = false;
    let mut dry = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--directory" | "--package" | "--path" => {
                let value = args.get(i + 1).context("option requires a value")?;
                if args[i] == "--directory" {
                    directory = Some(PathBuf::from(value));
                } else if args[i] == "--path" {
                    package_path = Some(PathBuf::from(value));
                } else {
                    prefix = Some(value.clone());
                }
                i += 1;
            }
            "--all" => all = true,
            "--stale" => stale = true,
            "--dry-run" => dry = true,
            value => bail!("unknown cache option: {value}"),
        }
        i += 1;
    }
    ensure!(
        !(all && (prefix.is_some() || package_path.is_some())),
        "choose --all or a package selector"
    );
    ensure!(
        directory.is_none() || (package_path.is_none() && !stale && command != "rebuild"),
        "--directory cannot be combined with --path, --stale or rebuild"
    );
    ensure!(
        command != "rebuild" || (!all && !stale && prefix.is_none() && package_path.is_none()),
        "rebuild does not accept package selectors"
    );
    ensure!(
        command != "clean" || all || prefix.is_some() || package_path.is_some() || stale,
        "clean requires --all, --package, --path or --stale"
    );
    if let Some(p) = &prefix {
        ensure!(
            !p.is_empty() && p.len() <= 64 && p.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid package ID prefix"
        );
    }
    let mut prefix = prefix.map(|p| p.to_ascii_lowercase());
    if directory.is_none()
        && let Some(query_prefix) = &prefix
    {
        // Check ambiguity across both formats before deleting anything.
        let base = cache_directory()?;
        let mut ids = std::collections::BTreeSet::new();
        for item in scan(&base, 2)? {
            if item.receipt.package_id.starts_with(query_prefix) {
                ids.insert(item.receipt.package_id);
            }
        }
        for path in crate::persistent::catalog::directories(&base)? {
            let id = path.file_name().unwrap().to_string_lossy().into_owned();
            if id.starts_with(query_prefix) {
                ids.insert(id);
            }
        }
        ensure!(ids.len() <= 1, "ambiguous package ID prefix");
        ensure!(!ids.is_empty(), "no matching cached package");
        prefix = ids.into_iter().next();
    }
    if directory.is_none() {
        crate::persistent::catalog::command(
            &cache_directory()?,
            command,
            package_path.as_deref(),
            prefix.as_deref(),
            stale,
            dry,
        )?;
        if command == "rebuild" || package_path.is_some() || stale {
            return Ok(());
        }
    }
    let sidecars = directory.is_some();
    let bases = if let Some(directory) = directory {
        let mut bases = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && entry.file_name().to_string_lossy().ends_with(".unpacked")
            {
                bases.push(entry.path());
            }
        }
        bases
    } else {
        vec![cache_directory()?]
    };
    let mut entries = Vec::new();
    for base in bases {
        entries.extend(scan(&base, 2)?);
        if sidecars {
            entries.extend(scan(&base, 3)?);
        }
    }
    if let Some(prefix) = prefix {
        let prefix = prefix.to_ascii_lowercase();
        entries.retain(|e| e.receipt.package_id.starts_with(&prefix));
        let ids = entries
            .iter()
            .map(|e| &e.receipt.package_id)
            .collect::<std::collections::BTreeSet<_>>();
        ensure!(ids.len() <= 1, "ambiguous package ID prefix");
        ensure!(!sidecars || !ids.is_empty(), "no matching cached package");
    }
    let mut bytes = 0u64;
    let mut count = 0;
    for entry in entries {
        let lock_path = entry.path.parent().unwrap().join(&entry.receipt.group);
        let lock = group_lock(&lock_path, false)?;
        let active = lock.try_lock().is_err();
        let state = if active {
            "in-use"
        } else if entry.partial {
            "incomplete"
        } else {
            "ready"
        };
        let actual_bytes = directory_bytes(&entry.path)?;
        if command == "clean" && !active && !dry {
            ensure!(
                !fs::symlink_metadata(&entry.path)?.file_type().is_symlink(),
                "refusing redirected cache directory"
            );
            fs::remove_dir_all(&entry.path)?;
        }
        println!(
            "{} {} {} {} {} bytes {}{}",
            &entry.receipt.package_id[..12],
            entry.receipt.target,
            entry.receipt.group,
            entry.receipt.app_id.escape_debug(),
            actual_bytes,
            state,
            if command == "clean" && !active {
                if dry { " (would remove)" } else { " (removed)" }
            } else {
                ""
            }
        );
        bytes = bytes.saturating_add(actual_bytes);
        count += 1;
    }
    println!("{count} groups, {bytes} file bytes (including group metadata)");
    Ok(())
}

fn directory_bytes(path: &Path) -> Result<u64> {
    let mut bytes = 0u64;
    let mut pending = vec![path.to_owned()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else {
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok(bytes)
}

fn publish_permissions(root: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut queue = vec![root.to_owned()];
    while let Some(path) = queue.pop() {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                queue.push(entry.path());
            } else if entry.file_type()?.is_file() {
                let mode = entry.metadata()?.permissions().mode() & 0o777;
                fs::set_permissions(
                    entry.path(),
                    fs::Permissions::from_mode(mode & !0o222 | 0o444),
                )?;
            }
        }
    }
    Ok(())
}
