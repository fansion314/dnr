use crate::Args;
use anyhow::{Context, Result, bail, ensure};
use clap::ValueEnum;
use dnr_package::{Include, PackOptions, PackageConfig, pack_with_config};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Target {
    Macos,
    Archlinux,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    app_id: String,
    name: String,
    version: String,
    entry: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    macos: Option<Macos>,
    #[serde(default)]
    linux: Option<Linux>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Macos {
    icon: PathBuf,
    /// Installed shared runtime path, never a runtime to embed or install.
    #[serde(default = "default_runtime")]
    runtime_path: String,
    #[serde(default = "default_minimum")]
    minimum_system_version: String,
}
fn default_runtime() -> String {
    "/usr/local/bin/dnr".into()
}
fn default_minimum() -> String {
    "11.0".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Linux {
    package_name: String,
    icon: PathBuf,
    #[serde(default = "one")]
    release: u32,
    #[serde(default)]
    backend: Backend,
    #[serde(default = "utility")]
    categories: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    license: Vec<String>,
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    runtime_package: Option<String>,
}
fn one() -> u32 {
    1
}
fn utility() -> Vec<String> {
    vec!["Utility".into()]
}
#[derive(Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Backend {
    #[default]
    Webview,
    SystemCef,
}

fn line(value: &str, field: &str) -> Result<()> {
    ensure!(
        !value.chars().any(char::is_control),
        "{field} must not contain control characters"
    );
    Ok(())
}
fn token(value: &str, field: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._+-".contains(&c))
            && value.as_bytes()[0].is_ascii_alphanumeric(),
        "invalid {field}: {value:?}"
    );
    Ok(())
}
fn load(path: &Path) -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(
        &fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .context("invalid desktop manifest")?;
    token(&m.app_id, "appId")?;
    ensure!(
        m.app_id.contains('.') && !m.app_id.contains('+'),
        "appId must be a reverse-DNS identifier"
    );
    for part in m.app_id.split('.') {
        ensure!(
            !part.is_empty() && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'),
            "invalid appId component"
        );
    }
    line(&m.name, "name")?;
    ensure!(!m.name.trim().is_empty(), "name must not be empty");
    line(&m.description, "description")?;
    ensure!(
        !m.version.is_empty()
            && m.version
                .split('.')
                .all(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_digit())),
        "version must contain dot-separated decimal numbers (for example 1.2.0)"
    );
    ensure!(!m.entry.is_empty(), "entry must not be empty");
    Ok(m)
}

pub fn build(args: &Args, path: &Path, target: Target) -> Result<()> {
    let m = load(path)?;
    ensure!(
        args.app_id.as_ref().is_none_or(|id| id == &m.app_id),
        "--app-id conflicts with desktop manifest appId"
    );
    match target {
        Target::Macos => ensure!(
            cfg!(all(target_os = "macos", target_arch = "aarch64")),
            "macos packaging requires a macOS ARM64 host with Xcode Command Line Tools"
        ),
        Target::Archlinux => ensure!(
            cfg!(all(target_os = "linux", target_arch = "x86_64")),
            "archlinux packaging requires an Arch/CachyOS x86_64 host with makepkg, fakeroot and zstd"
        ),
    }
    let expected = match target {
        Target::Macos => ".app",
        Target::Archlinux => ".pkg.tar.zst",
    };
    ensure!(
        args.output
            .file_name()
            .is_some_and(|s| s.to_string_lossy().ends_with(expected)),
        "output must end with {expected}"
    );
    let parent = args
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let output = parent
        .canonicalize()?
        .join(args.output.file_name().unwrap());
    if let Ok(meta) = fs::symlink_metadata(&output) {
        ensure!(
            args.force,
            "output exists; use --force: {}",
            output.display()
        );
        ensure!(
            !meta.file_type().is_symlink(),
            "refusing to replace an output symlink"
        );
        ensure!(
            match target {
                Target::Macos => meta.is_dir(),
                Target::Archlinux => meta.is_file(),
            },
            "existing output has the wrong type"
        );
    }
    // Build outside the input tree so neither staging files nor the old output
    // accidentally become application content. Publish only after validation.
    let work = tempfile::tempdir()?;
    let mut excludes = args.exclude.clone();
    let input = args.directory.canonicalize()?;
    ensure!(
        !input.starts_with(&output),
        "output must not contain the input directory"
    );
    if let Ok(relative) = output.strip_prefix(&input) {
        excludes.push(
            relative
                .to_str()
                .context("non-UTF-8 output path")?
                .replace(std::path::MAIN_SEPARATOR, "/"),
        );
    }
    let options = PackOptions {
        directory: input,
        entry: args.entry.clone().unwrap_or_else(|| m.entry.clone()),
        output: work.path().join("application.dnp"),
        includes: args
            .include
            .iter()
            .map(|s| Include::parse(s))
            .collect::<Result<_>>()?,
        excludes,
        app_id: Some(m.app_id.clone()),
        force: false,
    };
    let config = args
        .package_config
        .as_deref()
        .map(PackageConfig::load)
        .transpose()?
        .unwrap_or_default();
    let report = pack_with_config(&options, &config)?;
    let base = path.parent().unwrap_or(Path::new("."));
    // The final staging directory is a sibling, allowing same-filesystem rename.
    let stage = tempfile::Builder::new()
        .prefix(".dnc-")
        .tempdir_in(output.parent().unwrap())?;
    let artifact = match target {
        Target::Macos => macos(&m, base, work.path(), stage.path())?,
        Target::Archlinux => archlinux(&m, base, work.path(), stage.path())?,
    };
    publish(&artifact, &output, stage, args.force)?;
    eprintln!(
        "{}: {} application files, {} bytes (.dnp); shared dnr required",
        output.display(),
        report.files,
        report.package_bytes
    );
    Ok(())
}

fn publish(artifact: &Path, output: &Path, stage: tempfile::TempDir, force: bool) -> Result<()> {
    let backup = stage.path().join("previous");
    let exists = fs::symlink_metadata(output).is_ok();
    ensure!(
        force || !exists,
        "output appeared during build; use --force"
    );
    if exists {
        fs::rename(output, &backup).context("moving previous output aside")?;
    }
    if let Err(error) = fs::rename(artifact, output) {
        if exists && let Err(restore) = fs::rename(&backup, output) {
            // Do not let TempDir cleanup remove the only remaining old copy.
            let recovery = stage.keep();
            bail!(
                "publish failed: {error}; restore failed: {restore}; previous output saved at {}/previous",
                recovery.display()
            );
        }
        return Err(error).context("publishing desktop package");
    }
    Ok(())
}
fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("starting {command:?}"))?;
    ensure!(status.success(), "{command:?} failed: {status}");
    Ok(())
}
fn copy(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination.parent().unwrap())?;
    fs::copy(source, destination).with_context(|| format!("copying {}", source.display()))?;
    mode(destination, 0o644)
}
fn mode(path: &Path, value: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(value))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, value);
    }
    Ok(())
}
// Distribution permissions must not depend on the builder's umask.
fn normalize_modes(root: &Path) -> Result<()> {
    mode(root, 0o755)?;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            normalize_modes(&entry.path())?;
        } else {
            mode(&entry.path(), 0o644)?;
        }
    }
    Ok(())
}
fn icon_path(base: &Path, path: &Path, extensions: &[&str]) -> Result<PathBuf> {
    let path = base.join(path);
    ensure!(path.is_file(), "icon is not a file: {}", path.display());
    ensure!(
        path.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|e| extensions.contains(&e)),
        "icon must use one of these formats: {}",
        extensions.join(", ")
    );
    Ok(path)
}
fn macos(m: &Manifest, base: &Path, work: &Path, stage: &Path) -> Result<PathBuf> {
    let config = m.macos.as_ref().context("manifest needs macos settings")?;
    line(&config.runtime_path, "macos.runtimePath")?;
    ensure!(
        Path::new(&config.runtime_path).is_absolute(),
        "macos.runtimePath must be the absolute installed runtime path"
    );
    ensure!(
        config
            .minimum_system_version
            .split('.')
            .all(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_digit())),
        "invalid minimumSystemVersion"
    );
    let icon = icon_path(base, &config.icon, &["icns", "png"])?;
    let app = stage.join("application.app");
    let contents = app.join("Contents");
    let resources = contents.join("Resources");
    fs::create_dir_all(contents.join("MacOS"))?;
    copy(
        &work.join("application.dnp"),
        &resources.join("application.dnp"),
    )?;
    if icon.extension().is_some_and(|s| s == "icns") {
        ensure!(fs::read(&icon)?.starts_with(b"icns"), "invalid ICNS icon");
        copy(&icon, &resources.join("AppIcon.icns"))?;
    } else {
        let iconset = work.join("AppIcon.iconset");
        fs::create_dir(&iconset)?;
        for size in [16, 32, 128, 256, 512] {
            for scale in [1, 2] {
                let pixels = (size * scale).to_string();
                let suffix = if scale == 2 { "@2x" } else { "" };
                run(Command::new("sips")
                    .args(["-z", &pixels, &pixels])
                    .arg(&icon)
                    .arg("--out")
                    .arg(iconset.join(format!("icon_{size}x{size}{suffix}.png"))))?;
            }
        }
        run(Command::new("iconutil")
            .args(["-c", "icns"])
            .arg(&iconset)
            .arg("-o")
            .arg(resources.join("AppIcon.icns")))?;
    }
    let plist = contents.join("Info.plist");
    fs::write(
        &plist,
        serde_json::to_vec_pretty(&serde_json::json!({
            "CFBundleIdentifier": m.app_id, "CFBundleExecutable": "launcher",
            "CFBundleName": m.name, "CFBundleDisplayName": m.name,
            "CFBundleIconFile": "AppIcon.icns", "CFBundlePackageType": "APPL",
            "CFBundleInfoDictionaryVersion": "6.0", "CFBundleShortVersionString": m.version,
            "CFBundleVersion": m.version, "LSMinimumSystemVersion": config.minimum_system_version,
            "NSHighResolutionCapable": true, "NSSupportsAutomaticGraphicsSwitching": true,
            "NSAppTransportSecurity": {"NSAllowsLocalNetworking": true},
            "DNRRuntimePath": config.runtime_path
        }))?,
    )?;
    run(Command::new("plutil")
        .args(["-convert", "xml1"])
        .arg(&plist))?;
    let source = work.join("launcher.m");
    fs::write(&source, include_str!("../templates/macos-launcher.m"))?;
    run(Command::new("clang")
        .args([
            "-fobjc-arc",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-arch",
            "arm64",
            "-framework",
            "AppKit",
        ])
        .arg(format!(
            "-mmacosx-version-min={}",
            config.minimum_system_version
        ))
        .arg(source)
        .arg("-o")
        .arg(contents.join("MacOS/launcher")))?;
    normalize_modes(&app)?;
    mode(&contents.join("MacOS/launcher"), 0o755)?;
    run(Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(&app))?;
    run(Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app))?;
    Ok(app)
}

// POSIX shell literals (also used by generated PKGBUILD). Never interpolate
// manifest strings as shell syntax, desktop Exec fields, or variable names.
fn shell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn desktop_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace(' ', "\\s")
}
fn list(values: &[String], field: &str) -> Result<String> {
    let mut result = String::new();
    for value in values {
        line(value, field)?;
        ensure!(
            !value.is_empty() && !value.contains(';'),
            "invalid {field} item"
        );
        result.push_str(&desktop_string(value));
        result.push(';');
    }
    Ok(result)
}
fn array(values: &[String]) -> String {
    values
        .iter()
        .map(|v| shell(v))
        .collect::<Vec<_>>()
        .join(" ")
}
fn linux_files(m: &Manifest, base: &Path, work: &Path) -> Result<()> {
    let config = m.linux.as_ref().context("manifest needs linux settings")?;
    token(&config.package_name, "linux.packageName")?;
    ensure!(
        config.package_name == config.package_name.to_ascii_lowercase(),
        "linux.packageName must be lowercase"
    );
    ensure!(config.release > 0, "linux.release must be positive");
    for value in config
        .depends
        .iter()
        .chain(config.license.iter())
        .chain(config.runtime_package.iter())
    {
        line(value, "Linux package metadata")?;
        ensure!(!value.is_empty(), "empty Linux package metadata");
    }
    if let Some(name) = &config.runtime_package {
        token(name, "linux.runtimePackage")?;
    }
    let icon = icon_path(base, &config.icon, &["svg", "png"])?;
    let root = work.join("payload");
    let appdir = format!("usr/lib/{}", m.app_id);
    copy(
        &work.join("application.dnp"),
        &root.join(&appdir).join("application.dnp"),
    )?;
    let extension = icon.extension().unwrap().to_str().unwrap();
    // PNG uses the unthemed fallback directory; SVG is scalable in hicolor.
    let icon_dir = if extension == "svg" {
        "usr/share/icons/hicolor/scalable/apps"
    } else {
        "usr/share/pixmaps"
    };
    let icon_dest = format!("{icon_dir}/{}.{extension}", m.app_id);
    copy(&icon, &root.join(&icon_dest))?;
    let check = if config.backend == Backend::SystemCef {
        "\"$runtime\" --check-system-cef || exit $?\n"
    } else {
        ""
    };
    let launcher = format!(
        "#!/bin/sh\n# GUI sessions often omit /usr/local/bin from PATH.\nruntime=$(command -v dnr) || runtime=/usr/local/bin/dnr\nif [ ! -x \"$runtime\" ]; then\n  echo 'Shared dnr runtime not found; install dnr first.' >&2\n  exit 127\nfi\nexport LAUFEY_APP_ID={}\nexport LAUFEY_APP_NAME={}\nexport LAUFEY_APP_ICON={}\n{check}exec \"$runtime\" {} \"$@\"\n",
        shell(&m.app_id),
        shell(&m.name),
        shell(&format!("/{icon_dest}")),
        shell(&format!("/{appdir}/application.dnp"))
    );
    let bin = root.join("usr/bin").join(&config.package_name);
    fs::create_dir_all(bin.parent().unwrap())?;
    fs::write(&bin, launcher)?;
    mode(&bin, 0o755)?;
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec=/usr/bin/{}\nTryExec=/usr/bin/{}\nIcon={}\nTerminal=false\nStartupNotify=false\nStartupWMClass={}\nCategories={}\nKeywords={}\n",
        desktop_string(&m.name),
        desktop_string(&m.description),
        config.package_name,
        config.package_name,
        m.app_id,
        m.app_id,
        list(&config.categories, "categories")?,
        list(&config.keywords, "keywords")?
    );
    let desktop_path = root.join(format!("usr/share/applications/{}.desktop", m.app_id));
    fs::create_dir_all(desktop_path.parent().unwrap())?;
    fs::write(desktop_path, desktop)?;
    let mut depends = vec![
        "gtk3".to_owned(),
        if config.backend == Backend::SystemCef {
            "cef"
        } else {
            "webkit2gtk-4.1"
        }
        .to_owned(),
    ];
    depends.extend(config.depends.clone());
    depends.extend(config.runtime_package.clone());
    depends.sort();
    depends.dedup();
    let optional = if config.runtime_package.is_none() {
        "optdepends=('dnr: shared runtime, alternatively installed in /usr/local/bin')\n"
    } else {
        ""
    };
    let pkgbuild = format!(
        "# Generated by dnc; contains prepared files only.\npkgname={}\npkgver={}\npkgrel={}\npkgdesc={}\narch=('x86_64')\nlicense=({})\ndepends=({})\n{optional}options=('!strip' '!debug')\npackage() {{\n  cp -R -- \"$startdir/payload/.\" \"$pkgdir/\"\n}}\n",
        shell(&config.package_name),
        shell(&m.version),
        config.release,
        shell(&m.description),
        array(&config.license),
        array(&depends)
    );
    fs::write(work.join("PKGBUILD"), pkgbuild)?;
    normalize_modes(&root)?;
    mode(&bin, 0o755)?;
    Ok(())
}
fn archlinux(m: &Manifest, base: &Path, work: &Path, stage: &Path) -> Result<PathBuf> {
    linux_files(m, base, work)?;
    // makepkg owns ALPM metadata, mtree, ownership and compression. No -s/-i:
    // do not install dependencies or change the package database.
    let config = work.join("makepkg.conf");
    fs::write(
        &config,
        "source /etc/makepkg.conf\nPKGEXT='.pkg.tar.zst'\nCOMPRESSZST=(zstd -c -T0 -6 -)\n",
    )?;
    run(Command::new("makepkg")
        .current_dir(work)
        .args(["--nodeps", "--force", "--noconfirm", "--nosign", "--config"])
        .arg(&config)
        .env("PKGDEST", stage)
        .env("PKGEXT", ".pkg.tar.zst")
        .env("CARCH", "x86_64")
        .env("BUILDDIR", work.join("build")))?;
    let outputs = fs::read_dir(stage)?.collect::<std::io::Result<Vec<_>>>()?;
    let packages: Vec<_> = outputs
        .into_iter()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".pkg.tar.zst")
        })
        .collect();
    ensure!(
        packages.len() == 1,
        "makepkg must produce exactly one .pkg.tar.zst"
    );
    Ok(packages[0].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(dir: &Path) -> PathBuf {
        fs::write(
            dir.join("icon.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )
        .unwrap();
        fs::write(dir.join("application.dnp"), "package bytes").unwrap();
        let path = dir.join("desktop.json");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "appId": "org.example.test", "name": "测试 ' $(touch INJECTED) \\ app",
                "version": "1.2.3", "entry": "main.ts",
                "description": "测试 ' $(touch INJECTED)",
                "linux": { "packageName": "test-app", "icon": "icon.svg", "license": ["MIT"] }
            }))
            .unwrap(),
        )
        .unwrap();
        path
    }

    #[test]
    fn rejects_manifest_typos_and_identity_injection() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture(dir.path());
        let original: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        for (key, value) in [
            ("appId", "../escape"),
            ("name", "bad\nExec=sh"),
            ("version", "1;command"),
            ("typo", "ignored"),
        ] {
            let mut json = original.clone();
            json[key] = value.into();
            fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
            assert!(load(&path).is_err(), "{key}");
        }
    }

    #[test]
    fn publish_preserves_previous_output_on_failure_and_requires_force() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("app");
        fs::create_dir(&out).unwrap();
        fs::write(out.join("old"), "keep me").unwrap();
        let stage = tempfile::tempdir_in(dir.path()).unwrap();
        let missing = stage.path().join("missing");
        assert!(publish(&missing, &out, stage, true).is_err());
        assert_eq!(fs::read_to_string(out.join("old")).unwrap(), "keep me");
        let stage = tempfile::tempdir_in(dir.path()).unwrap();
        let new = stage.path().join("new");
        fs::create_dir(&new).unwrap();
        assert!(publish(&new, &out, stage, false).is_err());
        assert!(out.join("old").exists());
        let stage = tempfile::tempdir_in(dir.path()).unwrap();
        let new = stage.path().join("new");
        fs::create_dir(&new).unwrap();
        fs::write(new.join("new"), "new").unwrap();
        publish(&new, &out, stage, true).unwrap();
        assert!(out.join("new").exists());
        assert!(!out.join("old").exists());
    }

    #[test]
    #[cfg(unix)]
    fn linux_payload_shell_escaping_arguments_identity_and_cef_guard() {
        for backend in [Backend::Webview, Backend::SystemCef] {
            let dir = tempfile::tempdir().unwrap();
            let path = fixture(dir.path());
            let mut m = load(&path).unwrap();
            m.linux.as_mut().unwrap().backend = backend;
            linux_files(&m, dir.path(), dir.path()).unwrap();
            let launcher = dir.path().join("payload/usr/bin/test-app");
            let desktop = fs::read_to_string(
                dir.path()
                    .join("payload/usr/share/applications/org.example.test.desktop"),
            )
            .unwrap();
            assert!(desktop.contains("StartupWMClass=org.example.test\n"));
            assert!(desktop.contains("Name=测试\\s'\\s$(touch\\sINJECTED)\\s\\\\\\sapp\n"));
            // Execute PKGBUILD in disposable staging: shell syntax in metadata
            // must remain data and packaging must preserve the payload.
            let pkgdir = dir.path().join("installed");
            fs::create_dir(&pkgdir).unwrap();
            let status = Command::new("bash")
                .current_dir(dir.path())
                .args(["-ec", "source ./PKGBUILD; package"])
                .env("startdir", dir.path())
                .env("pkgdir", &pkgdir)
                .status()
                .unwrap();
            assert!(status.success());
            assert_eq!(
                fs::read(pkgdir.join("usr/lib/org.example.test/application.dnp")).unwrap(),
                b"package bytes"
            );
            let probe = dir.path().join("dnr");
            fs::write(&probe, "#!/bin/sh\nif [ \"$1\" = --check-system-cef ]; then exit \"$PROBE_CEF_STATUS\"; fi\nprintf '%s\\n' \"$LAUFEY_APP_NAME\" \"$LAUFEY_APP_ID\" \"$PWD\" \"$@\"\n").unwrap();
            mode(&probe, 0o755).unwrap();
            let run_probe = |code: &str| {
                Command::new(&launcher)
                    .current_dir(dir.path())
                    .args(["a b", "", "--literal", "'$()"])
                    .env("PATH", dir.path())
                    .env("PROBE_CEF_STATUS", code)
                    .output()
                    .unwrap()
            };
            let result = run_probe("0");
            assert!(result.status.success());
            let stdout = String::from_utf8(result.stdout).unwrap();
            assert!(stdout.starts_with(&format!("{}\n{}\n", m.name, m.app_id)));
            assert!(
                stdout.ends_with(
                    "/usr/lib/org.example.test/application.dnp\na b\n\n--literal\n'$()\n"
                )
            );
            assert!(!dir.path().join("INJECTED").exists());
            if m.linux.as_ref().unwrap().backend == Backend::SystemCef {
                assert_eq!(run_probe("42").status.code(), Some(42));
            }
        }
    }
}
