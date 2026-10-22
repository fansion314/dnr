//! VFS copy fast paths and async reads require the rebuilt native host.
use dnr_package::{PackOptions, PackageConfig, Region, pack, pack_with_config, zip_offset};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn binary() -> PathBuf {
    PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap()
}

fn run(package: &Path, temp: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .arg("--no-code-cache")
        .arg("--no-transpile-cache")
        .arg(package)
        .args(args)
        .env("DNR_CACHE_DIR", temp.join("cache"))
        .env("DNR_CONFIG_DIR", temp.join("config"))
        .current_dir(temp)
        .output()
        .unwrap()
}

fn success(output: Output, marker: &str) {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(marker));
}

fn options(source: &Path, package: &Path) -> PackOptions {
    PackOptions {
        directory: source.into(),
        entry: "main.ts".into(),
        output: package.into(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("world.fansionia.validation.vfs".into()),
        force: false,
    }
}

fn corrupt_entry(package: &Path, name: &str) {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(package)
        .unwrap();
    let offset = zip_offset(&mut file).unwrap();
    let start = {
        let mut archive =
            zip::ZipArchive::new(Region::new(file.try_clone().unwrap(), offset).unwrap()).unwrap();
        archive.by_name(name).unwrap().data_start().unwrap()
    };
    file.seek(SeekFrom::Start(offset + start)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    byte[0] ^= 0x80;
    file.seek(SeekFrom::Start(offset + start)).unwrap();
    file.write_all(&byte).unwrap();
}

#[test]
#[ignore = "requires a rebuilt DNR_BIN"]
fn recursive_copy_observes_overlay_and_propagates_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("assets")).unwrap();
    fs::create_dir_all(source.join("links")).unwrap();
    fs::write(source.join("assets/value.txt"), "ZIP").unwrap();
    fs::write(source.join("assets/zip-only.txt"), "ZIP_ONLY").unwrap();
    std::os::unix::fs::symlink("../assets/value.txt", source.join("links/value.txt")).unwrap();
    fs::write(source.join("main.ts"), r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
const [destination, mode] = Deno.args;
const source = new URL('./assets', import.meta.url);
const copy = [
  (to: string) => fs.cpSync(source, to, {recursive: true}),
  (to: string) => fs.promises.cp(source, to, {recursive: true}),
];
for (const [index, operation] of copy.entries()) {
  const to = path.join(destination, String(index));
  if (mode === 'corrupt') {
    let error = false;
    try { await operation(to); } catch { error = true; }
    assert(error, 'corrupt ZIP content must fail instead of cloning disk bytes');
    if (fs.existsSync(path.join(to, 'value.txt'))) {
      assert.notEqual(fs.readFileSync(path.join(to, 'value.txt'), 'utf8'), 'DISK');
    }
    continue;
  }
  await operation(to);
  assert.equal(fs.readFileSync(path.join(to, 'value.txt'), 'utf8'), 'ZIP');
  assert.equal(fs.readFileSync(path.join(to, 'zip-only.txt'), 'utf8'), 'ZIP_ONLY');
  if (mode === 'merged') assert.equal(fs.readFileSync(path.join(to, 'disk-only.txt'), 'utf8'), 'DISK_ONLY');
}
if (mode !== 'corrupt') {
  const to = path.join(destination, 'links');
  await fs.promises.cp(new URL('./links', import.meta.url), to, {recursive: true, dereference: true});
  assert.equal(fs.readFileSync(path.join(to, 'value.txt'), 'utf8'), 'ZIP');
  assert.throws(() => fs.copyFileSync(path.join(destination, '0/value.txt'), new URL('./assets/value.txt', import.meta.url)));
}
console.log('DNR_RECURSIVE_COPY_OK');
"#).unwrap();
    for grouped in [false, true] {
        let install = temp
            .path()
            .join(if grouped { "grouped" } else { "ordinary" });
        fs::create_dir_all(&install).unwrap();
        let package = install.join("app.dnp");
        if grouped {
            fs::write(source.join("tool"), "#!/bin/sh\nexit 0\n").unwrap();
            let host = dnr_package::config::Target::host();
            let config: PackageConfig = serde_json::from_value(serde_json::json!({
                "schemaVersion": 1,
                "targets": {host.id(): host},
                "groups": [{"id": "resources", "files": ["main.ts", "assets/**", "links/**", "tool"],
                    "native": {"executables": ["tool"]}}]
            }))
            .unwrap();
            pack_with_config(&options(&source, &package), &config).unwrap();
        } else {
            pack(&options(&source, &package)).unwrap();
        }
        for mode in ["zip-only", "merged", "corrupt"] {
            if mode != "zip-only" {
                fs::create_dir_all(install.join("assets")).unwrap();
                fs::write(install.join("assets/value.txt"), "DISK").unwrap();
                fs::write(install.join("assets/disk-only.txt"), "DISK_ONLY").unwrap();
            }
            if mode == "corrupt" {
                let physical = dnr_package::Package::open(&package, 0)
                    .unwrap()
                    .inspect()
                    .unwrap()["records"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|record| record["path"] == "assets/value.txt")
                    .unwrap()["source"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                corrupt_entry(&package, &physical);
            }
            let destination = temp.path().join(format!("copy-{grouped}-{mode}"));
            success(
                run(
                    &package,
                    temp.path(),
                    &[destination.to_str().unwrap(), mode],
                ),
                "DNR_RECURSIVE_COPY_OK",
            );
        }
    }
}

#[test]
#[ignore = "requires a rebuilt DNR_BIN"]
fn cold_async_reads_allow_timers_and_preserve_handle_positions() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let mut data = vec![0u8; 64 * 1024 * 1024];
    let mut state = 42u32;
    for byte in &mut data {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        *byte = ((state >> 28) as u8) + b'a';
    }
    for (index, byte) in data.iter_mut().take(256).enumerate() {
        *byte = index as u8;
    }
    fs::write(source.join("data.bin"), data).unwrap();
    fs::write(
        source.join("main.ts"),
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
const source = new URL('./data.bin', import.meta.url);
const mode = Deno.args[0];
let ticks = 0;
const timer = setInterval(() => ticks++, 1);
await new Promise(resolve => setTimeout(resolve, 10));
ticks = 0;
if (mode === 'readFile') {
  const [first, second] = await Promise.all([Deno.readFile(source), Deno.readFile(source)]);
  assert.equal(first.length, 64 * 1024 * 1024);
  assert.equal(first[17], 17);
  first[17] = 0;
  assert.equal(second[17], 17);
} else if (mode === 'byob') {
  using file = await Deno.open(source);
  const a = new Uint8Array(16), b = new Uint8Array(16);
  const counts = await Promise.all([file.read(a), file.read(b)]);
  assert.deepEqual(counts, [16, 16]);
  assert.deepEqual([a[0], b[0]].sort((x, y) => x - y), [0, 16]);
  assert.equal(file.seekSync(0, Deno.SeekMode.Current), 32);
  assert.equal(await file.read(a), 16);
  assert.equal(a[0], 32);
} else if (mode === 'at') {
  const fd = fs.openSync(source, 'r');
  const read = (position: number) => new Promise<Uint8Array>((resolve, reject) => {
    const bytes = new Uint8Array(16);
    fs.read(fd, bytes, 0, bytes.length, position, (error, count) => {
      if (error) reject(error); else { assert.equal(count, 16); resolve(bytes); }
    });
  });
  const [a, b] = await Promise.all([read(0), read(16)]);
  assert.equal(a[0], 0); assert.equal(b[0], 16);
  const next = new Uint8Array(1);
  assert.equal(fs.readSync(fd, next), 1);
  assert.equal(next[0], 0, 'positional reads must not advance the shared cursor');
  fs.closeSync(fd);
} else if (mode === 'abort') {
  const controller = new AbortController();
  const pending = Deno.readFile(source, {signal: controller.signal});
  controller.abort();
  await assert.rejects(pending, {name: 'AbortError'});
  const bytes = await Deno.readFile(source);
  assert.equal(bytes[17], 17);
} else throw Error('unknown mode');
clearInterval(timer);
assert(ticks > 0, 'cold ZIP decoding blocked the event loop');
console.log('DNR_ASYNC_ZIP_OK', mode, ticks);
"#,
    )
    .unwrap();
    let package = temp.path().join("app.dnp");
    pack(&options(&source, &package)).unwrap();
    for mode in ["readFile", "byob", "at", "abort"] {
        success(run(&package, temp.path(), &[mode]), "DNR_ASYNC_ZIP_OK");
    }
}

#[test]
#[ignore = "requires a rebuilt DNR_BIN"]
fn failed_zip_read_does_not_advance_file_position() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("data.bin"), "checksummed file contents").unwrap();
    fs::write(
        source.join("main.ts"),
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
const source = new URL('./data.bin', import.meta.url);
using file = await Deno.open(source);
await assert.rejects(file.read(new Uint8Array(4)));
assert.equal(file.seekSync(0, Deno.SeekMode.Current), 0);
assert.throws(() => file.readSync(new Uint8Array(4)));
assert.equal(file.seekSync(0, Deno.SeekMode.Current), 0);
const handle = await fs.promises.open(source);
await assert.rejects(handle.readFile());
assert.throws(() => fs.readSync(handle.fd, new Uint8Array(4)));
await handle.close();
console.log('DNR_FAILED_READ_POSITION_OK');
"#,
    )
    .unwrap();
    let package = temp.path().join("app.dnp");
    pack(&options(&source, &package)).unwrap();
    corrupt_entry(&package, "data.bin");
    success(
        run(&package, temp.path(), &[]),
        "DNR_FAILED_READ_POSITION_OK",
    );
}
