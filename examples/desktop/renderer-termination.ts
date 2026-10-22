// Linux-only native regression: terminate exactly this test window's renderer.
const assert = (condition: unknown, message: string) => {
  if (!condition) throw new Error(message);
};
const pause = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
async function until(check: () => boolean | Promise<boolean>, message: string) {
  for (let attempt = 0; attempt < 150; attempt++) {
    if (await check()) return;
    await pause(20);
  }
  throw new Error(message);
}

if (Deno.build.os !== 'linux') throw new Error('Linux renderer process probe only');
const server = Deno.serve({hostname: '127.0.0.1', port: 0, onListen() {}}, () =>
  new Response('<!doctype html><script>window.bindings.capture().then(value => window.ready = value)</script>',
    {headers: {'Content-Type': 'text/html'}}));
const origin = `http://127.0.0.1:${server.addr.port}`;
const window = new Deno.BrowserWindow({documentPolicy: {allowedOrigins: [origin]}});
let saved: Deno.BindingContext | undefined;
window.bindWithContext('capture', context => {
  saved = context;
  return context.documentId;
});

function children(parent: number): number[] {
  try {
    const found = new Set<number>();
    // A child belongs to the thread that called clone(), which need not be
    // the thread-group leader. Inspect every task before walking descendants.
    for (const task of Deno.readDirSync(`/proc/${parent}/task`)) {
      if (!/^\d+$/.test(task.name)) continue;
      try {
        const text = Deno.readTextFileSync(`/proc/${parent}/task/${task.name}/children`).trim();
        for (const value of text.split(/\s+/)) {
          if (value) found.add(Number(value));
        }
      } catch (error) {
        if (!(error instanceof Deno.errors.NotFound)) throw error;
      }
    }
    return [...found].filter(Number.isSafeInteger);
  } catch (error) {
    if (error instanceof Deno.errors.NotFound) return [];
    throw error;
  }
}
function descendants(root: number): number[] {
  const found: number[] = [], queue = [root];
  while (queue.length) {
    const parent = queue.shift()!;
    for (const child of children(parent)) {
      assert(found.length < 64, 'unexpectedly large renderer process tree');
      found.push(child); queue.push(child);
    }
  }
  return found;
}
function startTime(pid: number): string {
  const stat = Deno.readTextFileSync(`/proc/${pid}/stat`);
  return stat.slice(stat.lastIndexOf(')') + 2).trim().split(/\s+/)[19];
}
function renderer(pid: number, runtime: string): boolean {
  try {
    const executable = Deno.realPathSync(`/proc/${pid}/exe`);
    if (executable.endsWith('/WebKitWebProcess')) return true;
    if (executable !== runtime) return false;
    const args = new TextDecoder().decode(Deno.readFileSync(`/proc/${pid}/cmdline`)).split('\0');
    return args.includes('--type=renderer');
  } catch (error) {
    if (error instanceof Deno.errors.NotFound) return false;
    throw error;
  }
}
try {
  window.navigate(`${origin}/`);
  await until(async () => {
    if (!saved) return false;
    try { return (await window.executeJs('window.ready')).value === saved.documentId; }
    catch { return false; }
  }, 'protected renderer did not finish its initial binding');
  const context = saved!;
  assert(context.currentDocument && !context.signal.aborted, 'document already expired');
  await pause(150); // The completed binding is no longer in flight.
  const runtime = Deno.realPathSync(Deno.execPath());
  const matches = descendants(Deno.pid).filter(pid => renderer(pid, runtime));
  assert(matches.length === 1, `expected exactly one owned renderer, found ${matches.length}`);
  const pid = matches[0], birth = startTime(pid);
  assert(descendants(Deno.pid).includes(pid) && renderer(pid, runtime) && startTime(pid) === birth,
    'renderer changed before termination');
  Deno.kill(pid, 'SIGKILL');
  await until(() => !context.currentDocument && context.signal.aborted,
    'saved context did not abort after renderer termination');
  console.log('DNR_RENDERER_TERMINATION_OK');
} finally {
  window.destroy();
  await server.shutdown();
}
