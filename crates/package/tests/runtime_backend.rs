//! Host backend options must not consume application arguments or initialize GUI.
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires a built native dnr; set DNR_BIN and pass --ignored"]
fn desktop_metadata_does_not_activate_gui() {
    use dnr_package::{DesktopMetadata, PackOptions, PackageConfig, WindowIcon};
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("DNR_BIN"));
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("main.ts"),
        r#"
      console.log(JSON.stringify([Deno.env.get('LAUFEY_APP_ID'), Deno.env.get('LAUFEY_APP_NAME')]));
      if (Deno.build.os === 'linux' && Deno.args.includes('dual')) {
        const maps = Deno.readTextFileSync('/proc/self/maps');
        if (/libcef|libwebkit2gtk|libgtk-3/.test(maps)) throw Error('GUI loaded by metadata');
      }
    "#,
    )
    .unwrap();
    let package = temp.path().join("app.dnp");
    dnr_package::pack_with_desktop(
        &PackOptions {
            directory: source,
            entry: "main.ts".into(),
            output: package.clone(),
            includes: vec![],
            excludes: vec![],
            app_id: Some("test.desktop.identity".into()),
            force: false,
        },
        &PackageConfig::default(),
        Some(DesktopMetadata {
            name: Some("图标测试".into()),
            window_icon: Some(WindowIcon {
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255],
            }),
        }),
    )
    .unwrap();
    let full = temp.path().join("installed");
    dnr_package::commands::install(
        &[
            package.display().to_string(),
            full.display().to_string(),
            "--mode".into(),
            "full".into(),
        ],
        true,
    )
    .unwrap();
    let version = Command::new(&binary).arg("--version").output().unwrap();
    for input in [package, full] {
        let output = Command::new(&binary)
            .arg(input)
            .arg(
                if String::from_utf8_lossy(&version.stdout).contains("backend dual") {
                    "dual"
                } else {
                    "single"
                },
            )
            .env("XDG_CACHE_HOME", temp.path().join("cache"))
            .env("LAUFEY_APP_ID", "wrong.inherited.identity")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            r#"["test.desktop.identity","图标测试"]"#
        );
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
#[ignore = "requires a built native dnr; set DNR_BIN and pass --ignored"]
fn backend_selection_preserves_cli_and_package_arguments() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("main.ts"),
        "console.log(JSON.stringify(Deno.args));",
    )
    .unwrap();
    let package = temp.path().join("app.dnp");
    dnr_package::pack(&dnr_package::PackOptions {
        directory: temp.path().into(),
        entry: "main.ts".into(),
        output: package.clone(),
        includes: vec![],
        excludes: vec![],
        app_id: Some("test.backend.args".into()),
        force: false,
    })
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(&binary)
            .args(args)
            .current_dir(temp.path())
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap()
    };
    let version = String::from_utf8(run(&["--version"]).stdout).unwrap();
    for backend in ["auto", "system-cef", "webview"] {
        let available = backend == "auto"
            || version.contains("backend dual")
            || version.contains(&format!("backend {backend}"));
        for entry in ["main.ts", "app.dnp"] {
            let result = run(&[
                "--no-code-cache",
                "--backend",
                backend,
                "--no-transpile-cache",
                entry,
                "--backend",
                "app-value",
                "--type=renderer",
                "",
            ]);
            assert_eq!(result.status.success(), available, "{result:?}");
            if available {
                assert_eq!(
                    String::from_utf8_lossy(&result.stdout).trim(),
                    r#"["--backend","app-value","--type=renderer",""]"#
                );
                assert!(result.stderr.is_empty(), "{result:?}");
            } else {
                assert_eq!(result.status.code(), Some(2));
                assert!(String::from_utf8_lossy(&result.stderr).contains("does not include"));
            }
        }
    }
    let result = run(&[
        "--backend=auto",
        "--package",
        "app.dnp",
        "--entry",
        "main.ts",
        "--",
        "--backend=webview",
    ]);
    assert!(result.status.success(), "{result:?}");
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        r#"["--backend=webview"]"#
    );
    for options in [
        vec!["--backend"],
        vec!["--backend=bad"],
        vec!["--backend=auto", "--backend=webview"],
    ] {
        assert_eq!(run(&options).status.code(), Some(2));
    }
}
