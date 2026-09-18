use clap::Parser;
use dnr_package::{Include, PackOptions, pack};
use std::path::PathBuf;

/// Package a prepared JS/TS application without bundling its runtime.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Directory containing the prepared application and its dependencies.
    directory: PathBuf,
    /// Entry module, relative to the application directory.
    #[arg(long)]
    entry: String,
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
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let options = PackOptions {
        directory: args.directory,
        entry: args.entry,
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
