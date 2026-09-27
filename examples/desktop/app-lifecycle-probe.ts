// Test-only wrapper: run an unchanged desktop entry and expose read-only
// window/page observations on a random loopback port. Use isolated app data.
// dnr app-lifecycle-probe.ts /absolute/path/to/desktop/main.ts
import { pathToFileURL } from "node:url";
const original = Deno.BrowserWindow;
const windows: Array<{ window: InstanceType<typeof original>; loads: number; closes: number }> = [];
function ProbeWindow(...args: ConstructorParameters<typeof original>) {
  const window = new original(...args);
  const record = { window, loads: 0, closes: 0 };
  windows.push(record);
  window.addEventListener("load", () => { record.loads++; });
  window.addEventListener("close", () => { record.closes++; });
  return window;
}
Object.setPrototypeOf(ProbeWindow, original);
ProbeWindow.prototype = original.prototype;
Deno.BrowserWindow = ProbeWindow as unknown as typeof original;
const watchdog = setTimeout(() => { throw Error("app probe timed out"); }, 600_000);
const probe = Deno.serve({ hostname: "127.0.0.1", port: 0, onListen() {} }, async (request) => {
  if (request.method !== "GET" || new URL(request.url).pathname !== "/state") {
    return new Response("Not found", { status: 404 });
  }
  const result = [];
  for (const record of windows) {
    const window = record.window;
    const closed = window.isClosed();
    result.push({
      id: window.windowId,
      loads: record.loads,
      closes: record.closes,
      closed,
      visible: !closed && window.isVisible(),
      page: closed ? null : await window.executeJs(`JSON.stringify({
        token: globalThis.__dnrLifecycleToken ??= Math.random().toString(36),
        url: location.href,
        title: document.title,
        text: document.body.innerText,
        inputs: Array.from(document.querySelectorAll('input,textarea')).map(x => ({value:x.value,type:x.type})),
        audio: Array.from(document.querySelectorAll('audio')).map(x => ({paused:x.paused,time:x.currentTime}))
      })`),
    });
  }
  return Response.json(result);
});
console.log("APP_PROBE_URL", `http://127.0.0.1:${probe.addr.port}/state`);
const entry = Deno.args[0];
if (!entry?.startsWith("/")) throw Error("absolute desktop entry path required");
try {
  await import(pathToFileURL(entry).href);
} catch (error) {
  clearTimeout(watchdog);
  await probe.shutdown();
  throw error;
}
