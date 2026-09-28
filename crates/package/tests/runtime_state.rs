//! The persistence controller must work without initializing a GUI backend.
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires rebuilt DNR_BIN"]
fn persistence_switches_debounce_and_legacy_migration() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config");
    let script = temp.path().join("state.ts");
    fs::write(&script, r#"
const d = Deno.desktop;
const assert = (ok, message) => { if (!ok) throw Error(message); };
const config = Deno.env.get('DNR_CONFIG_DIR');
const mode = Deno.args[0];
const files = () => { try { return [...Deno.readDirSync(config + '/app-state')].filter(e=>e.name.endsWith('.json')); } catch { return []; } };
const read = () => JSON.parse(Deno.readTextFileSync(config + '/app-state/' + files()[0].name));
assert(JSON.stringify(d.getStatePersistence()) === '{"zoom":true,"windowSize":true}', 'defaults');
for (const value of [null, 1, {zoom: 1}, {unknown: true}]) {
  let bad = false; try { d.setStatePersistence(value); } catch (e) { bad = e instanceof TypeError; }
  assert(bad, 'validate persistence options');
}
if (mode === 'save') { d.setZoomFactor(1.2); Deno.exit(0); }
if (mode === 'disabled') {
  assert(d.getZoomFactor() === 1.2, 'restore');
  d.setStatePersistence({zoom: false});
  assert(d.getZoomFactor() === 1 && d.getStatePersistence().windowSize, 'undo untouched restoration');
  d.setZoomFactor(1.5); Deno.exit(0);
}
if (mode === 'explicit') {
  assert(d.getZoomFactor() === 1.2, 'disabled did not overwrite disk');
  d.setZoomFactor(1.75);
  d.setStatePersistence({zoom:false, windowSize:false});
  assert(d.getZoomFactor() === 1.75, 'keep explicit current factor');
  Deno.exit(0);
}
if (mode === 'legacy') {
  assert(d.getZoomFactor() === 1.5, 'legacy restored');
  d.setZoomFactor(1.25); Deno.exit(0);
}
if (mode === 'concurrent-zoom') {
  d.setZoomFactor(1.5);
  Deno.writeTextFileSync(Deno.args[1] + '.ready', 'ready');
  for (;;) {
    try { Deno.statSync(Deno.args[1]); break; } catch { await new Promise(r=>setTimeout(r,10)); }
  }
  Deno.exit(0);
}
if (mode === 'concurrent-size') {
  Deno[Deno.internal].core.ops.op_desktop_record_window_size('dialog', 510, 410);
  Deno.writeTextFileSync(Deno.args[1], 'go');
  Deno.exit(0);
}
if (mode === 'debounce') {
  const pause = ms=>new Promise(r=>setTimeout(r,ms));
  d.setZoomFactor(1.1); await pause(250); d.setZoomFactor(1.2); await pause(250); d.setZoomFactor(1.75);
  await pause(750);
  assert(read().zoomFactor === 1.25, 'no early writes');
  await pause(4500);
  assert(read().zoomFactor === 1.75, 'final value after debounce');
  const path = config + '/app-state/' + files()[0].name;
  const stamp = Deno.statSync(path).mtime.valueOf();
  d.setZoomFactor(1.75); await pause(5200);
  assert(Deno.statSync(path).mtime.valueOf() === stamp, 'no-op does not write');
  d.setZoomFactor(1.2); d.setStatePersistence({zoom:false}); await pause(5200);
  assert(read().zoomFactor === 1.75, 'disable cancels pending write');
}
console.log('STATE_OK');
"#).unwrap();
    let run = |mode: &str| {
        let result = Command::new(&binary)
            .arg(&script)
            .arg(mode)
            .env("DNR_CONFIG_DIR", &config)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert!(result.status.success(), "{mode}: {result:?}");
    };
    run("save");
    run("disabled");
    run("explicit");
    let path = fs::read_dir(config.join("app-state"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap()["zoomFactor"],
        1.2
    );
    fs::create_dir(config.join("app-zoom")).unwrap();
    fs::write(
        config.join("app-zoom").join(path.file_name().unwrap()),
        r#"{"version":1,"zoomFactor":1.5}"#,
    )
    .unwrap();
    fs::remove_file(&path).unwrap();
    run("legacy");
    run("debounce");
    let gate = temp.path().join("concurrent-gate");
    let mut zoom = Command::new(&binary)
        .arg(&script)
        .arg("concurrent-zoom")
        .arg(&gate)
        .env("DNR_CONFIG_DIR", &config)
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    while !gate.with_extension("ready").exists() {
        assert!(
            started.elapsed().as_secs() < 5,
            "concurrent writer did not start"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        Command::new(&binary)
            .arg(&script)
            .arg("concurrent-size")
            .arg(&gate)
            .env("DNR_CONFIG_DIR", &config)
            .status()
            .unwrap()
            .success()
    );
    assert!(zoom.wait().unwrap().success());
    let merged: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(merged["zoomFactor"], 1.5);
    assert_eq!(merged["windows"]["dialog"], serde_json::json!([510, 410]));
}
