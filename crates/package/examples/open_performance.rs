//! Isolated Package::open comparison; intentionally never reads application payloads.
use dnr_package::Package;
use std::{path::Path, time::Instant};
fn main() -> anyhow::Result<()> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(paths.len() == 2, "usage: open_performance <v3-a> <v3-b>");
    let mut samples = [Vec::new(), Vec::new()];
    for round in 0..110 {
        for which in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
            let start = Instant::now();
            let package = Package::open(Path::new(&paths[which]), 0)?;
            assert_eq!(package.stats().decompressions, 0);
            if round >= 10 {
                samples[which].push(start.elapsed().as_secs_f64() * 1000.0);
            }
            std::hint::black_box(package);
        }
    }
    for (path, mut values) in paths.iter().zip(samples) {
        values.sort_by(f64::total_cmp);
        println!(
            "{path}: mean_ms={:.4} median_ms={:.4} samples={}",
            values.iter().sum::<f64>() / values.len() as f64,
            (values[49] + values[50]) / 2.0,
            values.len()
        );
    }
    Ok(())
}
