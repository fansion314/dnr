//! Native cache regression suite; run with DNR_BIN and --ignored.
use dnr_package::{PackOptions, PackageConfig, pack_with_config};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn success(output: Output) -> (String, String) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}
fn binary() -> PathBuf {
    PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap()
}
fn build(src: &Path, dest: &Path) {
    pack_with_config(
        &PackOptions {
            directory: src.into(),
            output: dest.into(),
            entry: "main.ts".into(),
            includes: vec![],
            excludes: vec![],
            app_id: Some("test.cache".into()),
            force: true,
        },
        &PackageConfig::default(),
    )
    .unwrap();
}
fn run(binary: &Path, package: &Path, cache: &Path, extra: &[&str]) -> (String, String) {
    success(
        Command::new(binary)
            .args(extra)
            .arg(package)
            .env("DNR_CACHE_DIR", cache)
            .env("DNR_CACHE_STATS", "1")
            .output()
            .unwrap(),
    )
}
#[test]
#[ignore = "requires native DNR_BIN"]
fn v3_cache_workers_cjs_extensions_and_full_install() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    let src = root.join("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(src.join("main.ts"), r#"
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
console.log('cjs', require('./value.cjs'));
const worker = new Worker(new URL('./worker.ts', import.meta.url), {type:'module'});
await new Promise<void>((resolve, reject) => { worker.onmessage = e => { console.log('worker', e.data); worker.terminate(); resolve(); }; worker.onerror = reject; });
const { Worker: NodeWorker } = await import('node:worker_threads');
const nodeWorker = new NodeWorker(new URL('./node-worker.cjs', import.meta.url));
await new Promise<void>((resolve, reject) => { nodeWorker.on('message', v => { console.log('node-worker', v); resolve(); }); nodeWorker.on('error', reject); });
await new Promise(resolve => setTimeout(resolve, 5));
console.log('late', (await import('./late.ts')).value);
console.log('extension', (await import('./extension.ts')).value);
console.log('identity', Deno.mainModule);
"#).unwrap();
    fs::write(src.join("value.cjs"), "module.exports = 42;").unwrap();
    fs::write(
        src.join("worker.ts"),
        "const n: number = 43; postMessage(n);",
    )
    .unwrap();
    fs::write(
        src.join("node-worker.cjs"),
        "require('node:worker_threads').parentPort.postMessage(44);",
    )
    .unwrap();
    fs::write(src.join("late.ts"), "export const value: number = 45;").unwrap();
    fs::write(
        root.join("extension.ts"),
        "export const value: number = 46;",
    )
    .unwrap();
    let package = root.join("app.dnp");
    build(&src, &package);
    let binary = binary();
    let cache = root.join("cache");
    let cold = run(&binary, &package, &cache, &[]);
    let warm = run(&binary, &package, &cache, &[]);
    assert_eq!(cold.0, warm.0);
    assert!(
        warm.0
            .contains("cjs 42\nworker 43\nnode-worker 44\nlate 45\nextension 46")
    );
    assert!(warm.1.contains("code_misses=0"), "{}", warm.1);
    assert!(warm.1.contains("emit_misses=0"), "{}", warm.1);
    let old = fs::metadata(root.join("extension.ts")).unwrap();
    fs::write(
        root.join("extension.ts"),
        "export const value: number = 47;",
    )
    .unwrap();
    fs::File::open(root.join("extension.ts"))
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old.modified().unwrap()))
        .unwrap();
    assert!(
        run(&binary, &package, &cache, &[])
            .0
            .contains("extension 47")
    );
    let disabled = run(
        &binary,
        &package,
        &root.join("disabled"),
        &["--no-code-cache", "--no-transpile-cache"],
    );
    assert!(disabled.0.contains("extension 47"));
    assert!(!root.join("disabled").exists());
    let full = root.join("full");
    success(
        Command::new(&binary)
            .args(["install"])
            .arg(&package)
            .arg(&full)
            .args(["--mode", "full"])
            .env("DNR_CACHE_DIR", root.join("install-unused"))
            .output()
            .unwrap(),
    );
    assert!(!root.join("install-unused").exists());
    fs::copy(root.join("extension.ts"), full.join("extension.ts")).unwrap();
    assert!(run(&binary, &full, &cache, &[]).0.contains("extension 47"));
    assert!(run(&binary, &full, &cache, &[]).1.contains("code_misses=0"));
    fs::write(full.join("late.ts"), "export const value: number = 48;").unwrap();
    assert!(run(&binary, &full, &cache, &[]).0.contains("late 48"));
    fs::copy(root.join("extension.ts"), src.join("extension.ts")).unwrap();
    run(
        &binary,
        &src.join("main.ts"),
        &root.join("script-cache"),
        &[],
    );
    assert!(!root.join("script-cache").exists());
}

#[test]
#[ignore = "requires native DNR_BIN"]
fn cache_damage_readonly_and_parallel_startup_are_nonfatal() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("src");
    fs::create_dir(&src).unwrap();
    fs::write(
        src.join("main.ts"),
        "const n: number = 123; console.log(n);",
    )
    .unwrap();
    let package = t.path().join("app");
    build(&src, &package);
    let cache = t.path().join("cache");
    let binary = binary();
    let mut children = Vec::new();
    for _ in 0..4 {
        children.push(
            Command::new(&binary)
                .arg(&package)
                .env("DNR_CACHE_DIR", &cache)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        assert_eq!(success(child.wait_with_output().unwrap()).0, "123\n");
    }
    let mut queue = vec![cache.clone()];
    while let Some(path) = queue.pop() {
        for e in fs::read_dir(path).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                queue.push(e.path());
            } else if e
                .path()
                .parent()
                .unwrap()
                .file_name()
                .is_some_and(|n| n == "code" || n == "emit")
            {
                fs::write(e.path(), "broken").unwrap();
            }
        }
    }
    assert_eq!(run(&binary, &package, &cache, &[]).0, "123\n");
    assert!(
        run(&binary, &package, &cache, &[])
            .1
            .contains("code_misses=0")
    );
    let invalid = t.path().join("not-directory");
    fs::write(&invalid, "readonly").unwrap();
    assert_eq!(run(&binary, &package, &invalid, &[]).0, "123\n");
}
