// Automatic companion to close-behavior.ts (which exercises native WM close).
function assert(ok: unknown, message: string) { if (!ok) throw Error(message); }
const watchdog = setTimeout(() => { throw Error("close API timeout"); }, 30_000);
const pause = () => new Promise((resolve) => setTimeout(resolve, 150));
let invalid = false;
try { new Deno.BrowserWindow({ closeBehavior: "invalid" as "hide" }); } catch (e) { invalid = e instanceof TypeError; }
assert(invalid, "invalid behavior must fail before creating a window");
const win = new Deno.BrowserWindow({ title: "DNR close API", closeBehavior: "hide" });
let loads = 0;
let lastPageToken;
const loaded = new Promise<void>((resolve) => win.addEventListener("load", async () => {
  try {
    const value = (await win.executeJs("window.token") as { value: string }).value;
    if (typeof value !== "string" || value === lastPageToken) return;
    lastPageToken = value; loads++; resolve();
  } catch { /* A prior document may complete while a close is in flight. */ }
}));
win.navigate("data:text/html,<h1>Close API test</h1><script>window.token=Math.random().toString(36)</script>");
await loaded;
const token = (await win.executeJs("window.token") as { value: string }).value;
assert(typeof token === "string" && token.length > 0, "page token exists");
let events = 0;
const cancel = (event: Event) => { events++; assert(event.cancelable, "cancelable"); event.preventDefault(); win.close(); };
win.addEventListener("close", cancel);
win.close();
await pause();
assert(events === 1 && !win.isClosed() && win.isVisible(), "cancel + reentrant close");
win.removeEventListener("close", cancel);
win.addEventListener("close", () => { events++; });
win.close();
await pause();
assert(!win.isClosed() && !win.isVisible(), "hide retains window");
win.show();
await pause();
assert((await win.executeJs("window.token") as { value: string }).value === token && loads === 1, "same page after show");
assert(win.getCloseBehavior() === "hide", "configured behavior");
win.setCloseBehavior("destroy");
win.close();
await pause();
assert(win.isClosed() && events === 3, "destroy default action");
win.close();
win.destroy();
assert(events === 3, "closed calls are idempotent");
const force = new Deno.BrowserWindow({ closeBehavior: "hide" });
force.addEventListener("close", () => { throw Error("destroy must bypass close"); });
force.destroy();
await pause();
assert(force.isClosed(), "explicit destroy ignores hide policy");
const normal = new Deno.BrowserWindow();
assert(normal.getCloseBehavior() === "hide", "default behavior");
normal.close();
await pause();
assert(!normal.isClosed() && !normal.isVisible(), "default hidden window");
normal.destroy();
const tray = new Deno.Tray();
const panel = tray.attachPanel({ width: 200, height: 100 });
panel.window.setCloseBehavior("hide");
panel.window.addEventListener("close", () => { throw Error("panel destroy must bypass close"); });
panel.destroy();
await pause();
assert(panel.window.isClosed(), "panel destruction remains unconditional");
tray.destroy();
clearTimeout(watchdog);
console.log("CLOSE_API_OK");
