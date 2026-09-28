// DNR 0.5 window policy / persistence regression. Use an isolated DNR_CONFIG_DIR.
const assert = (ok: unknown, message: string) => { if (!ok) throw Error(message); };
const pause = (ms = 150) => new Promise((resolve) => setTimeout(resolve, ms));
const watchdog = setTimeout(() => { console.error("WINDOW_STATE_TIMEOUT"); Deno.exit(1); }, 45_000);
const operations = (Deno as any)[(Deno as any).internal].core.ops;
if (Deno.args.includes("--no-remember")) Deno.desktop.setStatePersistence({ zoom: false, windowSize: false });
const win = new Deno.BrowserWindow({ title: "DNR window state verification", width: 760, height: 520 });
let loads = 0;
let lastPageToken;
win.addEventListener("load", async () => {
  try {
    const value = ((await win.executeJs("window.token")) as { value: string }).value;
    if (typeof value === "string" && value !== lastPageToken) { lastPageToken = value; loads++; }
  } catch { /* An old document can finish loading during release. */ }
});
win.bind("ping", (value: string) => `pong:${value}`);
win.navigate("data:text/html," + encodeURIComponent(`<!doctype html><meta charset=utf-8><style>body{font:24px system-ui;background:#e7f1ee;color:#16362c;padding:32px}button,input{font:inherit;margin:12px}</style><h1>DNR 窗口状态验证</h1><p>关闭、释放后重新打开仍可调用宿主绑定。</p><input placeholder="输入后隐藏，检查页面状态"><button onclick="window.bindings.ping('click').then(v=>document.querySelector('output').textContent=v)">验证绑定</button><output></output><script>window.token=Math.random().toString(36)</script>`));
async function ready(previousLoads = 0) {
  for (let i = 0; i < 100 && loads <= previousLoads; i++) await pause(50);
  assert(loads > previousLoads, "page loaded");
}
async function token() { return ((await win.executeJs("window.token")) as { value: string }).value; }
await ready();
const initial = await token();
const logicalId = win.windowId;
assert(win.getCloseBehavior() === "hide", "new default close behavior");
assert(win.getHiddenWindowPolicy().mode === "delayed", "new default memory policy");
assert((win.getHiddenWindowPolicy() as { delayMs: number }).delayMs === 300_000, "default delay");
let duplicate = false;
try { new Deno.BrowserWindow({ stateKey: "main" }); } catch (error) { duplicate = error instanceof TypeError; }
assert(duplicate, "duplicate state key rejected before native allocation");

if (Deno.args.includes("--restore-probe") || Deno.args.includes("--no-remember")) {
  const disabled = Deno.args.includes("--no-remember");
  assert(JSON.stringify(win.getSize()) === JSON.stringify(disabled ? [760, 520] : [880, 600]), "window dimensions restored or deliberately ignored");
  assert(Deno.desktop.getZoomFactor() === (disabled ? 1 : 1.25), "zoom restored or deliberately ignored");
  win.destroy(); clearTimeout(watchdog);
  console.log("WINDOW_RESTORE_OK", JSON.stringify({ disabled, size: win.getSize() }));
} else if (Deno.args.includes("--interactive")) {
  clearTimeout(watchdog);
  win.setHiddenWindowPolicy({ mode: "delayed", delayMs: 1000 });
  const server = Deno.serve({ hostname: "127.0.0.1", port: 0, onListen: ({ port }) => console.log(`WINDOW_STATE_URL=http://127.0.0.1:${port}`) }, async (request) => {
    const path = new URL(request.url).pathname;
    if (path === "/show") win.show();
    if (path === "/hide") win.hide();
    if (path === "/allocate") await win.executeJs("window.memory = new Uint8Array(64 * 1024 * 1024); window.memory.fill(17); document.querySelector('output').textContent='64 MiB allocated'");
    if (path === "/binding") {
      await win.executeJs("window.bindings.ping('probe').then(v => { window.bindingResult=v; document.querySelector('output').textContent=v; })");
      for (let i=0; i<50; i++) {
        if (((await win.executeJs("window.bindingResult")) as {value:string}).value === "pong:probe") break;
        await pause(50);
      }
    }
    if (path === "/exit") { setTimeout(() => Deno.exit(0), 100); }
    let pageToken = null;
    try { pageToken = await token(); } catch { /* released */ }
    return Response.json({ loads, logicalId: win.windowId, visible: win.isVisible(), closed: win.isClosed(), token: pageToken, initial, size: win.getSize(), initialNative: operations.op_desktop_window_metrics(logicalId), pid: Deno.pid });
  });
  await server.finished;
} else {
  const cancel = (event: Event) => event.preventDefault();
  win.addEventListener("close", cancel); win.close(); await pause();
  assert(win.isVisible(), "cancelable close"); win.removeEventListener("close", cancel);
  win.setHiddenWindowPolicy({ mode: "keep" }); win.close(); await pause(); win.show(); await pause();
  assert(await token() === initial && loads === 1, "keep preserves page");
  win.setHiddenWindowPolicy({ mode: "delayed", delayMs: 300 });
  win.hide(); await pause(50); win.show(); await pause(350);
  assert(await token() === initial, "show cancels release timer");
  win.hide(); await pause(180); win.hide(); await pause(220);
  assert(operations.op_desktop_window_metrics(logicalId)[5] === 0, "repeat hide does not postpone real teardown");
  let refused = false; try { await win.executeJs("1"); } catch { refused = true; }
  assert(refused && !win.isClosed(), "released page rejects JS but logical object survives");
  const oldLoads = loads;
  win.setSize(820, 560); win.show(); await ready(oldLoads);
  assert(win.windowId === logicalId && await token() !== initial, "stable object recreates a fresh page");
  await win.executeJs("window.bindings.ping('restored').then(v => window.bindingResult = v)");
  let bindingResult;
  for (let i = 0; i < 50; i++) {
    bindingResult = ((await win.executeJs("window.bindingResult")) as { value: string }).value;
    if (bindingResult === "pong:restored") break;
    await pause(50);
  }
  assert(bindingResult === "pong:restored", "bindings restored before navigation");
  const beforeImmediate = loads;
  win.setHiddenWindowPolicy({ mode: "immediate" }); win.hide(); win.focus();
  await ready(beforeImmediate);
  assert(win.isVisible(), "focus while closing queues recreation");
  Deno.desktop.setZoomFactor(1.1); Deno.desktop.setZoomFactor(1.2); Deno.desktop.setZoomFactor(1.25);
  win.setSize(860, 580); await pause(300); win.setSize(880, 600);
  await pause(5300);
  const config = Deno.env.get("DNR_CONFIG_DIR")!;
  const files = [...Deno.readDirSync(`${config}/app-state`)].filter((entry) => entry.name.endsWith(".json"));
  assert(files.length === 1, "one app settings document");
  const saved = JSON.parse(Deno.readTextFileSync(`${config}/app-state/${files[0].name}`));
  assert(saved.zoomFactor === 1.25 && saved.windows.main[0] === 880 && saved.windows.main[1] === 600, "debounced final zoom and size saved together");
  win.destroy(); await pause(); assert(win.isClosed(), "permanent destroy");
  clearTimeout(watchdog);
  console.log("WINDOW_STATE_OK", JSON.stringify({ loads, logicalId, saved }));
}
