// Driven by scripts/test-macos-window.py through real macOS keyboard/Dock UI.
if (Deno.build.os !== "darwin") throw new Error("This smoke test requires macOS");
const watchdog = setTimeout(() => {
  console.error("DNR_MACOS_WINDOW_TIMEOUT");
  Deno.exit(1);
}, 60000);
const dock = new Deno.Dock();
dock.addEventListener("reopen", () => console.log("DNR_MACOS_DOCK_REOPEN"));
const win = new Deno.BrowserWindow({
  title: "dnr macOS window smoke",
  width: 500,
  height: 300,
});
win.addEventListener("load", async () => {
  const result = await win.executeJs("document.title");
  if (result.ok && result.value === "dnr macOS window smoke") {
    console.log("DNR_MACOS_WINDOW_READY");
  }
});
win.addEventListener("close", () => {
  clearTimeout(watchdog);
  // Native close must still deliver the event and let queued work finish.
  setTimeout(() => console.log("DNR_MACOS_WINDOW_CLOSED"), 100);
});
win.navigate("data:text/html,<title>dnr macOS window smoke</title><h1>macOS window smoke</h1>");
