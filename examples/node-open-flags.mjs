// Runs unchanged under Node and dnr; never opens a real device.
import assert from "node:assert/strict";
import * as fs from "node:fs";
import { open } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const directory = fs.mkdtempSync(join(tmpdir(), "dnr-open-flags-"));
const existing = join(directory, "existing");
const missing = join(directory, "missing");
const { O_RDONLY, O_RDWR, O_WRONLY, O_EXCL, O_CREAT } = fs.constants;
fs.writeFileSync(existing, "preserve these bytes");
const apis = {
  sync: async (path, flags) => fs.closeSync(fs.openSync(path, flags)),
  callback: (path, flags) => new Promise((resolve, reject) => {
    fs.open(path, flags, (error, fd) => {
      if (error) reject(error);
      else fs.close(fd, (error) => error ? reject(error) : resolve());
    });
  }),
  promise: async (path, flags) => { const file = await open(path, flags); await file.close(); },
};
try {
  for (const [name, run] of Object.entries(apis)) {
    for (const access of [O_RDONLY, O_WRONLY, O_RDWR]) {
      await run(existing, access | O_EXCL);
      await assert.rejects(run(missing, access | O_EXCL), { code: "ENOENT" });
    }
    for (const access of [O_WRONLY, O_RDWR]) {
      await assert.rejects(run(existing, access | O_CREAT | O_EXCL), { code: "EEXIST" });
      await run(missing, access | O_CREAT | O_EXCL);
      fs.unlinkSync(missing);
    }
    assert.equal(fs.readFileSync(existing, "utf8"), "preserve these bytes");
    console.log(`NODE_OPEN_FLAGS_OK ${name}`);
  }
} finally {
  fs.rmSync(directory, { recursive: true, force: true });
}
