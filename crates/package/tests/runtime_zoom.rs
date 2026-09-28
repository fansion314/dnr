//! Global zoom CLI and application APIs must remain usable without a display.
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires a rebuilt native DNR_BIN"]
fn application_zoom_survives_restart_and_package_relocation() {
    use dnr_package::{PackOptions, pack};
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let script = source.join("main.ts");
    fs::write(&script, r#"
const d = Deno.desktop;
console.log(JSON.stringify([d.getGlobalZoomFactor(), d.getZoomFactor(), d.getEffectiveZoomFactor()]));
if (Deno.args.length) d.setZoomFactor(Number(Deno.args[0]));
// No event-loop drain or graceful shutdown is needed to save the setting.
Deno.exit(0);
"#).unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(&binary)
            .args(args)
            .current_dir(temp.path())
            .env("DNR_CONFIG_DIR", &config)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        output
    };
    run(&["zoom", "set", "1.25"]);
    let probe = |entry: &str, factor: Option<&str>, expected: &str| {
        let mut args = vec![entry];
        args.extend(factor);
        let output = run(&args);
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
        assert!(output.stderr.is_empty(), "{output:?}");
    };
    probe("source/main.ts", Some("1.2"), "[1.25,1,1.25]");
    probe("source/main.ts", None, "[1.25,1.2,1.5]");
    fs::copy(&script, source.join("other.ts")).unwrap();
    probe("source/other.ts", None, "[1.25,1,1.25]");
    for (name, id) in [("one.dnp", "test.zoom.one"), ("two.dnp", "test.zoom.two")] {
        pack(&PackOptions {
            directory: source.clone(),
            entry: "main.ts".into(),
            output: temp.path().join(name),
            includes: vec![],
            excludes: vec![],
            app_id: Some(id.into()),
            force: false,
        })
        .unwrap();
    }
    probe("one.dnp", Some("1.5"), "[1.25,1,1.25]");
    fs::rename(temp.path().join("one.dnp"), temp.path().join("moved.dnp")).unwrap();
    probe("moved.dnp", None, "[1.25,1.5,1.875]");
    probe("two.dnp", None, "[1.25,1,1.25]");
    run(&["install", "moved.dnp", "installed", "--mode", "full"]);
    probe("installed", None, "[1.25,1.5,1.875]");
    run(&["zoom", "set", "2"]);
    probe("installed", Some("1"), "[2,1.5,3]");
    probe("moved.dnp", None, "[2,1,2]");
    assert_eq!(fs::read_dir(config.join("app-zoom")).unwrap().count(), 2);
    // A corrupt app file falls back independently of the global preference.
    for entry in fs::read_dir(config.join("app-zoom")).unwrap() {
        fs::write(entry.unwrap().path(), "broken").unwrap();
    }
    let fallback = run(&["moved.dnp", "1.1"]);
    assert_eq!(String::from_utf8_lossy(&fallback.stdout).trim(), "[2,1,2]");
    assert!(String::from_utf8_lossy(&fallback.stderr).contains("cannot read application zoom"));
    probe("moved.dnp", None, "[2,1.1,2.2]");
}

#[test]
#[ignore = "requires a rebuilt native DNR_BIN"]
fn zoom_config_and_process_snapshot_without_gui() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let run = |args: &[&str]| {
        Command::new(&binary)
            .args(args)
            .current_dir(temp.path())
            .env("DNR_CONFIG_DIR", &config)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap()
    };
    let query = run(&["zoom"]);
    assert!(query.status.success(), "{query:?}");
    assert!(String::from_utf8_lossy(&query.stdout).contains("Global zoom: 1 (100%)"));
    assert!(!config.exists());
    for value in ["0", "NaN", "inf", "0.49", "2.01", "125%", "no"] {
        assert!(!run(&["zoom", "set", value]).status.success());
        assert!(!config.exists());
    }
    assert!(run(&["zoom", "set", "1.25"]).status.success());
    fs::write(
        temp.path().join("probe.ts"),
        r#"
function assert(ok, message) { if (!ok) throw Error(message); }
const d = Deno.desktop;
assert(d.getGlobalZoomFactor() === 1.25, 'global');
assert(d.getZoomFactor() === 1 && d.getZoomShortcutsEnabled(), 'defaults');
const events = [];
d.addEventListener('zoomchange', e => events.push(e.detail));
d.setZoomFactor(1.2);
assert(d.getEffectiveZoomFactor() === 1.5, 'multiplication');
d.setZoomFactor(1.2);
for (const x of [NaN, Infinity, -Infinity, 0, 0.49, 2.01]) {
  let caught = false;
  try { d.setZoomFactor(x); } catch(e) { caught = e instanceof RangeError; }
  assert(caught && d.getZoomFactor() === 1.2, 'invalid numeric factor');
}
for (const x of ['1.2', null, undefined, {}]) {
  let caught = false;
  try { d.setZoomFactor(x); } catch(e) { caught = e instanceof TypeError; }
  assert(caught, 'invalid type');
}
d.setZoomShortcutsEnabled(false);
assert(!d.getZoomShortcutsEnabled(), 'disable shortcuts');
d.setZoomShortcutsEnabled(true);
d.setZoomFactor(1);
assert(d.getEffectiveZoomFactor() === 1.25, 'reset preserves global');
await new Promise(r => setTimeout(r, 30));
assert(events.length === 2, 'exactly two change events');
assert(events[0].globalFactor === 1.25 && events[0].appFactor === 1.2 && events[0].effectiveFactor === 1.5 && events[0].source === 'api', 'event');
// Changing the file after launch cannot change the process snapshot.
const path = Deno.env.get('DNR_CONFIG_DIR') + '/zoom.json';
Deno.writeTextFileSync(path, JSON.stringify({version:1,zoomFactor:1.5}));
assert(d.getGlobalZoomFactor() === 1.25, 'startup snapshot');
if (Deno.build.os === 'linux' && Deno.args.includes('dual')) {
  assert(!/libcef|libwebkit2gtk|libgtk-3/.test(Deno.readTextFileSync('/proc/self/maps')), 'GUI libraries loaded');
}
console.log('ZOOM_HEADLESS_OK');
"#,
    ).unwrap();
    let version = run(&["--version"]);
    let dual = String::from_utf8_lossy(&version.stdout).contains("backend dual");
    let probe = run(&["probe.ts", if dual { "dual" } else { "single" }]);
    assert!(probe.status.success(), "{probe:?}");
    assert!(probe.stderr.is_empty(), "{probe:?}");
    assert!(String::from_utf8_lossy(&probe.stdout).contains("ZOOM_HEADLESS_OK"));
    fs::write(
        temp.path().join("zoom"),
        "console.log(Deno.desktop.getGlobalZoomFactor(), JSON.stringify(Deno.args));",
    )
    .unwrap();
    let next = run(&["./zoom", "zoom", "set", "2"]);
    assert!(next.status.success(), "{next:?}");
    assert_eq!(
        String::from_utf8_lossy(&next.stdout).trim(),
        "1.5 [\"zoom\",\"set\",\"2\"]"
    );
    fs::write(config.join("zoom.json"), "broken").unwrap();
    assert!(!run(&["zoom"]).status.success());
    let fallback = run(&["./zoom"]);
    assert!(fallback.status.success(), "{fallback:?}");
    assert_eq!(String::from_utf8_lossy(&fallback.stdout).trim(), "1 []");
    assert!(String::from_utf8_lossy(&fallback.stderr).contains("using 1.0"));
    assert!(run(&["zoom", "reset"]).status.success());
    assert!(run(&["./zoom"]).stderr.is_empty());
}
