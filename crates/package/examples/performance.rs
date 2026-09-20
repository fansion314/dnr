//! Reproducible hot-cache microbenchmarks; no timing assertions.
//! cargo run --release -p dnr-package --example performance
//! Add DNR_BIN=/absolute/path/to/dnr for end-to-end startup/readdir timings.
use anyhow::Result;
use dnr_package::{DEFAULT_CACHE_BYTES, MANIFEST, MARKER, Manifest, Package, Region};
use std::{fs, hint::black_box, io::Write, path::Path, process::Command, time::Instant};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn measure(mut f: impl FnMut() -> Result<()>) -> Result<f64> {
    f()?; // Warm filesystem and executable pages before the measured samples.
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start = Instant::now();
        f()?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    Ok(samples[samples.len() / 2])
}

fn fixture(path: &Path, nested: bool) -> Result<()> {
    let header = format!("#!/bin/sh\nexit 127\n{MARKER}");
    let mut file = fs::File::create(path)?;
    file.write_all(header.as_bytes())?;
    let mut zip = ZipWriter::new(Region::new(file, header.len() as u64)?);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Zstd)
        .compression_level(Some(6));
    zip.start_file(MANIFEST, options)?;
    zip.write_all(&serde_json::to_vec(&Manifest {
        format_version: 1,
        targets: Default::default(),
        groups: Default::default(),
        integrity: None,
        entry: "main.js".into(),
        app_id: "dnr.performance".into(),
    })?)?;
    zip.start_file("main.js", options)?;
    zip.write_all(
        br#"
import { readdirSync } from 'node:fs';
if (Deno.args[0] === 'readdir') {
  const root = new URL('./wide/', import.meta.url);
  const start = performance.now();
  for (let n = 0; n < 20; n++) {
    if ([...Deno.readDirSync(root)].length !== 12500) throw Error('Deno merge');
    if (readdirSync(root).length !== 12500) throw Error('Node merge');
  }
  console.log(performance.now() - start);
}
"#,
    )?;
    for n in 0..10_000 {
        let name = if nested {
            format!("node_modules/pkg{:04}/file{}.js", n / 4, n % 4)
        } else {
            format!("wide/f{n:05}")
        };
        zip.start_file(name, options)?;
        zip.write_all(b"export default 42;\n")?;
    }
    zip.finish()?;
    Ok(())
}

fn main() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let nested = temp.path().join("nested.dnp");
    fixture(&nested, true)?;
    println!(
        "open_12502_entries_ms={:.3}",
        measure(|| {
            black_box(Package::open(&nested, DEFAULT_CACHE_BYTES)?);
            Ok(())
        })?
    );
    for count in [100, 10_000] {
        let package = Package::open(&nested, DEFAULT_CACHE_BYTES)?;
        let indices: Vec<_> = package
            .entries
            .values()
            .filter(|e| e.kind == dnr_package::EntryKind::File)
            .take(count)
            .map(|e| e.index)
            .collect();
        for &index in &indices {
            black_box(package.read_index(index)?);
        }
        println!(
            "cache_{count}_20000_hits_ms={:.3}",
            measure(|| {
                for n in 0..20_000 {
                    black_box(package.read_index(indices[n % count])?);
                }
                Ok(())
            })?
        );
    }
    if let Some(binary) = std::env::var_os("DNR_BIN") {
        let binary = fs::canonicalize(binary)?;
        println!(
            "runtime_nested_startup_ms={:.3}",
            measure(|| {
                let output = Command::new(&binary).arg(&nested).output()?;
                anyhow::ensure!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            })?
        );
        // 10k ZIP files, 5k disk files: 2.5k overlap, 2.5k disk-only.
        let wide = temp.path().join("wide.dnp");
        fixture(&wide, false)?;
        fs::create_dir(temp.path().join("wide"))?;
        for n in 7500..12_500 {
            fs::write(temp.path().join(format!("wide/f{n:05}")), "disk")?;
        }
        let mut times = Vec::new();
        for _ in 0..8 {
            let output = Command::new(&binary).arg(&wide).arg("readdir").output()?;
            anyhow::ensure!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            times.push(String::from_utf8(output.stdout)?.trim().parse::<f64>()?);
        }
        times.remove(0);
        times.sort_by(f64::total_cmp);
        println!("runtime_40_merged_readdir_ms={:.3}", times[3]);
    }
    Ok(())
}
