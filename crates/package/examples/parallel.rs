//! Use a fixture from scripts/bench-parallel-read.py --keep PATH.
//! cargo run --release -p dnr-package --example parallel -- PATH/app.dnp
use anyhow::{Result, ensure};
use dnr_package::{DEFAULT_CACHE_BYTES, Package};
use std::{hint::black_box, path::PathBuf, sync::Barrier, thread, time::Instant};

fn main() -> Result<()> {
    let path = PathBuf::from(std::env::args_os().nth(1).expect("pass app.dnp"));
    for mode in ["cold", "hot"] {
        for threads in [1, 2, 4, 8] {
            let mut samples = Vec::new();
            for sample in 0..8 {
                let package = Package::open(&path, DEFAULT_CACHE_BYTES)?;
                let indices: Vec<_> = package
                    .entries
                    .values()
                    .filter(|e| {
                        e.name.starts_with("assets/") && e.kind == dnr_package::EntryKind::File
                    })
                    .map(|e| e.index)
                    .collect();
                ensure!(indices.len() >= threads, "not enough assets");
                if mode == "hot" {
                    for &index in &indices {
                        black_box(package.read_index(index)?);
                    }
                }
                let barrier = Barrier::new(threads + 1);
                let elapsed = thread::scope(|scope| -> Result<f64> {
                    let workers: Vec<_> = (0..threads)
                        .map(|id| {
                            let package = &package;
                            let indices = &indices;
                            let barrier = &barrier;
                            scope.spawn(move || -> Result<()> {
                                barrier.wait();
                                if mode == "cold" {
                                    for at in (id..indices.len()).step_by(threads) {
                                        black_box(package.read_index(indices[at])?);
                                    }
                                } else {
                                    for n in 0..200_000 / threads {
                                        black_box(package.read_index(
                                            indices[(n * threads + id) % indices.len()],
                                        )?);
                                    }
                                }
                                Ok(())
                            })
                        })
                        .collect();
                    let start = Instant::now();
                    barrier.wait();
                    for worker in workers {
                        worker.join().unwrap()?;
                    }
                    Ok(start.elapsed().as_secs_f64() * 1000.0)
                })?;
                if sample != 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!("package_{mode}_{threads}_threads_ms={:.3}", samples[3]);
        }
    }
    Ok(())
}
