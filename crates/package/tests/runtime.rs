//! Run explicitly after building the native host:
//! DNR_BIN=dist/dnr cargo test -p dnr-package --test runtime -- --ignored
use dnr_package::{PackOptions, pack};
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires native dnr and a C compiler; set DNR_BIN and pass --ignored"]
fn native_addon_disk_only() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let c = temp.path().join("addon.c");
    fs::write(&c, "typedef void* napi_env; typedef void* napi_value; extern int napi_create_int32(napi_env,int,napi_value*); __attribute__((visibility(\"default\"))) napi_value napi_register_module_v1(napi_env env,napi_value exports){ napi_value value; napi_create_int32(env,42,&value); return value; }\n").unwrap();
    let addon = source.join("addon.node");
    let mut cc = Command::new("cc");
    cc.args(["-shared", "-fPIC"]);
    if cfg!(target_os = "macos") {
        cc.args(["-undefined", "dynamic_lookup"]);
    }
    assert!(cc.arg(&c).arg("-o").arg(&addon).status().unwrap().success());
    fs::write(source.join("main.ts"), "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url); console.log(require('./addon.node'));\n").unwrap();
    let disk = Command::new(&binary)
        .arg(source.join("main.ts"))
        .output()
        .unwrap();
    assert!(
        disk.status.success(),
        "{}",
        String::from_utf8_lossy(&disk.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&disk.stdout).trim(), "42");
    let package = temp.path().join("app.dnp");
    let mut opts = PackOptions {
        directory: source,
        entry: "main.ts".into(),
        output: package.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("test.addon".into()),
        force: false,
    };
    pack(&opts).unwrap();
    let embedded = Command::new(&binary).arg(&package).output().unwrap();
    assert!(!embedded.status.success());
    assert!(
        String::from_utf8_lossy(&embedded.stderr).contains("does not extract native libraries"),
        "{}",
        String::from_utf8_lossy(&embedded.stderr)
    );
    opts.excludes.push("addon.node".into());
    opts.force = true;
    pack(&opts).unwrap();
    fs::copy(addon, temp.path().join("addon.node")).unwrap();
    let external = Command::new(&binary).arg(package).output().unwrap();
    assert!(
        external.status.success(),
        "{}",
        String::from_utf8_lossy(&external.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&external.stdout).trim(), "42");
}

#[test]
#[ignore = "requires a built native dnr; set DNR_BIN and pass --ignored"]
fn local_and_packaged_runtime() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    let install = temporary.path().join("installed");
    let cwd = temporary.path().join("caller");
    fs::create_dir_all(source.join("node_modules/demo-lib")).unwrap();
    fs::create_dir_all(&install).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    fs::write(
        source.join("deno.json"),
        r##"{"imports":{"#local":"./value.ts"}}"##,
    )
    .unwrap();
    fs::write(
        source.join("package.json"),
        r#"{"name":"dnr-test","type":"module"}"#,
    )
    .unwrap();
    fs::write(
        source.join("node_modules/demo-lib/package.json"),
        r#"{"name":"demo-lib","exports":"./index.cjs"}"#,
    )
    .unwrap();
    fs::write(
        source.join("node_modules/demo-lib/index.cjs"),
        "exports.value = 42;\n",
    )
    .unwrap();
    fs::write(
        source.join("value.ts"),
        "export const value: number = 42;\n",
    )
    .unwrap();
    fs::write(
        source.join("worker.ts"),
        "self.onmessage = () => { self.postMessage(42); self.close(); };\n",
    )
    .unwrap();
    fs::write(source.join("message.txt"), "archive wins").unwrap();
    fs::write(source.join("unused.txt"), "never read".repeat(200_000)).unwrap();
    fs::write(source.join("main.ts"), r##"
import { value } from '#local';
import { value as npmValue } from 'demo-lib';
import * as fs from 'node:fs';
function assert(ok: unknown, text: string) { if (!ok) throw new Error(text); }
assert(value === 42 && npmValue === 42, 'TS, import map, and CJS npm exports');
const file = new URL('./message.txt', import.meta.url);
assert(await Deno.readTextFile(file) === 'archive wins', 'Deno reads archive');
assert(fs.readFileSync(file, 'utf8') === 'archive wins', 'Node reads same archive');
const handle = await Deno.open(file);
assert(await handle.seek(-4, Deno.SeekMode.End) === 8, 'negative seek from end');
const tail = new Uint8Array(4);
assert(await handle.read(tail) === 4 && new TextDecoder().decode(tail) === 'wins', 'seeked read');
assert(await handle.seek(3, Deno.SeekMode.End) === 15, 'seek past end');
assert(await handle.read(tail) === null, 'EOF beyond end');
handle.close();
assert(await Deno.readTextFile(new URL('./disk-only.txt', import.meta.url)) === 'disk fallback', 'disk fallback');
assert(fs.readdirSync(new URL('./', import.meta.url)).includes('disk-only.txt'), 'merged readdir');
assert(Deno.cwd() === Deno.env.get('EXPECTED_CWD'), 'caller cwd preserved');
const caller = Deno.cwd();
Deno.chdir(new URL('./', import.meta.url));
assert(Deno.cwd() !== caller, 'explicit chdir to a real directory works');
Deno.chdir(caller);
if (Deno.env.get('PACKAGED') === '1') {
  let rejected = false;
  try { Deno.chdir(new URL('./node_modules/demo-lib/', import.meta.url)); } catch { rejected = true; }
  assert(rejected, 'ZIP-only directories are not OS working directories');
}
assert(JSON.stringify(Deno.args) === JSON.stringify(['hello world', '', '--literal']), 'argv');
if (Deno.env.get('PACKAGED') === '1') {
  let rejected = false;
  try { await Deno.writeTextFile(file, 'bad'); } catch { rejected = true; }
  assert(rejected, 'ZIP is readonly');
  rejected = false;
  try { fs.openSync(file, 'w'); } catch { rejected = true; }
  assert(rejected, 'Node cannot truncate ZIP entry');
}
await Deno.writeTextFile('created.txt', 'caller output');
const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
const answer = await new Promise((resolve, reject) => {
  worker.onmessage = e => resolve(e.data);
  worker.onerror = e => reject(new Error(e.message));
  worker.postMessage('go');
});
worker.terminate();
assert(answer === 42, 'worker module source');
console.log('DNR_RUNTIME_OK');
"##).unwrap();
    let package = install.join("test.dnp");
    pack(&PackOptions {
        directory: source.clone(),
        entry: "main.ts".into(),
        output: package.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("dnr.integration".into()),
        force: false,
    })
    .unwrap();
    fs::write(source.join("disk-only.txt"), "disk fallback").unwrap();
    fs::write(install.join("disk-only.txt"), "disk fallback").unwrap();
    fs::write(install.join("message.txt"), "disk must lose").unwrap();
    for (entry, packaged) in [(source.join("main.ts"), false), (package, true)] {
        let out = Command::new(&binary)
            .arg(entry)
            .args(["hello world", "", "--literal"])
            .current_dir(&cwd)
            .env("EXPECTED_CWD", cwd.canonicalize().unwrap())
            .env("PACKAGED", if packaged { "1" } else { "0" })
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "packaged={packaged}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("DNR_RUNTIME_OK"));
        assert_eq!(
            fs::read_to_string(cwd.join("created.txt")).unwrap(),
            "caller output"
        );
    }
}

#[test]
#[ignore = "requires a built native dnr; set DNR_BIN and pass --ignored"]
fn typescript_script_without_module_syntax() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("plain");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("a# b' entry.ts"),
        "const answer: number = 42; console.log(answer);\n",
    )
    .unwrap();
    let package = temporary.path().join("plain.dnp");
    pack(&PackOptions {
        directory: source.clone(),
        entry: "a# b' entry.ts".into(),
        output: package.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("test.plain".into()),
        force: false,
    })
    .unwrap();
    for entry in [source.join("a# b' entry.ts"), package] {
        let output = Command::new(&binary).arg(entry).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
    }
}

#[test]
#[ignore = "requires a built native dnr; set DNR_BIN and pass --ignored"]
fn cli_server_and_errors_do_not_start_gui() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let script = temporary.path().join("server.ts");
    fs::write(&script, "const server = Deno.serve({ port: 0, hostname: '127.0.0.1' }, () => new Response('ok')); setTimeout(() => server.shutdown(), 50);\n").unwrap();
    let output = Command::new(&binary).arg(&script).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Runtime started"));
    fs::write(
        &script,
        "setTimeout(() => { throw new Error('DNR_EXPECTED_ERROR'); }, 0);\n",
    )
    .unwrap();
    let output = Command::new(&binary).arg(&script).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("DNR_EXPECTED_ERROR"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Runtime started"));
    fs::write(&script, "import process from 'node:process'; process.once('beforeExit', () => { setTimeout(() => console.log('BEFORE_EXIT_OK'), 0); });\n").unwrap();
    let output = Command::new(&binary).arg(&script).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("BEFORE_EXIT_OK"));
}
