// Automated native WebView + page/Deno binding smoke test. Opens a window briefly.
const timeout = setTimeout(() => { console.error("DNR_GUI_TIMEOUT"); Deno.exit(1); }, 20000);
const server = Deno.serve({ hostname: "127.0.0.1", port: 0 }, () => new Response(
  `<!doctype html><title>dnr smoke</title><h1>dnr native smoke test</h1>
  <button id="go" onclick="bindings.greet('browser').then(value => bindings.report(value))">Test binding</button>`,
  { headers: { "content-type": "text/html" } },
));
const win = new Deno.BrowserWindow({ title: "dnr verification", width: 640, height: 400 });
win.bind("greet", (name: string) => `hello ${name}`);
win.bind("report", async (value: string) => {
  if (value !== "hello browser") throw new Error(`Bad binding response: ${value}`);
  clearTimeout(timeout);
  win.close();
  // The native shell must not kill the process while JS still has work.
  await new Promise((resolve) => setTimeout(resolve, 50));
  await server.shutdown();
  console.log("DNR_GUI_OK");
});
win.addEventListener("load", async () => {
  const title = await win.executeJs("document.title");
  if (!title.ok || title.value !== "dnr smoke") throw new Error(`Unexpected page title: ${JSON.stringify(title)}`);
  await win.executeJs("document.getElementById('go').click(); 'clicked'");
});
win.navigate(`http://127.0.0.1:${server.addr.port}`);
