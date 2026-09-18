mod desktop;

use clap::Parser;
use dnr_package::{Include, PackOptions, pack};
use std::path::PathBuf;

/// Package a prepared JS/TS application as .dnp or a thin desktop app.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Directory containing the prepared application and its dependencies.
    directory: PathBuf,
    /// Entry module, relative to the application directory.
    #[arg(long, required_unless_present = "desktop_manifest")]
    entry: Option<String>,
    #[arg(short, long)]
    output: PathBuf,
    /// Include an external file/directory: source[=archive/path].
    #[arg(long)]
    include: Vec<String>,
    /// Exclude an archive-relative file or directory (including its descendants).
    #[arg(long)]
    exclude: Vec<String>,
    /// Stable identity used for application storage.
    #[arg(long)]
    app_id: Option<String>,
    #[arg(long)]
    force: bool,
    /// JSON desktop manifest. Relative icon paths are resolved beside this file.
    #[arg(long, requires = "target")]
    desktop_manifest: Option<PathBuf>,
    /// Build a thin macOS .app or Arch/CachyOS .pkg.tar.zst (on the native host).
    #[arg(long, value_enum, requires = "desktop_manifest")]
    target: Option<desktop::Target>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if let Some(manifest) = &args.desktop_manifest {
        return desktop::build(&args, manifest, args.target.unwrap());
    }
    let options = PackOptions {
        directory: args.directory,
        entry: args.entry.unwrap(),
        output: args.output,
        includes: args
            .include
            .iter()
            .map(|s| Include::parse(s))
            .collect::<anyhow::Result<_>>()?,
        excludes: args.exclude,
        app_id: args.app_id,
        force: args.force,
    };
    let report = pack(&options)?;
    eprintln!(
        "{}: {} files, {} bytes → {} bytes",
        options.output.display(),
        report.files,
        report.source_bytes,
        report.package_bytes
    );
    Ok(())
}
