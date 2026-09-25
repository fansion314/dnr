use std::{fs, path::Path, process::Command};

fn dnc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dnc"))
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
    assert_eq!(package.manifest.format_version, 3);
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
    fs::write(dir.path().join("input/main.ts"), "console.log(JSON.stringify({args:Deno.args,cwd:Deno.cwd(),name:Deno.env.get('LAUFEY_APP_NAME'),id:Deno.env.get('LAUFEY_APP_ID')}));").unwrap();
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
