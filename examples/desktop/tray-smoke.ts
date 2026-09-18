// Close the window, keep only a tray and an unreferenced timer, then exit.
const watchdog = setTimeout(() => {
  console.error("DNR_TRAY_TIMEOUT");
  Deno.exit(1);
}, 15000);
const tray = new Deno.Tray();
if (tray.trayId === 0) throw new Error("Native tray unavailable");
tray.setIcon(Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAAG0lEQVR4nGPQrbjznxLMMGrAaBiMpoP/wyQMAOMegB/F2KyGAAAAAElFTkSuQmCC"), c => c.charCodeAt(0)));
tray.setTooltip("dnr Linux lifecycle verification");
tray.setMenu([{ item: { label: "dnr lifecycle verification", enabled: false } }]);
const win = new Deno.BrowserWindow({ title: "dnr tray verification", width: 400, height: 200 });
let loaded = false;
win.addEventListener("load", async () => {
  const location = await win.executeJs("location.href");
  if (!location.ok || location.value === "about:blank") return;
  if (loaded) return;
  loaded = true;
  // Closing a window must settle evaluations whose renderer replies are lost.
  const pending = Array.from({ length: 16 }, () => win.executeJs("'pending'"));
  win.close();
  await Promise.all(pending);
  clearTimeout(watchdog);
  console.log("DNR_TRAY_WINDOW_CLOSED");
  // Only the tray should keep the runtime alive during this interval.
  const timer = setTimeout(() => {
    tray.destroy();
    console.log("DNR_TRAY_OK");
  }, 750);
  Deno.unrefTimer(timer);
});
win.navigate("data:text/html,<title>dnr tray</title><h1>Tray lifecycle check</h1>");
