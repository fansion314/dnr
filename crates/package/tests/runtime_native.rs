//! Real native loading tests; run with DNR_BIN and --ignored after xtask build.
use dnr_package::{PackOptions, pack};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const ADDON: &str = "node_modules/test-addon/build/Release/addon.node";

fn binary() -> PathBuf {
    PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap()
}

fn compile(path: &Path, value: i32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let c = path.with_extension("c");
    fs::write(&c, format!(r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stddef.h>
#include <stdio.h>
typedef void* napi_env;
typedef void* napi_value;
extern int napi_create_string_utf8(napi_env, const char*, size_t, napi_value*);
__attribute__((visibility("default"))) int addon_value(void) {{ return {value}; }}
__attribute__((visibility("default"))) napi_value napi_register_module_v1(napi_env env,napi_value exports) {{
  Dl_info info;
  if (!dladdr((void*)&napi_register_module_v1, &info)) return exports;
  char text[8192];
  snprintf(text, sizeof(text), "%d|%s", addon_value(), info.dli_fname);
  napi_value result;
  napi_create_string_utf8(env, text, (size_t)-1, &result);
  return result;
}}
"#)).unwrap();
    let mut cc = Command::new("cc");
    cc.args(["-shared", "-fPIC"]);
    if cfg!(target_os = "macos") {
        cc.args(["-undefined", "dynamic_lookup"]);
    } else {
        cc.arg("-ldl");
    }
    assert!(cc.arg(&c).arg("-o").arg(path).status().unwrap().success());
    fs::remove_file(c).unwrap();
}

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    compile(&source.join(ADDON), 42);
    fs::write(
        source.join("node_modules/test-addon/package.json"),
        r#"{"main":"build/Release/addon.node"}"#,
    )
    .unwrap();
    fs::write(
        source.join("main.ts"),
        r#"
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const value = require('test-addon');
if (value !== require('test-addon')) throw Error('repeat load changed');
const extractedRoot = value.split('|')[1].slice(0, -'node_modules/test-addon/build/Release/addon.node'.length);
if ((Deno.statSync(extractedRoot).mode & 0o777) !== 0o700) throw Error('native tmp directory is not private');
if (Deno.args[0] === 'workers') {
  const values = await Promise.all(Array.from({ length: 8 }, () => new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./worker.ts', import.meta.url).href, { type: 'module' });
    worker.onmessage = (event) => { worker.terminate(); resolve(event.data); };
    worker.onerror = (event) => { worker.terminate(); reject(Error(event.message)); };
  })));
  if (values.some(result => result !== value)) throw Error('workers loaded different OS paths');
}
console.log(value);
if (Deno.args[0] === 'exit') Deno.exit(7);
if (Deno.args[0] === 'node-exit') (await import('node:process')).default.exit(9);
if (Deno.args[0] === 'error') throw Error('expected native cleanup test error');
"#,
    )
    .unwrap();
    fs::write(
        source.join("worker.ts"),
        r#"
import { createRequire } from 'node:module';
postMessage(createRequire(import.meta.url)('test-addon'));
"#,
    )
    .unwrap();
    // The same loader is used by Deno FFI for explicit shared library requests.
    fs::copy(source.join(ADDON), source.join("addon.so")).unwrap();
    fs::write(
        source.join("ffi.ts"),
        r#"
const library = Deno.dlopen(new URL('./addon.so', import.meta.url), {
  addon_value: { parameters: [], result: 'i32' },
});
console.log(library.symbols.addon_value());
library.close();
"#,
    )
    .unwrap();
    let dest = temp.path().join("bundle");
    fs::create_dir(&dest).unwrap();
    let package = dest.join("application.dnp");
    pack(&PackOptions {
        directory: source,
        entry: "main.ts".into(),
        output: package.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("native-test".into()),
        force: false,
    })
    .unwrap();
    (temp, package)
}

fn result(output: Output, value: i32) -> PathBuf {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let (actual, path) = stdout
        .trim()
        .split_once('|')
        .expect("addon result with loaded OS path");
    assert_eq!(actual, value.to_string());
    PathBuf::from(path)
}

#[test]
#[ignore = "requires native dnr and C compiler; set DNR_BIN and pass --ignored"]
fn native_addon_parallel_processes_use_isolated_tmp() {
    let (temp, package) = fixture();
    let bin = binary();
    let tmp = temp.path().join("private-tmp");
    fs::create_dir(&tmp).unwrap();
    let children: Vec<_> = (0..6)
        .map(|_| {
            Command::new(&bin)
                .arg(&package)
                .env("TMPDIR", &tmp)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut paths = std::collections::HashSet::new();
    for child in children {
        let path = result(child.wait_with_output().unwrap(), 42);
        assert!(path.starts_with(tmp.canonicalize().unwrap()));
        assert!(path.ends_with(ADDON));
        assert!(!path.exists());
        assert!(paths.insert(path));
    }
    assert!(!package.parent().unwrap().join("node_modules").exists());
    // A different library beside the package must not override packaged bytes.
    compile(&package.parent().unwrap().join(ADDON), 73);
    let path = result(
        Command::new(&bin)
            .arg(&package)
            .env("TMPDIR", &tmp)
            .output()
            .unwrap(),
        42,
    );
    assert!(path.starts_with(tmp.canonicalize().unwrap()));
    assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
}

#[test]
#[ignore = "requires native dnr and C compiler; set DNR_BIN and pass --ignored"]
fn native_workers_and_exit_paths_clean_up() {
    let (temp, package) = fixture();
    let bin = binary();
    let tmp = temp.path().join("private-tmp");
    fs::create_dir(&tmp).unwrap();
    for (mode, code) in [("workers", 0), ("exit", 7), ("node-exit", 9), ("error", 1)] {
        let output = Command::new(&bin)
            .arg(&package)
            .arg(mode)
            .env("TMPDIR", &tmp)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let (value, path) = stdout.trim().split_once('|').unwrap();
        assert_eq!(value, "42");
        assert!(Path::new(path).starts_with(tmp.canonicalize().unwrap()));
        assert!(!Path::new(path).exists());
        assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
    }
    // A broken temp location is an error, never a reason to load a disk override.
    compile(&package.parent().unwrap().join(ADDON), 73);
    let blocked = temp.path().join("not-a-directory");
    fs::write(&blocked, "block").unwrap();
    let output = Command::new(&bin)
        .arg(&package)
        .env("TMPDIR", blocked)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("creating temporary native library directory")
    );
}

#[test]
#[cfg(unix)]
#[ignore = "requires native dnr, non-root user and C compiler; set DNR_BIN and pass --ignored"]
fn native_addon_readonly_directory_and_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, package) = fixture();
    let bin = binary();
    let root = package.parent().unwrap();
    let tmp = temp.path().join("private-tmp");
    fs::create_dir(&tmp).unwrap();
    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let _restore = Restore(root.to_owned());
    fs::set_permissions(root, fs::Permissions::from_mode(0o555)).unwrap();
    assert!(
        fs::write(root.join("write-probe"), "").is_err(),
        "readonly test requires a non-root user"
    );
    let path = result(
        Command::new(&bin)
            .arg(&package)
            .env("TMPDIR", &tmp)
            .output()
            .unwrap(),
        42,
    );
    assert!(path.starts_with(tmp.canonicalize().unwrap()));
    assert!(path.ends_with(ADDON));
    assert!(!root.join("node_modules").exists());
    assert!(
        !path.exists(),
        "temporary libraries are cleaned on normal host exit"
    );
    assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);

    fs::set_permissions(root, fs::Permissions::from_mode(0o755)).unwrap();
    compile(&root.join(ADDON), 73);
    fs::set_permissions(root, fs::Permissions::from_mode(0o555)).unwrap();
    let path = result(
        Command::new(&bin)
            .arg(&package)
            .env("TMPDIR", &tmp)
            .output()
            .unwrap(),
        42,
    );
    assert!(path.starts_with(tmp.canonicalize().unwrap()));
    assert!(!path.exists());
    assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
}

#[test]
#[ignore = "requires native dnr and C compiler; set DNR_BIN and pass --ignored"]
fn packaged_ffi_library_is_extracted() {
    let (temp, package) = fixture();
    pack(&PackOptions {
        directory: temp.path().join("source"),
        entry: "ffi.ts".into(),
        output: package.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: None,
        force: true,
    })
    .unwrap();
    let output = Command::new(binary()).arg(&package).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
    assert!(!package.parent().unwrap().join("addon.so").exists());
}
