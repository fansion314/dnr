const server = Deno.serve({ hostname: "127.0.0.1", port: 0 }, () =>
  new Response(`<!doctype html><meta charset="utf-8"><title>dnr</title>
  <style>body{font:20px system-ui;padding:64px;background:#f4f0e8;color:#292820}button{padding:12px}</style>
  <h1>dnr · shared runtime</h1><p>This application contains no runtime binary.</p>
  <button onclick="window.bindings.greet('desktop').then(v => document.querySelector('p').textContent=v)">Call Deno binding</button>`,
  { headers: { "content-type": "text/html; charset=utf-8" } }),
);
const window = new Deno.BrowserWindow({ title: "dnr", width: 900, height: 650 });
window.bind("greet", (name: string) => `Hello, ${name}!`);
window.addEventListener("close", () => { server.shutdown(); });
window.navigate(`http://127.0.0.1:${server.addr.port}`);
