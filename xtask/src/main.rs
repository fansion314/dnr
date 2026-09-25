use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Task,
}
#[derive(Subcommand)]
enum Task {
    /// Prepare pinned upstream source trees and apply the local integration.
    Prepare {
        #[arg(long, default_value = "../mirror/deno")]
        deno: PathBuf,
        #[arg(long, default_value = "../mirror/laufey")]
        laufey: PathBuf,
    },
    /// Build dnr from the prepared sources; system-cef requires native Linux.
    Build {
        #[arg(long, default_value = if cfg!(target_os = "linux") { "dual" } else { "webview" })]
        backend: String,
        #[arg(long)]
        debug: bool,
        /// Build only dnr, leaving dnc to its standalone Cargo/package build.
        #[arg(long)]
        runtime_only: bool,
    },
}

fn run(cmd: &mut Command) -> Result<()> {
    let status = cmd.status().with_context(|| format!("starting {cmd:?}"))?;
    if !status.success() {
        bail!("command failed: {cmd:?} ({status})")
    }
    Ok(())
}

fn verify_linux_napi_exports(root: &Path, artifact: &Path) -> Result<()> {
    let exports = fs::read_to_string(
        root.join(".upstream/deno/ext/napi/generated_symbol_exports_list_linux.def"),
    )?;
    let expected: Vec<_> = exports.split('"').skip(1).step_by(2).collect();
    if expected.is_empty() {
        bail!("upstream Node-API export list is empty or has changed format");
    }
    let output = Command::new("readelf")
        .args(["--wide", "--dyn-syms"])
        .arg(artifact)
        .output()
        .context("checking runtime Node-API dynamic exports with readelf")?;
    if !output.status.success() {
        bail!(
            "readelf failed for {}: {}",
            artifact.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let symbols = String::from_utf8(output.stdout)?;
    let defined: std::collections::HashSet<_> = symbols
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.len() >= 8
                && matches!(fields[4], "GLOBAL" | "WEAK")
                && matches!(fields[5], "DEFAULT" | "PROTECTED")
                && fields[6] != "UND")
                .then(|| fields[7].split('@').next().unwrap())
        })
        .collect();
    let missing: Vec<_> = expected
        .into_iter()
        .filter(|name| !defined.contains(name))
        .collect();
    if !missing.is_empty() {
        bail!(
            "runtime is missing {} required Node-API exports: {}",
            missing.len(),
            missing.join(", ")
        );
    }
    Ok(())
}

fn verify_linux_no_gui_needed(artifact: &Path) -> Result<()> {
    let output = Command::new("readelf")
        .args(["--wide", "--dynamic"])
        .arg(artifact)
        .output()
        .context("checking runtime GUI dynamic dependencies with readelf")?;
    if !output.status.success() {
        bail!(
            "readelf failed for {}: {}",
            artifact.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let dynamic = String::from_utf8(output.stdout)?;
    let gui_prefixes = [
        "libcef.so",
        "libwebkit2gtk-",
        "libjavascriptcoregtk-",
        "libgtk-3.so",
        "libgdk-3.so",
        "libsoup-3.0.so",
        "libXi.so",
        "libX11.so",
        "libcairo.so",
        "libgio-2.0.so",
        "libgobject-2.0.so",
        "libglib-2.0.so",
    ];
    let gui_needed: Vec<_> = dynamic
        .lines()
        .filter(|line| line.contains("(NEEDED)"))
        .filter_map(|line| {
            line.split_once("Shared library: [")
                .and_then(|(_, name)| name.split_once(']'))
                .map(|(name, _)| name)
        })
        .filter(|name| gui_prefixes.iter().any(|prefix| name.starts_with(prefix)))
        .collect();
    if !gui_needed.is_empty() {
        bail!(
            "runtime directly links GUI libraries that must load lazily: {}",
            gui_needed.join(", ")
        );
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | "target" | "node_modules")) {
            continue;
        }
        let dest = dst.join(&name);
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            #[cfg(unix)]
            {
                let _ = fs::remove_file(&dest);
                std::os::unix::fs::symlink(fs::read_link(entry.path())?, dest)?;
            }
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    match Args::parse().command {
        Task::Prepare { deno, laufey } => {
            for (source, name, revision) in
                [(deno, "deno", "abd22074e4"), (laufey, "laufey", "1fe8787")]
            {
                let output = Command::new("git")
                    .arg("-C")
                    .arg(&source)
                    .args(["rev-parse", "HEAD"])
                    .output()?;
                let actual = String::from_utf8(output.stdout)?;
                if !output.status.success() || !actual.starts_with(revision) {
                    bail!("{name}: expected {revision}, found {}", actual.trim());
                }
                let dest = root.join(".upstream").join(name);
                if !dest.exists() {
                    copy_tree(&source, &dest)?;
                }
            }
            for name in ["deno", "laufey"] {
                let dest = root.join(".upstream").join(name);
                let patch = root.join("integration").join(format!("{name}.patch"));
                let already_applied = Command::new("git")
                    .current_dir(&dest)
                    .args(["apply", "--reverse", "--check"])
                    .arg(&patch)
                    .output()?
                    .status
                    .success();
                if !already_applied {
                    run(Command::new("git")
                        .current_dir(&dest)
                        .args(["apply", "--check"])
                        .arg(&patch))?;
                    run(Command::new("git")
                        .current_dir(&dest)
                        .arg("apply")
                        .arg(&patch))?;
                }
            }
            sync_integration(&root)?;
        }
        Task::Build {
            backend,
            debug,
            runtime_only,
        } => {
            if !((cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"))
                || (cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")))
            {
                bail!("dnr supports native macOS ARM64 and Linux x86_64 builds");
            }
            if !matches!(backend.as_str(), "webview" | "system-cef" | "dual") {
                bail!("unknown backend: {backend}");
            }
            if backend != "webview" && !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64"))
            {
                bail!("system-cef and dual require native Linux x86_64");
            }
            let manifest = root.join(".upstream/deno/Cargo.toml");
            if !manifest.exists() {
                bail!("run cargo run -p xtask -- prepare first");
            }
            if !runtime_only {
                let mut packager = Command::new("cargo");
                packager
                    .current_dir(&root)
                    .args(["build", "--locked", "-p", "dnc"])
                    .env("CARGO_TARGET_DIR", root.join("target"));
                if !debug {
                    packager.arg("--release");
                }
                run(&mut packager)?;
            }
            let mut cargo = Command::new("cargo");
            sync_integration(&root)?;
            cargo
                .current_dir(&root)
                .args(["build", "--manifest-path"])
                .arg(manifest)
                .args(["-p", "denort", "--bin", "dnr", "--locked"]);
            if !debug {
                cargo.arg("--release");
            }
            cargo
                .env("DNR_BACKEND", backend)
                .env("DNR_ROOT", &root)
                .env("CARGO_TARGET_DIR", root.join("target/runtime"));
            let cmake = root.join(".upstream/build-tools/cmake/data/bin/cmake");
            if cmake.exists() {
                cargo.env("CMAKE", cmake);
            }
            run(&mut cargo)?;
            let runtime_artifact = root.join(if debug {
                "target/runtime/debug/dnr"
            } else {
                "target/runtime/release/dnr"
            });
            if cfg!(target_os = "linux") {
                verify_linux_napi_exports(&root, &runtime_artifact)?;
                verify_linux_no_gui_needed(&runtime_artifact)?;
            }
            fs::create_dir_all(root.join("dist"))?;
            if !runtime_only {
                fs::copy(
                    root.join(if debug {
                        "target/debug/dnc"
                    } else {
                        "target/release/dnc"
                    }),
                    root.join("dist/dnc"),
                )?;
            }
            fs::copy(runtime_artifact, root.join("dist/dnr"))?;
            if !debug {
                let artifact = root.join("dist/dnr");
                let mut strip = Command::new("strip");
                if cfg!(target_os = "macos") {
                    strip.args(["-S", "-x"]);
                } else {
                    strip.arg("--strip-unneeded");
                }
                run(strip.arg(&artifact))?;
                if cfg!(target_os = "macos") {
                    run(Command::new("codesign")
                        .args(["--force", "--sign", "-"])
                        .arg(&artifact))?;
                }
            }
        }
    }
    Ok(())
}

fn sync_integration(root: &Path) -> Result<()> {
    let lock = fs::read(root.join("integration/deno.Cargo.lock"))?;
    let destination = root.join(".upstream/deno/Cargo.lock");
    if fs::read(&destination).ok().as_deref() != Some(&lock) {
        fs::write(destination, lock)?;
    }
    let rt = root.join(".upstream/deno/cli/rt");
    for file in [
        "dnr.rs",
        "dnr_cache.rs",
        "dnr_vfs.rs",
        "dnr_main.rs",
        "dnr_backend.rs",
        "build.rs",
    ] {
        let source = fs::read(root.join("integration/rt").join(file))?;
        if fs::read(rt.join(file)).ok().as_deref() != Some(&source) {
            fs::write(rt.join(file), source)?;
        }
    }
    let upstream = fs::read_to_string(root.join(".upstream/deno/cli/rt_desktop/lib.rs"))?;
    let prefix = upstream
        .split_once("/// Promote this dylib")
        .context("upstream desktop bridge changed")?
        .0;
    let mut prefix = prefix.to_owned();
    for unused in [
        "std::borrow::Cow",
        "std::path::Path",
        "std::path::PathBuf",
        "std::sync::atomic::AtomicU32",
        "deno_core::anyhow::Context",
        "deno_core::anyhow::bail",
        "deno_core::error::AnyError",
        "deno_lib::util::net::allocate_random_port",
        "deno_lib::util::result::js_error_downcast_ref",
        "deno_lib::version::otel_runtime_config",
        "deno_runtime::fmt_errors::format_js_error",
        "deno_terminal::colors",
        "denort::desktop::DesktopApi",
    ] {
        prefix = prefix.replace(&format!("use {unused};\n"), "");
    }
    prefix = prefix.replace(
        "  fn create_initial_window(",
        "  #[allow(dead_code)]\n  fn create_initial_window(",
    );
    let helpers = upstream
        .split_once("fn laufey_value_to_desktop_value(")
        .context("missing value bridge")?
        .1
        .split_once("/// Find the embedded data section in this dylib")
        .context("missing bridge boundary")?
        .0;
    let source = format!(
        "{prefix}\nfn laufey_value_to_desktop_value({helpers}\n{}",
        fs::read_to_string(root.join("integration/rt/desktop_tail.rs"))?
    );
    if fs::read_to_string(rt.join("dnr_desktop.rs"))
        .ok()
        .as_deref()
        != Some(&source)
    {
        fs::write(rt.join("dnr_desktop.rs"), source)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../integration/rt/dnr_backend.rs"]
mod dnr_backend;
