//! Shared application host. The archive is external to the runtime executable.
use crate::{
    binary::{StandaloneData, StandaloneModules},
    file_system::{DenoRtSys, FileBackedVfs, VfsRoot},
};
use deno_core::{
    anyhow::{Context, bail},
    error::AnyError,
    url::Url,
};
use deno_lib::standalone::{
    binary::{
        Metadata, NodeModules, SerializedWorkspaceResolver, SerializedWorkspaceResolverImportMap,
    },
    virtual_fs::*,
};
use dnr_package::{DEFAULT_CACHE_BYTES, EntryKind, Package};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

static PACKAGE: OnceLock<Arc<Package>> = OnceLock::new();
pub static WINDOW_ICON: OnceLock<dnr_package::WindowIcon> = OnceLock::new();

// Called while loading the application, before the JS/native threads start.
fn desktop_identity(manifest: &dnr_package::Manifest) {
    unsafe {
        std::env::set_var("LAUFEY_APP_ID", &manifest.app_id);
        if let Some(desktop) = &manifest.desktop {
            if let Some(name) = &desktop.name {
                std::env::set_var("LAUFEY_APP_NAME", name);
            }
            if let Some(icon) = &desktop.window_icon {
                let _ = WINDOW_ICON.set(icon.clone());
            }
        }
    }
}

struct PackageCommands(Arc<Package>);
impl deno_runtime::deno_process::NativeCommandResolver for PackageCommands {
    fn resolve(
        &self,
        command: &str,
        cwd: &Path,
        path: Option<&std::ffi::OsStr>,
    ) -> std::io::Result<Option<PathBuf>> {
        let candidates = if command.contains('/') {
            vec![cwd.join(command)]
        } else {
            path.map(|p| {
                std::env::split_paths(p)
                    .map(|p| cwd.join(p).join(command))
                    .collect()
            })
            .unwrap_or_default()
        };
        for candidate in candidates {
            if let Some(name) = self.0.logical_path(&candidate) {
                if let Some(real) = self
                    .0
                    .native_path(&name, dnr_package::NativeUse::Executable)
                    .map_err(|e| std::io::Error::other(format!("{e:#}")))?
                {
                    return Ok(Some(real));
                }
            }
            if !command.contains('/') && candidate.is_file() {
                use std::os::unix::fs::PermissionsExt;
                if candidate.metadata()?.permissions().mode() & 0o111 != 0 {
                    return Ok(Some(candidate));
                }
            }
        }
        Ok(None)
    }
}

pub fn cleanup_native_libraries() {
    if let Some(package) = PACKAGE.get() {
        package.cleanup_native_libraries();
    }
    crate::dnr_cache::finish();
    if let Some(package) = PACKAGE.get() {
        package.finish_cache();
    }
}

pub fn read_text(path: &Path) -> std::io::Result<String> {
    if let Some(package) = PACKAGE.get()
        && let Some(relative) = package.logical_path(path)
    {
        if let Some(bytes) = package.read(&relative).map_err(std::io::Error::other)? {
            return String::from_utf8(bytes.to_vec()).map_err(std::io::Error::other);
        }
    }
    std::fs::read_to_string(path)
}

fn directory(package: &Package) -> Result<VirtualDirectory, AnyError> {
    // Parents sort before descendants in the validated flat index. Walking it
    // backwards lets us finish each directory once, without rescanning the
    // entire archive per directory (or recursively growing the call stack).
    let mut children: BTreeMap<&str, Vec<VfsEntry>> = BTreeMap::new();
    for (name, entry) in package.entries.iter().rev() {
        let (parent, basename) = name.rsplit_once('/').unwrap_or(("", name));
        let basename = basename.to_owned();
        let node = match &entry.kind {
            EntryKind::Directory => VfsEntry::Dir(VirtualDirectory {
                name: basename,
                entries: VirtualDirectoryEntries::new(
                    children.remove(name.as_str()).unwrap_or_default(),
                ),
            }),
            EntryKind::Symlink(target) => {
                let target = dnr_package::resolve_link(name, target)?;
                VfsEntry::Symlink(VirtualSymlink {
                    name: basename,
                    dest_parts: VirtualSymlinkParts::from_path(Path::new(&target)),
                    dest_is_dir: package
                        .entries
                        .get(&target)
                        .is_some_and(|e| e.kind == EntryKind::Directory),
                })
            }
            EntryKind::File => VfsEntry::File(VirtualFile {
                name: basename,
                offset: OffsetWithLength {
                    offset: entry.index as u64,
                    len: entry.size,
                },
                is_valid_utf8: false,
                transpiled_offset: None,
                cjs_export_analysis_offset: None,
                source_map_offset: None,
                mtime: None,
                executable: package.is_executable(name),
            }),
        };
        children.entry(parent).or_default().push(node);
    }
    Ok(VirtualDirectory {
        name: String::new(),
        entries: VirtualDirectoryEntries::new(children.remove("").unwrap_or_default()),
    })
}

static INSTALLED_LEASE: OnceLock<std::fs::File> = OnceLock::new();

pub fn application(mut args: Vec<String>) -> Result<StandaloneData, AnyError> {
    let mut code_cache = true;
    let mut transpile_cache = true;
    while let Some(arg) = args.first() {
        match arg.as_str() {
            "--no-code-cache" => code_cache = false,
            "--no-transpile-cache" => transpile_cache = false,
            _ => break,
        }
        args.remove(0);
    }
    let explicit = args.first().is_some_and(|s| s == "--package");
    if explicit {
        args.remove(0);
    }
    if args.is_empty() {
        bail!("usage: dnr <script.ts|application.dnp> [args...]");
    }
    let input = PathBuf::from(args.remove(0))
        .canonicalize()
        .context("opening application")?;
    let mut expected_entry = None;
    if explicit && args.first().is_some_and(|s| s == "--entry") {
        args.remove(0);
        if args.is_empty() {
            bail!("--entry needs a value");
        }
        expected_entry = Some(args.remove(0));
    }
    if args.first().is_some_and(|s| s == "--") {
        args.remove(0);
    }
    let installed = if input.is_dir() {
        let _ = INSTALLED_LEASE.set(dnr_package::metadata::installed_lease(&input)?);
        Some(dnr_package::metadata::installed_manifest(&input)?)
    } else {
        None
    };
    let is_package = explicit
        || (installed.is_none() && {
            use std::io::Read;
            let mut header = [0u8; 10];
            std::fs::File::open(&input)?.read(&mut header)? == 10 && &header == b"#!/bin/sh\n"
        });
    let (root, entry, app_id, mut vfs) = if is_package {
        let package = Arc::new(Package::open(&input, DEFAULT_CACHE_BYTES)?);
        package.check_platform()?;
        desktop_identity(&package.manifest);
        if code_cache || transpile_cache {
            if let Ok(generation) = package.cache_generation() {
                crate::dnr_cache::init(generation, code_cache, transpile_cache);
            }
        }
        deno_runtime::deno_process::set_native_command_resolver(Arc::new(PackageCommands(
            package.clone(),
        )));
        if let Some(expected) = expected_entry {
            if expected != package.manifest.entry {
                bail!("header entry does not match manifest");
            }
        }
        let root = package.root.clone();
        let tree = directory(&package)?;
        let entry = package.manifest.entry.clone();
        let id = package.manifest.app_id.clone();
        let mut vfs = FileBackedVfs::new(
            Cow::Borrowed(&[]),
            VfsRoot {
                dir: tree,
                root_path: root.clone(),
                start_file_offset: 0,
            },
            FileSystemCaseSensitivity::Sensitive,
        );
        vfs.dnr_package = Some(package.clone());
        let _ = PACKAGE.set(package);
        (root, entry, id, vfs)
    } else {
        let (root, entry, id) = if let Some((manifest, content)) = installed {
            desktop_identity(&manifest);
            if code_cache || transpile_cache {
                let open = || -> Result<_, AnyError> {
                    let stamp = dnr_package::persistent::SourceStamp::read(
                        &input.join(dnr_package::metadata::INSTALL),
                    )?;
                    dnr_package::persistent::Generation::open(
                        &dnr_package::cache_directory()?,
                        &input,
                        &content,
                        &manifest.app_id,
                        &stamp,
                    )
                };
                if let Ok(generation) = open() {
                    crate::dnr_cache::init(generation, code_cache, transpile_cache);
                }
            }
            (input.clone(), manifest.entry, manifest.app_id)
        } else {
            (
                input.parent().unwrap().to_owned(),
                input.file_name().unwrap().to_string_lossy().into_owned(),
                format!("script:{}", input.display()),
            )
        };
        let vfs = FileBackedVfs::new(
            Cow::Borrowed(&[]),
            VfsRoot {
                dir: VirtualDirectory {
                    name: String::new(),
                    entries: VirtualDirectoryEntries::new(vec![]),
                },
                root_path: root.clone(),
                start_file_offset: 0,
            },
            FileSystemCaseSensitivity::Sensitive,
        );
        (root, entry, id, vfs)
    };
    vfs.dnr_overlay = true;
    let mut import_map = None;
    for name in ["deno.json", "deno.jsonc"] {
        match read_text(&root.join(name)) {
            Ok(text) => {
                let config = jsonc_parser::parse_to_serde_value::<serde_json::Value>(
                    &text,
                    &Default::default(),
                )?;
                if let Some(map_path) = config.get("importMap").and_then(|v| v.as_str()) {
                    let map = root.join(map_path);
                    let json = read_text(&map).context("importMap must be a local file")?;
                    import_map = Some(SerializedWorkspaceResolverImportMap {
                        specifier: Url::from_file_path(map).unwrap().to_string(),
                        json,
                    });
                } else if config.get("imports").is_some() || config.get("scopes").is_some() {
                    import_map = Some(SerializedWorkspaceResolverImportMap {
                        specifier: name.into(),
                        json: config.to_string(),
                    });
                }
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    let mut package_jsons = BTreeMap::new();
    if let Ok(text) = read_text(&root.join("package.json")) {
        package_jsons.insert("package.json".into(), serde_json::from_str(&text)?);
    }
    use sha2::{Digest, Sha256};
    let storage_id = format!("dnr-{:x}", Sha256::digest(app_id.as_bytes()));
    let metadata = Metadata {
        argv: args,
        seed: None,
        code_cache_key: None,
        permissions: deno_runtime::deno_permissions::PermissionsOptions {
            allow_read: Some(vec![]),
            allow_write: Some(vec![]),
            allow_net: Some(vec![]),
            allow_env: Some(vec![]),
            allow_run: Some(vec![]),
            allow_ffi: Some(vec![]),
            allow_sys: Some(vec![]),
            allow_import: Some(vec![]),
            ..Default::default()
        },
        location: None,
        v8_flags: vec![
            "--stack-size=1024".into(),
            "--inspector-live-edit".into(),
            "--external-memory-max-reasonable-size=0".into(),
        ],
        log_level: None,
        ca_stores: None,
        ca_data: None,
        unsafely_ignore_certificate_errors: None,
        env_vars_from_env_file: Default::default(),
        workspace_resolver: SerializedWorkspaceResolver {
            import_map,
            jsr_pkgs: vec![],
            package_jsons,
            pkg_json_resolution: deno_resolver::workspace::PackageJsonDepResolution::Disabled,
            catalogs: Default::default(),
        },
        entrypoint_key: Url::from_file_path(root.join(&entry))
            .map_err(|_| deno_core::anyhow::anyhow!("invalid entrypoint path"))?
            .to_string(),
        preload_modules: vec![],
        require_modules: vec![],
        node_modules: Some(NodeModules::Byonm {
            root_node_modules_dir: Some("node_modules".into()),
        }),
        unstable_config: deno_lib::args::UnstableConfig {
            detect_cjs: true,
            raw_imports: true,
            ..Default::default()
        },
        otel_config: Default::default(),
        vfs_case_sensitivity: FileSystemCaseSensitivity::Sensitive,
        self_extracting: None,
        app_name: Some(storage_id),
        app_version: None,
        error_reporting_url: None,
        release_base_url: None,
    };
    let vfs = Arc::new(vfs);
    let modules = Arc::new(StandaloneModules::dnr(vfs.clone()));
    Ok(StandaloneData {
        metadata,
        modules,
        npm_snapshot: None,
        root_path: root,
        vfs,
    })
}

pub async fn execute(data: StandaloneData) -> Result<i32, AnyError> {
    let sys = DenoRtSys::new(data.vfs.clone());
    deno_runtime::deno_telemetry::init(
        &sys,
        deno_lib::version::otel_runtime_config(),
        data.metadata.otel_config.clone(),
    )?;
    let options = crate::dnr_desktop::options();
    let pump = tokio::spawn(crate::dnr_desktop::pump_bindings());
    let result = crate::run::run_with_options(Arc::new(sys.clone()), sys, data, options).await;
    pump.abort();
    result
}
