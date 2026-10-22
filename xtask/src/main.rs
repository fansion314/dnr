mod linux_artifact;
mod provenance;

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

fn apply_upstream_patch(dest: &Path, patch: &Path) -> Result<()> {
    // Git archives exclude .git. Without a local repository, Git
    // discovers dnr's parent checkout and can silently skip every patch path
    // when invoked from this subdirectory, even for --reverse --check.
    run(Command::new("git").args(["init", "--quiet"]).arg(dest))?;
    let already_applied = Command::new("git")
        .current_dir(dest)
        .args(["apply", "--reverse", "--check"])
        .arg(patch)
        .output()?
        .status
        .success();
    if !already_applied {
        run(Command::new("git")
            .current_dir(dest)
            .args(["apply", "--check"])
            .arg(patch))?;
        run(Command::new("git")
            .current_dir(dest)
            .arg("apply")
            .arg(patch))?;
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
            let _lease = provenance::prepare(&root, &[("deno", deno), ("laufey", laufey)])?;
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
            // Check before synchronizing any files or building dnc. Keep the
            // shared lease until compilation finishes so prepare cannot swap
            // either source tree underneath this build.
            let _lease = provenance::verify_for_build(&root)?;
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
                .env("DNR_BACKEND", &backend)
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
                linux_artifact::verify_napi_exports(&root, &runtime_artifact)?;
                linux_artifact::verify_backend_dependencies(&runtime_artifact, &backend)?;
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
        "dnr_zoom_config.rs",
        "dnr_state.rs",
        "dnr_windows.js",
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

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../integration/rt/dnr_zoom_config.rs"]
mod dnr_zoom_config;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../integration/rt/dnr_state.rs"]
mod dnr_state;

#[cfg(test)]
mod prepare_tests {
    use super::*;

    #[test]
    fn applies_inside_parent_checkout_and_is_idempotent() -> Result<()> {
        let temp = tempfile::tempdir()?;
        run(Command::new("git")
            .args(["init", "--quiet"])
            .arg(temp.path()))?;
        let dest = temp.path().join(".upstream/source");
        fs::create_dir_all(&dest)?;
        fs::write(dest.join("value.txt"), "before\n")?;
        let patch = temp.path().join("change.patch");
        fs::write(
            &patch,
            "diff --git a/value.txt b/value.txt\n--- a/value.txt\n+++ b/value.txt\n@@ -1 +1 @@\n-before\n+after\n",
        )?;
        apply_upstream_patch(&dest, &patch)?;
        assert_eq!(fs::read_to_string(dest.join("value.txt"))?, "after\n");
        apply_upstream_patch(&dest, &patch)?;
        assert_eq!(fs::read_to_string(dest.join("value.txt"))?, "after\n");
        Ok(())
    }
}
