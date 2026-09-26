use dnr_package::{DesktopMetadata, PackOptions, Package, PackageConfig, WindowIcon};

#[test]
fn optional_icon_roundtrip_identity_and_full_install() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("main.ts"), "console.log('cli')").unwrap();
    let options = PackOptions {
        directory: source,
        entry: "main.ts".into(),
        output: dir.path().join("app.dnp"),
        includes: vec![],
        excludes: vec![],
        app_id: Some("test.icon".into()),
        force: true,
    };
    dnr_package::pack(&options).unwrap();
    let cli = Package::open(&options.output, 0).unwrap();
    assert!(cli.manifest.desktop.is_none());
    let identity = cli.package_id().to_owned();
    drop(cli);
    let mut desktop = DesktopMetadata {
        name: Some("图标测试".into()),
        window_icon: Some(WindowIcon {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 128],
        }),
    };
    dnr_package::pack_with_desktop(&options, &PackageConfig::default(), Some(desktop.clone()))
        .unwrap();
    let package = Package::open(&options.output, 0).unwrap();
    assert_eq!(package.manifest.desktop.as_ref(), Some(&desktop));
    assert_ne!(package.package_id(), identity);
    assert_eq!(package.stats().decompressions, 0);
    assert_eq!(
        package.inspect().unwrap()["manifest"]["desktop"]["windowIcon"]["bytes"],
        8
    );
    let full = dir.path().join("full");
    dnr_package::commands::install(
        &[
            options.output.display().to_string(),
            full.display().to_string(),
            "--mode".into(),
            "full".into(),
        ],
        true,
    )
    .unwrap();
    assert_eq!(
        dnr_package::metadata::installed_manifest(&full)
            .unwrap()
            .0
            .desktop
            .as_ref(),
        Some(&desktop)
    );
    desktop.window_icon.as_mut().unwrap().rgba[0] = 0;
    dnr_package::pack_with_desktop(&options, &PackageConfig::default(), Some(desktop.clone()))
        .unwrap();
    assert_ne!(
        Package::open(&options.output, 0).unwrap().package_id(),
        package.package_id()
    );
    desktop.window_icon.as_mut().unwrap().width = u32::MAX;
    assert!(
        dnr_package::pack_with_desktop(&options, &PackageConfig::default(), Some(desktop)).is_err()
    );
}
