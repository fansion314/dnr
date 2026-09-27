// Run in a real desktop; close the native window three times, restoring it
// from the tray after the second close. The page token must remain unchanged.
const win = new Deno.BrowserWindow({ title: "DNR close behavior", closeBehavior: "hide" });
const tray = new Deno.Tray();
const icon = Deno.args[0];
if (icon) tray.setIcon(await Deno.readFile(icon));
tray.setMenu([{ item: { label: "Restore", id: "restore", enabled: true } }]);
const assert = (ok: unknown, message: string) => { if (!ok) throw Error(message); };
let requests = 0;
let loads = 0;
let token: unknown;
win.addEventListener("load", async () => {
  loads++;
  token = (await win.executeJs("window.pageToken") as { value: string }).value;
  console.log("CLOSE_READY", win.windowId, token);
});
win.addEventListener("close", (event) => {
  requests++;
  assert(event.cancelable, "close must be cancelable");
  assert(!win.isClosed(), "native close must wait for JS");
  if (requests === 1) {
    event.preventDefault();
    // Reentrant requests must not dispatch recursively.
    win.close();
    console.log("CLOSE_CANCELED");
  } else if (requests === 2) {
    setTimeout(() => {
      assert(!win.isVisible() && !win.isClosed(), "hide must retain the window");
      console.log("CLOSE_HIDDEN");
    }, 200);
  } else {
    win.setCloseBehavior("destroy");
    setTimeout(() => {
      assert(win.isClosed(), "destroy must close the window");
      tray.destroy();
      clearTimeout(watchdog);
      console.log("CLOSE_DESTROYED");
    }, 200);
  }
});
const restore = async () => {
  win.show();
  win.focus();
  assert((await win.executeJs("window.pageToken") as { value: string }).value === token, "page was recreated");
  assert(loads === 1, "page was reloaded");
  console.log("CLOSE_RESTORED_SAME_PAGE", requests);
};
tray.addEventListener("click", restore);
tray.addEventListener("menuclick", restore);
win.navigate("data:text/html,<title>DNR close behavior</title><h1>Close / hide / restore</h1><script>window.pageToken=Math.random().toString(36)</script>");
const watchdog = setTimeout(() => { throw Error("close behavior test timed out"); }, 120_000);
