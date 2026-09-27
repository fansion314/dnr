use std::{fs, path::Path, process::Command};

fn dnc() -> Command {
    Command::new(std::env::var_os("DNC_BIN").unwrap_or_else(|| env!("CARGO_BIN_EXE_dnc").into()))
}
fn fixture(dir: &Path) {
    fs::create_dir(dir.join("input")).unwrap();
    fs::write(dir.join("input/main.ts"), "console.log('ok')").unwrap();
    fs::write(
        dir.join("desktop.json"),
        r#"{
      "appId":"org.example.test", "name":"测试应用", "version":"1.0.0", "entry":"main.ts",
      "macos":{"icon":"missing.icns"}, "linux":{"packageName":"test-app","icon":"missing.svg"}
    }"#,
    )
    .unwrap();
}

#[test]
fn v3_dnp_and_desktop_cli_contract() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let input = dir.path().join("input");
    let output = dir.path().join("app.dnp");
    assert!(
        dnc()
            .arg(&input)
            .args(["--entry", "main.ts", "-o"])
            .arg(&output)
            .status()
            .unwrap()
            .success()
    );
    let package = dnr_package::Package::open(&output, 1024).unwrap();
    assert_eq!(package.manifest.entry, "main.ts");
    assert_eq!(package.manifest.format_version, 4);
    for version in ["1", "2"] {
        let rejected = dnc()
            .arg(&input)
            .args(["--entry", "main.ts", "--format-version", version, "-o"])
            .arg(dir.path().join("unsupported.dnp"))
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        assert!(!dir.path().join("unsupported.dnp").exists());
    }
    for args in [
        vec!["--desktop-manifest", "desktop.json"],
        vec!["--target", "macos"],
        vec!["--desktop-manifest", "desktop.json", "--target", "windows"],
        vec![
            "--desktop-manifest",
            "desktop.json",
            "--target",
            "macos",
            "--app-id",
            "org.other.app",
        ],
    ] {
        assert!(
            !dnc()
                .current_dir(dir.path())
                .arg("input")
                .args(args)
                .args(["-o", "test.app"])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[ignore = "requires Xcode CLT, DNC_TEST_ICON (ICNS or PNG), and DNR_BIN"]
fn macos_bundle_real_runtime() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let runtime = fs::canonicalize(std::env::var_os("DNR_BIN").expect("DNR_BIN")).unwrap();
    let icon = fs::canonicalize(std::env::var_os("DNC_TEST_ICON").expect("DNC_TEST_ICON")).unwrap();
    fs::write(
        dir.path().join("input/main.ts"),
        r#"
if (Deno.args[0] === "__exit") Deno.exit(37);
if (Deno.args[0] === "__wait") {
  Deno.addSignalListener("SIGTERM", () => Deno.exit(23));
  Deno.writeTextFileSync("child.pid", String(Deno.pid));
  setTimeout(() => Deno.exit(99), 15000);
} else {
  console.log(JSON.stringify({args:Deno.args,cwd:Deno.cwd(),name:Deno.env.get('LAUFEY_APP_NAME'),id:Deno.env.get('LAUFEY_APP_ID')}));
}
"#,
    )
    .unwrap();
    let manifest = serde_json::json!({
        "appId": "org.example.test", "name": "测试 ' & 应用", "version": "1.0.0", "entry": "main.ts",
        "macos": { "icon": icon, "runtimePath": runtime }
    });
    fs::write(
        dir.path().join("desktop.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let output = dir.path().join("input/测试 app.app");
    let build = || {
        let mut c = dnc();
        c.current_dir(dir.path())
            .args([
                "input",
                "--desktop-manifest",
                "desktop.json",
                "--target",
                "macos",
                "-o",
            ])
            .arg(&output);
        c
    };
    assert!(build().status().unwrap().success());
    let original = fs::read(output.join("Contents/Resources/application.dnp")).unwrap();
    assert!(!build().output().unwrap().status.success());
    assert_eq!(
        fs::read(output.join("Contents/Resources/application.dnp")).unwrap(),
        original
    );
    assert!(build().arg("--force").status().unwrap().success());
    let pkg = dnr_package::Package::open(&output.join("Contents/Resources/application.dnp"), 1024)
        .unwrap();
    assert_eq!(pkg.manifest.app_id, "org.example.test");
    // Rebuilding with output inside input must not recursively archive the old app.
    assert!(pkg.entries.values().all(|e| !e.name.contains(".app/")));
    let result = Command::new(output.join("Contents/MacOS/launcher"))
        .current_dir(dir.path())
        .args(["a b", "", "--literal", "'$()"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        value["args"],
        serde_json::json!(["a b", "", "--literal", "'$()"])
    );
    assert_eq!(
        value["cwd"],
        fs::canonicalize(dir.path()).unwrap().to_str().unwrap()
    );
    assert_eq!(value["name"], manifest["name"]);
    assert_eq!(value["id"], manifest["appId"]);

    let launcher = output.join("Contents/MacOS/launcher");
    assert_eq!(
        Command::new(&launcher)
            .arg("__exit")
            .status()
            .unwrap()
            .code(),
        Some(37)
    );
    // A supervisor must forward termination and reap its child, including a
    // child killed externally. The script's watchdog bounds a failed test.
    for kill_child in [false, true] {
        let pid_file = dir.path().join("child.pid");
        let _ = fs::remove_file(&pid_file);
        let mut process = Command::new(&launcher)
            .current_dir(dir.path())
            .arg("__wait")
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        while !pid_file.exists() {
            if started.elapsed() > std::time::Duration::from_secs(10) {
                let _ = process.kill();
                let _ = process.wait();
                panic!("runtime did not publish its PID");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let child: u32 = fs::read_to_string(&pid_file).unwrap().parse().unwrap();
        assert_ne!(child, process.id(), "launcher must not exec the runtime");
        let target = if kill_child { child } else { process.id() };
        assert!(
            Command::new("kill")
                .args([
                    if kill_child { "-KILL" } else { "-TERM" },
                    &target.to_string()
                ])
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            process.wait().unwrap().code(),
            Some(if kill_child { 137 } else { 23 })
        );
        assert!(
            !Command::new("kill")
                .args(["-0", &child.to_string()])
                .output()
                .unwrap()
                .status
                .success(),
            "runtime must be reaped before launcher exits"
        );
    }

    // The default bundle discovers dnr from the caller's PATH, including paths
    // containing spaces and relative entries, without changing cwd or argv.
    let mut automatic = manifest.clone();
    automatic["macos"]
        .as_object_mut()
        .unwrap()
        .remove("runtimePath");
    fs::write(
        dir.path().join("desktop.json"),
        serde_json::to_vec(&automatic).unwrap(),
    )
    .unwrap();
    assert!(build().arg("--force").status().unwrap().success());
    let bin = dir.path().join("runtime bin");
    fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink(&runtime, bin.join("dnr")).unwrap();
    let result = Command::new(&launcher)
        .current_dir(dir.path())
        .env("PATH", "nonexistent:runtime bin")
        .args(["a b", "", "--literal"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["args"], serde_json::json!(["a b", "", "--literal"]));
    assert_eq!(
        value["cwd"],
        fs::canonicalize(dir.path()).unwrap().to_str().unwrap()
    );
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[ignore = "requires Xcode CLT; checks native runtime discovery without opening UI"]
fn macos_launcher_path_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("path-test");
    let output = Command::new("clang")
        .args([
            "-fobjc-arc",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-framework",
            "AppKit",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/launcher-path.m"))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        Command::new(binary)
            .arg(dir.path())
            .status()
            .unwrap()
            .success()
    );
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[ignore = "requires native Arch/CachyOS makepkg, fakeroot, zstd, pacman; run as non-root"]
fn arch_package_metadata_and_payload() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    fs::write(
        dir.path().join("missing.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    )
    .unwrap();
    let output = dir.path().join("test.pkg.tar.zst");
    assert!(
        dnc()
            .current_dir(dir.path())
            .args([
                "input",
                "--desktop-manifest",
                "desktop.json",
                "--target",
                "archlinux",
                "-o"
            ])
            .arg(&output)
            .status()
            .unwrap()
            .success()
    );
    let query = Command::new("pacman")
        .args(["-Qip"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    assert!(String::from_utf8_lossy(&query.stdout).contains("test-app"));
    let listing = Command::new("bsdtar")
        .arg("-tf")
        .arg(&output)
        .output()
        .unwrap();
    assert!(listing.status.success());
    let list = String::from_utf8(listing.stdout).unwrap();
    for file in [
        ".PKGINFO",
        ".BUILDINFO",
        ".MTREE",
        "usr/bin/test-app",
        "usr/lib/org.example.test/application.dnp",
        "usr/share/applications/org.example.test.desktop",
    ] {
        assert!(
            list.lines()
                .any(|line| line.trim_start_matches("./") == file.trim_start_matches("./")),
            "{file}: {list}"
        );
    }
}
