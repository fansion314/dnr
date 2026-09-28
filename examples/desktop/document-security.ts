// No credentials or network services outside loopback are used by this fixture.
const assert = (value: unknown, message: string) => { if (!value) throw new Error(message); };
const pause = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
async function until(check: () => boolean | Promise<boolean>, message: string) {
  for (let i = 0; i < 100; ++i) { if (await check()) return; await pause(50); }
  throw new Error(message);
}
const seen: string[] = [];
const html = (body: string) => new Response(`<!doctype html><title>DNR document security fixture</title>${body}`, {headers:{'Content-Type':'text/html'}});
const foreign = Deno.serve({hostname:'127.0.0.1',port:0,onListen(){}}, () => html(`<script>window.bindings?.who('foreign')</script>`));
const trusted = Deno.serve({hostname:'127.0.0.1',port:0,onListen(){}}, request => {
  const path = new URL(request.url).pathname;
  if (path === '/redirect') return Response.redirect(`http://127.0.0.1:${foreign.addr.port}/`,302);
  if (path === '/redirect-local') return Response.redirect(new URL('/quiet',request.url),302);
  if (new URL(request.url).pathname === '/quiet') return html('<p>Two-window binding test</p>');
  return html(`<p>Protected document</p><script>window.received=[];window.bindings.who('boot').then(value=>window.received.push(value)).catch(error=>window.fixtureError=String(error));</script>`);
});
const origin = `http://127.0.0.1:${trusted.addr.port}`;
const evil = `http://127.0.0.1:${foreign.addr.port}`;
const capabilities = Deno.desktop.getSecurityCapabilities();
assert(capabilities.apiVersion === 1 && capabilities.bindingContext, 'security API contract');
const win = new Deno.BrowserWindow({title:'DNR protected document',stateKey:'security-main',
  documentPolicy:{allowedOrigins:[origin]},hiddenWindowPolicy:{mode:'immediate'}});
let activeContext: Deno.BindingContext | undefined;
let pendingContext: Deno.BindingContext | undefined;
let finishPending: ((value: string) => void) | undefined;
win.bindWithContext('who', (context, label) => {
  assert(context.trusted && context.mainFrame && context.currentDocument, 'native context');
  assert(context.origin === origin, 'native origin');
  activeContext = context; seen.push(String(label)); return context.documentId;
});
win.bindWithContext('wait', context => {
  pendingContext = context;
  return new Promise<string>(resolve => { finishPending = resolve; });
});
let a: Deno.BrowserWindow | undefined, b: Deno.BrowserWindow | undefined;
try {
  win.navigate(`${origin}/`);
  try { await until(() => seen.length === 1, 'initial protected binding failed'); }
  catch (error) {
    console.log((await win.executeJs('JSON.stringify({href:location.href,state:document.readyState,error:window.fixtureError,received:window.received})')).value);
    throw error;
  }
  const first = activeContext!;
  let blocked = false;
  try { win.navigate(`${evil}/`); } catch { blocked = true; }
  assert(blocked && first.currentDocument, 'host foreign navigation must fail before changing the page');
  for (const path of ['/redirect','/redirect-local']) {
    const redirected = new Deno.BrowserWindow({documentPolicy:{allowedOrigins:[origin]}});
    let called = false;
    redirected.bind('who',() => {called=true;});
    try {
      redirected.navigate(origin+path);
      await pause(500);
      assert(!called, 'redirect inherited binding authority');
      const location = String((await redirected.executeJs('location.href')).value);
      assert(!location.startsWith(evil) && !location.endsWith('/quiet'), 'redirect committed a new application document');
    } finally {redirected.destroy();}
  }
  await win.executeJs(`location.href=${JSON.stringify(evil + '/')}`);
  await pause(300);
  assert(String((await win.executeJs('location.origin')).value) === origin, 'page navigation escaped');
  const beforeFrames = seen.length;
  await win.executeJs(`(() => {
    const child=document.createElement('iframe');child.src=${JSON.stringify(evil + '/')};document.body.append(child);
    const raw=document.createElement('iframe');raw.srcdoc='<script>try{window.webkit.messageHandlers.laufey.postMessage({callId:1,method:"who",args:["child"]})}catch{};try{parent.bindings.who("child-via-parent")}catch{}<\/script>';document.body.append(raw);
  })()`);
  await pause(300);
  assert(seen.length === beforeFrames, 'subframe invoked a host binding');
  await win.executeJs("void window.bindings.wait().then(value=>window.received.push(value))");
  await until(() => finishPending !== undefined, 'pending binding did not start');
  win.hide();
  await until(() => pendingContext!.signal.aborted && !pendingContext!.currentDocument, 'release did not revoke context');
  win.show();
  await until(() => seen.length > beforeFrames, 'restored protected binding failed');
  assert(activeContext!.documentId !== first.documentId, 'document generation reused');
  finishPending!('STALE_RESPONSE_SENTINEL');
  await pause(200);
  assert(!String((await win.executeJs('JSON.stringify(window.received)')).value).includes('STALE_RESPONSE_SENTINEL'), 'old response reached new document');

  a = new Deno.BrowserWindow({stateKey:'security-a',documentPolicy:{allowedOrigins:[origin]}});
  b = new Deno.BrowserWindow({stateKey:'security-b',documentPolicy:{allowedOrigins:[origin]}});
  let resolveA: ((value:string)=>void)|undefined, resolveB: ((value:string)=>void)|undefined;
  a.bind('take', () => new Promise<string>(resolve => {resolveA=resolve;}));
  b.bind('take', () => new Promise<string>(resolve => {resolveB=resolve;}));
  a.navigate(`${origin}/quiet`); b.navigate(`${origin}/quiet`);
  await until(async () => {
    const ready = 'document.readyState === "complete" && location.pathname === "/quiet"';
    try { return (await a!.executeJs(ready)).value === true && (await b!.executeJs(ready)).value === true; } catch {return false;}
  }, 'two-window pages did not load');
  await a.executeJs('void window.bindings.take().then(value=>window.answer=value)');
  await b.executeJs('void window.bindings.take().then(value=>window.answer=value)');
  await until(() => !!resolveA && !!resolveB, 'two-window requests did not start');
  resolveA!('window-a'); resolveB!('window-b');
  await until(async () => (await a!.executeJs('window.answer')).value === 'window-a' && (await b!.executeJs('window.answer')).value === 'window-b', 'cross-window response confusion');
  console.log('DNR_DOCUMENT_SECURITY_OK');
} finally {
  win.destroy(); a?.destroy(); b?.destroy();
  await trusted.shutdown(); await foreign.shutdown();
}
