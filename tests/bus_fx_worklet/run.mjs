import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const wasmPath = resolve(process.argv[2] || join(root, 'target/wasm32-unknown-unknown/debug/shoop_rt_host_worklet.wasm'));
const assets = new Map([
  ['/index.html', ['text/html', await readFile(join(here, 'index.html'))]],
  ['/scenario.js', ['text/javascript', await readFile(join(here, 'scenario.js'))]],
  ['/engine.wasm', ['application/wasm', await readFile(wasmPath)]],
]);
let worklet = await readFile(join(root, 'src/rust/shoopdaloop/rt_audio_worklet.js'), 'utf8');
const growthGuard = `if (this.wasm.memory.buffer.byteLength !== this.acceptanceBytes) {
  this.port.postMessage({kind: 'memory-growth'}); throw new Error('render memory grew');
}`;
for (const [before, after] of [
  ['this.views();', 'this.views(); this.acceptanceBytes = this.wasm.memory.buffer.byteLength;'],
  ['const input = inputs[0] || [];', `${growthGuard} const input = inputs[0] || [];`],
  ["this.post(RECORDS_OUT, 'records');", `${growthGuard} this.post(RECORDS_OUT, 'records');`],
]) {
  if (!worklet.includes(before)) throw new Error(`missing worklet instrumentation point: ${before}`);
  worklet = worklet.replace(before, after);
}
assets.set('/worklet.js', ['text/javascript', worklet]);
const server = createServer((request, response) => {
  const asset = assets.get(request.url);
  response.writeHead(asset ? 200 : 404, { 'Content-Type': asset?.[0] || 'text/plain' });
  response.end(asset?.[1] || 'not found');
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const profile = await mkdtemp(join(tmpdir(), 'busfx-chromium-'));
const browser = spawn(process.env.CHROME_BIN || 'chromium', [
  '--headless=new', '--no-sandbox', '--disable-dev-shm-usage', '--disable-gpu',
  '--remote-debugging-port=0', '--autoplay-policy=no-user-gesture-required', `--user-data-dir=${profile}`, 'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });
let errors = '';
browser.stderr.on('data', data => { errors = (errors + data).slice(-16000); });
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
let socket;
try {
  let port;
  for (let n = 0; !port && n < 200; n++) {
    try { port = Number((await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0]); } catch {}
    if (browser.exitCode !== null) throw new Error(`Chromium exited: ${errors}`);
    if (!port) await delay(50);
  }
  if (!port) throw new Error(`Chromium did not start: ${errors}`);
  const target = await (await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent(`http://127.0.0.1:${server.address().port}/index.html`)}`, { method: 'PUT' })).json();
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let next = 0;
  const pending = new Map();
  socket.onmessage = ({ data }) => {
    const message = JSON.parse(data);
    const waiter = pending.get(message.id);
    if (waiter) { pending.delete(message.id); clearTimeout(waiter.timer); message.error ? waiter.reject(message.error) : waiter.resolve(message.result); }
  };
  const call = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++next;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 180000);
    pending.set(id, { resolve, reject, timer });
    socket.send(JSON.stringify({ id, method, params }));
  });
  await call('Runtime.enable');
  let ready = false;
  for (let n = 0; !ready && n < 100; n++) {
    const state = await call('Runtime.evaluate', { expression: '!!window.busFxDone', returnByValue: true });
    ready = state.result?.value === true;
    if (!ready) await delay(50);
  }
  if (!ready) throw new Error('acceptance page did not initialize');
  const result = await call('Runtime.evaluate', { expression: 'window.busFxDone', awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails || !result.result?.value?.passed) throw new Error(JSON.stringify(result));
  console.log(JSON.stringify(result.result.value, null, 2));
} finally {
  socket?.close();
  browser.kill('SIGTERM');
  await new Promise(resolve => browser.exitCode !== null ? resolve() : browser.once('exit', resolve));
  server.close();
  await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}
