mod desktop;

use clap::Parser;
use dnr_package::{Include, PackOptions, PackageConfig, pack_with_version};
use std::path::PathBuf;

/// Package a prepared JS/TS application as .dnp or a thin desktop app.
#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(long, default_value_t = 3)]
    format_version: u32,
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
    /// Reviewed native groups and platform mappings (JSON).
    #[arg(long)]
    package_config: Option<PathBuf>,
    /// JSON desktop manifest. Relative icon paths are resolved beside this file.
    #[arg(long, requires = "target")]
    desktop_manifest: Option<PathBuf>,
    /// Build a thin macOS .app or Arch/CachyOS .pkg.tar.zst (on the native host).
    #[arg(long, value_enum, requires = "desktop_manifest")]
    target: Option<desktop::Target>,
}

fn main() -> anyhow::Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().is_some_and(|a| a == "cat") {
        anyhow::ensure!(raw.len() == 3, "usage: dnc cat <package> <archive-path>");
        let package = dnr_package::Package::open(std::path::Path::new(&raw[1]), 0)?;
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(&package.read_archive_file(&raw[2])?)?;
        return Ok(());
    }
    if raw.first().is_some_and(|a| a == "inspect") {
        anyhow::ensure!(
            raw.len() == 3 && raw[2] == "--json",
            "usage: dnc inspect <package> --json"
        );
        let package = dnr_package::Package::open(std::path::Path::new(&raw[1]), 0)?;
        println!("{}", serde_json::to_string_pretty(&package.inspect()?)?);
        return Ok(());
    }
    if raw.first().is_some_and(|a| a == "install") {
        return dnr_package::commands::install(&raw[1..], true);
    }
    if raw.first().is_some_and(|a| a == "scan") {
        #[derive(Parser)]
        struct Scan {
            directory: PathBuf,
            #[arg(short, long)]
            output: PathBuf,
            #[arg(long)]
            force: bool,
        }
        let scan =
            Scan::parse_from(std::iter::once("dnc scan".to_owned()).chain(raw.into_iter().skip(1)));
        let (config, notes) = dnr_package::config::scan(&scan.directory)?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(scan.force)
            .create_new(!scan.force)
            .open(&scan.output)?;
        use std::io::Write;
        output.write_all(&serde_json::to_vec_pretty(&config)?)?;
        output.write_all(b"\n")?;
        for note in notes {
            eprintln!("{note}");
        }
        eprintln!(
            "Review {} before passing it to --package-config.",
            scan.output.display()
        );
        return Ok(());
    }
    let args = Args::parse();
    if let Some(manifest) = &args.desktop_manifest {
        return desktop::build(&args, manifest, args.target.unwrap());
    }
    let config = args
        .package_config
        .as_deref()
        .map(PackageConfig::load)
        .transpose()?
        .unwrap_or_default();
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
    let report = pack_with_version(&options, &config, args.format_version)?;
    eprintln!(
        "{}: {} files, {} bytes → {} bytes",
        options.output.display(),
        report.files,
        report.source_bytes,
        report.package_bytes
    );
    Ok(())
}
