//! Validate the final ELF before publishing it to dist/.
use anyhow::{Context, Result, bail};
use std::{fs, path::Path, process::Command};

pub fn verify_napi_exports(root: &Path, artifact: &Path) -> Result<()> {
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

pub fn verify_backend_dependencies(artifact: &Path, backend: &str) -> Result<()> {
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
    if backend == "dual" && !gui_needed.is_empty() {
        bail!(
            "runtime directly links GUI libraries that must load lazily: {}",
            gui_needed.join(", ")
        );
    }
    if backend != "dual" {
        let cef = gui_needed.contains(&"libcef.so");
        let webview = gui_needed
            .iter()
            .any(|name| name.starts_with("libwebkit2gtk-"));
        if (cef, webview) != (backend == "system-cef", backend == "webview") {
            bail!("{backend} must directly link only its selected GUI engine: {gui_needed:?}");
        }
    }
    Ok(())
}
