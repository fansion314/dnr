// Run with an isolated DNR_CONFIG_DIR; `dnr zoom set 1.25` makes G × 1.2 = 1.5.
// --interactive leaves a loopback test controller and windows for native keys.
function assert(ok: unknown, message: string) { if (!ok) throw Error(message); }
const desktop = Deno.desktop;
const changes: unknown[] = [];
desktop.addEventListener("zoomchange", (event) => changes.push(event.detail));
desktop.setZoomFactor(1.2);
const windows: Deno.BrowserWindow[] = [];
const html = `<!doctype html><meta charset="utf-8"><style>
body{margin:24px;font:16px system-ui;background:#f8f7f3;color:#222}
button,input{font:inherit;padding:8px}#box{width:100px;height:100px;background:#30796f}
</style><h1>DNR 缩放验证</h1><p>系统 DPI × 全局倍率 × 应用倍率</p>
<div id="box"></div><p>100 CSS px 色块 · 16 CSS px 正文</p>
<input placeholder="键盘焦点测试"><button onclick="this.textContent='已点击'">点击测试</button>
<script>window.token=Math.random().toString(36);window.zoomKeys=0;
addEventListener('keydown',e=>{if((e.metaKey||e.ctrlKey)&&['+','=','-','0'].includes(e.key))window.zoomKeys++});</script>`;
async function pageState(win: Deno.BrowserWindow) {
  const result = await win.executeJs(`({width:innerWidth,height:innerHeight,dpr:devicePixelRatio,
    box:document.querySelector('#box')?.getBoundingClientRect().width ?? -1,token:window.token,keys:window.zoomKeys,
    focus:document.activeElement?.tagName,url:location.href.slice(0,100),title:document.title,diagnostic:document.querySelector('#box')?'':document.body?.innerText.slice(0,500)})`);
  return (result as { value: { width: number; height: number; dpr: number; box: number; token: string; keys: number } }).value;
}
let finish!: () => void;
const done = new Promise<void>((resolve) => finish = resolve);
const server = Deno.serve({ hostname: "127.0.0.1", port: 0, onListen() {} }, async (request) => {
  const url = new URL(request.url);
  if (url.pathname === "/state") {
    return Response.json({ global: desktop.getGlobalZoomFactor(), app: desktop.getZoomFactor(),
      effective: desktop.getEffectiveZoomFactor(), shortcuts: desktop.getZoomShortcutsEnabled(), changes,
      windows: await Promise.all(windows.map(pageState)) });
  }
  if (url.pathname === "/factor") desktop.setZoomFactor(Number(url.searchParams.get("value")));
  else if (url.pathname === "/shortcuts") desktop.setZoomShortcutsEnabled(url.searchParams.get("enabled") === "true");
  else if (url.pathname === "/quit") { finish(); return new Response("bye"); }
  else return new Response(html, { headers: { "content-type": "text/html; charset=utf-8" } });
  return new Response("ok");
});
const port = server.addr.port;
async function navigate(win: Deno.BrowserWindow, host = "127.0.0.1") {
  const url = Deno.args.includes("--data") ? `data:text/html,${encodeURIComponent(html)}` : `http://${host}:${port}/page`;
  const loaded = new Promise<void>((resolve) => {
    const onLoad = async () => {
      // CEF emits an initial about:blank load as well as the requested page.
      const result = await win.executeJs("location.href");
      if (result.ok && result.value === url) {
        win.removeEventListener("load", onLoad);
        resolve();
      }
    };
    win.addEventListener("load", onLoad);
  });
  win.navigate(url);
  await loaded;
}
async function create(title: string) {
  const win = new Deno.BrowserWindow({ title, width: 800, height: 600, closeBehavior: "hide" });
  windows.push(win);
  await navigate(win);
  return win;
}
async function check(message: string) {
  // Native painting and JS evaluation are asynchronous on all backends.
  const deadline = Date.now() + 5000;
  for (;;) {
    const states = await Promise.all(windows.map(pageState));
    if (states.every((s) => Math.abs(s.width * desktop.getEffectiveZoomFactor() - 800) <= 5 && Math.abs(s.box - 100) < 1)) {
      console.log(JSON.stringify({ check: message, effective: desktop.getEffectiveZoomFactor(), states }));
      return;
    }
    assert(Date.now() < deadline, `${message}: unexpected layout ${JSON.stringify(states)}`);
    await new Promise((r) => setTimeout(r, 50));
  }
}
const watchdog = setTimeout(() => { console.error("ZOOM_TIMEOUT"); Deno.exit(1); }, 60_000);
try {
  const first = await create("DNR Zoom A");
  const second = await create("DNR Zoom B");
  await check("initial multiplication / same origin");
  const token = (await pageState(first)).token;
  first.hide();
  desktop.setZoomFactor(0.8);
  await check("hidden window updated");
  first.show();
  assert((await pageState(first)).token === token, "show recreated page");
  desktop.setZoomFactor(1.75);
  await navigate(second, "localhost");
  await check(Deno.args.includes("--data") ? "data navigation" : "cross-origin navigation");
  await navigate(first);
  await check("reload");
  await create("DNR Zoom C");
  await check("new window inherits current factor");
  desktop.setZoomFactor(1);
  await check("reset preserves global");
  console.log("ZOOM_API_OK");
  clearTimeout(watchdog);
  if (Deno.args.includes("--interactive")) {
    first.focus();
    console.log(`ZOOM_READY http://127.0.0.1:${port}`);
    await done;
  }
} finally {
  clearTimeout(watchdog);
  windows.forEach((win) => win.destroy());
  await server.shutdown();
}
