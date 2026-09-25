//! Comparable native-group microbenchmarks; fixture creation is outside timers.
use anyhow::{Result, ensure};
use dnr_package::{PackOptions, Package, PackageConfig, pack, pack_with_config};
use std::{
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};

fn median(mut f: impl FnMut(usize) -> Result<()>, samples: usize) -> Result<f64> {
    let mut times = Vec::new();
    for i in 0..samples {
        let start = Instant::now();
        f(i)?;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    Ok(times[times.len() / 2])
}
fn options(directory: &Path, output: &Path) -> PackOptions {
    PackOptions {
        directory: directory.into(),
        entry: "main.ts".into(),
        output: output.into(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("dnr.native-benchmark".into()),
        force: true,
    }
}
fn package(source: &Path, output: &Path, groups: Vec<serde_json::Value>) -> Result<()> {
    let host = dnr_package::config::Target::host();
    let config: PackageConfig = serde_json::from_value(
        serde_json::json!({"schemaVersion":1,"targets":if groups.is_empty(){serde_json::json!({})}else{serde_json::json!({host.id():host})},"groups":groups}),
    )?;
    pack_with_config(&options(source, output), &config)?;
    Ok(())
}
fn fixture(base: &Path, group_count: usize) -> Result<PathBuf> {
    let source = base.join(format!("source-{group_count}"));
    fs::create_dir_all(source.join("plain"))?;
    fs::write(
        source.join("main.ts"),
        r#"
const count=Number(Deno.args[0]||0);
for(let n=0;n<count;n++) await import(`./groups/g${String(n).padStart(4,'0')}/wrapper.js`);
if(Deno.args[1]==='stat') {
  const paths=Array.from({length:100},(_,n)=>new URL(`./plain/f${String(n).padStart(5,'0')}.js`,import.meta.url));
  const start=performance.now(); for(let n=0;n<20000;n++) Deno.statSync(paths[n%100]);
  console.log(performance.now()-start);
}
"#,
    )?;
    for n in 0..10000 {
        fs::write(
            source.join(format!("plain/f{n:05}.js")),
            "export default 42;",
        )?;
    }
    let mut groups = Vec::new();
    for n in 0..group_count {
        let name = format!("g{n:04}");
        let prefix = format!("groups/{name}");
        fs::create_dir_all(source.join(&prefix))?;
        fs::write(source.join(&prefix).join("wrapper.js"), "export default 1;")?;
        fs::write(
            source.join(&prefix).join("addon.node"),
            "benchmark-native-payload",
        )?;
        groups.push(serde_json::json!({"id":name,"files":[format!("{prefix}/**")],"native":{"addons":[{"path":format!("{prefix}/addon.node"),"napi":8}]}}));
    }
    let output = base.join(format!("groups-{group_count}.dnp"));
    package(&source, &output, groups)?;
    if group_count == 0 {
        pack(&options(&source, &base.join("plain-v3.dnp")))?;
    }
    Ok(output)
}
fn extraction_fixture(base: &Path, name: &str, files: usize, size: usize) -> Result<PathBuf> {
    let source = base.join(name);
    fs::create_dir_all(source.join("native"))?;
    fs::write(source.join("main.ts"), "")?;
    fs::write(source.join("native/wrapper.js"), "export default 1;")?;
    let mut seed = 0x12345678u32;
    let bytes: Vec<_> = (0..size)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            b'a' + (seed & 15) as u8
        })
        .collect();
    fs::write(source.join("native/addon.node"), &bytes)?;
    for n in 1..files {
        let folder = source.join(format!("native/data/d{:03}", n / 32));
        fs::create_dir_all(&folder)?;
        fs::write(folder.join(format!("f{n}")), &bytes)?;
    }
    let output = base.join(format!("{name}.dnp"));
    package(
        &source,
        &output,
        vec![
            serde_json::json!({"id":"native","files":["native/**"],"native":{"addons":[{"path":"native/addon.node","napi":8}]}}),
        ],
    )?;
    Ok(output)
}
fn open(path: &Path, cache: &Path) -> Result<Package> {
    Package::open(path, 0)?.with_cache_directory(cache)
}
fn main() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_owned());
    fs::create_dir_all(&base)?;
    let base = base.canonicalize()?;
    let mut metrics = serde_json::Map::new();
    for count in [0, 1, 100, 1000] {
        let path = fixture(&base, count)?;
        let cache = base.join("lookup-cache");
        metrics.insert(
            format!("open_{count}_groups_ms"),
            median(
                |_| {
                    black_box(open(&path, &cache)?);
                    Ok(())
                },
                5,
            )?
            .into(),
        );
        let package = open(&path, &cache)?;
        let mut last = None;
        for n in 0..count {
            last = package.module_path(&format!("groups/g{n:04}/wrapper.js"))?;
        }
        let names: Vec<_> = (0..100).map(|n| format!("plain/f{n:05}.js")).collect();
        metrics.insert(
            format!("plain_100k_lookup_{count}_groups_ms"),
            median(
                |_| {
                    for n in 0..100000 {
                        ensure!(
                            black_box(package.module_path(&names[n % 100])?).is_none(),
                            "plain file acquired a group"
                        );
                    }
                    Ok(())
                },
                5,
            )?
            .into(),
        );
        let plain = package.root.join(&names[0]);
        metrics.insert(
            format!("plain_10k_reverse_{count}_groups_ms"),
            median(
                |_| {
                    for _ in 0..10000 {
                        black_box(package.logical_path(&plain));
                    }
                    Ok(())
                },
                5,
            )?
            .into(),
        );
        if let Some(last) = last {
            let name = format!("groups/g{:04}/wrapper.js", count - 1);
            metrics.insert(
                format!("group_10k_lookup_{count}_groups_ms"),
                median(
                    |_| {
                        for _ in 0..10000 {
                            black_box(package.module_path(&name)?);
                        }
                        Ok(())
                    },
                    5,
                )?
                .into(),
            );
            metrics.insert(
                format!("group_10k_reverse_{count}_groups_ms"),
                median(
                    |_| {
                        for _ in 0..10000 {
                            black_box(package.logical_path(&last));
                        }
                        Ok(())
                    },
                    5,
                )?
                .into(),
            );
        }
    }
    metrics.insert(
        "open_plain_v3_ms".into(),
        median(
            |_| {
                black_box(Package::open(&base.join("plain-v3.dnp"), 0)?);
                Ok(())
            },
            5,
        )?
        .into(),
    );
    for (name, files, size) in [("large", 1, 32 * 1024 * 1024), ("many", 1000, 256)] {
        let path = extraction_fixture(&base, name, files, size)?;
        let cold: Vec<_> = (0..3)
            .map(|n| open(&path, &base.join(format!("{name}-cold-{n}"))))
            .collect::<Result<_>>()?;
        metrics.insert(
            format!("{name}_cold_ms"),
            median(
                |n| {
                    black_box(cold[n].native_library_path("native/addon.node")?);
                    Ok(())
                },
                3,
            )?
            .into(),
        );
        let cache = base.join(format!("{name}-warm"));
        drop(open(&path, &cache)?.native_library_path("native/addon.node")?);
        let warm: Vec<_> = (0..5).map(|_| open(&path, &cache)).collect::<Result<_>>()?;
        metrics.insert(
            format!("{name}_warm_ms"),
            median(
                |n| {
                    black_box(warm[n].native_library_path("native/addon.node")?);
                    Ok(())
                },
                5,
            )?
            .into(),
        );
        metrics.insert(
            format!("{name}_10k_reuse_ms"),
            median(
                |_| {
                    for _ in 0..10000 {
                        black_box(warm[0].native_library_path("native/addon.node")?);
                    }
                    Ok(())
                },
                5,
            )?
            .into(),
        );
        let install = base.join(format!("{name}-installed"));
        dnr_package::commands::run(&[
            "install".into(),
            path.display().to_string(),
            install.display().to_string(),
        ])?;
        let installed = install.join(path.file_name().unwrap());
        let adjacent: Vec<_> = (0..5)
            .map(|_| open(&installed, &base.join("unused-cache")))
            .collect::<Result<_>>()?;
        metrics.insert(
            format!("{name}_sidecar_ms"),
            median(
                |n| {
                    black_box(adjacent[n].native_library_path("native/addon.node")?);
                    Ok(())
                },
                5,
            )?
            .into(),
        );
    }
    fs::write(
        base.join("results.json"),
        serde_json::to_vec_pretty(&metrics)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&metrics)?);
    Ok(())
}
