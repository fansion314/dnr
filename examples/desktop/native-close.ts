// Close using the titlebar button (or the compositor), not win.close().
// Modes: deno (0), node (9), shutdown (0), idle (0).
import process from "node:process";

const mode = Deno.args[0] ?? "deno";
if (!["deno", "node", "shutdown", "idle"].includes(mode)) {
  throw new Error(`Unknown close test mode: ${mode}`);
}
const watchdog = setTimeout(() => {
  console.error("DNR_NATIVE_CLOSE_TIMEOUT");
  Deno.exit(1);
}, 20000);
const server = mode === "idle" ? undefined : Deno.serve(
  { hostname: "127.0.0.1", port: 0 },
  () => new Response("<title>dnr close</title><h1>Close this window</h1>"),
);
const win = new Deno.BrowserWindow({ title: `dnr native close: ${mode}`, width: 500, height: 300 });
win.addEventListener("close", () => {
  clearTimeout(watchdog);
  console.log("DNR_NATIVE_CLOSE_DELIVERED");
  // These exits must stop the host even while the HTTP server is listening.
  if (mode === "deno") Deno.exit(0);
  if (mode === "node") process.exit(9);
  // Without an explicit exit, allow work scheduled by the close handler to end.
  Promise.resolve(server?.shutdown()).then(() => {
    setTimeout(() => console.log("DNR_NATIVE_CLOSE_ASYNC_OK"), 100);
  });
});
let ready = false;
win.addEventListener("load", async () => {
  const location = await win.executeJs("location.href");
  if (!location.ok || location.value === "about:blank" || ready) return;
  ready = true;
  console.log("DNR_NATIVE_CLOSE_READY");
});
win.navigate(server
  ? `http://127.0.0.1:${server.addr.port}`
  : "data:text/html,<title>dnr close</title><h1>Close this window</h1>");
