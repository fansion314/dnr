//! Device flush regression: Darwin raw devices do not accept F_FULLFSYNC.
#![cfg(target_os = "macos")]

use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "requires a built native dnr; set DNR_BIN and pass --ignored"]
fn device_and_regular_file_sync_preserve_errors() {
    let binary = PathBuf::from(std::env::var_os("DNR_BIN").expect("set DNR_BIN"))
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("sync.mjs");
    fs::write(
        &script,
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { promisify } from 'node:util';
// /dev/null is harmless, but rejects Darwin F_FULLFSYNC just like raw disks.
// No real storage device is opened by this automated test.
for (const path of ['/dev/null', './regular']) {
  const handle = await fs.promises.open(path, path === '/dev/null' ? 'r+' : 'w+');
  await handle.write(Buffer.from('flush regression'));
  await handle.sync();
  await handle.datasync();
  fs.fsyncSync(handle.fd);
  fs.fdatasyncSync(handle.fd);
  await promisify(fs.fsync)(handle.fd);
  await promisify(fs.fdatasync)(handle.fd);
  const fd = handle.fd;
  await handle.close();
  assert.throws(() => fs.fsyncSync(fd));
  await assert.rejects(promisify(fs.fsync)(fd));
  await assert.rejects(handle.sync());
  const file = await Deno.open(path, {read: true, write: true});
  await file.sync();
  await file.syncData();
  file.syncSync();
  file.syncDataSync();
  file.close();
  assert.throws(() => file.syncSync());
}
assert.equal(await fs.promises.readFile('./regular', 'utf8'), 'flush regression');
console.log('DNR_DEVICE_SYNC_OK');
"#,
    )
    .unwrap();
    let output = Command::new(binary)
        .arg(&script)
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("DNR_DEVICE_SYNC_OK"));
}
