//! Real Node-API, dependent shared libraries, process paths and persistent groups.
use dnr_package::{PackOptions, PackageConfig, pack_with_config};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn binary() -> PathBuf {
    PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap()
}
fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn compile(source: &Path, destination: &Path, flags: &[&str]) {
    let mut cc = Command::new("cc");
    cc.args(flags).arg(source).arg("-o").arg(destination);
    successful(cc.output().unwrap());
    fs::remove_file(source).unwrap();
}
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let group = source.join("node_modules/native-fixture");
    fs::create_dir_all(&group).unwrap();
    let libname = if cfg!(target_os = "macos") {
        "libsupport.dylib"
    } else {
        "libsupport.so"
    };
    fs::write(group.join("support.c"), "int support(void) { return 42; }").unwrap();
    let mut flags = vec!["-shared", "-fPIC"];
    if cfg!(target_os = "macos") {
        flags.push("-Wl,-install_name,@loader_path/libsupport.dylib");
    } else {
        flags.push("-Wl,-soname,libsupport.so");
    }
    compile(&group.join("support.c"), &group.join(libname), &flags);
    fs::write(
        group.join("addon.c"),
        r#"
typedef void* napi_env; typedef void* napi_value;
extern int napi_create_int32(napi_env,int,napi_value*);
extern int support(void);
int addon_value(void) { return support(); }
napi_value napi_register_module_v1(napi_env env, napi_value exports) {
  napi_value result; napi_create_int32(env, support(), &result); return result;
}
"#,
    )
    .unwrap();
    let libpath = group.join(libname);
    let libpath = libpath.to_str().unwrap();
    let mut flags = vec!["-shared", "-fPIC", libpath];
    if cfg!(target_os = "macos") {
        flags.extend(["-undefined", "dynamic_lookup"]);
    } else {
        flags.push("-Wl,-rpath,$ORIGIN");
    }
    compile(&group.join("addon.c"), &group.join("addon.node"), &flags);
    fs::write(
        group.join("tool.c"),
        r#"
#include <stdio.h>
extern int support(void);
int main(int argc,char** argv) {
  if(argc!=2) return 2;
  FILE* file=fopen(argv[1],"r"); if(!file) return 3;
  char buffer[128]; if(!fgets(buffer,sizeof(buffer),file)) return 4;
  fclose(file); printf("%d:%s",support(),buffer); return 0;
}
"#,
    )
    .unwrap();
    let mut flags = vec![libpath];
    if cfg!(target_os = "linux") {
        flags.push("-Wl,-rpath,$ORIGIN");
    }
    compile(&group.join("tool.c"), &group.join("tool"), &flags);
    fs::write(group.join("data.txt"), "group-resource").unwrap();
    fs::write(
        group.join("package.json"),
        r#"{"name":"native-fixture","main":"index.cjs"}"#,
    )
    .unwrap();
    fs::write(group.join("index.cjs"), r#"
const path = require('node:path');
exports.paths = { binary: path.join(__dirname,'tool'), data: path.join(__dirname,'data.txt'), addon: path.join(__dirname,'addon.node'), cwd: __dirname };
exports.load = () => require('./addon.node');
"#).unwrap();
    fs::write(source.join("main.ts"), r#"
import {createRequire} from 'node:module';
import {execFileSync, spawn, spawnSync} from 'node:child_process';
const fixture = createRequire(import.meta.url)('native-fixture');
const p = fixture.paths;
const fallback = p.cwd + '/disk-only.txt';
if(Deno.readTextFileSync(fallback)!=='disk fallback') throw Error('group disk fallback failed');
Deno.writeTextFileSync(p.cwd + '/created.txt','disk write');
if((await Deno.readTextFile(fallback))!=='disk fallback') throw Error('async fallback failed');
await Deno.writeTextFile(p.cwd + '/async-created.txt','async disk write');
if((Deno.statSync(p.binary).mode & 0o111)===0) throw Error('VFS lost execute permission');
if(Deno.readTextFileSync(p.data)!=='group-resource') throw Error('VFS alias read failed');
if(Deno.args[0]==='read') { console.log(JSON.stringify(p)); Deno.exit(0); }
const expect = (bytes) => { if(new TextDecoder().decode(bytes)!=='42:group-resource') throw Error('child resource mismatch'); };
expect(new Deno.Command(p.binary,{args:[p.data],cwd:p.cwd}).outputSync().stdout);
expect((await new Deno.Command(p.binary,{args:[p.data],cwd:p.cwd}).output()).stdout);
expect(execFileSync(p.binary,[p.data],{cwd:p.cwd}));
expect(spawnSync(p.binary,[p.data],{cwd:p.cwd}).stdout);
await new Promise((resolve,reject)=>{
  const child=spawn(p.binary,[p.data],{cwd:p.cwd}); let out='';
  child.stdout.on('data',b=>out+=b); child.on('error',reject);
  child.on('close',code=>code===0&&out==='42:group-resource'?resolve():reject(Error('spawn failed')));
});
if(fixture.load()!==42) throw Error('Node-API failed');
const library=Deno.dlopen(p.addon,{addon_value:{parameters:[],result:'i32'}});
if(library.symbols.addon_value()!==42) throw Error('FFI failed'); library.close();
try { Deno.writeTextFileSync(p.data,'bad'); throw Error('group became writable'); } catch(e) { if(e.message==='group became writable') throw e; }
console.log(JSON.stringify(p));
"#).unwrap();
    let host = dnr_package::config::Target::host();
    let config: PackageConfig = serde_json::from_value(serde_json::json!({"schemaVersion":1,"targets":{host.id():host},"groups":[{
        "id":"fixture","files":["node_modules/native-fixture/**"],
        "native":{"addons":[{"path":"node_modules/native-fixture/addon.node","napi":8}],"executables":["node_modules/native-fixture/tool"],"libraries":[format!("node_modules/native-fixture/{libname}")]}
    }]})).unwrap();
    let package = temp.path().join("application.dnp");
    pack_with_config(
        &PackOptions {
            directory: source,
            entry: "main.ts".into(),
            output: package.clone(),
            includes: vec![],
            excludes: vec![],
            app_id: Some("test.runtime-groups".into()),
            force: false,
        },
        &config,
    )
    .unwrap();
    let fallback = temp.path().join("node_modules/native-fixture");
    fs::create_dir_all(&fallback).unwrap();
    fs::write(fallback.join("disk-only.txt"), "disk fallback").unwrap();
    (temp, package)
}
fn run(package: &Path, cache: &Path, arguments: &[&str]) -> Output {
    Command::new(binary())
        .arg(package)
        .args(arguments)
        .env("DNR_CACHE_DIR", cache)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

#[test]
#[ignore = "requires a rebuilt DNR_BIN"]
fn path_search_respects_virtual_nodes_permissions_and_native_declarations() {
    use std::{
        io::{Read, Seek, SeekFrom, Write},
        os::unix::fs::PermissionsExt,
    };
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    for directory in [
        "skip-file",
        "skip-dir/audit-tool",
        "skip-link",
        "blocked",
        "native",
        "native-link",
    ] {
        fs::create_dir_all(source.join(directory)).unwrap();
    }
    fs::write(source.join("skip-file/audit-tool"), "ordinary resource").unwrap();
    fs::write(source.join("not-a-directory"), "ordinary resource").unwrap();
    std::os::unix::fs::symlink("not-a-directory", source.join("not-a-directory-link")).unwrap();
    fs::write(
        source.join("blocked/audit-tool.mjs"),
        "#!/bin/sh\nprintf UNDECLARED\n",
    )
    .unwrap();
    fs::set_permissions(
        source.join("blocked/audit-tool.mjs"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(
        source.join("native/audit-tool"),
        "#!/bin/sh\nprintf NATIVE_OK\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        "../skip-file/audit-tool",
        source.join("skip-link/audit-tool"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        "../native/audit-tool",
        source.join("native-link/audit-tool"),
    )
    .unwrap();
    fs::write(source.join("worker.mjs"), r#"
onmessage = async ({data: path}) => {
  try {
    const output = await new Deno.Command('audit-tool', {env: {PATH: path}, stdout: 'piped'}).output();
    postMessage({code: output.code, text: new TextDecoder().decode(output.stdout)});
  } catch (error) { postMessage({error: String(error)}); }
};
"#).unwrap();
    fs::write(source.join("main.mjs"), r#"
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const here = name => new URL('./' + name + '/', import.meta.url).pathname;
const external = Deno.args[0];
const path = names => [...names.map(here), external].join(':');
const output = async (value, command = 'audit-tool') => {
  const result = await new Deno.Command(command, {env: {PATH: value}, stdout: 'piped'}).output();
  assert.equal(result.code, 0);
  return new TextDecoder().decode(result.stdout);
};
const rejects = async (action, pattern) => {
  await assert.rejects(async () => await action(), pattern);
};
if (Deno.args[1] === 'damaged') {
  await rejects(() => output(path(['native'])), /./);
} else {
  const skipped = path(['not-a-directory', 'not-a-directory-link', 'skip-file', 'skip-dir', 'skip-link']);
  assert.equal(await output(skipped), 'DISK_OK');
  const node = spawnSync('audit-tool', {env: {PATH: skipped}, encoding: 'utf8'});
  assert.equal(node.status, 0); assert.equal(node.stdout, 'DISK_OK');
  assert.equal(await output(path(['disk-only'])), 'DISK_ONLY_OK');
  assert.equal(await output(external + ':' + here('blocked'), 'audit-tool.mjs'), 'DISK_OK');
  await rejects(() => output(path(['blocked']), 'audit-tool.mjs'), /undeclared packaged native/);
  await rejects(() => new Deno.Command(here('skip-file') + 'audit-tool').output(), /undeclared packaged native/);
  await rejects(() => new Deno.Command(here('not-a-directory') + 'audit-tool').output(), /not a directory/);
  await rejects(() => output(path(['foreign-bin'])), /unavailable/);
  assert.equal(await output(path(['native-link'])), 'NATIVE_OK');
  const worker = new Worker(new URL('./worker.mjs', import.meta.url).href, {type: 'module'});
  const response = new Promise((resolve, reject) => { worker.onmessage = ({data}) => resolve(data); worker.onerror = reject; });
  worker.postMessage(skipped);
  try { assert.deepEqual(await response, {code: 0, text: 'DISK_OK'}); } finally { worker.terminate(); }
}
console.log('DNR_PATH_OK');
"#).unwrap();
    let host = dnr_package::config::Target::host();
    let foreign: dnr_package::config::Target =
        serde_json::from_value(if cfg!(target_os = "macos") {
            serde_json::json!({"os":"linux","arch":"x64","libc":"glibc"})
        } else {
            serde_json::json!({"os":"darwin","arch":"arm64"})
        })
        .unwrap();
    fs::write(temp.path().join("foreign-tool"), "#!/bin/sh\nexit 0\n").unwrap();
    let mut config: PackageConfig = serde_json::from_value(serde_json::json!({
        "schemaVersion": 1, "targets": {host.id():host, foreign.id():foreign},
        "groups": [
            {"id":"tool", "files":["native/**", "native-link/**"], "native":{"executables":["native/audit-tool"]}},
            {"id":"foreign", "variants":{foreign.id():[{"from":"foreign-tool","to":"foreign-bin/audit-tool"}]}, "native":{"executables":["foreign-bin/audit-tool"]}}
        ]
    })).unwrap();
    config.base_dir = temp.path().to_owned();
    let package = temp.path().join("application.dnp");
    pack_with_config(
        &PackOptions {
            directory: source,
            entry: "main.mjs".into(),
            output: package.clone(),
            includes: vec![],
            excludes: vec![],
            app_id: Some("test.runtime-path".into()),
            force: false,
        },
        &config,
    )
    .unwrap();
    let external = temp.path().join("external");
    for directory in [
        "external",
        "skip-file",
        "skip-dir",
        "skip-link",
        "disk-only",
        "foreign-bin",
        "native",
        "not-a-directory",
        "not-a-directory-link",
    ] {
        fs::create_dir_all(temp.path().join(directory)).unwrap();
        let text = if directory == "external" {
            "DISK_OK"
        } else if directory == "disk-only" {
            "DISK_ONLY_OK"
        } else {
            "WRONG_SHADOW"
        };
        let file = temp.path().join(directory).join("audit-tool");
        fs::write(&file, format!("#!/bin/sh\nprintf {text}\n")).unwrap();
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::copy(external.join("audit-tool"), external.join("audit-tool.mjs")).unwrap();
    let arguments = [external.to_str().unwrap()];
    assert!(
        successful(run(&package, &temp.path().join("cache"), &arguments)).contains("DNR_PATH_OK")
    );

    // Corrupt the selected group's ZIP bytes; even an executable disk shadow
    // and a later PATH candidate must not turn a failed native load into success.
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&package)
        .unwrap();
    let offset = dnr_package::zip_offset(&mut file).unwrap();
    let source = dnr_package::Package::open(&package, 0)
        .unwrap()
        .inspect()
        .unwrap()["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["path"] == "native/audit-tool")
        .unwrap()["source"]
        .as_str()
        .unwrap()
        .to_owned();
    let position = {
        let mut zip = zip::ZipArchive::new(
            dnr_package::Region::new(file.try_clone().unwrap(), offset).unwrap(),
        )
        .unwrap();
        let data_start = zip.by_name(&source).unwrap().data_start().unwrap();
        data_start + offset
    };
    file.seek(SeekFrom::Start(position)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    byte[0] ^= 0xff;
    file.seek(SeekFrom::Start(position)).unwrap();
    file.write_all(&byte).unwrap();
    assert!(
        successful(run(
            &package,
            &temp.path().join("damaged-cache"),
            &[external.to_str().unwrap(), "damaged"]
        ))
        .contains("DNR_PATH_OK")
    );
}

fn native_directory(package: &Path, cache: &Path) -> PathBuf {
    let package = dnr_package::Package::open(package, 0).unwrap();
    let path_hash = dnr_package::persistent::hash(package.source_path.to_str().unwrap().as_bytes());
    cache
        .join("v4")
        .join(path_hash)
        .join("generations")
        .join(package.package_id())
        .join("native")
}

#[test]
#[ignore = "requires a rebuilt DNR_BIN and C compiler"]
fn native_group_paths_subprocesses_and_warm_reuse() {
    let (temp, package) = fixture();
    let cache = temp.path().join("cache");
    let paths: serde_json::Value =
        serde_json::from_str(successful(run(&package, &cache, &["read"])).trim()).unwrap();
    assert!(
        !native_directory(&package, &cache).exists(),
        "ordinary reads must not materialize the group"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("node_modules/native-fixture/created.txt")).unwrap(),
        "disk write"
    );
    assert_eq!(
        fs::read_to_string(
            temp.path()
                .join("node_modules/native-fixture/async-created.txt")
        )
        .unwrap(),
        "async disk write"
    );
    let native = PathBuf::from(paths["addon"].as_str().unwrap());
    assert!(!native.exists());
    successful(run(&package, &cache, &[]));
    assert!(native.exists());
    let mtime = fs::metadata(&native).unwrap().modified().unwrap();
    successful(run(&package, &cache, &[]));
    assert_eq!(fs::metadata(native).unwrap().modified().unwrap(), mtime);
}

#[test]
#[ignore = "requires a rebuilt DNR_BIN and C compiler"]
fn install_cli_and_parallel_cold_starts() {
    let (temp, package) = fixture();
    let cache = temp.path().join("cache");
    let mut children: Vec<_> = (0..4)
        .map(|_| {
            Command::new(binary())
                .arg(&package)
                .env("DNR_CACHE_DIR", &cache)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children.drain(..) {
        successful(child.wait_with_output().unwrap());
    }
    successful(
        Command::new(binary())
            .args(["cache", "info"])
            .env("DNR_CACHE_DIR", &cache)
            .output()
            .unwrap(),
    );
    let dest = temp.path().join("installed");
    fs::create_dir_all(dest.join("node_modules/native-fixture")).unwrap();
    fs::write(
        dest.join("node_modules/native-fixture/disk-only.txt"),
        "disk fallback",
    )
    .unwrap();
    successful(
        Command::new(binary())
            .arg("install")
            .arg(&package)
            .arg(&dest)
            .output()
            .unwrap(),
    );
    let paths: serde_json::Value = serde_json::from_str(
        successful(run(
            &dest.join("application.dnp"),
            &temp.path().join("unused"),
            &[],
        ))
        .trim(),
    )
    .unwrap();
    assert!(
        paths["addon"]
            .as_str()
            .unwrap()
            .contains("application.dnp.unpacked")
    );
    assert!(!native_directory(&dest.join("application.dnp"), &temp.path().join("unused")).exists());
    successful(
        Command::new(binary())
            .args(["cache", "clean", "--all"])
            .env("DNR_CACHE_DIR", &cache)
            .output()
            .unwrap(),
    );
}

#[test]
#[ignore = "requires a rebuilt DNR_BIN and C compiler"]
fn v3_native_groups_ffi_subprocesses_and_sidecar() {
    let (temp, package) = fixture();
    let cache = temp.path().join("cache");
    let paths: serde_json::Value =
        serde_json::from_str(successful(run(&package, &cache, &["read"])).trim()).unwrap();
    let native = PathBuf::from(paths["addon"].as_str().unwrap());
    assert!(
        !native.exists(),
        "compilation must not extract native groups"
    );
    successful(run(&package, &cache, &[]));
    let stamp = fs::metadata(&native).unwrap().modified().unwrap();
    successful(run(&package, &cache, &[]));
    assert_eq!(stamp, fs::metadata(&native).unwrap().modified().unwrap());
    let dest = temp.path().join("installed");
    fs::create_dir_all(dest.join("node_modules/native-fixture")).unwrap();
    fs::write(
        dest.join("node_modules/native-fixture/disk-only.txt"),
        "disk fallback",
    )
    .unwrap();
    let install_cache = temp.path().join("install-must-not-write");
    successful(
        Command::new(binary())
            .arg("install")
            .arg(&package)
            .arg(&dest)
            .env("DNR_CACHE_DIR", &install_cache)
            .output()
            .unwrap(),
    );
    assert!(!install_cache.exists());
    let sidecar: serde_json::Value = serde_json::from_str(
        successful(run(
            &dest.join("application.dnp"),
            &temp.path().join("compile-only"),
            &[],
        ))
        .trim(),
    )
    .unwrap();
    assert!(
        sidecar["addon"]
            .as_str()
            .unwrap()
            .contains("application.dnp.unpacked/fixture/root/")
    );
    let full = temp.path().join("full");
    successful(
        Command::new(binary())
            .arg("install")
            .arg(&package)
            .arg(&full)
            .args(["--mode", "full"])
            .env("DNR_CACHE_DIR", &install_cache)
            .output()
            .unwrap(),
    );
    assert!(!install_cache.exists());
    fs::write(
        full.join("node_modules/native-fixture/disk-only.txt"),
        "disk fallback",
    )
    .unwrap();
    // Full installations intentionally have ordinary mutable disk semantics.
    let main = fs::read_to_string(full.join("main.ts")).unwrap();
    let main = main
        .lines()
        .filter(|line| !line.starts_with("try { Deno.writeTextFileSync(p.data,"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(full.join("main.ts"), main).unwrap();
    let full_paths: serde_json::Value =
        serde_json::from_str(successful(run(&full, &cache, &[])).trim()).unwrap();
    assert!(
        Path::new(full_paths["addon"].as_str().unwrap()).starts_with(full.canonicalize().unwrap())
    );
}
