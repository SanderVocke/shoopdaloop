import { spawn } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

if (typeof WebSocket === 'undefined') {
  throw new Error('WebSocket is unavailable; run Node with --experimental-websocket');
}

const host = '127.0.0.1';
const webPort = 8766;
const debugPort = 9223;
const chrome = process.env.CHROME_BIN || 'google-chrome';
const browserSize = process.env.BROWSER_SIZE || '900,600';
const selfContained = process.env.SELF_CONTAINED === '1';
const profile = await mkdtemp(join(tmpdir(), 'shoop-connection-preview-'));
const children = [];
let websocket;

const delay = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds));
function start(command, args, options = {}) {
  const child = spawn(command, args, { stdio: 'pipe', ...options });
  children.push(child);
  return child;
}
async function waitForJson(url, timeout = 15_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return response.json();
    } catch (_) {
      // Processes are still starting.
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${url}`);
}

try {
  if (!selfContained) {
    start('python3', ['-m', 'http.server', String(webPort), '--bind', host], { cwd: 'dist' });
  }
  const chromeArgs = [
    '--headless=new', '--no-sandbox', '--disable-gpu-sandbox', '--enable-webgl',
    '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', `--window-size=${browserSize}`,
    `--remote-debugging-port=${debugPort}`, `--user-data-dir=${profile}`, 'about:blank',
  ];
  if (selfContained) chromeArgs.splice(-1, 0, '--allow-file-access-from-files');
  start(chrome, chromeArgs);
  const targets = await waitForJson(`http://${host}:${debugPort}/json`);
  const target = targets.find(candidate => candidate.type === 'page');
  if (!target) throw new Error('Chrome exposed no page target');
  websocket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    websocket.onopen = resolve;
    websocket.onerror = reject;
  });
  let nextId = 1;
  const pending = new Map();
  const failures = [];
  websocket.onmessage = event => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
    } else if (message.method === 'Runtime.exceptionThrown') {
      failures.push(message);
    } else if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
      failures.push(message);
    }
  };
  function call(method, params = {}) {
    const id = nextId++;
    websocket.send(JSON.stringify({ id, method, params }));
    return new Promise(resolve => pending.set(id, resolve));
  }
  async function evaluate(expression) {
    const response = await call('Runtime.evaluate', { expression, returnByValue: true });
    return response.result?.result?.value;
  }
  async function verifyScope(scope) {
    const base = selfContained
      ? pathToFileURL(join(process.cwd(), 'dist', 'preview.html')).href
      : `http://${host}:${webPort}/`;
    await call('Page.navigate', { url: `${base}?scope=${scope}` });
    const deadline = Date.now() + 30_000;
    let state;
    while (Date.now() < deadline) {
      state = await evaluate(`({
        scope: document.body?.getAttribute('data-connection-scope'),
        ports: Number(document.body?.getAttribute('data-connection-port-count')),
        revision: Number(document.body?.getAttribute('data-connection-revision')),
        canvasWidth: document.getElementById('shoop_canvas')?.width,
        canvasHeight: document.getElementById('shoop_canvas')?.height,
      })`);
      if (state?.scope === scope && state.ports >= 9 && state.revision > 0
          && state.canvasWidth === viewportWidth && state.canvasHeight === viewportHeight) {
        return state;
      }
      await delay(100);
    }
    throw new Error(`connection ${scope} scope did not render: ${JSON.stringify(state)}; failures=${JSON.stringify(failures)}`);
  }
  await call('Runtime.enable');
  await call('Page.enable');
  const [viewportWidth, viewportHeight] = browserSize.split(',').map(Number);
  await call('Emulation.setDeviceMetricsOverride', {
    width: viewportWidth,
    height: viewportHeight,
    deviceScaleFactor: 1,
    mobile: false,
  });
  const all = await verifyScope('all');
  const track = await verifyScope('track');
  if (failures.length) throw new Error(`browser exceptions: ${JSON.stringify(failures)}`);
  console.log(`connection preview passed at ${browserSize}: all=${JSON.stringify(all)}, track=${JSON.stringify(track)}`);
} finally {
  if (websocket) websocket.close();
  for (const child of children.reverse()) child.kill('SIGTERM');
  await delay(500);
  await rm(profile, { recursive: true, force: true }).catch(() => {});
}
